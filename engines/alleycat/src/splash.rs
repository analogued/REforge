// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! The original first and post-game title screens, using an external-data copy.
//!
//! A call to `advance` represents one pass through the title loop. Its BIOS
//! tick and retrace samples are supplied by the caller; this module has no host
//! clock. Menus and sound output are separate from this presentation.

use crate::alley::Machine;
use crate::assets::{DATA_LENGTH, idle_sources, next_random, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer, PlayerCompositor, pixel_offset};
use crate::player::{Controls, Draw, Player};
use crate::timing::{BiosSample, PlayerGate};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SplashStep {
    pub finished: bool,
    pub player_accepted: bool,
    pub redrawn: bool,
    pub(crate) work: TitleWork,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TitleWork {
    pub steering: Option<Steering>,
    pub copyright: bool,
    pub player_called: bool,
    pub player_countdown_zero: bool,
    pub entering: bool,
    pub draw: Option<Draw>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Steering {
    LeftEdge,
    RightEdge,
    Recent,
    RandomMoving,
    RandomIdle,
}

pub struct Splash {
    data: Vec<u8>,
    pub(crate) sound: crate::sound::Sound,
    machine: Machine,
    framebuffer: Framebuffer,
    player: Player,
    compositor: PlayerCompositor,
    gate: PlayerGate,
    repeated: bool,
    pending_key: bool,
    finished: bool,
}

impl Splash {
    pub fn new(external_data: &[u8], machine: Machine) -> Result<Self, String> {
        Self::with_start_tick(external_data, machine, 0)
    }

    /// A nonzero external seed permits controlled native comparisons. The
    /// executable's uninitialized zero seed uses the existing startup default.
    pub fn with_start_tick(
        external_data: &[u8],
        machine: Machine,
        tick: u16,
    ) -> Result<Self, String> {
        Self::build(external_data, machine, tick, false)
    }

    /// Return to the title using the completed game's score, selected skill,
    /// sound flag, room history and shared random state. This entry repeats the
    /// title sequence until a fresh key make event.
    pub fn after_game_over(
        external_data: &[u8],
        machine: Machine,
        tick: u16,
    ) -> Result<Self, String> {
        Self::build(external_data, machine, tick, true)
    }

    fn build(
        external_data: &[u8],
        machine: Machine,
        tick: u16,
        repeated: bool,
    ) -> Result<Self, String> {
        if external_data.len() < DATA_LENGTH {
            return Err("Executable splash data segment is truncated".into());
        }
        let mut data = external_data.to_vec();
        if !repeated && word(&data, 0x2ae5) == 0 {
            set_word(&mut data, 0x2ae5, 0xfa59);
        }
        data[0x697] = if machine == Machine::Pcjr { 0xfd } else { 0xfc };
        set_word(&mut data, 4, 0);
        set_word(&mut data, 8, 0);
        data[0x41a] = u8::from(repeated);
        if !repeated {
            set_word(&mut data, 0x6df8, 0);
            data[0] = 0xff;
            data[0x69a] = 16;
            data[0x69b] = 0; // Desktop keyboard path; joystick setup is omitted.
            set_word(&mut data, 0x41d, 0xffff);
            set_word(&mut data, 0x41f, 0xffff);
            crate::decoration::initialize_score_state(&mut data)?;
        }
        let mut framebuffer = Framebuffer::default();
        draw_scenery(&mut data, &mut framebuffer)?;
        for (source, destination, dimensions) in [
            (0x6152, 0x00bd, 0x1d0b),
            (0x63d0, 0x069e, 0x160e),
            (0x6638, 0x0a78, 0x0c03),
            (0x6680, 0x0ca8, 0x080e),
            (0x6760, 0x1d6e, 0x0b0c),
            (0x6868, 0x1dec, 0x0804),
        ] {
            blit(&data, &mut framebuffer, source, destination, dimensions)?;
        }
        set_word(&mut data, 0x6a8d, 0);
        draw_copyright(&mut data, &mut framebuffer)?;

        // CS:070d's entry state, followed by the title's Y96 overrides. Other
        // movement fields retain their executable initialization values.
        for (offset, value) in [
            (0x579, 0),
            (0x55f, pixel_offset(0, 180)),
            (0x561, 0x0b03),
            (0x572, 2),
            (0x554, 0),
            (0x57d, 0),
            (0x684, 0),
        ] {
            set_word(&mut data, offset, value);
        }
        for (offset, value) in [
            (0x56e, 1),
            (0x558, 3),
            (0x559, 12),
            (0x57b, 96),
            (0x57c, 146),
            (0x576, 1),
            (0x571, 0),
            (0x55b, 0),
            (0x550, 0),
            (0x55c, 0),
            (0x55a, 0),
            (0x583, 0),
            (0x698, 0),
            (0x699, 0),
            (0x551, 0),
            (0x584, 0),
            (0x552, 0),
            (0x553, 0),
            (0x127c, 0),
            (0x1665, 0),
            (0x1673, 0),
            (0x1677, 0),
            (0x1678, 0),
            (0x1cbf, 0),
            (0x1cc0, 0),
            (0x1cc1, 0),
            (0x1cb8, 0),
            (0x1cc8, 0xb1),
            (0x6a8a, 0),
        ] {
            data[offset] = value;
        }
        set_word(&mut data, 0x166c, 9);
        set_word(&mut data, 0x1ce1, 0);
        data[0x1f80] = 9;
        data[0x1f81] = 0xff;
        crate::decoration::draw_hud(&mut data, &mut framebuffer)?;
        let make_counter = word(&data, 0x693);
        set_word(&mut data, 0x6150, make_counter);
        for offset in [0x6a8b, 0x5322, 0x6a93] {
            set_word(&mut data, offset, tick);
        }
        set_word(&mut data, 0x6a88, tick.wrapping_sub(48));
        set_word(&mut data, 0x5320, 0);
        let player = Player::from_data(&data).map_err(str::to_owned)?;
        Ok(Self {
            sound: crate::sound::Sound::default(),
            data,
            machine,
            framebuffer,
            player,
            compositor: PlayerCompositor::default(),
            gate: PlayerGate::default(),
            repeated,
            pending_key: false,
            finished: false,
        })
    }

    /// Advance one original title-loop opportunity. `key_pressed` represents a
    /// fresh key make event, rather than a held key or a key release.
    pub fn advance(
        &mut self,
        tick: u16,
        retrace: bool,
        key_pressed: bool,
    ) -> Result<SplashStep, String> {
        let mut step = SplashStep {
            finished: self.finished,
            ..SplashStep::default()
        };
        if self.finished {
            return Ok(step);
        }
        // The original compares its cumulative make counter to the entry
        // baseline. A sequence reset leaves an untested make event pending.
        self.pending_key |= key_pressed;
        if tick.wrapping_sub(word(&self.data, 0x6a93)) >= 36 {
            set_word(&mut self.data, 0x6a93, tick);
            draw_copyright(&mut self.data, &mut self.framebuffer)?;
            step.work.copyright = true;
            step.redrawn = true;
        }
        let elapsed = tick.wrapping_sub(word(&self.data, 0x6a8b));
        let duration = word(&self.data, 0x56da);
        if self.repeated && elapsed >= duration.wrapping_add(72) {
            for offset in [0x6a8b, 0x5322, 0x6a93] {
                set_word(&mut self.data, offset, tick);
            }
            set_word(&mut self.data, 0x6a88, tick.wrapping_sub(48));
            set_word(&mut self.data, 0x5320, 0);
            // CS:5dd4 restarts at the BIOS sample before music, movement and
            // key testing. The caller supplies that new sample on its next pass.
            return Ok(step);
        }
        if !self.repeated && elapsed > duration.wrapping_add(6) {
            self.sound.stop();
            self.finished = true;
            step.finished = true;
            return Ok(step);
        }
        self.advance_music_sequence(tick)?;
        step.work.steering = Some(self.choose_direction(tick));
        // CS:5e1c tests retrace before even entering the player acceptance gate.
        if retrace {
            step.work.player_called = true;
            step.work.player_countdown_zero = self.gate.countdown == 0;
            step.work.entering = self.player.entry_state != 0;
            self.player.horizontal_step = 4;
            let accepted = self.gate.attempt(
                BiosSample {
                    tick_low: tick,
                    ax: 0,
                },
                self.machine,
                0,
                self.player.horizontal_direction as i8,
                self.player.vertical_direction as i8,
                retrace,
            );
            set_word(&mut self.data, 0x684, self.gate.countdown);
            if let Some(acceptance) = accepted {
                set_word(&mut self.data, 0x57d, acceptance.tick_low);
                set_word(&mut self.data, 0x57f, acceptance.ax);
                let outcome = self
                    .player
                    .update(
                        Controls {
                            x: self.data[0x698] as i8,
                            y: 0,
                        },
                        &self.data,
                    )
                    .map_err(str::to_owned)?;
                step.player_accepted = true;
                step.work.draw = Some(outcome.draw);
                step.redrawn |= self.draw_player(outcome.draw)?;
                self.player
                    .write_data(&mut self.data)
                    .map_err(str::to_owned)?;
            } else {
                set_word(&mut self.data, 0x572, 4);
            }
        }
        if self.pending_key {
            self.sound.stop();
            self.finished = true;
        }
        step.finished = self.finished;
        Ok(step)
    }

    fn choose_direction(&mut self, tick: u16) -> Steering {
        if self.player.x <= 32 {
            self.data[0x698] = 1;
            Steering::LeftEdge
        } else if self.player.x >= 288 {
            self.data[0x698] = 0xff;
            Steering::RightEdge
        } else if tick.wrapping_sub(word(&self.data, 0x6a88)) >= 18 {
            set_word(&mut self.data, 0x6a88, tick);
            let random = next_random(&mut self.data) as u8;
            self.data[0x698] = if random > 0xa0 {
                0
            } else if random & 1 == 0 {
                0xff
            } else {
                1
            };
            if random > 0xa0 {
                Steering::RandomIdle
            } else {
                Steering::RandomMoving
            }
        } else {
            Steering::Recent
        }
    }

    fn advance_music_sequence(&mut self, tick: u16) -> Result<(), String> {
        self.sound.splash(&mut self.data, tick)
    }

    fn draw_player(&mut self, draw: Draw) -> Result<bool, String> {
        let offset = pixel_offset(self.player.x, self.player.y);
        match draw {
            Draw::Keep => return Ok(false),
            Draw::Erase => self.compositor.restore(&mut self.framebuffer)?,
            Draw::Idle => {
                let (top, bottom) = idle_sources(&mut self.data);
                self.compositor.draw_idle(
                    &mut self.framebuffer,
                    offset,
                    sprite_bytes(&self.data, top, Dimensions(0x0602))?,
                    sprite_bytes(&self.data, bottom, Dimensions(0x0602))?,
                )?;
            }
            Draw::Sprite(sprite) => self.compositor.draw(
                &mut self.framebuffer,
                offset,
                Dimensions(sprite.dimensions),
                sprite_bytes(&self.data, sprite.source, Dimensions(sprite.dimensions))?,
            )?,
            Draw::StridedSprite {
                sprite,
                stride_bytes,
            } => {
                let dimensions = Dimensions(sprite.dimensions);
                let width = dimensions.width_bytes();
                let mut bytes = Vec::with_capacity(dimensions.len());
                for row in 0..dimensions.rows() {
                    let start = usize::from(sprite.source) + row * stride_bytes;
                    bytes.extend_from_slice(
                        self.data
                            .get(start..start + width)
                            .ok_or("Splash entry sprite exceeds external data")?,
                    );
                }
                self.compositor
                    .draw(&mut self.framebuffer, offset, dimensions, &bytes)?;
            }
        }
        set_word(&mut self.data, 0x55f, offset);
        Ok(true)
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    pub fn player(&self) -> &Player {
        &self.player
    }
    #[cfg(feature = "sdl")]
    pub(crate) fn toggle_sound(&mut self) {
        self.data[0] = !self.data[0];
        if self.data[0] == 0 {
            self.sound.stop();
        }
    }
    pub fn machine(&self) -> Machine {
        self.machine
    }
    pub fn finished(&self) -> bool {
        self.finished
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

fn blit(
    data: &[u8],
    screen: &mut Framebuffer,
    source: u16,
    destination: u16,
    dimensions: u16,
) -> Result<(), String> {
    let dimensions = Dimensions(dimensions);
    screen.copy_rect(
        destination,
        dimensions,
        sprite_bytes(data, source, dimensions)?,
    )
}

fn draw_copyright(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    let counter = word(data, 0x6a8d).wrapping_add(2);
    set_word(data, 0x6a8d, counter);
    blit(
        data,
        screen,
        word(data, 0x6a8f + usize::from(counter & 2)),
        0x1d38,
        0x0c0a,
    )
}

/// CS:2a30 shares the alley's sidewalk/fence/bin art, omitting all clothes.
fn draw_scenery(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    screen.fill_visible(0xaa);
    let mut previous = data[0x2ac4];
    for destination in (0x1040..0x1090).step_by(2) {
        let selected = loop {
            let selected = next_random(data) & 0x30;
            if selected as u8 != previous {
                break selected;
            }
        };
        previous = selected as u8;
        blit(data, screen, 0x2904 + selected, destination, 0x0801)?;
    }
    let ground: Vec<u8> = [0x55, 0x56].into_iter().cycle().take(0x0a00).collect();
    screen.copy_bytes(0x1180, ground.len(), 1, &ground)?;
    screen.copy_bytes(0x3180, ground.len(), 1, &ground)?;
    for source in (0x2944..0x296c).step_by(10) {
        for _ in 0..9 {
            let destination = (next_random(data) & 0x0776) + 0x12c0;
            blit(data, screen, source, destination, 0x0501)?;
        }
    }
    for _ in 0..5 {
        let destination = (next_random(data) & 0x3e) + 0x3a98;
        blit(data, screen, 0x296c, destination, 0x0501)?;
    }
    data[0x2ac4] = 0;
    let dimensions = word(data, 0x28a0);
    let width = usize::from(dimensions as u8);
    let rows = usize::from(dimensions >> 8);
    let mut list = 0x28a2;
    loop {
        let entry = data
            .get(list..list + 4)
            .ok_or("Unterminated splash fence list")?;
        let source = usize::from(u16::from_le_bytes([entry[0], entry[1]]));
        if source == 0xffff {
            break;
        }
        let destination = u16::from_le_bytes([entry[2], entry[3]]);
        let bytes = data
            .get(source..source + width * rows)
            .ok_or("Splash fence rectangle exceeds external data")?;
        screen.copy_bytes(destination, width, rows, bytes)?;
        list += 4;
    }
    let skill = usize::from(word(data, 0x6df8) & 3);
    blit(data, screen, word(data, 0x2ad1 + skill * 2), 0x1902, 0x0801)?;
    let mut list = 0x2a86 + usize::from(data[0x2ab3]);
    loop {
        let entry = data
            .get(list..list + 2)
            .ok_or("Unterminated splash bin list")?;
        let destination = u16::from_le_bytes([entry[0], entry[1]]);
        if destination == 0 {
            break;
        }
        blit(data, screen, 0x2976, destination, 0x0c05)?;
        let mut middle = destination.wrapping_add(0x1e0);
        for _ in 0..if destination < 0x1720 { 3 } else { 2 } {
            blit(data, screen, 0x29ee, middle, 0x0804)?;
            middle = middle.wrapping_add(0x140);
        }
        blit(data, screen, 0x2a2e, middle, 0x0b04)?;
        list += 2;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x2ae5, 0xfa59);
        set_word(&mut data, 0x28a0, 0x0802);
        set_word(&mut data, 0x28a2, 0xffff);
        set_word(&mut data, 0x6a8f, 0x68a8);
        set_word(&mut data, 0x6a91, 0x6998);
        data[0x68a8..0x68a8 + 240].fill(0x55);
        data[0x6998..0x6998 + 240].fill(0xff);
        data[0x538c] = 0x66;
        set_word(&mut data, 0x56da, 846);
        for offset in (0x0f7a..0x0faa).step_by(2) {
            set_word(&mut data, offset, 0x1800);
        }
        data[0x1800..0x1880].fill(0x55);
        data
    }

    #[test]
    fn external_input_is_immutable_and_title_construction_is_repeatable() {
        let mut data = synthetic_data();
        set_word(&mut data, 0x2ae5, 0x1234);
        let before = data.clone();
        let mut splash = Splash::new(&data, Machine::Cga).unwrap();
        for tick in 1..100 {
            splash.advance(tick, true, false).unwrap();
        }
        assert_eq!(data, before);
        let again = Splash::new(&data, Machine::Cga).unwrap();
        let repeated = Splash::new(&data, Machine::Cga).unwrap();
        assert_eq!(again.framebuffer().bytes(), repeated.framebuffer().bytes());
        assert_eq!(again.data(), repeated.data());
    }

    #[test]
    fn copyright_starts_with_second_external_image_and_alternates_at_36_ticks() {
        let mut splash = Splash::new(&synthetic_data(), Machine::Cga).unwrap();
        let image = |splash: &Splash| {
            splash
                .framebuffer()
                .read_rect(0x1d38, Dimensions(0x0c0a))
                .unwrap()
        };
        assert_eq!(image(&splash), vec![0xff; 240]);
        assert!(!splash.advance(35, false, false).unwrap().redrawn);
        assert_eq!(image(&splash), vec![0xff; 240]);
        assert!(splash.advance(36, false, false).unwrap().redrawn);
        assert_eq!(image(&splash), vec![0x55; 240]);
        assert!(!splash.advance(36, false, false).unwrap().redrawn);
        splash.advance(72, false, false).unwrap();
        assert_eq!(image(&splash), vec![0xff; 240]);
    }

    #[test]
    fn initial_timeout_is_strict_and_copyright_runs_before_exit() {
        let mut data = synthetic_data();
        set_word(&mut data, 0x56da, 29);
        let mut splash = Splash::new(&data, Machine::Cga).unwrap();
        assert!(!splash.advance(35, true, false).unwrap().finished);
        let step = splash.advance(36, true, false).unwrap();
        assert!(step.finished && step.redrawn && !step.player_accepted);
        let costs = crate::runtime::ReferenceCosts::ibm_xt(Machine::Cga);
        assert_eq!(costs.splash_nanos(step), costs.title.unwrap().copyright);
        let final_data = splash.data().to_vec();
        let final_frame = splash.framebuffer().bytes().to_vec();
        assert_eq!(
            splash.advance(100, true, true).unwrap(),
            SplashStep {
                finished: true,
                ..SplashStep::default()
            }
        );
        assert_eq!(splash.data(), final_data);
        assert_eq!(splash.framebuffer().bytes().as_slice(), final_frame);
    }

    #[test]
    fn title_prices_copyright_and_player_work_when_they_share_a_pass() {
        for machine in [Machine::Cga, Machine::Pcjr] {
            let mut splash = Splash::new(&synthetic_data(), machine).unwrap();
            let step = splash.advance(36, true, false).unwrap();
            assert!(step.redrawn && step.player_accepted);
            let mut costs = crate::runtime::ReferenceCosts::ibm_xt(machine);
            costs.title.as_mut().unwrap().copyright = 0;
            let player_and_steering = costs.splash_nanos(step);
            costs.title.as_mut().unwrap().copyright = 5_000_000;
            assert_eq!(costs.splash_nanos(step), player_and_steering + 5_000_000);
            let rejected = splash.advance(36, false, false).unwrap();
            assert!(!rejected.work.player_called);
            assert_eq!(costs.splash_nanos(rejected), costs.title.unwrap().left_edge);
        }
    }

    #[test]
    fn title_entry_prices_the_final_handoff_before_ordinary_walking() {
        let mut splash = Splash::new(&synthetic_data(), Machine::Cga).unwrap();
        let mut costs = crate::runtime::ReferenceCosts::ibm_xt(Machine::Cga);
        let title = costs.title.as_mut().unwrap();
        title.entry_wait = 1;
        title.entry_first_crop = 2;
        title.entry_second_crop = 3;
        title.entry_finish = 4;
        title.walk = 5;
        for tick in 1..=16 {
            let mut step = splash.advance(tick, true, false).unwrap();
            assert!(step.player_accepted);
            // Isolate the whole player helper from the title wrapper.
            step.work.steering = None;
            let expected = match tick {
                1..=12 => 1,
                13 => 2,
                14 => 3,
                15 => 4,
                _ => 5,
            };
            assert_eq!(costs.splash_nanos(step), expected);
        }
    }

    #[test]
    fn post_game_title_preserves_run_state_and_refreshes_the_nine_lives_glyph() {
        let mut data = synthetic_data();
        set_word(&mut data, 0x2ae5, 0x1234);
        set_word(&mut data, 4, 6);
        set_word(&mut data, 8, 7);
        set_word(&mut data, 0x6df8, 3);
        set_word(&mut data, 0x2ad1, 0x2800);
        set_word(&mut data, 0x2ad7, 0x2810);
        data[0x2800..0x2810].fill(0x33);
        data[0x2810..0x2820].fill(0xcc);
        set_word(&mut data, 0x41d, 5);
        set_word(&mut data, 0x41f, 1);
        data[0] = 0;
        data[0x1f80] = 0;
        data[0x1f81] = 9;
        data[0x1f82..0x1f89].copy_from_slice(&[0, 1, 2, 3, 4, 5, 6]);
        data[0x1f89..0x1f90].copy_from_slice(&[0, 9, 8, 7, 6, 5, 4]);
        let nine = 0x2720 + 9 * 16;
        data[nine..nine + 16].fill(0x77);
        let before = data.clone();
        let mut expected_random = data.clone();
        draw_scenery(&mut expected_random, &mut Framebuffer::default()).unwrap();

        let mut splash = Splash::after_game_over(&data, Machine::Pcjr, 100).unwrap();
        assert_eq!(data, before);
        assert_eq!(splash.data()[0x1f82..0x1f90], before[0x1f82..0x1f90]);
        assert_eq!(word(splash.data(), 0x6df8), 3);
        assert_eq!(word(splash.data(), 0x41d), 5);
        assert_eq!(word(splash.data(), 0x41f), 1);
        assert_eq!(word(splash.data(), 0x2ae5), word(&expected_random, 0x2ae5));
        assert_eq!(word(splash.data(), 4), 0);
        assert_eq!(word(splash.data(), 8), 0);
        assert_eq!(splash.data()[0x41a], 1);
        assert_eq!(splash.data()[0x1f80..0x1f82], [9, 9]);
        assert_eq!(
            splash
                .framebuffer()
                .read_rect(0x1902, Dimensions(0x0801))
                .unwrap(),
            vec![0xcc; 16]
        );
        assert_eq!(
            splash
                .framebuffer()
                .read_rect(0x1260, Dimensions(0x0801))
                .unwrap(),
            vec![0x77; 16]
        );
        splash.advance(101, false, false).unwrap();
        assert_eq!(splash.data()[0], 0);
        assert_eq!(word(splash.data(), 0x5322), 100);
    }

    #[test]
    fn post_game_timeout_restarts_before_testing_the_pending_key_on_the_next_pass() {
        let mut splash = Splash::after_game_over(&synthetic_data(), Machine::Cga, 100).unwrap();
        assert!(!splash.advance(1017, false, false).unwrap().finished);
        let player = splash.player().clone();
        let step = splash.advance(1018, true, true).unwrap();
        assert!(!step.finished && !step.player_accepted);
        assert_eq!(splash.player().entry_delay, player.entry_delay);
        for offset in [0x6a8b, 0x5322, 0x6a93] {
            assert_eq!(word(splash.data(), offset), 1018);
        }
        assert_eq!(word(splash.data(), 0x6a88), 970);
        assert_eq!(word(splash.data(), 0x5320), 0);
        assert!(splash.advance(1019, true, false).unwrap().finished);
    }

    #[test]
    fn post_game_title_cycles_across_bios_word_wrap_and_keys_exit_early() {
        let mut splash =
            Splash::after_game_over(&synthetic_data(), Machine::Pcjr, u16::MAX - 10).unwrap();
        assert!(!splash.advance(906, false, false).unwrap().finished);
        assert!(!splash.advance(907, false, false).unwrap().finished);
        assert_eq!(word(splash.data(), 0x6a8b), 907);
        assert_eq!(word(splash.data(), 0x6a88), 859);
        assert!(splash.advance(908, false, true).unwrap().finished);

        let mut early = Splash::after_game_over(&synthetic_data(), Machine::Cga, 10).unwrap();
        assert!(early.advance(11, true, true).unwrap().finished);
    }

    #[test]
    fn key_make_exits_after_the_last_player_opportunity() {
        let mut splash = Splash::new(&synthetic_data(), Machine::Pcjr).unwrap();
        let step = splash.advance(1, true, true).unwrap();
        assert!(step.finished && step.player_accepted);
        assert_eq!(splash.player().entry_delay, 11);
    }

    #[test]
    fn title_retrace_gate_does_not_count_rejected_opportunities() {
        for (machine, countdown) in [(Machine::Cga, 32), (Machine::Pcjr, 16)] {
            let mut splash = Splash::new(&synthetic_data(), machine).unwrap();
            assert!(!splash.advance(1, false, false).unwrap().player_accepted);
            assert!(splash.advance(1, true, false).unwrap().player_accepted);
            assert_eq!(word(splash.data(), 0x684), countdown);
            for _ in 0..100 {
                splash.advance(1, false, false).unwrap();
            }
            assert_eq!(word(splash.data(), 0x684), countdown);
            for _ in 1..countdown {
                assert!(!splash.advance(1, true, false).unwrap().player_accepted);
            }
            assert!(splash.advance(1, true, false).unwrap().player_accepted);
            assert!(!splash.advance(1, true, false).unwrap().player_accepted);
        }
    }

    #[test]
    fn cat_entry_waits_twelve_accepted_updates_then_reveals_cropped_sprite() {
        let mut splash = Splash::new(&synthetic_data(), Machine::Cga).unwrap();
        let initial = splash.framebuffer().bytes().to_vec();
        for tick in 1..=12 {
            let step = splash.advance(tick, true, false).unwrap();
            assert!(step.player_accepted && !step.redrawn);
        }
        assert_eq!(splash.framebuffer().bytes().as_slice(), initial);
        let step = splash.advance(13, true, false).unwrap();
        assert!(step.player_accepted && step.redrawn);
        assert_eq!((splash.player().x, splash.player().y), (0, 96));
        assert_eq!(splash.player().entry_state, 2);
        assert_eq!(word(splash.data(), 0x561), 0x0b01);
        splash.advance(14, true, false).unwrap();
        assert_eq!(word(splash.data(), 0x561), 0x0b02);
        splash.advance(15, true, false).unwrap();
        assert_eq!(splash.player().entry_state, 0);
        assert_eq!(word(splash.data(), 0x561), 0x0b03);
        assert_eq!(splash.player().y, 96);
    }

    #[test]
    fn copyright_and_timeout_subtractions_wrap_like_bios_words() {
        let mut splash =
            Splash::with_start_tick(&synthetic_data(), Machine::Cga, u16::MAX - 10).unwrap();
        assert!(!splash.advance(24, false, false).unwrap().redrawn);
        assert!(splash.advance(25, false, false).unwrap().redrawn);
        assert!(!splash.advance(841, false, false).unwrap().finished);
        assert!(splash.advance(842, false, false).unwrap().finished);
    }

    #[test]
    fn truncated_data_and_invalid_external_rectangles_are_rejected() {
        assert!(Splash::new(&[0; 20], Machine::Cga).is_err());
        let mut data = synthetic_data();
        set_word(&mut data, 0x6a91, 0x7100);
        assert!(Splash::new(&data, Machine::Cga).is_err());
    }
}
