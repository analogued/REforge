// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native DOS tokenization, dictionary lookup and parser-service facade.

pub mod grammar;

use std::collections::BTreeSet;

use crate::{
    Error, ObjectId, VarAddr,
    image::{StoryImage, dos_data},
    read_word,
    text::TextCodec,
    vm::Vm,
};
use grammar::{GrammarResult, GrammarState};

/// The native lexical state before grammar matching. Raw and lowercase buffers
/// include the submitted CR; token and dictionary lists omit C terminators.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LexicalLine {
    pub raw: Vec<u8>,
    pub lowercase: Vec<u8>,
    pub tokens: Vec<Vec<u8>>,
    pub recognized: Vec<u16>,
    pub token_count: u16,
    pub input_offset: u16,
    pub unknown: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseResult {
    pub unknown: bool,
    /// Address of the trailing 0xffff, relative to the variable image.
    pub output_end: VarAddr,
}

pub struct Parser {
    classes: [u8; 128],
    grammar: GrammarState,
    lexical: LexicalLine,
    unknown_word: Vec<u8>,
    grammar_result: Option<GrammarResult>,
    output_end: VarAddr,
}

impl Parser {
    /// Reads the observed DOS character classes; this is not revision detection.
    pub fn from_dos_executable(exe: &[u8]) -> Result<Self, Error> {
        let classes = dos_data(exe)?
            .get(0x7f..0xff)
            .ok_or_else(|| Error("missing DOS parser character classes".into()))?;
        Ok(Self::from_classes(classes.try_into().unwrap()))
    }

    pub fn from_classes(classes: [u8; 128]) -> Self {
        Self {
            classes,
            grammar: GrammarState::default(),
            lexical: LexicalLine::default(),
            unknown_word: Vec::new(),
            grammar_result: None,
            output_end: VarAddr(0x2e),
        }
    }

    pub fn last_lexical(&self) -> &LexicalLine {
        &self.lexical
    }

    pub fn last_grammar(&self) -> Option<&GrammarResult> {
        self.grammar_result.as_ref()
    }

    /// Last completed input as a C string, preserving original case and CR.
    pub fn last_input(&self) -> &[u8] {
        c_string(&self.lexical.raw)
    }

    pub fn lower_input(&self) -> &[u8] {
        c_string(&self.lexical.lowercase)
    }

    /// Native storage retains the previous unknown word until another miss.
    pub fn unknown_word(&self) -> &[u8] {
        &self.unknown_word
    }

    pub fn output_end(&self) -> VarAddr {
        self.output_end
    }

    pub fn remember_object(&mut self, object: ObjectId, maximum: u16) -> Result<(), Error> {
        self.grammar.remember_object(object, maximum)
    }

    pub fn reset_context(&mut self) {
        self.grammar.reset();
    }
    pub fn history(&self) -> &[u16; 100] {
        self.grammar.history()
    }
    pub fn history_head(&self) -> usize {
        self.grammar.history_head()
    }
    pub fn restore_history(
        &mut self,
        history: [u16; 100],
        head: usize,
        maximum: u16,
    ) -> Result<(), Error> {
        self.grammar.restore_history(history, head, maximum)
    }

    /// Exact dictionary tree search. Eligibility for input is a separate cutoff
    /// check, since finding a text-only dictionary word is not an unknown miss.
    pub fn dictionary_lookup(
        &self,
        image: &StoryImage,
        codec: &TextCodec,
        token: &[u8],
    ) -> Result<Option<u16>, Error> {
        let dictionary = image.store(3)?;
        let mut address = 2u16;
        let mut visited = BTreeSet::new();
        while address != 0 {
            if !visited.insert(address) {
                return Err(Error("cyclic input dictionary tree".into()));
            }
            let text = codec.word(image, address)?;
            match token.cmp(&text) {
                std::cmp::Ordering::Equal => return Ok(Some(address)),
                std::cmp::Ordering::Less => {
                    address = read_word(dictionary, usize::from(address), "dictionary left")?;
                }
                std::cmp::Ordering::Greater => {
                    address = read_word(dictionary, usize::from(address) + 2, "dictionary right")?;
                }
            }
        }
        Ok(None)
    }

