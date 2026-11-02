// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Player presentation choices; the interpreter remains frontend independent.

use reforge_sdl::launcher::{Choice, DISPLAY, OptionDefinition, Storage, WINDOW};

pub const OPTIONS: &[OptionDefinition] = &[
    OptionDefinition {
        label: "Renderer",
        choices: &[
            Choice {
                value: "graphical",
                label: "Graphical",
                short_label: "Graphical",
                badge: None,
                palette: None,
            },
            Choice {
                value: "terminal",
                label: "Terminal",
                short_label: "Terminal",
                badge: Some("Terminal"),
                palette: None,
            },
        ],
        default: "graphical",
        storage: Storage::TerminalMode,
    },
    DISPLAY,
    WINDOW,
];
