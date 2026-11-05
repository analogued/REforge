// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Accumulating post-title questions and controls, using external menu text.

use crate::alley::Machine;
use crate::framebuffer::{Dimensions, Framebuffer, pixel_offset};

const TEXT_POINTERS: usize = 0x6d37;
const TEXT_POSITIONS: usize = 0x6d63;
const QUESTION_LINES: [usize; 5] = [1, 2, 3, 4, 5];
const CONTROL_LINES: [usize; 8] = [6, 7, 8, 9, 10, 14, 15, 11];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuKey {
    Kitten,
    HouseCat,
    Tomcat,
    AlleyCat,
    Up,
    Down,
    Confirm,
    Other,
}

impl MenuKey {
    fn difficulty(self) -> Option<u16> {
        match self {
            Self::Kitten => Some(0),
            Self::HouseCat => Some(1),
            Self::Tomcat => Some(2),
            Self::AlleyCat => Some(3),
            Self::Up | Self::Down | Self::Confirm | Self::Other => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuStage {
    Difficulty,
    Controls,
    Finished,
}

struct Line {
    row: u8,
    text: Vec<u8>,
}

impl Line {
    fn external(data: &[u8], index: usize) -> Result<Self, String> {
        let pointer = table_word(data, TEXT_POINTERS, index)?;
        let position = table_word(data, TEXT_POSITIONS, index)?;
        let tail = data
            .get(usize::from(pointer)..)
            .ok_or("Menu text pointer exceeds executable data")?;
        let length = tail
            .iter()
            .position(|&byte| byte == 0)
            .ok_or("External menu text is not null terminated")?;
        let text = &tail[..length];
        if !text.iter().all(|byte| (b' '..=b'~').contains(byte)) {
            return Err("External menu text contains non-printable ASCII".into());
        }
        let line = Self {
            // CS:5e5b keeps DH from the external position and explicitly sets
            // DL to zero. Its input's column byte does not affect placement.
            row: (position >> 8) as u8,
            text: text.to_vec(),
        };
        line.validate()?;
        Ok(line)
    }

    fn validate(&self) -> Result<(), String> {
        if self.row >= 25 || self.text.len() > 40 {
            return Err("External menu text exceeds the 40 by 25 screen".into());
        }
        Ok(())
    }

    fn draw(&self, framebuffer: &mut Framebuffer) -> Result<(), String> {
        for (column, &character) in self.text.iter().enumerate() {
            let mut pixels = crate::font::character(character)?;
            // The shared font packs color three; BIOS teletype BL=2 renders
            // these menu glyphs in color two on the cleared color-zero screen.
            pixels.iter_mut().for_each(|byte| *byte &= 0xaa);
            framebuffer.copy_rect(
                pixel_offset(column as u16 * 8, self.row * 8),
                Dimensions(0x0801),
                &pixels,
            )?;
        }
        Ok(())
    }
}

fn table_word(data: &[u8], table: usize, index: usize) -> Result<u16, String> {
    let offset = table + index * 2;
    let bytes = data
        .get(offset..offset + 2)
        .ok_or("External menu table is truncated")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

pub struct Menu {
    machine: Machine,
    framebuffer: Framebuffer,
    controls: Vec<Line>,
    difficulty: Option<u16>,
    stage: MenuStage,
    question_frame: Framebuffer,
    skill_rows: [u8; 4],
    cursor_selection: Option<u16>,
}

impl Menu {
    pub fn new(data: &[u8], machine: Machine) -> Result<Self, String> {
        let questions = QUESTION_LINES
            .into_iter()
            .map(|index| Line::external(data, index))
            .collect::<Result<Vec<_>, _>>()?;
        let mut controls = CONTROL_LINES
            .into_iter()
            .map(|index| Line::external(data, index))
            .collect::<Result<Vec<_>, _>>()?;
        // Keep the native Ctrl-S sound instruction; describe host pause keys.
        controls[4].text = b"P / Pause  toggles paws mode.".to_vec();
        let mut framebuffer = Framebuffer::default();
        for line in &questions {
            line.draw(&mut framebuffer)?;
        }
        Ok(Self {
            machine,
            question_frame: framebuffer.clone(),
            skill_rows: std::array::from_fn(|i| questions[i + 1].row),
            framebuffer,
            controls,
            difficulty: None,
            stage: MenuStage::Difficulty,
            cursor_selection: None,
        })
    }

    /// An explicitly supplied starting difficulty bypasses the question while
    /// retaining the controls and their separate key-to-start wait.
    pub fn with_difficulty(data: &[u8], machine: Machine, difficulty: u16) -> Result<Self, String> {
        if difficulty > 7 {
            return Err("Menu difficulty must be in 0..7".into());
        }
        let mut menu = Self::new(data, machine)?;
        menu.framebuffer = Framebuffer::default();
        menu.select_difficulty(difficulty)?;
        Ok(menu)
    }

    fn select_difficulty(&mut self, difficulty: u16) -> Result<(), String> {
        let mut framebuffer = self.framebuffer.clone();
        for line in &self.controls {
            line.draw(&mut framebuffer)?;
        }
        self.framebuffer = framebuffer;
        self.difficulty = Some(difficulty);
        self.stage = MenuStage::Controls;
        Ok(())
    }

    /// Handle a fresh key press. Selecting a difficulty reveals the controls;
    /// a separate subsequent key press starts play. Invalid choices are ignored.
    pub fn press(&mut self, key: MenuKey) -> Result<bool, String> {
        match self.stage {
            MenuStage::Difficulty => {
                if let Some(difficulty) = key.difficulty() {
                    self.framebuffer = self.question_frame.clone();
                    self.cursor_selection = None;
                    self.select_difficulty(difficulty)?;
                } else {
                    match key {
                        MenuKey::Up => return self.controller(-1, false),
                        MenuKey::Down => return self.controller(1, false),
                        MenuKey::Confirm => return self.controller(0, true),
                        _ => {}
                    }
                }
            }
            MenuStage::Controls => self.stage = MenuStage::Finished,
            MenuStage::Finished => {}
        }
        Ok(self.finished())
    }

    /// Reveal the first choice without moving or selecting it. Used when a
    /// controller advanced the title, or for the first navigation input here.
    pub fn reveal_cursor(&mut self) -> Result<(), String> {
        if self.stage == MenuStage::Difficulty && self.cursor_selection.is_none() {
            self.cursor_selection = Some(0);
            self.draw_cursor()?;
        }
        Ok(())
    }

    fn draw_cursor(&mut self) -> Result<(), String> {
        let selection = self.cursor_selection.expect("visible menu cursor");
        self.framebuffer = self.question_frame.clone();
        self.framebuffer.copy_rect(
            pixel_offset(39 * 8, self.skill_rows[usize::from(selection)] * 8),
            Dimensions(0x0801),
            &crate::font::character(b'<')?,
        )
    }

    /// Controller and keyboard navigation share one visible selection. The
    /// first input reveals Kitten; only subsequent input can move or confirm.
    pub fn controller(&mut self, delta: i8, confirm: bool) -> Result<bool, String> {
        if self.stage == MenuStage::Difficulty && (delta != 0 || confirm) {
            let Some(selection) = self.cursor_selection else {
                self.reveal_cursor()?;
                return Ok(false);
            };
            let selection = (selection as i16 + i16::from(delta)).rem_euclid(4) as u16;
            self.cursor_selection = Some(selection);
            if confirm {
                self.select_difficulty(selection)?;
            } else {
                self.draw_cursor()?;
            }
        } else if self.stage == MenuStage::Controls && confirm {
            self.stage = MenuStage::Finished;
        }
        Ok(self.finished())
    }

    pub fn stage(&self) -> MenuStage {
        self.stage
    }

    pub fn difficulty(&self) -> Option<u16> {
        self.difficulty
    }

    pub fn finished(&self) -> bool {
        self.stage == MenuStage::Finished
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn rgba(&self) -> Vec<u8> {
        let colors = self.machine.colors();
        self.framebuffer
            .indices()
            .into_iter()
            .flat_map(|index| colors[usize::from(index)])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn controller_selects_each_skill_then_requires_a_separate_confirmation() {
        for (delta, expected) in [(0, 0), (1, 1), (2, 2), (-1, 3)] {
            let mut menu = Menu::new(&synthetic_data(), Machine::Cga).unwrap();
            menu.reveal_cursor().unwrap();
            let initial = menu.framebuffer.clone();
            assert!(!menu.controller(delta, false).unwrap());
            if delta != 0 {
                assert!(menu.framebuffer.bytes() != initial.bytes());
            }
            assert!(!menu.controller(0, true).unwrap());
            assert_eq!(menu.difficulty(), Some(expected));
            assert_eq!(menu.stage(), MenuStage::Controls);
            assert!(!menu.controller(1, false).unwrap());
            assert!(menu.controller(0, true).unwrap());
        }
        let mut keyboard = Menu::new(&synthetic_data(), Machine::Cga).unwrap();
        keyboard.press(MenuKey::Kitten).unwrap();
        let mut mixed = Menu::new(&synthetic_data(), Machine::Cga).unwrap();
        mixed.controller(1, false).unwrap();
        mixed.press(MenuKey::Kitten).unwrap();
        assert!(
            mixed.framebuffer.bytes() == keyboard.framebuffer.bytes(),
            "keyboard selection must remove an old controller highlight"
        );
    }
    use super::*;
    use crate::assets::{DATA_LENGTH, set_word};

    fn synthetic_data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        for (index, row) in [
            (1, 2),
            (2, 4),
            (3, 5),
            (4, 6),
            (5, 7),
            (6, 9),
            (7, 10),
            (8, 11),
            (9, 12),
            (10, 13),
            (14, 15),
            (15, 16),
            (11, 20),
        ] {
            let pointer = 0x7000 + index * 4;
            set_word(&mut data, TEXT_POINTERS + index * 2, pointer as u16);
            set_word(&mut data, TEXT_POSITIONS + index * 2, row << 8);
            data[pointer] = b'A' + index as u8;
        }
        // No entry zero is populated: the omitted joystick question must never
        // be parsed or rendered, even if its original pointer is invalid.
        set_word(&mut data, TEXT_POINTERS, 0xffff);
        data
    }

    fn cell(menu: &Menu, row: u8, column: u16) -> Vec<u8> {
        menu.framebuffer()
            .read_rect(pixel_offset(column * 8, row * 8), Dimensions(0x0801))
            .unwrap()
    }

    #[test]
    fn first_navigation_reveals_kitten_without_moving_or_selecting() {
        for machine in [Machine::Cga, Machine::Pcjr] {
            for key in [MenuKey::Up, MenuKey::Down, MenuKey::Confirm] {
                let mut menu = Menu::new(&synthetic_data(), machine).unwrap();
                assert_eq!(menu.cursor_selection, None);
                assert!(!menu.press(key).unwrap());
                assert_cursor_on_kitten(&menu);
            }
            for (delta, confirm) in [(-1, false), (1, false), (0, true), (1, true)] {
                let mut menu = Menu::new(&synthetic_data(), machine).unwrap();
                assert!(!menu.controller(0, false).unwrap());
                assert_eq!(
                    menu.cursor_selection, None,
                    "idle polling must not reveal the cursor"
                );
                assert!(!menu.controller(delta, confirm).unwrap());
                assert_cursor_on_kitten(&menu);
            }
        }
    }

    fn assert_cursor_on_kitten(menu: &Menu) {
        assert_eq!(menu.stage(), MenuStage::Difficulty);
        assert_eq!(menu.difficulty(), None);
        assert_eq!(menu.cursor_selection, Some(0));
        assert_eq!(cell(menu, 4, 39), crate::font::character(b'<').unwrap());
        for row in [5, 6, 7] {
            assert_eq!(cell(menu, row, 39), [0; 16]);
        }
        assert_eq!(
            cell(menu, 9, 0),
            [0; 16],
            "revealing must not append instructions"
        );
    }

    #[test]
    fn controller_title_cursor_is_visible_before_confirmation_and_ignores_idle() {
        for machine in [Machine::Cga, Machine::Pcjr] {
            let mut menu = Menu::new(&synthetic_data(), machine).unwrap();
            menu.reveal_cursor().unwrap();
            assert_cursor_on_kitten(&menu);
            let shown = menu.framebuffer().bytes().to_vec();
            assert!(!menu.controller(0, false).unwrap());
            menu.reveal_cursor().unwrap();
            assert_eq!(menu.framebuffer().bytes().as_slice(), shown);
            assert!(!menu.controller(0, true).unwrap());
            assert_eq!(menu.difficulty(), Some(0));
            assert_eq!(menu.stage(), MenuStage::Controls);
            assert!(!menu.controller(0, false).unwrap());
            assert!(menu.controller(0, true).unwrap());
        }
    }

    #[test]
    fn keyboard_and_controller_share_cursor_and_wrap_then_confirm() {
        for confirm_with_keyboard in [true, false] {
            let mut menu = Menu::new(&synthetic_data(), Machine::Cga).unwrap();
            menu.press(MenuKey::Down).unwrap(); // Reveal, without moving.
            menu.press(MenuKey::Down).unwrap();
            assert_eq!(menu.cursor_selection, Some(1));
            menu.controller(-1, false).unwrap();
            assert_eq!(menu.cursor_selection, Some(0));
            menu.press(MenuKey::Up).unwrap();
            assert_eq!(menu.cursor_selection, Some(3));
            menu.press(MenuKey::Down).unwrap();
            assert_cursor_on_kitten(&menu);
            menu.controller(-1, false).unwrap();
            if confirm_with_keyboard {
                menu.press(MenuKey::Confirm).unwrap();
            } else {
                menu.controller(0, true).unwrap();
            }
            assert_eq!(menu.difficulty(), Some(3));
            assert_eq!(menu.stage(), MenuStage::Controls);
            assert!(menu.press(MenuKey::Confirm).unwrap());
        }
        let mut explicit = Menu::with_difficulty(&synthetic_data(), Machine::Cga, 7).unwrap();
        let before = explicit.framebuffer().bytes().to_vec();
        explicit.reveal_cursor().unwrap();
        assert_eq!(explicit.framebuffer().bytes().as_slice(), before);
        assert_eq!(
            explicit.cursor_selection, None,
            "CLI difficulty keeps its controls-only screen"
        );
    }

    #[test]
    fn external_positions_and_text_determine_the_question_pixels() {
        let mut data = synthetic_data();
        set_word(&mut data, TEXT_POSITIONS + 2, 0x170f);
        data[0x7004] = b'Z';
        let original = data.clone();
        let menu = Menu::new(&data, Machine::Cga).unwrap();
        let expected = crate::font::character(b'Z')
            .unwrap()
            .map(|byte| byte & 0xaa);
        assert_eq!(cell(&menu, 23, 0), expected);
        assert_eq!(cell(&menu, 23, 15), [0; 16]);
        assert_eq!(cell(&menu, 2, 0), [0; 16]);
        assert_eq!(cell(&menu, 0, 0), [0; 16]);
        assert_eq!(data, original);
        assert!(
            menu.framebuffer()
                .indices()
                .iter()
                .all(|&pixel| pixel == 0 || pixel == 2)
        );
    }

    #[test]
    fn invalid_choices_preserve_question_and_valid_choice_appends_controls() {
        let mut menu = Menu::new(&synthetic_data(), Machine::Cga).unwrap();
        let before = menu.framebuffer().bytes().to_vec();
        let questions: Vec<_> = [2, 4, 5, 6, 7]
            .into_iter()
            .map(|row| cell(&menu, row, 0))
            .collect();
        for _ in 0..3 {
            assert!(!menu.press(MenuKey::Other).unwrap());
            assert_eq!(menu.stage(), MenuStage::Difficulty);
            assert_eq!(menu.difficulty(), None);
            assert_eq!(menu.framebuffer().bytes().as_slice(), before);
        }
        assert!(!menu.press(MenuKey::HouseCat).unwrap());
        assert_eq!(menu.difficulty(), Some(1));
        assert_eq!(menu.stage(), MenuStage::Controls);
        assert_ne!(cell(&menu, 9, 0), [0; 16]);
        for (row, question) in [2, 4, 5, 6, 7].into_iter().zip(questions) {
            assert_eq!(cell(&menu, row, 0), question);
        }
        let controls = menu.framebuffer().bytes().to_vec();
        assert!(menu.press(MenuKey::Other).unwrap());
        assert!(menu.press(MenuKey::Kitten).unwrap());
        assert_eq!(menu.difficulty(), Some(1));
        assert_eq!(menu.stage(), MenuStage::Finished);
        assert_eq!(menu.framebuffer().bytes().as_slice(), controls);
    }

    #[test]
    fn all_four_choices_use_the_original_starting_difficulties() {
        for (key, difficulty) in [
            (MenuKey::Kitten, 0),
            (MenuKey::HouseCat, 1),
            (MenuKey::Tomcat, 2),
            (MenuKey::AlleyCat, 3),
        ] {
            let mut menu = Menu::new(&synthetic_data(), Machine::Pcjr).unwrap();
            assert!(!menu.press(key).unwrap());
            assert_eq!(menu.difficulty(), Some(difficulty));
            assert!(menu.press(key).unwrap());
        }
    }

    #[test]
    fn explicit_difficulty_bypasses_question_but_waits_on_controls() {
        let data = synthetic_data();
        let mut menu = Menu::with_difficulty(&data, Machine::Cga, 7).unwrap();
        assert_eq!(menu.difficulty(), Some(7));
        assert_eq!(menu.stage(), MenuStage::Controls);
        for row in [2, 4, 5, 6, 7] {
            assert_eq!(cell(&menu, row, 0), [0; 16]);
        }
        assert_ne!(cell(&menu, 9, 0), [0; 16]);
        assert!(menu.press(MenuKey::Other).unwrap());
        assert!(Menu::with_difficulty(&data, Machine::Cga, 8).is_err());
    }

    #[test]
    fn appended_controls_retain_their_external_text_and_row() {
        let mut data = synthetic_data();
        set_word(&mut data, TEXT_POSITIONS + 6 * 2, 0x1800);
        data[0x7018] = b'Y';
        let mut menu = Menu::new(&data, Machine::Cga).unwrap();
        assert_eq!(cell(&menu, 24, 0), [0; 16]);
        menu.press(MenuKey::Kitten).unwrap();
        assert_eq!(
            cell(&menu, 24, 0),
            crate::font::character(b'Y')
                .unwrap()
                .map(|byte| byte & 0xaa)
        );
        assert_eq!(cell(&menu, 9, 0), [0; 16]);
    }

    #[test]
    fn malformed_external_text_and_layout_are_rejected_at_creation() {
        assert!(Menu::new(&[0; 20], Machine::Cga).is_err());
        let original = synthetic_data();
        let mut data = original.clone();
        set_word(&mut data, TEXT_POINTERS + 2, 0xffff);
        assert!(Menu::new(&data, Machine::Cga).is_err());
        let mut data = original.clone();
        set_word(&mut data, TEXT_POINTERS + 2, (DATA_LENGTH - 1) as u16);
        data[DATA_LENGTH - 1] = b'A';
        assert!(Menu::new(&data, Machine::Cga).is_err());
        let mut data = original.clone();
        data[0x7004] = b'\n';
        assert!(Menu::new(&data, Machine::Cga).is_err());
        let mut data = original.clone();
        set_word(&mut data, TEXT_POSITIONS + 2, 25 << 8);
        assert!(Menu::new(&data, Machine::Cga).is_err());
        let mut data = original;
        set_word(&mut data, TEXT_POINTERS + 2, 0x7100);
        data[0x7100..0x7129].fill(b'A');
        assert!(Menu::new(&data, Machine::Cga).is_err());
    }
}
