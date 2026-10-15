// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Persistent preferences and temporary launch overrides, independent of SDL.
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item};

pub const KEYS: [&str; 5] = ["window_mode", "scale", "renderer", "backend", "vsync"];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoSettings {
    pub fullscreen: bool,
    pub scale: Option<u32>,
    pub renderer: Renderer,
    pub backend: Backend,
    pub vsync: bool,
}
impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            scale: None,
            renderer: Renderer::Auto,
            backend: Backend::Auto,
            vsync: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Renderer {
    #[default]
    Auto,
    Gpu,
    Sdl,
    Software,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Backend {
    #[default]
    Auto,
    Vulkan,
    OpenGl,
    OpenGlEs,
    Direct3d11,
    Direct3d12,
    Metal,
}
impl Renderer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Gpu => "gpu",
            Self::Sdl => "sdl",
            Self::Software => "software",
        }
    }
}
impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Vulkan => "vulkan",
            Self::OpenGl => "opengl",
            Self::OpenGlEs => "opengles2",
            Self::Direct3d11 => "direct3d11",
            Self::Direct3d12 => "direct3d12",
            Self::Metal => "metal",
        }
    }
}
impl std::str::FromStr for Renderer {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "auto" => Ok(Self::Auto),
            "gpu" => Ok(Self::Gpu),
            "sdl" => Ok(Self::Sdl),
            "software" => Ok(Self::Software),
            _ => Err("renderer must be auto, gpu, sdl or software".into()),
        }
    }
}
impl std::str::FromStr for Backend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "auto" => Ok(Self::Auto),
            "vulkan" => Ok(Self::Vulkan),
            "opengl" => Ok(Self::OpenGl),
            "opengles2" => Ok(Self::OpenGlEs),
            "direct3d11" => Ok(Self::Direct3d11),
            "direct3d12" => Ok(Self::Direct3d12),
            "metal" => Ok(Self::Metal),
            _ => Err("unknown renderer backend".into()),
        }
    }
}
impl VideoSettings {
    pub fn get(self, key: &str) -> String {
        match key {
            "window_mode" => if self.fullscreen {
                "fullscreen"
            } else {
                "windowed"
            }
            .into(),
            "scale" => self.scale.map_or("auto".into(), |x| x.to_string()),
            "renderer" => self.renderer.as_str().into(),
            "backend" => self.backend.as_str().into(),
            "vsync" => self.vsync.to_string(),
            _ => String::new(),
        }
    }
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "window_mode" => {
                self.fullscreen = match value {
                    "windowed" => false,
                    "fullscreen" => true,
                    _ => return Err("window mode must be windowed or fullscreen".into()),
                }
            }
            "scale" => {
                self.scale = if value == "auto" {
                    None
                } else {
                    Some(
                        value
                            .parse::<u32>()
                            .ok()
                            .filter(|x| (1..=16).contains(x))
                            .ok_or("scale must be auto or 1..16")?,
                    )
                }
            }
            "renderer" => self.renderer = value.parse()?,
            "backend" => self.backend = value.parse()?,
            "vsync" => {
                self.vsync = match value {
                    "true" | "on" => true,
                    "false" | "off" => false,
                    _ => return Err("vsync must be on or off".into()),
                }
            }
            _ => return Err("unknown display setting".into()),
        };
        Ok(())
    }
}
pub type Patch = BTreeMap<String, String>;
fn overlay(mut base: VideoSettings, patch: &Patch) -> VideoSettings {
    for (k, v) in patch {
        let _ = base.set(k, v);
    }
    base
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Draft {
    pub global: Patch,
    pub game: Patch,
}
impl Draft {
    pub fn global_values(&self) -> VideoSettings {
        overlay(VideoSettings::default(), &self.global)
    }
    pub fn game_values(&self) -> VideoSettings {
        overlay(self.global_values(), &self.game)
    }
    pub fn edit(&mut self, game: bool, key: &str, value: String) {
        let base = if game {
            self.global_values()
        } else {
            VideoSettings::default()
        };
        let patch = if game {
            &mut self.game
        } else {
            &mut self.global
        };
        if value == base.get(key) {
            patch.remove(key);
        } else {
            patch.insert(key.into(), value);
        }
    }
}
#[derive(Clone, Debug)]
pub struct Launch {
    pub game: String,
    pub path: Option<PathBuf>,
    pub cli: Patch,
}
thread_local! {static LAUNCH:RefCell<Option<Launch>>=const{RefCell::new(None)};}
pub struct LaunchGuard(Option<Launch>);
impl Launch {
    pub fn install(self) -> LaunchGuard {
        LaunchGuard(LAUNCH.with(|v| v.replace(Some(self))))
    }
}
impl Drop for LaunchGuard {
    fn drop(&mut self) {
        LAUNCH.with(|v| v.replace(self.0.take()));
    }
}

#[derive(Clone)]
pub struct Session {
    pub game: String,
    pub path: Option<PathBuf>,
    pub cli: Patch,
    pub saved: Draft,
    pub warning: Option<String>,
    blocked: bool,
}
impl Session {
    pub fn for_window(scale: Option<u32>) -> Self {
        let launch = LAUNCH.with(|v| v.borrow().clone());
        match launch {
            Some(x) => Self::load(x),
            None => Self {
                game: "standalone".into(),
                path: None,
                cli: Patch::new(),
                saved: Draft {
                    global: scale
                        .map(|n| BTreeMap::from([("scale".into(), n.to_string())]))
                        .unwrap_or_default(),
                    game: Patch::new(),
                },
                warning: None,
                blocked: false,
            },
        }
    }
    pub fn load(launch: Launch) -> Self {
        let mut s = Self {
            game: launch.game,
            path: launch.path,
            cli: launch.cli,
            saved: Draft::default(),
            warning: None,
            blocked: false,
        };
        if let Some(path) = &s.path {
            match read(path) {
                Ok(doc) => {
                    s.saved = read_draft(&doc, &s.game, &mut s.warning);
                }
                Err(e) => {
                    s.warning = Some(e);
                    s.blocked = true;
                }
            }
        }
        s
    }
    pub fn effective(&self, draft: &Draft) -> VideoSettings {
        overlay(draft.game_values(), &self.cli)
    }
    pub fn commit(&mut self, draft: Draft) -> Result<(), String> {
        if self.blocked {
            return Err("preferences were applied, but the invalid/unreadable configuration was preserved; repair it before saving".into());
        }
        if let Some(path) = &self.path {
            save_changes(path, &self.game, &self.saved, &draft)?;
        }
        self.saved = draft;
        self.warning = None;
        Ok(())
    }
}
fn read(path: &Path) -> Result<DocumentMut, String> {
    match fs::read_to_string(path) {
        Ok(s) => {
            let doc = s
                .parse::<DocumentMut>()
                .map_err(|e| format!("Cannot parse {}: {e}", path.display()))?;
            if doc
                .get("schema_version")
                .is_some_and(|v| v.as_integer() != Some(1))
            {
                return Err("unsupported settings schema_version; file preserved".into());
            }
            Ok(doc)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}
fn read_draft(doc: &DocumentMut, game: &str, warning: &mut Option<String>) -> Draft {
    fn patch(item: Option<&Item>, warning: &mut Option<String>) -> Patch {
        let mut p = Patch::new();
        if let Some(item) = item {
            if let Some(table) = item.as_table_like() {
                for key in KEYS {
                    if let Some(v) = table.get(key) {
                        let text: String = if let Some(s) = v.as_str() {
                            s.into()
                        } else if let Some(n) = v.as_integer() {
                            n.to_string()
                        } else if let Some(b) = v.as_bool() {
                            b.to_string()
                        } else {
                            String::new()
                        };
                        let mut validated = VideoSettings::default();
                        if validated.set(key, &text).is_ok() {
                            p.insert(key.into(), validated.get(key));
                        } else {
                            *warning = Some(format!(
                                "Invalid display setting {key}; using inherited value"
                            ));
                        }
                    }
                }
            } else {
                *warning = Some("Invalid display section; using inherited values".into());
            }
        }
        p
    }
    Draft {
        global: patch(doc.get("global").and_then(|x| x.get("display")), warning),
        game: patch(
            doc.get("games")
                .and_then(|x| x.get(game))
                .and_then(|x| x.get("display")),
            warning,
        ),
    }
}
fn save_changes(path: &Path, game: &str, old: &Draft, new: &Draft) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut directories = fs::DirBuilder::new();
    directories.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directories.mode(0o700);
    }
    directories.create(parent).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|e| format!("configuration is busy; retry Apply: {e}"))?;
    let mut doc = read(path)?;
    if !doc.contains_key("schema_version") {
        doc["schema_version"] = toml_edit::value(1);
    }
    for (before, after, route) in [
        (&old.global, &new.global, vec!["global", "display"]),
        (&old.game, &new.game, vec!["games", game, "display"]),
    ] {
        for key in KEYS {
            if before.get(key) == after.get(key) {
                continue;
            }
            let mut item = doc.as_item_mut();
            for part in &route {
                if item.is_none() {
                    *item = Item::Table(toml_edit::Table::new());
                }
                if !item.is_table_like() {
                    return Err("configuration section is not a table; file preserved".into());
                }
                item = &mut item[part];
            }
            if let Some(text) = after.get(key) {
                let replacement = match key {
                    "vsync" => toml_edit::Value::from(text == "true" || text == "on"),
                    "scale" if text != "auto" => {
                        toml_edit::Value::from(text.parse::<i64>().map_err(|e| e.to_string())?)
                    }
                    _ => toml_edit::Value::from(text.as_str()),
                };
                let mut replacement = replacement;
                if let Some(old) = item.get(key).and_then(Item::as_value) {
                    *replacement.decor_mut() = old.decor().clone();
                }
                item[key] = Item::Value(replacement);
            } else if let Some(table) = item.as_table_like_mut() {
                table.remove(key);
            }
        }
    }
    let temporary = parent.join(format!(
        ".reforge-config-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&temporary).map_err(|e| e.to_string())?;
        f.write_all(doc.to_string().as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub fn default_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("REforge/config.toml"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join("Library/Application Support/REforge/config.toml"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|x| !x.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|x| PathBuf::from(x).join(".config")))
            .map(|p| p.join("reforge/config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "reforge-prefs-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn file(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn session(path: PathBuf, game: &str) -> Session {
        Session::load(Launch {
            game: game.into(),
            path: Some(path),
            cli: Patch::new(),
        })
    }
    #[test]
    fn sparse_game_edits_track_global_and_remove_equal_values() {
        let mut d = Draft::default();
        d.edit(false, "scale", "4".into());
        assert_eq!(d.game_values().scale, Some(4));
        assert!(d.game.is_empty());
        d.edit(true, "scale", "8".into());
        d.edit(false, "vsync", "false".into());
        assert!(!d.game_values().vsync);
        assert_eq!(d.game.len(), 1);
        d.edit(true, "scale", "4".into());
        assert!(d.game.is_empty());
    }
    #[test]
    fn temporary_cli_is_not_saved_and_global_reset_keeps_game_entries() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(
            &path,
            "schema_version=1\n[global.display]\nscale=4\n[games.mach-3.display]\nvsync=false\n",
        )
        .unwrap();
        let mut s = session(path.clone(), "mach-3");
        s.cli.insert("scale".into(), "8".into());
        assert_eq!(s.effective(&s.saved).scale, Some(8));
        let mut d = s.saved.clone();
        d.global.clear();
        s.commit(d).unwrap();
        let loaded = session(path.clone(), "mach-3");
        assert_eq!(loaded.saved.game_values().scale, None);
        assert!(!loaded.saved.game_values().vsync);
        assert!(!fs::read_to_string(path).unwrap().contains("scale = 8"));
    }
    #[test]
    fn merging_two_sessions_preserves_comments_other_games_and_unknown_keys() {
        let dir = TestDir::new();
        let path = dir.file();
        fs::write(&path,"# user note\nschema_version=1\n[global.display]\nvsync=true # keep this\nfuture='keep'\n[games.alley-cat.display]\nscale=3\n").unwrap();
        let mut a = session(path.clone(), "alley-cat");
        let mut b = session(path.clone(), "mach-3");
        let mut ad = a.saved.clone();
        ad.edit(true, "scale", "5".into());
        a.commit(ad).unwrap();
        let mut bd = b.saved.clone();
        bd.edit(false, "vsync", "false".into());
        bd.edit(true, "renderer", "software".into());
        b.commit(bd).unwrap();
        let result = fs::read_to_string(&path).unwrap();
        assert!(
            result.contains("# user note")
                && result.contains("# keep this")
                && result.contains("future='keep'")
        );
        assert_eq!(
            session(path.clone(), "alley-cat").saved.game_values().scale,
            Some(5)
        );
        assert_eq!(
            session(path, "mach-3").saved.game_values().renderer,
            Renderer::Software
        );
    }
    #[test]
    fn malformed_files_are_preserved_and_invalid_fields_do_not_hide_valid_fields() {
        let dir = TestDir::new();
        let path = dir.file();
        let bad = "[broken\n";
        fs::write(&path, bad).unwrap();
        let mut s = session(path.clone(), "mach-3");
        assert!(s.warning.is_some());
        assert!(s.commit(Draft::default()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), bad);
        fs::write(
            &path,
            "[global.display]\nscale=99\nvsync=false # valid with comment\nrenderer='software'\n",
        )
        .unwrap();
        let s = session(path, "mach-3");
        assert!(s.warning.is_some());
        assert_eq!(s.saved.game_values().scale, None);
        assert!(!s.saved.game_values().vsync);
        assert_eq!(s.saved.game_values().renderer, Renderer::Software);
    }
    #[test]
    fn writer_contention_is_reported_and_can_be_retried() {
        let dir = TestDir::new();
        let path = dir.file();
        let mut s = session(path.clone(), "mach-3");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        let mut d = s.saved.clone();
        d.edit(true, "scale", "4".into());
        assert!(s.commit(d.clone()).is_err());
        assert!(!path.exists());
        drop(lock);
        s.commit(d).unwrap();
        assert_eq!(session(path, "mach-3").saved.game_values().scale, Some(4));
    }
}
