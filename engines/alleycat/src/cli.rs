// Copyright (c) 2026 analogued. Project license has not yet been selected.

use alleycat::alley::{Alley, Machine};
#[cfg(any(feature = "sdl", feature = "dev-tools"))]
use alleycat::cga::{HEIGHT, WIDTH};
#[cfg(feature = "dev-tools")]
use alleycat::dev_tools::DevOptions;
#[cfg(any(feature = "dev-tools", all(test, feature = "sdl")))]
use alleycat::player::Controls;
#[cfg(feature = "dev-tools")]
use alleycat::runtime::PassOutcome;
#[cfg(any(feature = "sdl", feature = "dev-tools"))]
use alleycat::runtime::Runtime;
#[cfg(any(feature = "sdl", feature = "dev-tools"))]
use alleycat::splash::Splash;
#[cfg(any(feature = "sdl", feature = "dev-tools"))]
use alleycat::timing::{RetraceProfile, VirtualClock};
#[cfg(feature = "dev-tools")]
use std::fs::{self, File};
#[cfg(feature = "dev-tools")]
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[cfg(all(feature = "sdl", feature = "dev-tools"))]
const BIOS_TICK_HZ: f64 = 18.2065;

const DEFAULT_SEED: u16 = 0xfa59;
#[cfg(feature = "sdl")]
const GAME_NAME: &str = "Alley Cat";

#[cfg(any(feature = "sdl", feature = "dev-tools"))]
struct Startup {
    splash: Splash,
    #[cfg(feature = "sdl")]
    difficulty_override: Option<u16>,
}

const HELP: &str = "Alley Cat — game reconstruction

Usage: reforge --original /path/to/CAT.EXE [options]

  --machine cga|pcjr       Hardware path (default: cga)
  --scale auto|1..16       Initial drawable pixel multiplier (default: auto)
  --presentation pixels|hyllian  Window presentation (default: pixels; unavailable shader falls back)
  --difficulty N          Initial difficulty 0..7; skips the skill question
  --help                  Show this help

Window controls: arrows to move/jump; Esc, P or Pause/Break to toggle pause.
Press a key to leave the animated splash, then select K/H/T/A in the skill menu.
F10 opens settings; Escape or F10 closes them.
Ctrl+Q or closing the window quits.
Ctrl+R restarts; Ctrl+M returns to the skill menu.
After the last life, the animated title returns; press a key to play again at the selected skill.
Alt drinks milk, uses paired holes in cheese and places a carried gift in the hearts room.
Gifts are picked up automatically.
Ordinary windows lead to fishbowl, bookcase, cheese, birdcage or milk; after success, the next leads to hearts.
PC Speaker audio is enabled; Ctrl-S toggles the native sound flag.
Timing uses a stock 4.77 MHz XT model; unmeasured paths retain provisional costs.
Your original executable is required; original data is never bundled.";

#[cfg(feature = "dev-tools")]
const DEV_HELP: &str = "
Development options (compiled with dev-tools):
  --scene NAME            Start in splash, alley, fishbowl, underwater, bookcase, cheese, birdcage, milk, hearts or completion
  --seed N                Random seed, decimal or 0xHEX (default: 0xfa59)
  --step-hz N             Diagnostic accepted updates/sec
  --headless              Run without a window
  --timed                 Use the reference clock in headless runs
  --steps N               Accepted updates to run (default: 60 headless)
  --script FILE           Headless CSV: x,y[,action]; directions -1..1, action 0/1
  --trace FILE            Write accepted player states as CSV
  --trace-timing FILE     Write timed outer-loop events as CSV
  --dump-cga FILE         Write the final packed 16384-byte framebuffer
  --dump-ppm FILE         Write the final 320x200 image
  --disable-damage SOURCES  all, none or comma-separated damage identifiers
  --disable-actors NAMES    all, none or comma-separated actor identifiers
  --infinite-lives          Preserve lives while retaining hit/death effects
  --original-window-timing Restore the original difficulty-6 window hold

Damage identifiers: alley-dog, room-dog, window-projectiles, milk-dogs, spider, eels, air.
Actor identifiers: alley-dog, room-dog, window-projectiles, milk-dogs, spider, eels, bin-cat, alley-mice, broom, hearts-arrows, hearts-rivals.
Repeated disable flags replace the previous list; the last value wins.
Damage suppression leaves actors active but makes their contacts harmless.";

#[cfg(feature = "dev-tools")]
#[derive(Clone, Copy, Eq, PartialEq)]
enum StartScene {
    Splash,
    Alley,
    Birdcage,
    Bookcase,
    Fishbowl,
    Underwater,
    Cheese,
    Milk,
    Hearts,
    Completion,
}

struct Options {
    original: PathBuf,
    machine: Machine,
    scale: Option<u32>,
    #[cfg(feature = "sdl")]
    presentation: reforge_sdl::Presentation,
    difficulty: u16,
    difficulty_selected: bool,
    #[cfg(feature = "dev-tools")]
    dev: DiagnosticOptions,
}

#[cfg(feature = "dev-tools")]
struct DiagnosticOptions {
    scene: StartScene,
    scene_selected: bool,
    seed: u16,
    step_hz: Option<f64>,
    headless: bool,
    timed: bool,
    steps: Option<usize>,
    script: Option<PathBuf>,
    trace: Option<PathBuf>,
    timing_trace: Option<PathBuf>,
    cga: Option<PathBuf>,
    ppm: Option<PathBuf>,
    switches: DevOptions,
}

#[cfg(feature = "dev-tools")]
impl Default for DiagnosticOptions {
    fn default() -> Self {
        Self {
            scene: StartScene::Alley,
            scene_selected: false,
            seed: DEFAULT_SEED,
            step_hz: None,
            headless: false,
            timed: false,
            steps: None,
            script: None,
            trace: None,
            timing_trace: None,
            cga: None,
            ppm: None,
            switches: DevOptions::default(),
        }
    }
}

