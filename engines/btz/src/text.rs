// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! DOS text decoding and a presentation-independent character screen.

use std::collections::VecDeque;

use crate::input::InputAction;
use crate::services::TranscriptEvent;
use crate::{
    Error, ObjectId, ProgramAddr,
    image::{StoryImage, dos_data},
    read_word,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct TextCursor {
    pub argument: u16,
    pub position: u16,
    end: u16,
    message: bool,
}

#[derive(Clone, Debug)]
pub struct TextCodec {
    pairs: [u8; 512],
    thirds: [u8; 128],
    pause: Vec<u8>,
    input_classes: [u8; 128],
}

impl TextCodec {
    /// Reads the observed Mindwheel DOS table layout, not a revision detector.
    /// Tables remain external original data, rather than constants in the engine.
    pub fn from_dos_executable(exe: &[u8]) -> Result<Self, Error> {
        Self::from_dos_resources(exe, crate::interpreter::MINDWHEEL_RESOURCES)
    }

    pub fn from_dos_resources(
        exe: &[u8],
        layout: crate::interpreter::DosResources,
    ) -> Result<Self, Error> {
        let data = dos_data(exe)?;
        let pairs = data
            .get(layout.pairs..layout.pairs.saturating_add(512))
            .ok_or_else(|| Error("missing text pair tables".into()))?
            .try_into()
            .unwrap();
        let thirds = data
            .get(layout.thirds..layout.thirds.saturating_add(128))
            .ok_or_else(|| Error("missing text triple table".into()))?
            .try_into()
            .unwrap();
        let pause = data
            .get(layout.pause..)
            .and_then(|s| s.iter().position(|&b| b == 0).map(|end| s[..end].to_vec()))
            .ok_or_else(|| Error("unterminated interpreter pause prompt".into()))?;
        let input_classes = data
            .get(layout.classes..layout.classes.saturating_add(128))
            .ok_or_else(|| Error("missing input character classes".into()))?
            .try_into()
            .unwrap();
        Ok(Self {
            pairs,
            thirds,
            pause,
            input_classes,
        })
    }

    pub fn pause_prompt(&self) -> &[u8] {
        &self.pause
    }

    pub fn input_classes(&self) -> &[u8; 128] {
        &self.input_classes
    }

    pub fn word(&self, image: &StoryImage, address: u16) -> Result<Vec<u8>, Error> {
        let dictionary = image.store(3)?;
        let mut pos = usize::from(address) + 4;
        read_word(dictionary, usize::from(address) + 2, "dictionary node")?;
        let mut result = Vec::new();
        loop {
            let byte = *dictionary
                .get(pos)
                .ok_or_else(|| Error("unterminated dictionary word".into()))?;
            pos += 1;
            if byte == 0 {
                return Ok(result);
            }
            let index = usize::from(byte);
            if self.pairs[index] == 0 {
                result.push(byte);
            } else {
                result.extend([self.pairs[index], self.pairs[index + 256]]);
                if index < 128 && self.thirds[index] != 0 {
                    result.push(self.thirds[index]);
                }
            }
            if result.len() >= 40 {
                return Err(Error("decoded word reaches DOS text buffer limit".into()));
            }
        }
    }

    pub(crate) fn cursor(
        &self,
        image: &StoryImage,
        argument: u16,
        resume: Option<u16>,
        message: bool,
    ) -> Result<TextCursor, Error> {
        if argument == 0xffff {
            return Ok(TextCursor {
                argument,
                position: 0,
                end: 0,
                message,
            });
        }
        let (start, end) = if message {
            if argument >= 0xc180 {
                return Err(Error("message ID exceeds DOS message limit".into()));
            }
            let offset = argument.wrapping_sub(0x8000).wrapping_mul(2);
            (
                read_word(image.store(2)?, usize::from(offset), "message start")?,
                read_word(
                    image.store(2)?,
                    usize::from(offset.wrapping_add(2)),
                    "message end",
                )?,
            )
        } else {
            let maximum = read_word(image.initial_variables(), 0x102, "maximum object")?;
            if argument > maximum {
                return Err(Error("object name outside native range".into()));
            }
            let table = read_word(image.initial_variables(), 0xf0, "object phrase table")?;
            let offset = table.wrapping_add(argument.wrapping_mul(2));
            (
                image.program_word(ProgramAddr(offset.wrapping_sub(2)))?,
                image.program_word(ProgramAddr(offset))?,
            )
        };
        let position = resume.unwrap_or(start);
        if position < start || position > end {
            return Err(Error("invalid text continuation position".into()));
        }
        Ok(TextCursor {
            argument,
            position,
            end,
            message,
        })
    }

    pub(crate) fn next_word(
        &self,
        image: &StoryImage,
        cursor: &mut TextCursor,
    ) -> Result<Option<Vec<u8>>, Error> {
        if cursor.position >= cursor.end {
            return Ok(None);
        }
        let address = if cursor.message {
            let address = read_word(
                image.store(4)?,
                usize::from(cursor.position) * 2,
                "message word",
            )?;
            cursor.position += 1;
            address
        } else {
            let grammar = image.store(1)?;
            let offset = usize::from(cursor.position);
            if grammar.get(offset) == Some(&0xff) {
                cursor.position = cursor.end;
                return Ok(None);
            }
            if cursor.end - cursor.position < 2 {
                return Err(Error("truncated object word".into()));
            }
            let pair = grammar
                .get(offset..offset + 2)
                .ok_or_else(|| Error("object word outside grammar".into()))?;
            cursor.position += 2;
            u16::from_be_bytes(pair.try_into().unwrap())
        };
        if address == 0xffff {
            cursor.position = cursor.end;
            return Ok(None);
        }
        Ok(Some(self.word(image, address)?))
    }

    pub fn message(&self, image: &StoryImage, id: u16) -> Result<Vec<Vec<u8>>, Error> {
        if id == 0xffff {
            return Ok(Vec::new());
        }
        if id >= 0xc180 {
            return Err(Error("message ID exceeds DOS message limit".into()));
        }
        let offset = id.wrapping_sub(0x8000).wrapping_mul(2);
        let bounds = image.store(2)?;
        let start = usize::from(read_word(bounds, usize::from(offset), "message start")?);
        let end = usize::from(read_word(
            bounds,
            usize::from(offset.wrapping_add(2)),
            "message end",
        )?);
        let references = image.store(4)?;
        if start > end || end > references.len() / 2 {
            return Err(Error("invalid message word extent".into()));
        }
        let mut result = Vec::new();
        for index in start..end {
            let address = read_word(references, index * 2, "message word reference")?;
            if address == 0xffff {
                break;
            }
            result.push(self.word(image, address)?);
        }
        Ok(result)
    }

    /// Object names use the first phrase in the BE16 grammar stream; a single
    /// 0xff ends it. Message word references use a different address space.
    pub fn object_words(
        &self,
        image: &StoryImage,
        object: ObjectId,
    ) -> Result<Vec<Vec<u8>>, Error> {
        if object.0 == 0xffff {
            return Ok(Vec::new());
        }
        let maximum = read_word(image.initial_variables(), 0x102, "maximum object")?;
        if object.0 > 0x7fff || object.0 > maximum {
            return Err(Error("object name outside native object range".into()));
        }
        let table = read_word(image.initial_variables(), 0xf0, "object phrase table")?;
        let offset = table.wrapping_add(object.0.wrapping_mul(2));
        let start = usize::from(image.program_word(ProgramAddr(offset.wrapping_sub(2)))?);
        let end = usize::from(image.program_word(ProgramAddr(offset))?);
        let grammar = image.store(1)?;
        if start > end || end > grammar.len() {
            return Err(Error("invalid object name extent".into()));
        }
        let mut result = Vec::new();
        let mut pos = start;
        while pos < end {
            let high = grammar[pos];
            if high == 0xff {
                break;
            }
            let low = *grammar
                .get(pos + 1)
                .filter(|_| pos + 1 < end)
                .ok_or_else(|| Error("truncated object-name dictionary address".into()))?;
            result.push(self.word(image, u16::from_be_bytes([high, low]))?);
            pos += 2;
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug)]
struct TextRegion {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
    status: usize,
}

/// A DOS 80 by 25 text screen. Each cell has a CP437 byte and an attribute.
#[derive(Clone, Debug)]
pub struct TextScreen {
    cells: Vec<u8>,
    row: usize,
    column: usize,
    region: TextRegion,
}

impl Default for TextScreen {
    fn default() -> Self {
        let mut screen = Self {
            cells: [b' ', 7].repeat(2000),
            row: 1,
            column: 1,
            region: TextRegion {
                left: 1,
                top: 1,
                right: 77,
                bottom: 17,
                status: 24,
            },
        };
        for row in [0, 18, 23] {
            for column in 0..79 {
                screen.put(row, column, 0xcd);
            }
        }
        for row in 0..24 {
            for column in [0, 78] {
                screen.put(row, column, 0xba);
            }
        }
        for (row, left, right) in [(0, 0xc9, 0xbb), (18, 0xcc, 0xb9), (23, 0xc8, 0xbc)] {
            screen.put(row, 0, left);
            screen.put(row, 78, right);
        }
        screen.put(19, 1, 0xdb);
        screen
    }
}

impl TextScreen {
    fn put(&mut self, row: usize, column: usize, byte: u8) {
        if row < 25 && column < 80 {
            self.cells[(row * 80 + column) * 2] = byte;
        }
    }

    fn character(&mut self, byte: u8) {
        match byte {
            8 => {
                self.column = self.column.saturating_sub(1);
                self.put(self.row, self.column, b' ');
            }
            10 => {
                self.column = self.region.left;
                self.row += 1;
            }
            13 => {}
            7 => self.column += 1,
            _ => {
                self.put(self.row, self.column, byte);
                self.column += 1;
            }
        }
        if self.row == self.region.bottom + 1 {
            self.row = self.region.bottom;
            for row in self.region.top..self.region.bottom {
                for column in self.region.left..=self.region.right {
                    let byte = self.cells[((row + 1) * 80 + column) * 2];
                    self.put(row, column, byte);
                }
            }
            for column in self.region.left..=self.region.right {
                self.put(self.region.bottom, column, b' ');
            }
        }
    }

    fn status(&mut self, text: &[u8]) {
        for column in 0..80 {
            self.put(
                self.region.status,
                column,
                text.get(column).copied().unwrap_or(b' '),
            );
        }
    }

    fn clear_upper(&mut self) {
        for row in self.region.top..=self.region.bottom {
            for column in self.region.left..=self.region.right {
                self.put(row, column, b' ');
            }
        }
        self.row = self.region.top;
        self.column = self.region.left;
    }

    pub fn dos_cells(&self) -> &[u8] {
        &self.cells
    }

    pub fn apply_input(&mut self, action: &InputAction, pause_prompt: &[u8]) -> Result<(), Error> {
        match *action {
            InputAction::Put { row, column, byte } => {
                if row >= 25 || column >= 80 {
                    return Err(Error("input cell outside DOS screen".into()));
                }
                self.put(row, column, byte);
            }
            InputAction::Scroll {
                top,
                bottom,
                left,
                right,
            } => {
                if top > bottom || bottom >= 25 || left > right || right >= 80 {
                    return Err(Error("input scroll outside DOS screen".into()));
                }
                for row in top..bottom {
                    for column in left..=right {
                        let source = ((row + 1) * 80 + column) * 2;
                        let target = (row * 80 + column) * 2;
                        self.cells.copy_within(source..source + 2, target);
                    }
                }
                for column in left..=right {
                    self.put(bottom, column, b' ');
                }
            }
            InputAction::PauseStatus { active } => {
                self.status(if active { pause_prompt } else { &[] })
            }
            InputAction::ClearSubmitFlag => {}
            InputAction::Bell => {}
        }
        Ok(())
    }

    pub fn upper_cursor(&self) -> (usize, usize) {
        (self.row, self.column)
    }

    pub fn unicode(&self) -> String {
        let mut text = String::new();
        for row in self.cells.as_chunks::<160>().0 {
            for cell in row.as_chunks::<2>().0 {
                text.push(cp437(cell[0]));
            }
            text.push('\n');
        }
        text
    }
}

#[derive(Clone, Debug)]
enum Action {
    Token(Vec<u8>),
    TokenBody(Vec<u8>),
    AfterToken(u8),
    Separator,
    Byte(u8),
    Pause,
    ClearUpper,
    LineBreak(bool),
    Replay(Vec<u8>),
    ReplayByte(u8),
    EraseByte,
}

/// Stateful formatting shared across messages, with an explicit key-wait boundary.
#[derive(Clone, Debug)]
pub struct TextOutput {
    screen: TextScreen,
    pending: VecDeque<Action>,
    waiting: bool,
    concatenate: bool,
    capitalize: bool,
    uppercase: bool,
    quote: bool,
    sentence_space: bool,
    line_length: u16,
    word: Vec<u8>,
    page_remaining: u16,
    page_reset: u16,
    transcript_events: VecDeque<TranscriptEvent>,
    keyboard_polling: bool,
    keyboard_poll_pending: bool,
    callbacks: bool,
    region_wrap: bool,
    capturing: bool,
    callback_letter: u8,
    callback_pending: Option<u8>,
    value_pushes: Vec<u16>,
    bells: usize,
}

impl Default for TextOutput {
    fn default() -> Self {
        Self {
            screen: TextScreen::default(),
            pending: VecDeque::new(),
            waiting: false,
            concatenate: false,
            capitalize: true,
            uppercase: false,
            quote: false,
            sentence_space: false,
            line_length: 0,
            word: Vec::new(),
            page_remaining: 0xffff,
            page_reset: 0xffff,
            transcript_events: VecDeque::new(),
            keyboard_polling: false,
            keyboard_poll_pending: false,
            callbacks: false,
            region_wrap: false,
            capturing: false,
            callback_letter: 0,
            callback_pending: None,
            value_pushes: Vec::new(),
            bells: 0,
        }
    }
}

impl TextOutput {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn enable_callbacks(&mut self) {
        self.callbacks = true;
    }
    pub fn idle(&self) -> bool {
        self.pending.is_empty() && !self.waiting && !self.keyboard_poll_pending
    }
    pub fn take_callback(&mut self) -> Option<u8> {
        self.callback_pending.take()
    }
    pub fn take_value_pushes(&mut self) -> impl Iterator<Item = u16> + '_ {
        self.value_pushes.drain(..)
    }
    pub fn raw_character(&mut self, byte: u8, pause_prompt: &[u8]) {
        if matches!(byte, 11 | 12) {
            return;
        }
        self.pending.push_back(Action::Byte(byte));
        self.drain(pause_prompt);
    }
    pub(crate) fn set_region(
        &mut self,
        left: u16,
        top: u16,
        right: u16,
        bottom: u16,
        status: u16,
    ) -> Result<(), Error> {
        if left > right || right >= 80 || top > bottom || bottom >= 25 || status >= 25 {
            return Err(Error("text region outside DOS screen".into()));
        }
        self.region_wrap = true;
        self.screen.region = TextRegion {
            left: usize::from(left),
            top: usize::from(top),
            right: usize::from(right),
            bottom: usize::from(bottom),
            status: usize::from(status),
        };
        Ok(())
    }
    pub(crate) fn set_cursor(&mut self, row: u16, column: u16) -> Result<(), Error> {
        if row >= 25 || column >= 80 {
            return Err(Error("cursor outside DOS screen".into()));
        }
        self.screen.row = usize::from(row);
        self.screen.column = usize::from(column);
        self.line_length = column.wrapping_sub(self.screen.region.left as u16);
        self.word.clear();
        self.transcript_events.push_back(TranscriptEvent::MarkSpace);
        Ok(())
    }
    pub(crate) fn redraw_frame(&mut self, input_row: usize, input_column: usize) {
        let frame = TextScreen::default();
        for row in 0..24 {
            for column in 0..79 {
                if matches!(row, 0 | 18 | 23) || matches!(column, 0 | 78) {
                    self.screen
                        .put(row, column, frame.cells[(row * 80 + column) * 2]);
                }
            }
        }
        self.screen.put(input_row, input_column, 0xdb);
    }
    pub fn screen(&self) -> &TextScreen {
        &self.screen
    }
    pub fn apply_input(&mut self, action: &InputAction, pause_prompt: &[u8]) -> Result<(), Error> {
        self.screen.apply_input(action, pause_prompt)
    }
    pub fn waiting_for_key(&self) -> bool {
        self.waiting
    }
    /// Session enables the native post-character keyboard boundaries. Standalone
    /// formatting without a keyboard continues to drain synchronously.
    pub fn enable_keyboard_polling(&mut self) {
        self.keyboard_polling = true;
    }
    pub fn keyboard_poll_pending(&self) -> bool {
        self.keyboard_poll_pending
    }
    pub fn resume_keyboard_poll(&mut self, pause_prompt: &[u8]) {
        self.keyboard_poll_pending = false;
        self.drain(pause_prompt);
    }
    pub fn pagination_remaining(&self) -> u16 {
        self.page_remaining
    }
    pub fn take_transcript_events(&mut self) -> impl Iterator<Item = TranscriptEvent> + '_ {
        self.transcript_events.drain(..)
    }
    pub fn set_pagination(&mut self, remaining: u16, reset: u16) {
        self.page_remaining = remaining;
        self.page_reset = reset;
    }

    pub fn message(&mut self, tokens: Vec<Vec<u8>>, pause_prompt: &[u8]) -> Result<(), Error> {
        if self.waiting {
            return Err(Error("text output is waiting for a key".into()));
        }
        if tokens
            .iter()
            .any(|token| token.len() >= 40 || token.contains(&0))
        {
            return Err(Error("invalid decoded DOS text token".into()));
        }
        self.pending.extend(tokens.into_iter().map(Action::Token));
        self.drain(pause_prompt);
        Ok(())
    }

    /// Native services print completed input/unknown words through the same
    /// formatter as one C string, rather than a sequence of dictionary words.
    pub fn native_token(&mut self, token: &[u8], pause_prompt: &[u8]) -> Result<(), Error> {
        if self.waiting || token.len() > 77 || token.contains(&0) {
            return Err(Error(
                "invalid native input text or pending key wait".into(),
            ));
        }
        self.pending.push_back(Action::Token(token.to_vec()));
        self.drain(pause_prompt);
        Ok(())
    }

    pub fn suppress_case(&mut self) {
        self.capitalize = false;
        self.uppercase = false;
    }
    pub fn rebuild_frame(&mut self) {
        self.screen = TextScreen::default();
        self.line_length = 0;
    }
    pub fn clear_for_exit(&mut self) {
        self.screen.cells = [b' ', 7].repeat(2000);
        self.screen.row = 0;
        self.screen.column = 0;
    }

    /// Native service 3 pauses without clearing either pane after the key.
    pub fn wait_key(&mut self, pause_prompt: &[u8]) -> Result<(), Error> {
        if self.waiting {
            return Err(Error("text output is already waiting for a key".into()));
        }
        self.pending.push_back(Action::Pause);
        self.drain(pause_prompt);
        Ok(())
    }

    pub fn resume_key(&mut self, pause_prompt: &[u8]) -> Result<(), Error> {
        if !self.waiting {
            return Err(Error("text output has no pending key wait".into()));
        }
        self.waiting = false;
        self.screen.status(&[]);
        self.drain(pause_prompt);
        Ok(())
    }

    fn drain(&mut self, pause_prompt: &[u8]) {
        if self.keyboard_poll_pending {
            return;
        }
        while let Some(action) = self.pending.pop_front() {
            match action {
                Action::Token(token) => {
                    let first = token.first().copied().unwrap_or(0);
                    let no_separator = matches!(
                        first,
                        b')' | b']'
                            | b'?'
                            | b'!'
                            | b','
                            | b'.'
                            | b';'
                            | b':'
                            | b'@'
                            | 3
                            | 0x10
                            | 0x17
                            | 0x0e
                            | 0x15
                            | 6
                            | 2
                            | 0x14
                    ) || (first == b'"' && self.quote)
                        || (self.callbacks && (first == b'|' || self.capturing));
                    self.pending.push_front(Action::TokenBody(token));
                    if !self.concatenate && !no_separator {
                        self.pending.push_front(Action::Separator);
                    }
                }
                Action::TokenBody(token) => self.token(token),
                Action::AfterToken(first) => {
                    if matches!(first, b'(' | b'[') || (first == b'"' && self.quote) {
                        self.concatenate = true;
                    }
                    if matches!(first, b'.' | b'?' | b'!') {
                        self.capitalize = true;
                        self.sentence_space = true;
                    }
                }
                Action::Separator => {
                    if self.line_length == 0 {
                        self.sentence_space = false;
                    } else if self.sentence_space {
                        self.sentence_space = false;
                        self.pending.push_front(Action::Separator);
                        self.pending.push_front(Action::Byte(b' '));
                    } else {
                        self.pending.push_front(Action::Byte(b' '));
                    }
                }
                Action::Byte(byte) => self.character(byte),
                Action::Pause => {
                    self.waiting = true;
                    self.screen.status(pause_prompt);
                    break;
                }
                Action::ClearUpper => {
                    self.screen.clear_upper();
                    self.line_length = 0;
                    self.page_remaining = self.page_reset;
                    self.sentence_space = false;
                }
                Action::LineBreak(paginate) => {
                    if paginate && self.page_remaining != 0xffff {
                        self.page_remaining = self.page_remaining.wrapping_sub(1);
                        if self.page_remaining == 0 {
                            self.page_remaining = self.page_reset;
                            self.pending.push_front(Action::LineBreak(false));
                            self.pending.push_front(Action::Pause);
                            continue;
                        }
                    }
                    self.screen_character(10);
                    self.transcript_events.push_back(TranscriptEvent::LineBreak);
                    self.line_length = 0;
                    self.sentence_space = false;
                }
                Action::Replay(word) => {
                    for byte in word.into_iter().rev() {
                        self.pending.push_front(Action::ReplayByte(byte));
                    }
                }
                Action::ReplayByte(byte) => {
                    self.screen_character(byte);
                    self.transcript_events
                        .push_back(TranscriptEvent::Append(byte));
                    self.line_length = self.line_length.wrapping_add(1);
                }
                Action::EraseByte => self.screen_character(8),
            }
            if self.keyboard_poll_pending || self.callback_pending.is_some() {
                break;
            }
        }
    }

    pub fn take_bells(&mut self) -> usize {
        std::mem::take(&mut self.bells)
    }

    fn screen_character(&mut self, byte: u8) {
        if byte == 7 {
            self.bells += 1;
        }
        self.screen.character(byte);
        self.keyboard_poll_pending = self.keyboard_polling;
    }

    fn token(&mut self, mut token: Vec<u8>) {
        let Some(&first) = token.first() else {
            return;
        };
        if self.callbacks && (first == b'|' || self.capturing) {
            if first == b'|' {
                if self.capturing {
                    self.capturing = false;
                    self.callback_pending = Some(self.callback_letter);
                    return;
                }
                self.capturing = true;
            }
            for (index, byte) in token.into_iter().enumerate() {
                if byte.is_ascii_alphabetic() {
                    self.callback_letter = byte;
                }
                if byte.is_ascii_digit() {
                    self.value_pushes.push(u16::from(byte - b'0'));
                }
                if index != 0 && byte == b'|' {
                    self.capturing = false;
                    self.callback_pending = Some(self.callback_letter);
                }
            }
            return;
        }
        let mut actions = Vec::new();
        match first {
            0x0e => {}
            0x17 => self.uppercase = true,
            0x15 => self.capitalize = false,
            3 => self.capitalize = true,
            2 | 6 | 0x14 => {
                self.concatenate = true;
                self.sentence_space = false;
                actions.extend(
                    (0..match first {
                        2 => 1,
                        6 => 20,
                        _ => 9,
                    })
                        .map(|_| Action::Byte(b' ')),
                );
            }
            b'@' | 0x10 => {
                self.concatenate = true;
                self.sentence_space = false;
                self.quote = false;
                actions.push(Action::Byte(10));
                if first == 0x10 {
                    actions.extend([Action::Byte(b' '), Action::Byte(b' ')]);
                    self.capitalize = true;
                }
            }
            0x0c => actions.extend([Action::Pause, Action::ClearUpper]),
            _ => {
                if first == b'"' {
                    self.quote = !self.quote;
                }
                self.concatenate = false;
                if first.is_ascii_alphabetic() || token.len() > 1 {
                    if self.capitalize {
                        token[0].make_ascii_uppercase();
                        self.capitalize = false;
                    }
                    if self.uppercase {
                        token.make_ascii_uppercase();
                        self.uppercase = false;
                    }
                }
                actions.extend(token.into_iter().map(Action::Byte));
                actions.push(Action::AfterToken(first));
            }
        }
        for action in actions.into_iter().rev() {
            self.pending.push_front(action);
        }
    }

    fn character(&mut self, byte: u8) {
        if byte == 13 {
            return;
        }
        if byte == 10 {
            self.transcript_events.push_back(TranscriptEvent::MarkSpace);
            self.pending.push_front(Action::LineBreak(true));
            self.word.clear();
            return;
        }
        self.screen_character(byte);
        self.transcript_events
            .push_back(TranscriptEvent::Append(byte));
        self.line_length = self.line_length.wrapping_add(1);
        self.word.push(byte);
        if self
            .line_length
            .wrapping_add(self.screen.region.left as u16)
            > self.screen.region.right as u16
        {
            if byte == b' '
                || (self.region_wrap
                    && self.word.len() >= self.screen.region.right - self.screen.region.left)
            {
                self.pending.push_front(Action::LineBreak(true));
            } else {
                let erase_count = self.word.len();
                self.pending
                    .push_front(Action::Replay(std::mem::take(&mut self.word)));
                self.pending.push_front(Action::LineBreak(true));
                for _ in 0..erase_count {
                    self.pending.push_front(Action::EraseByte);
                }
            }
        }
        if byte == b' ' {
            self.transcript_events.push_back(TranscriptEvent::MarkSpace);
            self.word.clear();
        }
    }
}

fn cp437(byte: u8) -> char {
    match byte {
        0 => ' ',
        1..=31 => "☺☻♥♦♣♠•◘○◙♂♀♪♫☼►◄↕‼¶§▬↨↑↓→←∟↔▲▼".chars().nth(usize::from(byte - 1)).unwrap(),
        32..=126 => char::from(byte),
        127 => '⌂',
        _ => "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■ ".chars().nth(usize::from(byte - 128)).unwrap(),
    }
}
