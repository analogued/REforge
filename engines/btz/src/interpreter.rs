// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Identified DOS interpreter behavior, independent of story data and EXE layout.

use crate::VarAddr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeService {
    ReadParse,
    ParsedEnd,
    WaitKey,
    Exit,
    Save,
    Restore,
    EchoUnknown,
    OpenTranscript,
    CloseTranscript,
    EchoInput,
    ResetVariables,
    ResetStacks,
    NoOp,
    SelectDisk,
    GetCursor,
    SetCursor,
    DrawFrame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionOpcode {
    NoOp,
    Character,
    LoadByte,
    StoreByte,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextExecution {
    Direct,
    WordcodeCallbacks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenModel {
    Fixed,
    VariableBacked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericInput {
    Disabled,
    ObjectSlots,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmittedRead {
    ShiftEight,
    Divide85,
}

impl SubmittedRead {
    pub fn value(self, word: u16) -> u16 {
        match self {
            Self::ShiftEight => word >> 8,
            Self::Divide85 => word / 85,
        }
        .saturating_sub(1)
    }

    pub fn representative_word(self, value: u16) -> Option<u16> {
        if value == 0 {
            return Some(0);
        }
        let factor = match self {
            Self::ShiftEight => 256,
            Self::Divide85 => 85,
        };
        value.checked_add(1)?.checked_mul(factor)
    }
}

/// Only demonstrated differences belong here. These names identify known
/// implementations; they do not assert official version numbers or ancestry.
#[derive(Clone, Copy, Debug)]
pub struct Interpreter {
    pub name: &'static str,
    pub initial_values: &'static [(VarAddr, u16)],
    pub extensions: [ExtensionOpcode; 3],
    pub services: &'static [NativeService],
    pub text: TextExecution,
    pub screen: ScreenModel,
    pub numeric_input: NumericInput,
    pub submitted_read: SubmittedRead,
    pub validate_property_id: bool,
}

impl Interpreter {
    pub fn service(self, id: u16) -> Option<NativeService> {
        self.services.get(usize::from(id.checked_sub(1)?)).copied()
    }
}

use NativeService::*;

pub static MINDWHEEL_DOS: Interpreter = Interpreter {
    name: "Mindwheel DOS",
    initial_values: &[],
    extensions: [ExtensionOpcode::NoOp; 3],
    services: &[
        ReadParse,
        ParsedEnd,
        WaitKey,
        Exit,
        Save,
        Restore,
        EchoUnknown,
        OpenTranscript,
        CloseTranscript,
        EchoInput,
        ResetVariables,
        ResetStacks,
        NoOp,
        SelectDisk,
    ],
    text: TextExecution::Direct,
    screen: ScreenModel::Fixed,
    numeric_input: NumericInput::Disabled,
    submitted_read: SubmittedRead::ShiftEight,
    validate_property_id: true,
};

pub static ESSEX_DOS: Interpreter = Interpreter {
    name: "Essex DOS",
    initial_values: &[
        (VarAddr(0x118), 1),
        (VarAddr(0x11a), 1),
        (VarAddr(0x11c), 1),
        (VarAddr(0x11e), 1),
        (VarAddr(0x120), 77),
        (VarAddr(0x122), 17),
        (VarAddr(0x124), 79),
        (VarAddr(0x126), 24),
        (VarAddr(0x128), 0),
        (VarAddr(0x12a), 0),
        (VarAddr(0x12c), 15),
        (VarAddr(0x12e), 16),
        (VarAddr(0x132), 18),
    ],
    extensions: [
        ExtensionOpcode::Character,
        ExtensionOpcode::LoadByte,
        ExtensionOpcode::StoreByte,
    ],
    services: &[
        ReadParse,
        ParsedEnd,
        WaitKey,
        Exit,
        Save,
        Restore,
        EchoInput,
        OpenTranscript,
        CloseTranscript,
        EchoUnknown,
        ResetVariables,
        ResetStacks,
        NoOp,
        SelectDisk,
        GetCursor,
        SetCursor,
        NoOp,
        DrawFrame,
    ],
    text: TextExecution::WordcodeCallbacks,
    screen: ScreenModel::VariableBacked,
    numeric_input: NumericInput::ObjectSlots,
    submitted_read: SubmittedRead::Divide85,
    validate_property_id: false,
};

/// Locations of externally supplied resources. Moving a table does not change
/// VM semantics, so these offsets are kept out of `Interpreter`.
#[derive(Clone, Copy, Debug)]
pub struct DosResources {
    pub pairs: usize,
    pub thirds: usize,
    pub pause: usize,
    pub classes: usize,
    pub random_adjustment: usize,
    pub service_strings: [usize; 4],
}

pub const MINDWHEEL_RESOURCES: DosResources = DosResources {
    pairs: 0x36d,
    thirds: 0x56d,
    pause: 0xff,
    classes: 0x7f,
    random_adjustment: 0x6c7,
    service_strings: [0x23f, 0x18f, 0x270, 0x297],
};

pub const ESSEX_RESOURCES: DosResources = DosResources {
    pairs: 0x312,
    thirds: 0x512,
    pause: 0x105,
    classes: 0x22,
    random_adjustment: 0x6d5,
    service_strings: [0x213, 0x163, 0x244, 0x268],
};