fn integer(value: &str) -> Result<u16, String> {
    if let Some(hex) = value.strip_prefix("0x") {
        u16::from_str_radix(hex, 16).map_err(|_| format!("Invalid integer: {value}"))
    } else {
        value
            .parse()
            .map_err(|_| format!("Invalid integer: {value}"))
    }
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>, String> {
    let mut original = None;
    let mut presentation = None;
    let mut result = Options {
        original: PathBuf::new(),
        machine: Machine::Cga,
        scale: None,
        #[cfg(feature = "sdl")]
        presentation: reforge_sdl::Presentation::Pixels,
        difficulty: 0,
        difficulty_selected: false,
        #[cfg(feature = "dev-tools")]
        dev: DiagnosticOptions::default(),
    };
    let mut args = args.into_iter();
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            println!("{HELP}");
            #[cfg(feature = "dev-tools")]
            println!("{DEV_HELP}");
            return Ok(None);
        }
        #[cfg(feature = "dev-tools")]
        match option.as_str() {
            "--headless" => {
                result.dev.headless = true;
                continue;
            }
            "--timed" => {
                result.dev.timed = true;
                continue;
            }
            "--infinite-lives" => {
                result.dev.switches.infinite_lives = true;
                continue;
            }
            "--original-window-timing" => {
                result.dev.switches.original_window_timing = true;
                continue;
            }
            _ => {}
        }
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("Missing value for {option}"))
        };
        match option.as_str() {
            "--original" => original = Some(PathBuf::from(value()?)),
            "--machine" => {
                result.machine = match value()?.as_str() {
                    "cga" => Machine::Cga,
                    "pcjr" => Machine::Pcjr,
                    _ => return Err("Machine must be cga or pcjr".into()),
                }
            }
            "--scale" => {
                let value = value()?;
                result.scale = if value == "auto" {
                    None
                } else {
                    Some(u32::from(integer(&value)?))
                };
            }
            "--presentation" => {
                let value = value()?;
                if !matches!(value.as_str(), "pixels" | "hyllian") {
                    return Err("--presentation must be pixels or hyllian".into());
                }
                presentation = Some(value);
            }
            "--difficulty" => {
                result.difficulty = integer(&value()?)?;
                result.difficulty_selected = true;
            }
            #[cfg(feature = "dev-tools")]
            "--seed" => result.dev.seed = integer(&value()?)?,
            #[cfg(feature = "dev-tools")]
            "--scene" => {
                result.dev.scene_selected = true;
                result.dev.scene = match value()?.as_str() {
                    "splash" => StartScene::Splash,
                    "alley" => StartScene::Alley,
                    "birdcage" => StartScene::Birdcage,
                    "bookcase" => StartScene::Bookcase,
                    "fishbowl" => StartScene::Fishbowl,
                    "underwater" => StartScene::Underwater,
                    "cheese" => StartScene::Cheese,
                    "milk" => StartScene::Milk,
                    "hearts" => StartScene::Hearts,
                    "completion" => StartScene::Completion,
                    _ => return Err("Scene must be splash, alley, fishbowl, underwater, bookcase, cheese, birdcage, milk, hearts or completion".into()),
                }
            }
            #[cfg(feature = "dev-tools")]
            "--step-hz" => {
                let rate: f64 = value()?.parse().map_err(|_| "Invalid --step-hz")?;
                if !rate.is_finite() || !(0.1..=1000.0).contains(&rate) {
                    return Err("--step-hz must be finite and between 0.1 and 1000".into());
                }
                result.dev.step_hz = Some(rate);
            }
            #[cfg(feature = "dev-tools")]
            "--steps" => result.dev.steps = Some(value()?.parse().map_err(|_| "Invalid --steps")?),
            #[cfg(feature = "dev-tools")]
            "--script" => result.dev.script = Some(value()?.into()),
            #[cfg(feature = "dev-tools")]
            "--trace" => result.dev.trace = Some(value()?.into()),
            #[cfg(feature = "dev-tools")]
            "--trace-timing" => result.dev.timing_trace = Some(value()?.into()),
            #[cfg(feature = "dev-tools")]
            "--dump-cga" => result.dev.cga = Some(value()?.into()),
            #[cfg(feature = "dev-tools")]
            "--dump-ppm" => result.dev.ppm = Some(value()?.into()),
            #[cfg(feature = "dev-tools")]
            "--disable-damage" => result.dev.switches.set_disabled_damage(&value()?)?,
            #[cfg(feature = "dev-tools")]
            "--disable-actors" => result.dev.switches.set_disabled_actors(&value()?)?,
            _ => return Err(format!("Unknown option: {option}")),
        }
    }
    result.original = original.ok_or("Supply --original /path/to/CAT.EXE")?;
    if result.scale.is_some_and(|scale| !(1..=16).contains(&scale)) {
        return Err("Scale must be auto or an integer between 1 and 16".into());
    }
    if result.difficulty > 7 {
        return Err("Difficulty must be between 0 and 7".into());
    }
    #[cfg(feature = "dev-tools")]
    {
        let dev = &result.dev;
        if dev.scene == StartScene::Splash
            && (dev.script.is_some()
                || dev.trace.is_some()
                || dev.timing_trace.is_some()
                || dev.step_hz.is_some())
        {
            return Err("Splash preview does not use gameplay scripts, traces or --step-hz".into());
        }
        if dev.script.is_some() && !dev.headless {
            return Err("--script requires --headless".into());
        }
        if dev.timed && !dev.headless {
            return Err("--timed is a headless option; windowed runs use timing by default".into());
        }
        if dev.step_hz.is_some() && (dev.timed || dev.timing_trace.is_some()) {
            return Err(
                "--step-hz cannot be combined with timed execution or --trace-timing".into(),
            );
        }
        if dev.headless && dev.timing_trace.is_some() && !dev.timed {
            return Err("Headless --trace-timing requires --timed".into());
        }
    }
    if let Some(presentation) = presentation {
        #[cfg(feature = "sdl")]
        {
            #[cfg(feature = "dev-tools")]
            let windowed = !result.dev.headless;
            #[cfg(not(feature = "dev-tools"))]
            let windowed = true;
            if windowed {
                result.presentation = presentation.parse()?;
            } else {
                eprintln!("warning: --presentation does not apply to --headless; ignored");
            }
        }
        #[cfg(not(feature = "sdl"))]
        {
            let _ = presentation;
            eprintln!("warning: --presentation requires an SDL build; ignored");
        }
    }
    Ok(Some(result))
}

#[cfg(feature = "dev-tools")]
#[derive(Clone, Copy, Default)]
struct ScriptInput {
    controls: Controls,
    action: bool,
}

#[cfg(feature = "dev-tools")]
fn read_script(path: &PathBuf) -> Result<Vec<ScriptInput>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut controls = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || matches!(line, "x,y" | "x,y,action") {
            continue;
        }
        let fields: Vec<_> = line.split(',').collect();
        if !(2..=3).contains(&fields.len()) {
            return Err(format!(
                "Script line {} must contain x,y[,action]",
                index + 1
            ));
        }
        let (x, y) = (fields[0], fields[1]);
        let x = x.trim().parse().map_err(|_| "Invalid script X direction")?;
        let y = y.trim().parse().map_err(|_| "Invalid script Y direction")?;
        if !(-1..=1).contains(&x) || !(-1..=1).contains(&y) {
            return Err(format!(
                "Script line {} has a direction outside -1..1",
                index + 1
            ));
        }
        let action = match fields.get(2).map(|field| field.trim()) {
            None | Some("0") => false,
            Some("1") => true,
            _ => return Err(format!("Script line {} action must be 0 or 1", index + 1)),
        };
        controls.push(ScriptInput {
            controls: Controls { x, y },
            action,
        });
    }
    Ok(controls)
}

