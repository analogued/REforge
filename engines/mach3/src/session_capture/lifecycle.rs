// Copyright (c) 2026 analogued. Project license has not been selected.
//! Optional ordered input and presentation timeline for consecutive games.

use super::*;
use crate::{
    opening::{Ending, EndingStage, EndingTitle, SessionState, Snapshot},
    startup::StartupFrames,
};
use std::collections::VecDeque;

const HEADER: &str = "seq\tgame\tbody\tpass\tevent\tsite\tmask\tf1\tf2\tf3\tlast_scan\tframe_seq";

#[derive(Debug)]
struct Event {
    seq: u64,
    game: u64,
    body: u16,
    pass: u16,
    event: String,
    site: String,
    mask: u16,
    keys: HeldKeys,
    last_scan: u8,
    frame_seq: u64,
}

fn read_timeline(reader: impl BufRead) -> Result<Vec<Event>, Error> {
    let mut lines = reader.lines();
    if lines.next().transpose()?.as_deref() != Some(HEADER) {
        return Err(invalid("timeline.tsv: invalid header"));
    }
    let mut events = Vec::new();
    let mut frame_seq = 0;
    for (seq, line) in lines.enumerate() {
        let line = line?;
        let f = columns(&line, 12, "timeline.tsv")?;
        let flag = |value| match number::<u8>(value, "timeline.tsv")? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid("timeline.tsv: flags must be zero or one")),
        };
        let event = Event {
            seq: number(f[0], "timeline.tsv")?,
            game: number(f[1], "timeline.tsv")?,
            body: number(f[2], "timeline.tsv")?,
            pass: number(f[3], "timeline.tsv")?,
            event: f[4].into(),
            site: f[5].into(),
            mask: number(f[6], "timeline.tsv")?,
            keys: HeldKeys {
                f1: flag(f[7])?,
                f2: flag(f[8])?,
                f3: flag(f[9])?,
                ..HeldKeys::default()
            },
            last_scan: number(f[10], "timeline.tsv")?,
            frame_seq: number(f[11], "timeline.tsv")?,
        };
        if event.seq != seq as u64 || event.frame_seq != frame_seq {
            return Err(invalid("timeline.tsv: noncontiguous event/frame cursor"));
        }
        if !matches!(
            event.event.as_str(),
            "frame"
                | "input"
                | "control"
                | "lifecycle-poll"
                | "audio-clear"
                | "game-end"
                | "intro-entry"
                | "game-reset"
                | "game-start"
        ) {
            return Err(invalid(format!(
                "timeline.tsv: unknown event {:?}",
                event.event
            )));
        }
        frame_seq += u64::from(event.event == "frame");
        events.push(event);
    }
    Ok(events)
}

#[derive(Debug)]
struct FrameBoundary {
    game: u64,
    body: u16,
    pass: u16,
    kind: String,
}

fn read_frames(reader: impl BufRead) -> Result<Vec<FrameBoundary>, Error> {
    let mut lines = reader.lines();
    if lines.next().transpose()?.as_deref() != Some("seq\tgame\tbody\tpass\tkind\tpalette\tmode") {
        return Err(invalid("frames.tsv: invalid header"));
    }
    let mut frames = Vec::new();
    for (seq, line) in lines.enumerate() {
        let line = line?;
        let f = columns(&line, 7, "frames.tsv")?;
        if number::<u64>(f[0], "frames.tsv")? != seq as u64 {
            return Err(invalid("frames.tsv: noncontiguous frame sequence"));
        }
        frames.push(FrameBoundary {
            game: number(f[1], "frames.tsv")?,
            body: number(f[2], "frames.tsv")?,
            pass: number(f[3], "frames.tsv")?,
            kind: f[4].into(),
        });
    }
    Ok(frames)
}

struct PendingFrame {
    body: u16,
    pass: u16,
    kind: &'static str,
    palette: u8,
    frame: Frame,
    state: Option<Snapshot>,
}

fn ending_site(ending: &Ending) -> &'static str {
    let s = ending.snapshot();
    match (s.title, s.stage) {
        (EndingTitle::GameOver, EndingStage::Approach) => "ending-go-in",
        (EndingTitle::GameOver, EndingStage::Departure) => "ending-go-out",
        (EndingTitle::HighScore, EndingStage::Approach) => "ending-hs-in",
        (EndingTitle::HighScore, EndingStage::ScoreOverlay) => "ending-hs-overlay",
        (EndingTitle::HighScore, EndingStage::Departure) => "ending-hs-out",
        (EndingTitle::GameOver, EndingStage::Hold) => "ending-go-hold",
        (EndingTitle::HighScore, EndingStage::Hold) => "ending-hs-hold",
        _ => "ending-pause",
    }
}

