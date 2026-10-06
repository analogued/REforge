// Copyright (c) 2026 analogued. Project license has not yet been selected.

use alley_cat::alley::{Alley, Machine};
#[cfg(any(feature = "sdl", feature = "dev-tools"))]
use alley_cat::cga::{HEIGHT, WIDTH};
#[cfg(feature = "dev-tools")]
use alley_cat::dev_tools::DevOptions;
#[cfg(any(feature = "sdl", feature = "dev-tools"))]
use alley_cat::player::Controls;
#[cfg(feature = "dev-tools")]
use alley_cat::runtime::PassOutcome;
#[cfg(any(feature = "sdl", feature = "dev-tools"))]
use alley_cat::runtime::Runtime;
#[cfg(feature = "dev-tools")]
use std::fs::{self, File};
#[cfg(feature = "dev-tools")]
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[cfg(all(feature = "sdl", feature = "dev-tools"))]
const BIOS_TICK_HZ: f64 = 18.2065;

const DEFAULT_SEED: u16 = 0xfa59;

const HELP: &str = "Alley Cat — game reconstruction

Usage: alley-cat --original /path/to/CAT.EXE [options]

  --machine cga|pcjr       Hardware path (default: cga)
  --scale N               Initial window multiplier, 1..16 (default: 4)
  --difficulty N          Initial difficulty 0..7 (default: 0)
  --help                  Show this help

Window controls: arrows to move/jump; Esc, P or Pause/Break to toggle pause.
Ctrl+Q or closing the window quits.
R starts a new game after all lives are lost.
Alt drinks milk, uses paired holes in cheese and places a carried gift in the hearts room.
Gifts are picked up automatically.
Ordinary windows lead to fishbowl, bookcase, cheese, birdcage or milk; after success, the next leads to hearts.
Audio output is not implemented.
Timing uses a DOSBox-X 240-cycle model.
Your original executable is required; original data is never bundled.";

#[cfg(feature = "dev-tools")]
const DEV_HELP: &str = "
Development options (compiled with dev-tools):
  --scene NAME            Start in alley, fishbowl, underwater, bookcase, cheese, birdcage, milk, hearts or completion
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
    scale: u16,
    difficulty: u16,
    #[cfg(feature = "dev-tools")]
    dev: DiagnosticOptions,
}

