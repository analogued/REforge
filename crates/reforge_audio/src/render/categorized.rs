// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Volume groups for one observed signal, preserving its original interruptions.
//! Filtering occurs per group so a previous group's decay retains its own gain.

use super::{BlipRenderer, CategoryMixer, Config, Error, Output, SignalRenderer};
use crate::category::{CategoryVolumes, SoundCategory};

pub struct CategorizedRenderer {
    reference: BlipRenderer,
    groups: [BlipRenderer; 3],
    scratch: [Vec<i16>; 4],
    category: SoundCategory,
    level: i16,
    mixer: CategoryMixer,
    mixed: Vec<i16>,
    stopped: bool,
}
impl CategorizedRenderer {
    pub fn new(config: Config, epoch: u64) -> Result<Self, Error> {
        Ok(Self {
            reference: BlipRenderer::new(config, epoch, 0)?,
            groups: [
                BlipRenderer::new(config, epoch, 0)?,
                BlipRenderer::new(config, epoch, 0)?,
                BlipRenderer::new(config, epoch, 0)?,
            ],
            scratch: std::array::from_fn(|_| Vec::new()),
            category: SoundCategory::Effects,
            level: 0,
            mixer: CategoryMixer::new(config.sample_rate)?,
            mixed: Vec::new(),
            stopped: false,
        })
    }
    /// Startup levels are immediate; later changes use five-millisecond ramps.
    /// This does not change or restart the source signal or its device clock.
    /// Finish existing audio to the desired source timestamp before changing
    /// gains if the change must start precisely at that committed boundary.
    pub fn set_volumes(&mut self, volumes: CategoryVolumes) -> Result<(), Error> {
        self.mixer.set_volumes(volumes)
    }
    pub fn level(&self) -> i16 {
        self.level
    }
    pub fn level_at(
        &mut self,
        tick: u64,
        category: SoundCategory,
        level: i16,
        output: &mut Output<'_>,
    ) -> Result<(), Error> {
        self.advance(tick, Some((category, level)), output)
    }
    pub fn finish_until(&mut self, tick: u64, output: &mut Output<'_>) -> Result<(), Error> {
        self.advance(tick, None, output)
    }
    fn advance(
        &mut self,
        tick: u64,
        change: Option<(SoundCategory, i16)>,
        output: &mut Output<'_>,
    ) -> Result<(), Error> {
        if self.stopped {
            return Err(Error::Stopped);
        }
        if tick < self.reference.tick() {
            return Err(Error::Invalid(
                "signal time cannot precede committed renderer time",
            ));
        }
        if change.is_some_and(|(_, level)| !(-super::MAX_LEVEL..=super::MAX_LEVEL).contains(&level))
        {
            return Err(Error::Invalid("signal level exceeds synthesis headroom"));
        }
        let result = (|| {
            // Drain one existing synthesis chunk at a time, bounding scratch
            // storage even when the caller advances across a long silent wait.
            while let Some(end) = self.reference.chunk_end().filter(|end| *end <= tick) {
                self.pass(end, None, output)?;
            }
            self.pass(tick, change, output)
        })();
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
    fn pass(
        &mut self,
        tick: u64,
        change: Option<(SoundCategory, i16)>,
        output: &mut Output<'_>,
    ) -> Result<(), Error> {
        for scratch in &mut self.scratch {
            scratch.clear();
        }
        if let Some((category, level)) = change {
            self.category = category;
            self.level = level;
            self.reference.level_at(tick, level, &mut |s| {
                self.scratch[0].extend_from_slice(s);
                Ok(())
            })?;
        } else {
            self.reference.finish_until(tick, &mut |s| {
                self.scratch[0].extend_from_slice(s);
                Ok(())
            })?;
        }
        for (index, renderer) in self.groups.iter_mut().enumerate() {
            let scratch = &mut self.scratch[index + 1];
            let mut collect = |s: &[i16]| {
                scratch.extend_from_slice(s);
                Ok(())
            };
            if change.is_some() {
                renderer.level_at(
                    tick,
                    if index == self.category.index() {
                        self.level
                    } else {
                        0
                    },
                    &mut collect,
                )?;
            } else {
                renderer.finish_until(tick, &mut collect)?;
            }
        }
        let n = self.scratch[0].len();
        if self.scratch[1..].iter().any(|s| s.len() != n) {
            return Err(Error::Invalid("category synthesis lost alignment"));
        }
        self.mixed.resize(n, 0);
        self.mixer.mix(
            [&self.scratch[1], &self.scratch[2], &self.scratch[3]],
            Some(&self.scratch[0]),
            &mut self.mixed,
        )?;
        if n != 0 {
            output(&self.mixed)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            clock_hz: 1_193_182,
            sample_rate: 48_000,
            chunk_ticks: 11_931,
        }
    }
    #[test]
    fn unity_matches_single_signal_across_category_handoffs_and_long_waits() {
        let mut original = BlipRenderer::new(config(), 0, 0).unwrap();
        let mut grouped = CategorizedRenderer::new(config(), 0).unwrap();
        let (mut expected, mut actual) = (Vec::new(), Vec::new());
        for (tick, category, level) in [
            (0, SoundCategory::Music, 4096),
            (53, SoundCategory::Music, 8192),
            (703, SoundCategory::Speech, 0),
            (790, SoundCategory::Speech, 4096),
            (16001, SoundCategory::Effects, 8192),
            (17007, SoundCategory::Effects, 0),
        ] {
            original
                .level_at(tick, level, &mut |s| {
                    expected.extend_from_slice(s);
                    Ok(())
                })
                .unwrap();
            grouped
                .level_at(tick, category, level, &mut |s| {
                    actual.extend_from_slice(s);
                    Ok(())
                })
                .unwrap();
        }
        original
            .finish_until(1_200_000, &mut |s| {
                expected.extend_from_slice(s);
                Ok(())
            })
            .unwrap();
        grouped
            .finish_until(1_200_000, &mut |s| {
                actual.extend_from_slice(s);
                Ok(())
            })
            .unwrap();
        assert_eq!(expected, actual);
    }
    #[test]
    fn live_gain_ramps_reach_half_volume_without_restarting_synthesis() {
        let mut full = CategorizedRenderer::new(config(), 0).unwrap();
        let mut adjusted = CategorizedRenderer::new(config(), 0).unwrap();
        let (mut reference, mut actual) = (Vec::new(), Vec::new());
        for tick in (0..80_000).step_by(500) {
            if tick == 20_000 {
                full.finish_until(tick, &mut |s| {
                    reference.extend_from_slice(s);
                    Ok(())
                })
                .unwrap();
                adjusted
                    .finish_until(tick, &mut |s| {
                        actual.extend_from_slice(s);
                        Ok(())
                    })
                    .unwrap();
                adjusted
                    .set_volumes(CategoryVolumes([50, 100, 100]))
                    .unwrap();
            }
            let level = if tick % 1000 == 0 { 8192 } else { 0 };
            full.level_at(tick, SoundCategory::Music, level, &mut |s| {
                reference.extend_from_slice(s);
                Ok(())
            })
            .unwrap();
            adjusted
                .level_at(tick, SoundCategory::Music, level, &mut |s| {
                    actual.extend_from_slice(s);
                    Ok(())
                })
                .unwrap();
        }
        full.finish_until(80_000, &mut |s| {
            reference.extend_from_slice(s);
            Ok(())
        })
        .unwrap();
        adjusted
            .finish_until(80_000, &mut |s| {
                actual.extend_from_slice(s);
                Ok(())
            })
            .unwrap();
        assert_eq!(reference.len(), actual.len());
        assert_eq!(&reference[..700], &actual[..700]);
        for (&full, &half) in reference.iter().zip(&actual).skip(1500) {
            assert_eq!(half, (f64::from(full) * 0.5).round() as i16);
        }
    }

    #[test]
    fn muted_group_is_silent_without_muting_the_following_group() {
        let mut renderer = CategorizedRenderer::new(config(), 0).unwrap();
        renderer
            .set_volumes(CategoryVolumes([100, 0, 100]))
            .unwrap();
        renderer.finish_until(20_000, &mut |_| Ok(())).unwrap();
        let mut muted = Vec::new();
        for tick in (20_000..60_000).step_by(500) {
            renderer
                .level_at(
                    tick,
                    SoundCategory::Speech,
                    if tick % 1000 == 0 { 8192 } else { 0 },
                    &mut |s| {
                        muted.extend_from_slice(s);
                        Ok(())
                    },
                )
                .unwrap();
        }
        renderer
            .finish_until(60_000, &mut |s| {
                muted.extend_from_slice(s);
                Ok(())
            })
            .unwrap();
        assert!(muted.iter().all(|s| *s == 0));
        let mut audible = Vec::new();
        for tick in (60_000..100_000).step_by(500) {
            renderer
                .level_at(
                    tick,
                    SoundCategory::Effects,
                    if tick % 1000 == 0 { 8192 } else { 0 },
                    &mut |s| {
                        audible.extend_from_slice(s);
                        Ok(())
                    },
                )
                .unwrap();
        }
        renderer
            .finish_until(100_000, &mut |s| {
                audible.extend_from_slice(s);
                Ok(())
            })
            .unwrap();
        assert!(audible.iter().any(|s| *s != 0));
    }
}
