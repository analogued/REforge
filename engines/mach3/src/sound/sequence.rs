// Copyright (c) 2026 analogued. Project license has not been selected.
//! Decode and execute one bounded traversal of the original intro wrappers.
//!
//! The original first wrapper repeats calls as unrolled instruction blocks.
//! Its return checks occur between groups, while the second wrapper always runs
//! all of its calls. The surrounding timed intro repetition is not modeled here.

use super::{Resource, SampleRequest};
use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntroCall {
    pub request: SampleRequest,
    pub return_ip: u16,
}

#[derive(Clone, Copy, Debug)]
struct Step {
    call: IntroCall,
    checkpoint: bool,
}

#[derive(Clone, Debug)]
pub struct IntroProgram {
    steps: Vec<Step>,
    tail_start: usize,
    pregame: SampleRequest,
}

impl IntroProgram {
    /// Read sample arguments and group boundaries from the expanded code image.
    ///
    /// This accepts the verified wrapper instruction forms, rather than a CPU
    /// instruction set. Loader-created resource buffers have canonical offset
    /// zero; their DS pointer-word operands identify resources without depending
    /// on relocated DOS allocation segments.
    pub fn from_expanded(image: &[u8]) -> Result<Self, Error> {
        let decoder = Decoder { image };
        decoder.expect(0x5025, &[0x55, 0x83, 0xec, 4, 0x8b, 0xec])?;
        decoder.call(0x502b, 0x503f)?;
        decoder.expect(0x502e, &[0x89, 0x46, 2])?;
        decoder.call(0x5031, 0x5308)?;
        decoder.expect(
            0x5034,
            &[0x09, 0x46, 2, 0x8b, 0x46, 2, 0x83, 0xc4, 4, 0x5d, 0xc3],
        )?;

        decoder.expect(
            0x503f,
            &[
                0x55, 0x83, 0xec, 8, 0x8b, 0xec, 0x33, 0xc0, 0xc7, 0x46, 6, 1, 0,
            ],
        )?;
        decoder.expect(0x504c, &[0x50, 0xbb])?;
        decoder.expect(0x5050, &[0x53])?;
        let first = decoder.arguments(0x5051, 0x504e, 0x505c, false)?;
        decoder.expect(0x5059, &[0x89, 0x46, 4])?;
        decoder.expect(
            0x505f,
            &[
                0x8b, 0xe5, 0x3d, 0x7f, 0, 0x89, 0x46, 2, 0x7e, 5, 0xc7, 0x46, 2, 0, 0,
            ],
        )?;
        let mut steps = vec![Step {
            call: first,
            checkpoint: false,
        }];
        let mut cursor = 0x506e;
        loop {
            let (call, next) = decoder.forward_block(cursor, 4, 2)?;
            cursor = next;
            let mut checkpoint = false;
            if decoder.bytes(cursor, 2)? == [0x74, 8] {
                decoder.epilogue(cursor + 2, 2)?;
                cursor += 10;
                checkpoint = true;
            } else if decoder.bytes(cursor, 3)? == [0x8b, 0x46, 2] {
                decoder.epilogue(cursor, 2)?;
                cursor += 8;
                steps.push(Step {
                    call,
                    checkpoint: true,
                });
                break;
            }
            steps.push(Step { call, checkpoint });
            if cursor >= 0x5308 {
                return Err(invalid("first wrapper has no valid final return"));
            }
        }
        if cursor != 0x5308 {
            return Err(invalid("first wrapper ends outside its verified boundary"));
        }
        let tail_start = steps.len();

        decoder.expect(
            0x5308,
            &[
                0x55, 0x83, 0xec, 8, 0x8b, 0xec, 0x33, 0xc0, 0xc7, 0x46, 4, 1, 0,
            ],
        )?;
        decoder.expect(0x5315, &[0x50, 0xbb])?;
        decoder.expect(0x5319, &[0x53])?;
        decoder.expect(0x5322, &[0x89, 0x46, 2])?;
        let first_tail = decoder.arguments(0x531a, 0x5317, 0x5325, false)?;
        decoder.expect(0x5328, &[0x8b, 0xe5, 0xff, 0x76, 2, 0xbb])?;
        decoder.expect(0x5330, &[0x53])?;
        decoder.expect(0x5339, &[0x89, 0x46, 6])?;
        let second_tail = decoder.arguments(0x5331, 0x532e, 0x533c, false)?;
        decoder.expect(0x533f, &[0x8b, 0xe5, 0x09, 0x46, 6])?;
        let (third_tail, cursor) = decoder.forward_block(0x5344, 2, 6)?;
        let (last_tail, cursor) = decoder.ordinary_block(cursor, 4, 6, true)?;
        decoder.epilogue(cursor, 6)?;
        if cursor + 8 != 0x537a {
            return Err(invalid("second wrapper ends outside its verified boundary"));
        }
        steps.extend(
            [first_tail, second_tail, third_tail, last_tail].map(|call| Step {
                call,
                checkpoint: false,
            }),
        );

        decoder.expect(
            0x4f8d,
            &[
                0x55, 0x83, 0xec, 2, 0x8b, 0xec, 0x83, 0x3e, 0x66, 0xb6, 0, 0x74, 0x14,
            ],
        )?;
        decoder.expect(0x4f9a, &[0x33, 0xc0, 0x50, 0xb8])?;
        decoder.expect(0x4fa0, &[0x50])?;
        let pregame = decoder.arguments(0x4fa1, 0x4f9e, 0x4fa9, false)?.request;
        decoder.expect(0x4fac, &[0x8b, 0xe5, 0x83, 0xc4, 2, 0x5d, 0xc3])?;
        Ok(Self {
            steps,
            tail_start,
            pregame,
        })
    }

