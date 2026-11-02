// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bind command-line launches to an existing installed copy without registering it.
use crate::detection::Detected;
use reforge_settings::{Launch, library::Library};
use std::{ffi::OsString, path::Path};

pub fn bind(
    launch: &mut Launch,
    detected: &Detected,
    args: &mut Vec<OsString>,
) -> Result<(), String> {
    let Some(config) = launch.path.as_ref() else {
        return Ok(());
    };
    let mut library = Library::new(config.with_file_name("library.toml")).with_storage(
        config.clone(),
        reforge_settings::library::default_saves_path(),
    );
    library.reload()?;
    let Some(entry) = library.find_directory(&detected.original)? else {
        return Ok(());
    };
    let registered = crate::detection::RELEASES
        .iter()
        .find(|r| r.id == entry.release_id && r.game().id() == entry.game_id);
    if !registered
        .is_some_and(|r| r.id == detected.release.id && r.compatible_with(detected.release))
    {
        return Err("Registered game files changed release; use Relocate in the launcher to verify the replacement".into());
    }
    launch.entry_id = Some(entry.entry_id.clone());
    add_native_options(config, &entry.entry_id, detected, args)
}

#[cfg(feature = "sdl")]
fn add_native_options(
    config: &Path,
    id: &str,
    detected: &Detected,
    args: &mut Vec<OsString>,
) -> Result<(), String> {
    use reforge_sdl::launcher::Storage;
    let saved = reforge_settings::launcher::native_options(config, id)?;
    let options = match detected.game {
        crate::detection::Game::AlleyCat => alleycat::launcher::OPTIONS,
        crate::detection::Game::Mach3 => mach3::launcher::OPTIONS,
        crate::detection::Game::Mindwheel | crate::detection::Game::Essex => btz::launcher::OPTIONS,
    };
    for option in options {
        if let Storage::Native { key, argument } = option.storage
            && !args.iter().any(|arg| arg == argument)
            && let Some(value) = saved
                .get(key)
                .filter(|value| option.choices.iter().any(|c| c.value == value.as_str()))
        {
            args.extend([argument.into(), value.into()]);
        }
    }
    Ok(())
}
#[cfg(not(feature = "sdl"))]
fn add_native_options(
    _: &Path,
    _: &str,
    _: &Detected,
    _: &mut Vec<OsString>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{Game, RELEASES};
    use reforge_settings::{Patch, Session};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "reforge-profile-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn directory_binding_is_per_copy_and_never_creates_an_unregistered_entry() {
        let temp = Temp::new();
        let config = temp.0.join("config.toml");
        let mut library =
            Library::new(temp.0.join("library.toml")).with_storage(config.clone(), None);
        let first = library
            .add(
                "alley-cat",
                RELEASES[0].id,
                "Alley Cat",
                &temp.0.join("one"),
            )
            .unwrap();
        let second = library
            .add(
                "alley-cat",
                RELEASES[0].id,
                "Alley Cat",
                &temp.0.join("two"),
            )
            .unwrap();
        let mut session = Session::load(Launch {
            game_id: "alley-cat".into(),
            entry_id: Some(first.clone()),
            path: Some(config.clone()),
            cli: Patch::new(),
        });
        let mut draft = session.saved.clone();
        draft.global.insert("master_volume".into(), "70".into());
        draft.game.insert("master_volume".into(), "40".into());
        session.commit(draft).unwrap();
        let mut launch = Launch {
            game_id: "alley-cat".into(),
            entry_id: None,
            path: Some(config.clone()),
            cli: Patch::new(),
        };
        for (directory, expected_id, volume) in [
            ("one", Some(first.as_str()), 40),
            ("two", Some(second.as_str()), 70),
            ("unregistered", None, 70),
        ] {
            launch.entry_id = None;
            let detected = Detected {
                files: Default::default(),
                game: Game::AlleyCat,
                release: &RELEASES[0],
                original: temp.0.join(directory),
            };
            bind(&mut launch, &detected, &mut Vec::new()).unwrap();
            assert_eq!(launch.entry_id.as_deref(), expected_id);
            let session = Session::load(launch.clone());
            assert_eq!(session.effective(&session.saved).audio.volume, volume);
        }
        library.reload().unwrap();
        assert_eq!(library.entries.len(), 2);
        launch.path = None;
        launch.entry_id = None;
        bind(
            &mut launch,
            &Detected {
                files: Default::default(),
                game: Game::AlleyCat,
                release: &RELEASES[0],
                original: temp.0.join("one"),
            },
            &mut Vec::new(),
        )
        .unwrap();
        assert!(launch.entry_id.is_none());
    }
    #[cfg(feature = "sdl")]
    #[test]
    fn native_pins_are_used_but_explicit_arguments_win() {
        let temp = Temp::new();
        let config = temp.0.join("config.toml");
        reforge_settings::launcher::save_native_option(
            &config,
            "alley-cat-a1b2c3",
            "machine",
            Some("pcjr"),
        )
        .unwrap();
        let detected = Detected {
            files: Default::default(),
            game: Game::AlleyCat,
            release: &RELEASES[0],
            original: temp.0.join("game"),
        };
        let mut args = Vec::new();
        add_native_options(&config, "alley-cat-a1b2c3", &detected, &mut args).unwrap();
        assert_eq!(args, [OsString::from("--machine"), OsString::from("pcjr")]);
        let mut args = vec![OsString::from("--machine"), OsString::from("cga")];
        add_native_options(&config, "alley-cat-a1b2c3", &detected, &mut args).unwrap();
        assert_eq!(args, [OsString::from("--machine"), OsString::from("cga")]);
    }
}
