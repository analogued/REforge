// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host file selection on a temporary DOS text screen. No story/editor state lives here.

use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::Loaded;
#[cfg(any(feature = "sdl", test))]
use super::cursor::Cursor;
use crate::{
    bookmark::{Bookmark, HEADER_BYTES},
    services::{HostReply, HostRequest},
};
#[cfg(any(feature = "sdl", test))]
use reforge_settings::FileCursorStyle;
#[cfg(any(feature = "sdl", test))]
use std::time::Duration;

pub(crate) fn begin_selection(
    loaded: &Loaded,
    request: &HostRequest,
    title: &str,
) -> Option<FileDialog> {
    let (operation, fixed) = match request {
        HostRequest::SaveBookmark { .. } => {
            (Operation::Save, loaded.files.bookmark_path.as_deref())
        }
        HostRequest::LoadBookmark => (Operation::Load, loaded.files.bookmark_path.as_deref()),
        HostRequest::TranscriptName { .. } => (
            Operation::Transcript,
            loaded.files.transcript_path.as_deref(),
        ),
        _ => return None,
    };
    let extent = loaded.session.vm().state().variables.len();
    let dialog = if let Some(path) = fixed {
        // Fixed file options keep their launch-directory semantics and skip
        // selection, but existing write destinations still need a decision.
        if operation == Operation::Load || !path.exists() {
            return None;
        }
        FileDialog::fixed(operation, path, title, extent)
    } else {
        FileDialog::new(operation, &loaded.files.save_directory, title, extent)
    };
    Some(dialog)
}

/// Errors leave the original host operation outstanding, so the user can
/// correct a filename or choose another bookmark without losing the browser.
pub(crate) fn complete_selection(
    loaded: &mut Loaded,
    request: &HostRequest,
    dialog: &mut FileDialog,
    action: Action,
) -> Result<bool, String> {
    match action {
        Action::Cancel => {
            loaded
                .session
                .complete_host(HostReply::Cancelled)
                .map_err(|e| e.to_string())?;
            Ok(true)
        }
        Action::Choose { path, replace } => {
            match loaded.files.handle_browser_path(request, path, replace) {
                Ok(Some(reply)) => {
                    if let Some(error) = loaded.files.take_error() {
                        dialog.error(error);
                        return Ok(false);
                    }
                    match loaded.session.complete_host(reply) {
                        Ok(()) => Ok(true),
                        Err(error) if matches!(request, HostRequest::LoadBookmark) => {
                            dialog.error(format!("Cannot restore bookmark: {error}"));
                            Ok(false)
                        }
                        Err(error) => Err(error.to_string()),
                    }
                }
                Ok(None) => {
                    dialog.error("The file operation is still waiting for a filename.".into());
                    Ok(false)
                }
                Err(error) => {
                    dialog.error(error.to_string());
                    Ok(false)
                }
            }
        }
    }
}

