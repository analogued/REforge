// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native services around the wordcode VM, with explicit input/parser boundaries.

use crate::interpreter::{Interpreter, MINDWHEEL_DOS, NativeService};
use crate::{
    Error, ProgramAddr, VarAddr,
    bookmark::Bookmark,
    decode::Instruction,
    image::StoryImage,
    input::{InputAction, InputEditor, Key},
    parser::Parser,
    random::{DosRandom, RandomState},
    services::{
        DiskSelection, HostReply, HostRequest, ServiceMessages, SideMap, TranscriptBuffer,
        TranscriptEvent,
    },
    text::{TextCodec, TextCursor, TextOutput, TextScreen},
    timing::{DosReadTiming, ReadPoll},
    vm::{Request, Step, Vm},
};
use std::collections::VecDeque;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionStep {
    Executed {
        instruction: Instruction,
        argument: u16,
    },
    Progress,
    WaitKey,
    ReadLine {
        timeout: u16,
    },
    ParsedLine {
        line: Vec<u8>,
        unknown: bool,
    },
    TimedOut {
        value: u16,
    },
    Unsupported(Request),
    Host(HostRequest),
    Exited {
        code: u8,
    },
}

pub struct Session {
    image: StoryImage,
    vm: Vm,
    codec: TextCodec,
    text: TextOutput,
    input: InputEditor,
    random: DosRandom,
    pending: Option<Request>,
    active_text: Option<TextCursor>,
    input_timeout: Option<u16>,
    bells: usize,
    parser: Parser,
    read_return_value: u16,
    host_requests: VecDeque<HostRequest>,
    transcript: TranscriptBuffer,
    service_messages: ServiceMessages,
    side_map: Option<SideMap>,
    physical_disk: u16,
    disk_transition: Option<(u16, u16)>,
    exited: Option<u8>,
    bios_ticks: u32,
    read_timing: Option<DosReadTiming>,
    vm_poll_pending: bool,
    text_poll_serviced: bool,
}

impl Session {
    pub fn new(image: StoryImage, codec: TextCodec, random: DosRandom) -> Result<Self, Error> {
        Self::new_with_keyboard(image, codec, random, &mut || Ok(Vec::new()))
    }

    /// Native nonblocking keyboard polls also occur while printing the title.
    /// The callback supplies only currently available keys, stopping at CR.
    pub fn new_with_keyboard(
        image: StoryImage,
        codec: TextCodec,
        random: DosRandom,
        keyboard: &mut dyn FnMut() -> Result<Vec<Key>, Error>,
    ) -> Result<Self, Error> {
        Self::with_interpreter_and_keyboard(image, codec, random, &MINDWHEEL_DOS, keyboard)
    }

    pub fn with_interpreter(
        image: StoryImage,
        codec: TextCodec,
        random: DosRandom,
        interpreter: &'static Interpreter,
    ) -> Result<Self, Error> {
        Self::with_interpreter_and_keyboard(image, codec, random, interpreter, &mut || {
            Ok(Vec::new())
        })
    }

    pub fn with_interpreter_and_keyboard(
        image: StoryImage,
        codec: TextCodec,
        random: DosRandom,
        interpreter: &'static Interpreter,
        keyboard: &mut dyn FnMut() -> Result<Vec<Key>, Error>,
    ) -> Result<Self, Error> {
        let vm = Vm::with_interpreter(image.clone(), interpreter)?;
        let mut text = TextOutput::new();
        text.enable_keyboard_polling();
        if interpreter.text == crate::interpreter::TextExecution::WordcodeCallbacks {
            text.enable_callbacks();
        }
        text.set_pagination(vm.variable(VarAddr(0xdc))?, vm.variable(VarAddr(0xde))?);
        text.message(
            codec.message(&image, vm.variable(VarAddr(0x112))?)?,
            codec.pause_prompt(),
        )?;
        let parser = Parser::from_classes(*codec.input_classes());
        let mut session = Self {
            image,
            vm,
            codec,
            text,
            input: InputEditor::new(),
            random,
            pending: None,
            active_text: None,
            input_timeout: None,
            bells: 0,
            parser,
            read_return_value: 0,
            host_requests: VecDeque::new(),
            transcript: TranscriptBuffer::new(),
            service_messages: ServiceMessages::default(),
            side_map: None,
            physical_disk: 1,
            disk_transition: None,
            exited: None,
            bios_ticks: 0,
            read_timing: None,
            vm_poll_pending: false,
            text_poll_serviced: false,
        };
        session.drain_text(Some(keyboard))?;
        session.sync_pagination()?;
        Ok(session)
    }

