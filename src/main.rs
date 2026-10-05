// Copyright (c) 2026 analogued. Project license has not yet been selected.

use alley_cat::alley::{Alley, Machine};
use alley_cat::cga::{HEIGHT, WIDTH};
use alley_cat::player::{Controls, Player};
use alley_cat::runtime::{PassOutcome, Runtime};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[cfg(feature = "sdl")]
const BIOS_TICK_HZ: f64 = 18.2065;

const HELP: &str = "Alley Cat — initial alley reconstruction

Usage: alley-cat --original /path/to/CAT.EXE [options]

  --machine cga|pcjr       Hardware path (default: cga)
  --scale N               Initial window multiplier, 1..16 (default: 4)
  --seed N                Initial random seed, decimal or 0xHEX (default: 0xfa59)
  --difficulty N          Alley layout difficulty 0..7 (default: 0)
  --step-hz N              Override timing with diagnostic accepted updates/sec
  --headless              Run without a window
  --timed                 Use the reference clock in headless runs
  --steps N               Accepted updates to run (default: 60 headless)
  --script FILE           Headless controls CSV: x,y, each -1, 0, or 1
  --trace FILE            Write accepted player states as CSV
  --trace-timing FILE     Write timed outer-loop events as CSV
  --dump-cga FILE         Write the final packed 16384-byte framebuffer
  --dump-ppm FILE         Write the final 320x200 image
  --help                  Show this help

Window controls: arrows to move/jump; Esc, P or Pause/Break to toggle pause.
Ctrl+Q or closing the window quits.
This slice includes bin cats and mice; hazards, rooms and audio remain absent.
Timing uses a DOSBox-X 240-cycle model.
Your original executable is required; original data is never bundled.";

struct Options {
    original: PathBuf,
    machine: Machine,
    scale: u16,
    seed: u16,
    difficulty: u16,
    step_hz: Option<f64>,
    headless: bool,
    timed: bool,
    steps: Option<usize>,
    script: Option<PathBuf>,
    trace: Option<PathBuf>,
    timing_trace: Option<PathBuf>,
    cga: Option<PathBuf>,
    ppm: Option<PathBuf>,
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
    let mut original = None;
    let mut result = Options {
        original: PathBuf::new(),
        machine: Machine::Cga,
        scale: 4,
        seed: 0xfa59,
        difficulty: 0,
        step_hz: None,
        headless: false,
        timed: false,
        steps: None,
        script: None,
        trace: None,
        timing_trace: None,
        cga: None,
        ppm: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            println!("{HELP}");
            return Ok(None);
        }
        if option == "--headless" {
            result.headless = true;
            continue;
        }
        if option == "--timed" {
            result.timed = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {option}"))?;
        match option.as_str() {
            "--original" => original = Some(PathBuf::from(value)),
            "--machine" => {
                result.machine = match value.as_str() {
                    "cga" => Machine::Cga,
                    "pcjr" => Machine::Pcjr,
                    _ => return Err("Machine must be cga or pcjr".into()),
                }
            }
            "--seed" => result.seed = integer(&value)?,
            "--scale" => result.scale = integer(&value)?,
            "--difficulty" => result.difficulty = integer(&value)?,
            "--step-hz" => {
                let rate: f64 = value.parse().map_err(|_| "Invalid --step-hz")?;
                if !rate.is_finite() || !(0.1..=1000.0).contains(&rate) {
                    return Err("--step-hz must be finite and between 0.1 and 1000".into());
                }
                result.step_hz = Some(rate);
            }
            "--steps" => result.steps = Some(value.parse().map_err(|_| "Invalid --steps")?),
            "--script" => result.script = Some(value.into()),
            "--trace" => result.trace = Some(value.into()),
            "--trace-timing" => result.timing_trace = Some(value.into()),
            "--dump-cga" => result.cga = Some(value.into()),
            "--dump-ppm" => result.ppm = Some(value.into()),
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
    if result.script.is_some() && !result.headless {
        return Err("--script requires --headless".into());
    }
    if result.timed && !result.headless {
        return Err("--timed is a headless option; windowed runs use timing by default".into());
    }
    if result.step_hz.is_some() && (result.timed || result.timing_trace.is_some()) {
        return Err("--step-hz cannot be combined with timed execution or --trace-timing".into());
    }
    if result.headless && result.timing_trace.is_some() && !result.timed {
        return Err("Headless --trace-timing requires --timed".into());
    }
    Ok(Some(result))
}

fn read_script(path: &PathBuf) -> Result<Vec<Controls>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut controls = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line == "x,y" {
            continue;
        }
        let (x, y) = line
            .split_once(',')
            .ok_or_else(|| format!("Script line {} must contain x,y", index + 1))?;
        let x = x.trim().parse().map_err(|_| "Invalid script X direction")?;
        let y = y.trim().parse().map_err(|_| "Invalid script Y direction")?;
        if !(-1..=1).contains(&x) || !(-1..=1).contains(&y) {
            return Err(format!(
                "Script line {} has a direction outside -1..1",
                index + 1
            ));
        }
        controls.push(Controls { x, y });
    }
    Ok(controls)
}

