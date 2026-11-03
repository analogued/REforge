// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! DOS wordcode decoding with definition-selected operand consumption.

use crate::{Error, ProgramAddr, read_word};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperandMode {
    Literal,
    Variable,
    IndirectVariable,
    ProgramAtVariable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Operand {
    pub mode: OperandMode,
    pub raw: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub address: ProgramAddr,
    pub word: u16,
    pub opcode: u8,
    pub form: u8,
    pub operands: Vec<Operand>,
    pub next: ProgramAddr,
}

pub fn decode(program: &[u8], address: ProgramAddr) -> Result<Instruction, Error> {
    decode_with_rules(program, address, crate::interpreter::RETAINED_FORMS)
}

pub fn decode_with_rules(
    program: &[u8],
    address: ProgramAddr,
    rules: crate::interpreter::FormRules,
) -> Result<Instruction, Error> {
    let word = read_word(program, usize::from(address.0), "program")?;
    let opcode = ((word >> 2) & 0x3f) as u8;
    let form = ((word & 3) + ((word & 0x0800) >> 9) + ((word & 0x8000) >> 12)) as u8;
    let count = *rules
        .arities
        .get(usize::from(form))
        .ok_or_else(|| Error(format!("unsupported instruction form {form}")))?;
    if opcode > 0x22 {
        return Err(Error(format!("unsupported opcode {opcode:#x}")));
    }
    let types = (word & 0x7700) >> 8;
    let mut next = address.0.wrapping_add(2);
    let mut operands = Vec::with_capacity(count);
    for (index, mask) in [0x11, 0x22, 0x44].into_iter().take(count).enumerate() {
        let encoded = (types & mask) >> index;
        let mode = match ((encoded & 0x70) >> 3) + (encoded & 7) {
            0 => OperandMode::Literal,
            1 => OperandMode::Variable,
            2 => OperandMode::IndirectVariable,
            3 => OperandMode::ProgramAtVariable,
            _ => unreachable!("two addressing bits"),
        };
        operands.push(Operand {
            mode,
            raw: read_word(program, usize::from(next), "program")?,
        });
        next = next.wrapping_add(2);
    }
    Ok(Instruction {
        address,
        word,
        opcode,
        form,
        operands,
        next: ProgramAddr(next),
    })
}
