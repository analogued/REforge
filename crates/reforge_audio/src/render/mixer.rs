// Copyright (c) 2026 analogued. Project license has not been selected.
//! Clock-independent mono category mixing, reusable by engine-owned sources.

use super::Error;
use crate::category::CategoryVolumes;

pub struct CategoryMixer {
    gains: [f64; 3],
    targets: [f64; 3],
    remaining: u32,
    ramp_samples: u32,
    started: bool,
}
impl CategoryMixer {
    pub fn new(sample_rate: u32) -> Result<Self, Error> {
        if sample_rate == 0 {
            return Err(Error::Invalid("sample rate must be positive"));
        }
        Ok(Self {
            gains: [1.0; 3],
            targets: [1.0; 3],
            remaining: 0,
            ramp_samples: (sample_rate / 200).max(1),
            started: false,
        })
    }
    /// Startup levels are immediate; later changes ramp over five milliseconds.
    pub fn set_volumes(&mut self, volumes: CategoryVolumes) -> Result<(), Error> {
        if !volumes.valid() {
            return Err(Error::Invalid("category volume must be 0..100"));
        }
        let targets = volumes.0.map(|v| f64::from(v) / 100.0);
        if self.targets != targets {
            self.targets = targets;
            if self.started {
                self.remaining = self.ramp_samples;
            } else {
                self.gains = targets;
                self.remaining = 0;
            }
        }
        Ok(())
    }
    /// Mix Music/Speech/Effects blocks at one sample rate and sample position.
    /// Optional reference PCM preserves an existing mono pipeline exactly at
    /// unity, avoiding rounding differences from independently filtered groups.
    /// Independent multi-source engines can omit it and sum their own groups.
    pub fn mix(
        &mut self,
        groups: [&[i16]; 3],
        reference: Option<&[i16]>,
        output: &mut [i16],
    ) -> Result<(), Error> {
        if groups.iter().any(|s| s.len() != output.len())
            || reference.is_some_and(|s| s.len() != output.len())
        {
            return Err(Error::Invalid("category PCM blocks are not aligned"));
        }
        for (index, sample) in output.iter_mut().enumerate() {
            if self.remaining != 0 {
                for i in 0..3 {
                    self.gains[i] += (self.targets[i] - self.gains[i]) / f64::from(self.remaining);
                }
                self.remaining -= 1;
                if self.remaining == 0 {
                    self.gains = self.targets;
                }
            }
            *sample = if let Some(reference) = reference.filter(|_| self.gains == [1.0; 3]) {
                reference[index]
            } else {
                let mixed: f64 = (0..3)
                    .map(|i| f64::from(groups[i][index]) * self.gains[i])
                    .sum();
                mixed
                    .round()
                    .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
            };
        }
        self.started |= !output.is_empty();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_sources_have_independent_gains_and_saturate_safely() {
        let mut mixer = CategoryMixer::new(48000).unwrap();
        mixer.set_volumes(CategoryVolumes([50, 100, 0])).unwrap();
        let mut output = [0; 2];
        mixer
            .mix(
                [&[2000, 30000], &[3000, 30000], &[4000, 30000]],
                None,
                &mut output,
            )
            .unwrap();
        assert_eq!(output, [4000, i16::MAX]);
        assert!(mixer.mix([&[], &[], &[]], None, &mut output).is_err());
    }
}