    pub fn vm(&self) -> &Vm {
        &self.vm
    }
    pub fn screen(&self) -> &TextScreen {
        self.text.screen()
    }
    pub fn input(&self) -> &InputEditor {
        &self.input
    }
    pub fn random_state(&self) -> RandomState {
        self.random.state()
    }
    pub fn parser(&self) -> &Parser {
        &self.parser
    }
    pub fn set_service_messages(&mut self, messages: ServiceMessages) {
        self.service_messages = messages;
    }
    pub fn set_side_map(&mut self, side_map: SideMap) {
        self.side_map = Some(side_map);
    }
    pub fn physical_disk(&self) -> u16 {
        self.physical_disk
    }
    /// The host supplies the original BIOS clock domain, independently of text
    /// rendering and idle RNG polls. Clock changes do not discard typed input.
    pub fn set_bios_ticks(&mut self, ticks: u32) {
        self.bios_ticks = ticks;
    }
    pub fn set_native_read_counter_word(&mut self, word: u16) {
        self.read_return_value = self.vm.interpreter().submitted_read.value(word);
    }

    /// Complete exactly the outstanding platform request. Cancellation returns
    /// from native file prompts without inventing a parser/accumulator result.
    pub fn complete_host(&mut self, reply: HostReply) -> Result<(), Error> {
        let request = self
            .host_requests
            .front()
            .cloned()
            .ok_or_else(|| Error("no host request pending".into()))?;
        match (&request, reply) {
            (HostRequest::WriteTranscript { .. }, HostReply::Done) => {
                self.host_requests.pop_front();
            }
            (HostRequest::SaveBookmark { .. }, HostReply::Done | HostReply::Cancelled) => {
                self.host_requests.pop_front();
                self.rebuild_frame();
                self.finish_native()?;
            }
            (HostRequest::LoadBookmark, HostReply::Cancelled) => {
                self.host_requests.pop_front();
                self.rebuild_frame();
                self.finish_native()?;
            }
            (HostRequest::LoadBookmark, HostReply::Bytes(bytes)) => {
                let bookmark = Bookmark::decode(&bytes, self.vm.state().variables.len())?;
                let maximum =
                    crate::read_word(self.image.initial_variables(), 0x102, "maximum object")?;
                let previous = self.vm.variable(VarAddr(0x1e))?;
                let loaded_slot =
                    crate::read_word(&bookmark.variables, 0x1e, "bookmark logical disk")?;
                if previous >= 10 || loaded_slot >= 10 {
                    return Err(Error("bookmark logical disk outside side map".into()));
                }
                // Validate history before replacing any live VM state.
                if bookmark
                    .history
                    .iter()
                    .any(|&v| v != 0xffff && (v > maximum || v > 0x7fff))
                {
                    return Err(Error("bookmark contains invalid grammar history".into()));
                }
                self.vm.restore_bookmark_parts(
                    &bookmark.variables,
                    bookmark.returns()?,
                    bookmark.values()?,
                    &bookmark.return_slots,
                    &bookmark.value_slots,
                )?;
                self.parser.restore_history(
                    bookmark.history,
                    usize::from(bookmark.history_head),
                    maximum,
                )?;
                self.host_requests.pop_front();
                self.physical_disk = bookmark.selected_disk;
                let target = self.vm.variable(VarAddr(0x1e))?;
                self.disk_transition = Some((previous, target));
                self.continue_disk(None)?;
            }
            (HostRequest::TranscriptName { max_bytes, .. }, HostReply::Name(path)) => {
                if path.len() > *max_bytes || path.contains(&0) {
                    return Err(Error("invalid transcript device name".into()));
                }
                self.host_requests.pop_front();
                self.host_requests
                    .push_front(HostRequest::OpenTranscript { path });
            }
            (
                HostRequest::TranscriptName { .. } | HostRequest::OpenTranscript { .. },
                HostReply::Cancelled,
            ) => {
                self.host_requests.pop_front();
                self.transcript.set_enabled(false);
                self.rebuild_frame();
                self.finish_native()?;
            }
            (HostRequest::OpenTranscript { .. }, HostReply::Done) => {
                self.host_requests.pop_front();
                self.transcript.set_enabled(true);
                self.rebuild_frame();
                self.finish_native()?;
            }
            (HostRequest::CloseTranscript, HostReply::Done) => {
                self.host_requests.pop_front();
                self.transcript.set_enabled(false);
                if matches!(
                    self.pending,
                    Some(Request::NativeService {
                        service: NativeService::CloseTranscript
                    })
                ) {
                    self.finish_native()?;
                }
            }
            (HostRequest::ReadSideMap, HostReply::SideMap(side_map)) => {
                self.host_requests.pop_front();
                self.side_map = Some(side_map);
                self.continue_disk(None)?;
            }
            (HostRequest::MountDisk { .. }, HostReply::Mounted { stores, side_map }) => {
                self.host_requests.pop_front();
                self.side_map = Some(side_map);
                self.continue_disk(Some(stores))?;
            }
            (HostRequest::Exit { code }, HostReply::Done) => {
                self.host_requests.pop_front();
                self.exited = Some(*code);
            }
            _ => {
                return Err(Error(
                    "host response does not match pending operation".into(),
                ));
            }
        }
        Ok(())
    }

