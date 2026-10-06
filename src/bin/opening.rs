// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use mach3::{
    Error,
    assets::GraphicsAssets,
    opening::{Ending, LAST_BODY, Opening},
    startup::StartupFrames,
};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

const HELP: &str = "Usage: mach3-opening [--steps 0..836] [--ending] <assets-directory> <new-output-directory>\n\n\
Export the no-input opening from the initial frame through the requested body.\n\
Writes numbered CGA frames and state/object TSV tables; output directory must be new.\n\
Default: 836 bodies, including the complete first no-input run. No audio or player controls.\n\
--ending: also export the first title pair and selected return intro image.";

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
    if args.peek().is_some_and(|arg| arg == "--steps") {
        args.next();
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
    let directory = args.next().map(PathBuf::from);
    let destination = args.next().map(PathBuf::from);
    let (Some(directory), Some(destination), None) = (directory, destination, args.next()) else {
        return Err(Error::InvalidData(HELP.into()));
    };
    let assets = GraphicsAssets::load(&directory)?;
    let mut opening = Opening::new(&assets)?;
    fs::create_dir(&destination)?;
    let mut state = new_file(destination.join("state.tsv"))?;
    let mut terrain = new_file(destination.join("terrain.tsv"))?;
    let mut enemies = new_file(destination.join("enemies.tsv"))?;
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
        "body\tdepth\tlateral\theight_world\trequested\tresolved\tx\ty\twidth\theight\tphase\tdamage\tanimation_row\tgrace\thorizon\tlives\tgame_over"
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
            opening.advance(&assets)?;
        }
        let snapshot = opening.snapshot();
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
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
            u8::from(p.game_over)
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
        frame.write_all(&opening.frame().to_cga())?;
        frame.flush()?;
    }
    state.flush()?;
    terrain.flush()?;
    enemies.flush()?;
    player.flush()?;
    order.flush()?;
    full_order.flush()?;
    special.flush()?;
    if include_ending {
        let mut ending = Ending::new(&assets, &opening.snapshot(), opening.frame())?;
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
