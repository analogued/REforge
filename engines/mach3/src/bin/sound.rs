// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]

use mach3::{
    Error,
    sound::{
        FarPointer, IntroCall, IntroProgram, IntroSequence, Resource, SampleBus, SampleBytes,
        SampleEnvironment, SamplePlayer, SampleSoundState, SoundAction, SoundAssets, StartResult,
        render::{AudioRenderer, RenderingPolicy},
    },
};
use reforge_audio::{
    render::{self, BlipRenderer, Config, SignalRenderer},
    speaker::{Events, PcSpeaker},
    wav::WavWriter,
};
use std::{
    collections::{BTreeSet, VecDeque},
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

const PIT_CLOCK_HZ: u32 = reforge_audio::PIT_HZ as u32;
const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_AUDIO_CHUNK_TICKS: u32 = 11_931;

const HELP: &str = "Usage: mach3-sound [--device [--wav [--sample-rate HZ] [--audio-chunk-ticks TICKS]]] --context INPUT.tsv [--external-byte RESOURCE:OFFSET:VALUE ...] <assets-directory> <new-output-directory>\n\n\
Export one bounded intro traversal followed by its pre-game sample.\n\
Context header: call<TAB>interrupt<TAB>port<TAB>value. Numbers are decimal or 0x hexadecimal.\n\
Calls start at 1. Each call requires one port 0x61 read at interrupt 0.\n\
Keyboard events are paired port 0x60/0x61 reads at the same interrupt >=1,\n\
delivered before that numbered IRQ0. Context contains device reads, not sample requests.\n\
Writes calls.tsv, ports.tsv, effects.tsv and diagnostic.txt; destination must be new.\n\
Nominal time advances by 60 PIT ticks per delivered IRQ0. This does not model\n\
setup gaps, PIT phase, host time, foreground instruction timing or rendered audio.\n\
--device drives IRQ0 from the shared PIT, also writing device.tsv with pin/drive edges.\n\
It uses explicit synthetic initial HIGH pins, whole-clock writes and zero instruction gaps;\n\
port61 context supplies unmodeled upper read bits, with lower speaker bits checked.\n\
--wav requires --device and writes paired offline mono 16-bit speaker.wav and enhanced-pcm.wav\n\
from the same execution, at 48000 Hz by default.\n\
--sample-rate and --audio-chunk-ticks require --wav; both are positive decimal integers.\n\
Audio chunks default to 11931 PIT ticks. No live audio playback is involved.";

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidData(message.into())
}

fn number(text: &str) -> Result<u32, Error> {
    let result = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else {
        text.parse()
    };
    result.map_err(|_| invalid(format!("invalid unsigned number: {text}")))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DeviceRead {
    call: u32,
    interrupt: u32,
    port: u16,
    value: u8,
}

fn parse_context(text: &str) -> Result<VecDeque<DeviceRead>, Error> {
    let mut lines = text.lines();
    if lines.next() != Some("call\tinterrupt\tport\tvalue") {
        return Err(invalid(
            "context header must be call<TAB>interrupt<TAB>port<TAB>value",
        ));
    }
    let mut reads = VecDeque::new();
    for (index, line) in lines.enumerate() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 4 {
            return Err(invalid(format!(
                "context row {} requires four fields",
                index + 2
            )));
        }
        let values = fields
            .iter()
            .map(|field| number(field))
            .collect::<Result<Vec<_>, _>>()?;
        if values[0] == 0 || values[2] > u32::from(u16::MAX) || values[3] > 255 {
            return Err(invalid(format!(
                "context row {} has an out-of-range call, port or byte",
                index + 2
            )));
        }
        reads.push_back(DeviceRead {
            call: values[0],
            interrupt: values[1],
            port: values[2] as u16,
            value: values[3] as u8,
        });
    }
    let mut cursor = 0;
    let mut expected_call = 1;
    while let Some(initial) = reads.get(cursor) {
        if (initial.call, initial.interrupt, initial.port) != (expected_call, 0, 0x61) {
            return Err(invalid(
                "each ordered call requires exactly one initial port 0x61 read at interrupt 0",
            ));
        }
        cursor += 1;
        let mut previous_interrupt = 0;
        while let Some(keyboard) = reads.get(cursor).filter(|row| row.call == expected_call) {
            let Some(control) = reads.get(cursor + 1) else {
                return Err(invalid(
                    "keyboard context requires paired port 0x60/0x61 reads",
                ));
            };
            if keyboard.interrupt <= previous_interrupt
                || keyboard.port != 0x60
                || (control.call, control.interrupt, control.port)
                    != (keyboard.call, keyboard.interrupt, 0x61)
            {
                return Err(invalid(
                    "keyboard read pairs must be ordered at increasing positive interrupt numbers",
                ));
            }
            previous_interrupt = keyboard.interrupt;
            cursor += 2;
        }
        expected_call += 1;
    }
    if reads.is_empty() {
        return Err(invalid("context requires at least one sample call"));
    }
    Ok(reads)
}

