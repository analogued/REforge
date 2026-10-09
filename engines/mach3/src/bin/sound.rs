// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

use mach3::{
    Error,
    sound::{
        FarPointer, IntroCall, IntroProgram, IntroSequence, Resource, SampleBus, SampleBytes,
        SampleEnvironment, SamplePlayer, SampleSoundState, SoundAction, SoundAssets, StartResult,
    },
};
use std::{
    collections::{BTreeSet, VecDeque},
    env,
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

const HELP: &str = "Usage: mach3-sound --context INPUT.tsv [--external-byte RESOURCE:OFFSET:VALUE ...] <assets-directory> <new-output-directory>\n\n\
Export one bounded intro traversal followed by its pre-game sample.\n\
Context header: call<TAB>interrupt<TAB>port<TAB>value. Numbers are decimal or 0x hexadecimal.\n\
Calls start at 1. Each call requires one port 0x61 read at interrupt 0.\n\
Keyboard events are paired port 0x60/0x61 reads at the same interrupt >=1,\n\
delivered before that numbered IRQ0. Context contains device reads, not sample requests.\n\
Writes calls.tsv, ports.tsv, effects.tsv and diagnostic.txt; destination must be new.\n\
Nominal time advances by 60 PIT ticks per delivered IRQ0. This does not model\n\
setup gaps, PIT phase, host time, foreground instruction timing or rendered audio.";

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

struct DiagnosticBus<P: Write, E: Write> {
    reads: VecDeque<DeviceRead>,
    ports: P,
    effects: E,
    call: u32,
    interrupt: u32,
    ticks: u64,
    order: u64,
    failure: Option<Error>,
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
        Ok(())
    }
}

impl<P: Write, E: Write> SampleBus for DiagnosticBus<P, E> {
    fn read_port(&mut self, port: u16) -> u8 {
        if self.failure.is_some() {
            return 0;
        }
        if let Some(read) = self.reads.front().copied() {
            if (read.call, read.interrupt, read.port) == (self.call, self.interrupt, port) {
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
        if bus.keyboard_due() {
            player.keyboard_interrupt(state, bus)?;
            bus.check()?;
        }
        bus.ticks += 60;
        player.timer_interrupt(memory, bus)?;
        bus.check()?;
    }
    // Reject undelivered keyboard reads before creating a successful return.
    bus.end_call()?;
    let result = player.finish(state, bus)?;
    bus.check()?;
    Ok(result)
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
    let mut context_path = None;
    let mut external = Vec::new();
    let mut bound = BTreeSet::new();
    while args
        .peek()
        .is_some_and(|arg| arg.to_str().is_some_and(|text| text.starts_with('-')))
    {
        let option = args.next().unwrap();
        if option == "--context" {
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
    let context = parse_context(&fs::read_to_string(context_path)?)?;
    let assets = SoundAssets::load(&directory)?;
    let expanded = mach3::rules::load_program(&directory)?;
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
    let mut description = new_file(destination.join("diagnostic.txt"))?;
    writeln!(
        description,
        "Bounded reconstructed intro and pre-game sample execution.\nCalls are one-based; IRQ0 deliveries are one-based within each call.\nContext IRQ1 reads occur before the numbered IRQ0.\nNominal ticks advance only by 60 per IRQ0; setup gaps, device phase, host time and foreground instruction timing are unmodeled.\nGame CS and saved IRQ vectors are synthetic zero values.\nNo renderer or audio output is involved."
    )?;
    description.flush()?;
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
}