const ROWS: usize = 6;
const LEFT: usize = 3;
const WIDTH: usize = 74;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Save,
    Load,
    Transcript,
}
impl Operation {
    fn label(self) -> &'static str {
        match self {
            Self::Save => "BOOKMARK",
            Self::Load => "RESUME",
            Self::Transcript => "SCRIPT",
        }
    }
    fn prompt(self) -> &'static str {
        match self {
            Self::Save => "Name this saved novel:",
            Self::Load => "Which version do you want to resume?",
            Self::Transcript => "Name this transcript:",
        }
    }
    fn accept(self) -> &'static str {
        match self {
            Self::Save => "[ Save ]",
            Self::Load => "[ Resume ]",
            Self::Transcript => "[ Write ]",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    List,
    Name,
    Accept,
    Cancel,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Input {
    Up,
    Down,
    PageUp,
    PageDown,
    Left(bool),
    Right(bool),
    Home(bool),
    End(bool),
    Tab(bool),
    Backspace,
    Delete,
    Enter,
    Escape,
    SelectAll,
    Text(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Choose { path: PathBuf, replace: bool },
    Cancel,
}
#[derive(Debug)]
struct Entry {
    path: PathBuf,
    name: String,
    directory: bool,
    modified: String,
}

pub(crate) struct FileDialog {
    operation: Operation,
    title: String,
    directory: PathBuf,
    entries: Vec<Entry>,
    selected: usize,
    top: usize,
    name: Vec<char>,
    caret: usize,
    anchor: Option<usize>,
    // Preserve the actual OS filename even when it cannot be represented by UTF-8.
    selected_path: Option<PathBuf>,
    focus: Focus,
    confirm: Option<PathBuf>,
    yes: bool,
    fixed: bool,
    notice: String,
    expected_bytes: usize,
    #[cfg(any(feature = "sdl", test))]
    cursor_style: FileCursorStyle,
}

impl FileDialog {
    pub(crate) fn new(
        operation: Operation,
        directory: &Path,
        title: &str,
        expected_bytes: usize,
    ) -> Self {
        let mut dialog = Self {
            operation,
            title: title.to_uppercase(),
            directory: std::path::absolute(directory).unwrap_or_else(|_| directory.to_path_buf()),
            entries: Vec::new(),
            selected: 0,
            top: 0,
            name: Vec::new(),
            caret: 0,
            anchor: None,
            selected_path: None,
            focus: Focus::Name,
            confirm: None,
            yes: false,
            fixed: false,
            notice: String::new(),
            expected_bytes,
            #[cfg(any(feature = "sdl", test))]
            cursor_style: FileCursorStyle::PulsingUnderline,
        };
        dialog.scan();
        if operation == Operation::Load
            && let Some(index) = dialog.entries.iter().position(|entry| !entry.directory)
        {
            dialog.select(index);
            dialog.focus = Focus::List;
        }
        dialog
    }

    pub(crate) fn fixed(
        operation: Operation,
        path: &Path,
        title: &str,
        expected_bytes: usize,
    ) -> Self {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let mut dialog = Self::new(
            operation,
            path.parent().unwrap_or(Path::new(".")),
            title,
            expected_bytes,
        );
        dialog.set_name(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
        );
        dialog.confirm = Some(path);
        dialog.fixed = true;
        dialog
    }

    fn scan(&mut self) {
        self.entries.clear();
        if let Some(parent) = self.directory.parent() {
            self.entries.push(Entry {
                path: parent.to_path_buf(),
                name: "..".into(),
                directory: true,
                modified: String::new(),
            });
        }
        match fs::read_dir(&self.directory) {
            Err(error) => self.notice = format!("Cannot list directory: {error}"),
            Ok(entries) => {
                let mut found = Vec::new();
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Ok(metadata) = fs::metadata(&path) else {
                        continue;
                    };
                    if !metadata.is_dir() && !metadata.is_file() {
                        continue;
                    }
                    if !metadata.is_dir()
                        && self.operation != Operation::Transcript
                        && !looks_like_bookmark(&path, metadata.len(), self.expected_bytes)
                    {
                        continue;
                    }
                    found.push(Entry {
                        path,
                        name: entry.file_name().to_string_lossy().into_owned(),
                        directory: metadata.is_dir(),
                        modified: metadata.modified().map(date).unwrap_or_default(),
                    });
                }
                found.sort_by(|a, b| {
                    b.directory
                        .cmp(&a.directory)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                        .then_with(|| a.path.cmp(&b.path))
                });
                self.entries.extend(found);
            }
        }
        self.selected = 0;
        self.top = 0;
    }

    fn set_name(&mut self, name: &str) {
        self.name = name.chars().collect();
        self.caret = self.name.len();
        self.anchor = None;
        self.selected_path = None;
    }
    fn select(&mut self, index: usize) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = index.min(self.entries.len() - 1);
        if !self.entries[self.selected].directory {
            let name = self.entries[self.selected].name.clone();
            self.set_name(&name);
            self.selected_path = Some(self.entries[self.selected].path.clone());
        }
        if self.selected < self.top {
            self.top = self.selected;
        }
        if self.selected >= self.top + ROWS {
            self.top = self.selected + 1 - ROWS;
        }
        self.notice.clear();
    }
    fn navigate(&mut self, path: PathBuf) {
        match path.canonicalize() {
            Ok(directory) if directory.is_dir() => {
                self.directory = directory;
                self.notice.clear();
                self.scan();
                self.set_name("");
                self.focus = Focus::List;
            }
            Ok(_) => self.error("The selected path is not a directory.".into()),
            Err(error) => self.error(format!("Cannot open directory: {error}")),
        }
    }
    fn selection(&self) -> Option<std::ops::Range<usize>> {
        self.anchor
            .filter(|a| *a != self.caret)
            .map(|a| a.min(self.caret)..a.max(self.caret))
    }
    #[cfg(any(feature = "sdl", test))]
    pub(crate) fn selected_text(&self) -> Option<String> {
        (self.focus == Focus::Name)
            .then(|| {
                self.selection()
                    .map(|range| self.name[range].iter().collect())
            })
            .flatten()
    }
    #[cfg(feature = "sdl")]
    pub(crate) fn editing_name(&self) -> bool {
        self.focus == Focus::Name && self.confirm.is_none()
    }
    fn erase_selection(&mut self) -> bool {
        let Some(range) = self.selection() else {
            self.anchor = None;
            return false;
        };
        self.caret = range.start;
        self.name.drain(range);
        self.anchor = None;
        self.selected_path = None;
        true
    }
    fn move_caret(&mut self, position: usize, select: bool) {
        if select {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = position.min(self.name.len());
    }
    pub(crate) fn error(&mut self, message: String) {
        self.notice = message;
        self.confirm = None;
        self.focus = Focus::Name;
    }

    fn submit(&mut self) -> Option<Action> {
        if self.focus == Focus::List && self.entries.get(self.selected).is_some_and(|e| e.directory)
        {
            self.navigate(self.entries[self.selected].path.clone());
            return None;
        }
        if self.name.is_empty() {
            self.error("Enter a filename or path.".into());
            return None;
        }
        let path = self
            .selected_path
            .clone()
            .unwrap_or_else(|| self.directory.join(self.name.iter().collect::<String>()));
        if path.is_dir() {
            self.navigate(path);
            return None;
        }
        if self.operation == Operation::Load {
            let read = || -> Result<(), String> {
                let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
                if !metadata.is_file()
                    || metadata.len() != (HEADER_BYTES + self.expected_bytes) as u64
                {
                    return Err("This is not a bookmark for this game.".into());
                }
                let mut bytes = Vec::new();
                File::open(&path)
                    .and_then(|file| {
                        file.take((HEADER_BYTES + self.expected_bytes + 1) as u64)
                            .read_to_end(&mut bytes)
                    })
                    .map_err(|e| e.to_string())?;
                Bookmark::decode(&bytes, self.expected_bytes)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            };
            match read() {
                Ok(_) => {
                    return Some(Action::Choose {
                        path,
                        replace: false,
                    });
                }
                Err(error) => {
                    self.error(format!("Cannot restore bookmark: {error}"));
                    return None;
                }
            }
        }
        if path.exists() {
            self.confirm = Some(path);
            self.yes = false;
            return None;
        }
        if !path.parent().unwrap_or(Path::new(".")).is_dir() {
            self.error("The destination directory does not exist.".into());
            return None;
        }
        Some(Action::Choose {
            path,
            replace: false,
        })
    }

    pub(crate) fn input(&mut self, input: Input) -> Option<Action> {
        if self.confirm.is_some() {
            match input {
                Input::Tab(_) | Input::Left(_) | Input::Right(_) => self.yes = !self.yes,
                Input::Text(text) if text.eq_ignore_ascii_case("y") => {
                    return Some(Action::Choose {
                        path: self.confirm.take().unwrap(),
                        replace: true,
                    });
                }
                Input::Enter if self.yes => {
                    return Some(Action::Choose {
                        path: self.confirm.take().unwrap(),
                        replace: true,
                    });
                }
                Input::Enter | Input::Escape => return self.decline(),
                Input::Text(text) if text.eq_ignore_ascii_case("n") => return self.decline(),
                _ => {}
            }
            return None;
        }
        match input {
            Input::Escape => return Some(Action::Cancel),
            Input::Tab(reverse) => {
                let order = [Focus::List, Focus::Name, Focus::Accept, Focus::Cancel];
                let index = order.iter().position(|focus| *focus == self.focus).unwrap();
                self.focus = order[(index + if reverse { 3 } else { 1 }) % 4];
            }
            Input::Enter => {
                return if self.focus == Focus::Cancel {
                    Some(Action::Cancel)
                } else {
                    self.submit()
                };
            }
            Input::Text(text) if self.focus == Focus::Name => {
                self.erase_selection();
                self.selected_path = None;
                let chars: Vec<_> = text.chars().filter(|c| !c.is_control()).collect();
                let count = chars.len();
                self.name.splice(self.caret..self.caret, chars);
                self.caret += count;
                self.notice.clear();
            }
            Input::Text(text)
                if text == " " && matches!(self.focus, Focus::Accept | Focus::Cancel) =>
            {
                return if self.focus == Focus::Cancel {
                    Some(Action::Cancel)
                } else {
                    self.submit()
                };
            }
            Input::Up | Input::Down | Input::PageUp | Input::PageDown
                if self.focus == Focus::List =>
            {
                let index = match input {
                    Input::Up => self.selected.saturating_sub(1),
                    Input::Down => self.selected + 1,
                    Input::PageUp => self.selected.saturating_sub(ROWS),
                    _ => self.selected + ROWS,
                };
                self.select(index);
            }
            Input::Home(_) if self.focus == Focus::List => self.select(0),
            Input::End(_) if self.focus == Focus::List => {
                self.select(self.entries.len().saturating_sub(1))
            }
            Input::Left(_) | Input::Right(_)
                if matches!(self.focus, Focus::Accept | Focus::Cancel) =>
            {
                self.focus = if self.focus == Focus::Accept {
                    Focus::Cancel
                } else {
                    Focus::Accept
                }
            }
            Input::Left(select) if self.focus == Focus::Name => {
                let position = if select {
                    self.caret.saturating_sub(1)
                } else {
                    self.selection()
                        .map_or(self.caret.saturating_sub(1), |range| range.start)
                };
                self.move_caret(position, select)
            }
            Input::Right(select) if self.focus == Focus::Name => {
                let position = if select {
                    self.caret + 1
                } else {
                    self.selection().map_or(self.caret + 1, |range| range.end)
                };
                self.move_caret(position, select)
            }
            Input::Home(select) if self.focus == Focus::Name => self.move_caret(0, select),
            Input::End(select) if self.focus == Focus::Name => {
                self.move_caret(self.name.len(), select)
            }
            Input::SelectAll if self.focus == Focus::Name => {
                self.anchor = Some(0);
                self.caret = self.name.len();
            }
            Input::Backspace if self.focus == Focus::List => {
                if let Some(parent) = self.directory.parent() {
                    self.navigate(parent.to_path_buf());
                }
            }
            Input::Backspace if self.focus == Focus::Name => {
                if !self.erase_selection() && self.caret > 0 {
                    self.caret -= 1;
                    self.name.remove(self.caret);
                    self.selected_path = None;
                }
            }
            Input::Delete
                if self.focus == Focus::Name
                    && !self.erase_selection()
                    && self.caret < self.name.len() =>
            {
                self.name.remove(self.caret);
                self.selected_path = None;
            }
            _ => {}
        }
        None
    }
    fn decline(&mut self) -> Option<Action> {
        self.confirm = None;
        self.yes = false;
        if self.fixed {
            Some(Action::Cancel)
        } else {
            self.focus = Focus::Name;
            None
        }
    }

    #[cfg(any(feature = "sdl", test))]
    pub(crate) fn click(&mut self, column: usize, row: usize, double: bool) -> Option<Action> {
        if self.confirm.is_some() {
            if row == 20 {
                if (LEFT..LEFT + 16).contains(&column) {
                    self.yes = true;
                    return self.input(Input::Enter);
                }
                if (LEFT + 19..LEFT + WIDTH).contains(&column) {
                    return self.decline();
                }
            }
        } else if (9..9 + ROWS).contains(&row) && (LEFT..LEFT + WIDTH).contains(&column) {
            let index = self.top + row - 9;
            if index < self.entries.len() {
                self.focus = Focus::List;
                self.select(index);
                if double {
                    return self.submit();
                }
            }
        } else if row == 18 && (LEFT..LEFT + WIDTH).contains(&column) {
            self.focus = Focus::Name;
            self.anchor = None;
            self.caret = (self.visible_start() + column - LEFT).min(self.name.len());
        } else if row == 20 {
            let accept = self.operation.accept().len();
            if (LEFT..LEFT + accept).contains(&column) {
                self.focus = Focus::Accept;
                return self.submit();
            }
            if (LEFT + accept + 3..LEFT + accept + 13).contains(&column) {
                return Some(Action::Cancel);
            }
        }
        None
    }
    #[cfg(feature = "sdl")]
    pub(crate) fn scroll(&mut self, lines: i32) {
        if self.confirm.is_none() {
            self.focus = Focus::List;
            self.select(self.selected.saturating_add_signed(lines as isize));
        }
    }
    fn visible_start(&self) -> usize {
        self.caret.saturating_sub(WIDTH - 2)
    }
    #[cfg(any(feature = "sdl", test))]
    pub(crate) fn set_cursor_style(&mut self, style: FileCursorStyle) {
        self.cursor_style = style;
    }
    pub(crate) fn cursor_position(&self) -> Option<(usize, usize)> {
        (self.confirm.is_none() && self.focus == Focus::Name)
            .then(|| (18, LEFT + self.caret - self.visible_start()))
    }
    #[cfg(any(feature = "sdl", test))]
    pub(crate) fn cursor(&self, elapsed: Duration) -> Option<Cursor> {
        let (row, column) = self.cursor_position()?;
        Cursor::at(row, column, self.cursor_style, elapsed)
    }
    pub(crate) fn cells(&self) -> Vec<u8> {
        let mut screen = Screen([b' ', 7].repeat(80 * 25));
        screen.border();
        screen.text(LEFT, 1, &shorten(&self.title, 52), false, 0);
        let label = self.operation.label();
        screen.text(77 - label.len(), 1, label, false, 0);
        screen.text(LEFT, 3, self.operation.prompt(), false, 0);
        screen.text(LEFT, 5, "Directory:", false, 0);
        screen.text(
            LEFT,
            6,
            &shorten(&self.directory.to_string_lossy(), WIDTH),
            false,
            0,
        );
        screen.text(LEFT + 2, 8, "Name", false, 0);
        screen.text(51, 8, "Modified", false, 0);
        for (offset, entry) in self.entries.iter().skip(self.top).take(ROWS).enumerate() {
            let selected = self.selected == self.top + offset
                && self.focus == Focus::List
                && self.confirm.is_none();
            let prefix = if selected { "> " } else { "  " };
            let label = if entry.directory {
                format!("{prefix}[{}]", entry.name)
            } else {
                format!("{prefix}{}", entry.name)
            };
            screen.text(LEFT, 9 + offset, &shorten(&label, 45), selected, WIDTH);
            screen.text(
                51,
                9 + offset,
                if entry.directory {
                    "<DIR>"
                } else {
                    &entry.modified
                },
                selected,
                0,
            );
        }
        if self.entries.len() > ROWS {
            screen.text(
                63,
                7,
                &format!(
                    "{}-{}/{}",
                    self.top + 1,
                    (self.top + ROWS).min(self.entries.len()),
                    self.entries.len()
                ),
                false,
                0,
            );
        }
        if let Some(path) = &self.confirm {
            screen.text(
                LEFT,
                16,
                &shorten(
                    &format!(
                        "{} already exists.",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    WIDTH,
                ),
                false,
                0,
            );
            screen.text(LEFT, 18, "Replace existing version? [y/n]", false, 0);
            screen.text(LEFT, 20, "[ Yes, replace ]", self.yes, 0);
            screen.text(
                LEFT + 19,
                20,
                if self.fixed {
                    "[ No, cancel ]"
                } else {
                    "[ No, choose another ]"
                },
                !self.yes,
                0,
            );
            screen.text(
                LEFT,
                22,
                "Left/Right Select   Enter Confirm   Esc Back",
                false,
                0,
            );
        } else {
            for (row, line) in self
                .notice
                .chars()
                .collect::<Vec<_>>()
                .chunks(WIDTH)
                .take(2)
                .enumerate()
            {
                screen.text(LEFT, 15 + row, &line.iter().collect::<String>(), false, 0);
            }
            screen.text(LEFT, 17, "Filename:", false, 0);
            let start = self.visible_start();
            let selection = self.selection();
            for (column, &character) in self.name.iter().skip(start).take(WIDTH).enumerate() {
                screen.text(
                    LEFT + column,
                    18,
                    &character.to_string(),
                    self.focus == Focus::Name
                        && selection
                            .as_ref()
                            .is_some_and(|range| range.contains(&(start + column))),
                    0,
                );
            }
            let action = self.operation.accept();
            screen.text(LEFT, 20, action, self.focus == Focus::Accept, 0);
            screen.text(
                LEFT + action.len() + 3,
                20,
                "[ Cancel ]",
                self.focus == Focus::Cancel,
                0,
            );
            screen.text(
                LEFT,
                22,
                "Up/Down Select   Enter Activate   Tab/Shift+Tab Focus   Esc Cancel",
                false,
                0,
            );
            screen.text(LEFT, 23, "PgUp/PgDn Page   Backspace Parent", false, 0);
        }
        screen.0
    }
}

fn looks_like_bookmark(path: &Path, bytes: u64, expected: usize) -> bool {
    if bytes != (HEADER_BYTES + expected) as u64 {
        return false;
    }
    let mut header = [0; 2];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && usize::from(u16::from_le_bytes(header)) == expected
}
fn shorten(text: &str, width: usize) -> String {
    let chars: Vec<_> = text.chars().collect();
    if chars.len() <= width {
        return text.into();
    }
    let left = (width - 3) / 2;
    chars[..left]
        .iter()
        .chain(['.', '.', '.'].iter())
        .chain(chars[chars.len() - (width - 3 - left)..].iter())
        .collect()
}
struct Screen(Vec<u8>);
impl Screen {
    fn text(&mut self, column: usize, row: usize, text: &str, inverse: bool, width: usize) {
        let attr = if inverse { 0x70 } else { 7 };
        for col in column..(column + width).min(78) {
            self.0[(row * 80 + col) * 2 + 1] = attr;
        }
        for (col, c) in text.chars().take(78 - column).enumerate() {
            let index = (row * 80 + column + col) * 2;
            self.0[index] = if c.is_ascii() && !c.is_control() {
                c as u8
            } else {
                b'?'
            };
            self.0[index + 1] = attr;
        }
    }
    fn border(&mut self) {
        for col in 2..78 {
            self.0[col * 2] = 0xcd;
            self.0[(24 * 80 + col) * 2] = 0xcd;
        }
        for row in 1..24 {
            self.0[(row * 80 + 1) * 2] = 0xba;
            self.0[(row * 80 + 78) * 2] = 0xba;
        }
        for (row, col, c) in [(0, 1, 0xc9), (0, 78, 0xbb), (24, 1, 0xc8), (24, 78, 0xbc)] {
            self.0[(row * 80 + col) * 2] = c;
        }
    }
}
fn date(time: SystemTime) -> String {
    super::terminal::format_local_timestamp(time).unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::UNIX_EPOCH;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    pub(crate) struct Directory(pub PathBuf);
    impl Directory {
        pub(crate) fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "btz-dialog-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        pub(crate) fn bookmark(&self, name: &str, extent: usize) {
            let book = Bookmark {
                value_slots: [0; 128],
                value_depth: 0,
                return_slots: [0; 128],
                return_depth: 0,
                history_head: 0,
                history: [0xffff; 100],
                selected_disk: 0,
                variables: vec![0; extent],
            };
            fs::write(self.0.join(name), book.encode().unwrap()).unwrap();
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn name(dialog: &FileDialog) -> String {
        dialog.name.iter().collect()
    }
    fn type_name(dialog: &mut FileDialog, text: &str) {
        dialog.focus = Focus::Name;
        dialog.input(Input::SelectAll);
        dialog.input(Input::Text(text.into()));
    }

    #[test]
    fn listing_recognizes_extensionless_saves_pages_and_keeps_focus_groups() {
        let dir = Directory::new();
        fs::create_dir(dir.0.join("archive")).unwrap();
        for i in 0..12 {
            dir.bookmark(&format!("SAVE-{i:02}"), 512);
        }
        dir.bookmark("OTHER.SAV", 514);
        fs::write(dir.0.join("GO.EXE"), b"original").unwrap();
        let mut dialog = FileDialog::new(Operation::Load, &dir.0, "Mindwheel", 512);
        assert_eq!(dialog.entries.len(), 14);
        assert_eq!(dialog.focus, Focus::List);
        assert_eq!(name(&dialog), "SAVE-00");
        for i in 1..12 {
            dialog.input(Input::Down);
            assert_eq!(name(&dialog), format!("SAVE-{i:02}"));
        }
        assert!(dialog.top > 0);
        dialog.input(Input::PageUp);
        assert_eq!(name(&dialog), "SAVE-05");
        for focus in [Focus::Name, Focus::Accept, Focus::Cancel, Focus::List] {
            dialog.input(Input::Tab(false));
            assert_eq!(dialog.focus, focus);
        }
        dialog.input(Input::Tab(true));
        assert_eq!(dialog.focus, Focus::Cancel);
        assert_eq!(dialog.input(Input::Enter), Some(Action::Cancel));
        dialog.input(Input::Tab(true));
        assert_eq!(dialog.focus, Focus::Accept);
        assert!(matches!(
            dialog.input(Input::Enter),
            Some(Action::Choose { replace: false, .. })
        ));
    }

    #[test]
    fn relative_parent_absolute_paths_and_folder_navigation_are_real() {
        let dir = Directory::new();
        let archive = dir.0.join("ARCHIVE");
        fs::create_dir(&archive).unwrap();
        dir.bookmark("BEFORE-DOOR", 512);
        let mut dialog = FileDialog::new(Operation::Load, &archive, "Mindwheel", 512);
        type_name(&mut dialog, "../BEFORE-DOOR");
        assert_eq!(
            dialog.input(Input::Enter),
            Some(Action::Choose {
                path: archive.join("../BEFORE-DOOR"),
                replace: false
            })
        );
        let path = dir.0.join("BEFORE-DOOR");
        type_name(&mut dialog, &path.to_string_lossy());
        assert_eq!(
            dialog.input(Input::Enter),
            Some(Action::Choose {
                path,
                replace: false
            })
        );
        type_name(&mut dialog, "..");
        dialog.input(Input::Enter);
        assert_eq!(dialog.directory, dir.0.canonicalize().unwrap());
        let index = dialog
            .entries
            .iter()
            .position(|entry| entry.name == "ARCHIVE")
            .unwrap();
        dialog.click(8, 9 + index, false);
        assert_eq!(dialog.focus, Focus::List);
        dialog.input(Input::Enter);
        assert_eq!(dialog.directory, archive.canonicalize().unwrap());
        dialog.input(Input::Backspace);
        assert_eq!(dialog.directory, dir.0.canonicalize().unwrap());
    }

    #[test]
    fn replacement_requires_explicit_yes_no_preserves_filename_and_fixed_no_cancels() {
        let dir = Directory::new();
        dir.bookmark("EXISTING", 512);
        for title in ["Mindwheel", "Essex", "Brimstone", "Breakers"] {
            let mut dialog = FileDialog::new(Operation::Save, &dir.0, title, 512);
            assert!(name(&dialog).is_empty());
            dialog.input(Input::Text("EXISTING".into()));
            assert_eq!(dialog.input(Input::Enter), None);
            assert!(dialog.confirm.is_some());
            assert!(!dialog.yes);
            assert_eq!(dialog.input(Input::Enter), None);
            assert!(dialog.confirm.is_none());
            assert_eq!(dialog.focus, Focus::Name);
            assert_eq!(name(&dialog), "EXISTING");
            dialog.input(Input::Enter);
            dialog.input(Input::Right(false));
            assert_eq!(
                dialog.input(Input::Enter),
                Some(Action::Choose {
                    path: dir.0.join("EXISTING"),
                    replace: true
                })
            );
        }
        let mut fixed =
            FileDialog::fixed(Operation::Save, &dir.0.join("EXISTING"), "Mindwheel", 512);
        assert_eq!(fixed.input(Input::Text("n".into())), Some(Action::Cancel));
        let mut script = FileDialog::new(Operation::Transcript, &dir.0, "Breakers", 512);
        assert!(name(&script).is_empty());
        script.input(Input::Text("EXISTING".into()));
        script.input(Input::Enter);
        assert!(script.confirm.is_some());
        script.input(Input::Escape);
        assert_eq!(name(&script), "EXISTING");
        assert_eq!(
            fs::metadata(dir.0.join("EXISTING")).unwrap().len(),
            (HEADER_BYTES + 512) as u64
        );
    }

    #[test]
    fn text_editor_preserves_unicode_caret_selection_and_long_paths() {
        let dir = Directory::new();
        let mut dialog = FileDialog::new(Operation::Save, &dir.0, "Essex", 512);
        type_name(&mut dialog, "café.sav");
        dialog.input(Input::Left(false));
        dialog.input(Input::Text("X".into()));
        assert_eq!(name(&dialog), "café.saXv");
        dialog.input(Input::Home(false));
        dialog.input(Input::Right(true));
        dialog.input(Input::Right(true));
        assert_eq!(dialog.selected_text().as_deref(), Some("ca"));
        dialog.input(Input::Text("CA".into()));
        assert_eq!(name(&dialog), "CAfé.saXv");
        dialog.input(Input::SelectAll);
        dialog.input(Input::Delete);
        assert!(dialog.name.is_empty());
        let long = "x".repeat(180);
        dialog.input(Input::Text(long.clone()));
        let cursor = dialog.cursor(Duration::ZERO).unwrap();
        assert!(cursor.column < 78);
        assert_eq!(name(&dialog), long);
        assert_eq!(dialog.cells().len(), 4000);
        dialog.input(Input::Tab(false));
        assert!(dialog.cursor(Duration::ZERO).is_none());
    }

    #[test]
    fn cursor_is_always_visible_and_can_switch_to_steady_without_altering_name() {
        let dir = Directory::new();
        let mut dialog = FileDialog::new(Operation::Save, &dir.0, "Breakers", 512);
        let heights: Vec<_> = [0, 100, 134, 200, 267]
            .map(|ms| dialog.cursor(Duration::from_millis(ms)).unwrap().height)
            .into();
        assert_eq!(heights, [1, 1, 2, 2, 1]);
        type_name(&mut dialog, "CURSOR");
        dialog.set_cursor_style(FileCursorStyle::SteadyBlock);
        assert_eq!(dialog.cursor(Duration::ZERO).unwrap().height, 8);
        assert_eq!(dialog.cursor(Duration::from_secs(1)).unwrap().height, 8);
        assert_eq!(name(&dialog), "CURSOR");
        #[cfg(feature = "sdl")]
        {
            let pixels = crate::display::render(&dialog.cells(), true).unwrap();
            assert!(pixels.iter().any(|v| *v != 0));
        }
    }

    #[test]
    fn blinking_underline_alternates_two_scanlines_with_no_cursor() {
        let dir = Directory::new();
        let mut dialog = FileDialog::new(Operation::Save, &dir.0, "Mindwheel", 512);
        type_name(&mut dialog, "CURSOR");
        let cells = dialog.cells();
        dialog.set_cursor_style(FileCursorStyle::BlinkingUnderline);
        let heights = [0, 100, 134, 200, 267, 400]
            .map(|ms| dialog.cursor(Duration::from_millis(ms)).map(|c| c.height));
        assert_eq!(heights, [None, None, Some(2), Some(2), None, Some(2)]);
        assert!(dialog.cursor(Duration::from_nanos(133_333_333)).is_none());
        assert_eq!(
            dialog
                .cursor(Duration::from_nanos(133_333_334))
                .unwrap()
                .height,
            2
        );
        assert!(dialog.cursor(Duration::from_nanos(266_666_667)).is_none());
        assert_eq!(name(&dialog), "CURSOR");
        assert_eq!(dialog.cells(), cells);
        dialog.input(Input::Tab(false));
        assert!(dialog.cursor(Duration::from_millis(134)).is_none());
    }

    #[test]
    fn corrupt_bookmarks_and_missing_parents_keep_the_dialog_open() {
        let dir = Directory::new();
        dir.bookmark("BAD.SAV", 512);
        let path = dir.0.join("BAD.SAV");
        let mut bytes = fs::read(&path).unwrap();
        bytes[258..260].copy_from_slice(&129u16.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        let mut load = FileDialog::new(Operation::Load, &dir.0, "Mindwheel", 512);
        assert!(load.input(Input::Enter).is_none());
        assert!(load.notice.contains("Cannot restore"));
        assert_eq!(load.focus, Focus::Name);
        fs::write(dir.0.join("not-a-bookmark"), b"unrelated file").unwrap();
        type_name(&mut load, "not-a-bookmark");
        assert!(load.input(Input::Enter).is_none());
        assert!(load.notice.contains("not a bookmark"));
        let mut save = FileDialog::new(Operation::Save, &dir.0, "Essex", 512);
        type_name(&mut save, "missing/save");
        assert!(save.input(Input::Enter).is_none());
        assert!(save.notice.contains("does not exist"));
        assert!(!dir.0.join("missing").exists());
    }

    #[cfg(unix)]
    #[test]
    fn selected_non_utf8_os_names_are_not_replaced_by_their_display_label() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let dir = Directory::new();
        dir.bookmark("VALID", 512);
        let path = dir.0.join(OsString::from_vec(b"native-\xff".to_vec()));
        fs::rename(dir.0.join("VALID"), &path).unwrap();
        let mut dialog = FileDialog::new(Operation::Load, &dir.0, "Mindwheel", 512);
        assert_eq!(
            dialog.input(Input::Enter),
            Some(Action::Choose {
                path,
                replace: false
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_dates_follow_timezone_and_daylight_saving() {
        const NAME: &str =
            "player::file_dialog::tests::local_dates_follow_timezone_and_daylight_saving";
        if std::env::var("REFORGE_BTZ_LOCAL_DATE").as_deref() != Ok("child") {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env("REFORGE_BTZ_LOCAL_DATE", "child")
                .env("TZ", "EST5EDT,M3.2.0,M11.1.0")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        assert_eq!(date(UNIX_EPOCH), "1969-12-31 19:00");
        assert_eq!(
            date(UNIX_EPOCH - Duration::from_secs(1)),
            "1969-12-31 18:59"
        );
        assert_eq!(
            date(UNIX_EPOCH + Duration::from_secs(1_704_110_400)),
            "2024-01-01 07:00"
        );
        assert_eq!(
            date(UNIX_EPOCH + Duration::from_secs(1_719_835_200)),
            "2024-07-01 08:00"
        );
    }
}
