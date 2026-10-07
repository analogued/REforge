// Copyright (c) 2026 analogued. Project license has not been selected.
//! Replay recorded poll results without consulting reference pixels or runtime dumps.

mod diagnostics;

use crate::{
    Error,
    assets::GraphicsAssets,
    frame::Frame,
    opening::{Controls, DisplayMode, GameStart, HeldKeys, Opening, ScriptRules, Steering},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordedGame {
    pub game: u64,
    pub script: u16,
    pub random: [u16; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordedInput {
    pub seq: u64,
    pub game: u64,
    pub body: u16,
    pub pass: u8,
    pub mask: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordedControl {
    game: u64,
    body: u16,
    site: String,
    keys: HeldKeys,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidData(message.into())
}

fn columns<'a>(line: &'a str, count: usize, table: &str) -> Result<Vec<&'a str>, Error> {
    let columns: Vec<_> = line.split('\t').collect();
    if columns.len() != count {
        return Err(invalid(format!("{table}: expected {count} TSV columns")));
    }
    Ok(columns)
}

fn number<T: std::str::FromStr>(value: &str, table: &str) -> Result<T, Error> {
    value
        .parse()
        .map_err(|_| invalid(format!("{table}: invalid integer {value:?}")))
}

pub fn read_games(reader: impl BufRead) -> Result<Vec<RecordedGame>, Error> {
    let mut lines = reader.lines();
    if lines.next().transpose()?.as_deref() != Some("game\tscript\trng_a\trng_b\trng_c") {
        return Err(invalid("games.tsv: invalid header"));
    }
    let mut games = Vec::new();
    for line in lines {
        let line = line?;
        let fields = columns(&line, 5, "games.tsv")?;
        let game = RecordedGame {
            game: number(fields[0], "games.tsv")?,
            script: number(fields[1], "games.tsv")?,
            random: [
                number(fields[2], "games.tsv")?,
                number(fields[3], "games.tsv")?,
                number(fields[4], "games.tsv")?,
            ],
        };
        if game.game != games.len() as u64 || game.script >= 21 {
            return Err(invalid(
                "games.tsv: games must start at zero and be contiguous; script must be 0..20",
            ));
        }
        games.push(game);
    }
    if games.is_empty() {
        return Err(invalid("games.tsv: no game start recorded"));
    }
    Ok(games)
}

/// The input sequence is wider than the original wrapping body word.
pub struct InputReader<R> {
    lines: std::io::Lines<R>,
    previous: Option<RecordedInput>,
}

impl<R: BufRead> InputReader<R> {
    pub fn new(reader: R) -> Result<Self, Error> {
        let mut lines = reader.lines();
        if lines.next().transpose()?.as_deref() != Some("seq\tgame\tbody\tpass\tmask") {
            return Err(invalid("inputs.tsv: invalid header"));
        }
        Ok(Self {
            lines,
            previous: None,
        })
    }

    pub fn read_next(&mut self) -> Result<Option<RecordedInput>, Error> {
        let Some(line) = self.lines.next() else {
            return Ok(None);
        };
        let line = line?;
        let fields = columns(&line, 5, "inputs.tsv")?;
        let row = RecordedInput {
            seq: number(fields[0], "inputs.tsv")?,
            game: number(fields[1], "inputs.tsv")?,
            body: number(fields[2], "inputs.tsv")?,
            pass: number(fields[3], "inputs.tsv")?,
            mask: number(fields[4], "inputs.tsv")?,
        };
        self.validate(row)?;
        self.previous = Some(row);
        Ok(Some(row))
    }

    fn validate(&self, row: RecordedInput) -> Result<(), Error> {
        if row.mask > 31 || !(1..=3).contains(&row.pass) {
            return Err(invalid(
                "inputs.tsv: pass must be 1..3 and mask must be 0..31",
            ));
        }
        let expected = match self.previous {
            None => (0, 0, 1, 1),
            Some(previous) if row.game == previous.game => (
                previous
                    .seq
                    .checked_add(1)
                    .ok_or_else(|| invalid("input sequence overflow"))?,
                previous.game,
                previous.body.wrapping_add(u16::from(previous.pass == 3)),
                if previous.pass == 3 {
                    1
                } else {
                    previous.pass + 1
                },
            ),
            Some(previous) => {
                if previous.pass != 3 {
                    return Err(invalid("inputs.tsv: game changed before the third pass"));
                }
                (
                    previous
                        .seq
                        .checked_add(1)
                        .ok_or_else(|| invalid("input sequence overflow"))?,
                    previous
                        .game
                        .checked_add(1)
                        .ok_or_else(|| invalid("game sequence overflow"))?,
                    1,
                    1,
                )
            }
        };
        if (row.seq, row.game, row.body, row.pass) != expected {
            return Err(invalid(format!(
                "inputs.tsv: unexpected poll {:?}; expected seq/game/body/pass {expected:?}",
                (row.seq, row.game, row.body, row.pass)
            )));
        }
        Ok(())
    }
}

fn read_controls(reader: impl BufRead) -> Result<Vec<RecordedControl>, Error> {
    let mut lines = reader.lines();
    if lines.next().transpose()?.as_deref() != Some("seq\tgame\tbody\tsite\tf1\tf2\tf3") {
        return Err(invalid("controls.tsv: invalid header"));
    }
    let mut controls = Vec::new();
    for (seq, line) in lines.enumerate() {
        let line = line?;
        let fields = columns(&line, 7, "controls.tsv")?;
        if number::<u64>(fields[0], "controls.tsv")? != seq as u64 {
            return Err(invalid(
                "controls.tsv: control sequence must start at zero and be contiguous",
            ));
        }
        if !matches!(
            fields[3],
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
            return Err(invalid(format!(
                "controls.tsv: unsupported control site {:?} at sequence {seq}",
                fields[3]
            )));
        }
        let flag = |value| -> Result<bool, Error> {
            match number::<u8>(value, "controls.tsv")? {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(invalid(
                    "controls.tsv: function key flags must be zero or one",
                )),
            }
        };
        let control = RecordedControl {
            game: number(fields[1], "controls.tsv")?,
            body: number(fields[2], "controls.tsv")?,
            site: fields[3].into(),
            keys: HeldKeys {
                f1: flag(fields[4])?,
                f2: flag(fields[5])?,
                f3: flag(fields[6])?,
                ..HeldKeys::default()
            },
        };
        // Older profiles did not classify disabled playback's tail-call. A
        // neutral function-key sample has no control side effects, regardless
        // of its caller. Never infer a non-neutral unknown poll's semantics.
        if control.site == "other" && control.keys != HeldKeys::default() {
            return Err(invalid(format!(
                "controls.tsv: unclassified function-key poll at sequence {seq}; record again with the updated profile"
            )));
        }
        controls.push(control);
    }
    Ok(controls)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CaptureBoundary {
    frames: u64,
    polls: u64,
}

/// Reference metadata only identifies when recording stopped. Neither reference
/// pixels nor presentation settings enter the native renderer.
fn read_boundary(reader: impl BufRead) -> Result<CaptureBoundary, Error> {
    let mut lines = reader.lines();
    if lines.next().transpose()?.as_deref() != Some("seq\tgame\tbody\tpass\tkind\tpalette\tmode") {
        return Err(invalid("frames.tsv: invalid header"));
    }
    let mut boundary = CaptureBoundary::default();
    for line in lines {
        let line = line?;
        let fields = columns(&line, 7, "frames.tsv")?;
        if number::<u64>(fields[1], "frames.tsv")? != 0 || fields[4] == "ending" {
            break;
        }
        if number::<u64>(fields[0], "frames.tsv")? != boundary.frames {
            return Err(invalid(
                "frames.tsv: first-game frame sequence is not contiguous",
            ));
        }
        if !matches!(fields[4], "initial" | "gameplay" | "transition") {
            return Err(invalid("frames.tsv: unsupported first-game frame kind"));
        }
        boundary.frames += 1;
        boundary.polls += u64::from(fields[4] == "gameplay");
    }
    if boundary.frames == 0 {
        return Err(invalid("frames.tsv: no first-game frames recorded"));
    }
    Ok(boundary)
}

fn read_end(reader: impl BufRead) -> Result<Option<u16>, Error> {
    let mut lines = reader.lines();
    if lines.next().transpose()?.as_deref() != Some("game\tbody\tquit") {
        return Err(invalid("ends.tsv: invalid header"));
    }
    let Some(line) = lines.next().transpose()? else {
        return Ok(None);
    };
    let fields = columns(&line, 3, "ends.tsv")?;
    if number::<u64>(fields[0], "ends.tsv")? != 0 {
        return Err(invalid("ends.tsv: first ending must belong to game zero"));
    }
    let body = number(fields[1], "ends.tsv")?;
    let _: u16 = number(fields[2], "ends.tsv")?;
    Ok(Some(body))
}

fn new_file(path: &Path) -> Result<BufWriter<File>, Error> {
    Ok(BufWriter::new(
        OpenOptions::new().write(true).create_new(true).open(path)?,
    ))
}

struct FrameWriter {
    table: BufWriter<File>,
    pixels: BufWriter<File>,
    seq: u64,
    limit: u64,
    diagnostics: diagnostics::Diagnostics,
}

impl FrameWriter {
    fn new(destination: &Path, limit: u64) -> Result<Self, Error> {
        fs::create_dir(destination)?;
        let mut table = new_file(&destination.join("frames.tsv"))?;
        writeln!(table, "seq\tgame\tbody\tpass\tkind\tpalette\tmode")?;
        Ok(Self {
            table,
            pixels: new_file(&destination.join("frames.bin"))?,
            seq: 0,
            limit,
            diagnostics: diagnostics::Diagnostics::new(destination)?,
        })
    }

    fn frame(
        &mut self,
        body: u16,
        pass: u8,
        kind: &str,
        palette: u8,
        mode: u8,
        capture: (&Frame, Option<&crate::opening::Snapshot>),
    ) -> Result<(), Error> {
        if self.seq >= self.limit {
            return Ok(());
        }
        self.pixels.write_all(capture.0.packed())?;
        if let Some(state) = capture.1 {
            self.diagnostics.frame(self.seq, body, pass, kind, state)?;
        }
        writeln!(
            self.table,
            "{}\t0\t{body}\t{pass}\t{kind}\t{palette}\t{mode}",
            self.seq
        )?;
        self.seq = self
            .seq
            .checked_add(1)
            .ok_or_else(|| invalid("frame sequence overflow"))?;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.pixels.flush()?;
        self.table.flush()?;
        self.diagnostics.flush()?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ReplayReport {
    pub frames: u64,
    pub inputs: u64,
    /// A partial replay remains available for inspecting the first divergence.
    pub diagnostic: Option<String>,
}

/// Replay the first game's recorded steering, including incomplete final bodies.
/// Only game-start metadata and sampled inputs affect the native simulation.
/// Later games and ending displays require a separate presentation timeline.
pub fn replay_first_game(
    assets: &GraphicsAssets,
    rules: ScriptRules,
    recording: &Path,
    destination: &Path,
) -> Result<ReplayReport, Error> {
    let games = read_games(BufReader::new(File::open(recording.join("games.tsv"))?))?;
    let controls_path = recording.join("controls.tsv");
    let controls = if controls_path.exists() {
        read_controls(BufReader::new(File::open(controls_path)?))?
    } else {
        Vec::new()
    };
    let mut inputs = InputReader::new(BufReader::new(File::open(recording.join("inputs.tsv"))?))?;
    let boundary = read_boundary(BufReader::new(File::open(recording.join("frames.tsv"))?))?;
    let end_path = recording.join("ends.tsv");
    let recorded_end = if end_path.exists() {
        read_end(BufReader::new(File::open(end_path)?))?
    } else {
        None
    };
    let first = games[0];
    let mut opening = Opening::for_play(
        assets,
        rules,
        GameStart {
            script_index: first.script,
            random: first.random,
            retained_world: None,
        },
    )?;
    let mut writer = FrameWriter::new(destination, boundary.frames)?;
    writer.frame(
        0,
        0,
        "initial",
        opening.palette_family(),
        4,
        (opening.frame(), Some(&opening.snapshot())),
    )?;
    let mut consumed = 0;
    let mut diagnostic = None;
    let mut control_cursor = 0;
    let mut native_controls = Controls::default();
    // A window can close during a transition drawn before its ordinary pass.
    // Such a tail has one sampled input whose gameplay frame was never displayed.
    while consumed < boundary.polls || (consumed == boundary.polls && writer.seq < boundary.frames)
    {
        let Some(input) = inputs.read_next()? else {
            diagnostic = Some(format!(
                "recording has fewer input polls than its {} displayed gameplay frames",
                boundary.polls
            ));
            break;
        };
        if input.game != 0 {
            diagnostic = Some(
                "first-game replay complete; subsequent games and ending displays are not replayed"
                    .into(),
            );
            break;
        }
        if opening.complete() {
            diagnostic = Some(format!(
                "native game ended before recorded input {} (body {}, pass {})",
                input.seq, input.body, input.pass
            ));
            break;
        }
        let target_site = format!("player{}", input.pass);
        let mut quit = false;
        if !controls.is_empty() {
            loop {
                let control = controls
                    .get(control_cursor)
                    .ok_or_else(|| invalid("controls.tsv: missing player poll"))?;
                let body_tail = matches!(control.site.as_str(), "audio-bypass" | "other")
                    && input.pass == 1
                    && control.body == input.body.wrapping_sub(1);
                if control.game != input.game || (control.body != input.body && !body_tail) {
                    return Err(invalid(format!(
                        "controls.tsv: poll does not match input {}",
                        input.seq
                    )));
                }
                let poll = native_controls.poll(control.keys);
                quit |= control.site == "pause-confirm" && poll.mask == 128;
                control_cursor += 1;
                if control.site == target_site {
                    break;
                }
                if control.site.starts_with("player") {
                    return Err(invalid("controls.tsv: unexpected player poll order"));
                }
            }
        }
        let palette = opening.palette_family();
        let mode = match native_controls.preferences.display_mode {
            DisplayMode::Color => 4,
            DisplayMode::Monochrome => 6,
        };
        if let Err(error) = opening.advance_subframe(assets, steering(input.mask)) {
            diagnostic = Some(format!(
                "native replay failed at input {} (body {}, pass {}): {error}",
                input.seq, input.body, input.pass
            ));
            break;
        }
        if quit {
            opening.request_game_over();
        }
        let transitions = opening.take_transitions();
        for (index, transition) in transitions
            .iter()
            .enumerate()
            .filter(|(_, frame)| frame.before_pass)
        {
            writer.frame(
                input.body,
                (index % 7 + 1) as u8,
                "transition",
                transition.palette_family,
                mode,
                (&transition.frame, transition.state.as_deref()),
            )?;
        }
        writer.frame(
            input.body,
            input.pass,
            "gameplay",
            palette,
            mode,
            (opening.pass_frame(), opening.pass_snapshot()),
        )?;
        for (index, transition) in transitions
            .iter()
            .enumerate()
            .filter(|(_, frame)| !frame.before_pass)
        {
            writer.frame(
                input.body,
                (index % 7 + 1) as u8,
                "transition",
                transition.palette_family,
                mode,
                (&transition.frame, transition.state.as_deref()),
            )?;
        }
        consumed += 1;
    }
    if diagnostic.is_none() {
        if let Some(trailing) = inputs.read_next()? {
            if trailing.game == 0 {
                if let Some(next) = inputs.read_next()? {
                    if next.game == 0 {
                        diagnostic = Some(
                            "recording contains more than one undisplayed trailing input poll"
                                .into(),
                        );
                    }
                }
            }
        }
        if writer.seq < boundary.frames {
            diagnostic = Some(format!(
                "native replay generated {} frames before the captured stop boundary of {}",
                writer.seq, boundary.frames
            ));
        }
    }
    if diagnostic.is_none() {
        if let Some(body) = recorded_end {
            if !opening.complete() || opening.snapshot().body != body {
                diagnostic = Some(format!(
                    "original game ended at body {body}; native game has not ended at that boundary"
                ));
            }
        }
    }
    writer.flush()?;
    let mut status = new_file(&destination.join("replay.tsv"))?;
    writeln!(status, "frames\tinputs\tdiagnostic")?;
    writeln!(
        status,
        "{}\t{}\t{}",
        writer.seq,
        consumed,
        diagnostic.as_deref().unwrap_or("")
    )?;
    status.flush()?;
    Ok(ReplayReport {
        frames: writer.seq,
        inputs: consumed,
        diagnostic,
    })
}

fn steering(mask: u8) -> Steering {
    Steering {
        up: mask & 1 != 0,
        down: mask & 2 != 0,
        left: mask & 4 != 0,
        right: mask & 8 != 0,
        fire: mask & 16 != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn synthetic_assets() -> GraphicsAssets {
        GraphicsAssets {
            primary: vec![0; 28_960],
            alternate: vec![0; 16_000],
            sprites: (0..190)
                .map(|index| crate::assets::Sprite {
                    width_bytes: 14,
                    height: 18,
                    descriptor: if index == 13 {
                        [0, 0, 15, 0, 0, 0, 0]
                    } else {
                        [0; 7]
                    },
                    color: vec![index as u8; 14 * 18],
                    mask: vec![255; 14 * 18],
                })
                .collect(),
        }
    }

    fn synthetic_rules() -> ScriptRules {
        let mut data = vec![0; 65536];
        data[0x04ac..0x04af].copy_from_slice(&[1, 2, 3]);
        for index in 0..21 {
            data[0x0a3e + 50 * index] = 0x85;
            data[0x0a3f + 50 * index] = 0xcf;
        }
        ScriptRules::from_data_segment(&data).unwrap()
    }

    #[test]
    fn replay_uses_native_pixels_and_display_controls_and_accepts_unrendered_tail() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "mach3-replay-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let recording = directory.join("recording");
        fs::create_dir(&recording).unwrap();
        fs::write(
            recording.join("games.tsv"),
            "game\tscript\trng_a\trng_b\trng_c\n0\t0\t3\t7\t9\n",
        )
        .unwrap();
        fs::write(
            recording.join("inputs.tsv"),
            "seq\tgame\tbody\tpass\tmask\n0\t0\t1\t1\t16\n1\t0\t1\t2\t5\n2\t0\t1\t3\t8\n",
        )
        .unwrap();
        // Deliberately bogus reference palette and mode must not be copied.
        fs::write(recording.join("frames.tsv"), "seq\tgame\tbody\tpass\tkind\tpalette\tmode\n0\t0\t0\t0\tinitial\t99\t255\n1\t0\t1\t1\tgameplay\t99\t255\n2\t0\t1\t2\tgameplay\t99\t255\n").unwrap();
        // A directory in place of reference pixels proves they are never opened.
        fs::create_dir(recording.join("frames.bin")).unwrap();
        for evidence in [
            "state.tsv",
            "registers.tsv",
            "objects.tsv",
            "draw-order.tsv",
            "data-segment.bin",
        ] {
            fs::create_dir(recording.join(evidence)).unwrap();
        }
        fs::write(recording.join("controls.tsv"), "seq\tgame\tbody\tsite\tf1\tf2\tf3\n0\t0\t1\tbody\t0\t0\t1\n1\t0\t1\tplayer1\t0\t0\t0\n2\t0\t1\tplayer2\t0\t0\t0\n3\t0\t1\tplayer3\t0\t0\t0\n").unwrap();
        let assets = synthetic_assets();
        let output = directory.join("replayed");
        let report = replay_first_game(&assets, synthetic_rules(), &recording, &output).unwrap();
        assert_eq!((report.frames, report.inputs), (3, 2));
        assert!(report.diagnostic.is_none());
        let mut native = Opening::for_play(
            &assets,
            synthetic_rules(),
            GameStart {
                script_index: 0,
                random: [3, 7, 9],
                retained_world: None,
            },
        )
        .unwrap();
        let mut expected = native.frame().packed().to_vec();
        for mask in [16, 5] {
            native.advance_subframe(&assets, steering(mask)).unwrap();
            expected.extend_from_slice(native.pass_frame().packed());
        }
        assert_eq!(fs::read(output.join("frames.bin")).unwrap(), expected);
        let state = fs::read_to_string(output.join("state.tsv")).unwrap();
        assert_eq!(state.lines().count(), 4);
        let objects = fs::read_to_string(output.join("objects-normalized.tsv")).unwrap();
        assert_eq!(objects.lines().count(), 1 + 3 * 29);
        let order = fs::read_to_string(output.join("draw-order.tsv")).unwrap();
        assert_eq!(order.lines().count(), 1 + 3 * 29);
        let metadata = fs::read_to_string(output.join("frames.tsv")).unwrap();
        assert!(metadata.contains("1\t0\t1\t1\tgameplay\t1\t6\n"));
        assert!(!metadata.contains("255"));
        // Matching displayed pixels must not hide a missed game-over condition.
        fs::write(recording.join("ends.tsv"), "game\tbody\tquit\n0\t1\t0\n").unwrap();
        let missed_end = replay_first_game(
            &assets,
            synthetic_rules(),
            &recording,
            &directory.join("missed-ending"),
        )
        .unwrap();
        assert!(
            missed_end
                .diagnostic
                .unwrap()
                .contains("native game has not ended")
        );
        fs::remove_file(recording.join("ends.tsv")).unwrap();
        fs::write(recording.join("inputs.tsv"), "seq\tgame\tbody\tpass\tmask\n0\t0\t1\t1\t0\n1\t0\t1\t2\t0\n2\t0\t1\t3\t0\n3\t0\t2\t1\t0\n").unwrap();
        fs::write(recording.join("frames.tsv"), "seq\tgame\tbody\tpass\tkind\tpalette\tmode\n0\t0\t0\t0\tinitial\t1\t4\n1\t0\t1\t1\tgameplay\t1\t4\n2\t0\t1\t2\tgameplay\t1\t4\n3\t0\t1\t3\tgameplay\t1\t4\n4\t0\t2\t1\tgameplay\t1\t4\n").unwrap();
        for (site, f3, mode) in [("other", 0, 4), ("audio-bypass", 1, 6)] {
            fs::write(recording.join("controls.tsv"), format!("seq\tgame\tbody\tsite\tf1\tf2\tf3\n0\t0\t1\tbody\t0\t0\t0\n1\t0\t1\tplayer1\t0\t0\t0\n2\t0\t1\tplayer2\t0\t0\t0\n3\t0\t1\tplayer3\t0\t0\t0\n4\t0\t1\t{site}\t0\t0\t{f3}\n5\t0\t2\tbody\t0\t0\t0\n6\t0\t2\tplayer1\t0\t0\t0\n")).unwrap();
            let output = directory.join(site);
            let report =
                replay_first_game(&assets, synthetic_rules(), &recording, &output).unwrap();
            assert_eq!((report.frames, report.inputs), (5, 4));
            assert!(report.diagnostic.is_none());
            let frames = fs::read_to_string(output.join("frames.tsv")).unwrap();
            assert!(frames.contains("3\t0\t1\t3\tgameplay\t1\t4\n"));
            assert!(frames.contains(&format!("4\t0\t2\t1\tgameplay\t1\t{mode}\n")));
        }
        fs::write(recording.join("controls.tsv"), "seq\tgame\tbody\tsite\tf1\tf2\tf3\n0\t0\t1\tbody\t1\t0\t0\n1\t0\t1\tpause-confirm\t1\t0\t0\n2\t0\t1\tplayer1\t0\t0\t0\n3\t0\t1\tplayer2\t0\t0\t0\n4\t0\t1\tplayer3\t0\t0\t0\n5\t0\t2\tplayer1\t0\t0\t0\n").unwrap();
        let early_output = directory.join("early-ending");
        let early =
            replay_first_game(&assets, synthetic_rules(), &recording, &early_output).unwrap();
        assert_eq!((early.frames, early.inputs), (4, 3));
        assert!(
            early
                .diagnostic
                .as_deref()
                .unwrap()
                .contains("native game ended before recorded input 3")
        );
        assert_eq!(
            fs::metadata(early_output.join("frames.bin")).unwrap().len(),
            4 * crate::frame::FRAME_BYTES as u64
        );
        assert!(
            fs::read_to_string(early_output.join("replay.tsv"))
                .unwrap()
                .contains("native game ended")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn window_close_during_pre_pass_transition_replays_its_sampled_input() {
        let directory = std::env::temp_dir().join(format!(
            "mach3-transition-replay-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let recording = directory.join("recording");
        fs::create_dir(&recording).unwrap();
        let mut assets = synthetic_assets();
        for sprite in &mut assets.sprites {
            sprite.descriptor[2] = 15;
        }
        let mut data = synthetic_rules().data_segment().to_vec();
        data[0x0a3e..0x0a70].fill(0xc8);
        data[0x0a3e] = 0x46;
        data[0x0a6f] = 0xcf;
        let rules = ScriptRules::from_data_segment(&data).unwrap();
        let start = GameStart {
            script_index: 0,
            random: [3, 7, 9],
            retained_world: None,
        };
        let mut game = Opening::for_play(&assets, rules.clone(), start).unwrap();
        let mut index =
            String::from("seq\tgame\tbody\tpass\tkind\tpalette\tmode\n0\t0\t0\t0\tinitial\t1\t4\n");
        let mut inputs = String::from("seq\tgame\tbody\tpass\tmask\n");
        let mut expected = game.frame().packed().to_vec();
        let mut frames = 1;
        let mut found_transition = false;
        let mut polls = 0;
        for seq in 0..900 {
            game.advance_subframe(&assets, steering(4)).unwrap();
            let snapshot = game.snapshot();
            inputs.push_str(&format!(
                "{seq}\t0\t{}\t{}\t4\n",
                snapshot.body, snapshot.render_pass
            ));
            polls += 1;
            let transitions = game.take_transitions();
            if transitions.iter().any(|frame| frame.before_pass) {
                // The capture ends after three expanding draws, before this
                // input's ordinary framebuffer write has become visible.
                for (step, transition) in transitions.iter().take(3).enumerate() {
                    index.push_str(&format!(
                        "{frames}\t0\t{}\t{}\ttransition\t1\t4\n",
                        snapshot.body,
                        step + 1
                    ));
                    expected.extend_from_slice(transition.frame.packed());
                    frames += 1;
                }
                found_transition = true;
                break;
            }
            assert!(transitions.is_empty());
            index.push_str(&format!(
                "{frames}\t0\t{}\t{}\tgameplay\t1\t4\n",
                snapshot.body, snapshot.render_pass
            ));
            expected.extend_from_slice(game.pass_frame().packed());
            frames += 1;
        }
        assert!(found_transition, "synthetic goal must be crossed");
        fs::write(
            recording.join("games.tsv"),
            "game\tscript\trng_a\trng_b\trng_c\n0\t0\t3\t7\t9\n",
        )
        .unwrap();
        fs::write(recording.join("inputs.tsv"), inputs).unwrap();
        fs::write(recording.join("frames.tsv"), index).unwrap();
        let output = directory.join("replayed");
        let report = replay_first_game(&assets, rules, &recording, &output).unwrap();
        assert!(report.diagnostic.is_none(), "{:?}", report.diagnostic);
        assert_eq!((report.frames, report.inputs), (frames, polls));
        assert_eq!(fs::read(output.join("frames.bin")).unwrap(), expected);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_gaps_and_accepts_a_partial_last_body() {
        let mut input = InputReader::new(Cursor::new(
            "seq\tgame\tbody\tpass\tmask\n0\t0\t1\t1\t16\n1\t0\t1\t2\t5\n",
        ))
        .unwrap();
        assert_eq!(input.read_next().unwrap().unwrap().mask, 16);
        assert_eq!(input.read_next().unwrap().unwrap().pass, 2);
        assert!(input.read_next().unwrap().is_none());
        let mut input = InputReader::new(Cursor::new(
            "seq\tgame\tbody\tpass\tmask\n0\t0\t1\t1\t0\n2\t0\t1\t2\t0\n",
        ))
        .unwrap();
        input.read_next().unwrap();
        assert!(input.read_next().is_err());
    }

    #[test]
    fn counter_wrap_retains_poll_order() {
        let mut input =
            InputReader::new(Cursor::new("seq\tgame\tbody\tpass\tmask\n8\t0\t0\t1\t31\n")).unwrap();
        input.previous = Some(RecordedInput {
            seq: 7,
            game: 0,
            body: u16::MAX,
            pass: 3,
            mask: 0,
        });
        let row = input.read_next().unwrap().unwrap();
        assert_eq!(row.body, 0);
        assert_eq!(steering(row.mask).bits(), 31);
    }

    #[test]
    fn game_change_requires_a_finished_body() {
        let mut input =
            InputReader::new(Cursor::new("seq\tgame\tbody\tpass\tmask\n2\t1\t1\t1\t0\n")).unwrap();
        input.previous = Some(RecordedInput {
            seq: 1,
            game: 0,
            body: 4,
            pass: 2,
            mask: 0,
        });
        assert!(input.read_next().is_err());
    }

    #[test]
    fn validates_external_game_start_and_pause_decision() {
        let games = read_games(Cursor::new(
            "game\tscript\trng_a\trng_b\trng_c\n0\t3\t65535\t2\t7\n",
        ))
        .unwrap();
        assert_eq!(games[0].random, [65535, 2, 7]);
        assert!(
            read_games(Cursor::new(
                "game\tscript\trng_a\trng_b\trng_c\n0\t21\t0\t0\t0\n"
            ))
            .is_err()
        );
        let controls = read_controls(Cursor::new("seq\tgame\tbody\tsite\tf1\tf2\tf3\n0\t0\t7\tbody\t1\t0\t0\n1\t0\t7\tpause-confirm\t1\t0\t0\n")).unwrap();
        assert_eq!(controls[0].site, "body");
        assert_eq!(Controls::default().poll(controls[1].keys).mask, 128);
        for (site, keys) in [("other", "0\t0\t1"), ("unknown", "0\t0\t0")] {
            assert!(
                read_controls(Cursor::new(format!(
                    "seq\tgame\tbody\tsite\tf1\tf2\tf3\n0\t0\t7\t{site}\t{keys}\n"
                )))
                .is_err()
            );
        }
    }
}
