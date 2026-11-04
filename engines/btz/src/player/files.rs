// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Filesystem and filename prompts for the development terminal host.

use crate::{
    media::TextStores,
    services::{HostReply, HostRequest, SideMap},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// Resolve the final filename using DOS ASCII case rules. Directory components
/// retain their host filesystem meaning; save and transcript paths do too.
pub fn resolve_original(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("original game path has no filename"))?;
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut matched = None;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry
            .file_name()
            .as_encoded_bytes()
            .eq_ignore_ascii_case(name.as_encoded_bytes())
            && fs::metadata(entry.path())?.is_file()
        {
            if matched.is_some() {
                return Err(io::Error::other(format!(
                    "ambiguous original game filename {} in {}: multiple case-insensitive matches",
                    name.to_string_lossy(),
                    directory.display()
                )));
            }
            matched = Some(entry.path());
        }
    }
    matched.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("missing original game file {}", path.display()),
        )
    })
}

pub struct Files {
    pub save_directory: PathBuf,
    pub bookmark_path: Option<PathBuf>,
    pub transcript_path: Option<PathBuf>,
    pub side_map_path: Option<PathBuf>,
    pub originals: Vec<PathBuf>,
    pub confirm_bookmark_replace: bool,
    pending_bookmark: Option<PathBuf>,
    transcript: Option<File>,
    last_error: Option<String>,
    pending_transcript_path: Option<PathBuf>,
}
impl Files {
    pub fn new(game_data: &Path) -> io::Result<Self> {
        let game_data = std::path::absolute(game_data)?;
        let save_directory = game_data
            .parent()
            .ok_or_else(|| io::Error::other("game data path has no parent directory"))?
            .to_path_buf();
        Ok(Self {
            save_directory,
            bookmark_path: None,
            transcript_path: None,
            side_map_path: None,
            originals: Vec::new(),
            confirm_bookmark_replace: false,
            pending_bookmark: None,
            transcript: None,
            last_error: None,
            pending_transcript_path: None,
        })
    }
    pub fn needs_input(&self, request: &HostRequest) -> bool {
        match request {
            HostRequest::SaveBookmark { .. } => {
                self.pending_bookmark.is_some()
                    || self.bookmark_path.is_none()
                    || (self.confirm_bookmark_replace
                        && self.bookmark_path.as_ref().is_some_and(|p| p.exists()))
            }
            HostRequest::LoadBookmark => self.bookmark_path.is_none(),
            HostRequest::TranscriptName { .. } => self.transcript_path.is_none(),
            HostRequest::MountDisk { .. } => true,
            _ => false,
        }
    }
    fn protect_originals(&self, path: &Path) -> io::Result<()> {
        if let Ok(target) = path.canonicalize()
            && self
                .originals
                .iter()
                .any(|p| p.canonicalize().is_ok_and(|p| p == target))
        {
            return Err(io::Error::other(
                "the selected path is an original game input",
            ));
        }
        Ok(())
    }
    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    pub fn replacement_pending(&self) -> bool {
        self.pending_bookmark.is_some()
    }

    pub fn prompt(&self, request: &HostRequest) -> Option<String> {
        if let Some(path) = &self.pending_bookmark {
            return Some(format!(
                "Replace existing bookmark {}? [y/n]: ",
                path.display()
            ));
        }
        if !self.needs_input(request) {
            return None;
        }
        if matches!(request, HostRequest::SaveBookmark { .. }) && self.bookmark_path.is_some() {
            // The fixed destination is already chosen. Handling it first may
            // produce a replacement question, never another filename prompt.
            return None;
        }
        Some(match request {
            HostRequest::SaveBookmark { .. } => "Bookmark file to write (blank cancels): ".into(),
            HostRequest::LoadBookmark => "Bookmark file to read (blank cancels): ".into(),
            HostRequest::TranscriptName { .. } => "Transcript file (blank cancels): ".into(),
            HostRequest::MountDisk { physical_disk, .. } => {
                format!("Folder containing program disk {physical_disk} (blank cancels): ")
            }
            _ => return None,
        })
    }

    pub fn handle_with_path(
        &mut self,
        request: &HostRequest,
        path: Option<PathBuf>,
    ) -> Result<Option<HostReply>, Box<dyn std::error::Error>> {
        let mut chosen = path;
        self.handle_with_prompt(request, true, &mut |_| Ok(chosen.take()), false)
    }