fn external_byte(text: &str) -> Result<(Resource, u16, u8), Error> {
    let fields: Vec<_> = text.split(':').collect();
    if fields.len() != 3 {
        return Err(invalid("--external-byte requires RESOURCE:OFFSET:VALUE"));
    }
    let resource = Resource::ALL
        .into_iter()
        .find(|resource| resource.name().eq_ignore_ascii_case(fields[0]))
        .ok_or_else(|| invalid("--external-byte names an unknown sample resource"))?;
    let offset = u16::try_from(number(fields[1])?)
        .map_err(|_| invalid("external byte offset exceeds 65535"))?;
    let value =
        u8::try_from(number(fields[2])?).map_err(|_| invalid("external byte value exceeds 255"))?;
    Ok((resource, offset, value))
}

fn new_file(path: impl AsRef<Path>) -> Result<BufWriter<std::fs::File>, Error> {
    Ok(BufWriter::new(
        OpenOptions::new().write(true).create_new(true).open(path)?,
    ))
}

struct RenderedWav {
    policy: RenderingPolicy,
    renderer: BlipRenderer,
    wav: WavWriter<File>,
}

impl RenderedWav {
    fn level(&mut self, tick: u64, level: Option<i16>) -> Result<(), Error> {
        if let Some(level) = level {
            self.renderer
                .level_at(tick, level, &mut |samples| {
                    self.wav.write_samples(samples).map_err(render::Error::Io)
                })
                .map_err(|error| invalid(error.to_string()))?;
        }
        Ok(())
    }

    fn finish(mut self, tick: u64) -> Result<(), Error> {
        self.renderer
            .finish_until(tick, &mut |samples| {
                self.wav.write_samples(samples).map_err(render::Error::Io)
            })
            .map_err(|error| invalid(error.to_string()))?;
        self.wav.finish()?;
        Ok(())
    }
}

struct PairedAudio([RenderedWav; 2]);

impl PairedAudio {
    fn new(config: Config, tick: u64, drive: bool, destination: &Path) -> Result<Self, Error> {
        let output = |mode, name| {
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination.join(name))?;
            Ok::<_, Error>(RenderedWav {
                policy: RenderingPolicy::new(mode, false),
                renderer: BlipRenderer::new(config, tick, 0)
                    .map_err(|error| invalid(error.to_string()))?,
                wav: WavWriter::new(file, config.sample_rate)?,
            })
        };
        let mut audio = Self([
            output(AudioRenderer::Speaker, "speaker.wav")?,
            output(AudioRenderer::EnhancedPcm, "enhanced-pcm.wav")?,
        ]);
        audio.drive(tick, drive)?;
        Ok(audio)
    }

    fn action(&mut self, tick: u64, action: SoundAction) -> Result<(), Error> {
        for output in &mut self.0 {
            let level = output.policy.action(tick, action);
            output.level(tick, level)?;
        }
        Ok(())
    }

    fn drive(&mut self, tick: u64, high: bool) -> Result<(), Error> {
        for output in &mut self.0 {
            let level = output.policy.drive(tick, high);
            output.level(tick, level)?;
        }
        Ok(())
    }

    fn finish(self, tick: u64) -> Result<(), Error> {
        for output in self.0 {
            output.finish(tick)?;
        }
        Ok(())
    }
}

struct DeviceTrace {
    speaker: PcSpeaker,
    rows: BufWriter<std::fs::File>,
    sequence: u64,
    audio: Option<PairedAudio>,
}

impl DeviceTrace {
    fn record(
        &mut self,
        events: Events,
        call: u32,
        interrupt: u32,
        order: u64,
    ) -> Result<(), Error> {
        for edge in events.pit {
            self.sequence += 1;
            writeln!(
                self.rows,
                "{}\t{}\t{call}\t{interrupt}\t{order}\tpit\t{}\t{}",
                self.sequence,
                events.tick,
                edge.channel,
                u8::from(edge.high)
            )?;
        }
        if let Some(edge) = events.drive {
            if let Some(audio) = &mut self.audio {
                audio.drive(edge.tick, edge.high)?;
            }
            self.sequence += 1;
            writeln!(
                self.rows,
                "{}\t{}\t{call}\t{interrupt}\t{order}\tdrive\t\t{}",
                self.sequence,
                events.tick,
                u8::from(edge.high)
            )?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        self.rows.flush()?;
        if let Some(audio) = self.audio.take() {
            audio.finish(self.speaker.pit().tick())?;
        }
        Ok(())
    }
}

struct DiagnosticBus<P: Write, E: Write> {
    reads: VecDeque<DeviceRead>,
    ports: P,
    effects: E,
    call: u32,
    interrupt: u32,
    ticks: u64,
    order: u64,
    failure: Option<Error>,
    device: Option<DeviceTrace>,
}

impl<P: Write, E: Write> DiagnosticBus<P, E> {
    fn new(reads: VecDeque<DeviceRead>, mut ports: P, mut effects: E) -> Result<Self, Error> {
        writeln!(
            ports,
            "order\tcall\tinterrupt\tnominal_pit_ticks\tdirection\tport\tvalue\tplayback\tvisit\tresource\toffset\tbyte\torigin"
        )?;
        writeln!(effects, "order\tcall\tinterrupt\tnominal_pit_ticks\taction")?;
        Ok(Self {
            reads,
            ports,
            effects,
            call: 0,
            interrupt: 0,
            ticks: 0,
            order: 0,
            failure: None,
            device: None,
        })
    }

