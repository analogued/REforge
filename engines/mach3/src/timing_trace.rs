// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Development-only host execution spans and native scheduling observations.

use crate::gameplay::Snapshot;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
    time::{Duration, Instant},
};

pub(crate) struct TimingTrace {
    origin: Instant,
    clock: Box<dyn FnMut() -> Instant>,
    output: BufWriter<File>,
    sequence: u64,
    game: Option<u64>,
    period: Duration,
    error: Option<std::io::Error>,
}

impl TimingTrace {
    pub fn create(path: &Path, period: Duration) -> Result<Self, String> {
        Self::create_with_clock(path, period, Instant::now(), Instant::now)
    }

    pub(crate) fn create_with_clock(
        path: &Path,
        period: Duration,
        origin: Instant,
        clock: impl FnMut() -> Instant + 'static,
    ) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("cannot create timing trace {}: {error}", path.display()))?;
        let mut trace = Self {
            origin,
            clock: Box::new(clock),
            output: BufWriter::new(file),
            sequence: 0,
            game: None,
            period,
            error: None,
        };
        trace.write(b"# reforge-mach3-timing-v1\nseq\tgame\tkind\tbody\tpass\tscheduled_ns\ttimeline_ns\thost_start_ns\thost_end_ns\tbody_period_ns\tscript\trng_a\trng_b\trng_c\tbackground_layer\talternate\tterrain_selector\tprimary_count\tdifficulty\tenemy_active\tspecial_active\tdetail\n");
        trace.flush()?;
        Ok(trace)
    }

    pub fn now(&mut self) -> Instant {
        (self.clock)()
    }

    pub fn begin_game(&mut self) {
        self.game = Some(self.game.map_or(0, |game| game + 1));
    }

    fn relative(&self, time: Instant) -> i128 {
        if time >= self.origin {
            time.duration_since(self.origin).as_nanos() as i128
        } else {
            -(self.origin.duration_since(time).as_nanos() as i128)
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        if self.error.is_none() {
            self.error = self.output.write_all(bytes).err();
        }
    }

    /// Host start/end bracket measured work; timeline/deadline belong to the
    /// pause-adjusted scheduler and must not be interpreted as CPU timestamps.
    pub fn span(
        &mut self,
        kind: &str,
        timeline: Instant,
        scheduled: Option<Instant>,
        host: (Instant, Instant),
        state: Option<&Snapshot>,
        detail: &str,
    ) {
        self.sequence += 1;
        let mut fields = vec![
            self.sequence.to_string(),
            self.game.map_or_else(String::new, |game| game.to_string()),
            kind.into(),
            state.map_or_else(String::new, |s| s.body.to_string()),
            state.map_or_else(String::new, |s| s.render_pass.to_string()),
            scheduled.map_or_else(String::new, |time| self.relative(time).to_string()),
            self.relative(timeline).to_string(),
            self.relative(host.0).to_string(),
            self.relative(host.1).to_string(),
            self.period.as_nanos().to_string(),
        ];
        if let Some(s) = state {
            fields.extend([
                s.script_index.to_string(),
                s.random[0].to_string(),
                s.random[1].to_string(),
                s.random[2].to_string(),
                s.background_layer.to_string(),
                u8::from(s.alternate_background).to_string(),
                s.terrain_selector.to_string(),
                s.primary_count.to_string(),
                s.difficulty.to_string(),
                s.enemies
                    .iter()
                    .filter(|enemy| enemy.active)
                    .count()
                    .to_string(),
                s.special
                    .iter()
                    .filter(|special| special.active)
                    .count()
                    .to_string(),
            ]);
        } else {
            fields.extend(std::iter::repeat_n(String::new(), 11));
        }
        fields.push(detail.replace(['\t', '\r', '\n'], " "));
        self.write(format!("{}\n", fields.join("\t")).as_bytes());
    }

    pub fn flush(&mut self) -> Result<(), String> {
        if let Some(error) = self.error.take() {
            return Err(format!("cannot write timing trace: {error}"));
        }
        self.output
            .flush()
            .map_err(|error| format!("cannot flush timing trace: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spans_keep_scheduler_and_host_clocks_separate_and_preserve_session_ids() {
        let path = std::env::temp_dir().join(format!(
            "mach3-timing-clock-{}-{}.tsv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let origin = Instant::now();
        let mut ticks = [
            origin + Duration::from_millis(105),
            origin + Duration::from_millis(107),
        ]
        .into_iter();
        let mut trace =
            TimingTrace::create_with_clock(&path, Duration::from_millis(82), origin, move || {
                ticks.next().unwrap()
            })
            .unwrap();
        assert!(TimingTrace::create(&path, Duration::from_millis(82)).is_err());
        trace.begin_game();
        let host = (trace.now(), trace.now());
        trace.span(
            "pass",
            origin + Duration::from_millis(90),
            Some(origin + Duration::from_millis(80)),
            host,
            None,
            "clock\ttest",
        );
        trace.begin_game();
        trace.span(
            "present",
            origin + Duration::from_millis(90),
            None,
            host,
            None,
            "",
        );
        trace.flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<Vec<_>> = text
            .lines()
            .skip(2)
            .map(|line| line.split('\t').collect())
            .collect();
        assert_eq!(rows[0][1], "0");
        assert_eq!(rows[1][1], "1");
        assert_eq!(
            &rows[0][5..9],
            &["80000000", "90000000", "105000000", "107000000"]
        );
        assert_eq!(rows[1][5], "");
        assert_eq!(rows[0][21], "clock test");
        std::fs::remove_file(path).unwrap();
    }
}