    pub fn pregame(&self) -> SampleRequest {
        self.pregame
    }
}

/// A resumable execution of 5025, with sample completion supplied by its caller.
#[derive(Clone, Debug)]
pub struct IntroSequence {
    steps: Vec<Step>,
    tail_start: usize,
    cursor: usize,
    first_result: u16,
    second_result: u16,
    result: Option<u16>,
}

impl IntroSequence {
    pub fn new(program: &IntroProgram) -> Self {
        Self {
            steps: program.steps.clone(),
            tail_start: program.tail_start,
            cursor: 0,
            first_result: 0,
            second_result: 0,
            result: None,
        }
    }

    pub fn current(&self) -> Option<IntroCall> {
        self.steps.get(self.cursor).map(|step| step.call)
    }

    pub fn complete(&mut self, mut result: u16) -> Result<(), Error> {
        let step = *self
            .steps
            .get(self.cursor)
            .ok_or_else(|| invalid("intro sequence is complete"))?;
        if self.cursor < self.tail_start {
            if self.cursor == 0 && (result as i16) > 127 {
                result = 0;
            }
            self.first_result |= result;
            self.cursor += 1;
            if step.checkpoint && self.first_result != 0 {
                self.cursor = self.tail_start;
            }
        } else {
            self.second_result |= result;
            self.cursor += 1;
        }
        if self.cursor == self.steps.len() {
            self.result = Some(self.first_result | self.second_result);
        }
        Ok(())
    }

    pub fn result(&self) -> Option<u16> {
        self.result
    }
}

struct Decoder<'a> {
    image: &'a [u8],
}

