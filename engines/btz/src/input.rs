// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! The observed DOS keyboard editor, independent of VM parsing and presentation.

use crate::Error;

/// Keys after the DOS keyboard helper's scan-code translation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Character(u8),
    Enter,
    LineFeed,
    Backspace,
    Tab,
    EraseLine,
    RecallLine,
    RecallCharacter,
    Pause,
    Escape,
    NoKey,
    Ignored,
}

impl Key {
    pub fn from_dos_byte(byte: u8) -> Self {
        match byte {
            0 => Self::NoKey,
            0x1e | 0x1f => Self::Ignored,
            8 | 0x1d => Self::Backspace,
            9 => Self::Tab,
            10 => Self::LineFeed,
            13 => Self::Enter,
            0x13 => Self::Pause,
            0x1b => Self::Escape,
            0xc8 | 0xd6 => Self::EraseLine,
            0xc9 | 0xd4 => Self::RecallLine,
            0x1c | 0xd2 => Self::RecallCharacter,
            0xc8..=0xdb => Self::Ignored,
            _ => Self::Character(byte),
        }
    }
}

/// Ordered changes for the host's existing character screen and VM state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputAction {
    Put {
        row: usize,
        column: usize,
        byte: u8,
    },
    Scroll {
        top: usize,
        bottom: usize,
        left: usize,
        right: usize,
    },
    /// CR clears the original submit-related variable at offset 0xda.
    ClearSubmitFlag,
    /// Show or clear the original keyboard-pause prompt on the bottom row.
    PauseStatus {
        active: bool,
    },
    /// BIOS teletype BEL has no painted character, but the editor still counts it.
    Bell,
}

#[derive(Clone, Debug)]
pub struct InputEditor {
    buffer: Vec<u8>,
    history: Vec<u8>,
    row: usize,
    column: usize,
    ready: bool,
    paused: bool,
}

impl Default for InputEditor {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            history: Vec::new(),
            row: 19,
            column: 1,
            ready: false,
            paused: false,
        }
    }
}

impl InputEditor {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.column)
    }
    pub fn line_ready(&self) -> bool {
        self.ready
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    /// BIOS frame initialization clears the current editor, retaining recall.
    pub fn rebuild_frame(&mut self) {
        self.buffer.clear();
        self.row = 19;
        self.column = 1;
        self.ready = false;
        self.paused = false;
    }
    pub fn submitted_line(&self) -> Option<&[u8]> {
        self.ready.then_some(&self.buffer)
    }

    /// Called by the parser only after accepting a submitted line. Raw case and
    /// CR are retained for recall; tokenization/lowercasing are separate work.
    pub fn acknowledge_line(&mut self) -> Result<Vec<u8>, Error> {
        if !self.ready {
            return Err(Error("keyboard editor has no submitted line".into()));
        }
        self.ready = false;
        let line = std::mem::take(&mut self.buffer);
        self.history.clone_from(&line);
        Ok(line)
    }

    /// Processes one translated key. Polling/draining a host queue belongs to
    /// the host; no key timing or grammar results are synthesized here.
    pub fn process_key(&mut self, key: Key) -> Vec<InputAction> {
        self.process_key_with_pauses(key, true)
    }

    pub fn process_key_with_pauses(&mut self, key: Key, pauses: bool) -> Vec<InputAction> {
        let key = match (pauses, key) {
            (false, Key::Pause) => Key::Character(0x13),
            (false, Key::Escape) => Key::Character(0x1b),
            _ => key,
        };
        let mut actions = Vec::new();
        if self.paused {
            if matches!(key, Key::Pause | Key::Escape | Key::NoKey) {
                return actions;
            }
            self.paused = false;
            actions.push(InputAction::PauseStatus { active: false });
            if matches!(key, Key::Enter | Key::LineFeed) {
                return actions;
            }
        } else if matches!(key, Key::Pause | Key::Escape) {
            self.paused = true;
            actions.push(InputAction::PauseStatus { active: true });
            return actions;
        }
        if key == Key::NoKey {
            return actions;
        }
        if self.ready {
            return actions;
        }
        match key {
            Key::Character(byte) => self.byte(byte, &mut actions),
            Key::Enter => self.byte(13, &mut actions),
            Key::LineFeed | Key::Ignored => self.byte(0, &mut actions),
            Key::Backspace => self.byte(8, &mut actions),
            Key::Tab => {
                for _ in 0..10 {
                    self.byte(b' ', &mut actions);
                }
                self.byte(0, &mut actions);
            }
            Key::EraseLine => {
                while self.column >= 2 && !self.buffer.is_empty() {
                    self.byte(8, &mut actions);
                }
                self.byte(0, &mut actions);
            }
            Key::RecallLine | Key::RecallCharacter => {
                let index = self.column - 1;
                let end = self
                    .history
                    .iter()
                    .position(|b| matches!(b, 0 | 10 | 13))
                    .unwrap_or(self.history.len());
                if index < end {
                    let count = if key == Key::RecallCharacter {
                        1
                    } else {
                        end - index
                    };
                    let recalled = self.history[index..index + count].to_vec();
                    for byte in recalled {
                        self.byte(byte, &mut actions);
                    }
                }
                self.byte(0, &mut actions);
            }
            Key::Pause | Key::Escape | Key::NoKey => {
                unreachable!("pause and empty polls handled above")
            }
        }
        actions
    }

    fn byte(&mut self, byte: u8, actions: &mut Vec<InputAction>) {
        match byte {
            0 | 10 => {}
            13 => {
                actions.push(InputAction::ClearSubmitFlag);
                self.ready = true;
                actions.push(InputAction::Put {
                    row: self.row,
                    column: self.column,
                    byte: b' ',
                });
                self.buffer.push(13);
                self.column = 1;
                self.row += 1;
            }
            8 => {
                if self.buffer.pop().is_some() {
                    actions.push(InputAction::Put {
                        row: self.row,
                        column: self.column,
                        byte: b' ',
                    });
                    if self.column >= 2 {
                        self.column -= 1;
                    } else {
                        self.column = 77;
                        self.row -= 1;
                    }
                }
            }
            _ => {
                if self.buffer.len() < 76 {
                    self.buffer.push(byte);
                    if byte == 7 {
                        actions.push(InputAction::Bell);
                    } else {
                        actions.push(InputAction::Put {
                            row: self.row,
                            column: self.column,
                            byte,
                        });
                    }
                    if self.column < 77 {
                        self.column += 1;
                    } else {
                        self.column = 1;
                        self.row += 1;
                    }
                }
            }
        }
        if self.row == 23 {
            self.row = 22;
            actions.push(InputAction::Scroll {
                top: 19,
                bottom: 22,
                left: 1,
                right: 77,
            });
        }
        actions.push(InputAction::Put {
            row: self.row,
            column: self.column,
            byte: 0xdb,
        });
    }
}
