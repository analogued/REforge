// Copyright (c) 2026 analogued. Project license has not been selected.
//! Startup screen assets, timer wait, and manual diagnostic presentation.

#[cfg(any(feature = "sdl", test))]
mod splash;
#[cfg(feature = "sdl")]
pub(crate) use splash::{SPLASH_DIVISOR, SplashWait};

use std::path::Path;

use crate::{
    Error,
    assets::{GraphicsAssets, read_resource},
    frame::Frame,
    scene,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Splash,
    Intro,
    InitialScene,
}

/// Decoded startup screens, initial scene, and graphics for the gameplay simulation.
#[derive(Debug)]
pub struct StartupFrames {
    frames: [Frame; 3],
    graphics: GraphicsAssets,
}

impl StartupFrames {
    #[cfg(all(test, feature = "sdl"))]
    pub(crate) fn from_test_graphics(graphics: GraphicsAssets) -> Self {
        let initial = scene::initial_frame(&graphics).unwrap();
        Self {
            frames: [
                Frame::from_packed(&[0; crate::frame::FRAME_BYTES]).unwrap(),
                Frame::from_packed(&[0; crate::frame::FRAME_BYTES]).unwrap(),
                initial,
            ],
            graphics,
        }
    }

    /// Load DATA1 and DATA6 as CGA screens and DATA2 through DATA5 as graphics.
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, Error> {
        Self::load_with(|name| read_resource(directory.as_ref(), name))
    }

    pub(crate) fn load_with(
        mut read: impl FnMut(&str) -> Result<Vec<u8>, Error>,
    ) -> Result<Self, Error> {
        let splash = Frame::from_cga(&read("DATA1")?)?;
        let intro = Frame::from_cga(&read("DATA6")?)?;
        let graphics = GraphicsAssets::load_with(&mut read)?;
        let initial_scene = scene::initial_frame(&graphics)?;
        Ok(Self {
            frames: [splash, intro, initial_scene],
            graphics,
        })
    }

    pub fn frame(&self, screen: Screen) -> &Frame {
        let index = match screen {
            Screen::Splash => 0,
            Screen::Intro => 1,
            Screen::InitialScene => 2,
        };
        &self.frames[index]
    }

    /// Choose the intro with the retained random stream's low-three-bit branch.
    pub fn return_intro(&self, use_data6: bool) -> &Frame {
        if use_data6 {
            &self.frames[1]
        } else {
            &self.frames[0]
        }
    }

    pub fn graphics(&self) -> &GraphicsAssets {
        &self.graphics
    }
}

/// Advance once per fresh Space press, stopping at the initial scene.
#[derive(Debug)]
pub struct Presentation {
    screen: Screen,
    space_held: bool,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            screen: Screen::Splash,
            space_held: false,
        }
    }
}

impl Presentation {
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Return true for a fresh press, including one on the terminal screen.
    pub fn space_down(&mut self, repeat: bool) -> bool {
        if repeat || self.space_held {
            return false;
        }
        self.space_held = true;
        self.advance_screen();
        true
    }

    /// A fresh controller confirmation has its own held/release tracking.
    pub fn advance_screen(&mut self) {
        self.screen = match self.screen {
            Screen::Splash => Screen::Intro,
            Screen::Intro | Screen::InitialScene => Screen::InitialScene,
        };
    }

    /// Return to intro while retaining the key latch that initiated the return.
    pub fn return_to_intro(&mut self) {
        self.screen = Screen::Intro;
    }

    pub fn space_up(&mut self) {
        self.space_held = false;
    }

    /// Clear the latch when focus loss prevents receiving a Space release.
    pub fn focus_lost(&mut self) {
        self.space_held = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{FRAME_BYTES, ROW_BYTES};
    use std::{fs, path::PathBuf};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = Self(std::env::temp_dir().join(format!(
                "mach3-startup-test-{}-{suffix}",
                std::process::id()
            )));
            fs::create_dir(&directory.0).unwrap();
            directory
        }

