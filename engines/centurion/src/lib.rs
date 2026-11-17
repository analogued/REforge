// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
mod arena;
mod assets;
mod audio;
#[cfg(feature = "sdl")]
mod audio_output;
#[cfg(any(feature = "sdl", test))]
mod audio_timeline;
mod battle;
mod campaign;
mod cleopatra;
pub mod cli;
mod difficulty;
mod diplomacy;
mod encounter;
mod ending;
mod enemy;
mod events;
mod fleet;
mod fleet_nav;
mod games;
mod graphics;
mod intro;
#[cfg(feature = "sdl")]
pub mod launcher;
mod legion;
mod movement;
mod music;
mod naval;
mod naval_clock;
mod naval_combat;
mod naval_fleet;
mod naval_intro;
mod naval_motion;
mod naval_probe;
#[cfg(feature = "sdl")]
mod player;
mod province;
mod race;
mod race_crash;
mod race_gallery;
mod race_motion;
mod race_render;
#[cfg(feature = "dev-tools")]
mod record;
mod rng;
mod session;
mod sound;
mod state;
mod turn;
mod water;
#[cfg(feature = "sdl")]
mod window;
#[cfg(any(feature = "sdl", test))]
mod window_geometry;

use assets::{Assets, Result};
use session::{Input, Phase, Session};
use std::io::Write;
use std::path::PathBuf;