fn trace_row(output: &mut impl Write, index: usize, player: &Player) -> Result<(), String> {
    writeln!(
        output,
        "{index},{},{},{},{},{},{},{},{},{},{},{},{}",
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

fn timing_row(output: &mut impl Write, index: usize, event: &PassOutcome) -> Result<(), String> {
    writeln!(
        output,
        "{index},{},{},{},{},{},{},{},{},{},{},{}",
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
    )
    .map_err(|error| error.to_string())
}

fn run() -> Result<(), String> {
    let Some(options) = options()? else {
        return Ok(());
    };
    let mut alley = Alley::load(
        &options.original,
        options.seed,
        options.difficulty,
        options.machine,
    )?;
    let mut trace = options
        .trace
        .as_ref()
        .map(|path| File::create_new(path).map(BufWriter::new))
        .transpose()
        .map_err(|error| format!("Cannot create trace: {error}"))?;
    if let Some(trace) = trace.as_mut() {
        writeln!(trace, "index,x,y,vertical_coordinate,x_direction,y_direction,x_step,y_step,phase,phase_decrement,support_state,entry_state,entry_delay")
            .map_err(|error| error.to_string())?;
    }
    let mut timing_trace = options
        .timing_trace
        .as_ref()
        .map(|path| File::create_new(path).map(BufWriter::new))
        .transpose()
        .map_err(|error| format!("Cannot create timing trace: {error}"))?;
    if let Some(output) = timing_trace.as_mut() {
        writeln!(output, "event,start_ns,end_ns,bios_tick,pit_counter,retrace,control_polled,accepted,accepted_ax,actor_group_due,fence_wait,mouse_wait")
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
            let controls = script
                .as_ref()
                .and_then(|script| script.get(index))
                .copied()
                .unwrap_or_default();
            if options.timed {
                let event = runtime.advance_pass(&mut alley, controls)?;
                if let Some(output) = timing_trace.as_mut() {
                    timing_row(output, pass, &event)?;
                }
                pass += 1;
                if event.acceptance.is_none() {
                    continue;
                }
            } else {
                alley.update(controls)?;
            }
            if let Some(trace) = trace.as_mut() {
                trace_row(trace, index, &alley.player)?;
            }
            index += 1;
        }
    } else {
        #[cfg(feature = "sdl")]
        if let Some(rate) = options.step_hz {
            interactive_diagnostic(&mut alley, options.scale, rate, options.steps, &mut trace)?;
        } else {
            interactive(
                &mut alley,
                options.scale,
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

#[cfg(feature = "sdl")]
fn interactive(
    alley: &mut Alley,
    scale: u16,
    steps: Option<usize>,
    trace: &mut Option<BufWriter<File>>,
    timing_trace: &mut Option<BufWriter<File>>,
) -> Result<(), String> {
    use alley_cat::platform::{PauseClock, PauseOverlay, Window};
    use std::time::{Duration, Instant};

    let mut window = Window::new(
        "Alley Cat — alley prototype",
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
    let mut index = 0;
    let mut pass = 0;
    window.present(&alley.rgba())?;
    while steps.is_none_or(|limit| index < limit) {
        let input = window.poll_input();
        if input.quit {
            break;
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
        while runtime.clock.elapsed_nanos() < target_nanos
            && steps.is_none_or(|limit| index < limit)
        {
            let event = runtime.advance_pass(alley, held)?;
            if let Some(output) = timing_trace.as_mut() {
                timing_row(output, pass, &event)?;
            }
            pass += 1;
            if event.acceptance.is_some() {
                if let Some(output) = trace.as_mut() {
                    trace_row(output, index, &alley.player)?;
                }
                index += 1;
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

#[cfg(feature = "sdl")]
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
        "Alley Cat — alley prototype",
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
                trace_row(trace, index, &alley.player)?;
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