/// Replay the optional lifecycle timeline, preserving the RNG, world and controls
/// across games. The first game's start is the sole captured simulation seed;
/// every later games.tsv row is an assertion against SessionState::begin_game.
/// Returned intro pixels come exclusively from externally loaded resources.
/// Legacy recordings without a timeline retain the first-game replay interface.
pub fn replay_session(
    assets: &GraphicsAssets,
    rules: ScriptRules,
    startup: Option<&StartupFrames>,
    recording: &Path,
    destination: &Path,
) -> Result<ReplayReport, Error> {
    let path = recording.join("timeline.tsv");
    if !path.exists() {
        let mut report = replay_first_game(assets, rules, recording, destination)?;
        let frames = read_frames(BufReader::new(File::open(recording.join("frames.tsv"))?))?;
        if report.diagnostic.is_none() && frames.len() as u64 != report.frames {
            report.diagnostic = Some(
                "recording includes ending or later frames without a lifecycle timeline".into(),
            );
            write_report(destination, &report)?;
        }
        return Ok(report);
    }
    let events = read_timeline(BufReader::new(File::open(path)?))?;
    let boundaries = read_frames(BufReader::new(File::open(recording.join("frames.tsv"))?))?;
    let games = read_games(BufReader::new(File::open(recording.join("games.tsv"))?))?;
    let mut inputs = InputReader::new(BufReader::new(File::open(recording.join("inputs.tsv"))?))?;
    let first = games[0];
    let mut session = SessionState {
        random: first.random,
        ..SessionState::default()
    };
    let mut opening = Opening::for_play(
        assets,
        rules.clone(),
        GameStart {
            script_index: first.script,
            random: first.random,
            retained_world: None,
        },
    )?;
    let mut writer = FrameWriter::new(destination, boundaries.len() as u64)?;
    let mut replay = Lifecycle {
        assets,
        rules,
        startup,
        session: &mut session,
        opening: &mut opening,
        writer: &mut writer,
        games: &games,
        boundaries: &boundaries,
        inputs: &mut inputs,
        game: 0,
        consumed: 0,
        pending: VecDeque::new(),
        ending: None,
        ending_ready: false,
        ending_draws: 0,
        hold_poll_seen: false,
        returned_intro: None,
        intro_input: false,
        quit: false,
        reset: false,
        intro_entered: false,
        started: false,
        initial_pending: false,
        palette_reset: false,
    };
    let mut diagnostic = initial_preferences(recording)?;
    for event in &events {
        if diagnostic.is_some() {
            break;
        }
        if let Err(error) = replay.event(event) {
            diagnostic = Some(format!(
                "timeline event {} ({} {}): {error}",
                event.seq, event.event, event.site
            ));
            break;
        }
    }
    let consumed = replay.consumed;
    drop(replay);
    if diagnostic.is_none() && writer.seq != boundaries.len() as u64 {
        diagnostic = Some(format!(
            "timeline ended after {} of {} recorded frames",
            writer.seq,
            boundaries.len()
        ));
    }
    writer.flush()?;
    let report = ReplayReport {
        frames: writer.seq,
        inputs: consumed,
        diagnostic,
    };
    write_report(destination, &report)?;
    Ok(report)
}

fn initial_preferences(recording: &Path) -> Result<Option<String>, Error> {
    let path = recording.join("session-state.tsv");
    if !path.exists() {
        return Ok(None);
    }
    let mut rows = BufReader::new(File::open(path)?).lines();
    if rows.next().transpose()?.as_deref()
        != Some(
            "seq\tgame\tbody\tpass\tkind\trng_a\trng_b\trng_c\tbest_score\tdisplay_mode\tsound_disabled\tsound_poll_counter\tintro_audio_enabled",
        )
    {
        return Err(invalid("session-state.tsv: invalid header"));
    }
    let Some(row) = rows.next().transpose()? else {
        return Ok(None);
    };
    let fields = columns(&row, 13, "session-state.tsv")?;
    let actual: Vec<u64> = fields[8..]
        .iter()
        .map(|value| number(value, "session-state.tsv"))
        .collect::<Result<_, _>>()?;
    if actual != [0, 4, 0, 0, 1] {
        return Ok(Some("unsupported nondefault first-game best score or preferences; startup control history was not captured".into()));
    }
    Ok(None)
}

fn write_report(destination: &Path, report: &ReplayReport) -> Result<(), Error> {
    // The destination was newly created by FrameWriter; this also updates the
    // legacy report when missing timeline coverage is detected.
    let mut status = BufWriter::new(File::create(destination.join("replay.tsv"))?);
    writeln!(status, "frames\tinputs\tdiagnostic")?;
    writeln!(
        status,
        "{}\t{}\t{}",
        report.frames,
        report.inputs,
        report.diagnostic.as_deref().unwrap_or("")
    )?;
    status.flush()?;
    Ok(())
}

struct Lifecycle<'a, R> {
    assets: &'a GraphicsAssets,
    rules: ScriptRules,
    startup: Option<&'a StartupFrames>,
    session: &'a mut SessionState,
    opening: &'a mut Opening,
    writer: &'a mut FrameWriter,
    games: &'a [RecordedGame],
    boundaries: &'a [FrameBoundary],
    inputs: &'a mut InputReader<R>,
    game: u64,
    consumed: u64,
    pending: VecDeque<PendingFrame>,
    ending: Option<Ending>,
    ending_ready: bool,
    ending_draws: u16,
    hold_poll_seen: bool,
    returned_intro: Option<bool>,
    intro_input: bool,
    quit: bool,
    reset: bool,
    intro_entered: bool,
    started: bool,
    initial_pending: bool,
    palette_reset: bool,
}

