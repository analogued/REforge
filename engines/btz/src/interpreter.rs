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
    SaveScreen,
    RestoreScreen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionOpcode {
    NoOp,
    Character,
    LoadByte,
    StoreByte,
    RestoreComparison,
    CharacterToken,
}

/// Operand consumption and constant arguments are independent of opcodes.
#[derive(Clone, Copy, Debug)]
pub struct FormRules {
    pub arities: [usize; 8],
    pub constants: [Option<u16>; 8],
}

pub const RETAINED_FORMS: FormRules = FormRules {
    arities: [1, 2, 2, 0, 3, 2, 0, 0],
    constants: [None; 8],
};

pub const CONSTANT_FORMS: FormRules = FormRules {
    arities: [1, 2, 2, 0, 0, 2, 0, 0],
    constants: [None, None, None, None, Some(0xffff), None, Some(0), None],
};

#[derive(Clone, Copy, Debug)]
pub struct ScreenLayout {
    pub frame_right: usize,
    /// Initial editor row, also used by frame initialization.
    pub input_top: usize,
    /// First row moved when command history overflows at the bottom.
    pub input_scroll_top: usize,
}

pub const DOS_LAYOUT: ScreenLayout = ScreenLayout {
    frame_right: 78,
    input_top: 19,
    input_scroll_top: 19,
};
pub const WIDE_DOS_LAYOUT: ScreenLayout = ScreenLayout {
    frame_right: 79,
    input_top: 19,
    input_scroll_top: 20,
};

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

/// Variable-backed switches introduced by the extended DOS runtime. These
/// control pauses and transcript output, not whether ordinary keys are read.
#[derive(Clone, Copy, Debug)]
pub struct OutputControls {
    pub pauses: VarAddr,
    pub transcript: VarAddr,
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
    /// Slots 0d, 0e, 0f, 17 and 18 respectively.
    pub extensions: [ExtensionOpcode; 5],
    pub forms: FormRules,
    pub comparison_backup: Option<VarAddr>,
    pub layout: ScreenLayout,
    pub reset_parser_gaps: bool,
    pub transcript_cancel_zero: bool,
    pub confirm_bookmark_replace: bool,
    pub services: &'static [NativeService],
    pub text: TextExecution,
    pub screen: ScreenModel,
    pub numeric_input: NumericInput,
    pub submitted_read: SubmittedRead,
    pub validate_property_id: bool,
    pub output_controls: Option<OutputControls>,
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
    extensions: [ExtensionOpcode::NoOp; 5],
    forms: RETAINED_FORMS,
    comparison_backup: None,
    layout: DOS_LAYOUT,
    reset_parser_gaps: false,
    transcript_cancel_zero: false,
    confirm_bookmark_replace: false,
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
    output_controls: None,
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
        (VarAddr(0x12c), 15),
        (VarAddr(0x12e), 16),
        (VarAddr(0x132), 18),
    ],
    extensions: [
        ExtensionOpcode::Character,
        ExtensionOpcode::LoadByte,
        ExtensionOpcode::StoreByte,
        ExtensionOpcode::NoOp,
        ExtensionOpcode::NoOp,
    ],
    forms: RETAINED_FORMS,
    comparison_backup: None,
    layout: DOS_LAYOUT,
    reset_parser_gaps: false,
    transcript_cancel_zero: false,
    confirm_bookmark_replace: false,
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
    output_controls: Some(OutputControls {
        pauses: VarAddr(0x134),
        transcript: VarAddr(0x136),
    }),
};

/// This identified runtime shares the demonstrated extended semantics. Keep its
/// identity explicit so a later verified difference can be selected locally.
pub static BRIMSTONE_DOS: Interpreter = Interpreter {
    name: "Brimstone DOS",
    ..ESSEX_DOS
};

pub static BREAKERS_DOS: Interpreter = Interpreter {
    name: "Breakers DOS",
    initial_values: &[
        (VarAddr(0x118), 1),
        (VarAddr(0x11a), 1),
        (VarAddr(0x11c), 1),
        (VarAddr(0x11e), 1),
        (VarAddr(0x120), 78),
        (VarAddr(0x122), 17),
        (VarAddr(0x124), 79),
        (VarAddr(0x126), 24),
        (VarAddr(0x12c), 15),
        (VarAddr(0x12e), 16),
        (VarAddr(0x132), 18),
        (VarAddr(0xd0), 17),
        (VarAddr(0xd2), 19),
    ],
    extensions: [
        ExtensionOpcode::NoOp,
        ExtensionOpcode::LoadByte,
        ExtensionOpcode::StoreByte,
        ExtensionOpcode::RestoreComparison,
        ExtensionOpcode::CharacterToken,
    ],
    forms: CONSTANT_FORMS,
    comparison_backup: Some(VarAddr(0xec)),
    layout: WIDE_DOS_LAYOUT,
    reset_parser_gaps: true,
    transcript_cancel_zero: true,
    confirm_bookmark_replace: true,
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
        GetCursor,
        SetCursor,
        SaveScreen,
        DrawFrame,
        RestoreScreen,
    ],
    ..BRIMSTONE_DOS
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

pub const BRIMSTONE_RESOURCES: DosResources = DosResources {
    pairs: 0x322,
    thirds: 0x522,
    pause: 0x105,
    classes: 0x22,
    random_adjustment: 0x6e5,
    service_strings: [0x223, 0x18c, 0x254, 0x278],
};

pub const BREAKERS_RESOURCES: DosResources = DosResources {
    pairs: 0x38b,
    thirds: 0x58b,
    pause: 0x114,
    classes: 0x22,
    random_adjustment: 0x73e,
    service_strings: [0x246, 0x19b, 0x277, 0x29b],
};