    /// The read-time return value is separate from parsing. In the collected
    /// conversion its native local counter is uninitialized. A timing adapter
    /// may provide the observed value; the diagnostic host defaults to zero.
    pub fn set_read_return_value(&mut self, value: u16) {
        self.read_return_value = value;
    }
    pub fn take_bells(&mut self) -> usize {
        std::mem::take(&mut self.bells) + self.text.take_bells()
    }

    /// The host supplies idle polls explicitly. No wall-clock pacing is invented
    /// inside the engine; the eventual scheduler must match the target release.
    pub fn idle_poll(&mut self) {
        if !self.input.paused() {
            self.random.idle_step();
        }
    }

    /// Process one native keyboard-poll batch. The read loop advances RNG once
    /// after draining the editor queue, including the poll that submits CR.
    pub fn key_batch(&mut self, keys: &[Key]) -> Result<(), Error> {
        let reading = matches!(
            self.pending,
            Some(Request::NativeService {
                service: NativeService::ReadParse
            })
        ) && !self.input.line_ready()
            && !self.text.waiting_for_key();
        for &key in keys {
            self.key(key)?;
            if self.input.line_ready() {
                break;
            }
        }
        if reading && !self.input.paused() {
            self.random.idle_step();
        }
        Ok(())
    }

    pub fn key(&mut self, key: Key) -> Result<(), Error> {
        if key == Key::NoKey {
            return Ok(());
        }
        if self.text.waiting_for_key() {
            self.text.resume_key(self.codec.pause_prompt())?;
            self.sync_pagination()?;
            // DOS wait_key forwards other keys into the input pane. CR/LF only
            // dismiss the pause and must not submit an empty command.
            if matches!(key, Key::Enter | Key::LineFeed) {
                return Ok(());
            }
        }
        for action in self.input.process_key(key) {
            if action == InputAction::ClearSubmitFlag {
                self.vm.write_variable(VarAddr(0xda), 0)?;
            }
            if action == InputAction::Bell {
                self.bells += 1;
            }
            self.text.apply_input(&action, self.codec.pause_prompt())?;
        }
        Ok(())
    }

    pub fn advance(&mut self) -> Result<SessionStep, Error> {
        self.advance_inner(None)
    }

    /// Execute with the original keyboard-service opportunities. A read poll
    /// checks submitted input and the current BIOS deadline before invoking the
    /// nonblocking source. Operand/output polls do not advance the RNG.
    pub fn advance_with_keyboard(
        &mut self,
        keyboard: &mut dyn FnMut() -> Result<Vec<Key>, Error>,
    ) -> Result<SessionStep, Error> {
        self.advance_inner(Some(keyboard))
    }