#[cfg(feature = "dev-tools")]
fn trace_row(output: &mut impl Write, index: usize, alley: &Alley) -> Result<(), String> {
    let player = &alley.player;
    writeln!(
        output,
        "{index},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        alley.scene(),
        player.x,
        player.y,
        player.vertical_coordinate,
        player.horizontal_direction,
        player.vertical_direction,
        player.horizontal_step,
        player.vertical_step,
        player.phase,
        player.phase_decrement,
        player.support_state,
        player.entry_state,
        player.entry_delay,
    )
    .map_err(|error| error.to_string())
}

#[cfg(feature = "dev-tools")]
const TIMING_HEADER: &str = "event,start_ns,end_ns,bios_tick,pit_counter,retrace,control_polled,accepted,accepted_ax,actor_group_due,fence_wait,mouse_wait,room_wait,scene,scene_changed,last_ordinary_room,previous_ordinary_room,random_state,difficulty,pending_hearts";

#[cfg(feature = "dev-tools")]
fn timing_row(
    output: &mut impl Write,
    index: usize,
    event: &PassOutcome,
    selection: alleycat::alley::SelectionTraceState,
) -> Result<(), String> {
    writeln!(
        output,
        "{index},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        event.start_nanos,
        event.end_nanos,
        event.tick_low,
        event.pit_counter,
        u8::from(event.retrace),
        u8::from(event.control_polled),
        u8::from(event.acceptance.is_some()),
        event.acceptance.map_or(0, |accepted| accepted.ax),
        u8::from(event.actor_group_due),
        u8::from(event.fence_wait),
        u8::from(event.mouse_wait),
        u8::from(event.room_wait),
        event.scene,
        u8::from(event.scene_changed),
        selection.last_room,
        selection.previous_room,
        selection.random_state,
        selection.difficulty,
        u8::from(selection.pending_hearts),
    )
    .map_err(|error| error.to_string())
}

/// Validate the engine argument contract without loading original data or SDL.
pub fn validate_arguments(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    parse_options(args).map(|_| ())
}

/// Run Alley Cat with arguments supplied by the application or legacy binary.
pub fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let Some(options) = parse_options(args)? else {
        return Ok(());
    };
    #[cfg(feature = "dev-tools")]
    let seed = options.dev.seed;
    #[cfg(not(feature = "dev-tools"))]
    let seed = DEFAULT_SEED;
    let alley = Alley::load(&options.original, seed, options.difficulty, options.machine)?;
    #[cfg(any(feature = "sdl", feature = "dev-tools"))]
    let startup = {
        #[cfg(feature = "dev-tools")]
        let enabled = options.dev.scene == StartScene::Splash
            || (!options.dev.headless && !options.dev.scene_selected);
        #[cfg(not(feature = "dev-tools"))]
        let enabled = true;
        if enabled {
            let assets = alleycat::assets::Assets::load(&options.original)?;
            let mut data = assets.runtime_data();
            alleycat::assets::set_word(&mut data, 0x2ae5, seed);
            Some(Startup {
                splash: Splash::new(&data, options.machine)?,
                #[cfg(feature = "sdl")]
                difficulty_override: options.difficulty_selected.then_some(options.difficulty),
            })
        } else {
            None
        }
    };
    #[cfg(feature = "dev-tools")]
    {
        let mut alley = alley;
        alley.set_dev_options(options.dev.switches)?;
        run_development(
            &mut alley,
            options.scale,
            #[cfg(feature = "sdl")]
            options.presentation,
            options.dev,
            startup,
        )
    }
    #[cfg(not(feature = "dev-tools"))]
    {
        #[cfg(feature = "sdl")]
        {
            let mut alley = alley;
            interactive(&mut alley, options.scale, options.presentation, startup)
        }
        #[cfg(not(feature = "sdl"))]
        {
            let _ = alley;
            Err("This build has no window backend; enable the sdl feature".into())
        }
    }
}

