// Copyright (c) 2026 analogued. Project license has not yet been selected.

use alley_cat::alley::{Alley, Machine};
use alley_cat::cga::{HEIGHT, WIDTH};
use alley_cat::player::{Controls, Player};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

const BIOS_TICK_HZ: f64 = 18.2065;
// Measured mean active cadence in the recorded DOSBox-X reference, not an
// emulation of its changed-tick/countdown-expiry spacing within each tick.
const DEFAULT_STEP_HZ: f64 = BIOS_TICK_HZ * 2.0;

const HELP: &str = "Alley Cat — initial alley reconstruction

Usage: alley-cat --original /path/to/CAT.EXE [options]

  --machine cga|pcjr       Palette selection (default: cga)
  --seed N                Initial random seed, decimal or 0xHEX (default: 0xfa59)
  --difficulty N          Alley layout difficulty 0..7 (default: 0)
  --step-hz N              Diagnostic player update rate (default: 36.413)
  --headless              Run without a window
  --steps N               Accepted updates to run (default: 60 headless)
  --script FILE           Headless controls CSV: x,y, each -1, 0, or 1
  --trace FILE            Write accepted player states as CSV
  --dump-cga FILE         Write the final packed 16384-byte framebuffer
  --dump-ppm FILE         Write the final 320x200 image
  --help                  Show this help

Window controls: arrows to move/jump; Escape to quit.
This slice omits actors, rooms and audio. Update pacing is diagnostic.
Your original executable is required; original data is never bundled.";

struct Options {
    original: PathBuf,
    machine: Machine,
    seed: u16,
    difficulty: u16,
    step_hz: f64,
    headless: bool,
    steps: Option<usize>,
    script: Option<PathBuf>,
    trace: Option<PathBuf>,
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
        seed: 0xfa59,
        difficulty: 0,
        step_hz: DEFAULT_STEP_HZ,
        headless: false,
        steps: None,
        script: None,
        trace: None,
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
            "--difficulty" => result.difficulty = integer(&value)?,
            "--step-hz" => {
                result.step_hz = value.parse().map_err(|_| "Invalid --step-hz")?;
                if !result.step_hz.is_finite() || !(0.1..=1000.0).contains(&result.step_hz) {
                    return Err("--step-hz must be finite and between 0.1 and 1000".into());
                }
            }
            "--steps" => result.steps = Some(value.parse().map_err(|_| "Invalid --steps")?),
            "--script" => result.script = Some(value.into()),
            "--trace" => result.trace = Some(value.into()),
            "--dump-cga" => result.cga = Some(value.into()),
            "--dump-ppm" => result.ppm = Some(value.into()),
            _ => return Err(format!("Unknown option: {option}")),
        }
    }
    result.original = original.ok_or("Supply --original /path/to/CAT.EXE")?;
    if result.difficulty > 7 {
        return Err("Difficulty must be between 0 and 7".into());
    }
    if result.script.is_some() && !result.headless {
        return Err("--script requires --headless".into());
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
    if options.headless {
        let script = options.script.as_ref().map(read_script).transpose()?;
        let steps = options
            .steps
            .unwrap_or_else(|| script.as_ref().map_or(60, Vec::len));
        for index in 0..steps {
            let controls = script
                .as_ref()
                .and_then(|script| script.get(index))
                .copied()
                .unwrap_or_default();
            alley.update(controls)?;
            if let Some(trace) = trace.as_mut() {
                trace_row(trace, index, &alley.player)?;
            }
        }
    } else {
        #[cfg(feature = "sdl")]
        interactive(&mut alley, options.step_hz, options.steps, &mut trace)?;
        #[cfg(not(feature = "sdl"))]
        return Err(
            "This build has no window backend; use --headless or enable the sdl feature".into(),
        );
    }
    if let Some(trace) = trace.as_mut() {
        trace.flush().map_err(|error| error.to_string())?;
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
    step_hz: f64,
    steps: Option<usize>,
    trace: &mut Option<BufWriter<File>>,
) -> Result<(), String> {
    use alley_cat::platform::Window;
    use std::time::{Duration, Instant};
    let mut window = Window::new(
        "Alley Cat — alley prototype",
        WIDTH as u32,
        HEIGHT as u32,
        3,
    )?;
    let period = Duration::from_secs_f64(1.0 / step_hz);
    // Original 1200 latches directions at intervals of at least two BIOS ticks,
    // independently of player acceptance and presentation.
    let control_period = Duration::from_secs_f64(2.0 / BIOS_TICK_HZ);
    let mut next_control_poll = Instant::now();
    let mut controls = Controls::default();
    let mut deadline = Instant::now() + period;
    let mut index = 0;
    window.present(&alley.rgba())?;
    while steps.is_none_or(|steps| index < steps) {
        let input = window.poll_input();
        if input.quit {
            break;
        }
        let now = Instant::now();
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