    fn check(&mut self) -> Result<(), Error> {
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        Ok(())
    }

    fn begin_call(&mut self, call: u32) -> Result<(), Error> {
        self.call = call;
        self.interrupt = 0;
        if self
            .reads
            .front()
            .is_none_or(|read| (read.call, read.interrupt, read.port) != (call, 0, 0x61))
        {
            return Err(invalid(format!(
                "missing initial device context for call {call}"
            )));
        }
        Ok(())
    }

    fn keyboard_due(&self) -> bool {
        self.reads.front().is_some_and(|read| {
            (read.call, read.interrupt, read.port) == (self.call, self.interrupt, 0x60)
        })
    }

    fn end_call(&mut self) -> Result<(), Error> {
        self.check()?;
        if self
            .reads
            .front()
            .is_some_and(|read| read.call <= self.call)
        {
            return Err(invalid(format!(
                "unconsumed device context exceeds call {} interrupt bound",
                self.call
            )));
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        self.check()?;
        if !self.reads.is_empty() {
            return Err(invalid(
                "unconsumed context remains after bounded execution",
            ));
        }
        self.ports.flush()?;
        self.effects.flush()?;
        if let Some(device) = &mut self.device {
            device.finish()?;
        }
        Ok(())
    }

    fn await_device_irq0(&mut self) -> Result<(), Error> {
        let device = self.device.as_mut().expect("device mode");
        loop {
            let deadline = device
                .speaker
                .pit()
                .next_deadline()
                .ok_or_else(|| invalid("sample timer has no next device deadline"))?;
            let events = device
                .speaker
                .advance_to(deadline)
                .map_err(|error| invalid(error.to_string()))?;
            self.ticks = events.tick;
            let irq0 = events.pit.iter().any(|edge| edge.channel == 0 && edge.high);
            device.record(events, self.call, self.interrupt, self.order)?;
            if irq0 {
                return Ok(());
            }
        }
    }
}

impl<P: Write, E: Write> SampleBus for DiagnosticBus<P, E> {
    fn read_port(&mut self, port: u16) -> u8 {
        if self.failure.is_some() {
            return 0;
        }
        if let Some(read) = self.reads.front().copied() {
            if (read.call, read.interrupt, read.port) == (self.call, self.interrupt, port) {
                if port == 0x61
                    && self
                        .device
                        .as_ref()
                        .is_some_and(|device| device.speaker.control() & 3 != read.value & 3)
                {
                    self.failure = Some(invalid(
                        "port61 context disagrees with simulated speaker-control bits",
                    ));
                    return 0;
                }
                self.reads.pop_front();
                return read.value;
            }
        }
        self.failure = Some(invalid(format!(
            "missing or mismatched device read at call {}, interrupt {}, port {port:#x}",
            self.call, self.interrupt
        )));
        0
    }

