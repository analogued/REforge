// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently implemented Butterworth speaker response. The filter design
//! (third-order high-pass 120 Hz, low-pass 4,300 Hz) follows DOSBox Staging
//! v0.83.0; see THIRD_PARTY.md. No upstream implementation code is used.

use super::Error;

/// Additional output response after band-limited reconstruction, before gain.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputFilter {
    /// Only the resampler's existing reconstruction and DC removal.
    #[default]
    None,
    /// Small-speaker bandwidth, independently of device timing and user gain.
    PcSpeaker,
}

impl OutputFilter {
    pub(super) fn build(self, rate: u32) -> Result<Option<SpeakerFilter>, Error> {
        match self {
            Self::None => Ok(None),
            Self::PcSpeaker if rate <= 8_600 => Err(Error::Invalid(
                "PC speaker response requires a sample rate above 8600 Hz",
            )),
            Self::PcSpeaker => Ok(Some(SpeakerFilter::new(rate))),
        }
    }
}

// A third-order Butterworth denominator factors into a first-order pole and
// a second-order section with Q = 1. Bilinear-transform prewarping places each
// digital cutoff at its specified frequency. Sections retain f64 state between
// blocks; only the final sample is rounded/clamped to the PCM output range.
struct Section {
    b: [f64; 3],
    a: [f64; 2],
    state: [f64; 2],
}

impl Section {
    fn new(rate: u32, cutoff: f64, high_pass: bool, second_order: bool) -> Self {
        let k = (std::f64::consts::PI * cutoff / f64::from(rate)).tan();
        let (b, a) = if second_order {
            let norm = 1.0 / (1.0 + k + k * k);
            let gain = if high_pass { norm } else { k * k * norm };
            (
                [gain, if high_pass { -2.0 } else { 2.0 } * gain, gain],
                [2.0 * (k * k - 1.0) * norm, (1.0 - k + k * k) * norm],
            )
        } else {
            let norm = 1.0 / (1.0 + k);
            let gain = if high_pass { norm } else { k * norm };
            (
                [gain, if high_pass { -gain } else { gain }, 0.0],
                [(k - 1.0) * norm, 0.0],
            )
        };
        Self {
            b,
            a,
            state: [0.0; 2],
        }
    }

    fn sample(&mut self, input: f64) -> f64 {
        let output = self.b[0] * input + self.state[0];
        self.state[0] = self.b[1] * input - self.a[0] * output + self.state[1];
        self.state[1] = self.b[2] * input - self.a[1] * output;
        // Drop inaudible residuals before a long silence reaches subnormals.
        for state in &mut self.state {
            if state.abs() < 1e-20 {
                *state = 0.0;
            }
        }
        output
    }
}

pub(super) struct SpeakerFilter([Section; 4]);

impl SpeakerFilter {
    fn new(rate: u32) -> Self {
        Self([
            Section::new(rate, 120.0, true, false),
            Section::new(rate, 120.0, true, true),
            Section::new(rate, 4_300.0, false, false),
            Section::new(rate, 4_300.0, false, true),
        ])
    }

    fn sample(&mut self, mut input: f64) -> f64 {
        for section in &mut self.0 {
            input = section.sample(input);
        }
        input
    }

    pub(super) fn process(&mut self, samples: &mut [i16]) {
        for sample in samples {
            *sample = self
                .sample(f64::from(*sample))
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_response_matches_bilinear_butterworth_at_multiple_rates() {
        // Measure steady-state sine power, independently of section coefficients.
        // The ideal third-order Butterworth power response is 1/(1+w^6),
        // using the bilinear frequency mapping for each digital cutoff.
        for rate in [11_025, 32_000, 44_100, 48_000, 96_000, 192_000] {
            for hz in [30.0, 120.0, 1_000.0, 4_300.0, f64::from(rate) * 0.45] {
                let mut filter = SpeakerFilter::new(rate);
                let (mut before, mut after) = (0.0, 0.0);
                for i in 0..rate * 2 {
                    let input = (std::f64::consts::TAU * hz * f64::from(i) / f64::from(rate)).sin();
                    let output = filter.sample(input);
                    if i >= rate {
                        before += input * input;
                        after += output * output;
                    }
                }
                let warp = |f: f64| (std::f64::consts::PI * f / f64::from(rate)).tan();
                let expected = 1.0
                    / (1.0 + (warp(120.0) / warp(hz)).powi(6))
                    / (1.0 + (warp(hz) / warp(4_300.0)).powi(6));
                let error_db = 10.0 * (after / before / expected).log10();
                assert!(error_db.abs() < 0.01, "{rate} Hz, {hz} Hz: {error_db} dB");
            }
        }
    }

    #[test]
    fn dc_settles_and_full_scale_transitions_stay_finite() {
        let mut filter = SpeakerFilter::new(48_000);
        for input in [f64::from(i16::MAX), f64::from(i16::MIN), 0.0] {
            let mut last = 0.0;
            for _ in 0..48_000 {
                last = filter.sample(input);
                assert!(last.is_finite());
            }
            assert!(last.abs() < 1e-6);
        }
    }
}