    /// The text browser resolves an absolute destination and owns its replacement
    /// question. The ordinary terminal/fixed-file path keeps its native policy.
    #[cfg(feature = "sdl")]
    pub(crate) fn handle_browser_path(
        &mut self,
        request: &HostRequest,
        path: PathBuf,
        replace: bool,
    ) -> Result<Option<HostReply>, Box<dyn std::error::Error>> {
        if matches!(
            request,
            HostRequest::SaveBookmark { .. } | HostRequest::TranscriptName { .. }
        ) && path.exists()
            && !replace
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "The destination now exists. Select it again to confirm replacement.",
            )
            .into());
        }
        let mut chosen = Some(path);
        self.handle_with_prompt(request, true, &mut |_| Ok(chosen.take()), true)
    }

    pub fn handle(
        &mut self,
        request: &HostRequest,
        interactive: bool,
    ) -> Result<Option<HostReply>, Box<dyn std::error::Error>> {
        let mut ask = |prompt: &str| {
            print!("{prompt}");
            io::stdout().flush()?;
            let line = super::terminal::read_prompt_line()?;
            let path = line.trim_end_matches(['\r', '\n']);
            Ok((!path.is_empty()).then(|| PathBuf::from(path)))
        };
        loop {
            let reply = self.handle_with_prompt(request, interactive, &mut ask, false)?;
            if reply.is_some() || !interactive || self.pending_bookmark.is_none() {
                return Ok(reply);
            }
        }
    }

    fn handle_with_prompt(
        &mut self,
        request: &HostRequest,
        interactive: bool,
        ask: &mut dyn FnMut(&str) -> io::Result<Option<PathBuf>>,
        replacement_confirmed: bool,
    ) -> Result<Option<HostReply>, Box<dyn std::error::Error>> {
        self.last_error = None;
        Ok(Some(match request {
            HostRequest::Exit { .. } => HostReply::Done,
            HostRequest::SaveBookmark { data } => {
                let replacing = self.pending_bookmark.is_some();
                let path = if let Some(path) = self.pending_bookmark.take() {
                    if !interactive {
                        self.pending_bookmark = Some(path);
                        return Ok(None);
                    }
                    let answer = ask(&format!(
                        "Replace existing bookmark {}? [y/n]: ",
                        path.display()
                    ))?;
                    if !answer.is_some_and(|answer| {
                        answer
                            .as_os_str()
                            .as_encoded_bytes()
                            .eq_ignore_ascii_case(b"y")
                    }) {
                        return Ok(Some(HostReply::Cancelled));
                    }
                    Some(path)
                } else {
                    self.save_path(
                        self.bookmark_path.as_ref(),
                        "Bookmark file to write (blank cancels): ",
                        interactive,
                        ask,
                    )?
                };
                let Some(path) = path else {
                    return Ok(if interactive {
                        Some(HostReply::Cancelled)
                    } else {
                        None
                    });
                };
                // A second call carries the explicit replacement decision. Do
                // not consume the filename itself as an answer or truncate yet.
                if self.confirm_bookmark_replace
                    && path.exists()
                    && !replacing
                    && !replacement_confirmed
                {
                    self.pending_bookmark = Some(path);
                    return Ok(None);
                }
                if let Err(error) = self
                    .protect_originals(&path)
                    .and_then(|()| write_bookmark(&path, data))
                {
                    let message = format!("Cannot write bookmark: {error}");
                    eprintln!("{message}");
                    self.last_error = Some(message);
                    HostReply::Cancelled
                } else {
                    HostReply::Done
                }
            }
            HostRequest::LoadBookmark => {
                let Some(path) = self.save_path(
                    self.bookmark_path.as_ref(),
                    "Bookmark file to read (blank cancels): ",
                    interactive,
                    ask,
                )?
                else {
                    return Ok(if interactive {
                        Some(HostReply::Cancelled)
                    } else {
                        None
                    });
                };
                match fs::read(path) {
                    Ok(data) => HostReply::Bytes(data),
                    Err(error) => {
                        let message = format!("Cannot read bookmark: {error}");
                        eprintln!("{message}");
                        self.last_error = Some(message);
                        HostReply::Cancelled
                    }
                }
            }
            HostRequest::TranscriptName { max_bytes, .. } => {
                if self.transcript_path.is_some() {
                    HostReply::Name(b"transcript.txt".to_vec())
                } else {
                    let Some(path) = self.save_path(
                        None,
                        "Transcript file (blank cancels): ",
                        interactive,
                        ask,
                    )?
                    else {
                        return Ok(if interactive {
                            Some(HostReply::Cancelled)
                        } else {
                            None
                        });
                    };
                    let bytes = path.to_string_lossy().as_bytes().to_vec();
                    if bytes.len() > *max_bytes {
                        // The original device name has a short DOS buffer; a
                        // modern filesystem path is resolved by this host.
                        self.pending_transcript_path = Some(path);
                        HostReply::Name(b"transcript.txt".to_vec())
                    } else {
                        HostReply::Name(bytes)
                    }
                }
            }
            HostRequest::OpenTranscript { path } => {
                let path = self
                    .pending_transcript_path
                    .take()
                    .or_else(|| self.transcript_path.clone())
                    .unwrap_or_else(|| PathBuf::from(String::from_utf8_lossy(path).into_owned()));
                match self
                    .protect_originals(&path)
                    .and_then(|()| create_transcript(&path))
                {
                    Ok(file) => {
                        self.transcript = Some(file);
                        HostReply::Done
                    }
                    Err(error) => {
                        self.transcript = None;
                        let message = format!("Cannot open transcript: {error}");
                        eprintln!("{message}");
                        self.last_error = Some(message);
                        HostReply::Cancelled
                    }
                }
            }
            HostRequest::WriteTranscript { bytes } => {
                self.transcript
                    .as_mut()
                    .ok_or("transcript writer is not open")?
                    .write_all(bytes)?;
                HostReply::Done
            }
            HostRequest::CloseTranscript => {
                if let Some(file) = self.transcript.as_mut() {
                    file.flush()?;
                }
                self.transcript = None;
                HostReply::Done
            }
            HostRequest::ReadSideMap => {
                let Some(path) = &self.side_map_path else {
                    return Ok(None);
                };
                HostReply::SideMap(SideMap::from_bytes(&fs::read(path)?)?)
            }
            HostRequest::MountDisk { physical_disk, .. } => {
                let Some(directory) = self.path(
                    None,
                    &format!("Folder containing program disk {physical_disk} (blank cancels): "),
                    interactive,
                    ask,
                )?
                else {
                    return Ok(None);
                };
                let data_path = resolve_original(&directory.join("ADISK.OBJ"))?;
                let map_path = resolve_original(&directory.join("SIDE.TXT"))?;
                let bytes = fs::read(&data_path)?;
                let stores = if *physical_disk == 1 {
                    TextStores::from_primary_dos_obj(bytes)?
                } else {
                    TextStores::from_secondary_dos_obj(bytes)?
                };
                let side_map = SideMap::from_bytes(&fs::read(&map_path)?)?;
                self.originals.extend([data_path, map_path]);
                HostReply::Mounted { stores, side_map }
            }
        }))
    }
    fn save_path(
        &self,
        configured: Option<&PathBuf>,
        prompt: &str,
        interactive: bool,
        ask: &mut dyn FnMut(&str) -> io::Result<Option<PathBuf>>,
    ) -> io::Result<Option<PathBuf>> {
        let path = self.path(configured, prompt, interactive, ask)?;
        // Explicit CLI file paths keep their command-line meaning (relative to
        // the launch directory). Prompted relative names use the game data
        // directory, or the host's chosen override; absolute paths override it.
        Ok(path.map(|path| {
            if configured.is_some() {
                path
            } else {
                self.save_directory.join(path)
            }
        }))
    }

    fn path(
        &self,
        configured: Option<&PathBuf>,
        prompt: &str,
        interactive: bool,
        ask: &mut dyn FnMut(&str) -> io::Result<Option<PathBuf>>,
    ) -> io::Result<Option<PathBuf>> {
        if let Some(path) = configured {
            return Ok(Some(path.clone()));
        }
        if !interactive {
            return Ok(None);
        }
        ask(prompt)
    }
}

fn write_bookmark(path: &Path, data: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("bookmark path has no filename"))?
        .to_string_lossy();
    for counter in 0..100 {
        let temporary = parent.join(format!(".{name}.btz-{}-{counter}.tmp", std::process::id()));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            file.write_all(data)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    Err(io::Error::other(
        "cannot allocate a temporary bookmark file",
    ))
}

// A new transcript replaces its destination inode. In particular, a hardlink
// alias must not truncate another name's original game input before protection
// can distinguish it. Existing canonical original paths are rejected above.
fn create_transcript(path: &Path) -> io::Result<File> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("transcript path has no filename"))?
        .to_string_lossy();
    for counter in 0..100 {
        let temporary = parent.join(format!(
            ".{name}.btz-transcript-{}-{counter}.tmp",
            std::process::id()
        ));
        let file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        if let Err(error) = fs::rename(&temporary, path) {
            drop(file);
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        return Ok(file);
    }
    Err(io::Error::other(
        "cannot allocate a temporary transcript file",
    ))
}
