// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Background engine preview generation; drawing never opens original/cache files.
use super::super::model::{EntryStatus, GameEntry};
use crate::detection::{self, Game, Release};
use reforge_sdl::{AspectRatio, DisplayGeometry, preview::Frame};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{BufReader, BufWriter},
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
struct Request {
    entry: String,
    release: String,
    original: PathBuf,
    preferred: String,
}
struct Reply {
    request: Request,
    frames: BTreeMap<String, Source>,
}
pub(in crate::launcher) struct Manager {
    send: mpsc::Sender<Request>,
    receive: mpsc::Receiver<Reply>,
    pending: HashSet<Request>,
}
impl Manager {
    pub fn new(cache: Option<PathBuf>) -> Self {
        let (send, jobs) = mpsc::channel::<Request>();
        let (done, receive) = mpsc::channel();
        // One worker bounds CPU and disk pressure when a library first opens.
        std::thread::spawn(move || {
            while let Ok(request) = jobs.recv() {
                let frames = generate(&request, cache.as_deref());
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
            self.pending.remove(&reply.request);
            if let Some(game) = games.iter_mut().find(|g| {
                g.entry_id == reply.request.entry
                    && g.release_id == reply.request.release
                    && g.original == reply.request.original
                    && g.status == EntryStatus::Verified
            }) {
                game.previews = reply.frames;
            }
        }
        for game in games {
            if game.preview_attempted || game.status.is_missing() {
                continue;
            }
            game.preview_attempted = true;
            let request = Request {
                entry: game.entry_id.clone(),
                release: game.release_id.clone(),
                original: game.original.clone(),
                preferred: game.preview_variant().into(),
            };
            if self.pending.insert(request.clone()) && self.send.send(request).is_err() {
                break;
            }
        }
    }
}
fn revision(game: Game) -> u32 {
    match game {
        Game::AlleyCat => alleycat::launcher::PREVIEW_REVISION,
        Game::Mach3 => mach3::launcher::PREVIEW_REVISION,
    }
}
fn cache_id(release: &Release, variant: &str) -> String {
    let mut hash = Sha256::new();
    // Version includes the PNG geometry contract; engines version their scene recipes.
    hash.update(format!(
        "reforge-preview-v1\0{}\0{}\0{}\0{}\0",
        release.game.id(),
        release.id,
        revision(release.game),
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
    let mut output = BTreeMap::new();
    let Some(release) = detection::RELEASES.iter().find(|r| r.id == request.release) else {
        return output;
    };
    let variants = match release.game {
        Game::AlleyCat => alleycat::launcher::PREVIEW_VARIANTS,
        Game::Mach3 => mach3::launcher::PREVIEW_VARIANTS,
    };
    // The entry was verified on the main thread. Reverify before decoding originals
    // so source replacement while a queued job waits cannot poison a release cache.
    let detected = match detection::detect(&request.original) {
        Ok(d) if d.release.id == request.release => d,
        _ => return output,
    };
    let mut ordered = variants.to_vec();
    ordered.sort_by_key(|v| *v != request.preferred);
    for variant in ordered {
        let id = cache_id(release, variant);
        let path = cache.map(|root| root.join(format!("{id}.png")));
        let cached = path.as_ref().and_then(|path| load(path).ok());
        let frame = if let Some(frame) = cached {
            frame
        } else {
            let generated = match release.game {
                Game::AlleyCat => alleycat::launcher::preview(&detected.original, variant),
                Game::Mach3 => mach3::launcher::preview(&detected.files, variant),
            };
            let frame = match generated {
                Ok(frame) => frame,
                Err(error) => {
                    eprintln!(
                        "REforge: preview unavailable for {} ({variant}): {error}",
                        release.metadata.title
                    );
                    continue;
                }
            };
            if let Some(path) = &path {
                if let Err(error) = store(path, &frame) {
                    eprintln!("REforge: cannot cache preview: {error}");
                }
            }
            frame
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
        .find(|t| t.keyword == "reforge.display.v1")
        .ok_or("Missing preview geometry")?;
    let dimensions = geometry
        .text
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if dimensions.len() != 2 || dimensions.contains(&0) {
        return Err("Invalid preview geometry".into());
    }
    let geometry = DisplayGeometry::new((info.width, info.height), (dimensions[0], dimensions[1]));
    let mut rgba = vec![0; reader.output_buffer_size()];
    let result = reader.next_frame(&mut rgba).map_err(|e| e.to_string())?;
    rgba.truncate(result.buffer_size());
    Frame::new(geometry, rgba)
}
fn store(path: &Path, frame: &Frame) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(path.parent().ok_or("Missing preview directory")?)
        .map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let (w, h) = frame.geometry().display_size(AspectRatio::Square);
        let file = File::create(&temporary).map_err(|e| e.to_string())?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let (aw, ah) = frame.geometry().display_size(AspectRatio::Original);
        encoder
            .add_text_chunk("reforge.display.v1".into(), format!("{aw} {ah}"))
            .map_err(|e| e.to_string())?;
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(frame.rgba())
            .map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
        // Unix rename replaces atomically. Windows may require removal of an old
        // cache item; source images are disposable and concurrent readers retry.
        if let Err(error) = fs::rename(&temporary, path) {
            if !path.exists() {
                return Err(error.to_string());
            }
            fs::remove_file(path).map_err(|e| e.to_string())?;
            fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_roundtrips_geometry_rejects_corruption_and_replaces_stale_data() {
        let dir =
            std::env::temp_dir().join(format!("reforge-preview-cache-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.png");
        let frame = Frame::new(DisplayGeometry::new((2, 1), (16, 9)), vec![255; 8]).unwrap();
        store(&path, &frame).unwrap();
        let read = load(&path).unwrap();
        assert_eq!(read.geometry(), frame.geometry());
        assert_eq!(read.rgba(), frame.rgba());
        fs::write(&path, b"broken cache").unwrap();
        assert!(load(&path).is_err());
        store(&path, &frame).unwrap();
        assert_eq!(load(&path).unwrap().rgba(), frame.rgba());
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn cache_identity_is_release_content_and_visual_variant_based() {
        let release = &detection::RELEASES[0];
        assert_ne!(cache_id(release, "cga"), cache_id(release, "pcjr"));
        let different = Release {
            files: &[],
            ..*release
        };
        assert_ne!(cache_id(release, "cga"), cache_id(&different, "cga"));
        assert_eq!(cache_id(release, "cga").len(), 64);
    }
}
