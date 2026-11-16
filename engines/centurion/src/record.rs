// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in native-first playthrough evidence; never changes simulation decisions.
use crate::assets::{Assets, Result};
use crate::session::{Input, Session};
use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub enum Trace {
    Rng(u16, u16, u16, u16), // bound, first/second before, result
    Sound(Option<u8>),
}
thread_local! {
    static TRACE: RefCell<Option<Vec<Trace>>> = const { RefCell::new(None) };
}
pub fn trace_rng(bound: u16, first: u16, second: u16, result: u16) {
    TRACE.with(|trace| {
        if let Some(events) = trace.borrow_mut().as_mut() {
            events.push(Trace::Rng(bound, first, second, result));
        }
    });
}
pub fn trace_sound(id: Option<u8>) {
    TRACE.with(|trace| {
        if let Some(events) = trace.borrow_mut().as_mut() {
            events.push(Trace::Sound(id));
        }
    });
}
pub fn begin_trace() {
    TRACE.with(|t| *t.borrow_mut() = Some(Vec::new()));
}
pub fn end_trace() -> Vec<Trace> {
    TRACE.with(|t| t.borrow_mut().take().unwrap_or_default())
}

pub fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
fn object(fields: &[(&str, String)]) -> String {
    format!(
        "{{{}}}\n",
        fields
            .iter()
            .map(|(k, v)| format!("{}:{v}", quote(k)))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .and_then(|mut f| f.write_all(bytes))
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn log(path: &Path, header: &str) -> Result<BufWriter<File>> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(header.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(writer)
}
// Flush intent before game code runs, including timer inputs that may trigger a
// simulation crash. Successful post-state remains in events.tsv/checkpoints.
fn persist_input(writer: &mut BufWriter<File>, input: &Input) -> Result<()> {
    writeln!(writer, "{}", input.command())
        .and_then(|()| writer.flush())
        .map_err(|e| e.to_string())
}
fn hashes(paths: &[PathBuf]) -> Result<Vec<String>> {
    let mut all = Vec::new();
    for group in paths.chunks(128) {
        let result = Command::new("b3sum")
            .args(["--no-names", "--"])
            .args(group)
            .output()
            .map_err(|e| format!("Recording needs installed b3sum: {e}"))?;
        if !result.status.success() {
            return Err(format!(
                "Recording BLAKE3: {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        let text = String::from_utf8(result.stdout).map_err(|e| e.to_string())?;
        let values: Vec<_> = text.lines().map(str::to_owned).collect();
        if values.len() != group.len()
            || values
                .iter()
                .any(|v| v.len() != 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid recording BLAKE3 output".into());
        }
        all.extend(values);
    }
    Ok(all)
}
fn files(root: &Path, at: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(at).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            files(root, &entry.path(), result)?;
        } else if kind.is_file() {
            result.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf(),
            );
        } else {
            return Err("Unexpected non-regular recording entry".into());
        }
    }
    Ok(())
}

/// CPR1: u32 little-endian raw size, then literals (tag+1 bytes) or runs
/// (tag high bit set: (tag&127)+3 repeats followed by their single byte).
pub fn pack(bytes: &[u8]) -> Vec<u8> {
    let mut out = b"CPR1".to_vec();
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    let mut at = 0;
    while at < bytes.len() {
        let run = bytes[at..]
            .iter()
            .take(130)
            .take_while(|b| **b == bytes[at])
            .count();
        if run >= 3 {
            out.extend_from_slice(&[0x80 | ((run - 3) as u8), bytes[at]]);
            at += run;
            continue;
        }
        let start = at;
        at += run;
        while at < bytes.len() && at - start < 128 {
            if at + 2 < bytes.len() && bytes[at] == bytes[at + 1] && bytes[at] == bytes[at + 2] {
                break;
            }
            at += 1;
        }
        out.push((at - start - 1) as u8);
        out.extend_from_slice(&bytes[start..at]);
    }
    out
}

pub struct Recorder {
    root: PathBuf,
    inputs: BufWriter<File>,
    events: BufWriter<File>,
    rng: BufWriter<File>,
    sound: BufWriter<File>,
    checkpoints: BufWriter<File>,
    seq: u64,
    checkpoint_count: u64,
    draws: u64,
    last_tick: u32,
    scene: String,
    saved: Vec<u8>,
}
impl Recorder {
    pub fn start(root: &Path, assets: &Assets, session: &Session, mute: bool) -> Result<Self> {
        if let Some(parent) = root.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::create_dir(root).map_err(|e| {
            format!(
                "Recording needs a new directory (will not overwrite): {}: {e}",
                root.display()
            )
        })?;
        fs::create_dir(root.join("checkpoints")).map_err(|e| e.to_string())?;
        fs::create_dir(root.join("initial-slots")).map_err(|e| e.to_string())?;
        let mut errors = String::new();
        for (slot, value) in session.record_slots(assets).into_iter().enumerate() {
            match value {
                Ok(bytes) => write_new(&root.join(format!("initial-slots/{slot}.CSV")), &bytes)?,
                Err(error) => {
                    errors.push_str(&format!("{slot}\t{}\n", Input::Text(error).command()))
                }
            }
        }
        write_new(&root.join("slot-errors.txt"), errors.as_bytes())?;
        write_new(&root.join("initial.CSV"), &session.campaign.save())?;
        write_new(&root.join("initial.ds.rle"), &pack(&session.campaign.data))?;
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        fs::copy(&executable, root.join("engine")).map_err(|e| e.to_string())?;
        let mut names = vec!["CENTURIO.EXE".to_string(), "OVL0.OVL".to_string()];
        for bank in [
            "CENT", "MINIT", "MMAP", "BATL", "MGLAD", "MRACE", "MNAVAL", "NAVL", "MNEGO",
        ] {
            for extension in ["DAT", "DIR"] {
                names.push(format!("{bank}.{extension}"));
            }
        }
        let mut paths: Vec<_> = names.iter().map(|n| assets.original().join(n)).collect();
        paths.push(root.join("engine"));
        let digests = hashes(&paths)?;
        let asset_records = names
            .iter()
            .zip(&digests)
            .map(|(name, digest)| object(&[("path", quote(name)), ("blake3", quote(digest))]))
            .collect::<Vec<_>>()
            .join(",");
        let sound = session
            .campaign
            .sound
            .pending()
            .iter()
            .map(|c| match c {
                crate::sound::Command::Play(n) => n.to_string(),
                crate::sound::Command::Stop => "null".into(),
            })
            .collect::<Vec<_>>()
            .join(",");
        let metadata = object(&[
            ("format", quote("centurion-native-playthrough-v1")),
            ("hash_algorithm", quote("blake3")),
            ("tribute_indicator", session.tribute_indicator.to_string()),
            (
                "fleet_information",
                quote(session.fleet_information.as_str()),
            ),
            ("popup_cursor", quote(session.popup_cursor.as_str())),
            (
                "ui_policy",
                quote(
                    "keyboard-style menus with mouse-hover feedback; applied tribute shading when enabled; selectable enemy fleet information; intentional differences from DOS menus",
                ),
            ),
            (
                "created_unix_ns",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
                    .to_string(),
            ),
            ("phase", quote(&format!("{:?}", session.phase))),
            (
                "seed",
                format!("[{},{}]", session.rng.first, session.rng.second),
            ),
            (
                "skip_battle_fanfare",
                session.skip_battle_fanfare.to_string(),
            ),
            ("mute", mute.to_string()),
            ("engine_blake3", quote(digests.last().unwrap())),
            ("assets", format!("[{asset_records}]")),
            ("initial_sound", format!("[{sound}]")),
            (
                "frame_format",
                quote("320x200 indexed, RGB8 palette, software cursor included"),
            ),
            ("pit_hz", crate::intro::PIT_HZ.to_string()),
            ("pit_divisor", crate::intro::PIT_DIVISOR.to_string()),
            (
                "rng_format",
                quote(
                    "16 bytes per draw: event:u64le,bound:u16le,first:u16le,second:u16le,result:u16le",
                ),
            ),
        ]);
        write_new(&root.join("metadata.json"), metadata.as_bytes())?;
        let mut result = Self {
            root: root.to_path_buf(),
            inputs: log(&root.join("inputs.txt"), "")?,
            events: log(
                &root.join("events.tsv"),
                "seq\ttick_before\ttick_after\tinput\tcontext_before\tcontext_after\n",
            )?,
            rng: log(&root.join("rng.bin"), "")?,
            sound: log(&root.join("sound.tsv"), "seq\tcommand\tid\n")?,
            checkpoints: log(
                &root.join("checkpoints.tsv"),
                "checkpoint\tseq\ttick\treason\tcontext\n",
            )?,
            seq: 0,
            checkpoint_count: 0,
            draws: 0,
            last_tick: session.ticks,
            scene: String::new(),
            saved: Vec::new(),
        };
        result.checkpoint(assets, session, "start")?;
        eprintln!(
            "Recording playthrough: {} (F12 marks a checkpoint)",
            root.display()
        );
        Ok(result)
    }
    pub fn before_input(&mut self, input: &Input) -> Result<()> {
        persist_input(&mut self.inputs, input)
    }
    pub fn event(
        &mut self,
        assets: &Assets,
        session: &Session,
        input: &Input,
        before: &(u32, String),
        traces: Vec<Trace>,
        error: Option<&str>,
    ) -> Result<()> {
        self.seq += 1;
        writeln!(
            self.events,
            "{}\t{}\t{}\t{}\t{}\t{}",
            self.seq,
            before.0,
            session.ticks,
            input.command(),
            before.1,
            session.record_context()
        )
        .map_err(|e| e.to_string())?;
        for trace in traces {
            match trace {
                Trace::Rng(bound, first, second, result) => {
                    self.rng
                        .write_all(&self.seq.to_le_bytes())
                        .map_err(|e| e.to_string())?;
                    for value in [bound, first, second, result] {
                        self.rng
                            .write_all(&value.to_le_bytes())
                            .map_err(|e| e.to_string())?;
                    }
                    self.draws += 1;
                }
                Trace::Sound(id) => {
                    writeln!(
                        self.sound,
                        "{}\t{}\t{}",
                        self.seq,
                        if id.is_some() { "play" } else { "stop" },
                        id.unwrap_or(255)
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        // Preserve the game error even if exporting its failed state also fails.
        if let Some(message) = error {
            self.fail(message);
        }
        let elapsed = session.ticks.wrapping_sub(self.last_tick);
        let reason = if matches!(input, Input::Mark(_)) {
            Some("mark")
        } else if self.scene != session.record_scene() {
            Some("scene")
        } else if self.saved != session.campaign.save() {
            Some("campaign")
        } else if !input.is_motion() && elapsed >= 36 {
            Some("action")
        } else if elapsed >= 720 {
            Some("periodic")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.checkpoint(assets, session, reason)?;
            if let Input::Mark(label) = input {
                eprintln!(
                    "Playthrough mark {} at event {}: {}",
                    self.checkpoint_count - 1,
                    self.seq,
                    label
                );
            }
        }
        // Persist decisions promptly; periodic checkpoints flush idle/timer data.
        if !input.is_motion() {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        for writer in [
            &mut self.inputs,
            &mut self.events,
            &mut self.rng,
            &mut self.sound,
            &mut self.checkpoints,
        ] {
            writer.flush().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn checkpoint(&mut self, assets: &Assets, session: &Session, reason: &str) -> Result<()> {
        let folder = self
            .root
            .join(format!("checkpoints/{:08}", self.checkpoint_count));
        fs::create_dir(&folder).map_err(|e| e.to_string())?;
        let frame = session.render(assets, true)?;
        write_new(&folder.join("frame.idx.rle"), &pack(&frame.pixels))?;
        write_new(
            &folder.join("frame.pal"),
            &frame
                .palette(assets)
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
        )?;
        for (name, bytes) in session.record_files() {
            if bytes.len() > 4096 {
                write_new(&folder.join(name + ".rle"), &pack(&bytes))?;
            } else {
                write_new(&folder.join(name), &bytes)?;
            }
        }
        let meta = object(&[
            ("seq", self.seq.to_string()),
            ("tick", session.ticks.to_string()),
            ("reason", quote(reason)),
            (
                "rng",
                format!("[{},{}]", session.rng.first, session.rng.second),
            ),
            ("rng_draws", self.draws.to_string()),
            ("context", quote(&session.record_context())),
            ("summary", quote(&session.state_summary())),
        ]);
        write_new(&folder.join("state.json"), meta.as_bytes())?;
        writeln!(
            self.checkpoints,
            "{}\t{}\t{}\t{}\t{}",
            self.checkpoint_count,
            self.seq,
            session.ticks,
            reason,
            session.record_context()
        )
        .map_err(|e| e.to_string())?;
        self.checkpoint_count += 1;
        self.last_tick = session.ticks;
        self.scene = session.record_scene();
        self.saved = session.campaign.save();
        self.flush()
    }
    pub fn fail(&mut self, message: &str) {
        let _ = self.flush();
        let _ = write_new(
            &self.root.join("error.json"),
            object(&[("seq", self.seq.to_string()), ("error", quote(message))]).as_bytes(),
        );
    }
    pub fn finish(&mut self, assets: &Assets, session: &Session) -> Result<()> {
        self.checkpoint(assets, session, "end")?;
        let mut names = Vec::new();
        files(&self.root, &self.root, &mut names)?;
        names.sort();
        let paths: Vec<_> = names.iter().map(|p| self.root.join(p)).collect();
        let digests = hashes(&paths)?;
        let records = names
            .iter()
            .zip(&paths)
            .zip(digests)
            .map(|((name, path), digest)| {
                Ok(object(&[
                    ("path", quote(&name.to_string_lossy())),
                    (
                        "bytes",
                        fs::metadata(path)
                            .map_err(|e| e.to_string())?
                            .len()
                            .to_string(),
                    ),
                    ("blake3", quote(&digest)),
                ]))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let manifest = object(&[
            ("format", quote("centurion-native-playthrough-v1")),
            (
                "complete",
                (!self.root.join("error.json").exists()).to_string(),
            ),
            ("events", self.seq.to_string()),
            ("checkpoints", self.checkpoint_count.to_string()),
            ("rng_draws", self.draws.to_string()),
            ("files", format!("[{records}]")),
        ]);
        write_new(&self.root.join("manifest.json"), manifest.as_bytes())?;
        eprintln!(
            "Saved playthrough: {} ({} events, {} checkpoints)",
            self.root.display(),
            self.seq,
            self.checkpoint_count
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn unpack(bytes: &[u8]) -> Vec<u8> {
        assert_eq!(&bytes[..4], b"CPR1");
        let expected = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let mut at = 8;
        let mut out = Vec::new();
        while at < bytes.len() {
            let tag = bytes[at];
            at += 1;
            if tag & 128 != 0 {
                out.extend(std::iter::repeat_n(bytes[at], usize::from(tag & 127) + 3));
                at += 1;
            } else {
                let n = usize::from(tag) + 1;
                out.extend_from_slice(&bytes[at..at + n]);
                at += n;
            }
        }
        assert_eq!(out.len(), expected);
        out
    }
    #[test]
    fn input_is_visible_before_application_or_writer_drop() {
        let name = std::env::temp_dir().join(format!(
            "centurion-input-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut writer = log(&name, "").unwrap();
        persist_input(&mut writer, &Input::Tick(12)).unwrap();
        assert_eq!(fs::read(&name).unwrap(), b"tick 12\n");
        drop(writer);
        fs::remove_file(name).unwrap();
    }
    #[test]
    fn checkpoint_codec_is_lossless_for_runs_literals_and_boundaries() {
        let mut bytes = Vec::new();
        for n in [0, 1, 2, 3, 127, 128, 129, 130, 131, 255, 65536] {
            let constant = vec![37; n];
            assert_eq!(unpack(&pack(&constant)), constant);
            bytes.extend((0..n).map(|v| (v % 251) as u8));
            assert_eq!(unpack(&pack(&bytes)), bytes);
        }
        assert!(pack(&vec![0; 64000]).len() < 1100);
    }
    #[test]
    fn metadata_escapes_control_bytes_without_losing_unicode() {
        assert_eq!(quote("\"\\\n\t\0α"), "\"\\\"\\\\\\n\\t\\u0000α\"");
    }
    #[test]
    fn tracing_is_scoped_and_preserves_draw_parameters() {
        let _ = end_trace();
        trace_rng(1, 2, 3, 4);
        assert!(end_trace().is_empty());
        begin_trace();
        trace_rng(5, 6, 7, 8);
        trace_sound(Some(3));
        let trace = end_trace();
        assert!(matches!(
            trace.as_slice(),
            [Trace::Rng(5, 6, 7, 8), Trace::Sound(Some(3))]
        ));
    }
}