#[cfg(feature = "dev-tools")]
struct DiagnosticOptions {
    scene: StartScene,
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

fn options() -> Result<Option<Options>, String> {
    parse_options(std::env::args().skip(1))
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>, String> {
    let mut original = None;
    let mut result = Options {
        original: PathBuf::new(),
        machine: Machine::Cga,
        scale: 4,
        difficulty: 0,
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
            "--scale" => result.scale = integer(&value()?)?,
            "--difficulty" => result.difficulty = integer(&value()?)?,
            #[cfg(feature = "dev-tools")]
            "--seed" => result.dev.seed = integer(&value()?)?,
            #[cfg(feature = "dev-tools")]
            "--scene" => {
                result.dev.scene = match value()?.as_str() {
                    "alley" => StartScene::Alley,
                    "birdcage" => StartScene::Birdcage,
                    "bookcase" => StartScene::Bookcase,
                    "fishbowl" => StartScene::Fishbowl,
                    "underwater" => StartScene::Underwater,
                    "cheese" => StartScene::Cheese,
                    "milk" => StartScene::Milk,
                    "hearts" => StartScene::Hearts,
                    "completion" => StartScene::Completion,
                    _ => return Err("Scene must be alley, fishbowl, underwater, bookcase, cheese, birdcage, milk, hearts or completion".into()),
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
    if !(1..=16).contains(&result.scale) {
        return Err("Scale must be between 1 and 16".into());
    }
    if result.difficulty > 7 {
        return Err("Difficulty must be between 0 and 7".into());
    }
    #[cfg(feature = "dev-tools")]
    {
        let dev = &result.dev;
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
fn timing_row(output: &mut impl Write, index: usize, event: &PassOutcome) -> Result<(), String> {
    writeln!(
        output,
        "{index},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
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
    )
    .map_err(|error| error.to_string())
}

fn run() -> Result<(), String> {
    let Some(options) = options()? else {
        return Ok(());
    };
    #[cfg(feature = "dev-tools")]
    let seed = options.dev.seed;
    #[cfg(not(feature = "dev-tools"))]
    let seed = DEFAULT_SEED;
    let alley = Alley::load(&options.original, seed, options.difficulty, options.machine)?;
    #[cfg(feature = "dev-tools")]
    {
        let mut alley = alley;
        alley.set_dev_options(options.dev.switches)?;
        run_development(&mut alley, options.scale, options.dev)
    }
    #[cfg(not(feature = "dev-tools"))]
    {
        #[cfg(feature = "sdl")]
        {
            let mut alley = alley;
            interactive(&mut alley, options.scale)
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
    scale: u16,
    options: DiagnosticOptions,
) -> Result<(), String> {
    #[cfg(not(feature = "sdl"))]
    let _ = scale;
    match options.scene {
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
        writeln!(output, "event,start_ns,end_ns,bios_tick,pit_counter,retrace,control_polled,accepted,accepted_ax,actor_group_due,fence_wait,mouse_wait,room_wait,scene,scene_changed")
            .map_err(|error| error.to_string())?;
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
                    timing_row(output, pass, &event)?;
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
            interactive_diagnostic(alley, scale, rate, options.steps, &mut trace)?;
        } else {
            interactive(alley, scale, options.steps, &mut trace, &mut timing_trace)?;
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

#[cfg(feature = "sdl")]
fn interactive(
    alley: &mut Alley,
    scale: u16,
    #[cfg(feature = "dev-tools")] steps: Option<usize>,
    #[cfg(feature = "dev-tools")] trace: &mut Option<BufWriter<File>>,
    #[cfg(feature = "dev-tools")] timing_trace: &mut Option<BufWriter<File>>,
) -> Result<(), String> {
    use alley_cat::platform::{PauseClock, PauseOverlay, Window};
    use std::time::{Duration, Instant};

    let mut window = Window::new(
        "Alley Cat — reconstruction",
        WIDTH as u32,
        HEIGHT as u32,
        u32::from(scale),
    )?;
    let mut runtime = Runtime::new(alley.machine());
    let start = Instant::now();
    let mut pause_clock = PauseClock::new(start);
    let pause_overlay = PauseOverlay::new(alley.pause_text()?[0], alley.machine())?;
    let mut target_nanos = 0;
    let mut next_present = start;
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
        let input = window.poll_input();
        if input.quit {
            break;
        }
        if input.restart_pressed && alley.game_over() {
            alley.restart(0)?;
            runtime = Runtime::new(alley.machine());
            target_nanos = 0;
        }
        let held = Controls {
            x: if input.left {
                -1
            } else {
                i8::from(input.right)
            },
            y: if input.up { -1 } else { i8::from(input.down) },
        };
        let now = Instant::now();
        let pause = pause_clock.sample(now, input);
        if pause.changed {
            // Do not carry a partial host pacing budget across the pause.
            target_nanos = runtime.clock.elapsed_nanos();
            next_present = now;
        }
        if pause.paused {
            if now >= next_present {
                window.present_with_pause(&alley.rgba(), Some(&pause_overlay))?;
                next_present = now + Duration::from_millis(16);
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        // Discard excess host stalls without resetting guest timer phases.
        let elapsed = pause.elapsed.min(Duration::from_millis(100));
        target_nanos += elapsed.as_nanos();
        while runtime.clock.elapsed_nanos() < target_nanos {
            #[cfg(feature = "dev-tools")]
            if steps.is_some_and(|limit| index >= limit) {
                break;
            }
            #[cfg(not(feature = "dev-tools"))]
            runtime.advance_pass_with_action(alley, held, input.alt)?;
            #[cfg(feature = "dev-tools")]
            {
                let event = runtime.advance_pass_with_action(alley, held, input.alt)?;
                if let Some(output) = timing_trace.as_mut() {
                    timing_row(output, pass, &event)?;
                }
                pass += 1;
                if event.acceptance.is_some() {
                    if let Some(output) = trace.as_mut() {
                        trace_row(output, index, alley)?;
                    }
                    index += 1;
                }
            }
        }
        if now >= next_present {
            window.present(&alley.rgba())?;
            next_present = now + Duration::from_millis(16);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    window.present(&alley.rgba())?;
    Ok(())
}

#[cfg(all(feature = "sdl", feature = "dev-tools"))]
fn interactive_diagnostic(
    alley: &mut Alley,
    scale: u16,
    step_hz: f64,
    steps: Option<usize>,
    trace: &mut Option<BufWriter<File>>,
) -> Result<(), String> {
    use alley_cat::platform::{PauseClock, PauseOverlay, Window};
    use std::time::{Duration, Instant};
    let mut window = Window::new(
        "Alley Cat — reconstruction",
        WIDTH as u32,
        HEIGHT as u32,
        u32::from(scale),
    )?;
    let period = Duration::from_secs_f64(1.0 / step_hz);
    // Original 1200 latches directions at intervals of at least two BIOS ticks,
    // independently of player acceptance and presentation.
    let control_period = Duration::from_secs_f64(2.0 / BIOS_TICK_HZ);
    let mut next_control_poll = Instant::now();
    let mut controls = Controls::default();
    let mut deadline = Instant::now() + period;
    let mut pause_clock = PauseClock::new(Instant::now());
    let pause_overlay = PauseOverlay::new(alley.pause_text()?[0], alley.machine())?;
    let mut next_present = Instant::now();
    let mut index = 0;
    window.present(&alley.rgba())?;
    while steps.is_none_or(|steps| index < steps) {
        let input = window.poll_input();
        if input.quit {
            break;
        }
        let now = Instant::now();
        let pause = pause_clock.sample(now, input);
        if let Some(duration) = pause.resumed_after {
            // Preserve the remaining diagnostic step/effect and input waits.
            deadline += duration;
            next_control_poll += duration;
        }
        if pause.changed || pause.paused {
            if pause.changed || now >= next_present {
                window.present_with_pause(&alley.rgba(), pause.paused.then_some(&pause_overlay))?;
                next_present = now + Duration::from_millis(16);
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

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        parse_options(args.iter().map(|arg| (*arg).to_owned()))
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
        assert_eq!((options.scale, options.difficulty), (3, 7));
        assert!(parse(&["--help"]).unwrap().is_none());
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
                .actor_disabled(alley_cat::dev_tools::Actor::AlleyDog)
        );
        assert!(
            !options
                .dev
                .switches
                .damage_disabled(alley_cat::dev_tools::Damage::AlleyDog)
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
