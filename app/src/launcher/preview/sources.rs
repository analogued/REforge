// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Background engine preview generation; drawing never opens original/cache files.
use super::super::model::GameEntry;
use crate::detection::{self, Detected, Game, Release, verification::Verification};
use reforge_sdl::{AspectRatio, DisplayGeometry, preview::Frame};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

#[derive(Clone, Debug)]
pub(in crate::launcher) struct Source {
    pub id: String,
    pub frame: Arc<Frame>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Identity {
    entry: String,
    game: String,
    directory: PathBuf,
    release: String,
    original: PathBuf,
    preferred: String,
    allow_generation: bool,
    generation: u64,
}
#[derive(Clone, Debug)]
struct Request {
    identity: Identity,
    verification: Option<Arc<Verification>>,
}
struct Reply {
    request: Request,
    frames: BTreeMap<String, Source>,
}
pub(in crate::launcher) struct Manager {
    send: mpsc::Sender<Request>,
    receive: mpsc::Receiver<Reply>,
    pending: HashSet<Identity>,
}
impl Manager {
    pub fn new(cache: Option<PathBuf>) -> Self {
        Self::with_generator(cache, generate)
    }
    fn with_generator(
        cache: Option<PathBuf>,
        mut generate: impl FnMut(&Request, Option<&Path>) -> BTreeMap<String, Source> + Send + 'static,
    ) -> Self {
        let (send, jobs) = mpsc::channel::<Request>();
        let (done, receive) = mpsc::channel();
        // One worker bounds CPU and disk pressure when a library first opens.
        std::thread::spawn(move || {
            while let Ok(request) = jobs.recv() {
                // A preview is optional. A failed job must not kill the worker
                // or leave every later entry permanently pending.
                let frames =
                    catch_unwind(AssertUnwindSafe(|| generate(&request, cache.as_deref())))
                        .unwrap_or_else(|_| {
                            eprintln!("REforge: preview job failed for {}", request.identity.entry);
                            BTreeMap::new()
                        });
                if done.send(Reply { request, frames }).is_err() {
                    break;
                }
            }
        });
        Self {
            send,
            receive,
            pending: HashSet::new(),
        }
    }
    pub fn poll(&mut self, games: &mut [GameEntry]) {
        while let Ok(reply) = self.receive.try_recv() {
            self.pending.remove(&reply.request.identity);
            if let Some(game) = games.iter_mut().find(|g| {
                g.entry_id == reply.request.identity.entry
                    && g.game_id == reply.request.identity.game
                    && g.directory == reply.request.identity.directory
                    && g.release_id == reply.request.identity.release
                    && g.original == reply.request.identity.original
            }) {
                game.previews.extend(reply.frames);
                if game.previews_complete() {
                    game.preview_attempted = true;
                } else if reply.request.identity.generation == game.preview_generation {
                    game.preview_attempted = true;
                    // A cached-only job must not suppress generation after recovery.
                    if !reply.request.identity.allow_generation && !game.status.is_missing() {
                        game.retry_incomplete_previews();
                    }
                }
            }
        }
        for game in games {
            if game.previews_complete() {
                game.preview_attempted = true;
            }
            if game.preview_attempted || game.edition.is_none() {
                continue;
            }
            game.preview_attempted = true;
            let request = Request {
                identity: Identity {
                    entry: game.entry_id.clone(),
                    game: game.game_id.clone(),
                    directory: game.directory.clone(),
                    release: game.release_id.clone(),
                    original: game.original.clone(),
                    preferred: game.preview_variant().into(),
                    allow_generation: !game.status.is_missing(),
                    generation: game.preview_generation,
                },
                verification: game.preview_verification.take(),
            };
            if self.pending.insert(request.identity.clone()) && self.send.send(request).is_err() {
                break;
            }
        }
    }
}
fn revision(game: Game) -> u32 {
    match game {
        Game::AlleyCat => alleycat::launcher::PREVIEW_REVISION,
        Game::Mach3 => mach3::launcher::PREVIEW_REVISION,
        Game::Mindwheel | Game::Essex | Game::Brimstone | Game::Breakers => {
            btz::launcher::PREVIEW_REVISION
        }
    }
}
fn cache_id(release: &Release, variant: &str) -> String {
    let mut hash = Sha256::new();
    // Version includes the PNG geometry/colour contract; engines version their scene recipes.
    hash.update(format!(
        "reforge-preview-v2\0{}\0{}\0{}\0{}\0",
        release.game().id(),
        release.id,
        revision(release.game()),
        variant
    ));
    for file in release.files {
        hash.update(format!("{}\0{}\0{}\0", file.name, file.size, file.sha256));
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn generate(request: &Request, cache: Option<&Path>) -> BTreeMap<String, Source> {
    generate_with(
        request,
        cache,
        detection::detect,
        |detected, variant| match detected.game {
            Game::AlleyCat => alleycat::launcher::preview(&detected.original, variant),
            Game::Mach3 => mach3::launcher::preview(&detected.files, variant),
            Game::Mindwheel | Game::Essex | Game::Brimstone | Game::Breakers => {
                btz::launcher::preview(&detected.files, variant)
            }
        },
    )
}

fn generate_with(
    request: &Request,
    cache: Option<&Path>,
    mut detect: impl FnMut(&Path) -> Result<Detected, String>,
    mut render: impl FnMut(&Detected, &str) -> Result<Frame, String>,
) -> BTreeMap<String, Source> {
    // This lock is used only by preview workers, never by the launched game.
    // A crashed process releases it automatically. Only its next owner may
    // clean abandoned staging files, so another launcher's writer is safe.
    let cache_guard = cache.and_then(|root| match CacheSession::open(root) {
        Ok(session) => Some(session),
        Err(error) => {
            eprintln!("REforge: preview cache unavailable: {error}");
            None
        }
    });
    let cache = cache_guard.as_ref().map(|session| session.root.as_path());
    let mut output = BTreeMap::new();
    let Some(release) = detection::RELEASES
        .iter()
        .find(|r| r.id == request.identity.release)
    else {
        return output;
    };
    let variants = match release.game() {
        Game::AlleyCat => alleycat::launcher::PREVIEW_VARIANTS,
        Game::Mach3 => mach3::launcher::PREVIEW_VARIANTS,
        Game::Mindwheel | Game::Essex | Game::Brimstone | Game::Breakers => {
            btz::launcher::PREVIEW_VARIANTS
        }
    };
    // Resolve originals lazily, once per job, only if a cached variant cannot
    // be decoded. Keep already loaded variants even when generation is impossible.
    let mut originals: Option<Result<Detected, String>> = None;
    let mut ordered = variants.to_vec();
    ordered.sort_by_key(|v| *v != request.identity.preferred);
    for variant in ordered {
        let id = cache_id(release, variant);
        let path = cache.map(|root| root.join(format!("{id}.png")));
        let frame = match cached_frame(path.as_deref(), || {
            let detected = originals.get_or_insert_with(|| {
                if !request.identity.allow_generation {
                    return Err("Original files are unavailable for preview generation".into());
                }
                if let Some(verified) = request.verification.as_ref().and_then(|verification| {
                    verification.current_for(&request.identity.original, &request.identity.release)
                }) {
                    return Ok(verified.clone());
                }
                let detected = detect(&request.identity.original)?;
                if detected.release.id != request.identity.release {
                    return Err("Original files changed release before preview generation".into());
                }
                Ok(detected)
            });
            render(detected.as_ref().map_err(Clone::clone)?, variant)
        }) {
            Ok(frame) => frame,
            Err(error) => {
                // Missing originals are an ordinary cached-only request.
                if request.identity.allow_generation {
                    eprintln!(
                        "REforge: preview unavailable for {} ({variant}): {error}",
                        release.edition.title()
                    );
                }
                continue;
            }
        };
        output.insert(
            variant.into(),
            Source {
                id,
                frame: Arc::new(frame),
            },
        );
    }
    output
}

/// The PNGs themselves are checkpoints: do not regenerate completed variants.
/// Restart only the missing/corrupt image, not an entire game's preview batch.
fn cached_frame(
    path: Option<&Path>,
    generate: impl FnOnce() -> Result<Frame, String>,
) -> Result<Frame, String> {
    if let Some(frame) = path.and_then(|path| load(path).ok()) {
        return Ok(frame);
    }
    let frame = catch_unwind(AssertUnwindSafe(generate))
        .map_err(|_| "Preview generator panicked".to_owned())??;
    if let Some(path) = path
        && let Err(error) = store(path, &frame)
    {
        eprintln!("REforge: cannot cache preview: {error}");
    }
    Ok(frame)
}

struct CacheSession {
    root: PathBuf,
    _lock: File,
}
impl CacheSession {
    fn open(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(".preview-cache.lock"))
            .map_err(|e| e.to_string())?;
        fs2::FileExt::lock_exclusive(&lock).map_err(|e| e.to_string())?;
        let session = Self {
            root: root.into(),
            _lock: lock,
        };
        for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if owned_temporary(&entry.file_name().to_string_lossy())
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(session)
    }
}
fn owned_temporary(name: &str) -> bool {
    let Some(body) = name
        .strip_prefix(".reforge-preview-")
        .and_then(|s| s.strip_suffix(".tmp"))
    else {
        return false;
    };
    let mut fields = body.rsplitn(3, '-');
    let Some(sequence) = fields.next() else {
        return false;
    };
    let Some(pid) = fields.next() else {
        return false;
    };
    let Some(key) = fields.next() else {
        return false;
    };
    !key.is_empty()
        && !sequence.is_empty()
        && sequence.bytes().all(|c| c.is_ascii_digit())
        && !pid.is_empty()
        && pid.bytes().all(|c| c.is_ascii_digit())
}
struct PendingWrite(PathBuf);
impl Drop for PendingWrite {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(in crate::launcher) fn default_cache_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("REforge/Cache/previews"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Caches/REforge/previews"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_CACHE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))
            .map(|p| p.join("reforge/previews"))
    }
}
fn load(path: &Path) -> Result<Frame, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("Preview cache too large".into());
    }
    let decoder = png::Decoder::new_with_limits(
        BufReader::new(file),
        png::Limits {
            bytes: 64 * 1024 * 1024,
        },
    );
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || u64::from(info.width) * u64::from(info.height) > 16 * 1024 * 1024
        || info.color_type != png::ColorType::Rgba
        || info.bit_depth != png::BitDepth::Eight
    {
        return Err("Invalid native preview cache".into());
    }
    let geometry = info
        .uncompressed_latin1_text
        .iter()
        .find(|t| t.keyword == "reforge.display.v2")
        .ok_or("Missing preview geometry")?;
    let dimensions = geometry
        .text
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if dimensions.len() != 3 || dimensions[..2].contains(&0) || dimensions[2] > 1 {
        return Err("Invalid preview geometry".into());
    }
    let geometry = DisplayGeometry::new((info.width, info.height), (dimensions[0], dimensions[1]));
    let mut rgba = vec![0; reader.output_buffer_size()];
    let result = reader.next_frame(&mut rgba).map_err(|e| e.to_string())?;
    rgba.truncate(result.buffer_size());
    reader.finish().map_err(|e| e.to_string())?;
    if dimensions[2] == 1 {
        Frame::new_monochrome(geometry, rgba)
    } else {
        Frame::new(geometry, rgba)
    }
}
fn store(path: &Path, frame: &Frame) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(path.parent().ok_or("Missing preview directory")?)
        .map_err(|e| e.to_string())?;
    let parent = path.parent().ok_or("Missing preview directory")?;
    let key = path
        .file_stem()
        .ok_or("Missing preview key")?
        .to_string_lossy();
    let (temporary, file) = loop {
        let path = parent.join(format!(
            ".reforge-preview-{key}-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => break (PendingWrite(path), file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    let (w, h) = frame.geometry().display_size(AspectRatio::Square);
    let mut encoder = png::Encoder::new(BufWriter::new(&file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let (aw, ah) = frame.geometry().display_size(AspectRatio::Original);
    encoder
        .add_text_chunk(
            "reforge.display.v2".into(),
            format!("{aw} {ah} {}", u8::from(frame.is_monochrome())),
        )
        .map_err(|e| e.to_string())?;
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(frame.rgba())
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    // finish flushes the complete PNG. Close the file before publishing it,
    // including on Windows. Never delete a previous checkpoint on failure.
    drop(file);
    fs::rename(&temporary.0, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