impl Decoder<'_> {
    fn bytes(&self, offset: usize, length: usize) -> Result<&[u8], Error> {
        self.image
            .get(offset..offset + length)
            .ok_or_else(|| invalid("truncated wrapper instructions"))
    }

    fn expect(&self, offset: usize, expected: &[u8]) -> Result<(), Error> {
        if self.bytes(offset, expected.len())? != expected {
            return Err(invalid(&format!(
                "unsupported wrapper instructions at {offset:04x}"
            )));
        }
        Ok(())
    }

    fn word(&self, offset: usize) -> Result<u16, Error> {
        let bytes = self.bytes(offset, 2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn call(&self, offset: usize, target: u16) -> Result<(), Error> {
        self.expect(offset, &[0xe8])?;
        if (offset as u16)
            .wrapping_add(3)
            .wrapping_add(self.word(offset + 1)?)
            != target
        {
            return Err(invalid(&format!(
                "unsupported wrapper call at {offset:04x}"
            )));
        }
        Ok(())
    }

    fn arguments(
        &self,
        pointers: usize,
        count: usize,
        call: usize,
        reverse: bool,
    ) -> Result<IntroCall, Error> {
        self.expect(pointers, &[0xff, 0x36])?;
        self.expect(pointers + 4, &[0xff, 0x36])?;
        let segment_word = self.word(pointers + 2)?;
        let offset_word = self.word(pointers + 6)?;
        if offset_word.checked_add(2) != Some(segment_word) {
            return Err(invalid(
                "sample argument pointer words do not form a far pointer",
            ));
        }
        let resource = Resource::from_pointer_word(offset_word)
            .ok_or_else(|| invalid("unknown sample resource pointer"))?;
        self.call(call, 0x55f9)?;
        Ok(IntroCall {
            request: SampleRequest {
                resource,
                offset: 0,
                count: self.word(count)?,
                reverse,
            },
            return_ip: (call + 3) as u16,
        })
    }

    fn ordinary_block(
        &self,
        start: usize,
        direction_local: u8,
        result_local: u8,
        reverse: bool,
    ) -> Result<(IntroCall, usize), Error> {
        self.expect(start, &[0xff, 0x76, direction_local, 0xb8])?;
        self.expect(start + 6, &[0x50])?;
        let call = self.arguments(start + 7, start + 4, start + 15, reverse)?;
        self.expect(start + 18, &[0x8b, 0xe5, 0x09, 0x46, result_local])?;
        Ok((call, start + 23))
    }

    fn forward_block(
        &self,
        start: usize,
        direction_local: u8,
        result_local: u8,
    ) -> Result<(IntroCall, usize), Error> {
        if self.bytes(start + 3, 3)? == [0x33, 0xc0, 0x50] {
            // The long-count source form has an extra ignored zero argument.
            self.expect(
                start,
                &[0xff, 0x76, direction_local, 0x33, 0xc0, 0x50, 0xb8],
            )?;
            self.expect(start + 9, &[0x50])?;
            let call = self.arguments(start + 10, start + 7, start + 18, false)?;
            self.expect(start + 21, &[0x8b, 0xe5, 0x09, 0x46, result_local])?;
            Ok((call, start + 26))
        } else {
            self.ordinary_block(start, direction_local, result_local, false)
        }
    }

    fn epilogue(&self, start: usize, result_local: u8) -> Result<(), Error> {
        self.expect(
            start,
            &[0x8b, 0x46, result_local, 0x83, 0xc4, 8, 0x5d, 0xc3],
        )
    }
}

fn invalid(message: &str) -> Error {
    Error::InvalidData(format!("Mach 3 intro sound: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(image: &mut [u8], offset: usize, bytes: &[u8]) {
        image[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    fn word(image: &mut [u8], offset: usize, value: u16) {
        put(image, offset, &value.to_le_bytes());
    }

    fn call(image: &mut [u8], offset: usize, target: u16) {
        put(image, offset, &[0xe8]);
        word(image, offset + 1, target.wrapping_sub((offset + 3) as u16));
    }

    fn arguments(image: &mut [u8], pointer: usize, count: usize, call_ip: usize, value: u16) {
        put(
            image,
            pointer,
            &[0xff, 0x36, 0x48, 0x0f, 0xff, 0x36, 0x46, 0x0f],
        );
        word(image, count, value);
        call(image, call_ip, 0x55f9);
    }

    fn epilogue(image: &mut [u8], offset: usize, result: u8) {
        put(
            image,
            offset,
            &[0x8b, 0x46, result, 0x83, 0xc4, 8, 0x5d, 0xc3],
        );
    }

    fn block(
        image: &mut [u8],
        start: usize,
        local: u8,
        result: u8,
        value: u16,
        extra: bool,
    ) -> usize {
        if extra {
            put(image, start, &[0xff, 0x76, local, 0x33, 0xc0, 0x50, 0xb8]);
            put(image, start + 9, &[0x50]);
            arguments(image, start + 10, start + 7, start + 18, value);
            put(image, start + 21, &[0x8b, 0xe5, 0x09, 0x46, result]);
            start + 26
        } else {
            put(image, start, &[0xff, 0x76, local, 0xb8]);
            put(image, start + 6, &[0x50]);
            arguments(image, start + 7, start + 4, start + 15, value);
            put(image, start + 18, &[0x8b, 0xe5, 0x09, 0x46, result]);
            start + 23
        }
    }

    // Authored instruction fixture: every resource is Data7 and counts increase
    // from 100. No asset bytes, executable slices, or captured calls are used.
    fn fixture() -> Vec<u8> {
        let mut image = vec![0; 0x537a];
        put(&mut image, 0x5025, &[0x55, 0x83, 0xec, 4, 0x8b, 0xec]);
        call(&mut image, 0x502b, 0x503f);
        put(&mut image, 0x502e, &[0x89, 0x46, 2]);
        call(&mut image, 0x5031, 0x5308);
        put(
            &mut image,
            0x5034,
            &[0x09, 0x46, 2, 0x8b, 0x46, 2, 0x83, 0xc4, 4, 0x5d, 0xc3],
        );
        put(
            &mut image,
            0x503f,
            &[
                0x55, 0x83, 0xec, 8, 0x8b, 0xec, 0x33, 0xc0, 0xc7, 0x46, 6, 1, 0,
            ],
        );
        put(&mut image, 0x504c, &[0x50, 0xbb]);
        put(&mut image, 0x5050, &[0x53]);
        arguments(&mut image, 0x5051, 0x504e, 0x505c, 100);
        put(&mut image, 0x5059, &[0x89, 0x46, 4]);
        put(
            &mut image,
            0x505f,
            &[
                0x8b, 0xe5, 0x3d, 0x7f, 0, 0x89, 0x46, 2, 0x7e, 5, 0xc7, 0x46, 2, 0, 0,
            ],
        );
        let mut cursor = 0x506e;
        let mut count = 101;
        for (group, size) in [1, 2, 4, 3, 2, 2, 2, 2, 2, 2, 2].into_iter().enumerate() {
            for member in 0..size {
                cursor = block(&mut image, cursor, 4, 2, count, group == 2 && member >= 2);
                count += 1;
            }
            if group == 10 {
                epilogue(&mut image, cursor, 2);
                cursor += 8;
            } else {
                put(&mut image, cursor, &[0x74, 8]);
                epilogue(&mut image, cursor + 2, 2);
                cursor += 10;
            }
        }
        assert_eq!(cursor, 0x5308);
        put(
            &mut image,
            0x5308,
            &[
                0x55, 0x83, 0xec, 8, 0x8b, 0xec, 0x33, 0xc0, 0xc7, 0x46, 4, 1, 0,
            ],
        );
        put(&mut image, 0x5315, &[0x50, 0xbb]);
        put(&mut image, 0x5319, &[0x53]);
        arguments(&mut image, 0x531a, 0x5317, 0x5325, count);
        count += 1;
        put(&mut image, 0x5322, &[0x89, 0x46, 2]);
        put(&mut image, 0x5328, &[0x8b, 0xe5, 0xff, 0x76, 2, 0xbb]);
        put(&mut image, 0x5330, &[0x53]);
        arguments(&mut image, 0x5331, 0x532e, 0x533c, count);
        count += 1;
        put(&mut image, 0x5339, &[0x89, 0x46, 6]);
        put(&mut image, 0x533f, &[0x8b, 0xe5, 0x09, 0x46, 6]);
        cursor = block(&mut image, 0x5344, 2, 6, count, false);
        cursor = block(&mut image, cursor, 4, 6, count + 1, false);
        epilogue(&mut image, cursor, 6);
        put(
            &mut image,
            0x4f8d,
            &[
                0x55, 0x83, 0xec, 2, 0x8b, 0xec, 0x83, 0x3e, 0x66, 0xb6, 0, 0x74, 0x14,
            ],
        );
        put(&mut image, 0x4f9a, &[0x33, 0xc0, 0x50, 0xb8]);
        put(&mut image, 0x4fa0, &[0x50]);
        arguments(&mut image, 0x4fa1, 0x4f9e, 0x4fa9, 19);
        put(&mut image, 0x4fac, &[0x8b, 0xe5, 0x83, 0xc4, 2, 0x5d, 0xc3]);
        image
    }

    fn finish(sequence: &mut IntroSequence) -> Vec<IntroCall> {
        let mut calls = Vec::new();
        while let Some(call) = sequence.current() {
            calls.push(call);
            sequence.complete(0).unwrap();
        }
        calls
    }

    #[test]
    fn decodes_external_operands_and_pregame() {
        let mut image = fixture();
        word(&mut image, 0x504e, 5432);
        word(&mut image, 0x5053, 0x0f58);
        word(&mut image, 0x5057, 0x0f56);
        word(&mut image, 0x4f9e, 71);
        let program = IntroProgram::from_expanded(&image).unwrap();
        assert_eq!(program.pregame().count, 71);
        let mut sequence = IntroSequence::new(&program);
        let calls = finish(&mut sequence);
        assert_eq!(calls[0].request.resource, Resource::DataB);
        assert_eq!(calls[0].request.count, 5432);
        assert_eq!(calls[0].return_ip, 0x505f);
        assert_eq!(calls[1].request.resource, Resource::Data7);
        assert_eq!(calls[1].request.count, 101);
        assert_eq!(calls.last().unwrap().request.count, 128);
        assert!(calls.last().unwrap().request.reverse);
        assert!(
            calls[..calls.len() - 1]
                .iter()
                .all(|call| !call.request.reverse)
        );
        assert_eq!(sequence.result(), Some(0));
        assert!(sequence.complete(0).is_err());
    }

    #[test]
    fn first_key_waits_for_group_then_always_runs_tail() {
        let program = IntroProgram::from_expanded(&fixture()).unwrap();
        let mut sequence = IntroSequence::new(&program);
        sequence.complete(0x39).unwrap();
        assert_eq!(sequence.current().unwrap().request.count, 101);
        sequence.complete(0).unwrap();
        assert_eq!(sequence.current().unwrap().request.count, 125);
        sequence.complete(0x80).unwrap();
        assert_eq!(finish(&mut sequence).len(), 3);
        assert_eq!(sequence.result(), Some(0xb9));
    }

    #[test]
    fn filters_first_break_only_and_uses_signed_comparison() {
        let program = IntroProgram::from_expanded(&fixture()).unwrap();
        let mut sequence = IntroSequence::new(&program);
        sequence.complete(0x80).unwrap();
        sequence.complete(0).unwrap();
        assert_eq!(sequence.current().unwrap().request.count, 102);
        sequence.complete(0x81).unwrap();
        assert_eq!(sequence.current().unwrap().request.count, 103);
        sequence.complete(0).unwrap();
        assert_eq!(sequence.current().unwrap().request.count, 125);
        finish(&mut sequence);
        assert_eq!(sequence.result(), Some(0x81));

        let mut sequence = IntroSequence::new(&program);
        sequence.complete(0x8000).unwrap();
        sequence.complete(0).unwrap();
        assert_eq!(sequence.current().unwrap().request.count, 125);
        finish(&mut sequence);
        assert_eq!(sequence.result(), Some(0x8000));
    }

    #[test]
    fn four_call_group_defers_nonzero_return() {
        let program = IntroProgram::from_expanded(&fixture()).unwrap();
        let mut sequence = IntroSequence::new(&program);
        for _ in 0..4 {
            sequence.complete(0).unwrap();
        }
        sequence.complete(1).unwrap();
        for expected_count in 105..108 {
            assert_eq!(sequence.current().unwrap().request.count, expected_count);
            sequence.complete(0).unwrap();
        }
        assert_eq!(sequence.current().unwrap().request.count, 125);
        finish(&mut sequence);
        assert_eq!(sequence.result(), Some(1));
    }

    #[test]
    fn rejects_changed_control_flow_pointer_pairs_and_truncation() {
        for offset in [0x5032, 0x5067, 0x5086, 0x5108, 0x5314, 0x535d, 0x5053] {
            let mut image = fixture();
            image[offset] ^= 1;
            assert!(
                IntroProgram::from_expanded(&image).is_err(),
                "accepted mutation at {offset:04x}"
            );
        }
        assert!(IntroProgram::from_expanded(&fixture()[..0x5379]).is_err());
    }
}
