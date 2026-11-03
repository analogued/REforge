// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded headless execution. Requests suspend until an explicit completion.

use crate::decode::{Instruction, OperandMode, decode_with_rules};
use crate::image::StoryImage;
use crate::interpreter::{ExtensionOpcode, Interpreter, MINDWHEEL_DOS, NativeService};
use crate::{Error, ObjectId, ProgramAddr, VarAddr, read_word};

const NONE: u16 = 0xffff;
const STACK_CAPACITY: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmState {
    pub ip: ProgramAddr,
    pub argument: u16,
    pub operands: [u16; 3],
    pub variables: Vec<u8>,
    pub returns: Vec<u16>,
    pub values: Vec<u16>,
}

#[derive(Clone, Copy, Debug)]
struct Layout {
    accumulator: VarAddr,
    max_object: u16,
    direct_properties: [u16; 2],
    wildcard: u16,
    direct_values: u16,
    records: u16,
    mutable_bias: u16,
    program_bias: u16,
    interval_lower: u16,
    interval_upper: u16,
    interval_bytes: u16,
}

impl Layout {
    fn read(bytes: &[u8]) -> Result<Self, Error> {
        let at = |a| read_word(bytes, a, "layout");
        Ok(Self {
            accumulator: VarAddr(0xce),
            max_object: at(0x102)?,
            direct_properties: [at(0xe2)?, at(0xe4)?],
            wildcard: at(0xe6)?,
            direct_values: at(0x10c)?,
            records: at(0xee)?,
            mutable_bias: at(0x10e)?,
            program_bias: at(0xe0)?,
            interval_lower: at(0xf6)?,
            interval_upper: at(0xf8)?,
            interval_bytes: at(0xfa)?,
        })
    }
}