/// Run the standalone Centurion research frontend with process arguments.
///
/// This entry point accepts an explicitly selected DOS data directory. It is
/// separate from REforge's catalog-driven player integration.
pub fn run_prototype() -> std::result::Result<(), String> {
    #[cfg(feature = "dev-tools")]
    let mut record_dir = None;
    #[cfg(feature = "dev-tools")]
    let mut slot_fallback = true;
    #[cfg(feature = "dev-tools")]
    let mut slot_errors = None;
    let mut naval_probe = None;
    let mut spawn_probe = None;
    let mut campaign_soak = None;
    let mut mute = false;
    let mut tribute_indicator = true;
    let mut skip_battle_fanfare = false;
    let mut music_wav = None;
    let mut music_trace = None;
    let mut music_ticks = 7000;
    let mut music_song = 0;
    let mut sound_trace = None;
    let mut music_suite = None;
    let mut original = None;
    let mut export = None;
    let mut export_steps = std::collections::BTreeSet::new();
    let mut script = None;
    let mut validate = false;
    let mut headless = false;
    let mut with_cursor = false;
    let mut load = None;
    let mut save = None;
    let mut state_dir = None;
    let mut phase = Phase::Intro;
    let mut scale = None;
    let mut seeds = (0u16, 0u16);
    let mut smoke_frames = None;
    let mut smoke_resize = None;
    let mut smoke_maximize = false;
    let mut smoke_skip = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("Missing value for {argument}"))
        };
        match argument.as_str() {
            #[cfg(feature = "dev-tools")]
            "--record" => record_dir = Some(PathBuf::from(value()?)),
            #[cfg(feature = "dev-tools")]
            "--no-slot-fallback" => slot_fallback = false,
            #[cfg(feature = "dev-tools")]
            "--record-slot-errors" => slot_errors = Some(PathBuf::from(value()?)),
            "--campaign-soak" => {
                campaign_soak = Some((
                    value()?.parse::<u16>().map_err(|e| e.to_string())?,
                    PathBuf::from(value()?),
                ))
            }
            "--enemy-spawn-probe" => {
                spawn_probe = Some((PathBuf::from(value()?), PathBuf::from(value()?)))
            }
            "--naval-probe" => naval_probe = Some((value()?, value()?, value()?, value()?)),
            "--mute" => mute = true,
            "--original-tribute-menus" => tribute_indicator = false,
            "--skip-battle-fanfare" => skip_battle_fanfare = true,
            "--sound-trace" => sound_trace = Some(PathBuf::from(value()?)),
            "--music-song" => music_song = value()?.parse::<u8>().map_err(|e| e.to_string())?,
            "--music-suite" => music_suite = Some(PathBuf::from(value()?)),
            "--music-wav" => music_wav = Some(PathBuf::from(value()?)),
            "--music-trace" => music_trace = Some(PathBuf::from(value()?)),
            "--music-ticks" => music_ticks = value()?.parse::<u32>().map_err(|e| e.to_string())?,
            "--original" => original = Some(PathBuf::from(value()?)),
            "--export" => export = Some(PathBuf::from(value()?)),
            "--export-step" => {
                export_steps.insert(value()?.parse::<usize>().map_err(|e| e.to_string())?);
            }
            "--script" => script = Some(PathBuf::from(value()?)),
            "--load" => load = Some(PathBuf::from(value()?)),
            "--save" => save = Some(PathBuf::from(value()?)),
            "--state-dir" => state_dir = Some(PathBuf::from(value()?)),
            "--phase" => phase = Phase::parse(&value()?)?,
            "--scale" => {
                let requested = value()?;
                if requested == "auto" {
                    scale = None;
                } else {
                    let number = requested.parse::<u32>().map_err(|e| e.to_string())?;
                    if !(1..=16).contains(&number) {
                        return Err("Scale must be auto or 1..16".into());
                    }
                    scale = Some(number);
                }
            }
            "--seed" => {
                let pair = value()?;
                let (first, second) = pair
                    .split_once(',')
                    .ok_or("Seed must be two decimal u16 values separated by a comma")?;
                seeds = (
                    first.parse::<u16>().map_err(|e| e.to_string())?,
                    second.parse::<u16>().map_err(|e| e.to_string())?,
                );
            }
            "--smoke-frames" => {
                smoke_frames = Some(value()?.parse::<u32>().map_err(|e| e.to_string())?)
            }
            "--smoke-resize" => {
                let dimensions = value()?;
                let (width, height) = dimensions.split_once('x').ok_or("Expected WIDTHxHEIGHT")?;
                let dimensions = (
                    width.parse::<i32>().map_err(|e| e.to_string())?,
                    height.parse::<i32>().map_err(|e| e.to_string())?,
                );
                if dimensions.0 <= 0 || dimensions.1 <= 0 {
                    return Err("Invalid smoke-test window size".into());
                }
                smoke_resize = Some(dimensions);
            }
            "--smoke-maximize" => smoke_maximize = true,
            "--smoke-skip" => smoke_skip = true,
            "--check-assets" => validate = true,
            "--headless" => headless = true,
            "--with-cursor" => with_cursor = true,
            "--help" | "-h" => {
                println!(
                    "Centurion private native prototype\nUsage: centurion-prototype --original DIR [--scale auto|1..16] [--state-dir DIR]\n       [--phase intro|title|credits|prologue|map|menu|about|rome|tribute|tribute-policy|province-list|legions|raise-legion|legion-list] [--seed FIRST,SECOND]\n       [--headless] [--export DIR] [--with-cursor] [--script FILE]\n       [--load FILE] [--save FILE] [--check-assets] [--mute] [--original-tribute-menus]\n       [--music-wav FILE] [--music-trace FILE] [--music-ticks N]\n\nThe timed intro runs automatically. A key/click skips to the orders prologue; another enters the map. Escape/F1 opens the map menu.\nArrow keys and Enter select actions; menu shortcuts: S L N A D Q C.\nMouse selection is supported. Save descriptions accept ASCII text.\nClick owned provinces for Tribute, Province List and local legion recruitment/reinforcement/upgrades. Rome offers global Tribute Policy; other owned provinces offer Plunder.\nLegions -> Move and map right-click support unopposed friendly marches. Arrows move the map cursor; Space selects a province, Enter a legion.\nForeign entry supports scouting, negotiations, peaceful alliances and withdrawal.\nGo to War opens formation/tactic selection, live battles and casualty reports. Space/click pauses; M/R/D order melee/retreat/resume, S displays Roman orders. Select a cohort for arrows/Space/Enter or drag orders.\nForced land battles and roaming-army interceptions resolve before the destination province. Shipbuilding supports all three ship types (Up/Down adjusts, Enter buys, Escape cancels); legions can embark and land using available transports.\nF lists fleets; Space selects a ready local fleet and sea target. Naval arrows steer/change speed; Space/Insert fires arrows, Enter/+ boards, R retreats.\nE or the End Turn button runs recovery, economy, revolts, enemy invasions and yearly events. Defensive battles resume the same turn. H opens provincial festivals or Rome's gladiator shows; Left/Right moves in the arena, Up/Down guards, Left+Up/Down attacks. Rome H -> R opens chariot racing, betting and skulduggery. Race arrows steer/brake; Up whips, Up+Left/Right strikes. Aegyptus now includes Cleopatra's alliance, courtship and animated ending.\nTribute menus shade the applied level; --original-tribute-menus restores the earlier presentation for comparisons.\nOriginal AdLib music and sound effects are enabled throughout the game; --mute disables playback."
                );
                #[cfg(feature = "dev-tools")]
                println!(
                    "\nDev tools: [--record NEW_DIR] [--no-slot-fallback] [--record-slot-errors FILE]\n--record NEW_DIR preserves logical inputs, RNG and sound requests, build identity and compressed frame/state checkpoints; F12 marks a point during play."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument {argument}")),
        }
    }
    #[cfg(not(feature = "sdl"))]
    let _ = (
        mute,
        scale,
        smoke_frames,
        smoke_resize,
        smoke_maximize,
        smoke_skip,
    );
    let original =
        original.ok_or("Supply --original DIR pointing to the selected writable game copy")?;
    let assets = Assets::open(&original)?;
    if let Some((input, output)) = spawn_probe {
        return enemy::spawn_observation(&assets, &input, &output);
    }
    if let Some((kind, side, input, output)) = naval_probe {
        return naval_probe::run(&assets, &kind, &side, &input, &output);
    }
    if let Some(path) = music_suite {
        audio::export_suite(&assets, &path)?;
    }
    if music_wav.is_some() || music_trace.is_some() {
        audio::export(
            &assets,
            music_song,
            music_ticks,
            music_wav.as_deref(),
            music_trace.as_deref(),
        )?;
    }
    if validate {
        let (raw, compressed, bitmaps, sprites) = assets.validate_all()?;
        println!(
            "Validated {raw} raw resources, {compressed} LZSS resources, {bitmaps} CBM bitmaps and {sprites} sprites."
        );
    }
    let mut session = Session::new(&assets, phase, seeds.0, seeds.1)?;
    session.save_dir = state_dir;
    session.tribute_indicator = tribute_indicator;
    #[cfg(feature = "dev-tools")]
    {
        session.slot_fallback = slot_fallback;
    }
    #[cfg(feature = "dev-tools")]
    if let Some(path) = slot_errors {
        for line in std::fs::read_to_string(path)
            .map_err(|e| e.to_string())?
            .lines()
        {
            let (slot, command) = line.split_once('\t').ok_or("Invalid slot-error record")?;
            let Input::Text(error) = Input::parse(command)? else {
                return Err("Invalid slot-error text".into());
            };
            session
                .slot_errors
                .insert(slot.parse::<usize>().map_err(|e| e.to_string())?, error);
        }
    }
    session.skip_battle_fanfare = skip_battle_fanfare;
    if let Some(path) = load {
        session.campaign.load(&assets::read(&path)?)?;
    }
    #[cfg(feature = "dev-tools")]
    if let Some(directory) = record_dir {
        session.start_recording(&assets, &directory, mute)?;
    }
    if sound_trace.is_some() && !headless {
        return Err("--sound-trace requires --headless".into());
    }
    let mut sound_file = sound_trace
        .map(std::fs::File::create)
        .transpose()
        .map_err(|e| e.to_string())?;
    if let Some(file) = &mut sound_file {
        writeln!(file, "step\ttick\tcommand\tid").map_err(|e| e.to_string())?;
    }
    let mut record_sound = |session: &Session, step: usize| -> Result<()> {
        if let Some(file) = &mut sound_file {
            for command in session.campaign.sound.drain() {
                let (kind, id) = match command {
                    sound::Command::Play(id) => ("play", id),
                    sound::Command::Stop => ("stop", 255),
                };
                writeln!(file, "{step}\t{}\t{kind}\t{id}", session.ticks)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    };
    record_sound(&session, 0)?;
    if let Some((years, output)) = campaign_soak {
        session.soak(&assets, years, &output)?;
    }
    if let Some(directory) = &export {
        session
            .render(&assets, with_cursor)?
            .export(&assets, directory, "frame-000")?;
        session.export_battle(directory, 0)?;
    }
    if let Some(path) = script {
        let commands =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut step = 0;
        for line in commands
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let input = Input::parse(line)?;
            session.input(&assets, input)?;
            step += 1;
            record_sound(&session, step)?;
            if let Some(directory) = &export
                && (export_steps.is_empty() || export_steps.contains(&step))
            {
                session.render(&assets, with_cursor)?.export(
                    &assets,
                    directory,
                    &format!("frame-{step:03}"),
                )?;
                session.export_battle(directory, step)?;
            }
            println!("{step}: {}", session.state_summary());
            if session.quit {
                break;
            }
        }
    }
    if !headless {
        #[cfg(not(feature = "sdl"))]
        return Err("Interactive playback requires the sdl feature".into());
        #[cfg(feature = "sdl")]
        window::run(
            &assets,
            &mut session,
            scale,
            window::Smoke {
                frames: smoke_frames,
                resize: smoke_resize,
                maximize: smoke_maximize,
                skip: smoke_skip,
            },
            mute,
        )?;
    }
    if let Some(path) = save {
        std::fs::write(&path, session.campaign.save())
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    #[cfg(feature = "dev-tools")]
    session.finish_recording(&assets)?;
    println!(
        "{}; original={}",
        session.state_summary(),
        assets.original().display()
    );
    Ok(())
}
