// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored test scenarios consumed by the production rule decoder and parser.

use super::ScriptRules;

pub(crate) fn diagnostic_rules() -> ScriptRules {
    // Exercise terrain, formations and enemy families in the bounded test game.
    // This replaces the former special-purpose opening command interpreter.
    let commands = [
        5, 0xcd, 0x91, 7, 0x84, 6, 0x8e, 3, 0x8a, 7, 0xd4, 0x88, 9, 0x40, 0x97, 9, 0x40, 0x96, 2,
        0xd4, 0x84, 2, 0x92, 0xcf,
    ];
    let mut data = vec![0; 65_536];
    for script in 0..21 {
        let start = 0x0a3e + script * 50;
        data[start..start + commands.len()].copy_from_slice(&commands);
    }
    for family in 0..10 {
        let first = if family == 0 {
            187
        } else {
            175 + 3 * (family - 1).min(3)
        };
        for choice in 0..3 {
            data[0x04ac + family * 3 + choice] = (first + choice) as u8;
        }
    }
    // Synthetic far-effect rows preserve the established unit-test geometry.
    for row in 0..16 {
        let sequence: &[u8] = match row {
            0..=2 => &[69, 61, 69, 67, 65, 64],
            3..=4 => &[69, 68, 67, 66, 65, 64],
            5..=7 => &[68, 67, 66, 65, 64],
            8..=11 => &[67, 66, 65, 64],
            _ => &[66, 65, 63, 64],
        };
        let start = 0x0562 + row * 10;
        data[start..start + sequence.len()].copy_from_slice(sequence);
    }
    ScriptRules::from_data_segment(&data).unwrap()
}