    fn advance_inner(
        &mut self,
        mut keyboard: Option<&mut dyn FnMut() -> Result<Vec<Key>, Error>>,
    ) -> Result<SessionStep, Error> {
        if let Some(code) = self.exited {
            return Ok(SessionStep::Exited { code });
        }
        if let Some(request) = self.host_requests.front() {
            return Ok(SessionStep::Host(request.clone()));
        }
        if self.text.waiting_for_key() || self.input.paused() {
            return Ok(SessionStep::WaitKey);
        }
        self.drain_text(keyboard.as_mut().map(|source| &mut **source as _))?;
        if self.text.waiting_for_key() || self.input.paused() {
            return Ok(SessionStep::WaitKey);
        }
        for value in self.text.take_value_pushes().collect::<Vec<_>>() {
            self.vm.push_text_value(value)?;
        }
        if let Some(letter) = self.text.take_callback() {
            let cursor = self
                .active_text
                .take()
                .ok_or_else(|| Error("callback outside resumable text".into()))?;
            self.vm
                .begin_text_callback(cursor.argument, cursor.position, letter)?;
            self.pending = None;
            return Ok(SessionStep::Progress);
        }
        if self.text.idle() && self.active_text.is_some() {
            let mut cursor = self.active_text.take().unwrap();
            if let Some(word) = self.codec.next_word(&self.image, &mut cursor)? {
                self.active_text = Some(cursor);
                self.print(vec![word])?;
                return Ok(SessionStep::Progress);
            }
        }
        if self.vm_poll_pending {
            self.vm.resume(None)?;
            self.vm_poll_pending = false;
            return Ok(SessionStep::Progress);
        }
        if let Some(request) = self.pending.clone() {
            return match request {
                Request::Message { .. }
                | Request::ResumeText { .. }
                | Request::Character { .. }
                | Request::Decimal { .. }
                | Request::ObjectWords { .. }
                | Request::NativeService {
                    service: NativeService::WaitKey,
                } => {
                    self.sync_pagination()?;
                    self.vm.resume(None)?;
                    self.pending = None;
                    Ok(SessionStep::Progress)
                }
                Request::NativeService {
                    service: NativeService::ReadParse,
                } => {
                    let timeout = self.input_timeout.expect("line service preserves timeout");
                    let timing = self.read_timing.expect("read service preserves deadline");
                    let (line, submitted, value) =
                        match timing.poll(self.bios_ticks, self.input.line_ready()) {
                            ReadPoll::Wait => {
                                if let Some(source) = &mut keyboard {
                                    let keys = source()?;
                                    self.key_batch(&keys)?;
                                }
                                return Ok(SessionStep::ReadLine { timeout });
                            }
                            ReadPoll::Submitted { .. } => {
                                self.vm
                                    .interpreter()
                                    .submitted_read
                                    .representative_word(self.read_return_value)
                                    .ok_or_else(|| {
                                        Error("submission result exceeds native range".into())
                                    })?;
                                (
                                    self.input.submitted_line().unwrap().to_vec(),
                                    true,
                                    self.read_return_value,
                                )
                            }
                            ReadPoll::TimedOut { value } => (Vec::new(), false, value),
                        };
                    self.vm.write_variable(VarAddr(0xce), value)?;
                    let parsed =
                        self.parser
                            .parse_line(&self.image, &self.codec, &mut self.vm, &line)?;
                    if parsed.unknown {
                        let handler = crate::read_word(
                            self.image.initial_variables(),
                            0xea,
                            "unknown-word handler",
                        )?;
                        self.vm.call_from_parser(ProgramAddr(handler))?;
                    }
                    if submitted {
                        self.input.acknowledge_line()?;
                    }
                    self.vm.resume(None)?;
                    self.pending = None;
                    self.input_timeout = None;
                    self.read_timing = None;
                    Ok(if submitted {
                        SessionStep::ParsedLine {
                            line,
                            unknown: parsed.unknown,
                        }
                    } else {
                        SessionStep::TimedOut { value }
                    })
                }
                Request::NativeService {
                    service: NativeService::EchoUnknown | NativeService::EchoInput,
                } => {
                    self.text.suppress_case();
                    self.vm.resume(None)?;
                    self.pending = None;
                    Ok(SessionStep::Progress)
                }
                request => Ok(SessionStep::Unsupported(request)),
            };
        }
        self.sync_screen_region()?;
        match self.vm.step()? {
            Step::Executed {
                instruction,
                argument,
            } => Ok(SessionStep::Executed {
                instruction,
                argument,
            }),
            Step::Suspended(Request::PollInput) => {
                self.vm_poll_pending = true;
                if let Some(source) = &mut keyboard {
                    for key in source()? {
                        self.key(key)?;
                        if self.input.line_ready() {
                            break;
                        }
                    }
                }
                if !self.input.paused() {
                    self.vm.resume(None)?;
                    self.vm_poll_pending = false;
                }
                Ok(SessionStep::Progress)
            }
            Step::Suspended(Request::Random { maximum }) => {
                let result = self.random.bounded(maximum)?;
                self.vm.resume(Some(result))?;
                Ok(SessionStep::Progress)
            }
            Step::Suspended(request) => {
                self.pending = Some(request.clone());
                match request {
                    Request::Message { id } => {
                        if self.vm.interpreter().text
                            == crate::interpreter::TextExecution::WordcodeCallbacks
                        {
                            self.active_text =
                                Some(self.codec.cursor(&self.image, id, None, true)?);
                        } else {
                            self.print(self.codec.message(&self.image, id)?)?;
                        }
                    }
                    Request::ResumeText { argument, position } => {
                        self.active_text = Some(self.codec.cursor(
                            &self.image,
                            argument,
                            Some(position),
                            argument & 0x8000 != 0,
                        )?);
                    }
                    Request::Character { byte } => {
                        self.text.raw_character(byte, self.codec.pause_prompt());
                        self.sync_pagination()?;
                    }
                    Request::Decimal { value } => {
                        self.print(vec![(value as i16).to_string().into_bytes()])?
                    }
                    Request::ObjectWords { object } => {
                        if self.vm.interpreter().text
                            == crate::interpreter::TextExecution::WordcodeCallbacks
                        {
                            self.active_text =
                                Some(self.codec.cursor(&self.image, object.0, None, false)?);
                        } else {
                            self.print(self.codec.object_words(&self.image, object)?)?;
                        }
                    }
                    Request::RememberObject { object } => {
                        let maximum = crate::read_word(
                            self.image.initial_variables(),
                            0x102,
                            "maximum object",
                        )?;
                        self.parser.remember_object(object, maximum)?;
                        self.vm.resume(None)?;
                        self.pending = None;
                    }
                    Request::NativeService {
                        service: NativeService::GetCursor,
                    } => {
                        let (row, column) = self.text.screen().upper_cursor();
                        self.vm.write_variable(VarAddr(0x118), column as u16)?;
                        self.vm.write_variable(VarAddr(0x11a), row as u16)?;
                        self.finish_native()?;
                    }
                    Request::NativeService {
                        service: NativeService::SetCursor,
                    } => {
                        self.text.set_cursor(
                            self.vm.variable(VarAddr(0x11a))?,
                            self.vm.variable(VarAddr(0x118))?,
                        )?;
                        self.finish_native()?;
                    }
                    Request::NativeService {
                        service: NativeService::DrawFrame,
                    } => {
                        let (row, column) = self.input.cursor();
                        self.text.redraw_frame(row, column);
                        self.finish_native()?;
                    }
                    Request::NativeService {
                        service: NativeService::ParsedEnd,
                    } => {
                        if self.parser.last_grammar().is_none() {
                            return Err(Error("parsed-output end requested before parsing".into()));
                        }
                        self.vm.resume(Some(self.parser.output_end().0))?;
                        self.pending = None;
                    }
                    Request::NativeService {
                        service: NativeService::EchoUnknown | NativeService::EchoInput,
                    } => {
                        self.text.set_pagination(
                            self.vm.variable(VarAddr(0xdc))?,
                            self.vm.variable(VarAddr(0xde))?,
                        );
                        let token = if matches!(
                            request,
                            Request::NativeService {
                                service: NativeService::EchoUnknown
                            }
                        ) {
                            self.parser.unknown_word()
                        } else {
                            self.parser.last_input()
                        };
                        self.text.native_token(token, self.codec.pause_prompt())?;
                        self.sync_pagination()?;
                    }
                    Request::NativeService {
                        service: NativeService::WaitKey,
                    } => self.text.wait_key(self.codec.pause_prompt())?,
                    Request::NativeService {
                        service: NativeService::ReadParse,
                    } => {
                        self.input_timeout = Some(self.vm.accumulator());
                        let word = self
                            .vm
                            .interpreter()
                            .submitted_read
                            .representative_word(self.read_return_value)
                            .ok_or_else(|| {
                                Error("submission result exceeds native range".into())
                            })?;
                        self.read_timing = Some(DosReadTiming::with_formula(
                            self.vm.accumulator(),
                            self.bios_ticks,
                            word,
                            self.vm.interpreter().submitted_read,
                        ));
                        self.vm.write_variable(VarAddr(0xce), 0)?;
                        self.vm.write_variable(VarAddr(0xd8), 0)?;
                        self.record_transcript(TranscriptEvent::FlushRemainder)?;
                    }
                    Request::NativeService {
                        service: NativeService::Exit,
                    } => {
                        self.text.clear_for_exit();
                        if self.transcript.enabled() {
                            self.host_requests.push_back(HostRequest::CloseTranscript);
                        }
                        self.host_requests.push_back(HostRequest::Exit { code: 0 });
                    }
                    Request::NativeService {
                        service: NativeService::Save,
                    } => {
                        let mut bookmark = Bookmark::from_vm(
                            self.vm.state(),
                            *self.parser.history(),
                            self.parser.history_head(),
                            self.physical_disk,
                        )?;
                        bookmark.return_slots = *self.vm.return_slots();
                        bookmark.value_slots = *self.vm.value_slots();
                        self.host_requests.push_back(HostRequest::SaveBookmark {
                            data: bookmark.encode()?,
                        });
                    }
                    Request::NativeService {
                        service: NativeService::Restore,
                    } => self.host_requests.push_back(HostRequest::LoadBookmark),
                    Request::NativeService {
                        service: NativeService::OpenTranscript,
                    } => self
                        .host_requests
                        .push_back(self.service_messages.transcript_name_request()),
                    Request::NativeService {
                        service: NativeService::CloseTranscript,
                    } => {
                        if self.transcript.enabled() {
                            self.host_requests.push_back(HostRequest::CloseTranscript);
                        } else {
                            self.finish_native()?;
                        }
                    }
                    Request::NativeService {
                        service: NativeService::ResetVariables | NativeService::SelectDisk,
                    } => {
                        let target = if matches!(
                            request,
                            Request::NativeService {
                                service: NativeService::ResetVariables
                            }
                        ) {
                            0
                        } else {
                            self.vm.accumulator()
                        };
                        if matches!(
                            request,
                            Request::NativeService {
                                service: NativeService::ResetVariables
                            }
                        ) {
                            self.vm.write_variable(VarAddr(0xce), 0)?;
                        }
                        self.begin_disk(target)?;
                    }
                    _ => return Ok(SessionStep::Unsupported(request)),
                }
                self.drain_text(keyboard.as_mut().map(|source| &mut **source as _))?;
                Ok(SessionStep::Progress)
            }
        }
    }

