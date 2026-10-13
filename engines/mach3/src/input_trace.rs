// Copyright (c) 2026 analogued. Project license has not been selected.
//! Development-only observation of host events and actual engine input polls.

use crate::gameplay::{InputPoll, SessionState};
use reforge_sdl::{EventClockSample, HostPause, event_clock_sample, sdl3::event::Event};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

pub(crate) struct InputTrace {
    origin: Instant,
    output: BufWriter<File>,
    sequence: u64,
    error: Option<std::io::Error>,
}

#[derive(Clone, Copy)]
pub(crate) struct PollSite {
    pub label: &'static str,
    pub timeline: Instant,
    pub deadline: Option<Instant>,
    pub body: u16,
    pub pass: u8,
    pub controls: bool,
}

impl InputTrace {
    pub fn create(path: &Path) -> Result<Self, String> {
        Self::create_at(path, Instant::now())
    }

    pub(crate) fn create_at(path: &Path, origin: Instant) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("cannot create input trace {}: {error}", path.display()))?;
        let mut trace = Self {
            origin,
            output: BufWriter::new(file),
            sequence: 0,
            error: None,
        };
        trace.write(b"# reforge-mach3-input-v1\nsequence\tkind\tlabel\thost_ns\ttimeline_ns\tsdl_ns\tclock_span_ns\tevent_sdl_ns\tdeadline_ns\tactions\tfunctions\tresult_mask\tdetail\n");
        trace.flush()?;
        Ok(trace)
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

    pub fn flush(&mut self) -> Result<(), String> {
        if let Some(error) = self.error.take() {
            return Err(format!("cannot write input trace: {error}"));
        }
        self.output
            .flush()
            .map_err(|error| format!("cannot flush input trace: {error}"))
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        kind: &str,
        label: &str,
        clock: EventClockSample,
        timeline: Instant,
        event: Option<u64>,
        deadline: Option<Instant>,
        session: &SessionState,
        result: Option<u8>,
        detail: &str,
    ) {
        let held = session.keyboard.held_keys(0);
        let functions = u8::from(held.f1) | (u8::from(held.f2) << 1) | (u8::from(held.f3) << 2);
        self.sequence += 1;
        let row = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.sequence,
            kind,
            label,
            self.relative(clock.host),
            self.relative(timeline),
            clock.sdl_ns,
            clock.span.as_nanos(),
            event.map_or_else(String::new, |value| value.to_string()),
            deadline.map_or_else(String::new, |value| self.relative(value).to_string()),
            held.actions,
            functions,
            result.map_or_else(String::new, |value| value.to_string()),
            detail.replace(['\t', '\r', '\n'], " "),
        );
        self.write(row.as_bytes());
    }

    /// Record after receipt. Held columns show the currently consumed DOS table;
    /// a `queued` event becomes effective at a later poll or boundary fold.
    pub fn event(
        &mut self,
        event: &Event,
        disposition: &str,
        pause: &HostPause,
        session: &SessionState,
    ) {
        self.event_with_input_time(event, disposition, pause, session, None);
    }

    pub fn event_with_input_time(
        &mut self,
        event: &Event,
        disposition: &str,
        pause: &HostPause,
        session: &SessionState,
        effective: Option<Instant>,
    ) {
        let (label, mut detail) = match event {
            Event::KeyDown {
                keycode,
                scancode,
                keymod,
                repeat,
                window_id,
                which,
                raw,
                ..
            } => (
                "down",
                format!(
                    "key={keycode:?} scan={scancode:?} mods={} repeat={repeat} window={window_id} device={which} raw={raw} disposition={disposition} native={:?}",
                    keymod.bits(),
                    session.pause.state
                ),
            ),
            Event::KeyUp {
                keycode,
                scancode,
                keymod,
                repeat,
                window_id,
                which,
                raw,
                ..
            } => (
                "up",
                format!(
                    "key={keycode:?} scan={scancode:?} mods={} repeat={repeat} window={window_id} device={which} raw={raw} disposition={disposition} native={:?}",
                    keymod.bits(),
                    session.pause.state
                ),
            ),
            Event::Window { win_event, .. } => (
                "window",
                format!(
                    "event={win_event:?} disposition={disposition} native={:?}",
                    session.pause.state
                ),
            ),
            Event::Quit { .. } => ("quit", disposition.to_owned()),
            _ => return,
        };
        if let Some(at) = effective {
            detail.push_str(&format!(" effective_input_ns={}", self.relative(at)));
        }
        let clock = event_clock_sample();
        self.row(
            "event",
            label,
            clock,
            pause.now(clock.host),
            Some(event.get_timestamp()),
            None,
            session,
            None,
            &detail,
        );
    }

    pub fn poll(&mut self, site: PollSite, session: &SessionState, result: InputPoll) {
        self.row("poll", site.label, event_clock_sample(), site.timeline, None, site.deadline, session, Some(result.mask),
            &format!("body={} pass={} controls={} native={:?} sound_counter={} sound_toggled={} display_toggled={} sound_disabled={} display={:?}",
                site.body, site.pass, site.controls, session.pause.state, session.controls.sound_poll_counter,
                result.sound_toggled, result.display_toggled, session.controls.preferences.sound_disabled, session.controls.preferences.display_mode));
    }

    pub fn boundary(
        &mut self,
        label: &str,
        timeline: Instant,
        deadline: Option<Instant>,
        session: &SessionState,
        detail: &str,
    ) {
        self.row(
            "boundary",
            label,
            event_clock_sample(),
            timeline,
            None,
            deadline,
            session,
            None,
            &format!("native={:?} {detail}", session.pause.state),
        );
    }

    pub fn rebase(
        &mut self,
        label: &str,
        timeline: Instant,
        before: Option<Instant>,
        after: Option<Instant>,
        session: &SessionState,
    ) {
        self.boundary(
            label,
            timeline,
            after,
            session,
            &format!(
                "old_deadline_ns={}",
                before.map_or_else(String::new, |value| self.relative(value).to_string())
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_refuses_to_overwrite_existing_evidence() {
        let path =
            std::env::temp_dir().join(format!("reforge-trace-existing-{}", std::process::id()));
        let mut existing = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        existing.write_all(b"existing evidence").unwrap();
        assert!(InputTrace::create(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing evidence");
        std::fs::remove_file(path).unwrap();
    }
}