impl<R: BufRead> Lifecycle<'_, R> {
    fn event(&mut self, e: &Event) -> Result<(), Error> {
        if e.game != self.game && !(e.event == "game-reset" && e.game == self.game + 1) {
            return Err(invalid(format!(
                "unexpected game {}, native game {}",
                e.game, self.game
            )));
        }
        match e.event.as_str() {
            "control" => {
                if !matches!(
                    e.site.as_str(),
                    "body"
                        | "player1"
                        | "player2"
                        | "player3"
                        | "pause"
                        | "pause269f"
                        | "pause26b8"
                        | "pause-confirm"
                        | "pause26e2"
                        | "audio-bypass"
                        | "other"
                        | "primer"
                        | "intro"
                        | "ending-go-release"
                        | "ending-go-in"
                        | "ending-go-hold"
                        | "ending-go-out"
                        | "ending-hs-release"
                        | "ending-hs-in"
                        | "ending-hs-hold"
                        | "ending-hs-out"
                ) {
                    return Err(invalid("unclassified control poll site"));
                }
                if e.site == "primer" && e.keys != HeldKeys::default() {
                    return Err(invalid(
                        "nonneutral initializer CS:03d7 input before INITIAL044a is not modeled",
                    ));
                }
                if e.site == "other" && e.keys != HeldKeys::default() {
                    return Err(invalid("unclassified nonneutral control poll"));
                }
                let poll = self.session.controls.poll(e.keys);
                if poll.display_toggled {
                    if self.pending.iter().any(|frame| frame.kind == "transition") {
                        return Err(invalid(
                            "F3 interleaved with queued transition draws is not modeled; later script palette writes require phased replay",
                        ));
                    }
                    self.opening.reset_video_palette();
                    self.palette_reset = true;
                }
                if e.site == "pause-confirm" && poll.mask == 128 {
                    self.quit = true;
                }
            }
            "audio-clear" => self.session.finish_audio_input(e.last_scan),
            "input" if e.site == "primer" => {
                if e.mask != 0 {
                    return Err(invalid(
                        "nonneutral initializer CS:03d7 poll before INITIAL044a is not modeled",
                    ));
                }
            }
            "input" => self.input(e)?,
            "frame" => self.frame(e)?,
            "game-end" => {
                if self.ending.is_some()
                    || !self.pending.is_empty()
                    || !self.opening.complete()
                    || self.opening.snapshot().body != e.body
                {
                    return Err(invalid(
                        "captured game end does not match native completed body",
                    ));
                }
                self.ending = Some(Ending::new_with_best(
                    self.assets,
                    &self.opening.snapshot(),
                    self.opening.frame(),
                    self.session.best_score,
                )?);
                self.ending_ready = true;
                self.ending_draws = 0;
            }
            "lifecycle-poll" => self.lifecycle_poll(e)?,
            "intro-entry" => {
                if self.returned_intro.is_none() {
                    return Err(invalid("intro return lacks a legal native title input"));
                }
                self.intro_entered = true;
            }
            "game-reset" => {
                if self.reset
                    || self.returned_intro.is_none()
                    || !self.intro_entered
                    || !self.intro_input
                {
                    return Err(invalid(
                        "game reset lacks a returned intro and nonzero intro input",
                    ));
                }
                let start = self.session.begin_game();
                *self.opening = Opening::for_play(self.assets, self.rules.clone(), start)?;
                self.game += 1;
                self.reset = true;
                self.started = false;
                self.quit = false;
                self.pending.clear();
            }
            "game-start" => {
                if self.started || (self.game != 0 && !self.reset) {
                    return Err(invalid("game-start arrived without a new-game reset"));
                }
                let captured = self
                    .games
                    .get(self.game as usize)
                    .ok_or_else(|| invalid("missing game start assertion"))?;
                let native = self.opening.snapshot();
                if native.script_index != captured.script
                    || native.random != captured.random
                    || native.body != 0
                {
                    return Err(invalid(format!(
                        "derived game {} script/RNG mismatch: native {} {:?}, recorded {} {:?}",
                        self.game,
                        native.script_index,
                        native.random,
                        captured.script,
                        captured.random
                    )));
                }
                self.reset = false;
                self.started = true;
                self.initial_pending = true;
                self.intro_entered = false;
                self.ending = None;
                self.returned_intro = None;
                self.intro_input = false;
            }
            _ => return Err(invalid("unsupported timeline event")),
        }
        Ok(())
    }

    fn input(&mut self, e: &Event) -> Result<(), Error> {
        if !self.started || self.initial_pending || self.ending.is_some() {
            return Err(invalid("player input outside initialized gameplay"));
        }
        if !self.pending.is_empty() {
            return Err(invalid(
                "player poll arrived before pending frames were displayed",
            ));
        }
        let input = self
            .inputs
            .read_next()?
            .ok_or_else(|| invalid("missing inputs.tsv row"))?;
        if (
            input.game,
            input.body,
            u16::from(input.pass),
            u16::from(input.mask),
        ) != (e.game, e.body, e.pass, e.mask & 31)
            || e.site != format!("player{}", input.pass)
        {
            return Err(invalid("input timeline does not match inputs.tsv poll"));
        }
        if self.opening.complete() {
            return Err(invalid("native game ended before captured player poll"));
        }
        let palette = self.opening.palette_family();
        let responding = self.opening.snapshot().player.phase != 0;
        let diagnostic_state = |state: &Snapshot| {
            let mut state = state.clone();
            if !responding {
                // The helper returns F1's high bit too; it affects diagnostic
                // input state even though only the low five bits steer motion.
                state.player.effective_input |= e.mask & !31;
            }
            state
        };
        self.palette_reset = false;
        self.opening
            .advance_subframe(self.assets, steering(input.mask))?;
        if self.quit {
            self.opening.request_game_over();
            self.quit = false;
        }
        let transitions = self.opening.take_transitions();
        for (index, t) in transitions
            .iter()
            .enumerate()
            .filter(|(_, t)| t.before_pass)
        {
            self.pending.push_back(PendingFrame {
                body: e.body,
                pass: (index % 7 + 1) as u16,
                kind: "transition",
                palette: t.palette_family,
                frame: t.frame.clone(),
                state: t.state.as_deref().map(diagnostic_state),
            });
        }
        self.pending.push_back(PendingFrame {
            body: e.body,
            pass: e.pass,
            kind: "gameplay",
            palette,
            frame: self.opening.pass_frame().clone(),
            state: self.opening.pass_snapshot().map(diagnostic_state),
        });
        for (index, t) in transitions
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.before_pass)
        {
            self.pending.push_back(PendingFrame {
                body: e.body,
                pass: (index % 7 + 1) as u16,
                kind: "transition",
                palette: t.palette_family,
                frame: t.frame.clone(),
                state: t.state.as_deref().map(diagnostic_state),
            });
        }
        self.consumed += 1;
        Ok(())
    }

    fn lifecycle_poll(&mut self, e: &Event) -> Result<(), Error> {
        if matches!(e.site.as_str(), "intro" | "intro-playback")
            || (e.site == "audio-bypass" && self.returned_intro.is_some())
        {
            if !self.intro_entered || self.returned_intro.is_none() || self.reset {
                return Err(invalid("intro result outside the returned intro"));
            }
            self.intro_input |= e.mask != 0;
            return Ok(());
        }
        if e.site == "primer" {
            if e.mask != 0 {
                return Err(invalid(
                    "nonneutral initializer CS:03d7 poll is not modeled",
                ));
            }
            return Ok(());
        }
        if matches!(
            e.site.as_str(),
            "body"
                | "player1"
                | "player2"
                | "player3"
                | "pause"
                | "pause269f"
                | "pause26b8"
                | "pause-confirm"
                | "pause26e2"
                | "audio-bypass"
                | "other"
        ) {
            return Ok(());
        }
        if matches!(e.site.as_str(), "ending-go-release" | "ending-hs-release") {
            if self.ending.is_none() || self.returned_intro.is_some() || self.reset {
                return Err(invalid("title release outside native ending"));
            }
            return Ok(());
        }
        let ending = self
            .ending
            .as_mut()
            .ok_or_else(|| invalid("title input before game end"))?;
        if self.ending_ready {
            return Err(invalid(
                "title input arrived before preceding draw was captured",
            ));
        }
        let hold = e.site.ends_with("-hold");
        if !ending.has_prepared_draw()
            && (!hold || self.hold_poll_seen || ending_site(ending) != e.site)
        {
            ending.prepare_next_step()?;
        }
        // Mandatory holds have no polls; compress those nondrawing timer waits.
        for _ in 0..300 {
            if ending_site(ending) == e.site {
                break;
            }
            if ending.has_prepared_draw() {
                return Err(invalid(
                    "title poll would skip an unrecorded framebuffer draw",
                ));
            }
            ending.prepare_next_step()?;
        }
        if ending_site(ending) != e.site || !ending.can_return() {
            return Err(invalid("title input at an unsupported or mandatory stage"));
        }
        self.hold_poll_seen = hold;
        if e.mask != 0 {
            let choice = ending
                .returned_intro_choice()
                .ok_or_else(|| invalid("illegal title return"))?;
            self.session
                .remember_ending(&ending.snapshot(), ending.special_snapshots());
            self.returned_intro = Some(choice);
        }
        Ok(())
    }

    fn frame(&mut self, e: &Event) -> Result<(), Error> {
        let expected = self
            .boundaries
            .get(self.writer.seq as usize)
            .ok_or_else(|| invalid("timeline has more frames than frames.tsv"))?;
        let kind = if e.site.starts_with("ending-") {
            "ending"
        } else {
            e.site.as_str()
        };
        if (
            expected.game,
            expected.body,
            expected.pass,
            expected.kind.as_str(),
        ) != (e.game, e.body, e.pass, kind)
        {
            return Err(invalid("timeline frame identity does not match frames.tsv"));
        }
        self.writer.game = self.game;
        let mode = match self.session.controls.preferences.display_mode {
            DisplayMode::Color => 4,
            DisplayMode::Monochrome => 6,
        };
        let prefix = format!(
            "{}\t{}\t{}\t{}\t{kind}",
            self.writer.seq, self.game, e.body, e.pass
        );
        let mut random = self.opening.snapshot().random;
        match kind {
            "initial" => {
                if !self.initial_pending || e.body != 0 || e.pass != 0 {
                    return Err(invalid(
                        "initial draw outside the native INITIAL044a boundary",
                    ));
                }
                self.writer.frame(
                    e.body,
                    e.pass,
                    kind,
                    self.opening.palette_family(),
                    mode,
                    (self.opening.frame(), Some(&self.opening.snapshot())),
                )?;
                self.initial_pending = false;
            }
            "gameplay" | "transition" => {
                let next = self
                    .pending
                    .pop_front()
                    .ok_or_else(|| invalid("recorded draw lacks native sampled input"))?;
                if (next.body, next.pass, next.kind) != (e.body, e.pass, kind) {
                    return Err(invalid("native draw order differs from recorded frame"));
                }
                if let Some(state) = &next.state {
                    random = state.random;
                }
                self.writer.frame(
                    e.body,
                    e.pass,
                    kind,
                    if self.palette_reset { 1 } else { next.palette },
                    mode,
                    (&next.frame, next.state.as_ref()),
                )?;
            }
            "ending" => {
                let ending = self
                    .ending
                    .as_mut()
                    .ok_or_else(|| invalid("ending draw before native game end"))?;
                if self.returned_intro.is_some() {
                    return Err(invalid("ending draw after return to intro"));
                }
                if !self.ending_ready {
                    for _ in 0..300 {
                        if ending.has_prepared_draw() {
                            break;
                        }
                        ending.prepare_next_step()?;
                    }
                    if !ending.has_prepared_draw() {
                        return Err(invalid("native ending did not produce next draw"));
                    }
                    ending.finish_prepared_step(self.assets)?;
                }
                if ending_site(ending) != e.site {
                    return Err(invalid(format!(
                        "native ending draw {} differs from {}",
                        ending_site(ending),
                        e.site
                    )));
                }
                if e.pass != self.ending_draws {
                    return Err(invalid("ending draw ordinal differs"));
                }
                self.ending_draws = self
                    .ending_draws
                    .checked_add(1)
                    .ok_or_else(|| invalid("ending draw ordinal overflow"))?;
                self.writer.frame(
                    e.body,
                    e.pass,
                    kind,
                    self.opening.palette_family(),
                    mode,
                    (ending.frame(), None),
                )?;
                self.writer.diagnostics.ending(
                    &prefix,
                    &ending.snapshot(),
                    ending.special_snapshots(),
                )?;
                random = ending.snapshot().random;
                self.session.best_score = ending.snapshot().best_score;
                self.ending_ready = false;
                self.hold_poll_seen = false;
            }
            "intro" => {
                if !self.intro_entered {
                    return Err(invalid("intro draw before intro-entry"));
                }
                let choice = self
                    .returned_intro
                    .ok_or_else(|| invalid("intro draw before legal title return"))?;
                let startup = self.startup.ok_or_else(|| {
                    invalid("returned intro requires externally loaded DATA1/DATA6 StartupFrames")
                })?;
                self.writer.frame(
                    e.body,
                    e.pass,
                    kind,
                    1,
                    mode,
                    (startup.return_intro(choice), None),
                )?;
                random = self.session.random;
                if let Some(ending) = &self.ending {
                    self.writer.diagnostics.ending(
                        &prefix,
                        &ending.snapshot(),
                        ending.special_snapshots(),
                    )?;
                }
            }
            _ => return Err(invalid("unsupported recorded frame kind")),
        }
        self.writer
            .diagnostics
            .session(&prefix, random, self.session)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct Fixture {
        timeline: String,
        frames: String,
        event_seq: u64,
        frame_seq: u64,
    }
    impl Fixture {
        fn new() -> Self {
            Self {
                timeline: format!("{HEADER}\n"),
                frames: "seq\tgame\tbody\tpass\tkind\tpalette\tmode\n".into(),
                event_seq: 0,
                frame_seq: 0,
            }
        }
        fn event(
            &mut self,
            game: u64,
            body: u16,
            pass: u16,
            event: &str,
            site: &str,
            effects: [u16; 5],
        ) {
            let [mask, f1, f2, f3, last_scan] = effects;
            self.timeline.push_str(&format!("{}\t{game}\t{body}\t{pass}\t{event}\t{site}\t{mask}\t{f1}\t{f2}\t{f3}\t{last_scan}\t{}\n", self.event_seq, self.frame_seq));
            self.event_seq += 1;
            if event == "frame" {
                let kind = if site.starts_with("ending-") {
                    "ending"
                } else {
                    site
                };
                // Bogus reference palette/mode must never become native state.
                self.frames.push_str(&format!(
                    "{}\t{game}\t{body}\t{pass}\t{kind}\t99\t255\n",
                    self.frame_seq
                ));
                self.frame_seq += 1;
            }
        }
    }

    fn startup(directory: &Path) -> StartupFrames {
        fs::create_dir(directory).unwrap();
        for (name, bytes) in [
            ("DATA1", vec![0x55; 0x4000]),
            ("DATA6", vec![0xaa; 0x4000]),
            ("DATA2", vec![0; 28_960]),
            ("DATA3", vec![0; 16_000]),
        ] {
            fs::write(directory.join(name), bytes).unwrap();
        }
        let mut pixels = vec![0; 0x200];
        for index in 0..190 {
            let pointer = u16::try_from(0x2000 + pixels.len() - 0x200).unwrap();
            pixels[index * 2..index * 2 + 2].copy_from_slice(&pointer.to_le_bytes());
            pixels.extend_from_slice(&[0, 0, 0, 14, 18, 0, 0]);
            pixels.extend_from_slice(&[0; 14 * 18]);
        }
        fs::write(directory.join("DATA4"), &pixels).unwrap();
        fs::write(directory.join("DATA5"), pixels).unwrap();
        StartupFrames::load(directory).unwrap()
    }

    #[test]
    fn session_derives_restart_retains_unprojected_return_and_preferences() {
        let directory = std::env::temp_dir().join(format!(
            "mach3-session-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let assets = super::super::tests::synthetic_assets();
        let mut data = super::super::tests::synthetic_rules()
            .data_segment()
            .to_vec();
        data[0x0a3e..0x0a40].copy_from_slice(&[0xce, 0xcf]);
        let rules = ScriptRules::from_data_segment(&data).unwrap();
        let startup = startup(&directory.join("assets"));
        let start = GameStart {
            script_index: 0,
            random: [3, 7, 9],
            retained_world: None,
        };
        let mut native = Opening::for_play(&assets, rules.clone(), start).unwrap();
        native
            .advance_subframe(&assets, Steering::default())
            .unwrap();
        native.request_game_over();
        native
            .advance_subframe(&assets, Steering::default())
            .unwrap();
        native
            .advance_subframe(&assets, Steering::default())
            .unwrap();
        assert!(native.complete());
        let mut ending = Ending::new(&assets, &native.snapshot(), native.frame()).unwrap();
        let mut fixture = Fixture::new();
        fixture.event(0, 0, 0, "game-start", "initial", [0; 5]);
        fixture.event(0, 0, 0, "frame", "initial", [0; 5]);
        fixture.event(0, 1, 0, "control", "body", [0, 1, 0, 0, 0]);
        fixture.event(0, 1, 0, "control", "pause-confirm", [0, 1, 0, 0, 0]);
        for pass in 1..=3 {
            fixture.event(0, 1, pass, "input", &format!("player{pass}"), [0; 5]);
            fixture.event(0, 1, pass, "frame", "gameplay", [0; 5]);
        }
        fixture.event(0, 1, 0, "game-end", "game-end", [0; 5]);
        let mut draw = 0;
        fixture.event(0, 1, draw, "frame", ending_site(&ending), [0; 5]);
        draw += 1;
        while !ending.can_return() {
            ending.prepare_next_step().unwrap();
            if ending.has_prepared_draw() {
                ending.finish_prepared_step(&assets).unwrap();
                fixture.event(0, 1, draw, "frame", ending_site(&ending), [0; 5]);
                draw += 1;
            }
        }
        assert_eq!(draw, 140); // mandatory pair139 + first repeated draw
        let previous_rect = ending.snapshot().world[5].rect;
        ending.prepare_next_step().unwrap();
        assert_eq!(ending.snapshot().world[5].depth, 972);
        assert_eq!(
            (
                ending.snapshot().world[5].rect.x_bytes,
                ending.snapshot().world[5].rect.y
            ),
            (previous_rect.x_bytes, previous_rect.y)
        );
        let choice = ending.returned_intro_choice().unwrap();
        let mut session = SessionState::default();
        session.remember_ending(&ending.snapshot(), ending.special_snapshots());
        let second = session.begin_game();
        fixture.event(0, 1, 0, "control", "ending-go-in", [0; 5]);
        fixture.event(0, 1, 0, "lifecycle-poll", "ending-go-in", [16, 0, 0, 0, 0]);
        fixture.event(0, 1, 0, "intro-entry", "intro", [0; 5]);
        fixture.event(0, 1, 0, "frame", "intro", [0; 5]);
        for _ in 0..3 {
            fixture.event(0, 1, 0, "control", "intro", [0, 0, 1, 0, 0]);
        }
        fixture.event(0, 1, 0, "control", "intro", [0, 0, 0, 1, 0]);
        fixture.event(0, 1, 0, "lifecycle-poll", "intro", [0; 5]);
        fixture.event(0, 1, 0, "audio-clear", "audio", [0, 0, 0, 0, 0xbc]);
        fixture.event(
            0,
            1,
            0,
            "lifecycle-poll",
            "intro-playback",
            [16, 0, 0, 0, 0],
        );
        fixture.event(1, 0, 0, "game-reset", "reset", [0; 5]);
        fixture.event(1, 0, 0, "control", "primer", [0; 5]);
        fixture.event(1, 0, 0, "input", "primer", [0; 5]);
        fixture.event(1, 0, 0, "game-start", "initial", [0; 5]);
        fixture.event(1, 0, 0, "frame", "initial", [0; 5]);
        fixture.event(1, 1, 1, "input", "player1", [128, 0, 0, 0, 0]);
        fixture.event(1, 1, 1, "frame", "gameplay", [0; 5]);
        let recording = directory.join("recording");
        fs::create_dir(&recording).unwrap();
        fs::create_dir(recording.join("frames.bin")).unwrap();
        fs::create_dir(recording.join("data-segment.bin")).unwrap();
        let games = format!(
            "game\tscript\trng_a\trng_b\trng_c\n0\t0\t3\t7\t9\n1\t{}\t{}\t{}\t{}\n",
            second.script_index, second.random[0], second.random[1], second.random[2]
        );
        fs::write(recording.join("games.tsv"), &games).unwrap();
        fs::write(recording.join("inputs.tsv"), "seq\tgame\tbody\tpass\tmask\n0\t0\t1\t1\t0\n1\t0\t1\t2\t0\n2\t0\t1\t3\t0\n3\t1\t1\t1\t0\n").unwrap();
        fs::write(recording.join("frames.tsv"), &fixture.frames).unwrap();
        fs::write(recording.join("timeline.tsv"), &fixture.timeline).unwrap();
        let output = directory.join("output");
        let report =
            replay_session(&assets, rules.clone(), Some(&startup), &recording, &output).unwrap();
        assert!(report.diagnostic.is_none(), "{:?}", report.diagnostic);
        assert_eq!((report.frames, report.inputs), (147, 4));
        let metadata = fs::read_to_string(output.join("frames.tsv")).unwrap();
        assert!(metadata.contains("145\t1\t0\t0\tinitial\t1\t6\n"));
        assert!(metadata.contains("144\t0\t1\t0\tintro\t1\t4\n"));
        let state = fs::read_to_string(output.join("state.tsv")).unwrap();
        let headers: Vec<_> = state.lines().next().unwrap().split('\t').collect();
        let input_column = headers
            .iter()
            .position(|column| *column == "player_input")
            .unwrap();
        let final_row: Vec<_> = state.lines().last().unwrap().split('\t').collect();
        assert_eq!(final_row[input_column], "128");
        let controls = fs::read_to_string(output.join("session-state.tsv")).unwrap();
        assert!(controls.lines().nth(146).unwrap().ends_with("\t6\t1\t0\t1"));
        let records = fs::read_to_string(output.join("objects-normalized.tsv")).unwrap();
        assert!(records.contains("145\t1\t0\t0\tinitial\t5\t972\t"));
        let pixels = fs::read(output.join("frames.bin")).unwrap();
        let offset = 144 * crate::frame::FRAME_BYTES;
        assert_eq!(
            &pixels[offset..offset + crate::frame::FRAME_BYTES],
            startup.return_intro(choice).packed()
        );
        // Closing during a title wait, a prepared input step, returned intro,
        // or reset before INITIAL preserves exactly the displayed prefix.
        for (name, event_name, site, zero_return, first_game_only) in [
            ("title-prefix", "frame", "ending-go-in", false, false),
            (
                "prepared-prefix",
                "lifecycle-poll",
                "ending-go-in",
                true,
                false,
            ),
            ("intro-prefix", "frame", "intro", false, false),
            ("reset-prefix", "game-reset", "reset", false, true),
        ] {
            let mut rows: Vec<_> = fixture.timeline.lines().map(str::to_owned).collect();
            let stop = rows
                .iter()
                .position(|row| {
                    let columns: Vec<_> = row.split('\t').collect();
                    columns.len() == 12 && columns[4] == event_name && columns[5] == site
                })
                .unwrap();
            rows.truncate(stop + 1);
            if zero_return {
                let mut columns: Vec<_> = rows[stop].split('\t').map(str::to_owned).collect();
                columns[6] = "0".into();
                rows[stop] = columns.join("\t");
            }
            let count = rows
                .iter()
                .skip(1)
                .filter(|row| row.split('\t').nth(4) == Some("frame"))
                .count();
            fs::write(
                recording.join("timeline.tsv"),
                format!("{}\n", rows.join("\n")),
            )
            .unwrap();
            fs::write(
                recording.join("frames.tsv"),
                format!(
                    "{}\n",
                    fixture
                        .frames
                        .lines()
                        .take(count + 1)
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            )
            .unwrap();
            fs::write(
                recording.join("games.tsv"),
                if first_game_only {
                    "game\tscript\trng_a\trng_b\trng_c\n0\t0\t3\t7\t9\n"
                } else {
                    &games
                },
            )
            .unwrap();
            let partial = replay_session(
                &assets,
                rules.clone(),
                Some(&startup),
                &recording,
                &directory.join(name),
            )
            .unwrap();
            assert!(
                partial.diagnostic.is_none(),
                "{name}: {:?}",
                partial.diagnostic
            );
            assert_eq!(partial.frames as usize, count);
        }
        fs::write(recording.join("timeline.tsv"), &fixture.timeline).unwrap();
        fs::write(recording.join("frames.tsv"), &fixture.frames).unwrap();
        fs::write(recording.join("games.tsv"), &games).unwrap();
        fs::write(
            recording.join("timeline.tsv"),
            fixture
                .timeline
                .replace("\tinput\tprimer\t0\t", "\tinput\tprimer\t16\t"),
        )
        .unwrap();
        let primer = replay_session(
            &assets,
            rules.clone(),
            Some(&startup),
            &recording,
            &directory.join("primer"),
        )
        .unwrap();
        assert_eq!(primer.frames, 145);
        assert!(
            primer
                .diagnostic
                .unwrap()
                .contains("nonneutral initializer")
        );
        fs::write(recording.join("timeline.tsv"), &fixture.timeline).unwrap();
        let missing = replay_session(
            &assets,
            rules.clone(),
            None,
            &recording,
            &directory.join("missing-intro"),
        )
        .unwrap();
        assert_eq!(missing.frames, 144);
        assert!(missing.diagnostic.unwrap().contains("externally loaded"));
        // A captured later seed mismatch stops before initial/gameplay, rather
        // than injecting the altered seed to make frames agree.
        fs::write(
            recording.join("games.tsv"),
            games.replace(
                &format!("1\t{}\t", second.script_index),
                &format!("1\t{}\t", (second.script_index + 1) % 4),
            ),
        )
        .unwrap();
        let mismatch = replay_session(
            &assets,
            rules,
            Some(&startup),
            &recording,
            &directory.join("mismatch"),
        )
        .unwrap();
        assert!(
            mismatch
                .diagnostic
                .unwrap()
                .contains("derived game 1 script/RNG mismatch")
        );
        assert_eq!(mismatch.frames, 145);
        fs::write(recording.join("games.tsv"), &games).unwrap();
        fs::write(recording.join("session-state.tsv"), "seq\tgame\tbody\tpass\tkind\trng_a\trng_b\trng_c\tbest_score\tdisplay_mode\tsound_disabled\tsound_poll_counter\tintro_audio_enabled\n0\t0\t0\t0\tinitial\t3\t7\t9\t0\t6\t0\t0\t1\n").unwrap();
        let preferences = replay_session(
            &assets,
            super::super::tests::synthetic_rules(),
            Some(&startup),
            &recording,
            &directory.join("preferences"),
        )
        .unwrap();
        assert_eq!(preferences.frames, 0);
        assert!(
            preferences
                .diagnostic
                .unwrap()
                .contains("unsupported nondefault")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ending_inherits_script_palette_until_f3_resets_it() {
        let directory = std::env::temp_dir().join(format!(
            "mach3-ending-palette-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let recording = directory.join("recording");
        fs::create_dir(&recording).unwrap();
        let mut data = super::super::tests::synthetic_rules()
            .data_segment()
            .to_vec();
        data[0x0a3e..0x0a40].copy_from_slice(&[0xce, 0xcf]);
        let rules = ScriptRules::from_data_segment(&data).unwrap();
        let mut fixture = Fixture::new();
        fixture.event(0, 0, 0, "game-start", "initial", [0; 5]);
        fixture.event(0, 0, 0, "frame", "initial", [0; 5]);
        let mut inputs = String::from("seq\tgame\tbody\tpass\tmask\n");
        for body in 1..=22 {
            if body == 22 {
                fixture.event(0, body, 0, "control", "pause-confirm", [0, 1, 0, 0, 0]);
            }
            for pass in 1..=3 {
                inputs.push_str(&format!(
                    "{}\t0\t{body}\t{pass}\t0\n",
                    (body - 1) * 3 + pass - 1
                ));
                fixture.event(0, body, pass, "input", &format!("player{pass}"), [0; 5]);
                fixture.event(0, body, pass, "frame", "gameplay", [0; 5]);
            }
        }
        fixture.event(0, 22, 0, "game-end", "game-end", [0; 5]);
        fixture.event(0, 22, 0, "frame", "ending-go-in", [0; 5]);
        fixture.event(0, 22, 0, "control", "audio-bypass", [0, 0, 0, 1, 0]);
        fixture.event(0, 22, 1, "frame", "ending-go-in", [0; 5]);
        fs::write(
            recording.join("games.tsv"),
            "game\tscript\trng_a\trng_b\trng_c\n0\t0\t3\t7\t9\n",
        )
        .unwrap();
        fs::write(recording.join("inputs.tsv"), inputs).unwrap();
        fs::write(recording.join("frames.tsv"), fixture.frames).unwrap();
        fs::write(recording.join("timeline.tsv"), fixture.timeline).unwrap();
        let output = directory.join("output");
        let report = replay_session(
            &super::super::tests::synthetic_assets(),
            rules,
            None,
            &recording,
            &output,
        )
        .unwrap();
        assert!(report.diagnostic.is_none(), "{:?}", report.diagnostic);
        let metadata = fs::read_to_string(output.join("frames.tsv")).unwrap();
        assert!(metadata.contains("67\t0\t22\t0\tending\t0\t4\n"));
        assert!(metadata.contains("68\t0\t22\t1\tending\t1\t6\n"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn f3_during_transition_stops_before_ambiguous_palette_order() {
        let directory = std::env::temp_dir().join(format!(
            "mach3-transition-f3-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let recording = directory.join("recording");
        fs::create_dir(&recording).unwrap();
        let mut assets = super::super::tests::synthetic_assets();
        for sprite in &mut assets.sprites {
            sprite.descriptor[2] = 15;
        }
        let mut data = super::super::tests::synthetic_rules()
            .data_segment()
            .to_vec();
        data[0x0a3e..0x0a70].fill(0xc8);
        data[0x0a3e] = 0x46;
        data[0x0a3f] = 0xce;
        data[0x0a6f] = 0xcf;
        let rules = ScriptRules::from_data_segment(&data).unwrap();
        let mut native = Opening::for_play(
            &assets,
            rules.clone(),
            GameStart {
                script_index: 0,
                random: [3, 7, 9],
                retained_world: None,
            },
        )
        .unwrap();
        let mut fixture = Fixture::new();
        fixture.event(0, 0, 0, "game-start", "initial", [0; 5]);
        fixture.event(0, 0, 0, "frame", "initial", [0; 5]);
        let mut inputs = String::from("seq\tgame\tbody\tpass\tmask\n");
        let mut stop_frames = None;
        for seq in 0..900 {
            native.advance_subframe(&assets, steering(4)).unwrap();
            let state = native.snapshot();
            let pass = u16::from(state.render_pass);
            inputs.push_str(&format!("{seq}\t0\t{}\t{pass}\t4\n", state.body));
            fixture.event(
                0,
                state.body,
                pass,
                "input",
                &format!("player{pass}"),
                [4, 0, 0, 0, 0],
            );
            let transitions = native.take_transitions();
            if !transitions.is_empty() {
                assert!(transitions[0].before_pass);
                assert!(transitions.len() > 1);
                for (index, _) in transitions.iter().enumerate() {
                    fixture.event(
                        0,
                        state.body,
                        (index % 7 + 1) as u16,
                        "frame",
                        "transition",
                        [0; 5],
                    );
                    if index == 0 {
                        stop_frames = Some(fixture.frame_seq);
                        fixture.event(
                            0,
                            state.body,
                            pass,
                            "control",
                            "audio-bypass",
                            [0, 0, 0, 1, 0],
                        );
                    }
                }
                fixture.event(0, state.body, pass, "frame", "gameplay", [0; 5]);
                break;
            }
            fixture.event(0, state.body, pass, "frame", "gameplay", [0; 5]);
        }
        let stop_frames = stop_frames.expect("authored goal must produce a transition");
        fs::write(
            recording.join("games.tsv"),
            "game\tscript\trng_a\trng_b\trng_c\n0\t0\t3\t7\t9\n",
        )
        .unwrap();
        fs::write(recording.join("inputs.tsv"), inputs).unwrap();
        fs::write(recording.join("frames.tsv"), fixture.frames).unwrap();
        fs::write(recording.join("timeline.tsv"), fixture.timeline).unwrap();
        let output = directory.join("output");
        let report = replay_session(&assets, rules, None, &recording, &output).unwrap();
        assert_eq!(report.frames, stop_frames);
        assert!(
            report
                .diagnostic
                .unwrap()
                .contains("F3 interleaved with queued transition draws is not modeled")
        );
        assert_eq!(
            fs::metadata(output.join("frames.bin")).unwrap().len(),
            stop_frames * crate::frame::FRAME_BYTES as u64
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn timeline_rejects_gaps_and_preserves_a_partial_control_tail() {
        assert!(
            read_timeline(Cursor::new(format!(
                "{HEADER}\n1\t0\t0\t0\tcontrol\tprimer\t0\t0\t0\t0\t0\t0\n"
            )))
            .is_err()
        );
        let rows = read_timeline(Cursor::new(format!(
            "{HEADER}\n0\t0\t0\t0\tcontrol\tprimer\t0\t0\t0\t0\t0\t0\n"
        )))
        .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