    fn sync_screen_region(&mut self) -> Result<(), Error> {
        if self.vm.interpreter().screen == crate::interpreter::ScreenModel::VariableBacked {
            self.text.set_region(
                self.vm.variable(VarAddr(0x11c))?,
                self.vm.variable(VarAddr(0x11e))?,
                self.vm.variable(VarAddr(0x120))?,
                self.vm.variable(VarAddr(0x122))?,
                self.vm.variable(VarAddr(0x126))?,
            )?;
        }
        Ok(())
    }

    fn drain_text(
        &mut self,
        mut keyboard: Option<&mut dyn FnMut() -> Result<Vec<Key>, Error>>,
    ) -> Result<(), Error> {
        let mut serviced_output = false;
        while self.text.keyboard_poll_pending() && !self.input.paused() {
            serviced_output = true;
            if !self.text_poll_serviced {
                if let Some(source) = &mut keyboard {
                    for key in source()? {
                        self.key(key)?;
                        if self.input.line_ready() {
                            break;
                        }
                    }
                }
                self.text_poll_serviced = true;
            }
            if self.input.paused() {
                break;
            }
            self.text_poll_serviced = false;
            self.text.resume_keyboard_poll(self.codec.pause_prompt());
        }
        // Wordcode can update the pagination variable between output requests.
        // An advance with no output must not replace it with formatter state.
        if serviced_output {
            self.sync_pagination()?;
        }
        Ok(())
    }