/// Operations delegated to native algorithms or a host adapter. `Session`
/// implements the verified subset, including the DOS RNG; other services remain
/// explicitly pending. A standalone VM never substitutes a host random source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    PollInput,
    Character { byte: u8 },
    CharacterToken { byte: u8 },
    ResumeText { argument: u16, position: u16 },
    Random { maximum: u16 },
    Message { id: u16 },
    NativeService { service: NativeService },
    ObjectWords { object: ObjectId },
    Decimal { value: u16 },
    RememberObject { object: ObjectId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Step {
    Executed {
        instruction: Instruction,
        argument: u16,
    },
    Suspended(Request),
}

#[derive(Clone, Debug)]
enum Stage {
    Fetch,
    Poll(Instruction),
    Ready(Instruction),
    Request(Request),
}

pub struct Vm {
    image: StoryImage,
    interpreter: &'static Interpreter,
    layout: Layout,
    state: VmState,
    stage: Stage,
    property_found: bool,
    return_slots: [u16; STACK_CAPACITY],
    value_slots: [u16; STACK_CAPACITY],
}

impl Vm {
    pub fn new(image: StoryImage) -> Result<Self, Error> {
        Self::with_interpreter(image, &MINDWHEEL_DOS)
    }

    pub fn with_interpreter(
        image: StoryImage,
        interpreter: &'static Interpreter,
    ) -> Result<Self, Error> {
        let initial = image.initial_variables();
        let mut variables = initial.to_vec();
        variables[..2].copy_from_slice(&NONE.to_le_bytes());
        initialize_variables(&mut variables, interpreter)?;
        let state = VmState {
            ip: ProgramAddr(read_word(initial, 0xe8, "entry")?),
            argument: 0,
            operands: [0; 3],
            variables,
            returns: Vec::new(),
            values: Vec::new(),
        };
        Self::from_state_with_interpreter(image, state, interpreter)
    }

    /// Restores a fetch-boundary checkpoint, not a suspended native operation.
    pub fn from_state(image: StoryImage, state: VmState) -> Result<Self, Error> {
        Self::from_state_with_interpreter(image, state, &MINDWHEEL_DOS)
    }

    pub fn from_state_with_interpreter(
        image: StoryImage,
        state: VmState,
        interpreter: &'static Interpreter,
    ) -> Result<Self, Error> {
        let layout = Layout::read(image.initial_variables())?;
        if state.variables.len() != image.initial_variables().len()
            || state.returns.len() > STACK_CAPACITY
            || state.values.len() > STACK_CAPACITY
        {
            return Err(Error("invalid VM checkpoint dimensions".into()));
        }
        read_word(
            &state.variables,
            usize::from(layout.accumulator.0),
            "accumulator",
        )?;
        let mut return_slots = [0; STACK_CAPACITY];
        return_slots[..state.returns.len()].copy_from_slice(&state.returns);
        let mut value_slots = [0; STACK_CAPACITY];
        value_slots[..state.values.len()].copy_from_slice(&state.values);
        Ok(Self {
            image,
            interpreter,
            layout,
            state,
            stage: Stage::Fetch,
            property_found: false,
            return_slots,
            value_slots,
        })
    }

    pub fn interpreter(&self) -> &'static Interpreter {
        self.interpreter
    }

    pub fn state(&self) -> &VmState {
        &self.state
    }
    /// Native bookmark files include inactive stack slots as well as depth.
    pub fn return_slots(&self) -> &[u16; STACK_CAPACITY] {
        &self.return_slots
    }
    pub fn value_slots(&self) -> &[u16; STACK_CAPACITY] {
        &self.value_slots
    }

    pub fn accumulator(&self) -> u16 {
        let offset = usize::from(self.layout.accumulator.0);
        u16::from_le_bytes([
            self.state.variables[offset],
            self.state.variables[offset + 1],
        ])
    }

    fn set_accumulator(&mut self, value: u16) {
        let offset = usize::from(self.layout.accumulator.0);
        self.state.variables[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub fn variable(&self, address: VarAddr) -> Result<u16, Error> {
        read_word(&self.state.variables, usize::from(address.0), "variables")
    }

    pub fn write_variable(&mut self, address: VarAddr, value: u16) -> Result<(), Error> {
        let value = if address.0 == 0 { NONE } else { value };
        if !address.0.is_multiple_of(2) || (2..10).contains(&address.0) {
            return Err(Error(format!(
                "protected or unaligned variable {:#x}",
                address.0
            )));
        }
        let offset = usize::from(address.0);
        let target = self
            .state
            .variables
            .get_mut(offset..offset + 2)
            .ok_or_else(|| Error(format!("variable write outside image at {offset:#x}")))?;
        target.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub(crate) fn write_buffer(&mut self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        if offset < 10 {
            return Err(Error("protected screen buffer write".into()));
        }
        let end = offset
            .checked_add(bytes.len())
            .ok_or_else(|| Error("screen buffer overflow".into()))?;
        self.state
            .variables
            .get_mut(offset..end)
            .ok_or_else(|| Error("screen buffer outside variable image".into()))?
            .copy_from_slice(bytes);
        Ok(())
    }

    fn pop_return(&mut self) -> Result<u16, Error> {
        self.state
            .returns
            .pop()
            .ok_or_else(|| Error("return stack underflow".into()))
    }

    pub(crate) fn push_text_value(&mut self, value: u16) -> Result<(), Error> {
        Self::push(
            &mut self.state.values,
            &mut self.value_slots,
            value,
            "value",
        )
    }

    /// The four words are native return-stack entries, so bookmarks taken
    /// inside a callback can resume the suspended message without host state.
    pub(crate) fn begin_text_callback(
        &mut self,
        argument: u16,
        position: u16,
        letter: u8,
    ) -> Result<(), Error> {
        if !letter.is_ascii_lowercase() {
            return Err(Error("text callback letter outside a..z".into()));
        }
        if !matches!(
            self.stage,
            Stage::Request(
                Request::Message { .. } | Request::ObjectWords { .. } | Request::ResumeText { .. }
            )
        ) {
            return Err(Error(
                "text callback requires a pending text request".into(),
            ));
        }
        let table = self.variable(VarAddr(0x138))?;
        let target = self.variable(VarAddr(table.wrapping_add(u16::from(letter - b'a') * 2)))?;
        if self.state.returns.len() + 4 > STACK_CAPACITY {
            return Err(Error("text continuation exceeds return stack".into()));
        }
        self.write_variable(VarAddr(0x130), argument)?;
        for word in [position, argument, self.state.ip.0, 0] {
            Self::push(
                &mut self.state.returns,
                &mut self.return_slots,
                word,
                "return",
            )?;
        }
        self.state.ip = ProgramAddr(target);
        self.stage = Stage::Fetch;
        Ok(())
    }

    fn program_word(&self, address: u16) -> Result<u16, Error> {
        self.image.program_word(ProgramAddr(address))
    }

    pub fn step(&mut self) -> Result<Step, Error> {
        let instruction = match &self.stage {
            Stage::Ready(instruction) => instruction.clone(),
            Stage::Poll(_) | Stage::Request(_) => {
                return Err(Error("complete pending request before stepping".into()));
            }
            Stage::Fetch => {
                let instruction =
                    decode_with_rules(self.image.program(), self.state.ip, self.interpreter.forms)?;
                self.state.ip = instruction.next;
                for (index, operand) in instruction.operands.iter().enumerate() {
                    self.state.operands[index] = match operand.mode {
                        OperandMode::Literal => operand.raw,
                        OperandMode::Variable => self.variable(VarAddr(operand.raw))?,
                        OperandMode::IndirectVariable => {
                            self.variable(VarAddr(self.variable(VarAddr(operand.raw))?))?
                        }
                        OperandMode::ProgramAtVariable => {
                            self.program_word(self.variable(VarAddr(operand.raw))?)?
                        }
                    };
                }
                if instruction.form == 1 {
                    self.stage = Stage::Poll(instruction);
                    return Ok(Step::Suspended(Request::PollInput));
                }
                self.resolve_form(instruction.form)?;
                instruction
            }
        };
        self.stage = Stage::Fetch;
        let argument = self.state.argument;
        let opcode = instruction.opcode;
        let accumulator = self.accumulator();
        let predicate = |test| if test { NONE } else { 0 };
        if (0x05..=0x0a).contains(&opcode)
            && let Some(address) = self.interpreter.comparison_backup
        {
            self.write_variable(address, accumulator)?;
        }
        match opcode {
            0x00 => {
                let callbacks =
                    self.interpreter.text == crate::interpreter::TextExecution::WordcodeCallbacks;
                if callbacks {
                    self.set_accumulator(argument);
                }
                self.state.ip = ProgramAddr(self.pop_return()?);
                if !callbacks {
                    self.set_accumulator(argument);
                }
                if callbacks && self.state.ip.0 == 0 {
                    self.state.ip = ProgramAddr(self.pop_return()?);
                    self.state.argument = self.pop_return()?;
                    self.write_variable(VarAddr(0x130), NONE)?;
                    let position = self.pop_return()?;
                    return Ok(self.suspend(Request::ResumeText {
                        argument: self.state.argument,
                        position,
                    }));
                }
            }
            0x01 => self.call(argument)?,
            0x02 => self.jump(argument),
            0x03 if accumulator == 0 => self.jump(argument),
            0x04 if accumulator == NONE => self.jump(argument),
            0x03 | 0x04 => {}
            0x05 => self.set_accumulator(predicate(accumulator != argument)),
            0x06 => self.set_accumulator(predicate(accumulator == argument)),
            0x07 => self.set_accumulator(predicate(accumulator >= argument)),
            0x08 => self.set_accumulator(predicate(accumulator <= argument)),
            0x09 => self.set_accumulator(predicate(accumulator > argument)),
            0x0a => self.set_accumulator(predicate(accumulator < argument)),
            0x0b => self.set_accumulator(argument),
            0x0c => self.write_variable(VarAddr(argument), accumulator)?,
            0x0d..=0x0f | 0x17 | 0x18 => match self.interpreter.extensions[if opcode <= 0x0f {
                usize::from(opcode - 0x0d)
            } else {
                usize::from(opcode - 0x17) + 3
            }] {
                ExtensionOpcode::NoOp => {}
                ExtensionOpcode::Character => {
                    return Ok(self.suspend(Request::Character {
                        byte: argument as u8,
                    }));
                }
                ExtensionOpcode::CharacterToken => {
                    return Ok(self.suspend(Request::CharacterToken {
                        byte: argument as u8,
                    }));
                }
                ExtensionOpcode::RestoreComparison => {
                    let address = self
                        .interpreter
                        .comparison_backup
                        .ok_or_else(|| Error("missing comparison backup variable".into()))?;
                    self.set_accumulator(self.variable(address)?);
                }
                ExtensionOpcode::LoadByte => {
                    let byte = *self
                        .state
                        .variables
                        .get(usize::from(argument))
                        .ok_or_else(|| Error("byte read outside variable image".into()))?;
                    self.set_accumulator(u16::from(byte));
                }
                ExtensionOpcode::StoreByte => {
                    // Native offset zero is changed to ffff before its bounds test.
                    if argument == 0 || (2..10).contains(&argument) {
                        return Err(Error("protected byte write".into()));
                    }
                    let target = self
                        .state
                        .variables
                        .get_mut(usize::from(argument))
                        .ok_or_else(|| Error("byte write outside variable image".into()))?;
                    *target = accumulator as u8;
                }
            },
            0x10 => self.set_accumulator(accumulator.wrapping_add(argument)),
            0x11 => self.set_accumulator(accumulator.wrapping_sub(argument)),
            0x12 => self.set_accumulator(argument.wrapping_sub(accumulator)),
            0x13 => self.set_accumulator(accumulator.wrapping_mul(argument)),
            0x14 | 0x21 if argument == 0 => return Err(Error("division by zero".into())),
            0x14 => self.set_accumulator(accumulator / argument),
            0x15 => return Ok(self.suspend(Request::Random { maximum: argument })),
            0x16 => return Ok(self.suspend(Request::Message { id: argument })),
            0x19 => match self.interpreter.service(argument) {
                Some(NativeService::ResetStacks) => {
                    self.state.returns.clear();
                    self.state.values.clear();
                }
                Some(NativeService::NoOp) => {}
                Some(service) => return Ok(self.suspend(Request::NativeService { service })),
                None => return Err(Error(format!("invalid native service {argument}"))),
            },
            0x1a => {
                return Ok(self.suspend(Request::ObjectWords {
                    object: ObjectId(argument),
                }));
            }
            0x1b => Self::push(
                &mut self.state.values,
                &mut self.value_slots,
                argument,
                "value",
            )?,
            0x1c => {
                let value = self
                    .state
                    .values
                    .pop()
                    .ok_or_else(|| Error("value stack underflow".into()))?;
                self.write_variable(VarAddr(argument), value)?;
            }
            0x1d => {
                let mut offset = 0;
                while offset < self.layout.interval_bytes {
                    let lower =
                        self.variable(VarAddr(self.layout.interval_lower.wrapping_add(offset)))?;
                    let upper =
                        self.variable(VarAddr(self.layout.interval_upper.wrapping_add(offset)))?;
                    if lower <= argument && upper >= argument {
                        break;
                    }
                    offset = offset.wrapping_add(2);
                }
                self.set_accumulator(offset);
            }
            0x1e if accumulator != 0 && accumulator != NONE => self.jump(argument),
            0x1e => {}
            0x1f => return Ok(self.suspend(Request::Decimal { value: argument })),
            0x20 => {
                return Ok(self.suspend(Request::RememberObject {
                    object: ObjectId(argument),
                }));
            }
            0x21 => self.set_accumulator(accumulator % argument),
            0x22 => self.set_accumulator(accumulator & argument),
            _ => unreachable!("decoder validates opcode"),
        }
        Ok(Step::Executed {
            instruction,
            argument,
        })
    }

    fn suspend(&mut self, request: Request) -> Step {
        self.stage = Stage::Request(request.clone());
        Step::Suspended(request)
    }

    /// Service 1 redirects lexical misses through the story's own handler,
    /// preserving the interrupted return address before completing the service.
    pub(crate) fn call_from_parser(&mut self, address: ProgramAddr) -> Result<(), Error> {
        if !matches!(
            self.stage,
            Stage::Request(Request::NativeService {
                service: NativeService::ReadParse
            })
        ) {
            return Err(Error(
                "parser redirect requires a pending read service".into(),
            ));
        }
        self.call(address.0)
    }

    /// DOS bookmark restore replaces variables and active stacks while retaining
    /// the load instruction's IP, decoded operands and pending host operation.
    pub(crate) fn restore_bookmark_parts(
        &mut self,
        variables: &[u8],
        returns: &[u16],
        values: &[u16],
        return_slots: &[u16; STACK_CAPACITY],
        value_slots: &[u16; STACK_CAPACITY],
    ) -> Result<(), Error> {
        if !matches!(
            self.stage,
            Stage::Request(Request::NativeService {
                service: NativeService::Restore
            })
        ) {
            return Err(Error(
                "bookmark restore requires a pending restore service".into(),
            ));
        }
        if variables.len() != self.state.variables.len()
            || returns.len() > STACK_CAPACITY
            || values.len() > STACK_CAPACITY
        {
            return Err(Error("bookmark dimensions do not match the VM".into()));
        }
        // Restore uses the native raw writer, not wordcode's protected setter.
        self.state.variables.copy_from_slice(variables);
        self.state.returns = returns.to_vec();
        self.state.values = values.to_vec();
        self.return_slots = *return_slots;
        self.value_slots = *value_slots;
        Ok(())
    }

    /// Native disk/reset service11 reloads initial variables and constants only.
    pub(crate) fn reset_initial_variables(&mut self) -> Result<(), Error> {
        if !matches!(
            self.stage,
            Stage::Request(Request::NativeService {
                service: NativeService::ResetVariables
            })
        ) {
            return Err(Error(
                "initial-variable reload requires native service11".into(),
            ));
        }
        self.state
            .variables
            .copy_from_slice(self.image.initial_variables());
        self.state.variables[..2].copy_from_slice(&NONE.to_le_bytes());
        initialize_variables(&mut self.state.variables, self.interpreter)?;
        Ok(())
    }

    /// Completes a request after the native algorithm or host adapter has run.
    /// PollInput carries no result; Session services its keyboard opportunity.
    pub fn resume(&mut self, value: Option<u16>) -> Result<(), Error> {
        match self.stage.clone() {
            Stage::Poll(instruction) => {
                if value.is_some() {
                    return Err(Error("native keyboard processing not implemented".into()));
                }
                self.resolve_form(1)?;
                self.stage = Stage::Ready(instruction);
            }
            Stage::Request(Request::Random { maximum }) => {
                let result =
                    value.ok_or_else(|| Error("random request requires a result".into()))?;
                if result > maximum {
                    return Err(Error("random result exceeds inclusive bound".into()));
                }
                self.set_accumulator(result);
                self.stage = Stage::Fetch;
            }
            Stage::Request(Request::NativeService { .. }) => {
                if let Some(result) = value {
                    self.set_accumulator(result);
                }
                self.stage = Stage::Fetch;
            }
            Stage::Request(_) if value.is_none() => self.stage = Stage::Fetch,
            Stage::Request(_) => {
                return Err(Error("output request does not return a value".into()));
            }
            _ => return Err(Error("no request is pending".into())),
        }
        Ok(())
    }

    fn resolve_form(&mut self, form: u8) -> Result<(), Error> {
        if let Some(value) = self.interpreter.forms.constants[usize::from(form)] {
            self.state.argument = value;
            return Ok(());
        }
        let [first, second, _] = self.state.operands;
        self.state.argument = match form {
            0 => first,
            1 => self.property(ObjectId(first), ObjectId(second), false)?,
            2 => {
                let mut object = second.wrapping_add(1);
                let mut result = NONE;
                while object <= self.layout.max_object {
                    if self.variable(VarAddr(
                        self.layout
                            .direct_values
                            .wrapping_add(object.wrapping_mul(2)),
                    ))? == first
                    {
                        result = object;
                        break;
                    }
                    object = object
                        .checked_add(1)
                        .ok_or_else(|| Error("object iteration overflow".into()))?;
                }
                result
            }
            3 => self.accumulator(),
            // Form4 consumes three operands; forms6/7 consume none. All three
            // reach opcode dispatch without replacing the previous argument.
            4 | 6 | 7 => self.state.argument,
            5 => self.property(ObjectId(first), ObjectId(second), true)?,
            _ => unreachable!("decoder validates form"),
        };
        Ok(())
    }

    fn jump(&mut self, address: u16) {
        if address != NONE {
            self.state.ip = ProgramAddr(address);
        }
    }

    fn call(&mut self, address: u16) -> Result<(), Error> {
        if address != NONE {
            Self::push(
                &mut self.state.returns,
                &mut self.return_slots,
                self.state.ip.0,
                "return",
            )?;
            self.jump(address);
        }
        Ok(())
    }

    fn push(
        stack: &mut Vec<u16>,
        slots: &mut [u16; STACK_CAPACITY],
        value: u16,
        name: &str,
    ) -> Result<(), Error> {
        if stack.len() == STACK_CAPACITY {
            return Err(Error(format!("{name} stack overflow")));
        }
        slots[stack.len()] = value;
        stack.push(value);
        Ok(())
    }

    /// Returns a value, or a writable variable address when `address` is true.
    /// A program-only property has no writable address: the original returns 0.
    pub fn property(
        &mut self,
        object: ObjectId,
        property: ObjectId,
        address: bool,
    ) -> Result<u16, Error> {
        self.property_found = false;
        if !address && (object.0 == NONE || property.0 == NONE) {
            return Ok(NONE);
        }
        if self.interpreter.validate_property_id
            && property.0 > self.layout.max_object
            && property.0 != NONE
        {
            return Err(Error("property ID outside object table".into()));
        }
        let mut object = object.0;
        let mut visited = Vec::new();
        loop {
            if object > self.layout.max_object {
                return Err(Error("object ID outside supported property table".into()));
            }
            if visited.contains(&object) {
                return Err(Error("cyclic property fallback".into()));
            }
            visited.push(object);
            if self.layout.direct_properties.contains(&property.0) {
                let slot = VarAddr(
                    self.layout
                        .direct_values
                        .wrapping_add(object.wrapping_mul(2)),
                );
                let value = self.variable(slot)?;
                if value != self.layout.wildcard {
                    self.property_found = true;
                    return Ok(if address { slot.0 } else { value });
                }
            }
            let record = self.layout.records.wrapping_add(object.wrapping_mul(4));
            let mutable_start = self
                .program_word(record)?
                .wrapping_add(self.layout.mutable_bias);
            let program_start = self
                .program_word(record.wrapping_add(2))?
                .wrapping_add(self.layout.program_bias);
            let mutable_end = self
                .program_word(record.wrapping_add(4))?
                .wrapping_add(self.layout.mutable_bias);
            let program_end = self
                .program_word(record.wrapping_add(6))?
                .wrapping_add(self.layout.program_bias);
            if mutable_start > mutable_end
                || program_start > program_end
                || (mutable_end - mutable_start) % 4 != 0
                || (program_end - program_start) % 4 != 0
            {
                return Err(Error("invalid property pair ranges".into()));
            }
            let mut fallback = NONE;
            for offset in (mutable_start..mutable_end).step_by(4) {
                let key = self.variable(VarAddr(offset))?;
                let slot = VarAddr(offset.wrapping_add(2));
                let value = self.variable(slot)?;
                if key == property.0 {
                    self.property_found = true;
                    return Ok(if address { slot.0 } else { value });
                }
                if key == self.layout.wildcard {
                    fallback = value;
                }
            }
            for offset in (program_start..program_end).step_by(4) {
                let key = self.program_word(offset)?;
                let value = self.program_word(offset.wrapping_add(2))?;
                if key == property.0 {
                    self.property_found = true;
                    if address {
                        self.write_variable(VarAddr(0), NONE)?;
                        return Ok(0);
                    }
                    return Ok(value);
                }
                if key == self.layout.wildcard {
                    fallback = value;
                }
            }
            if fallback == NONE {
                if address {
                    self.write_variable(VarAddr(0), NONE)?;
                    return Ok(0);
                }
                return Ok(NONE);
            }
            object = fallback;
        }
    }

    pub fn property_found(&self) -> bool {
        self.property_found
    }
}

fn initialize_variables(variables: &mut [u8], interpreter: &Interpreter) -> Result<(), Error> {
    for value in 1u16..=14 {
        let offset = usize::from(value) * 2;
        variables[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    for &(address, value) in interpreter.initial_values {
        let offset = usize::from(address.0);
        variables
            .get_mut(offset..offset + 2)
            .ok_or_else(|| Error("extended interpreter header is truncated".into()))?
            .copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}