    /// The native tokenizer receives the lowercase C buffer. Class bits, rather
    /// than Unicode or host whitespace rules, decide starts and delimiters.
    pub fn tokenize(&self, input: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
        let mut position = 0;
        let mut tokens = Vec::new();
        while let Some(token) = self.next_token(input, &mut position)? {
            tokens.push(token);
        }
        Ok(tokens)
    }

    pub fn lex_line(
        &mut self,
        image: &StoryImage,
        codec: &TextCodec,
        raw: &[u8],
    ) -> Result<&LexicalLine, Error> {
        if raw.len() > 77 {
            return Err(Error("submitted input exceeds DOS editor capacity".into()));
        }
        // The line copier requires CR. A ready buffer lacking it is discarded.
        let raw = raw
            .iter()
            .position(|&byte| byte == 13)
            .map_or_else(Vec::new, |end| raw[..=end].to_vec());
        let lowercase: Vec<_> = raw.iter().map(u8::to_ascii_lowercase).collect();
        let cutoff = read_word(image.store(3)?, 0, "input dictionary cutoff")?;
        let mut line = LexicalLine {
            raw,
            lowercase,
            ..LexicalLine::default()
        };
        let mut position = 0;
        while line.recognized.len() < 39 {
            let Some(token) = self.next_token(&line.lowercase, &mut position)? else {
                break;
            };
            match self.dictionary_lookup(image, codec, &token)? {
                Some(address) if address <= cutoff && address != 0xffff => {
                    line.recognized.push(address)
                }
                Some(_) => {}
                None => {
                    line.unknown = true;
                    self.unknown_word.clone_from(&token);
                }
            }
            line.tokens.push(token);
            line.token_count = line.token_count.wrapping_add(1);
        }
        line.input_offset = position as u16;
        self.lexical = line;
        self.grammar_result = None;
        Ok(&self.lexical)
    }

    /// Executes the algorithms of service 1 after a completed line has been
    /// copied. The session retains responsibility for timeout and VM control flow.
    pub fn parse_line(
        &mut self,
        image: &StoryImage,
        codec: &TextCodec,
        vm: &mut Vm,
        raw: &[u8],
    ) -> Result<ParseResult, Error> {
        self.lex_line(image, codec, raw)?;
        vm.write_variable(VarAddr(0xd8), self.lexical.token_count)?;
        let grammar = self.grammar.parse(image, vm, &self.lexical.recognized)?;
        let end = 0x2eusize + grammar.output.len() * 2;
        if end + 2 > vm.state().variables.len() || end > usize::from(u16::MAX) {
            return Err(Error("parsed output exceeds variable image".into()));
        }
        for (index, value) in grammar.output.iter().copied().chain([0xffff]).enumerate() {
            vm.write_variable(VarAddr((0x2e + index * 2) as u16), value)?;
        }
        self.output_end = VarAddr(end as u16);
        self.grammar_result = Some(grammar);
        Ok(ParseResult {
            unknown: self.lexical.unknown,
            output_end: self.output_end,
        })
    }

    fn next_token(&self, input: &[u8], position: &mut usize) -> Result<Option<Vec<u8>>, Error> {
        let byte = loop {
            let byte = input.get(*position).copied().unwrap_or(0);
            *position += 1;
            if byte == 0 {
                return Ok(None);
            }
            if self.classes[usize::from(byte & 0x7f)] & 2 != 0 {
                break byte;
            }
        };
        let mut token = vec![byte];
        if self.classes[usize::from(byte & 0x7f)] & 4 != 0 {
            return Ok(Some(token));
        }
        loop {
            let byte = input.get(*position).copied().unwrap_or(0);
            *position += 1;
            if self.classes[usize::from(byte & 0x7f)] & 4 != 0 {
                *position -= 1;
                return Ok(Some(token));
            }
            if byte == 0 {
                return Err(Error(
                    "parser character classes do not terminate C input".into(),
                ));
            }
            token.push(byte);
        }
    }
}

fn c_string(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len())]
}
