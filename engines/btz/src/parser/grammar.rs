// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Observed DOS grammar matching over external object phrase tables.

use crate::{Error, ObjectId, ProgramAddr, VarAddr, image::StoryImage, read_word, vm::Vm};

const NONE: u16 = 0xffff;
const HISTORY: usize = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrammarResult {
    pub output: Vec<u16>,
    pub objects: Vec<u16>,
    pub used: Vec<u16>,
    pub remaining_count: u16,
    /// (recognized-token index, object), in native encounter order.
    pub alternatives: Vec<(u16, u16)>,
}

#[derive(Clone, Debug)]
pub struct GrammarState {
    history: [u16; HISTORY],
    head: usize,
    free_match: bool,
    allow_gaps: bool,
}

impl Default for GrammarState {
    fn default() -> Self {
        let mut history = [0; HISTORY];
        history[..2].fill(NONE);
        Self {
            history,
            head: 0,
            free_match: false,
            allow_gaps: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Layout {
    phrases: u16,
    flags: u16,
    interval_lower: u16,
    interval_upper: u16,
    interval_bytes: u16,
    max_object: u16,
}
impl Layout {
    fn read(image: &StoryImage) -> Result<Self, Error> {
        let at = |offset| read_word(image.initial_variables(), offset, "grammar layout");
        let layout = Self {
            phrases: at(0xf0)?,
            flags: at(0x110)?,
            interval_lower: at(0xf6)?,
            interval_upper: at(0xf8)?,
            interval_bytes: at(0xfa)?,
            max_object: at(0x102)?,
        };
        if layout.max_object == NONE {
            return Err(Error("grammar object scan would wrap".into()));
        }
        Ok(layout)
    }
    fn bounds(self, image: &StoryImage, object: u16) -> Result<(u16, u16), Error> {
        let end = self.phrases.wrapping_add(object.wrapping_mul(2));
        Ok((
            image.program_word(ProgramAddr(end.wrapping_sub(2)))?,
            image.program_word(ProgramAddr(end))?,
        ))
    }
    fn object_flags(self, vm: &Vm, object: u16) -> Result<u16, Error> {
        let mut offset = 0;
        while offset < self.interval_bytes {
            let lower = vm.variable(VarAddr(self.interval_lower.wrapping_add(offset)))?;
            let upper = vm.variable(VarAddr(self.interval_upper.wrapping_add(offset)))?;
            if lower <= object && upper >= object {
                break;
            }
            offset = offset
                .checked_add(2)
                .ok_or_else(|| Error("grammar interval scan wraps".into()))?;
        }
        vm.variable(VarAddr(self.flags.wrapping_add(offset)))
    }
}

#[derive(Clone, Copy)]
struct Candidate {
    start: u16,
    end: u16,
    prefix_end: u16,
}
struct Line<'a> {
    tokens: &'a [u16],
    objects: Vec<u16>,
    used: Vec<u16>,
    remaining: u16,
    candidates: Vec<Candidate>,
    alternatives: Vec<(u16, u16)>,
}

impl GrammarState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn history(&self) -> &[u16; HISTORY] {
        &self.history
    }
    pub fn history_head(&self) -> usize {
        self.head
    }
    pub fn restore_history(
        &mut self,
        history: [u16; HISTORY],
        head: usize,
        maximum: u16,
    ) -> Result<(), Error> {
        if head >= HISTORY
            || history
                .iter()
                .any(|&value| value != NONE && (value > maximum || value > 0x7fff))
        {
            return Err(Error("invalid bookmark grammar history".into()));
        }
        self.history = history;
        self.head = head;
        Ok(())
    }
    pub fn free_match(&self) -> bool {
        self.free_match
    }
    pub fn allow_gaps(&self) -> bool {
        self.allow_gaps
    }

    /// Native startup resets the ring without changing the sticky match flags.
    pub fn reset(&mut self) {
        self.history[..2].fill(NONE);
        self.head = 0;
    }

    pub fn remember_object(&mut self, object: ObjectId, max_object: u16) -> Result<(), Error> {
        if object.0 == NONE {
            return Ok(());
        }
        if object.0 > i16::MAX as u16 || object.0 > max_object {
            return Err(Error(
                "remembered grammar object outside native range".into(),
            ));
        }
        self.head = (self.head + 1) % HISTORY;
        self.history[self.head] = object.0;
        self.history[(self.head + 1) % HISTORY] = NONE;
        Ok(())
    }

    /// Recognized dictionary addresses exclude their terminating 0xffff.
    /// Matching does not change the VM accumulator/argument: the DOS service
    /// wrapper preserves them around this algorithm.
    pub fn parse(
        &mut self,
        image: &StoryImage,
        vm: &mut Vm,
        tokens: &[u16],
    ) -> Result<GrammarResult, Error> {
        self.parse_with_numbers(image, vm, tokens, &[])
    }

    pub(crate) fn parse_with_numbers(
        &mut self,
        image: &StoryImage,
        vm: &mut Vm,
        tokens: &[u16],
        numbers: &[super::NumericToken],
    ) -> Result<GrammarResult, Error> {
        if tokens.len() > 39 || tokens.contains(&NONE) {
            return Err(Error("invalid recognized-token vector".into()));
        }
        let layout = Layout::read(image)?;
        let mut line = Line {
            tokens,
            objects: vec![NONE; tokens.len()],
            used: vec![0; tokens.len()],
            remaining: tokens.len() as u16,
            candidates: Vec::new(),
            alternatives: Vec::new(),
        };
        if !tokens.is_empty() {
            let mut index = self.head;
            for _ in 0..HISTORY {
                let object = self.history[index];
                if object == NONE || line.remaining == 0 {
                    break;
                }
                if object > layout.max_object {
                    return Err(Error("remembered grammar object outside image".into()));
                }
                let bounds = layout.bounds(image, object)?;
                if bounds.0 != bounds.1 {
                    self.match_object(image, vm, layout, object, bounds, &mut line)?;
                }
                index = if index == 0 { HISTORY - 1 } else { index - 1 };
            }
            self.free_match = true;
            for object in 0..=layout.max_object {
                if !self.free_match && line.remaining == 0 {
                    break;
                }
                let bounds = layout.bounds(image, object)?;
                if bounds.0 != bounds.1 {
                    self.match_object(image, vm, layout, object, bounds, &mut line)?;
                }
            }
        }
        let mut output = Vec::new();
        for index in 0..=line.objects.len() {
            for number in numbers
                .iter()
                .filter(|number| usize::from(number.position) == index)
            {
                output.push(number.object);
            }
            let Some(&object) = line.objects.get(index) else {
                break;
            };
            if object != NONE {
                output.push(object);
            }
            for &(position, object) in &line.alternatives {
                if usize::from(position) == index && object != NONE {
                    output.push(object);
                }
            }
        }
        if output.len() >= 80 {
            return Err(Error("parsed output exceeds DOS variable buffer".into()));
        }
        Ok(GrammarResult {
            output,
            objects: line.objects,
            used: line.used,
            remaining_count: line.remaining,
            alternatives: line.alternatives,
        })
    }

    fn match_object(
        &mut self,
        image: &StoryImage,
        vm: &Vm,
        layout: Layout,
        object: u16,
        bounds: (u16, u16),
        line: &mut Line<'_>,
    ) -> Result<(), Error> {
        let (words, starts) = phrases(image.store(1)?, bounds)?;
        let mut flags_initialized = false;
        for start in starts {
            let mut word_index = start;
            let mut first = NONE;
            let mut prefix = true;
            let mut prefix_end = 0;
            let mut positions = Vec::new();
            for (index, &token) in line.tokens.iter().enumerate() {
                let index = index as u16;
                if words[word_index] == token {
                    positions.push(index);
                    if first == NONE {
                        first = index;
                        if !flags_initialized {
                            let flags = layout.object_flags(vm, object)?;
                            self.free_match = flags & 1 != 0;
                            self.allow_gaps = flags & 2 != 0;
                            flags_initialized = true;
                        }
                    }
                    if words[word_index + 1] == NONE {
                        let end = index;
                        if prefix {
                            prefix_end = index;
                        }
                        let candidate = Candidate {
                            start: first,
                            end,
                            prefix_end,
                        };
                        if self.free_match || accept(candidate, &mut line.candidates) {
                            if self.free_match {
                                if line.alternatives.len() >= 70 {
                                    return Err(Error(
                                        "grammar alternative buffer exhausted".into(),
                                    ));
                                }
                                line.alternatives.push((first, object));
                            } else {
                                if line.candidates.len() >= 40 {
                                    return Err(Error("grammar candidate buffer exhausted".into()));
                                }
                                line.candidates.push(candidate);
                                for &position in &positions {
                                    let position = usize::from(position);
                                    line.objects[position] = NONE;
                                    if line.used[position] == 0 {
                                        line.used[position] = 1;
                                        line.remaining -= 1;
                                    }
                                }
                                line.objects[usize::from(first)] = object;
                            }
                        }
                        word_index = start;
                        first = NONE;
                        positions.clear();
                    } else {
                        word_index += 1;
                    }
                } else if first != NONE {
                    if !self.allow_gaps {
                        word_index = start;
                        first = NONE;
                        positions.clear();
                    } else if prefix {
                        prefix_end = index - 1;
                        prefix = false;
                    }
                }
            }
        }
        Ok(())
    }
}

fn phrases(bytes: &[u8], (start, end): (u16, u16)) -> Result<(Vec<u16>, Vec<usize>), Error> {
    let mut position = usize::from(start);
    let end = usize::from(end);
    if position > end || end > bytes.len() {
        return Err(Error("grammar phrase range outside store".into()));
    }
    let mut words = Vec::new();
    let mut starts = vec![0];
    while position < end {
        // The native 400-word buffer also needs the final sentinel. Reaching
        // all 400 decoded entries would overwrite the pattern-start array.
        if words.len() >= 399 {
            return Err(Error("grammar phrase buffer exhausted".into()));
        }
        let high = bytes[position];
        position += 1;
        if high == 0xff {
            words.push(NONE);
            if starts.len() >= 131 {
                return Err(Error("grammar pattern-start buffer exhausted".into()));
            }
            starts.push(words.len());
        } else {
            if position >= end {
                return Err(Error("truncated grammar word reference".into()));
            }
            words.push(u16::from_be_bytes([high, bytes[position]]));
            position += 1;
        }
    }
    words.push(NONE);
    Ok((words, starts))
}

fn accept(new: Candidate, old: &mut [Candidate]) -> bool {
    for prior in old {
        if new.end < prior.start || new.start > prior.end || prior.start == NONE {
            continue;
        }
        if (new.start >= prior.start && new.end <= prior.prefix_end)
            || (new.start == new.end && new.start == prior.end)
            || (new.start >= prior.start
                && new.prefix_end <= prior.prefix_end
                && new.end == prior.end)
        {
            return false;
        }
        if (new.start <= prior.start && new.prefix_end >= prior.end)
            || (new.end == prior.start && new.end == prior.end)
        {
            prior.start = NONE;
            continue;
        }
        if (new.start > prior.prefix_end && new.end < prior.end)
            || (prior.start > new.prefix_end && prior.end < new.end)
        {
            continue;
        }
        return false;
    }
    true
}