    fn emit(&mut self, action: SoundAction) {
        if self.failure.is_some() {
            return;
        }
        self.order += 1;
        if let Some(audio) = self
            .device
            .as_mut()
            .and_then(|device| device.audio.as_mut())
        {
            if let Err(error) = audio.action(self.ticks, action) {
                self.failure = Some(error);
                return;
            }
        }
        if let SoundAction::Write { port, value, .. } = action {
            if let Some(device) = &mut self.device {
                if (0x40..=0x43).contains(&port) || port == 0x61 {
                    let result = device
                        .speaker
                        .write_port(port, value)
                        .map_err(|error| invalid(error.to_string()))
                        .and_then(|events| {
                            device.record(events, self.call, self.interrupt, self.order)
                        });
                    if let Err(error) = result {
                        self.failure = Some(error);
                        return;
                    }
                }
            }
        }
        let result = match action {
            SoundAction::Read { port, value }
            | SoundAction::Write {
                port,
                value,
                source: None,
            } => {
                let direction = if matches!(action, SoundAction::Read { .. }) {
                    "read"
                } else {
                    "write"
                };
                writeln!(
                    self.ports,
                    "{}\t{}\t{}\t{}\t{direction}\t{port}\t{value}\t\t\t\t\t\t",
                    self.order, self.call, self.interrupt, self.ticks
                )
            }
            SoundAction::Write {
                port,
                value,
                source: Some(source),
            } => {
                writeln!(
                    self.ports,
                    "{}\t{}\t{}\t{}\twrite\t{port}\t{value}\t{}\t{}\t{}\t{}\t{}\t{:?}",
                    self.order,
                    self.call,
                    self.interrupt,
                    self.ticks,
                    source.playback,
                    source.visit,
                    source.resource.name(),
                    source.offset,
                    source.value,
                    source.origin
                )
            }
            SoundAction::InterruptReturn => Ok(()),
            effect => writeln!(
                self.effects,
                "{}\t{}\t{}\t{}\t{effect:?}",
                self.order, self.call, self.interrupt, self.ticks
            ),
        };
        if let Err(error) = result {
            self.failure = Some(error.into());
        }
    }
}

fn execute_call(
    call: IntroCall,
    state: &mut SampleSoundState,
    di: u16,
    memory: &SampleBytes<'_>,
    bus: &mut DiagnosticBus<impl Write, impl Write>,
) -> Result<u16, Error> {
    bus.order += 1;
    writeln!(
        bus.effects,
        "{}\t{}\t{}\t{}\tStateBeforeBegin {{ disabled: {}, last_scan: {}, di: {}, keyboard_words: {:?} }}",
        bus.order,
        bus.call,
        bus.interrupt,
        bus.ticks,
        state.disabled,
        state.last_scan,
        di,
        state.keyboard_words
    )?;
    // Diagnostic vectors are explicitly synthetic. IRQ restoration is exported
    // as an effect; these values are not claimed to reconstruct DOS boot state.
    let environment = SampleEnvironment {
        game_code_segment: 0,
        timer_handler: FarPointer {
            segment: 0,
            offset: 0,
        },
        keyboard_handler: FarPointer {
            segment: 0,
            offset: 0,
        },
        di,
    };
    let started = SamplePlayer::begin(u64::from(bus.call), call.request, state, environment, bus);
    bus.check()?;
    let StartResult::Playing(mut player) = started else {
        return Err(invalid(
            "diagnostic requires the unsupported regular keyboard poll at 73b2",
        ));
    };
    while !player.completed() {
        bus.interrupt += 1;
        if bus.interrupt > 2 * u32::from(call.request.count) {
            return Err(invalid("sample exceeded its bounded interrupt count"));
        }
        if bus.device.is_some() {
            bus.await_device_irq0()?;
        }
        if bus.keyboard_due() {
            player.keyboard_interrupt(state, bus)?;
            bus.check()?;
        }
        if bus.device.is_none() {
            bus.ticks += 60;
        }
        player.timer_interrupt(memory, bus)?;
        bus.check()?;
    }
    // Reject undelivered keyboard reads before creating a successful return.
    bus.end_call()?;
    let result = player.finish(state, bus)?;
    bus.check()?;
    Ok(result)
}

struct Options {
    context_path: PathBuf,
    external: Vec<(Resource, u16, u8)>,
    directory: PathBuf,
    destination: PathBuf,
    device_mode: bool,
    audio: Option<Config>,
}

fn parse_options(args: impl IntoIterator<Item = OsString>) -> Result<Options, Error> {
    let mut args = args.into_iter().peekable();
    let mut context_path = None;
    let mut external = Vec::new();
    let mut bound = BTreeSet::new();
    let mut device_mode = false;
    let mut wav = false;
    let mut sample_rate = None;
    let mut chunk_ticks = None;
    while args
        .peek()
        .is_some_and(|arg| arg.to_str().is_some_and(|text| text.starts_with('-')))
    {
        let option = args.next().unwrap();
        if option == "--device" {
            if device_mode {
                return Err(invalid("--device may be specified only once"));
            }
            device_mode = true;
        } else if option == "--wav" {
            if wav {
                return Err(invalid("--wav may be specified only once"));
            }
            wav = true;
        } else if option == "--sample-rate" || option == "--audio-chunk-ticks" {
            let target = if option == "--sample-rate" {
                &mut sample_rate
            } else {
                &mut chunk_ticks
            };
            if target.is_some() {
                return Err(invalid(format!(
                    "{} may be specified only once",
                    option.to_string_lossy()
                )));
            }
            let value = args.next().ok_or_else(|| {
                invalid(format!(
                    "{} requires a positive decimal integer",
                    option.to_string_lossy()
                ))
            })?;
            let value = value
                .to_str()
                .and_then(|text| {
                    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
                        .then(|| text.parse::<u32>().ok())
                        .flatten()
                })
                .filter(|&value| value != 0)
                .ok_or_else(|| {
                    invalid(format!(
                        "{} requires a positive decimal integer",
                        option.to_string_lossy()
                    ))
                })?;
            *target = Some(value);
        } else if option == "--context" {
            if context_path.is_some() {
                return Err(invalid("--context may be specified only once"));
            }
            context_path = Some(PathBuf::from(
                args.next()
                    .ok_or_else(|| invalid("--context requires a TSV path"))?,
            ));
        } else if option == "--external-byte" {
            let text = args
                .next()
                .ok_or_else(|| invalid("--external-byte requires RESOURCE:OFFSET:VALUE"))?;
            let binding = external_byte(
                text.to_str()
                    .ok_or_else(|| invalid("external byte binding must be UTF-8"))?,
            )?;
            if !bound.insert((binding.0, binding.1)) {
                return Err(invalid("duplicate external byte binding"));
            }
            external.push(binding);
        } else {
            return Err(invalid(format!(
                "unknown option {}\n{HELP}",
                option.to_string_lossy()
            )));
        }
    }
    let directory = args.next().map(PathBuf::from);
    let destination = args.next().map(PathBuf::from);
    let (Some(directory), Some(destination), None) = (directory, destination, args.next()) else {
        return Err(invalid(HELP));
    };
    let context_path = context_path.ok_or_else(|| invalid("--context is required"))?;
    if wav && !device_mode {
        return Err(invalid("--wav requires --device"));
    }
    if !wav && (sample_rate.is_some() || chunk_ticks.is_some()) {
        return Err(invalid(
            "--sample-rate and --audio-chunk-ticks require --wav",
        ));
    }
    let audio = wav.then_some(Config {
        clock_hz: PIT_CLOCK_HZ,
        sample_rate: sample_rate.unwrap_or(DEFAULT_SAMPLE_RATE),
        chunk_ticks: chunk_ticks.unwrap_or(DEFAULT_AUDIO_CHUNK_TICKS),
    });
    // Validate the renderer configuration before creating any output directory.
    if let Some(config) = audio {
        BlipRenderer::new(config, 0, 0).map_err(|error| invalid(error.to_string()))?;
    }
    Ok(Options {
        context_path,
        external,
        directory,
        destination,
        device_mode,
        audio,
    })
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
    let Options {
        context_path,
        external,
        directory,
        destination,
        device_mode,
        audio,
    } = parse_options(args)?;
    let context = parse_context(&fs::read_to_string(context_path)?)?;
    let assets = SoundAssets::load(&directory)?;
    let expanded = mach3::executable::load_program(&directory)?;
    let program = IntroProgram::from_expanded(&expanded)?;
    let mut memory = SampleBytes::new(&assets);
    for (resource, offset, value) in external {
        memory.bind_external_byte(resource, offset, value)?;
    }
    fs::create_dir(&destination)?;
    let mut calls = new_file(destination.join("calls.tsv"))?;
    writeln!(
        calls,
        "call\tphase\tresource\tcount\treverse\toffset\treturn_ip\tresult\tinterrupts"
    )?;
    let mut bus = DiagnosticBus::new(
        context,
        new_file(destination.join("ports.tsv"))?,
        new_file(destination.join("effects.tsv"))?,
    )?;
    if device_mode {
        let control = bus.reads.front().expect("validated device context").value;
        let speaker = PcSpeaker::with_mode3_count_one(
            control,
            [true; 3],
            reforge_audio::pit::CountOnePolicy::DosBoxWrap,
        );
        let mut rows = new_file(destination.join("device.tsv"))?;
        writeln!(
            rows,
            "sequence\ttick\tcall\tinterrupt\taction_order\tkind\tchannel\thigh"
        )?;
        writeln!(
            rows,
            "1\t0\t0\t0\t0\tdrive-initial\t\t{}",
            u8::from(speaker.drive())
        )?;
        for channel in 0..3 {
            writeln!(
                rows,
                "{}\t0\t0\t0\t0\tpit-initial\t{channel}\t1",
                channel + 2
            )?;
        }
        let audio = audio
            .map(|config| {
                PairedAudio::new(config, speaker.pit().tick(), speaker.drive(), &destination)
            })
            .transpose()?;
        bus.device = Some(DeviceTrace {
            speaker,
            rows,
            sequence: 4,
            audio,
        });
    }
    let mut description = new_file(destination.join("diagnostic.txt"))?;
    writeln!(
        description,
        "Bounded reconstructed intro and pre-game sample execution.\nCalls are one-based; IRQ0 deliveries are one-based within each call.\nGame CS and saved IRQ vectors are synthetic zero values."
    )?;
    if let Some(config) = audio {
        writeln!(
            description,
            "Paired offline WAVs from one execution: mono signed 16-bit PCM at {} Hz, {} Hz PIT clock, maximum {} ticks per audio chunk.\nAudio renderer speaker (speaker.wav): digital drive maps LOW to 0 and HIGH to 8192.\nAudio renderer enhanced-pcm (enhanced-pcm.wav): each annotated source visit supplies (byte - 128) * 32 + 4096 once; duplicate writes hold the same level.\nEnhanced PCM holds midpoint 4096 before the first source visit and after SourceEnd, suppressing residual sample PWM until the next source begins or foreground speaker reprogramming; other sound follows speaker drive.\nEach WAV uses its own fixed blip_buf filter state and ends at the same committed device tick; no trailing device phase or filter tail is appended.\nThe initial device pins and instruction gaps are synthetic context, not an acoustically validated speaker model.\nNo live audio playback is involved.",
            config.sample_rate, config.clock_hz, config.chunk_ticks
        )?;
    } else {
        writeln!(description, "No renderer or audio output is involved.")?;
    }
    description.flush()?;
    if device_mode {
        writeln!(
            description,
            "Device mode: IRQ0 dispatch follows shared binary 8254 mode-3 OUT0 rising edges.\nInitial pins are synthetic HIGH, channels unprogrammed, gates0/1 HIGH, gate2 from first port61 context.\nWrites occur after integer clock edges with no foreground/ISR instruction gaps.\nKeyboard context is delivered at the numbered IRQ0 edge before its handler.\nUpper port61 read bits remain external context; lower speaker bits are checked.\nThe nominal_pit_ticks column now records completed device clocks.\nNo PIC, interrupt-service latency, host clock or waveform/acoustic comparison is modeled."
        )?;
        description.flush()?;
    } else {
        writeln!(
            description,
            "Nominal mode: IRQ1 context precedes the numbered IRQ0.\nTicks advance only by 60 per IRQ0; device phase, setup gaps, host time and foreground instruction timing are unmodeled."
        )?;
        description.flush()?;
    }
    let mut sequence = IntroSequence::new(&program);
    let mut state = SampleSoundState::default();
    let mut call_number = 0;
    let mut di = 0x3f40;
    while let Some(call) = sequence.current() {
        call_number += 1;
        bus.begin_call(call_number)?;
        let result = execute_call(call, &mut state, di, &memory, &mut bus)?;
        writeln!(
            calls,
            "{call_number}\tintro\t{}\t{}\t{}\t{}\t{}\t{result}\t{}",
            call.request.resource.name(),
            call.request.count,
            u8::from(call.request.reverse),
            call.request.offset,
            call.return_ip,
            bus.interrupt
        )?;
        sequence.complete(result)?;
        di = SamplePlayer::return_di();
    }
    call_number += 1;
    bus.begin_call(call_number)?;
    let call = IntroCall {
        request: program.pregame(),
        return_ip: 0x4fac,
    };
    let result = execute_call(call, &mut state, 0x3f40, &memory, &mut bus)?;
    writeln!(
        calls,
        "{call_number}\tpre-game\t{}\t{}\t{}\t{}\t{}\t{result}\t{}",
        call.request.resource.name(),
        call.request.count,
        u8::from(call.request.reverse),
        call.request.offset,
        call.return_ip,
        bus.interrupt
    )?;
    bus.finish()?;
    calls.flush()?;
    println!(
        "Wrote {call_number} sample calls to {}",
        destination.display()
    );
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mach3-sound: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(prefix: &[&str]) -> Result<Options, Error> {
        parse_options(
            prefix
                .iter()
                .chain(["--context", "context.tsv", "assets", "new-output"].iter())
                .map(OsString::from),
        )
    }

    #[test]
    fn wav_options_require_device_and_validate_integer_configuration() {
        assert!(options(&[]).unwrap().audio.is_none());
        assert!(options(&["--device"]).unwrap().audio.is_none());
        let parsed = options(&["--wav", "--device"]).unwrap();
        assert!(parsed.device_mode);
        assert_eq!(
            parsed.audio.unwrap(),
            Config {
                clock_hz: PIT_CLOCK_HZ,
                sample_rate: 48_000,
                chunk_ticks: 11_931,
            }
        );
        let parsed = options(&[
            "--device",
            "--wav",
            "--sample-rate",
            "44100",
            "--audio-chunk-ticks",
            "1000",
        ])
        .unwrap();
        assert_eq!(parsed.audio.unwrap().sample_rate, 44_100);
        assert_eq!(parsed.audio.unwrap().chunk_ticks, 1_000);
        for prefix in [
            vec!["--wav"],
            vec!["--device", "--sample-rate", "48000"],
            vec!["--device", "--audio-chunk-ticks", "1000"],
            vec!["--device", "--wav", "--wav"],
            vec![
                "--device",
                "--wav",
                "--sample-rate",
                "48000",
                "--sample-rate",
                "48000",
            ],
            vec![
                "--device",
                "--wav",
                "--audio-chunk-ticks",
                "1",
                "--audio-chunk-ticks",
                "1",
            ],
        ] {
            assert!(options(&prefix).is_err(), "{prefix:?}");
        }
        for option in ["--sample-rate", "--audio-chunk-ticks"] {
            for value in ["0", "-1", "0x100", "1.5", "+1", "4294967296", "invalid"] {
                assert!(options(&["--device", "--wav", option, value]).is_err());
            }
            assert!(options(&["--device", "--wav", option]).is_err());
        }
        assert!(options(&["--device", "--wav", "--sample-rate", "4294967295"]).is_err());
        assert!(options(&["--device", "--wav", "--audio-chunk-ticks", "4294967295"]).is_err());
    }

    #[test]
    fn context_requires_ordered_initial_reads_and_keyboard_pairs() {
        let text = "call\tinterrupt\tport\tvalue\n1\t0\t0x61\t0x63\n1\t2\t0x60\t0xb9\n1\t2\t0x61\t99\n2\t0\t97\t0\n";
        let rows = parse_context(text).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[1].value, 0xb9);
        for body in [
            "0\t0\t97\t0",
            "1\t0\t96\t0",
            "1\t0\t97\t256",
            "1\t0\t97\t0\n1\t0\t97\t0",
            "1\t0\t97\t0\n3\t0\t97\t0",
            "1\t0\t97\t0\n1\t1\t96\t57",
            "1\t0\t97\t0\n1\t1\t96\t57\n1\t2\t97\t0",
        ] {
            assert!(parse_context(&format!("call\tinterrupt\tport\tvalue\n{body}\n")).is_err());
        }
    }

