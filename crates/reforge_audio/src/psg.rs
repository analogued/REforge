// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! SN76489 three-tone subset, in input-chip clocks (sixteen per counter step).
//!
//! Register writes do not restart the running tone counters. Noise is explicitly
//! unsupported except its mute command; this is sufficient for programs using
//! only the three tone generators. There is no host mute or sample clock here.

/// Relative two-decibel attenuation steps, scaled for three voices of headroom.
/// This is a presentation scale, not a calibrated physical output voltage.
const LEVELS: [i16; 16] = [
    2730, 2169, 1723, 1368, 1087, 863, 686, 545, 433, 344, 273, 217, 172, 137, 109, 0,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Tone {
    divider: u16,
    attenuation: u8,
    high: bool,
    next: Option<u64>,
}

/// Combined three-tone level after an oscillator transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Edge {
    pub tick: u64,
    pub level: i16,
}

/// Deterministic digital tone generators, with explicit initial silent phase.
/// Zero dividers use a 1,024-step period. This defines the TI-counter variant;
/// chips treating zero as a constant output are outside this subset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TonePsg {
    tick: u64,
    tones: [Tone; 3],
    latch: u8,
}

impl Default for TonePsg {
    fn default() -> Self {
        Self::new()
    }
}

impl TonePsg {
    /// Start at clock zero, muted, outputs low and one counter step from reload.
    /// This is an initialization convention, not a hardware power-on claim.
    pub fn new() -> Self {
        Self {
            tick: 0,
            tones: [Tone {
                divider: 0,
                attenuation: 15,
                high: false,
                next: Some(16),
            }; 3],
            latch: 0,
        }
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn level(&self) -> i16 {
        self.tones
            .iter()
            .filter(|tone| tone.high)
            .map(|tone| LEVELS[usize::from(tone.attenuation)])
            .sum()
    }

    /// Decode one latch/data byte. Unsupported audible noise leaves all state
    /// unchanged rather than silently approximating another generator.
    pub fn write(&mut self, value: u8) -> Result<(), &'static str> {
        let latch = if value & 0x80 != 0 {
            (value >> 4) & 7
        } else {
            self.latch
        };
        if latch == 6 || (latch == 7 && value & 15 != 15) {
            return Err("PSG noise generation is outside the three-tone subset");
        }
        if latch != 7 {
            let tone = &mut self.tones[usize::from(latch / 2)];
            if latch & 1 != 0 {
                tone.attenuation = value & 15;
            } else if value & 0x80 != 0 {
                tone.divider = (tone.divider & 0x3f0) | u16::from(value & 15);
            } else {
                tone.divider = (tone.divider & 15) | (u16::from(value & 0x3f) << 4);
            }
        }
        self.latch = latch;
        Ok(())
    }

    /// Advance at most to the next oscillator transition. All channels with
    /// that deadline transition together, even while their outputs are muted.
    pub fn advance_to(&mut self, target: u64) -> Result<Option<Edge>, &'static str> {
        if target < self.tick {
            return Err("PSG cannot run backwards");
        }
        let next = self.tones.iter().filter_map(|tone| tone.next).min();
        let Some(next) = next.filter(|next| *next <= target) else {
            self.tick = target;
            return Ok(None);
        };
        self.tick = next;
        for tone in &mut self.tones {
            if tone.next == Some(next) {
                tone.high = !tone.high;
                let divider = if tone.divider == 0 {
                    1024
                } else {
                    tone.divider
                };
                tone.next = next.checked_add(u64::from(divider) * 16);
            }
        }
        Ok(Some(Edge {
            tick: next,
            level: self.level(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(chip: &mut TonePsg, target: u64) -> Vec<Edge> {
        let mut edges = Vec::new();
        while chip.tick() < target {
            if let Some(edge) = chip.advance_to(target).unwrap() {
                edges.push(edge);
            }
        }
        edges
    }

    #[test]
    fn latch_data_period_and_two_decibel_attenuation() {
        let mut chip = TonePsg::new();
        chip.write(0x85).unwrap();
        chip.write(0x02).unwrap(); // Divider 0x25, not 0x205.
        chip.write(0x90).unwrap();
        let edges = advance(&mut chip, 16 + 0x25 * 32);
        assert_eq!(
            edges[0],
            Edge {
                tick: 16,
                level: 2730
            }
        );
        assert!(edges.contains(&Edge {
            tick: 16 + 0x25 * 16,
            level: 0
        }));
        assert!(edges.contains(&Edge {
            tick: 16 + 0x25 * 32,
            level: 2730
        }));
        chip.write(0x91).unwrap();
        assert_eq!(chip.level(), 2169);
        chip.write(15).unwrap(); // Data byte also updates latched volume.
        assert_eq!(chip.level(), 0);
    }

    #[test]
    fn writes_preserve_running_phase_and_muting_preserves_device_work() {
        let mut normal = TonePsg::new();
        normal.write(0x82).unwrap();
        normal.write(0).unwrap();
        normal.write(0x90).unwrap();
        advance(&mut normal, 20);
        let mut muted = normal.clone();
        muted.write(0x9f).unwrap();
        normal.write(0x84).unwrap();
        muted.write(0x84).unwrap();
        // The previous count completes at 48, then the new period takes effect.
        let edges = advance(&mut normal, 150);
        advance(&mut muted, 150);
        assert_eq!(edges[0].tick, 48);
        assert_eq!(edges[1].tick, 112);
        muted.write(0x90).unwrap();
        normal.write(0x90).unwrap();
        assert_eq!(normal, muted);
    }

    #[test]
    fn chunking_simultaneous_channels_and_rejected_noise() {
        let mut normal = TonePsg::new();
        for latch in [0x80, 0xa0, 0xc0] {
            normal.write(latch | 2).unwrap();
            normal.write(0).unwrap();
            normal.write(latch | 0x10).unwrap();
        }
        normal.write(0xff).unwrap();
        let mut split = normal.clone();
        let expected = advance(&mut normal, 1000);
        let actual: Vec<_> = [20, 57, 200, 1000]
            .into_iter()
            .flat_map(|until| advance(&mut split, until))
            .collect();
        assert_eq!(expected, actual);
        assert_eq!(expected[0].level, 8190);
        assert_eq!(normal, split);
        assert!(normal.write(0xe0).is_err());
        assert!(normal.write(0xf0).is_err());
        assert_eq!(normal, split);
    }
}
