// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Small synthetic declarations for existing shared UI tests. Production hosts
//! obtain one complete SettingsCapabilities value from their engine.
use super::*;

const TEST_FRONTEND: crate::launcher::OptionDefinition = crate::launcher::OptionDefinition {
    label: "Renderer",
    default: "graphical",
    storage: crate::launcher::Storage::TerminalMode,
    apply: ApplyTiming::NextLaunch,
    choices: &[
        crate::launcher::Choice {
            value: "graphical",
            label: "Graphical",
            short_label: "Graphical",
            badge: None,
            palette: None,
            palette_hint: crate::launcher::PaletteHint::Auto,
        },
        crate::launcher::Choice {
            value: "terminal",
            label: "Terminal",
            short_label: "Terminal",
            badge: None,
            palette: None,
            palette_hint: crate::launcher::PaletteHint::Auto,
        },
    ],
};

impl<'a> SettingsMenu<'a> {
    /// Expose Centurion's game-only presentation preferences.
    pub fn with_centurion_options(mut self) -> Self {
        let mut session = self.creator.session.borrow_mut();
        if session.game_id == "standalone" {
            session.game_id = "centurion-defender-of-rome".into();
        }
        self.capabilities.options = if session.game_id == "centurion-defender-of-rome" {
            EngineOptions::Centurion
        } else {
            EngineOptions::None
        };
        drop(session);
        self
    }
    /// Expose Alley Cat's game-only input and presentation preferences.
    pub fn with_alleycat_options(mut self) -> Self {
        let mut session = self.creator.session.borrow_mut();
        if session.game_id == "standalone" {
            session.game_id = "alley-cat".into();
        }
        self.capabilities.options = if session.game_id == "alley-cat" {
            EngineOptions::AlleyCat
        } else {
            EngineOptions::None
        };
        self.capabilities.controls.controller_layouts = if session.game_id == "alley-cat" {
            ControllerLayouts::AlleyCat
        } else {
            ControllerLayouts::None
        };
        drop(session);
        self
    }
    /// Use the engine's release-specific Machine choices and default. The
    /// selection shares the launcher's saved pin and applies on the next launch.
    pub fn with_alleycat_machines(
        mut self,
        machine: &'static crate::launcher::OptionDefinition,
    ) -> Self {
        if self.capabilities.options == EngineOptions::AlleyCat {
            self.capabilities.display.machine = Some(machine);
        }
        self
    }

    /// Expose terminal play as a BTZ-only startup choice. A launcher supplies
    /// whether its child can inherit an interactive terminal; SDL play passes
    /// false because an existing game window cannot switch host mid-session.
    pub fn with_btz_options(mut self, terminal_available: bool) -> Self {
        let mut session = self.creator.session.borrow_mut();
        if session.game_id == "standalone" {
            session.game_id = "mindwheel".into();
        }
        let supported = reforge_settings::supports_terminal(&session.game_id);
        self.capabilities.options = if supported {
            EngineOptions::Btz
        } else {
            EngineOptions::None
        };
        self.capabilities.display.frontend = supported.then_some(&TEST_FRONTEND);
        self.availability.terminal = terminal_available;
        drop(session);
        self
    }
    /// Declare available engine renderers, with the engine default first.
    /// Game settings display that concrete renderer for the internal default
    /// alias. Absence leaves game audio controls disabled while shared defaults
    /// can still be edited.
    pub fn with_audio_renderers(
        mut self,
        renderers: &'static [reforge_settings::SoundRenderer],
    ) -> Self {
        self.capabilities.audio.renderers = renderers;
        self.capabilities.audio.default = renderers
            .first()
            .copied()
            .unwrap_or(SoundRenderer::GameDefault);
        self.capabilities.audio.apply = ApplyTiming::Live;
        self
    }
    /// Use the same audio selection, inheritance and persistence as live
    /// renderers, but save changes for the next launch. Such edits require a
    /// registered entry so a running game cannot offer an ineffective change.
    pub fn with_startup_audio_renderers(
        self,
        renderers: &'static [reforge_settings::SoundRenderer],
    ) -> Self {
        let mut menu = self.with_audio_renderers(renderers);
        menu.capabilities.audio.apply = ApplyTiming::NextLaunch;
        menu
    }
    /// Declare semantic groups implemented by the engine's audio adapter.
    pub fn with_audio_categories(
        mut self,
        categories: &'static [reforge_settings::SoundCategory],
    ) -> Self {
        self.capabilities.audio.categories = categories;
        self
    }
    /// Expose Mach 3's native startup switch, independently of host audio.
    /// Standalone previews retain in-memory settings; app launches use their
    /// configured per-game storage. An explicit CLI value is temporary/locked.
    pub fn with_mach3_options(mut self, samples: Option<bool>) -> Self {
        {
            let mut session = self.creator.session.borrow_mut();
            if session.game_id == "standalone" {
                session.game_id = "mach-3".into();
            }
            self.capabilities.options = if session.game_id == "mach-3" {
                EngineOptions::Mach3
            } else {
                EngineOptions::None
            };
            if self.capabilities.options == EngineOptions::Mach3 {
                if let Some(value) = samples {
                    session.cli.insert(
                        reforge_settings::DIGITIZED_SAMPLES.into(),
                        value.to_string(),
                    );
                }
            }
        }
        self
    }
    /// A command-line motion choice is temporary and locked like shared display
    /// overrides. Ordinary menu edits remain per-game and apply live.
    pub fn with_mach3_motion(self, motion: Option<reforge_settings::MotionMode>) -> Self {
        if self.capabilities.options == EngineOptions::Mach3
            && let Some(motion) = motion
        {
            self.creator
                .session
                .borrow_mut()
                .cli
                .insert(reforge_settings::MOTION.into(), motion.as_str().into());
        }
        self
    }
    /// Temporarily lock Alley Cat's live presentation choice for this run.
    pub fn with_alleycat_motion(self, motion: Option<reforge_settings::AlleyCatMotion>) -> Self {
        if self.capabilities.options == EngineOptions::AlleyCat
            && let Some(motion) = motion
        {
            self.creator.session.borrow_mut().cli.insert(
                reforge_settings::ALLEYCAT_MOTION.into(),
                motion.as_str().into(),
            );
        }
        self
    }

    /// Resolve once before starting simulation. Saving a later draft does not
    /// change the running game's native sample flag.
    pub fn mach3_startup_settings(&self) -> reforge_settings::Mach3Settings {
        let session = self.creator.session.borrow();
        session.effective(&session.saved).mach3
    }
}