    #[test]
    fn device_context_is_consumed_only_at_its_matching_delivery() {
        let reads = parse_context(
            "call\tinterrupt\tport\tvalue\n1\t0\t97\t99\n1\t4\t96\t185\n1\t4\t97\t99\n",
        )
        .unwrap();
        let mut bus = DiagnosticBus::new(reads, Vec::new(), Vec::new()).unwrap();
        bus.begin_call(1).unwrap();
        assert_eq!(bus.read_port(0x61), 99);
        assert!(!bus.keyboard_due());
        bus.interrupt = 4;
        assert!(bus.keyboard_due());
        assert_eq!(bus.read_port(0x60), 185);
        assert_eq!(bus.read_port(0x61), 99);
        bus.end_call().unwrap();
        bus.finish().unwrap();
        assert_eq!(bus.read_port(0x61), 0);
        assert!(bus.check().is_err());
    }

    #[test]
    fn unconsumed_keyboard_rows_and_byte_overflows_fail() {
        let reads = parse_context(
            "call\tinterrupt\tport\tvalue\n1\t0\t97\t99\n1\t4\t96\t185\n1\t4\t97\t99\n",
        )
        .unwrap();
        let mut bus = DiagnosticBus::new(reads, Vec::new(), Vec::new()).unwrap();
        bus.begin_call(1).unwrap();
        bus.read_port(0x61);
        assert!(bus.end_call().is_err());
        assert!(bus.finish().is_err());
        assert!(external_byte("DATAB:15500:256").is_err());
        assert!(external_byte("DATAB:65536:51").is_err());
        assert_eq!(
            external_byte("DATAB:0x3c8c:0x33").unwrap(),
            (Resource::DataB, 15500, 51)
        );
    }

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "mach3-sample-device-{}-{time}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn paired_wavs_observe_one_sample_execution_without_changing_hardware_or_chunk_results() {
        let scratch = Scratch::new();
        for resource in Resource::ALL {
            let mut bytes = vec![0; resource.bytes()];
            if resource == Resource::Data7 {
                for (index, byte) in bytes[..128].iter_mut().enumerate() {
                    *byte = [0, 128, 255, 64][index % 4];
                }
            }
            fs::write(scratch.0.join(resource.name()), bytes).unwrap();
        }
        let assets = SoundAssets::load(&scratch.0).unwrap();
        let memory = SampleBytes::new(&assets);
        struct Execution {
            ports: Vec<u8>,
            effects: Vec<u8>,
            device: Vec<u8>,
            tick: u64,
            wavs: Option<[Vec<u8>; 2]>,
        }
        let execute = |name: &str, chunk_ticks: Option<u32>| {
            let destination = scratch.0.join(name);
            fs::create_dir(&destination).unwrap();
            let reads = parse_context(
                "call\tinterrupt\tport\tvalue\n1\t0\t97\t3\n1\t3\t96\t185\n1\t3\t97\t3\n",
            )
            .unwrap();
            let mut bus = DiagnosticBus::new(reads, Vec::new(), Vec::new()).unwrap();
            bus.device = Some(DeviceTrace {
                speaker: PcSpeaker::with_mode3_count_one(
                    3,
                    [true; 3],
                    reforge_audio::pit::CountOnePolicy::DosBoxWrap,
                ),
                rows: new_file(destination.join("device.tsv")).unwrap(),
                sequence: 0,
                audio: chunk_ticks.map(|chunk_ticks| {
                    PairedAudio::new(
                        Config {
                            clock_hz: PIT_CLOCK_HZ,
                            sample_rate: 48_000,
                            chunk_ticks,
                        },
                        0,
                        true,
                        &destination,
                    )
                    .unwrap()
                }),
            });
            bus.begin_call(1).unwrap();
            execute_call(
                IntroCall {
                    request: mach3::sound::SampleRequest {
                        resource: Resource::Data7,
                        offset: 0,
                        count: 128,
                        reverse: false,
                    },
                    return_ip: 0,
                },
                &mut SampleSoundState::default(),
                0,
                &memory,
                &mut bus,
            )
            .unwrap();
            bus.finish().unwrap();
            let wavs = chunk_ticks.map(|_| {
                ["speaker.wav", "enhanced-pcm.wav"]
                    .map(|name| fs::read(destination.join(name)).unwrap())
            });
            Execution {
                ports: bus.ports,
                effects: bus.effects,
                device: fs::read(destination.join("device.tsv")).unwrap(),
                tick: bus.ticks,
                wavs,
            }
        };
        let hardware = execute("hardware", None);
        let small = execute("small", Some(100));
        let large = execute("large", Some(DEFAULT_AUDIO_CHUNK_TICKS));
        for rendered in [&small, &large] {
            assert_eq!(rendered.ports, hardware.ports);
            assert_eq!(rendered.effects, hardware.effects);
            assert_eq!(rendered.device, hardware.device);
            assert_eq!(rendered.tick, hardware.tick);
        }
        // The source routine emits two annotated writes for each source visit.
        let annotated = std::str::from_utf8(&hardware.ports)
            .unwrap()
            .lines()
            .filter(|line| line.contains("\tDATA7\t"))
            .count();
        assert_eq!(annotated, 256);
        let [speaker, enhanced] = small.wavs.as_ref().unwrap();
        assert_eq!(small.wavs, large.wavs);
        assert_ne!(speaker[44..], enhanced[44..]);
        let samples = (hardware.tick * 48_000 / u64::from(PIT_CLOCK_HZ)) as usize;
        for wav in [speaker, enhanced] {
            assert_eq!(wav.len(), 44 + samples * 2);
            assert_eq!(
                u32::from_le_bytes(wav[40..44].try_into().unwrap()),
                samples as u32 * 2
            );
        }
        // Replay only the shared hardware's drive edges through the original
        // speaker mapping, independently of either rendering policy.
        let mut reference = BlipRenderer::new(
            Config {
                clock_hz: PIT_CLOCK_HZ,
                sample_rate: 48_000,
                chunk_ticks: 100,
            },
            0,
            8192,
        )
        .unwrap();
        let mut reference_pcm = Vec::new();
        let mut collect = |samples: &[i16]| {
            reference_pcm.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
            Ok(())
        };
        for line in std::str::from_utf8(&hardware.device).unwrap().lines() {
            let columns: Vec<_> = line.split('\t').collect();
            if columns[5] == "drive" {
                reference
                    .level_at(
                        columns[1].parse().unwrap(),
                        if columns[7] == "1" { 8192 } else { 0 },
                        &mut collect,
                    )
                    .unwrap();
            }
        }
        reference.finish_until(hardware.tick, &mut collect).unwrap();
        assert_eq!(speaker[44..], reference_pcm);
    }

