// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded headless execution. Requests suspend until an explicit completion.

use crate::decode::{Instruction, OperandMode, decode};
use crate::image::StoryImage;
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
    Random { maximum: u16 },
    Message { id: u16 },
    NativeService { id: u16 },
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
    layout: Layout,
    state: VmState,
    stage: Stage,
    property_found: bool,
    return_slots: [u16; STACK_CAPACITY],
    value_slots: [u16; STACK_CAPACITY],
}

impl Vm {
    pub fn new(image: StoryImage) -> Result<Self, Error> {
        let initial = image.initial_variables();
        let mut variables = initial.to_vec();
        variables[..2].copy_from_slice(&NONE.to_le_bytes());
        for value in 1u16..=14 {
            let offset = usize::from(value) * 2;
            variables[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        let state = VmState {
            ip: ProgramAddr(read_word(initial, 0xe8, "entry")?),
            argument: 0,
            operands: [0; 3],
            variables,
            returns: Vec::new(),
            values: Vec::new(),
        };
        Self::from_state(image, state)
    }

    /// Restores a fetch-boundary checkpoint, not a suspended native operation.
    pub fn from_state(image: StoryImage, state: VmState) -> Result<Self, Error> {
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
            layout,
            state,
            stage: Stage::Fetch,
            property_found: false,
            return_slots,
            value_slots,
        })
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
                let instruction = decode(self.image.program(), self.state.ip)?;
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
        match opcode {
            0x00 => {
                self.state.ip = ProgramAddr(
                    self.state
                        .returns
                        .pop()
                        .ok_or_else(|| Error("return stack underflow".into()))?,
                );
                self.set_accumulator(argument);
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
            0x0d..=0x0f | 0x17 | 0x18 => {}
            0x10 => self.set_accumulator(accumulator.wrapping_add(argument)),
            0x11 => self.set_accumulator(accumulator.wrapping_sub(argument)),
            0x12 => self.set_accumulator(argument.wrapping_sub(accumulator)),
            0x13 => self.set_accumulator(accumulator.wrapping_mul(argument)),
            0x14 | 0x21 if argument == 0 => return Err(Error("division by zero".into())),
            0x14 => self.set_accumulator(accumulator / argument),
            0x15 => return Ok(self.suspend(Request::Random { maximum: argument })),
            0x16 => return Ok(self.suspend(Request::Message { id: argument })),
            0x19 => match argument {
                12 => {
                    self.state.returns.clear();
                    self.state.values.clear();
                }
                13 => {}
                1..=14 => return Ok(self.suspend(Request::NativeService { id: argument })),
                _ => return Err(Error(format!("invalid native service {argument}"))),
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
        if !matches!(self.stage, Stage::Request(Request::NativeService { id: 1 })) {
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
        if !matches!(self.stage, Stage::Request(Request::NativeService { id: 6 })) {
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
        // The original loader uses its variable writer: offset0 is forced to
        // ffff and protected constants2..8 retain their current values.
        let constants: [u8; 8] = self.state.variables[2..10].try_into().unwrap();
        self.state.variables.copy_from_slice(variables);
        self.state.variables[..2].copy_from_slice(&NONE.to_le_bytes());
        self.state.variables[2..10].copy_from_slice(&constants);
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
            Stage::Request(Request::NativeService { id: 11 })
        ) {
            return Err(Error(
                "initial-variable reload requires native service11".into(),
            ));
        }
        self.state
            .variables
            .copy_from_slice(self.image.initial_variables());
        self.state.variables[..2].copy_from_slice(&NONE.to_le_bytes());
        for value in 1u16..=14 {
            let offset = usize::from(value) * 2;
            self.state.variables[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        Ok(())
    }

    /// Completes a request. PollInput currently supports an empty keyboard only;
    /// actual keyboard parsing is explicitly not implemented by this core.
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
        if property.0 > self.layout.max_object && property.0 != NONE {
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
