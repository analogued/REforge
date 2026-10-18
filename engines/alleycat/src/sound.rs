// Copyright (c) 2026 analogued. Project license has not been selected.
//! Original PC speaker programs. Host muting never gates game-state work.
//!
//! Tables remain in the external data segment. Events are device writes in PIT
//! clocks, not host notes or independently timed oscillators.

#[cfg(feature = "sdl")]
pub(crate) mod playback;
#[cfg(any(feature = "sdl", feature = "dev-tools"))]
pub mod render;

use crate::assets::{next_random, set_word, word};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    Music,
    Effects,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Write {
    pub tick: u64,
    pub port: u16,
    pub value: u8,
    pub category: Category,
}
#[derive(Default)]
pub struct Sound {
    tick: u64,
    control: u8,
    capture: bool,
    writes: Vec<Write>,
    busy_ticks: u64,
}
impl Sound {
    pub fn record(&mut self, enabled: bool) {
        self.capture = enabled;
        if !enabled {
            self.writes.clear();
        }
    }
    pub fn at(&mut self, tick: u64) {
        self.tick = self.tick.max(tick);
    }
    pub fn reset(&mut self) {
        let capture = self.capture;
        *self = Self::default();
        self.capture = capture;
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn drain(&mut self) -> impl Iterator<Item = Write> + '_ {
        self.writes.drain(..)
    }
    pub fn take_busy_ticks(&mut self) -> u64 {
        std::mem::take(&mut self.busy_ticks)
    }
    fn wait(&mut self, ticks: u64) {
        self.tick += ticks;
        self.busy_ticks += ticks;
    }
    fn write(&mut self, port: u16, value: u8, category: Category) {
        if port == 0x61 {
            self.control = value;
        }
        if self.capture {
            self.writes.push(Write {
                tick: self.tick,
                port,
                value,
                category,
            });
        }
    }
    pub fn stop(&mut self) {
        self.write(0x61, self.control & !3, Category::Effects);
    }
    pub fn gate(&mut self, value: u8) {
        self.write(0x61, value, Category::Effects);
    }
    pub fn control(&self) -> u8 {
        self.control
    }
    pub fn tone(&mut self, divisor: u16, category: Category) {
        self.retune(divisor, category);
        self.write(0x61, self.control | 3, category);
    }
    /// Reprogram channel two without changing the speaker-control bits.
    pub fn retune(&mut self, divisor: u16, category: Category) {
        self.write(0x43, 0xb6, category);
        self.write(0x42, divisor as u8, category);
        self.write(0x42, (divisor >> 8) as u8, category);
    }
    /// CS:53b0, including rests and the non-advancing 66h end marker.
    pub fn splash(&mut self, data: &mut [u8], tick: u16) -> Result<(), String> {
        if data[0] == 0 || word(data, 0x5322) == tick {
            return Ok(());
        }
        set_word(data, 0x5322, tick);
        let index = word(data, 0x5320);
        let note = *data
            .get(0x538c + usize::from(index))
            .ok_or("Splash music exceeds external data")?;
        if note != 0x66 {
            set_word(data, 0x5320, index.wrapping_add(1));
        }
        if note == 0 || note == 0x66 {
            self.stop();
        } else {
            self.tone(word(data, 0x5324 + usize::from(note)), Category::Music);
        }
        Ok(())
    }
    /// CS:5b63; equal consecutive notes are rests, not retriggered notes.
    pub fn melody(&mut self, data: &mut [u8], tick: u16) {
        if data[0] == 0 || tick.wrapping_sub(word(data, 0x59c0)) < 2 {
            return;
        }
        set_word(data, 0x59c0, tick);
        let mut index = word(data, 0x59be) & 0xfe;
        if index >= 0x86 {
            index = 0;
        }
        set_word(data, 0x59be, index + 2);
        let old = word(data, 0x59bc);
        let note = word(data, 0x5934 + usize::from(index));
        set_word(data, 0x59bc, note);
        if old == note {
            self.stop();
        } else {
            self.tone(note, Category::Music);
        }
    }
    pub fn failure_tone(&mut self, data: &mut [u8], tick: u16) {
        if data[0] == 0 || tick.wrapping_sub(word(data, 0x5a83)) < 2 {
            return;
        }
        set_word(data, 0x5a83, tick);
        let index = word(data, 0x5a85);
        set_word(data, 0x5a85, index.wrapping_add(2));
        let address = if data[0x552] == 0 { 0x5a87 } else { 0x5aa3 };
        let note = word(data, address + usize::from(index));
        if data[0x552] != 0 && note == 0 {
            self.stop();
        } else {
            self.tone(note, Category::Effects);
        }
    }
    /// CS:593b. AX is the first divisor; BX is the continuation divisor.
    pub fn request(&mut self, data: &mut [u8], tick: u16, first: u16, next: u16) {
        if data[0] == 0 {
            return;
        }
        set_word(data, 0x5923, next);
        self.tone(first, Category::Effects);
        data[0x5920] = 2;
        set_word(data, 0x5921, tick);
    }
    /// CS:59a3's CX-counted LOOP, fitted to the stock 8088 XT reference.
    pub fn beep(&mut self, data: &[u8], divisor: u16, count: u16) {
        if data[0] == 0 {
            return;
        }
        self.tone(divisor, Category::Effects);
        let count = if data[0x697] == 0xfd {
            count >> 1
        } else {
            count
        };
        self.loop_delay(count, 112);
        self.stop();
    }
    fn loop_delay(&mut self, count: u16, overhead_cycles: u64) {
        let iterations = if count == 0 { 65_536 } else { u64::from(count) };
        // Effective busy-loop cost from MartyPC at 157500000/33 Hz. The XT
        // CPU frequency is four times the PIT input frequency, so divide
        // CPU cycles by four. 18 cycles/iteration is a bounded fit,
        // not a universal instruction timing model. Helper overhead differs:
        // 112 cycles for a beep's gate window, 360 for the sweep's note cadence.
        self.wait((iterations * 18 + overhead_cycles) / 4);
    }
    /// CS:572e. Unlike 59a3, the counted delay runs with native sound off.
    pub fn hearts_sweep(&mut self, data: &mut [u8]) -> u16 {
        let before = self.tick / crate::timing::BIOS_TICK_PIT_CLOCKS;
        for (start, end, step) in [(500u16, 200u16, -30i16), (500, 300, -20), (300, 800, 30)] {
            let mut divisor = start;
            loop {
                set_word(data, 0x5acb, divisor);
                self.loop_delay(if data[0x697] == 0xfd { 0x800 } else { 0x1000 }, 360);
                if data[0] != 0 {
                    self.tone(divisor, Category::Effects);
                }
                divisor = divisor.wrapping_add_signed(step);
                if (step < 0 && divisor <= end) || (step > 0 && divisor >= end) {
                    set_word(data, 0x5acb, divisor);
                    break;
                }
            }
        }
        self.stop();
        (self.tick / crate::timing::BIOS_TICK_PIT_CLOCKS - before) as u16
    }
    /// CS:56f4/5704. The caller owns the nine-BIOS-tick blocking interval.
    pub fn spider_start(&mut self, data: &mut [u8]) {
        set_word(
            data,
            0x5ad0,
            if data[0x697] == 0xfd { 0x400 } else { 0x200 },
        );
    }
    pub fn spider_step(&mut self, data: &mut [u8]) {
        let phase = word(data, 0x5ad0).wrapping_add(1);
        set_word(data, 0x5ad0, phase);
        let index = (phase >> ((phase >> 9) & 15)) & 15;
        self.gate((self.control & !3) | (data[0x5ad2 + usize::from(index)] & data[0]));
    }
    /// CS:5797/57a6. These counters advance even with native sound off.
    pub fn eel_start(&mut self, data: &mut [u8]) {
        self.stop();
        data[0x5acf] = 0;
        set_word(data, 0x5acd, 8);
    }
    pub fn eel_step(&mut self, data: &mut [u8]) {
        data[0x5acf] = data[0x5acf].wrapping_add(1);
        let phase = data[0x5acf] & 63;
        if phase == 0 {
            set_word(data, 0x5acd, word(data, 0x5acd).wrapping_add(1));
        }
        let pulse = if u16::from(phase) < (word(data, 0x5acd) >> 2) & 31 {
            0
        } else {
            2
        };
        self.gate((self.control & !2) | (pulse & data[0]));
    }
    /// CS:5a1c. Supply the latched channel-zero counter, not its output bit.
    pub fn projectile_step(&mut self, data: &mut [u8], counter: u16) {
        if data[0] == 0 {
            return;
        }
        let (elapsed, borrow) = word(data, 0x5a16).overflowing_sub(counter);
        if !borrow && elapsed <= 0x260 {
            return;
        }
        set_word(data, 0x5a16, counter);
        let phase = word(data, 0x5a18).wrapping_add(1);
        set_word(data, 0x5a18, phase);
        let mut divisor = word(data, 0x5a3c) & 0x3ff;
        if divisor >= 0x180 {
            divisor = 0x180u16.wrapping_sub(divisor);
        }
        divisor = (divisor >> 2).wrapping_add(word(data, 0x5a1a + usize::from(phase & 0x1e)));
        let step = if data[0x697] == 0xfd { 2 } else { 1 };
        set_word(data, 0x5a3e, word(data, 0x5a3e).wrapping_add(step));
        set_word(data, 0x5a3c, word(data, 0x5a3c).wrapping_add(step * 4));
        self.tone(
            divisor.wrapping_add(word(data, 0x5a3e) >> 3),
            Category::Effects,
        );
    }
    fn counter(&self) -> u16 {
        (0u16).wrapping_sub((self.tick.wrapping_mul(2) & 0xffff) as u16)
    }
    /// CS:5691. Poll cadence is fitted to the stock XT; the two-tick exit and
    /// table/bit operations are native.
    pub fn milk_warning(&mut self, data: &mut [u8], tick: u16) {
        self.stop();
        set_word(data, 0x5ae2, tick);
        set_word(data, 0x5ae4, 0);
        data[0x5b06] = if data[0x697] == 0xfd { 1 } else { 2 };
        let end = (self.tick / 65_536 + 2) * 65_536;
        loop {
            if data[0] != 0 {
                let phase = word(data, 0x5ae4).wrapping_add(1);
                set_word(data, 0x5ae4, phase);
                let index = (phase >> data[0x5b06]) & 31;
                self.gate(self.control ^ data[0x5ae6 + usize::from(index)]);
            }
            // MartyPC XT pulses average 612 CPU cycles, or 153 PIT clocks.
            // BIOS/IRQ placement and the shorter PCjr shift remain approximate.
            self.wait(153.min(end - self.tick));
            if self.tick == end {
                break;
            }
        }
        data[0x5b07] = 12;
        self.stop();
    }
    /// CS:5ac2, the noisy landing on upper support. Preserve native random
    /// consumption even when host playback is muted.
    pub fn upper_landing(&mut self, data: &mut [u8], tick: u16) {
        set_word(data, 0x5a12, 0x338);
        set_word(data, 0x5a10, tick);
        set_word(data, 0x5a0e, self.counter());
        let end = (self.tick / 65_536 + 2) * 65_536;
        loop {
            let counter = self.counter();
            if counter.wrapping_sub(word(data, 0x5a0e)) >= 0x9c40 {
                set_word(data, 0x5a0e, counter);
                if data[0] != 0 {
                    let divisor = (next_random(data) & 0x7ff).wrapping_add(word(data, 0x5a12));
                    set_word(data, 0x5a12, word(data, 0x5a12).wrapping_sub(2));
                    self.tone(divisor, Category::Effects);
                }
            }
            // XT steady polls average about 1044 CPU cycles with note writes,
            // or 677 with native sound off. Fit to whole PIT clocks; exact
            // counter reads, startup overhead and IRQ placement are unmodeled.
            let poll_ticks = if data[0] != 0 { 261 } else { 169 };
            self.wait(poll_ticks.min(end - self.tick));
            if self.tick == end {
                break;
            }
        }
        self.stop();
    }
    fn burst(&mut self, data: &[u8], category: Category) {
        self.tone(word(data, 0x592a), category);
        // CS:5b28 measures a mode-3 channel-zero count difference, which advances
        // twice per PIT clock. Poll overshoot is handled by caller timing later.
        self.wait(u64::from(word(data, 0x592e)).div_ceil(2));
        self.stop();
    }
    /// CS:546d..5690, called in the original alley divider or room stage.
    pub fn ambient(&mut self, data: &mut [u8], tick: u16) {
        if data[0] == 0 {
            return;
        }
        if data[0x1cb8] != 0 {
            if data[0x5b0f] == 0 {
                if word(data, 0x5b10) != tick {
                    set_word(data, 0x5b10, tick);
                    self.tone((word(data, 0x5b12) & 0x1ff) + 200, Category::Effects);
                    set_word(data, 0x5b12, word(data, 0x5b12).wrapping_sub(75));
                }
            } else {
                if word(data, 0x5b10) != tick {
                    set_word(data, 0x5b10, tick);
                    data[0x5b0f] -= 1;
                }
                data[0x5b0e] = data[0x5b0e].wrapping_sub(1);
                if data[0x5b0e] == 0 {
                    data[0x5b0e] = if data[0x697] == 0xfd { 1 } else { 2 };
                    if next_random(data) as u8 <= 4 {
                        set_word(data, 0x5b0c, word(data, 0x5b0c).wrapping_add(1));
                    }
                    if word(data, 0x5b0c) & 1 != 0 {
                        set_word(data, 0x5b0a, word(data, 0x5b0a).wrapping_add(7));
                    }
                    let divisor = (next_random(data) & word(data, 0x5b0a) & 0x1ff) + 400;
                    self.tone(divisor, Category::Effects);
                }
            }
            return;
        }
        if data[0x5920] != 0 {
            if word(data, 0x5921) != tick {
                set_word(data, 0x5921, tick);
                data[0x5920] -= 1;
                if data[0x5920] == 0 {
                    self.stop();
                } else {
                    self.tone(word(data, 0x5923), Category::Effects);
                }
            }
            return;
        }
        if word(data, 0x5925) == tick {
            return;
        }
        let mut si = 3usize;
        if data[0x1cbf] | data[0x5b07] == 0 {
            si = 1;
            if word(data, 4) == 0 {
                si = if data[0x1673] != 0 { 2 } else { 0 };
            }
        }
        let di = si * 2;
        if data[0x584] | data[0x5b07] == 0
            && tick.wrapping_sub(word(data, 0x5925)) < word(data, di + 0x59f2)
        {
            return;
        }
        set_word(data, 0x5925, tick);
        if data[0x1cbf] != 0 || data[0x5b07] != 0 {
            if data[0x1cbf] == 0 {
                data[0x5b07] -= 1;
            }
            set_word(data, 0x592e, 0x1200);
            let mut bx = word(data, 0x59ba);
            if bx >= 6 {
                bx = 0;
                set_word(data, 0x59ba, 0);
            }
            set_word(data, 0x59ba, word(data, 0x59ba) + 2);
            set_word(data, 0x592a, word(data, usize::from(bx) + 0x5a44));
            self.burst(data, Category::Effects);
            return;
        }
        if si == 2 {
            set_word(data, 0x592a, (u16::from(data[0x1673]) << 4) + 0x200);
            set_word(data, 0x592e, 0x1800);
            self.burst(data, Category::Effects);
            return;
        }
        set_word(data, 0x592e, word(data, di + 0x5a02));
        let carry = data[0x5927] & 1 != 0;
        data[0x5927] >>= 1;
        if carry {
            set_word(data, 0x592e, 0x1000);
            data[0x5927] = 0x80;
            data[0x5928] = data[0x5928].wrapping_add(1);
            let mut al = data[0x5928] & data[si + 0x59fa];
            if al == 0 {
                data[0x5929] = data[0x5929].wrapping_add(data[si + 0x5a0a]);
                let dl = next_random(data) as u8;
                if dl <= data[si + 0x5a0c] {
                    data[0x592d] = dl & 7;
                }
                let mut dx = (next_random(data) & 255) << 1;
                let direction = if dx & 2 != 0 {
                    dx = dx.wrapping_add(0x300);
                    255
                } else {
                    1
                };
                set_word(data, 0x592a, dx);
                data[0x592c] = direction;
            }
            al |= data[0x5929] & data[si + 0x59fc];
            data[0x5928] = al;
        }
        if data[0x592c] != 255 {
            set_word(data, 0x5a54, word(data, 0x5a54).wrapping_add(2));
            let bx = usize::from(word(data, 0x5a54) & 14);
            set_word(data, 0x592a, word(data, bx + 0x5a44));
        } else {
            if word(data, 0x592a) <= 200 {
                set_word(data, 0x592a, 0x500);
            }
            set_word(data, 0x592a, word(data, 0x592a).wrapping_sub(25));
        }
        if data[0x584] != 0 {
            set_word(data, 0x592e, 0x2000);
            data[0x592c] = 255;
            self.burst(data, Category::Effects);
            return;
        }
        let bx = usize::from(data[0x5928]);
        let pattern = data[bx + usize::from(word(data, di + 0x59fe)) + 0x59c2];
        // The recurring alley/room accompaniment is Music. The movement and
        // interaction branches above remain Effects, despite sharing a burst.
        if pattern & data[0x5927] != 0 {
            self.burst(data, Category::Music);
        } else if data[0x592d] != 0 {
            data[0x592d] -= 1;
            set_word(data, 0x592e, word(data, di + 0x5a06));
            self.burst(data, Category::Music);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> Vec<u8> {
        let mut data = vec![0; crate::assets::DATA_LENGTH];
        data[0] = 255;
        set_word(&mut data, 0x2ae5, 0xfa59);
        data
    }
    #[test]
    fn asynchronous_request_keeps_distinct_first_pitch_and_native_tick_expiry() {
        let mut data = data();
        let seed = word(&data, 0x2ae5);
        let mut sound = Sound::default();
        sound.record(true);
        sound.request(&mut data, 100, 0xc00, 0xb54);
        let writes: Vec<_> = sound.drain().collect();
        assert_eq!(
            writes.iter().map(|w| (w.port, w.value)).collect::<Vec<_>>(),
            [(0x43, 0xb6), (0x42, 0), (0x42, 12), (0x61, 3)]
        );
        sound.ambient(&mut data, 100);
        assert_eq!(sound.drain().count(), 0);
        sound.ambient(&mut data, 105);
        assert_eq!(data[0x5920], 1);
        let writes: Vec<_> = sound.drain().collect();
        assert_eq!((writes[1].value, writes[2].value), (0x54, 0x0b));
        sound.ambient(&mut data, 105);
        assert_eq!(sound.drain().count(), 0);
        sound.ambient(&mut data, 106);
        assert_eq!(data[0x5920], 0);
        assert_eq!(sound.drain().last().unwrap().value & 3, 0);
        assert_eq!(word(&data, 0x2ae5), seed);
    }
    #[test]
    fn playback_observation_cannot_gate_ambient_state_randomness_or_blocking_work() {
        let mut audible = data();
        let mut silent = audible.clone();
        for data in [&mut audible, &mut silent] {
            data[0x584] = 1;
            data[0x5927] = 1;
            data[0x592c] = 255;
            set_word(data, 0x592a, 0x500);
        }
        let mut recording = Sound::default();
        recording.record(true);
        let mut discarded = Sound::default();
        for tick in 1..80 {
            recording.ambient(&mut audible, tick);
            discarded.ambient(&mut silent, tick);
        }
        assert_eq!(audible, silent);
        assert_eq!(recording.tick(), discarded.tick());
        assert!(recording.drain().count() > 0);
        assert_eq!(discarded.drain().count(), 0);
        assert_ne!(word(&audible, 0x2ae5), 0xfa59);
        audible[0] = 0;
        let before = audible.clone();
        recording.ambient(&mut audible, 100);
        assert_eq!(before, audible);
        assert_eq!(recording.drain().count(), 0);
    }
    #[test]
    fn splash_rest_and_sentinel_stop_without_advancing_end_marker() {
        let mut data = data();
        data[0x538c] = 2;
        data[0x538d] = 0;
        data[0x538e] = 0x66;
        set_word(&mut data, 0x5326, 1000);
        let mut sound = Sound::default();
        sound.record(true);
        sound.splash(&mut data, 1).unwrap();
        assert_eq!(word(&data, 0x5320), 1);
        assert_eq!(sound.drain().count(), 4);
        sound.splash(&mut data, 2).unwrap();
        assert_eq!(word(&data, 0x5320), 2);
        assert_eq!(sound.drain().last().unwrap().value & 3, 0);
        sound.splash(&mut data, 3).unwrap();
        assert_eq!(word(&data, 0x5320), 2);
        assert_eq!(sound.drain().last().unwrap().value & 3, 0);
    }
    #[test]
    fn repeated_melody_note_is_a_rest_and_table_wrap_is_native() {
        let mut data = data();
        set_word(&mut data, 0x5934, 1200);
        set_word(&mut data, 0x5936, 1200);
        let mut sound = Sound::default();
        sound.record(true);
        sound.melody(&mut data, 2);
        assert_eq!(sound.drain().count(), 4);
        sound.melody(&mut data, 3);
        assert_eq!(sound.drain().count(), 0);
        sound.melody(&mut data, 4);
        assert_eq!(sound.drain().count(), 1);
        set_word(&mut data, 0x59be, 0x86);
        sound.melody(&mut data, 6);
        assert_eq!(word(&data, 0x59be), 2);
    }
    #[test]
    fn hearts_sweep_retains_blocking_work_when_native_sound_is_off() {
        for machine in [0, 0xfd] {
            let mut on = data();
            on[0x697] = machine;
            let mut off = on.clone();
            off[0] = 0;
            let mut audible = Sound::default();
            audible.record(true);
            let mut muted = Sound::default();
            muted.record(true);
            audible.hearts_sweep(&mut on);
            muted.hearts_sweep(&mut off);
            assert_eq!(word(&on, 0x5acb), 810);
            assert_eq!(word(&off, 0x5acb), 810);
            assert_eq!(audible.tick(), muted.tick());
            let writes: Vec<_> = audible.drain().collect();
            assert_eq!(writes.len(), 37 * 4 + 1);
            assert!(
                writes
                    .iter()
                    .all(|write| write.category == Category::Effects)
            );
            assert!(writes.windows(2).all(|w| w[0].tick <= w[1].tick));
            assert_eq!(muted.drain().count(), 1);
            assert_eq!(word(&on, 0x2ae5), 0xfa59);
        }
    }
    #[test]
    fn counted_beep_follows_xt_cadence_and_native_count_rules() {
        let mut cga = data();
        let mut sound = Sound::default();
        sound.beep(&cga, 1024, 6144);
        let cga_ticks = sound.take_busy_ticks();
        let micros = cga_ticks * 1_000_000 / crate::timing::PIT_HZ;
        assert!((23_100..23_300).contains(&micros));
        cga[0x697] = 0xfd;
        sound.beep(&cga, 1024, 6144);
        let pcjr_ticks = sound.take_busy_ticks();
        assert!(pcjr_ticks > cga_ticks / 2);
        assert!(pcjr_ticks < cga_ticks / 2 + 100);
        cga[0] = 0;
        sound.beep(&cga, 1024, 6144);
        assert_eq!(sound.take_busy_ticks(), 0);
        cga[0] = 0xff;
        sound.beep(&cga, 1024, 0);
        assert!(sound.take_busy_ticks() > cga_ticks * 10);
    }
    #[test]
    fn direct_drive_damage_programs_advance_while_native_muted() {
        let mut data = data();
        data[0] = 0;
        data[0x697] = 0xfd;
        let mut sound = Sound::default();
        sound.spider_start(&mut data);
        assert_eq!(word(&data, 0x5ad0), 0x400);
        sound.spider_step(&mut data);
        assert_eq!(word(&data, 0x5ad0), 0x401);
        assert_eq!(sound.control() & 3, 0);
        sound.eel_start(&mut data);
        for _ in 0..64 {
            sound.eel_step(&mut data);
        }
        assert_eq!(word(&data, 0x5acd), 9);
        assert_eq!(data[0x5acf], 64);
        assert_eq!(sound.control() & 3, 0);
    }
    #[test]
    fn projectile_sound_uses_counter_borrow_and_strict_difference_gate() {
        let mut data = data();
        let mut sound = Sound::default();
        sound.record(true);
        set_word(&mut data, 0x5a16, 0x1000);
        sound.projectile_step(&mut data, 0xda0);
        assert_eq!(sound.drain().count(), 0);
        sound.projectile_step(&mut data, 0xd9f);
        assert_eq!(sound.drain().count(), 4);
        assert_eq!(word(&data, 0x5a18), 1);
        assert_eq!(word(&data, 0x5a3c), 4);
        // A reload/borrow accepts even if wrapping subtraction is small.
        sound.projectile_step(&mut data, 0xffff);
        assert_eq!(sound.drain().count(), 4);
        data[0] = 0;
        let before = data.clone();
        sound.projectile_step(&mut data, 0);
        assert_eq!(before, data);
    }
    #[test]
    fn upper_landing_capture_does_not_change_random_consumption_or_duration() {
        let mut a = data();
        let mut b = a.clone();
        let mut recording = Sound::default();
        recording.record(true);
        let mut discarded = Sound::default();
        recording.at(12345);
        discarded.at(12345);
        recording.upper_landing(&mut a, 42);
        discarded.upper_landing(&mut b, 42);
        assert_eq!(a, b);
        assert_eq!(recording.tick(), 2 * 65_536);
        assert_eq!(recording.take_busy_ticks(), discarded.take_busy_ticks());
        assert_ne!(word(&a, 0x2ae5), 0xfa59);
        assert!(recording.drain().count() > 1);
        assert_eq!(discarded.drain().count(), 0);
    }
}