    #[test]
    fn device_irq_execution_preserves_sample_and_keyboard_operations_and_leaves_timer_running() {
        let scratch = Scratch::new();
        for resource in Resource::ALL {
            let mut bytes = vec![0; resource.bytes()];
            if resource == Resource::Data7 {
                bytes[..3].copy_from_slice(&[128, 0, 255]);
            }
            fs::write(scratch.0.join(resource.name()), bytes).unwrap();
        }
        let assets = SoundAssets::load(&scratch.0).unwrap();
        let memory = SampleBytes::new(&assets);
        let reads = parse_context(
            "call\tinterrupt\tport\tvalue\n1\t0\t97\t3\n1\t3\t96\t185\n1\t3\t97\t3\n",
        )
        .unwrap();
        let mut nominal = DiagnosticBus::new(reads.clone(), Vec::new(), Vec::new()).unwrap();
        let mut device = DiagnosticBus::new(reads, Vec::new(), Vec::new()).unwrap();
        device.device = Some(DeviceTrace {
            speaker: PcSpeaker::new(3, [true; 3]),
            rows: new_file(scratch.0.join("device.tsv")).unwrap(),
            sequence: 0,
            audio: Some(
                PairedAudio::new(
                    Config {
                        clock_hz: PIT_CLOCK_HZ,
                        sample_rate: 48_000,
                        chunk_ticks: 100,
                    },
                    0,
                    true,
                    &scratch.0,
                )
                .unwrap(),
            ),
        });
        let call = IntroCall {
            request: mach3::sound::SampleRequest {
                resource: Resource::Data7,
                offset: 0,
                count: 3,
                reverse: false,
            },
            return_ip: 0,
        };
        let mut nominal_state = SampleSoundState::default();
        let mut device_state = SampleSoundState::default();
        nominal.begin_call(1).unwrap();
        device.begin_call(1).unwrap();
        assert_eq!(
            execute_call(call, &mut nominal_state, 0, &memory, &mut nominal).unwrap(),
            185
        );
        assert_eq!(
            execute_call(call, &mut device_state, 0, &memory, &mut device).unwrap(),
            185
        );
        nominal.finish().unwrap();
        device.finish().unwrap();
        assert_eq!(nominal.interrupt, device.interrupt);
        assert_eq!(nominal_state.last_scan, device_state.last_scan);
        assert_eq!(nominal_state.disabled, device_state.disabled);
        assert_eq!(nominal_state.keyboard_words, device_state.keyboard_words);
        fn without_ticks(bytes: &[u8]) -> Vec<Vec<String>> {
            std::str::from_utf8(bytes)
                .unwrap()
                .lines()
                .map(|line| {
                    line.split('\t')
                        .enumerate()
                        .filter(|(i, _)| *i != 3)
                        .map(|(_, s)| s.to_owned())
                        .collect()
                })
                .collect()
        }
        assert_eq!(without_ticks(&nominal.ports), without_ticks(&device.ports));
        assert_eq!(device.ticks, 361); // Load at1; rising OUT0 at61 then every60.
        let trace = device.device.as_mut().unwrap();
        assert_eq!(trace.speaker.pit().tick(), 361);
        assert!(trace.audio.is_none()); // Explicit completion consumed the WAV writer.
        let wav = fs::read(scratch.0.join("speaker.wav")).unwrap();
        let data_bytes = u32::from_le_bytes(wav[40..44].try_into().unwrap());
        let samples = (361_u64 * 48_000 / u64::from(PIT_CLOCK_HZ)) as u32;
        assert_eq!(data_bytes, samples * 2); // No invented trailing device time/filter tail.
        assert_eq!(wav.len(), 44 + data_bytes as usize);
        assert!(wav[44..].iter().any(|&byte| byte != 0));
        let restored = trace.speaker.pit().channel(0).unwrap();
        assert_eq!(restored.programmed_count, Some(0x4c68));
        assert!(restored.pending_load);
        assert!(!trace.speaker.drive());
        // Foreground return did NOT stop channel2. Last count1 takes effect later.
        let load = trace.speaker.advance_to(363).unwrap();
        assert_eq!(load.tick, 362);
        assert!(trace.speaker.advance_to(363).unwrap().drive.unwrap().high);
        assert_eq!(
            trace.speaker.pit().channel(0).unwrap().counter,
            Some(0x4c66) // Mode3 decrements its even count by two per clock.
        );
    }
}
