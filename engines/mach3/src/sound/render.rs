// Copyright (c) 2026 analogued. Project license has not been selected.
//! Select one signal from observed execution; never schedule a sample or device.

use super::{SoundAction, category::sample_category};
use reforge_audio::category::SoundCategory;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioRenderer {
    #[default]
    Speaker,
    EnhancedPcm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Interval {
    Speaker,
    Sample { playback: u64, visit: Option<u32> },
    // The sample returned without stopping channel 2. Suppress its residual
    // PWM until deliberate channel-2/speaker programming or another sample.
    SampleTail,
}

/// Pure rendering policy shared by live and offline output. Initial port-61
/// speaker bits must be observed through actions before any write changes them.
/// The engine's initial speaker bits are zero. Levels use the reference path's
/// 0..8192 range; unsigned PCM midpoint 128 maps to 4096, before backend filtering.
pub struct RenderingPolicy {
    mode: AudioRenderer,
    interval: Interval,
    speaker_high: bool,
    control: u8,
    sample_level: i16,
    category: SoundCategory,
}

impl RenderingPolicy {
    pub fn new(mode: AudioRenderer, initial_drive: bool) -> Self {
        Self {
            mode,
            interval: Interval::Speaker,
            speaker_high: initial_drive,
            control: 0,
            sample_level: 4096,
            category: SoundCategory::Effects,
        }
    }
    pub fn category(&self) -> SoundCategory {
        self.category
    }

    /// Switch host rendering without resetting device/sample observation.
    pub fn set_mode(&mut self, mode: AudioRenderer) -> Option<i16> {
        if self.mode == mode {
            return None;
        }
        self.mode = mode;
        Some(
            if mode == AudioRenderer::Speaker || self.interval == Interval::Speaker {
                self.speaker_level()
            } else {
                self.sample_level
            },
        )
    }
    fn speaker_level(&self) -> i16 {
        if self.speaker_high { 8192 } else { 0 }
    }

    /// Called in device/action order at the event's timestamp. A returned level
    /// is applied at that same timestamp, including equal-timestamp handoffs.
    pub fn drive(&mut self, _tick: u64, high: bool) -> Option<i16> {
        self.speaker_high = high;
        (self.mode == AudioRenderer::Speaker || self.interval == Interval::Speaker)
            .then(|| self.speaker_level())
    }

    /// Observe the action before its derived device edges. Repeated writes for
    /// one source visit remain real hardware operations but select no new PCM
    /// sample; distinct visits select levels even when their values are equal.
    pub fn action(&mut self, _tick: u64, action: SoundAction) -> Option<i16> {
        let speaker_reprogrammed = match action {
            SoundAction::Read { port: 0x61, value } => {
                self.control = value & 3;
                false
            }
            SoundAction::Write {
                port: 0x61, value, ..
            } => {
                let changed = self.control != value & 3;
                self.control = value & 3;
                changed
            }
            SoundAction::Write { port: 0x42, .. } => true,
            // Latch commands do not reprogram the counter.
            SoundAction::Write {
                port: 0x43, value, ..
            } => value >> 6 == 2 && value & 0x30 != 0,
            _ => false,
        };
        let level = match action {
            SoundAction::SourceBegin { playback, request } => {
                self.category = sample_category(request.resource);
                self.interval = Interval::Sample {
                    playback,
                    visit: None,
                };
                Some(4096)
            }
            SoundAction::Write {
                source: Some(source),
                ..
            } => {
                if let Interval::Sample { playback, visit } = &mut self.interval {
                    if source.playback == *playback && *visit != Some(source.visit) {
                        *visit = Some(source.visit);
                        // (value - 128) * 32 + 4096, without signed underflow.
                        self.sample_level = i16::from(source.value) * 32;
                        return (self.mode == AudioRenderer::EnhancedPcm)
                            .then_some(self.sample_level);
                    }
                }
                None
            }
            SoundAction::SourceEnd { playback } if matches!(self.interval, Interval::Sample { playback: active, .. } if active == playback) =>
            {
                self.interval = Interval::SampleTail;
                Some(4096)
            }
            _ if self.interval == Interval::SampleTail && speaker_reprogrammed => {
                self.interval = Interval::Speaker;
                self.category = SoundCategory::Effects;
                Some(self.speaker_level())
            }
            _ => None,
        };
        if let Some(level) = level {
            self.sample_level = level;
        }
        if self.mode == AudioRenderer::Speaker {
            None
        } else {
            level
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::{ByteOrigin, Resource, SampleRequest, SourceByte};

    fn begin(playback: u64) -> SoundAction {
        SoundAction::SourceBegin {
            playback,
            request: SampleRequest {
                resource: Resource::DataB,
                offset: 0,
                count: 4,
                reverse: true,
            },
        }
    }

    fn byte(playback: u64, visit: u32, value: u8, offset: u16) -> SoundAction {
        SoundAction::Write {
            port: 0x42,
            value: 64 - (value >> 2),
            source: Some(SourceByte {
                playback,
                visit,
                value,
                offset,
                resource: Resource::DataB,
                origin: ByteOrigin::Asset,
            }),
        }
    }

    #[test]
    fn classification_survives_renderer_switches_and_tail_then_returns_to_effects() {
        let mut p = RenderingPolicy::new(AudioRenderer::Speaker, false);
        let mut sample = begin(1);
        if let SoundAction::SourceBegin { request, .. } = &mut sample {
            request.resource = Resource::Data9;
        }
        p.action(0, sample);
        assert_eq!(p.category(), SoundCategory::Speech);
        p.set_mode(AudioRenderer::EnhancedPcm);
        assert_eq!(p.category(), SoundCategory::Speech);
        p.action(100, SoundAction::SourceEnd { playback: 1 });
        assert_eq!(p.category(), SoundCategory::Speech);
        p.action(
            101,
            SoundAction::Write {
                port: 0x42,
                value: 10,
                source: None,
            },
        );
        assert_eq!(p.category(), SoundCategory::Effects);
        if let SoundAction::SourceBegin { request, .. } = &mut sample {
            request.resource = Resource::DataC;
        }
        p.action(102, sample);
        assert_eq!(p.category(), SoundCategory::Music);
    }

    #[test]
    fn live_mode_changes_retain_sample_visits_tail_and_speaker_state() {
        let mut p = RenderingPolicy::new(AudioRenderer::Speaker, false);
        assert_eq!(p.action(1, begin(1)), None);
        assert_eq!(p.action(2, byte(1, 0, 65, 4)), None);
        assert_eq!(p.drive(2, true), Some(8192));
        assert_eq!(p.set_mode(AudioRenderer::EnhancedPcm), Some(2080));
        assert_eq!(p.action(3, byte(1, 0, 65, 4)), None); // retained visit
        assert_eq!(p.action(4, byte(1, 1, 66, 3)), Some(2112));
        assert_eq!(p.set_mode(AudioRenderer::Speaker), Some(8192));
        assert_eq!(p.action(5, SoundAction::SourceEnd { playback: 1 }), None);
        assert_eq!(p.set_mode(AudioRenderer::EnhancedPcm), Some(4096));
        assert_eq!(p.set_mode(AudioRenderer::EnhancedPcm), None);
        assert_eq!(p.drive(6, false), None); // retained sample tail
        assert_eq!(
            p.action(
                7,
                SoundAction::Write {
                    port: 0x42,
                    value: 1,
                    source: None
                }
            ),
            Some(0)
        );
        assert_eq!(p.drive(8, true), Some(8192));
    }
    #[test]
    fn source_visits_preserve_low_bits_equal_values_and_reverse_boundary_bytes() {
        let mut policy = RenderingPolicy::new(AudioRenderer::EnhancedPcm, false);
        assert_eq!(policy.action(10, begin(1)), Some(4096));
        assert_eq!(policy.drive(10, true), None); // setup interval
        let mut boundary = byte(1, 0, 65, 4);
        if let SoundAction::Write {
            source: Some(source),
            ..
        } = &mut boundary
        {
            source.origin = ByteOrigin::ExternalMemory;
        }
        assert_eq!(policy.action(71, boundary), Some(2080));
        assert_eq!(policy.action(131, boundary), None);
        // Same PIT count, distinct visits/amplitudes; no fixed-rate cursor.
        assert_eq!(policy.action(191, byte(1, 1, 66, 3)), Some(2112));
        assert_eq!(policy.action(191, byte(1, 2, 66, 2)), Some(2112));
        assert_eq!(policy.action(260, byte(1, 3, 67, 1)), Some(2144));
        assert_eq!(
            policy.action(261, SoundAction::SourceEnd { playback: 2 }),
            None
        );
        assert_eq!(
            policy.action(262, SoundAction::SourceEnd { playback: 1 }),
            Some(4096)
        );
        assert_eq!(policy.action(263, byte(1, 4, 255, 0)), None);
        assert_eq!(policy.action(300, begin(2)), Some(4096));
        assert_eq!(policy.action(331, byte(2, 0, 128, 0)), Some(4096));
        assert_eq!(policy.action(451, byte(2, 1, 255, 1)), Some(8160));
        assert_eq!(policy.action(571, byte(2, 2, 0, 2)), Some(0));
    }

    #[test]
    fn tail_mutes_residual_pwm_but_tracks_drive_and_ignores_keyboard_ack_and_latches() {
        let mut policy = RenderingPolicy::new(AudioRenderer::EnhancedPcm, false);
        policy.action(1, begin(1));
        policy.action(
            1,
            SoundAction::Write {
                port: 0x61,
                value: 0x63,
                source: None,
            },
        );
        policy.action(61, byte(1, 0, 255, 0));
        assert_eq!(
            policy.action(61, SoundAction::SourceEnd { playback: 1 }),
            Some(4096)
        );
        assert_eq!(policy.drive(62, true), None);
        for action in [
            SoundAction::Read {
                port: 0x61,
                value: 0x63,
            },
            SoundAction::Write {
                port: 0x61,
                value: 0xe3,
                source: None,
            },
            SoundAction::Write {
                port: 0x61,
                value: 0x63,
                source: None,
            },
            SoundAction::Write {
                port: 0x43,
                value: 0x80,
                source: None,
            },
            SoundAction::Write {
                port: 0x43,
                value: 0x36,
                source: None,
            },
        ] {
            assert_eq!(policy.action(100, action), None);
        }
        assert_eq!(
            policy.action(
                101,
                SoundAction::Write {
                    port: 0x43,
                    value: 0xb6,
                    source: None
                }
            ),
            Some(8192)
        );
        assert_eq!(policy.drive(101, false), Some(0));
        policy.action(102, begin(2));
        policy.action(103, SoundAction::SourceEnd { playback: 2 });
        assert_eq!(
            policy.action(
                104,
                SoundAction::Write {
                    port: 0x61,
                    value: 0x60,
                    source: None
                }
            ),
            Some(0)
        );
        assert_eq!(policy.drive(105, true), Some(8192));
    }

    #[test]
    fn reference_speaker_never_changes_levels_from_sample_annotations() {
        let mut policy = RenderingPolicy::new(AudioRenderer::Speaker, false);
        assert_eq!(policy.drive(0, false), Some(0));
        assert_eq!(policy.action(1, begin(1)), None);
        assert_eq!(policy.action(61, byte(1, 0, 255, 0)), None);
        assert_eq!(policy.drive(62, true), Some(8192));
        assert_eq!(
            policy.action(63, SoundAction::SourceEnd { playback: 1 }),
            None
        );
        assert_eq!(policy.drive(64, false), Some(0));
    }
}
