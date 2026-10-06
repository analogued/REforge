// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use mach3::{
    Error,
    assets::GraphicsAssets,
    opening::{LAST_BODY, Opening},
};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

const HELP: &str = "Usage: mach3-opening [--steps 0..360] <assets-directory> <new-output-directory>\n\n\
Export the no-input opening from the initial frame through the requested body.\n\
Writes numbered CGA frames and state/object TSV tables; output directory must be new.\n\
Default: 360 bodies, including the first four enemy waves. No audio or player controls.";

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
                Error::InvalidData("--steps must be an integer between 0 and 360".into())
            })?;
    }
    let directory = args.next().map(PathBuf::from);
    let destination = args.next().map(PathBuf::from);
    let (Some(directory), Some(destination), None) = (directory, destination, args.next()) else {
        return Err(Error::InvalidData(HELP.into()));
    };
    let assets = GraphicsAssets::load(directory)?;
    let mut opening = Opening::new(&assets)?;
    fs::create_dir(&destination)?;
    let mut state = new_file(destination.join("state.tsv"))?;
    let mut terrain = new_file(destination.join("terrain.tsv"))?;
    let mut enemies = new_file(destination.join("enemies.tsv"))?;
    let mut player = new_file(destination.join("player.tsv"))?;
    writeln!(
        state,
        "body\tstrip\trng_a\trng_b\trng_c\tcursor\tdelay\tterrain_selector\tfuel\tscore\tmarker_requested\tmarker_previous\tmarker_resolved\tmarker_width\tmarker_height\ttarget_lateral\ttarget_height\tpending_score\tstrip_y\tvertical_origin\tlives_requested\tlives_resolved\tbar_width\tbar_draw_width"
    )?;
    writeln!(
        player,
        "body\tdepth\tlateral\theight_world\trequested\tresolved\tx\ty\twidth\theight\tphase\tdamage\tanimation_row\tgrace\thorizon\tlives"
    )?;
    writeln!(
        enemies,
        "body\tslot\tdepth\tlateral\theight_world\tactive\trequested\tresolved\tx\ty\twidth\theight\tphase"
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
        let h = snapshot.hud;
        let [a, b, c] = snapshot.random;
        writeln!(
            state,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
            h.left_bar_draw_width
        )?;
        let p = snapshot.player;
        writeln!(
            player,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
            p.lives
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
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
                slot.phase
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
