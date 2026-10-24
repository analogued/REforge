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
use std::path::PathBuf;

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
        diagnostics::run(
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
            frontend::interactive(&mut alley, options.scale, options.presentation, startup)
        }
        #[cfg(not(feature = "sdl"))]
        {
            let _ = alley;
            Err("This build has no window backend; enable the sdl feature".into())
        }
    }
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

#[cfg(feature = "dev-tools")]
mod diagnostics;
#[cfg(feature = "sdl")]
mod frontend;
#[cfg(test)]
mod tests;