#[cfg(feature = "dev-tools")]
fn run_development(
    alley: &mut Alley,
    scale: Option<u32>,
    #[cfg(feature = "sdl")] presentation: reforge_sdl::Presentation,
    options: DiagnosticOptions,
    startup: Option<Startup>,
) -> Result<(), String> {
    #[cfg(not(feature = "sdl"))]
    let _ = scale;
    match options.scene {
        StartScene::Splash => {
            return splash_preview(
                startup.ok_or("Splash preview was not initialized")?.splash,
                scale,
                #[cfg(feature = "sdl")]
                presentation,
                options,
            );
        }
        StartScene::Alley => {}
        StartScene::Birdcage => alley.enter_birdcage(0)?,
        StartScene::Fishbowl | StartScene::Underwater => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Fishbowl play requires default timing or --headless --timed".into());
            }
            if options.scene == StartScene::Fishbowl {
                alley.enter_fishbowl(0)?;
            } else {
                alley.enter_underwater(0)?;
            }
        }
        StartScene::Bookcase => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Bookcase play requires default timing or --headless --timed".into());
            }
            alley.enter_bookcase(0)?;
        }
        StartScene::Cheese => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Cheese play requires default timing or --headless --timed".into());
            }
            alley.enter_cheese(0)?;
        }
        StartScene::Hearts => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Hearts play requires default timing or --headless --timed".into());
            }
            alley.enter_hearts(0)?;
        }
        StartScene::Milk => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err("Milk play requires default timing or --headless --timed".into());
            }
            alley.enter_milk(0)?;
        }
        StartScene::Completion => {
            if options.step_hz.is_some() || (options.headless && !options.timed) {
                return Err(
                    "Completion preview requires default timing or --headless --timed".into(),
                );
            }
            alley.enter_completion(0)?;
        }
    }
    let mut trace = options
        .trace
        .as_ref()
        .map(|path| File::create_new(path).map(BufWriter::new))
        .transpose()
        .map_err(|error| format!("Cannot create trace: {error}"))?;
    if let Some(trace) = trace.as_mut() {
        writeln!(trace, "index,scene,x,y,vertical_coordinate,x_direction,y_direction,x_step,y_step,phase,phase_decrement,support_state,entry_state,entry_delay")
            .map_err(|error| error.to_string())?;
    }
    let mut timing_trace = options
        .timing_trace
        .as_ref()
        .map(|path| File::create_new(path).map(BufWriter::new))
        .transpose()
        .map_err(|error| format!("Cannot create timing trace: {error}"))?;
    if let Some(output) = timing_trace.as_mut() {
        writeln!(output, "{TIMING_HEADER}").map_err(|error| error.to_string())?;
    }
    if options.headless {
        let script = options.script.as_ref().map(read_script).transpose()?;
        let steps = options
            .steps
            .unwrap_or_else(|| script.as_ref().map_or(60, Vec::len));
        let mut runtime = Runtime::new(alley.machine());
        let mut index = 0;
        let mut pass = 0;
        while index < steps {
            let input = script
                .as_ref()
                .and_then(|script| script.get(index))
                .copied()
                .unwrap_or_default();
            if options.timed {
                let event =
                    runtime.advance_pass_with_action(alley, input.controls, input.action)?;
                if let Some(output) = timing_trace.as_mut() {
                    timing_row(output, pass, &event, alley.selection_trace_state())?;
                }
                pass += 1;
                if alley.game_over() {
                    break;
                }
                if event.acceptance.is_none() {
                    continue;
                }
            } else {
                alley.update(input.controls)?;
            }
            if let Some(trace) = trace.as_mut() {
                trace_row(trace, index, alley)?;
            }
            index += 1;
        }
    } else {
        #[cfg(feature = "sdl")]
        if let Some(rate) = options.step_hz {
            interactive_diagnostic(
                alley,
                scale,
                presentation,
                startup,
                rate,
                options.steps,
                &mut trace,
            )?;
        } else {
            interactive(
                alley,
                scale,
                presentation,
                startup,
                options.steps,
                &mut trace,
                &mut timing_trace,
            )?;
        }
        #[cfg(not(feature = "sdl"))]
        return Err(
            "This build has no window backend; use --headless or enable the sdl feature".into(),
        );
    }
    if let Some(trace) = trace.as_mut() {
        trace.flush().map_err(|error| error.to_string())?;
    }
    if let Some(output) = timing_trace.as_mut() {
        output.flush().map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.cga {
        let mut file = File::create_new(path).map_err(|error| error.to_string())?;
        file.write_all(alley.framebuffer().bytes())
            .map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.ppm {
        let mut file = BufWriter::new(File::create_new(path).map_err(|error| error.to_string())?);
        write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n").map_err(|error| error.to_string())?;
        for rgba in alley.rgba().chunks_exact(4) {
            file.write_all(&rgba[..3])
                .map_err(|error| error.to_string())?;
        }
        file.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(any(feature = "sdl", feature = "dev-tools"))]
fn advance_splash(
    splash: &mut Splash,
    clock: &mut VirtualClock,
    key_pressed: bool,
) -> Result<(), String> {
    // The outer-loop prelude remains approximate; measured title helpers are
    // charged only when executed, independently of host presentation.
    let costs = alleycat::runtime::ReferenceCosts::ibm_xt(splash.machine());
    clock.advance_nanos(costs.prelude);
    let sample = clock.sample();
    splash.sound.at(clock.elapsed_pit_ticks() as u64);
    let step = splash.advance(sample.bios.tick_low, sample.retrace, key_pressed)?;
    clock.advance_nanos(costs.splash_nanos(step));
    Ok(())
}

#[cfg(feature = "sdl")]
fn present_splash(
    window: &mut alleycat::platform::Window,
    splash: &mut Splash,
    ticks: Option<usize>,
) -> Result<bool, String> {
    use std::time::{Duration, Instant};
    window.audio.reset()?;
    splash.sound.record(true);
    let mut clock = VirtualClock::new(RetraceProfile::for_machine(splash.machine()));
    let start = Instant::now();
    let mut pause_clock = alleycat::platform::PauseClock::new(start);
    let mut target_nanos = 0;
    window.present(&splash.rgba())?;
    while !splash.finished()
        && ticks.is_none_or(|limit| usize::from(clock.bios_sample().tick_low) < limit)
    {
        let input = window.poll_input();
        if input.quit {
            return Ok(false);
        }
        if input.sound_pressed {
            splash.toggle_sound();
        }
        let now = Instant::now();
        // The settings panel pauses title animation without changing native state.
        // Ordinary title keys remain available when the panel is closed.
        let pause = pause_clock.sample(
            now,
            alleycat::platform::Input {
                pause_pressed: false,
                ..input
            },
        );
        target_nanos += pause.elapsed.min(Duration::from_millis(100)).as_nanos();
        window.audio.pause(pause.paused);
        let mut key_pressed = input.continue_pressed;
        while !pause.paused
            && !splash.finished()
            && (clock.elapsed_nanos() < target_nanos || key_pressed)
        {
            advance_splash(splash, &mut clock, key_pressed)?;
            key_pressed = false;
        }
        window.audio.service(
            &mut splash.sound,
            clock.elapsed_pit_ticks() as u64,
            target_nanos,
        );
        if pause.changed || window.presentation_due() {
            window.present(&splash.rgba())?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    window.audio.pause(true);
    window.present(&splash.rgba())?;
    Ok(true)
}

#[cfg(feature = "dev-tools")]
fn splash_preview(
    mut splash: Splash,
    scale: Option<u32>,
    #[cfg(feature = "sdl")] presentation: reforge_sdl::Presentation,
    options: DiagnosticOptions,
) -> Result<(), String> {
    if options.headless {
        let mut clock = VirtualClock::new(RetraceProfile::for_machine(splash.machine()));
        let ticks = options.steps.unwrap_or(60);
        while !splash.finished() && usize::from(clock.bios_sample().tick_low) < ticks {
            advance_splash(&mut splash, &mut clock, false)?;
        }
    } else {
        #[cfg(feature = "sdl")]
        {
            let display = alleycat::platform::Display::new_with_presentation(
                &reforge_sdl::game_title(GAME_NAME, Some("Splash")),
                320,
                240,
                scale,
                presentation,
            )?;
            let creator = display.texture_creator();
            let mut window =
                alleycat::platform::Window::new(display, &creator, WIDTH as u32, HEIGHT as u32)?;
            present_splash(&mut window, &mut splash, options.steps)?;
        }
        #[cfg(not(feature = "sdl"))]
        {
            let _ = scale;
            return Err(
                "This build has no window backend; use --headless or enable the sdl feature".into(),
            );
        }
    }
    if let Some(path) = options.cga {
        let mut file = File::create_new(path).map_err(|error| error.to_string())?;
        file.write_all(splash.framebuffer().bytes())
            .map_err(|error| error.to_string())?;
    }
    if let Some(path) = options.ppm {
        let mut file = BufWriter::new(File::create_new(path).map_err(|error| error.to_string())?);
        write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n").map_err(|error| error.to_string())?;
        for rgba in splash.rgba().chunks_exact(4) {
            file.write_all(&rgba[..3])
                .map_err(|error| error.to_string())?;
        }
        file.flush().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(feature = "sdl")]
fn present_menu(
    window: &mut alleycat::platform::Window,
    alley: &mut Alley,
    difficulty: Option<u16>,
) -> Result<bool, String> {
    use std::time::Duration;
    window.audio.pause(true);
    let mut menu = alley.menu(difficulty)?;
    window.present(&menu.rgba())?;
    loop {
        let input = window.poll_input();
        if input.quit {
            return Ok(false);
        }
        if input.sound_pressed {
            alley.toggle_sound();
        }
        let changed = input.menu_key.is_some();
        if let Some(key) = input.menu_key {
            if menu.press(key)? {
                alley.start_new_game(menu.difficulty().ok_or("Menu has no skill selection")?, 0)?;
                return Ok(true);
            }
        }
        // The native question screen is static, but the host menu needs frames
        // for pointer hover, live settings and window resize updates.
        if window.presentation_due() || changed || input.settings_changed {
            window.present(&menu.rgba())?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(feature = "sdl")]
fn present_game_over(
    window: &mut alleycat::platform::Window,
    alley: &mut Alley,
) -> Result<bool, String> {
    let mut splash = alley.game_over_splash()?;
    if !present_splash(window, &mut splash, None)? {
        return Ok(false);
    }
    alley.start_after_game_over(&splash)?;
    Ok(true)
}

#[cfg(feature = "sdl")]
const CATCH_UP_MAX_PASSES: usize = 256;
#[cfg(feature = "sdl")]
const CATCH_UP_TIME_SLICE: std::time::Duration = std::time::Duration::from_millis(4);

/// Yield between complete guest passes; never fast-forward or discard work.
/// A time slice also bounds slow-host batches, except for their final pass.
#[cfg(feature = "sdl")]
fn catch_up_batch(
    mut elapsed: impl FnMut() -> std::time::Duration,
    mut advance: impl FnMut() -> Result<bool, String>,
) -> Result<usize, String> {
    let mut passes = 0;
    while passes < CATCH_UP_MAX_PASSES {
        // Always permit one pass so an expensive pass cannot prevent progress.
        if passes != 0 && elapsed() >= CATCH_UP_TIME_SLICE {
            break;
        }
        if !advance()? {
            break;
        }
        passes += 1;
    }
    Ok(passes)
}

#[cfg(feature = "sdl")]
fn interactive(
    alley: &mut Alley,
    scale: Option<u32>,
    presentation: reforge_sdl::Presentation,
    startup: Option<Startup>,
    #[cfg(feature = "dev-tools")] steps: Option<usize>,
    #[cfg(feature = "dev-tools")] trace: &mut Option<BufWriter<File>>,
    #[cfg(feature = "dev-tools")] timing_trace: &mut Option<BufWriter<File>>,
) -> Result<(), String> {
    use crate::control_history::ControlHistory;
    use alleycat::platform::{PauseClock, PauseOverlay, Window};
    use std::time::{Duration, Instant};

    let display = alleycat::platform::Display::new_with_presentation(
        &reforge_sdl::game_title(GAME_NAME, None),
        320,
        240,
        scale,
        presentation,
    )?;
    let creator = display.texture_creator();
    let mut window = Window::new(display, &creator, WIDTH as u32, HEIGHT as u32)?;
    if let Some(mut startup) = startup {
        if !present_splash(&mut window, &mut startup.splash, None)? {
            return Ok(());
        }
        alley.title_sound_preference(&startup.splash);
        if !present_menu(&mut window, alley, startup.difficulty_override)? {
            return Ok(());
        }
    }
    window.audio.reset()?;
    window.audio.pause(false);
    alley.sound.reset();
    alley.sound.record(true);
    let mut runtime = Runtime::new(alley.machine());
    runtime.responsive_controls = window.responsive_controls();
    let mut control_history = ControlHistory::new(window.control_keys());
    let start = Instant::now();
    let mut pause_clock = PauseClock::new(start);
    let pause_overlay = PauseOverlay::new(alley.pause_text()?[0], alley.machine())?;
    let mut target_nanos = 0;
    #[cfg(feature = "dev-tools")]
    let mut index = 0;
    #[cfg(feature = "dev-tools")]
    let mut pass = 0;
    window.present(&alley.rgba())?;
    loop {
        #[cfg(feature = "dev-tools")]
        if steps.is_some_and(|limit| index >= limit) {
            break;
        }
        if alley.game_over() {
            if !present_game_over(&mut window, alley)? {
                return Ok(());
            }
            window.audio.reset()?;
            window.audio.pause(false);
            alley.sound.reset();
            alley.sound.record(true);
            runtime = Runtime::new(alley.machine());
            runtime.responsive_controls = window.responsive_controls();
            control_history = ControlHistory::new(window.control_keys());
            target_nanos = 0;
            let now = Instant::now();
            pause_clock = PauseClock::new(now);
            window.present(&alley.rgba())?;
            continue;
        }
        runtime.responsive_controls = window.responsive_controls();
        let input = window.poll_input();
        if input.quit {
            break;
        }
        if input.sound_pressed {
            alley.toggle_sound();
        }
        let restarting = input.new_game_pressed;
        if input.menu_pressed {
            if !present_menu(&mut window, alley, None)? {
                break;
            }
        } else if restarting {
            alley.restart(0)?;
        }
        if input.menu_pressed || restarting {
            window.audio.reset()?;
            window.audio.pause(false);
            alley.sound.reset();
            alley.sound.record(true);
            runtime = Runtime::new(alley.machine());
            runtime.responsive_controls = window.responsive_controls();
            control_history = ControlHistory::new(window.control_keys());
            target_nanos = 0;
            let now = Instant::now();
            pause_clock = PauseClock::new(now);
            window.present(&alley.rgba())?;
            continue;
        }
        let (host_now_ns, changes) = window.take_control_changes();
        let now = Instant::now();
        let pause = pause_clock.sample(now, input);
        window.audio.pause(pause.paused);
        if pause.changed {
            // Preserve pending guest work across the settings panel; retain the
            // existing manual-pause policy of discarding partial pacing budget.
            if !input.settings_changed {
                target_nanos = runtime.clock.elapsed_nanos();
                control_history = ControlHistory::new(window.control_keys());
            } else {
                control_history.boundary(target_nanos, window.control_keys());
            }
            // Manual pause can discard catch-up debt and move this target
            // backward, or forward over committed blocking work. Retain audio
            // across that explicit mapping boundary instead of aging/replaying it.
            window.audio.rebase_presentation(target_nanos);
        }
        if pause.paused {
            if pause.changed || window.presentation_due() {
                window.present_with_pause(
                    &alley.rgba(),
                    (!input.settings_open).then_some(&pause_overlay),
                )?;
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        // Discard excess host stalls without resetting guest timer phases.
        let elapsed = pause.elapsed.min(Duration::from_millis(100));
        if !pause.changed {
            control_history.extend(host_now_ns, elapsed, target_nanos, changes);
        }
        target_nanos += elapsed.as_nanos();
        let batch_start = Instant::now();
        catch_up_batch(
            || batch_start.elapsed(),
            || {
                if runtime.clock.elapsed_nanos() >= target_nanos || alley.game_over() {
                    return Ok(false);
                }
                #[cfg(feature = "dev-tools")]
                if steps.is_some_and(|limit| index >= limit) {
                    return Ok(false);
                }
                #[cfg(not(feature = "dev-tools"))]
                runtime.advance_pass_with_input(alley, |at| control_history.sample(at))?;
                #[cfg(feature = "dev-tools")]
                {
                    let event =
                        runtime.advance_pass_with_input(alley, |at| control_history.sample(at))?;
                    if let Some(output) = timing_trace.as_mut() {
                        timing_row(output, pass, &event, alley.selection_trace_state())?;
                    }
                    pass += 1;
                    if event.acceptance.is_some() {
                        if let Some(output) = trace.as_mut() {
                            trace_row(output, index, alley)?;
                        }
                        index += 1;
                    }
                }
                Ok(true)
            },
        )?;
        window.audio.service(
            &mut alley.sound,
            runtime.clock.elapsed_pit_ticks() as u64,
            target_nanos,
        );
        if runtime.clock.elapsed_nanos() < target_nanos && !alley.game_over() {
            // Retain debt and repump events immediately. Intermediate rendering
            // and sleep must not make a slow catch-up batch fall further behind.
            continue;
        }
        if pause.changed || window.presentation_due() {
            window.present(&alley.rgba())?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    window.audio.pause(true);
    window.present(&alley.rgba())?;
    Ok(())
}

#[cfg(all(feature = "sdl", feature = "dev-tools"))]
fn interactive_diagnostic(
    alley: &mut Alley,
    scale: Option<u32>,
    presentation: reforge_sdl::Presentation,
    startup: Option<Startup>,
    step_hz: f64,
    steps: Option<usize>,
    trace: &mut Option<BufWriter<File>>,
) -> Result<(), String> {
    use alleycat::platform::{PauseClock, PauseOverlay, Window};
    use std::time::{Duration, Instant};
    let display = alleycat::platform::Display::new_with_presentation(
        &reforge_sdl::game_title(GAME_NAME, None),
        320,
        240,
        scale,
        presentation,
    )?;
    let creator = display.texture_creator();
    let mut window = Window::new(display, &creator, WIDTH as u32, HEIGHT as u32)?;
    if let Some(mut startup) = startup {
        if !present_splash(&mut window, &mut startup.splash, None)? {
            return Ok(());
        }
        alley.title_sound_preference(&startup.splash);
        if !present_menu(&mut window, alley, startup.difficulty_override)? {
            return Ok(());
        }
    }
    let period = Duration::from_secs_f64(1.0 / step_hz);
    // Original 1200 latches directions at intervals of at least two BIOS ticks,
    // independently of player acceptance and presentation.
    let control_period = Duration::from_secs_f64(2.0 / BIOS_TICK_HZ);
    let mut next_control_poll = Instant::now();
    let mut controls = Controls::default();
    let mut deadline = Instant::now() + period;
    let mut pause_clock = PauseClock::new(Instant::now());
    let pause_overlay = PauseOverlay::new(alley.pause_text()?[0], alley.machine())?;
    let mut index = 0;
    window.present(&alley.rgba())?;
    while steps.is_none_or(|steps| index < steps) {
        if alley.game_over() {
            if !present_game_over(&mut window, alley)? {
                return Ok(());
            }
            let now = Instant::now();
            deadline = now + period;
            next_control_poll = now;
            controls = Controls::default();
            pause_clock = PauseClock::new(now);
            window.present(&alley.rgba())?;
            continue;
        }
        let input = window.poll_input();
        if input.quit {
            break;
        }
        let restarting = input.new_game_pressed;
        if input.menu_pressed {
            if !present_menu(&mut window, alley, None)? {
                break;
            }
        } else if restarting {
            alley.restart(0)?;
        }
        if input.menu_pressed || restarting {
            let now = Instant::now();
            deadline = now + period;
            next_control_poll = now;
            controls = Controls::default();
            pause_clock = PauseClock::new(now);
            window.present(&alley.rgba())?;
            continue;
        }
        let now = Instant::now();
        let pause = pause_clock.sample(now, input);
        if input.settings_changed {
            controls = Controls::default();
        }
        if let Some(duration) = pause.resumed_after {
            // Preserve the remaining diagnostic step/effect and input waits.
            deadline += duration;
            next_control_poll += duration;
        }
        if pause.changed || pause.paused {
            if pause.changed || window.presentation_due() {
                window.present_with_pause(
                    &alley.rgba(),
                    (pause.paused && !input.settings_open).then_some(&pause_overlay),
                )?;
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        if now >= next_control_poll {
            // Left and up take precedence, matching the original keyboard decoder.
            controls = Controls {
                x: if input.left {
                    -1
                } else {
                    i8::from(input.right)
                },
                y: if input.up { -1 } else { i8::from(input.down) },
            };
            next_control_poll += control_period;
            if now >= next_control_poll {
                next_control_poll = now + control_period;
            }
        }
        if now >= deadline {
            let outcome = alley.update(controls)?;
            if let Some(trace) = trace.as_mut() {
                trace_row(trace, index, alley)?;
            }
            index += 1;
            deadline += period;
            if outcome.edge_reset {
                // The original fence-reset speaker effect blocks game work
                // until seven BIOS ticks elapse. Preserve a diagnostic pause
                // without blocking host input; synthesis/PRNG timing is deferred.
                deadline = now + Duration::from_secs_f64(7.0 / BIOS_TICK_HZ);
                // Interrupts may update held keys during the effect, but the
                // original direction poll resumes only after it returns.
                next_control_poll = deadline;
            }
            // Keep focus/debugger pauses from producing a burst of old input updates.
            if now.saturating_duration_since(deadline) > period * 4 {
                deadline = now + period;
            }
            window.present(&alley.rgba())?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "sdl")]
    #[test]
    fn catch_up_yields_for_slow_passes_and_retains_pending_work() {
        use std::cell::Cell;
        use std::time::Duration;

        let host_elapsed = Cell::new(Duration::ZERO);
        let mut guest_nanos = 0;
        let target = 100_000_000;
        let mut batches = 0;
        while guest_nanos < target {
            host_elapsed.set(Duration::ZERO);
            let passes = catch_up_batch(
                || host_elapsed.get(),
                || {
                    if guest_nanos >= target {
                        return Ok(false);
                    }
                    // Expensive host work advances only a short guest interval.
                    host_elapsed.set(host_elapsed.get() + Duration::from_millis(2));
                    guest_nanos += 50_000;
                    Ok(true)
                },
            )
            .unwrap();
            assert_eq!(passes, 2);
            batches += 1;
            if batches == 1 {
                assert_eq!(guest_nanos, 100_000);
                assert!(guest_nanos < target);
            }
        }
        assert_eq!(batches, 1000);
        assert_eq!(guest_nanos, target);
        // Even a single over-budget pass must complete before yielding.
        let passes = catch_up_batch(
            || Duration::from_secs(1),
            || {
                guest_nanos += 1;
                Ok(true)
            },
        )
        .unwrap();
        assert_eq!(passes, 1);
        assert_eq!(guest_nanos, target + 1);
    }

    #[cfg(feature = "sdl")]
    #[test]
    fn catch_up_pass_limit_stops_work_and_propagates_errors() {
        use std::time::Duration;

        let mut passes = 0;
        assert_eq!(
            catch_up_batch(
                || Duration::ZERO,
                || {
                    passes += 1;
                    Ok(true)
                }
            )
            .unwrap(),
            CATCH_UP_MAX_PASSES
        );
        assert_eq!(passes, CATCH_UP_MAX_PASSES);
        assert_eq!(catch_up_batch(|| Duration::ZERO, || Ok(false)), Ok(0));
        assert_eq!(
            catch_up_batch(|| Duration::ZERO, || Err("failed guest pass".into())),
            Err("failed guest pass".into())
        );
    }

    #[cfg(feature = "sdl")]
    #[test]
    fn catch_up_batches_preserve_guest_passes_and_blocking_fence_wait() {
        use std::time::Duration;

        for machine in [Machine::Cga, Machine::Pcjr] {
            for falling in [false, true] {
                let replay = |bounded: bool| {
                    let mut alley = Alley::synthetic();
                    if falling {
                        alley.player.band_state = 2;
                        alley.player.y = 108;
                        alley.player.vertical_coordinate = 158;
                        alley.player.vertical_direction = 1;
                    }
                    let mut runtime = Runtime::new(machine);
                    let target = 600_000_000;
                    let mut trace = Vec::new();
                    let mut fence_waits = 0;
                    let mut batches = 0;
                    while runtime.clock.elapsed_nanos() < target {
                        let mut advance = || {
                            if runtime.clock.elapsed_nanos() >= target {
                                return Ok(false);
                            }
                            let event = runtime.advance_pass_with_action(
                                &mut alley,
                                Controls { x: 1, y: 0 },
                                false,
                            )?;
                            fence_waits += usize::from(event.fence_wait);
                            trace.push(format!("{event:?};{:?}", alley.player));
                            Ok(true)
                        };
                        if bounded {
                            catch_up_batch(|| Duration::ZERO, advance).unwrap();
                        } else {
                            while advance().unwrap() {}
                        }
                        batches += 1;
                    }
                    (
                        trace,
                        alley.framebuffer().bytes().to_vec(),
                        runtime.clock.elapsed_nanos(),
                        fence_waits,
                        batches,
                    )
                };
                let reference = replay(false);
                let bounded = replay(true);
                assert_eq!(bounded.0, reference.0);
                assert_eq!(bounded.1, reference.1);
                assert_eq!(bounded.2, reference.2);
                assert_eq!(bounded.3, reference.3);
                if falling {
                    assert!(bounded.3 > CATCH_UP_MAX_PASSES);
                    assert!(bounded.4 > 1);
                }
            }
        }
    }

    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        parse_options(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn presentation_defaults_to_pixels_and_accepts_valid_choices() {
        let defaults = parse(&["--original", "example.exe"]).unwrap().unwrap();
        #[cfg(feature = "sdl")]
        assert_eq!(defaults.presentation, reforge_sdl::Presentation::Pixels);
        #[cfg(not(feature = "sdl"))]
        let _ = defaults;
        assert!(HELP.contains("--presentation pixels|hyllian"));
        for value in ["pixels", "hyllian"] {
            let options = parse(&["--original", "example.exe", "--presentation", value])
                .unwrap()
                .unwrap();
            #[cfg(feature = "sdl")]
            assert_eq!(options.presentation, value.parse().unwrap());
            #[cfg(not(feature = "sdl"))]
            let _ = options;
        }
        #[cfg(feature = "sdl")]
        {
            let options = parse(&[
                "--original",
                "example.exe",
                "--presentation",
                "hyllian",
                "--presentation",
                "pixels",
            ])
            .unwrap()
            .unwrap();
            assert_eq!(options.presentation, reforge_sdl::Presentation::Pixels);
        }
        for value in ["", "crt", "Hyllian", "--help"] {
            assert!(parse(&["--original", "example.exe", "--presentation", value]).is_err());
        }
        assert!(parse(&["--original", "example.exe", "--presentation"]).is_err());
    }

    #[cfg(all(feature = "sdl", feature = "dev-tools"))]
    #[test]
    fn headless_ignores_selected_presentation_in_either_order() {
        for input in [
            vec![
                "--original",
                "example.exe",
                "--presentation",
                "hyllian",
                "--headless",
            ],
            vec![
                "--original",
                "example.exe",
                "--headless",
                "--presentation",
                "hyllian",
            ],
        ] {
            let options = parse(&input).unwrap().unwrap();
            assert_eq!(options.presentation, reforge_sdl::Presentation::Pixels);
            assert!(options.dev.headless);
        }
    }

    #[test]
    fn scale_defaults_to_auto_and_last_explicit_selection_wins() {
        assert_eq!(
            parse(&["--original", "example.exe"])
                .unwrap()
                .unwrap()
                .scale,
            None
        );
        assert!(HELP.contains("--scale auto|1..16"));
        for (value, expected) in [
            ("auto", None),
            ("1", Some(1)),
            ("16", Some(16)),
            ("0x4", Some(4)),
        ] {
            let options = parse(&["--original", "example.exe", "--scale", value])
                .unwrap()
                .unwrap();
            assert_eq!(options.scale, expected);
        }
        for (first, last, expected) in [("auto", "3", Some(3)), ("3", "auto", None)] {
            let options = parse(&[
                "--original",
                "example.exe",
                "--scale",
                first,
                "--scale",
                last,
            ])
            .unwrap()
            .unwrap();
            assert_eq!(options.scale, expected);
        }
        for value in ["0", "17", "-1", "1.5", "Auto", "", "65536"] {
            assert!(
                parse(&["--original", "example.exe", "--scale", value]).is_err(),
                "{value:?}"
            );
        }
        assert!(parse(&["--original", "example.exe", "--scale"]).is_err());
    }

    #[test]
    fn player_options_remain_available() {
        let options = parse(&[
            "--original",
            "example.exe",
            "--machine",
            "pcjr",
            "--scale",
            "3",
            "--difficulty",
            "7",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(options.original, PathBuf::from("example.exe"));
        assert_eq!(options.machine, Machine::Pcjr);
        assert_eq!((options.scale, options.difficulty), (Some(3), 7));
        assert!(options.difficulty_selected);
        assert!(parse(&["--help"]).unwrap().is_none());
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn timing_trace_keeps_selection_state_without_an_accepted_player_update() {
        let event = PassOutcome {
            start_nanos: 100,
            end_nanos: 200,
            tick_low: 18,
            pit_counter: 65530,
            retrace: false,
            control_polled: true,
            acceptance: None,
            actor_group_due: true,
            fence_wait: false,
            mouse_wait: false,
            room_wait: false,
            scene: 7,
            scene_changed: true,
        };
        let selection = alleycat::alley::SelectionTraceState {
            last_room: 5,
            previous_room: 5,
            random_state: 64089,
            difficulty: 6,
            pending_hearts: false,
        };
        let mut output = Vec::new();
        timing_row(&mut output, 9, &event, selection).unwrap();
        let row = String::from_utf8(output).unwrap();
        let fields: Vec<_> = row.trim_end().split(',').collect();
        let columns: Vec<_> = TIMING_HEADER.split(',').collect();
        assert_eq!(fields.len(), columns.len());
        let value = |name| fields[columns.iter().position(|column| *column == name).unwrap()];
        for (name, expected) in [
            ("event", "9"),
            ("accepted", "0"),
            ("scene", "7"),
            ("scene_changed", "1"),
            ("last_ordinary_room", "5"),
            ("previous_ordinary_room", "5"),
            ("random_state", "64089"),
            ("difficulty", "6"),
            ("pending_hearts", "0"),
        ] {
            assert_eq!(value(name), expected, "{name}");
        }
    }

    #[test]
    fn invalid_player_options_are_rejected() {
        for args in [
            vec!["--original", "example.exe", "--scale", "0"],
            vec!["--original", "example.exe", "--scale", "17"],
            vec!["--original", "example.exe", "--difficulty", "8"],
            vec!["--original", "example.exe", "--machine", "other"],
            vec!["--original"],
            vec!["--unknown"],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    #[cfg(not(feature = "dev-tools"))]
    #[test]
    fn normal_parser_rejects_every_development_flag() {
        for flag in [
            "--seed",
            "--scene",
            "--step-hz",
            "--headless",
            "--timed",
            "--steps",
            "--script",
            "--trace",
            "--trace-timing",
            "--dump-cga",
            "--dump-ppm",
            "--disable-damage",
            "--disable-actors",
            "--infinite-lives",
            "--original-window-timing",
        ] {
            assert_eq!(
                parse(&[flag]).err(),
                Some(format!("Unknown option: {flag}"))
            );
            assert!(!HELP.contains(flag));
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn development_parser_retains_diagnostics_and_switches() {
        let options = parse(&[
            "--original",
            "example.exe",
            "--seed",
            "0x1234",
            "--scene",
            "underwater",
            "--headless",
            "--timed",
            "--steps",
            "12",
            "--script",
            "controls.csv",
            "--trace",
            "states.csv",
            "--trace-timing",
            "timing.csv",
            "--dump-cga",
            "screen.bin",
            "--dump-ppm",
            "screen.ppm",
            "--disable-damage",
            "eels,air",
            "--disable-actors",
            "spider,broom",
            "--infinite-lives",
            "--original-window-timing",
        ])
        .unwrap()
        .unwrap();
        let dev = options.dev;
        assert_eq!(dev.seed, 0x1234);
        assert!(dev.scene == StartScene::Underwater);
        assert!(dev.headless && dev.timed && dev.switches.infinite_lives);
        assert!(dev.switches.original_window_timing);
        assert_eq!(dev.steps, Some(12));
        assert_eq!(dev.script, Some(PathBuf::from("controls.csv")));
        assert_eq!(dev.trace, Some(PathBuf::from("states.csv")));
        assert_eq!(dev.timing_trace, Some(PathBuf::from("timing.csv")));
        assert_eq!(dev.cga, Some(PathBuf::from("screen.bin")));
        assert_eq!(dev.ppm, Some(PathBuf::from("screen.ppm")));
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn repeated_disable_flags_replace_launcher_defaults() {
        let options = parse(&[
            "--original",
            "example.exe",
            "--disable-damage",
            "all",
            "--disable-damage",
            "none",
            "--disable-actors",
            "all",
            "--disable-actors",
            "none",
        ])
        .unwrap()
        .unwrap();
        assert!(
            !options
                .dev
                .switches
                .actor_disabled(alleycat::dev_tools::Actor::AlleyDog)
        );
        assert!(
            !options
                .dev
                .switches
                .damage_disabled(alleycat::dev_tools::Damage::AlleyDog)
        );
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn milk_scene_is_available_for_timed_development_play() {
        let options = parse(&[
            "--original",
            "example.exe",
            "--scene",
            "milk",
            "--headless",
            "--timed",
        ])
        .unwrap()
        .unwrap();
        assert!(options.dev.scene == StartScene::Milk);
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn explicit_scenes_bypass_startup_and_splash_accepts_frame_exports() {
        let default = parse(&["--original", "example.exe"]).unwrap().unwrap();
        assert!(!default.dev.scene_selected);
        for scene in ["alley", "splash"] {
            let options = parse(&[
                "--original",
                "example.exe",
                "--scene",
                scene,
                "--headless",
                "--steps",
                "72",
                "--dump-cga",
                "title.cga",
            ])
            .unwrap()
            .unwrap();
            assert!(options.dev.scene_selected);
            assert_eq!(options.dev.steps, Some(72));
            assert_eq!(options.dev.cga, Some(PathBuf::from("title.cga")));
        }
        for extra in [
            vec!["--script", "controls.csv"],
            vec!["--trace", "player.csv"],
            vec!["--trace-timing", "timing.csv"],
            vec!["--step-hz", "36"],
        ] {
            let mut args = vec![
                "--original",
                "example.exe",
                "--scene",
                "splash",
                "--headless",
            ];
            args.extend(extra);
            assert_eq!(
                parse(&args).err(),
                Some("Splash preview does not use gameplay scripts, traces or --step-hz".into())
            );
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn diagnostic_constraints_remain_enforced() {
        for extra in [
            vec!["--script", "controls.csv"],
            vec!["--timed"],
            vec!["--headless", "--trace-timing", "timing.csv"],
            vec!["--headless", "--timed", "--step-hz", "10"],
            vec!["--step-hz", "NaN"],
            vec!["--step-hz", "0"],
            vec!["--disable-damage", "all,eels"],
            vec!["--disable-actors", "unknown"],
        ] {
            let mut args = vec!["--original", "example.exe"];
            args.extend(extra);
            assert!(parse(&args).is_err());
        }
    }
}