        fn write_resources(&self) {
            let mut splash = vec![0; 0x4000];
            splash[0] = 0x1b;
            splash[0x2000] = 0xe4;
            let mut intro = vec![0; 0x3f40];
            intro[0] = 0x55;
            intro[0x3f3f] = 0xff;
            fs::write(self.0.join("data1"), splash).unwrap();
            fs::write(self.0.join("DaTa6"), intro).unwrap();
            fs::write(self.0.join("DATA2"), vec![0x08; 28_960]).unwrap();
            fs::write(self.0.join("data3"), vec![0; 16_000]).unwrap();
            fs::write(self.0.join("Data4"), sprites(0)).unwrap();
            fs::write(self.0.join("DATA5"), sprites(0xff)).unwrap();
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn sprites(pixel: u8) -> Vec<u8> {
        // All scene draws fit within these authored synthetic records.
        let mut input = vec![0; 0x200];
        for index in 0..190 {
            let pointer = u16::try_from(0x2000 + input.len() - 0x200).unwrap();
            input[index * 2..index * 2 + 2].copy_from_slice(&pointer.to_le_bytes());
            input.extend_from_slice(&[0, 0, 0, 14, 18, 0, 0]);
            input.extend_from_slice(&[pixel; 14 * 18]);
        }
        input
    }

    fn assert_invalid(result: Result<StartupFrames, Error>, message: &str) {
        match result {
            Err(Error::InvalidData(error)) => assert!(error.contains(message), "{error}"),
            _ => panic!("expected InvalidData containing {message}"),
        }
    }

    #[test]
    fn requires_a_fresh_press_for_each_screen_and_stops_at_scene() {
        let mut presentation = Presentation::default();
        assert_eq!(presentation.screen(), Screen::Splash);
        presentation.space_down(true);
        assert_eq!(presentation.screen(), Screen::Splash);
        presentation.space_down(false);
        assert_eq!(presentation.screen(), Screen::Intro);
        presentation.space_down(true);
        presentation.space_down(false);
        assert_eq!(presentation.screen(), Screen::Intro);
        presentation.space_up();
        assert_eq!(presentation.screen(), Screen::Intro);
        presentation.space_down(true);
        assert_eq!(presentation.screen(), Screen::Intro);
        presentation.space_down(false);
        assert_eq!(presentation.screen(), Screen::InitialScene);
        for _ in 0..3 {
            presentation.space_up();
            presentation.space_down(false);
            assert_eq!(presentation.screen(), Screen::InitialScene);
        }
    }

    #[test]
    fn focus_loss_clears_a_held_space_without_advancing() {
        let mut presentation = Presentation::default();
        presentation.space_down(false);
        presentation.focus_lost();
        assert_eq!(presentation.screen(), Screen::Intro);
        presentation.space_down(true);
        assert_eq!(presentation.screen(), Screen::Intro);
        presentation.space_down(false);
        assert_eq!(presentation.screen(), Screen::InitialScene);
    }

    #[test]
    fn intro_remains_until_a_fresh_manual_press() {
        let mut presentation = Presentation::default();
        presentation.space_down(false);
        presentation.space_up();
        for _ in 0..100 {
            assert_eq!(presentation.screen(), Screen::Intro);
        }
    }

    #[test]
    fn explicit_resource_reader_loads_selected_images_and_graphics_despite_bad_case_variants() {
        let directory = TestDirectory::new();
        directory.write_resources();
        let reference = StartupFrames::load(&directory.0).unwrap();
        fs::rename(directory.0.join("DATA2"), directory.0.join("data2")).unwrap();
        fs::write(directory.0.join("DATA1"), b"wrong image").unwrap();
        fs::write(directory.0.join("DATA2"), b"wrong graphics").unwrap();
        let variants = fs::read_dir(&directory.0)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .eq_ignore_ascii_case("DATA1")
            })
            .count();
        if variants != 2 {
            return;
        }
        assert!(StartupFrames::load(&directory.0).is_err());
        let selected = StartupFrames::load_with(|name| match name {
            "DATA1" => fs::read(directory.0.join("data1")).map_err(Error::Io),
            "DATA2" => fs::read(directory.0.join("data2")).map_err(Error::Io),
            other => read_resource(&directory.0, other),
        })
        .unwrap();
        for screen in [Screen::Splash, Screen::Intro, Screen::InitialScene] {
            assert_eq!(
                selected.frame(screen).packed(),
                reference.frame(screen).packed()
            );
        }
        assert_eq!(selected.graphics.primary, reference.graphics.primary);
    }

    #[test]
    fn loads_cga_screens_and_composes_graphics_with_dos_names() {
        let directory = TestDirectory::new();
        directory.write_resources();
        let startup = StartupFrames::load(&directory.0).unwrap();
        assert_eq!(startup.frame(Screen::Splash).packed()[0], 0x1b);
        assert_eq!(startup.frame(Screen::Splash).packed()[ROW_BYTES], 0xe4);
        assert_eq!(startup.frame(Screen::Intro).packed()[0], 0x55);
        assert_eq!(startup.frame(Screen::Intro).packed()[FRAME_BYTES - 1], 0xff);
        assert!(
            startup
                .frame(Screen::InitialScene)
                .packed()
                .iter()
                .all(|byte| *byte == 0x80)
        );
    }

    #[test]
    fn rejects_missing_and_truncated_startup_screens() {
        let directory = TestDirectory::new();
        assert_invalid(StartupFrames::load(&directory.0), "missing resource DATA1");
        directory.write_resources();
        fs::write(directory.0.join("data1"), vec![0; 0x3f3f]).unwrap();
        assert_invalid(
            StartupFrames::load(&directory.0),
            "truncated CGA framebuffer",
        );
        directory.write_resources();
        fs::write(directory.0.join("DaTa6"), vec![0; 0x3f3f]).unwrap();
        assert_invalid(
            StartupFrames::load(&directory.0),
            "truncated CGA framebuffer",
        );
    }

    #[test]
    fn rejects_ambiguous_startup_resource_names() {
        let directory = TestDirectory::new();
        directory.write_resources();
        fs::write(directory.0.join("DATA1"), vec![0; 0x4000]).unwrap();
        let matches = fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_str().unwrap().eq_ignore_ascii_case("DATA1"))
            .count();
        // Case-insensitive filesystems cannot create two distinct spellings.
        if matches == 2 {
            assert_invalid(
                StartupFrames::load(&directory.0),
                "ambiguous resource name DATA1",
            );
        }
    }
}
