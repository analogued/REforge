// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use mach3::{
    Error,
    assets::GraphicsAssets,
    gameplay::{
        Ending, Gameplay, LAST_BODY, STEERING_LAST_BODY, ScriptRules, SessionState, Steering,
    },
    startup::StartupFrames,
};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

const HELP: &str = "Usage: mach3-opening [--steps 0..836] [--ending] [--inputs input.tsv] [--subframes] <assets-directory> <new-output-directory>\n\n\
Export the no-input opening from the initial frame through the requested body.\n\
Writes numbered CGA frames and state/object TSV tables; output directory must be new.\n\
Default: 836 bodies, including the complete first no-input run. Audio is deferred.\n\
--ending: also export the first title pair and selected return intro image.\n\
--inputs: held action TSV (body, update, mask); defaults to 90 bodies, explicit --steps permits up to 836.\n\
--subframes: also export the three displayed passes as NNN-1.cga through NNN-3.cga.";

fn new_file(path: impl AsRef<Path>) -> Result<BufWriter<std::fs::File>, Error> {
    Ok(BufWriter::new(
        OpenOptions::new().write(true).create_new(true).open(path)?,
    ))
}

fn run() -> Result<(), Error> {
    let mut args = env::args_os().skip(1).peekable();
    if args
        .peek()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!("{HELP}");
        return Ok(());
    }
    let mut steps = LAST_BODY;
    let mut steps_set = false;
    if args.peek().is_some_and(|arg| arg == "--steps") {
        args.next();
        steps_set = true;
        steps = args
            .next()
            .and_then(|arg| arg.to_str().and_then(|arg| arg.parse::<u16>().ok()))
            .filter(|steps| *steps <= LAST_BODY)
            .ok_or_else(|| {
                Error::InvalidData("--steps must be an integer between 0 and 836".into())
            })?;
    }
    let include_ending = args.peek().is_some_and(|arg| arg == "--ending");
    if include_ending {
        args.next();
    }
    if include_ending && steps != LAST_BODY {
        return Err(Error::InvalidData(
            "--ending requires the complete no-input run".into(),
        ));
    }
    let input_path = if args.peek().is_some_and(|arg| arg == "--inputs") {
        args.next();
        Some(
            args.next()
                .map(PathBuf::from)
                .ok_or_else(|| Error::InvalidData("--inputs requires a TSV file".into()))?,
        )
    } else {
        None
    };
    let mut input_changes = std::collections::BTreeMap::new();
    if let Some(path) = &input_path {
        if include_ending {
            return Err(Error::InvalidData("input export excludes --ending".into()));
        }
        if !steps_set {
            steps = STEERING_LAST_BODY;
        }
        let text = fs::read_to_string(path)?;
        let mut lines = text.lines();
        if lines.next() != Some("body\tupdate\tmask") {
            return Err(Error::InvalidData(
                "input header must be body<TAB>update<TAB>mask".into(),
            ));
        }
        let mut previous = None;
        for line in lines {
            let fields: Vec<_> = line.split('\t').collect();
            let row = fields
                .iter()
                .map(|s| s.parse::<u16>().ok())
                .collect::<Option<Vec<_>>>()
                .filter(|row| {
                    row.len() == 3
                        && (1..=LAST_BODY).contains(&row[0])
                        && (1..=3).contains(&row[1])
                        && row[2] <= 31
                })
                .ok_or_else(|| {
                    Error::InvalidData(
                        "input rows require body 1..836, update 1..3, mask 0..31".into(),
                    )
                })?;
            let key = (row[0], row[1]);
            if previous.is_some_and(|old| old >= key) {
                return Err(Error::InvalidData(
                    "input rows must be strictly ordered".into(),
                ));
            }
            previous = Some(key);
            input_changes.insert(
                key,
                Steering {
                    up: row[2] & 1 != 0,
                    down: row[2] & 2 != 0,
                    left: row[2] & 4 != 0,
                    right: row[2] & 8 != 0,
                    fire: row[2] & 16 != 0,
                },
            );
        }
    }
    let include_subframes = args.peek().is_some_and(|arg| arg == "--subframes");
    if include_subframes {
        args.next();
    }
    let directory = args.next().map(PathBuf::from);
    let destination = args.next().map(PathBuf::from);
    let (Some(directory), Some(destination), None) = (directory, destination, args.next()) else {
        return Err(Error::InvalidData(HELP.into()));
    };
    let assets = GraphicsAssets::load(&directory)?;
    let program = mach3::executable::load_program(&directory)?;
    let rules = ScriptRules::from_data_segment(&program[mach3::executable::DATA_SEGMENT_OFFSET..])?;
    let mut session = SessionState::default();
    session.select_intro();
    let mut gameplay = Gameplay::from_game_start(&assets, rules, session.begin_game(), steps)?;
    let mut held_input = Steering::default();
    fs::create_dir(&destination)?;
    let mut state = new_file(destination.join("state.tsv"))?;
    let mut terrain = new_file(destination.join("terrain.tsv"))?;
    let mut enemies = new_file(destination.join("enemies.tsv"))?;
    let mut weapon = new_file(destination.join("weapon.tsv"))?;
    writeln!(
        weapon,
        "body\tcounter\tslot\tdepth\tlateral\theight_world\tactive\trequested\tresolved\tx\ty\twidth\theight\tphase"
    )?;
    let mut player = new_file(destination.join("player.tsv"))?;
    let mut special = new_file(destination.join("special.tsv"))?;
    writeln!(
        special,
        "body\tslot\tdepth\tlateral\theight_world\textent_depth\textent_lateral\textent_height\tactive\trequested\tresolved\tx\ty\twidth\theight\tphase\tmovement\tscale"
    )?;
    let mut order = new_file(destination.join("order.tsv"))?;
    writeln!(order, "body\tposition\tslot")?;
    let mut full_order = new_file(destination.join("full-order.tsv"))?;
    writeln!(full_order, "body\tposition\tslot")?;
    writeln!(
        state,
        "body\tstrip\trng_a\trng_b\trng_c\tcursor\tdelay\tterrain_selector\tfuel\tscore\tmarker_requested\tmarker_previous\tmarker_resolved\tmarker_width\tmarker_height\ttarget_lateral\ttarget_height\tpending_score\tstrip_y\tvertical_origin\tlives_requested\tlives_resolved\tbar_width\tbar_draw_width\tprimary_count"
    )?;
    writeln!(
        player,
        "body\tdepth\tlateral\theight_world\trequested\tresolved\tx\ty\twidth\theight\tphase\tdamage\tanimation_row\tgrace\thorizon\tlives\tgame_over\tvelocity_lateral\tvelocity_height\torientation\tvertical_band\tinput\tupdate_velocity"
    )?;
    writeln!(
        enemies,
        "body\tslot\tdepth\tlateral\theight_world\tactive\trequested\tresolved\tx\ty\twidth\theight\tphase\tmovement\tvariant\tanimation_phase"
    )?;
    writeln!(
        terrain,
        "body\tslot\tdepth\tlateral\tactive\trequested\tresolved\tx\ty\twidth\theight\tphase\tkind\tmovement\tanimation_phase"
    )?;
    for body in 0..=steps {
        if body > 0 {
            let inputs = std::array::from_fn(|i| {
                if let Some(&change) = input_changes.get(&(body, i as u16 + 1)) {
                    held_input = change;
                }
                held_input
            });
            if include_subframes {
                for (pass, input) in inputs.into_iter().enumerate() {
                    gameplay.advance_subframe(&assets, input)?;
                    let mut output =
                        new_file(destination.join(format!("{body:03}-{}.cga", pass + 1)))?;
                    output.write_all(&gameplay.frame().to_cga())?;
                    output.flush()?;
                }
            } else {
                gameplay.advance_with_inputs(&assets, inputs)?;
            }
        }
        let snapshot = gameplay.snapshot();
        for shot in snapshot.weapon.shots {
            writeln!(
                weapon,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                body,
                snapshot.weapon.counter,
                shot.slot,
                shot.depth,
                shot.lateral,
                shot.height,
                u8::from(shot.active),
                shot.requested_sprite,
                shot.resolved_sprite,
                shot.rect.x_bytes,
                shot.rect.y,
                shot.rect.width_bytes,
                shot.rect.height,
                shot.phase
            )?;
        }
        for (position, slot) in snapshot.full_world_order.into_iter().enumerate() {
            writeln!(full_order, "{body}\t{position}\t{slot}")?;
        }
        for (position, slot) in snapshot.world_order.into_iter().enumerate() {
            writeln!(order, "{body}\t{position}\t{slot}")?;
        }
        for slot in snapshot.special {
            writeln!(
                special,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                body,
                slot.slot,
                slot.depth,
                slot.lateral,
                slot.height,
                slot.extents[0],
                slot.extents[1],
                slot.extents[2],
                u8::from(slot.active),
                slot.requested_sprite,
                slot.resolved_sprite,
                slot.rect.x_bytes,
                slot.rect.y,
                slot.rect.width_bytes,
                slot.rect.height,
                slot.phase,
                slot.movement,
                slot.scale
            )?;
        }
        let h = snapshot.hud;
        let [a, b, c] = snapshot.random;
        writeln!(
            state,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            body,
            snapshot.strip_index,
            a,
            b,
            c,
            snapshot.script_cursor,
            snapshot.script_delay,
            snapshot.terrain_selector,
            h.fuel,
            h.score,
            h.marker_requested,
            h.marker_previous,
            h.marker_resolved,
            h.marker_width,
            h.marker_height,
            snapshot.enemy_target[0],
            snapshot.enemy_target[1],
            h.pending_score,
            snapshot.strip_y,
            snapshot.vertical_origin,
            h.lives_requested,
            h.lives_resolved,
            h.left_bar_width,
            h.left_bar_draw_width,
            snapshot.primary_count
        )?;
        let p = snapshot.player;
        writeln!(
            player,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            body,
            p.coordinates[0],
            p.coordinates[1],
            p.coordinates[2],
            p.requested_sprite,
            p.resolved_sprite,
            p.rect.x_bytes,
            p.rect.y,
            p.rect.width_bytes,
            p.rect.height,
            p.phase,
            p.damage,
            p.animation_row,
            p.grace,
            p.horizon,
            p.lives,
            u8::from(p.game_over),
            p.velocity[0],
            p.velocity[1],
            p.orientation,
            p.vertical_band,
            p.effective_input,
            p.update_lateral_velocity
        )?;
        for slot in snapshot.terrain {
            writeln!(
                terrain,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                body,
                slot.slot,
                slot.depth,
                slot.lateral,
                u8::from(slot.active),
                slot.requested_sprite,
                slot.resolved_sprite,
                slot.rect.x_bytes,
                slot.rect.y,
                slot.rect.width_bytes,
                slot.rect.height,
                slot.phase,
                slot.kind,
                slot.movement,
                slot.animation_phase
            )?;
        }
        for slot in snapshot.enemies {
            writeln!(
                enemies,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                body,
                slot.slot,
                slot.depth,
                slot.lateral,
                slot.height,
                u8::from(slot.active),
                slot.requested_sprite,
                slot.resolved_sprite,
                slot.rect.x_bytes,
                slot.rect.y,
                slot.rect.width_bytes,
                slot.rect.height,
                slot.phase,
                slot.movement,
                slot.variant,
                slot.animation_phase
            )?;
        }
        let mut frame = new_file(destination.join(format!("{body:03}.cga")))?;
        frame.write_all(&gameplay.frame().to_cga())?;
        frame.flush()?;
    }
    weapon.flush()?;
    state.flush()?;
    terrain.flush()?;
    enemies.flush()?;
    player.flush()?;
    order.flush()?;
    full_order.flush()?;
    special.flush()?;
    if include_ending {
        let mut ending = Ending::new(&assets, &gameplay.snapshot(), gameplay.frame())?;
        let mut table = new_file(destination.join("ending.tsv"))?;
        let mut objects = new_file(destination.join("ending-world.tsv"))?;
        let mut order = new_file(destination.join("ending-order.tsv"))?;
        writeln!(
            table,
            "tick\ttitle\tstage\tdraw_index\thold_tick\tcycle\tbest_score\tdepth\trequested\tresolved\tx\ty\twidth\theight\tphase"
        )?;
        writeln!(
            objects,
            "tick\tslot\tdepth\tlateral\theight_world\tactive\trequested\tresolved\tx\ty\twidth\theight\tphase\tmovement"
        )?;
        writeln!(order, "tick\tposition\tslot")?;
        let mut tick = 0;
        loop {
            let state = ending.snapshot();
            let title = state.world[5];
            writeln!(
                table,
                "{}\t{:?}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                tick,
                state.title,
                state.stage,
                state.draw_index,
                state.hold_tick,
                state.cycle,
                state.best_score,
                title.depth,
                title.requested_sprite,
                title.resolved_sprite,
                title.rect.x_bytes,
                title.rect.y,
                title.rect.width_bytes,
                title.rect.height,
                title.phase
            )?;
            for slot in state.world {
                writeln!(
                    objects,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    tick,
                    slot.slot,
                    slot.depth,
                    slot.lateral,
                    slot.height,
                    u8::from(slot.active),
                    slot.requested_sprite,
                    slot.resolved_sprite,
                    slot.rect.x_bytes,
                    slot.rect.y,
                    slot.rect.width_bytes,
                    slot.rect.height,
                    slot.phase,
                    slot.movement
                )?;
            }
            for (position, slot) in state.full_world_order.into_iter().enumerate() {
                writeln!(order, "{tick}\t{position}\t{slot}")?;
            }
            let mut output = new_file(destination.join(format!("ending-{tick:03}.cga")))?;
            output.write_all(&ending.frame().to_cga())?;
            output.flush()?;
            if ending.can_return() {
                break;
            }
            ending.advance(&assets)?;
            tick += 1;
        }
        table.flush()?;
        objects.flush()?;
        order.flush()?;
        let startup = StartupFrames::load(&directory)?;
        let choice = ending.returned_intro_choice().unwrap();
        let mut output = new_file(destination.join("return-intro.cga"))?;
        output.write_all(&startup.return_intro(choice).to_cga())?;
        output.flush()?;
    }
    println!(
        "Wrote {} opening frames to {}",
        steps + 1,
        destination.display()
    );
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mach3-opening: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