    fn print(&mut self, tokens: Vec<Vec<u8>>) -> Result<(), Error> {
        self.text.set_pagination(
            self.vm.variable(VarAddr(0xdc))?,
            self.vm.variable(VarAddr(0xde))?,
        );
        self.text.message(tokens, self.codec.pause_prompt())?;
        self.sync_pagination()
    }

    fn sync_pagination(&mut self) -> Result<(), Error> {
        self.vm
            .write_variable(VarAddr(0xdc), self.text.pagination_remaining())?;
        let events: Vec<_> = self.text.take_transcript_events().collect();
        for event in events {
            self.record_transcript(event)?;
        }
        Ok(())
    }

    fn record_transcript(&mut self, event: TranscriptEvent) -> Result<(), Error> {
        if let Some(bytes) = self.transcript.record(event)? {
            self.host_requests
                .push_back(HostRequest::WriteTranscript { bytes });
        }
        Ok(())
    }
    fn finish_native(&mut self) -> Result<(), Error> {
        self.vm.resume(None)?;
        self.pending = None;
        self.input_timeout = None;
        Ok(())
    }
    fn rebuild_frame(&mut self) {
        self.text.rebuild_frame();
        self.input.rebuild_frame();
    }
    fn begin_disk(&mut self, target: u16) -> Result<(), Error> {
        if target >= 10 {
            return Err(Error("logical disk outside native side map".into()));
        }
        let previous = self.vm.variable(VarAddr(0x1e))?;
        self.vm.write_variable(VarAddr(0x1e), target)?;
        self.disk_transition = Some((previous, target));
        self.continue_disk(None)
    }
    fn continue_disk(&mut self, disk: Option<crate::media::TextStores>) -> Result<(), Error> {
        let (previous, target) = self
            .disk_transition
            .ok_or_else(|| Error("no disk selection pending".into()))?;
        let Some(mapping) = &self.side_map else {
            self.host_requests.push_back(HostRequest::ReadSideMap);
            return Ok(());
        };
        match mapping.selection(previous, target)? {
            DiskSelection::RequestMedia {
                logical_slot,
                physical_disk,
                drive,
            } => {
                if matches!(
                    self.pending,
                    Some(Request::NativeService {
                        service: NativeService::Restore
                    })
                ) {
                    self.vm.write_variable(VarAddr(0xce), target)?;
                }
                self.physical_disk = physical_disk;
                self.host_requests
                    .push_back(self.service_messages.mount_request(
                        logical_slot,
                        physical_disk,
                        drive,
                    ));
            }
            DiskSelection::Ready { drive_changed, .. } => {
                if drive_changed {
                    if matches!(
                        self.pending,
                        Some(Request::NativeService {
                            service: NativeService::Restore
                        })
                    ) {
                        self.vm.write_variable(VarAddr(0xce), target)?;
                    }
                    let Some(disk) = disk else {
                        let entry = mapping.entries[usize::from(target)];
                        self.host_requests
                            .push_back(self.service_messages.mount_request(
                                target,
                                self.physical_disk,
                                entry.drive,
                            ));
                        return Ok(());
                    };
                    self.image.replace_text_stores(&disk)?;
                }
                if matches!(
                    self.pending,
                    Some(Request::NativeService {
                        service: NativeService::ResetVariables
                    })
                ) {
                    self.vm.reset_initial_variables()?;
                }
                if matches!(
                    self.pending,
                    Some(Request::NativeService {
                        service: NativeService::Restore
                    })
                ) {
                    self.rebuild_frame();
                }
                self.disk_transition = None;
                self.finish_native()?;
            }
        }
        Ok(())
    }
}
