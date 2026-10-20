// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Bonus-room return presentation. Original tables and sprites remain external;
//! BIOS-style text uses the attributed SDL glyphs; speaker writes stay ordered
//! with the original presentation stages.

use crate::assets::{DATA_LENGTH, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer, pixel_offset};
use crate::completion::{Wipe, Work};

/// Native presentation work, separate from tick/retrace waits and host redraws.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PresentationWork {
    pub sprites: [u8; 6],
    pub pair_items: u8,
    pub increasing_axes: u8,
    pub decreasing_axes: u8,
    pub upper_clamps: u8,
    pub lower_clamps: u8,
    pub dance_frames: u8,
    pub dance_copies: u8,
    pub bookkeeping: Option<Bookkeeping>,
}

/// Bounded state work, emitted once rather than on subsequent waiting polls.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Bookkeeping {
    ApproachInit,
    Approach { drawing: bool },
    PairInit,
    Reward { life: bool, difficulty: bool },
    DanceInit { active: bool },
}

#[derive(Clone, Copy)]
pub(crate) enum PresentationSprite {
    ApproachCat,
    ApproachPanel,
    PairSmall,
    PairMedium,
    FallingCat,
    LandingCat,
}

impl PresentationWork {
    fn sprite(&mut self, sprite: PresentationSprite) {
        self.sprites[sprite as usize] += 1;
    }

    pub(crate) fn bytes(self) -> usize {
        let sizes = [448, 104, 32, 128, 72, 204];
        self.sprites
            .into_iter()
            .zip(sizes)
            .map(|(count, size)| usize::from(count) * size)
            .sum::<usize>()
            + usize::from(self.dance_copies) * 280
    }
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Start,
    Approach,
    ApproachWait { start: u16, first: bool },
    Score,
    ScoreHold { start: u16, sample: Option<u16> },
    PairStart(u8),
    PairFrame(u8),
    PairWait { group: u8, start: u16 },
    MelodyEnd,
    Reward,
    Black(Wipe),
    DanceStart,
    Dance { start: u16, frame: u16 },
    DropStart,
    Drop,
    DropTick,
    DropHold,
    Fill(Wipe),
    Complete,
}

pub(crate) struct HeartsCompletion {
    phase: Phase,
    center: (u16, u8),
    success: bool,
    background: u8,
    presentation_palette: bool,
    alley_palette: bool,
}

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        return Err("Executable hearts presentation data is truncated".into());
    }
    Ok(())
}

fn copy_sprite(
    data: &mut [u8],
    screen: &mut Framebuffer,
    source: u16,
    destination: u16,
    dimensions: Dimensions,
) -> Result<usize, String> {
    screen.copy_rect(
        destination,
        dimensions,
        sprite_bytes(data, source, dimensions)?,
    )?;
    data[0x2ae0] = dimensions.0 as u8;
    data[0x2ae2] = 0;
    Ok(dimensions.len())
}

fn transparent_sprite(
    data: &mut [u8],
    screen: &mut Framebuffer,
    source: u16,
    destination: u16,
    dimensions: Dimensions,
) -> Result<usize, String> {
    let background = screen.zero_transparent_rect(
        destination,
        dimensions,
        sprite_bytes(data, source, dimensions)?,
    )?;
    let end = 0x0e + background.len();
    data.get_mut(0x0e..end)
        .ok_or("Hearts presentation scratch exceeds executable data")?
        .copy_from_slice(&background);
    data[0x2ae0] = dimensions.0 as u8;
    data[0x2ae2] = 0;
    Ok(dimensions.len())
}

/// CS:5b54/5b63. Logical melody state only; no random draws or host waits.
fn start_melody(data: &mut [u8], tick: u16) {
    set_word(data, 0x59be, 0);
    set_word(data, 0x59c0, tick);
}

fn melody(data: &mut [u8], tick: u16, sound: &mut crate::sound::Sound) {
    sound.melody(data, tick);
}

fn add_decimal(data: &mut [u8], destination: usize, addend: &[u8; 7]) {
    let mut carry = 0;
    for index in (0..7).rev() {
        let sum = data[destination + index] + addend[index] + carry;
        data[destination + index] = sum % 10;
        carry = u8::from(sum >= 10);
    }
}

/// CS:38de..396c. The bonus score is awarded before the difficulty increases.
fn award(data: &mut [u8], tick: u16) -> Result<crate::completion::AwardWork, String> {
    let difficulty = usize::from(word(data, 8));
    if difficulty > 7 {
        return Err("Hearts completion difficulty is outside 0..7".into());
    }
    let bonus = 10800u16.saturating_sub(tick.wrapping_sub(word(data, 0x412))) >> 1;
    set_word(data, 0x3697, bonus);
    set_word(data, 0x368b, bonus);
    data[0x368d..0x3695].fill(0);
    let mut value = bonus;
    for index in (0..7).rev() {
        data[0x368d + index] = (value % 10) as u8;
        value /= 10;
    }
    let base = usize::from(word(data, 0x36da));
    let base: [u8; 7] = data
        .get(base..base + 7)
        .ok_or("Hearts score base exceeds executable data")?
        .try_into()
        .unwrap();
    if base.iter().any(|digit| *digit > 9) || data[0x1f82..0x1f89].iter().any(|digit| *digit > 9) {
        return Err("Hearts score contains an invalid decimal digit".into());
    }
    add_decimal(data, 0x368d, &base);
    let doubled = word(data, 0x2e8d) < 8;
    let multiplier = word(data, 0x36dc + difficulty * 2).wrapping_mul(if doubled { 2 } else { 1 });
    // Supported data contains nonzero multipliers; reject corrupt synthetic data
    // instead of reproducing LOOP's 65536 iterations for a zero CX.
    if multiplier == 0 {
        return Err("Hearts score multiplier is zero".into());
    }
    set_word(
        data,
        0x370c,
        difficulty as u16 * 2 + if doubled { 16 } else { 0 },
    );
    let points: [u8; 7] = data[0x368d..0x3694].try_into().unwrap();
    for _ in 0..multiplier {
        add_decimal(data, 0x1f82, &points);
    }
    set_word(
        data,
        0x3720,
        word(data, 0x36ec + usize::from(word(data, 0x370c))),
    );
    data[0x369e] = 56;
    data[0x3699] = 1;
    set_word(data, 0x3722, 68);
    Ok(crate::completion::AwardWork {
        bonus,
        multiplier: Some(multiplier),
    })
}

fn score_frame(data: &mut [u8], screen: &mut Framebuffer) -> Result<usize, String> {
    let saved = screen.read_rect(0x08e4, Dimensions(0x0804))?;
    data[0x0e..0x4e].copy_from_slice(&saved);
    let saved = screen.read_rect(0x0c94, Dimensions(0x0814))?;
    data[0x4e..0x18e].copy_from_slice(&saved);
    for index in 3..7 {
        screen.copy_rect(
            pixel_offset(144 + (index - 3) * 8, 56),
            Dimensions(0x0801),
            &crate::completion::digit(data[0x368d + usize::from(index)])?,
        )?;
    }
    set_word(data, 0x36a0, 7);
    for index in 0..20 {
        screen.copy_rect(
            pixel_offset(80 + index as u16 * 8, 80),
            Dimensions(0x0801),
            &crate::font::character(data[0x370e + index])?,
        )?;
    }
    Ok(384)
}

/// CS:5060..50a1, excluding drawing and the enclosing timing loop.
fn initialize_approach(data: &mut [u8], x: u16) {
    let x = x.min(278).saturating_sub(16) & 0xfff0;
    set_word(data, 0x579, x);
    data[0x57b] = 20;
    let delta = x.wrapping_sub(128);
    let distance = if x < 128 { !delta } else { delta };
    set_word(data, 0x4d6a, (distance >> 3).min(13) + 2);
    set_word(data, 0x4dd6, 10);
}

/// CS:50b3..50f6. None denotes the return before drawing another image.
fn move_approach(data: &mut [u8]) -> Option<u16> {
    let x = word(data, 0x579);
    let aligned = x & 0xfff0;
    let x = if aligned == 128 {
        aligned
    } else if aligned > 128 {
        x.wrapping_sub(word(data, 0x4d6a))
    } else {
        x.wrapping_add(word(data, 0x4d6a))
    };
    set_word(data, 0x579, x);
    if data[0x57b] >= 84 {
        return None;
    }
    data[0x57b] = data[0x57b].wrapping_add(8);
    let destination = pixel_offset(x.wrapping_add(4), data[0x57b]);
    set_word(data, 0x4dd8, destination);
    Some(destination)
}

fn initialize_pair(data: &mut [u8], group: u8) {
    let offset = usize::from(group) * 2;
    for (source, destination) in [
        (0x4da4, 0x4d6a),
        (0x4daa, 0x4d6c),
        (0x4d98, 0x4dcc),
        (0x4d9e, 0x4dce),
        (0x4db0, 0x4dd0),
        (0x4db6, 0x4dd2),
        (0x4dbc, 0x4dd4),
        (0x4d92, 0x4dca),
        (0x4dc2, 0x4dc8),
    ] {
        set_word(data, destination, word(data, source + offset));
    }
    for index in 0..8 {
        set_word(data, 0x4d4a + index * 2, word(data, 0x4dcc));
        set_word(data, 0x4d5a + index * 2, word(data, 0x4dce));
    }
    data[0x4d91] = 1;
}

/// CS:52a5..52c9; the caller separately starts the black wipe.
fn reward(data: &mut [u8], tick: u16) -> Bookkeeping {
    let life = data[0x1f80] < 9;
    let difficulty = word(data, 8) < 7;
    if life {
        data[0x1f80] += 1;
    }
    if difficulty {
        set_word(data, 8, word(data, 8) + 1);
    }
    set_word(data, 0x414, 0);
    set_word(data, 0x412, tick);
    Bookkeeping::Reward { life, difficulty }
}

/// CS:5313..533c; drawing the first celebration frame is separate work.
fn initialize_dance(data: &mut [u8], tick: u16) -> bool {
    if word(data, 8) < 2 {
        return false;
    }
    set_word(data, 0x5016, 0);
    for field in [0x52c0, 0x52c2, 0x52c4] {
        set_word(data, field, tick);
    }
    set_word(data, 0x52c6, 0);
    set_word(data, 0x52c8, 0);
    true
}

fn pair_frame(data: &mut [u8], screen: &mut Framebuffer, work: &mut Work) -> Result<(), String> {
    data[0x4d6e] = 0;
    for index in (0..8).rev() {
        let offset = index * 2;
        if data[0x4d91] == 0 || index == 7 {
            let destination = pixel_offset(
                word(data, 0x4d4a + offset),
                word(data, 0x4d5a + offset) as u8,
            );
            work.bytes_drawn += transparent_sprite(
                data,
                screen,
                word(data, 0x4dca),
                destination,
                Dimensions(word(data, 0x4dd4)),
            )?;
            let sprite = match word(data, 0x4dd4) {
                0x0802 => Some(PresentationSprite::PairSmall),
                0x1004 => Some(PresentationSprite::PairMedium),
                0x2007 => Some(PresentationSprite::ApproachCat),
                _ => None,
            };
            if let Some(sprite) = sprite {
                work.hearts.sprite(sprite);
            }
        }
        pair_position(data, offset, &mut work.hearts);
    }
    Ok(())
}

/// CS:522a, one pair item; drawing and the enclosing wait are separate.
fn pair_position(data: &mut [u8], offset: usize, work: &mut PresentationWork) {
    work.pair_items += 1;
    for (positions, directions, step, limit) in [
        (0x4d4a, 0x4d6f, 0x4d6a, 0x4dd0),
        (0x4d5a, 0x4d7f, 0x4d6c, 0x4dd2),
    ] {
        let old = word(data, positions + offset);
        let direction = word(data, directions + offset);
        let (next, finished) = if direction == 0 {
            (old, false)
        } else if direction == 1 {
            work.increasing_axes += 1;
            let next = old.wrapping_add(word(data, step));
            work.upper_clamps += u8::from(next > word(data, limit));
            (next.min(word(data, limit)), next > word(data, limit))
        } else {
            work.decreasing_axes += 1;
            let (next, overflow) = old.overflowing_sub(word(data, step));
            work.lower_clamps += u8::from(overflow);
            (if overflow { 0 } else { next }, overflow)
        };
        set_word(data, positions + offset, next);
        if finished {
            data[0x4d6e] = data[0x4d6e].wrapping_add(1);
        }
    }
}

fn dance_frame(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<usize, String> {
    let difficulty = usize::from(word(data, 8));
    let mut list = usize::from(word(data, 0x52ae + difficulty * 2));
    let mut bytes = 0;
    for _ in 0..DATA_LENGTH / 4 {
        let entry = data
            .get(list..list + 4)
            .ok_or("Hearts celebration list exceeds executable data")?;
        let destination = u16::from_le_bytes([entry[0], entry[1]]);
        set_word(data, 0x5010, list as u16);
        if destination == 0 {
            return Ok(bytes);
        }
        let choice = (word(data, list + 2) ^ word(data, 0x5016)) & 2;
        bytes += copy_sprite(
            data,
            screen,
            word(data, 0x5012 + usize::from(choice)),
            destination,
            Dimensions(0x2304),
        )?;
        dance_tone(data, tick, sound)?;
        list += 4;
    }
    Err("Unterminated hearts celebration list".into())
}

fn dance_tone(data: &mut [u8], tick: u16, sound: &mut crate::sound::Sound) -> Result<(), String> {
    if data[0] == 0 || word(data, 0x52c4) == tick {
        return Ok(());
    }
    set_word(data, 0x52c4, tick);
    let offset = 0x52ca + usize::from(word(data, 0x52c6));
    let tone = data
        .get(offset..offset + 2)
        .ok_or("Hearts celebration tone table exceeds executable data")?;
    let tone = u16::from_le_bytes([tone[0], tone[1]]);
    set_word(data, 0x52c6, word(data, 0x52c6).wrapping_add(2));
    if tone == word(data, 0x52c8) {
        sound.stop();
    } else {
        sound.tone(tone, crate::sound::Category::Music);
    }
    set_word(data, 0x52c8, tone);
    Ok(())
}

impl HeartsCompletion {
    pub(crate) fn new(x: u16, y: u8, success: bool) -> Result<Self, String> {
        if x > 296 || y > 198 {
            return Err("Hearts return center is outside the player area".into());
        }
        Ok(Self {
            phase: Phase::Start,
            center: (x, y),
            success,
            background: 0,
            presentation_palette: false,
            alley_palette: false,
        })
    }

    /// BIOS background color, independent of the framebuffer's pixel indices.
    pub(crate) fn background(&self) -> u8 {
        self.background
    }

    /// CS:528b temporarily requests CGA palette one before the black wipe;
    /// PCjr keeps its existing palette until the shared alley palette request.
    pub(crate) fn presentation_palette(&self) -> bool {
        self.presentation_palette
    }

    pub(crate) fn uses_alley_palette(&self) -> bool {
        self.alley_palette
    }

    pub(crate) fn advance(
        &mut self,
        data: &mut [u8],
        screen: &mut Framebuffer,
        tick: u16,
        retrace: bool,
        sound: &mut crate::sound::Sound,
    ) -> Result<Work, String> {
        check_data(data)?;
        let mut work = Work::default();
        match self.phase {
            Phase::Start => {
                if self.success {
                    start_melody(data, tick);
                    self.presentation_palette = true;
                    initialize_approach(data, self.center.0);
                    work.hearts.bookkeeping = Some(Bookkeeping::ApproachInit);
                    self.phase = Phase::Approach;
                } else {
                    self.phase = Phase::Black(Wipe::new(self.center.0, self.center.1, 0));
                }
            }
            Phase::Approach => {
                let destination = move_approach(data);
                work.hearts.bookkeeping = Some(Bookkeeping::Approach {
                    drawing: destination.is_some(),
                });
                if let Some(destination) = destination {
                    work.bytes_drawn +=
                        transparent_sprite(data, screen, 0x4b8a, destination, Dimensions(0x2007))?;
                    work.hearts.sprite(PresentationSprite::ApproachCat);
                    work.bytes_drawn +=
                        copy_sprite(data, screen, 0x4a82, destination + 0xf3, Dimensions(0x0d04))?;
                    work.hearts.sprite(PresentationSprite::ApproachPanel);
                    let first = word(data, 0x4dd6) == 10;
                    if first {
                        let elapsed = sound.hearts_sweep(data);
                        start_melody(data, tick.wrapping_add(elapsed));
                    }
                    set_word(data, 0x4a80, tick);
                    self.phase = Phase::ApproachWait { start: tick, first };
                } else {
                    self.phase = Phase::PairStart(0);
                }
            }
            Phase::ApproachWait { start, first } => {
                melody(data, tick, sound);
                if tick.wrapping_sub(start) >= word(data, 0x4dd6) {
                    if first {
                        self.phase = Phase::Score;
                    } else {
                        self.phase = Phase::Approach;
                    }
                    set_word(data, 0x4dd6, 2);
                }
            }
            Phase::Score => {
                work.bookkeeping.award = Some(award(data, tick)?);
                work.bytes_drawn = score_frame(data, screen)?;
                set_word(data, 0x3695, tick);
                self.phase = Phase::ScoreHold {
                    start: tick,
                    sample: None,
                };
            }
            Phase::ScoreHold { start, sample } => {
                if sample.is_none() {
                    melody(data, tick, sound);
                }
                let sampled = sample.unwrap_or(tick);
                if retrace {
                    self.background = if sampled & 4 == 0 { 1 } else { 0 };
                    if sampled.wrapping_sub(start) >= 68 {
                        self.background = 0;
                        work.bytes_drawn +=
                            copy_sprite(data, screen, 0x0e, 0x08e4, Dimensions(0x0804))?;
                        work.bytes_drawn +=
                            copy_sprite(data, screen, 0x4e, 0x0c94, Dimensions(0x0814))?;
                        self.phase = Phase::Approach;
                    } else {
                        self.phase = Phase::ScoreHold {
                            start,
                            sample: None,
                        };
                    }
                } else {
                    self.phase = Phase::ScoreHold {
                        start,
                        sample: Some(sampled),
                    };
                }
            }
            Phase::PairStart(group) => {
                initialize_pair(data, group);
                work.hearts.bookkeeping = Some(Bookkeeping::PairInit);
                self.phase = Phase::PairFrame(group);
            }
            Phase::PairFrame(group) => {
                melody(data, tick, sound);
                pair_frame(data, screen, &mut work)?;
                set_word(data, 0x4a80, tick);
                self.phase = Phase::PairWait { group, start: tick };
            }
            Phase::PairWait { group, start } => {
                melody(data, tick, sound);
                if tick.wrapping_sub(start) >= word(data, 0x4dc8) {
                    data[0x4d91] = 0;
                    self.phase = if data[0x4d6e] == 0 {
                        Phase::PairFrame(group)
                    } else if group < 2 {
                        Phase::PairStart(group + 1)
                    } else {
                        Phase::MelodyEnd
                    };
                }
            }
            Phase::MelodyEnd => {
                melody(data, tick, sound);
                if data[0] == 0 || word(data, 0x59be) >= 124 {
                    sound.stop();
                    self.phase = Phase::Reward;
                }
            }
            Phase::Reward => {
                work.hearts.bookkeeping = Some(reward(data, tick));
                set_word(data, 0x579, 152);
                data[0x57b] = 95;
                self.center = (152, 95);
                self.phase = Phase::Black(Wipe::new(152, 95, 0));
            }
            Phase::Black(mut wipe) => {
                work = wipe.advance(data, screen, sound)?;
                if work.complete {
                    self.presentation_palette = false;
                    self.alley_palette = true;
                    self.phase = if self.success {
                        Phase::DanceStart
                    } else {
                        Phase::DropStart
                    };
                } else {
                    self.phase = Phase::Black(wipe);
                }
                work.complete = false;
            }
            Phase::DanceStart => {
                let active = initialize_dance(data, tick);
                work.hearts.bookkeeping = Some(Bookkeeping::DanceInit { active });
                if !active {
                    self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, 0xaa));
                } else {
                    work.bytes_drawn = dance_frame(data, screen, tick, sound)?;
                    work.hearts.dance_frames = 1;
                    work.hearts.dance_copies = (work.bytes_drawn / 280) as u8;
                    set_word(data, 0x5016, word(data, 0x5016) ^ 2);
                    self.phase = Phase::Dance {
                        start: tick,
                        frame: tick,
                    };
                }
            }
            Phase::Dance { start, frame } => {
                dance_tone(data, tick, sound)?;
                if tick.wrapping_sub(frame) >= 5 {
                    set_word(data, 0x52c0, tick);
                    if tick.wrapping_sub(start) >= 40 {
                        sound.stop();
                        self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, 0xaa));
                    } else {
                        work.bytes_drawn = dance_frame(data, screen, tick, sound)?;
                        work.hearts.dance_frames = 1;
                        work.hearts.dance_copies = (work.bytes_drawn / 280) as u8;
                        set_word(data, 0x5016, word(data, 0x5016) ^ 2);
                        self.phase = Phase::Dance { start, frame: tick };
                    }
                }
            }
            Phase::DropStart => {
                data[0x0e..0x56].fill(0);
                set_word(data, 0x6f24, 37);
                self.phase = Phase::Drop;
            }
            Phase::Drop => {
                if retrace {
                    let destination = word(data, 0x6f24);
                    work.bytes_drawn +=
                        copy_sprite(data, screen, 0x0e, destination, Dimensions(0x0c03))?;
                    work.hearts.sprite(PresentationSprite::FallingCat);
                    let destination = destination + 480;
                    set_word(data, 0x6f24, destination);
                    work.bytes_drawn +=
                        copy_sprite(data, screen, 0x6e10, destination, Dimensions(0x0c03))?;
                    work.hearts.sprite(PresentationSprite::FallingCat);
                    self.phase = Phase::DropTick;
                }
            }
            Phase::DropTick => {
                if word(data, 0x6f26) != tick {
                    set_word(data, 0x6f26, tick);
                    // CS:608b..60a5: the pitch descends with the falling sprite.
                    if data[0] != 0 {
                        sound.tone(word(data, 0x6f24) >> 1, crate::sound::Category::Effects);
                    }
                    if word(data, 0x6f24) < 0x1a40 {
                        self.phase = Phase::Drop;
                    } else {
                        work.bytes_drawn += copy_sprite(
                            data,
                            screen,
                            0x6e58,
                            word(data, 0x6f24),
                            Dimensions(0x1106),
                        )?;
                        work.hearts.sprite(PresentationSprite::LandingCat);
                        self.phase = Phase::DropHold;
                    }
                }
            }
            Phase::DropHold => {
                if word(data, 0x6f28) != tick {
                    set_word(data, 0x6f28, tick);
                    // CS:60ca..60e4 keeps the speaker gate from the last drop
                    // note, alternating divisors until the final tick's stop.
                    if data[0] != 0 {
                        sound.retune(
                            if tick & 1 == 0 { 0x0c00 } else { 0x0b54 },
                            crate::sound::Category::Effects,
                        );
                    }
                    if tick.wrapping_sub(word(data, 0x6f26)) >= 18 {
                        sound.stop();
                        self.phase = Phase::Fill(Wipe::new(self.center.0, self.center.1, 0xaa));
                    }
                }
            }
            Phase::Fill(mut wipe) => {
                work = wipe.advance(data, screen, sound)?;
                self.phase = if work.complete {
                    Phase::Complete
                } else {
                    Phase::Fill(wipe)
                };
            }
            Phase::Complete => work.complete = true,
        }
        Ok(work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        data[0x370e..0x3722].copy_from_slice(b"BONUS MULTIPLIER: 01");
        set_word(&mut data, 0x36da, 0x6100);
        data[0x6100..0x6107].copy_from_slice(&[0, 0, 0, 1, 0, 0, 0]);
        for index in 0..16 {
            set_word(&mut data, 0x36ec + index * 2, 0x3130);
        }
        for difficulty in 0..8 {
            set_word(
                &mut data,
                0x36dc + difficulty * 2,
                difficulty as u16 * 2 + 1,
            );
        }
        data[0x1f80] = 3;
        data
    }

    #[test]
    fn approach_and_pair_setup_report_work_only_in_the_executing_phase() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        let mut sound = crate::sound::Sound::default();
        let mut c = HeartsCompletion::new(296, 100, true).unwrap();
        let start = c
            .advance(&mut data, &mut screen, 100, true, &mut sound)
            .unwrap();
        assert!(matches!(
            start.hearts.bookkeeping,
            Some(Bookkeeping::ApproachInit)
        ));
        let moved = c
            .advance(&mut data, &mut screen, 100, true, &mut sound)
            .unwrap();
        assert!(matches!(
            moved.hearts.bookkeeping,
            Some(Bookkeeping::Approach { drawing: true })
        ));
        assert_eq!(moved.bytes_drawn, 552);
        let waiting = c
            .advance(&mut data, &mut screen, 100, true, &mut sound)
            .unwrap();
        assert!(waiting.hearts.bookkeeping.is_none());
        c.phase = Phase::PairStart(0);
        let pair = c
            .advance(&mut data, &mut screen, 100, true, &mut sound)
            .unwrap();
        assert!(matches!(
            pair.hearts.bookkeeping,
            Some(Bookkeeping::PairInit)
        ));
        assert_eq!(pair.bytes_drawn, 0);
    }

    #[test]
    fn celebration_starts_on_the_completed_black_wipe_and_keeps_its_tick_wait() {
        let mut data = data();
        set_word(&mut data, 8, 2);
        set_word(&mut data, 0x52b2, 0x6000);
        let destination = pixel_offset(80, 60);
        set_word(&mut data, 0x6000, destination);
        set_word(&mut data, 0x5012, 0x6200);
        data[0x6200..0x6318].fill(0xff);
        let mut screen = Framebuffer::from_bytes(&[0xaa; 0x4000]).unwrap();
        let mut sound = crate::sound::Sound::default();
        let mut presentation = HeartsCompletion::new(152, 95, true).unwrap();
        presentation.phase = Phase::Black(Wipe::new(152, 95, 0));
        for _ in 0..32 {
            presentation
                .advance(&mut data, &mut screen, 100, true, &mut sound)
                .unwrap();
            if matches!(presentation.phase, Phase::DanceStart) {
                break;
            }
        }
        assert!(matches!(presentation.phase, Phase::DanceStart));
        assert!(screen.bytes()[..8000].iter().all(|byte| *byte == 0));
        assert!(screen.bytes()[8192..16192].iter().all(|byte| *byte == 0));
        let frame = presentation
            .advance(&mut data, &mut screen, 100, true, &mut sound)
            .unwrap();
        assert_eq!(frame.bytes_drawn, 280);
        assert_eq!(frame.hearts.bytes(), 280);
        assert_eq!(frame.hearts.dance_frames, 1);
        assert_eq!(frame.hearts.dance_copies, 1);
        assert!(matches!(
            frame.hearts.bookkeeping,
            Some(Bookkeeping::DanceInit { active: true })
        ));
        assert_eq!(word(&data, 0x5010), 0x6004);
        assert_eq!(data[0x2ae0], 4);
        let wait = presentation
            .advance(&mut data, &mut screen, 104, true, &mut sound)
            .unwrap();
        assert_eq!(wait.bytes_drawn, 0);
        assert_eq!(wait.hearts.dance_frames, 0);
        assert!(wait.hearts.bookkeeping.is_none());
        assert!(matches!(
            presentation.phase,
            Phase::Dance { frame: 100, .. }
        ));
    }

    #[test]
    fn pairing_counts_only_the_images_drawn_on_each_native_frame() {
        let mut data = data();
        set_word(&mut data, 0x4dca, 0x6100);
        set_word(&mut data, 0x4dd4, 0x0802);
        set_word(&mut data, 0x4dc8, 2);
        data[0x4d91] = 1;
        let mut screen = Framebuffer::default();
        let mut sound = crate::sound::Sound::default();
        let mut presentation = HeartsCompletion::new(152, 95, true).unwrap();
        presentation.phase = Phase::PairFrame(0);
        let first = presentation
            .advance(&mut data, &mut screen, 100, true, &mut sound)
            .unwrap();
        assert_eq!(first.bytes_drawn, 32);
        assert_eq!(first.hearts.bytes(), 32);
        assert_eq!(first.hearts.pair_items, 8);
        for tick in [101, 102] {
            let wait = presentation
                .advance(&mut data, &mut screen, tick, true, &mut sound)
                .unwrap();
            assert_eq!(wait.hearts.bytes(), 0);
            assert_eq!(wait.hearts.pair_items, 0);
        }
        let next = presentation
            .advance(&mut data, &mut screen, 103, true, &mut sound)
            .unwrap();
        assert_eq!(next.bytes_drawn, 256);
        assert_eq!(next.hearts.bytes(), 256);
        assert_eq!(next.hearts.pair_items, 8);
    }

    #[test]
    fn failed_hearts_drop_plays_descent_and_landing_hold_once_per_tick() {
        for native_sound in [0, 255] {
            let mut data = data();
            data[0] = native_sound;
            let mut screen = Framebuffer::default();
            let mut sound = crate::sound::Sound::default();
            sound.record(true);
            let mut completion = HeartsCompletion::new(100, 198, false).unwrap();
            completion.phase = Phase::DropStart;
            completion
                .advance(&mut data, &mut screen, 100, true, &mut sound)
                .unwrap();
            let mut tick = 100;
            let mut pitches = Vec::new();
            while !matches!(completion.phase, Phase::DropHold) {
                completion
                    .advance(&mut data, &mut screen, tick, true, &mut sound)
                    .unwrap();
                for write in sound.drain() {
                    if write.port == 0x42 {
                        pitches.push(write.value);
                    }
                }
                tick += 1;
                assert!(tick < 200);
            }
            let expected: Vec<_> = if native_sound == 0 {
                Vec::new()
            } else {
                (1..=14u16)
                    .flat_map(|step| ((37 + 480 * step) >> 1).to_le_bytes())
                    .collect()
            };
            assert_eq!(pitches, expected);
            let landed = word(&data, 0x6f26);
            // The last retune still occurs on the terminating tick, then stop.
            for tick in landed..=landed + 18 {
                completion
                    .advance(&mut data, &mut screen, tick, true, &mut sound)
                    .unwrap();
                let writes: Vec<_> = sound.drain().collect();
                let divisor: u16 = if tick & 1 == 0 { 0x0c00 } else { 0x0b54 };
                let mut expected = Vec::new();
                if native_sound != 0 {
                    expected.extend([
                        (0x43, 0xb6),
                        (0x42, divisor as u8),
                        (0x42, (divisor >> 8) as u8),
                    ]);
                }
                if tick == landed + 18 {
                    expected.push((0x61, 0));
                }
                assert_eq!(
                    writes.iter().map(|w| (w.port, w.value)).collect::<Vec<_>>(),
                    expected
                );
                assert!(
                    writes
                        .iter()
                        .all(|w| w.category == crate::sound::Category::Effects)
                );
                if tick != landed + 18 {
                    completion
                        .advance(&mut data, &mut screen, tick, true, &mut sound)
                        .unwrap();
                    assert_eq!(sound.drain().count(), 0);
                }
            }
            assert!(matches!(completion.phase, Phase::Fill(_)));
            assert_eq!(data[0x1f80], 3);
        }
    }

    #[test]
    fn score_uses_wrapping_elapsed_old_difficulty_and_carried_gift_multiplier() {
        let mut data = data();
        set_word(&mut data, 0x412, u16::MAX - 9);
        set_word(&mut data, 8, 2);
        set_word(&mut data, 0x2e8d, 0xffff);
        award(&mut data, 10).unwrap();
        assert_eq!(word(&data, 0x3697), 5390);
        assert_eq!(&data[0x368d..0x3694], &[0, 0, 0, 6, 3, 9, 0]);
        assert_eq!(&data[0x1f82..0x1f89], &[0, 0, 3, 1, 9, 5, 0]);
        assert_eq!(word(&data, 0x370c), 4);
        data[0x1f82..0x1f89].fill(0);
        set_word(&mut data, 0x2e8d, 7);
        award(&mut data, 10).unwrap();
        assert_eq!(&data[0x1f82..0x1f89], &[0, 0, 6, 3, 9, 0, 0]);
        assert_eq!(word(&data, 0x370c), 20);
    }

    #[test]
    fn reward_is_once_capped_and_resets_completed_rooms() {
        for (lives, difficulty, expected_lives, expected_difficulty) in [(3, 2, 4, 3), (9, 7, 9, 7)]
        {
            let mut data = data();
            data[0x1f80] = lives;
            set_word(&mut data, 8, difficulty);
            set_word(&mut data, 0x414, 9);
            let mut completion = HeartsCompletion::new(100, 20, true).unwrap();
            completion.phase = Phase::Reward;
            let mut screen = Framebuffer::default();
            let reward = completion
                .advance(
                    &mut data,
                    &mut screen,
                    42,
                    true,
                    &mut crate::sound::Sound::default(),
                )
                .unwrap();
            assert!(matches!(
                reward.hearts.bookkeeping,
                Some(Bookkeeping::Reward { life, difficulty: increased })
                    if life == (lives < 9) && increased == (difficulty < 7)
            ));
            let next = completion
                .advance(
                    &mut data,
                    &mut screen,
                    42,
                    true,
                    &mut crate::sound::Sound::default(),
                )
                .unwrap();
            assert!(next.hearts.bookkeeping.is_none());
            assert_eq!(
                (data[0x1f80], word(&data, 8)),
                (expected_lives, expected_difficulty)
            );
            assert_eq!((word(&data, 0x414), word(&data, 0x412)), (0, 42));
        }
    }

    #[test]
    fn success_progresses_through_score_reward_and_return_without_repeating_award() {
        let mut data = data();
        set_word(&mut data, 8, 1);
        set_word(&mut data, 0x2e8d, 0xffff);
        set_word(&mut data, 0x2ae5, 0xfa59);
        // Synthetic pair tables terminate on the first X step. No original
        // graphics or native table bytes are embedded in the test.
        set_word(&mut data, 0x4d6f + 14, 1);
        for group in 0..3 {
            set_word(&mut data, 0x4da4 + group * 2, 1);
            set_word(&mut data, 0x4dbc + group * 2, 0x0101);
            set_word(&mut data, 0x4d92 + group * 2, 0x6100);
            set_word(&mut data, 0x4dc2 + group * 2, 1);
        }
        let mut screen = Framebuffer::default();
        let mut completion = HeartsCompletion::new(100, 20, true).unwrap();
        let mut done = false;
        let mut awards = 0;
        for tick in 0..300 {
            let work = completion
                .advance(
                    &mut data,
                    &mut screen,
                    tick,
                    true,
                    &mut crate::sound::Sound::default(),
                )
                .unwrap();
            if let Some(award) = work.bookkeeping.award {
                assert_eq!(award.multiplier, Some(3));
                awards += 1;
            }
            if work.complete {
                done = true;
                break;
            }
        }
        assert!(done);
        assert_eq!(awards, 1);
        assert_eq!(
            (data[0x1f80], word(&data, 8), word(&data, 0x414)),
            (4, 2, 0)
        );
        assert!(data[0x1f82..0x1f89].iter().any(|digit| *digit != 0));
        assert_eq!(word(&data, 0x2ae5), 0xfa59);
        assert!(screen.indices().iter().all(|pixel| *pixel == 2));
        let score = data[0x1f82..0x1f89].to_vec();
        completion
            .advance(
                &mut data,
                &mut screen,
                301,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert_eq!(data[0x1f82..0x1f89], score);
        assert!(!completion.presentation_palette());
    }

    #[test]
    fn neutral_return_preserves_score_lives_difficulty_and_random_state() {
        let mut data = data();
        data[0x1f82..0x1f89].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7]);
        set_word(&mut data, 8, 3);
        set_word(&mut data, 0x2ae5, 0xfa59);
        let mut screen = Framebuffer::default();
        let mut completion = HeartsCompletion::new(80, 180, false).unwrap();
        let mut done = false;
        for tick in 0..150 {
            if completion
                .advance(
                    &mut data,
                    &mut screen,
                    tick,
                    true,
                    &mut crate::sound::Sound::default(),
                )
                .unwrap()
                .complete
            {
                done = true;
                break;
            }
        }
        assert!(done);
        assert_eq!(&data[0x1f82..0x1f89], &[1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            (data[0x1f80], word(&data, 8), word(&data, 0x2ae5)),
            (3, 3, 0xfa59)
        );
        assert!(screen.indices().iter().all(|pixel| *pixel == 2));
    }

    #[test]
    fn neutral_drop_obeys_retrace_and_distinct_wrapping_ticks() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        let mut completion = HeartsCompletion::new(80, 180, false).unwrap();
        completion.phase = Phase::DropStart;
        completion
            .advance(
                &mut data,
                &mut screen,
                u16::MAX,
                false,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert_eq!(
            completion
                .advance(
                    &mut data,
                    &mut screen,
                    u16::MAX,
                    false,
                    &mut crate::sound::Sound::default()
                )
                .unwrap()
                .bytes_drawn,
            0
        );
        set_word(&mut data, 0x6f26, u16::MAX);
        assert_eq!(
            completion
                .advance(
                    &mut data,
                    &mut screen,
                    u16::MAX,
                    true,
                    &mut crate::sound::Sound::default()
                )
                .unwrap()
                .bytes_drawn,
            144
        );
        completion
            .advance(
                &mut data,
                &mut screen,
                u16::MAX,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert!(matches!(completion.phase, Phase::DropTick));
        completion
            .advance(
                &mut data,
                &mut screen,
                0,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert!(matches!(completion.phase, Phase::Drop));
    }

    #[test]
    fn score_hold_preserves_tick_sample_across_retrace_wait() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        let mut completion = HeartsCompletion::new(80, 180, true).unwrap();
        completion.phase = Phase::ScoreHold {
            start: 0,
            sample: None,
        };
        completion
            .advance(
                &mut data,
                &mut screen,
                67,
                false,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        completion
            .advance(
                &mut data,
                &mut screen,
                68,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert!(matches!(completion.phase, Phase::ScoreHold { .. }));
        assert_eq!(completion.background(), 1);
        completion
            .advance(
                &mut data,
                &mut screen,
                68,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
        assert!(matches!(completion.phase, Phase::Approach));
        assert_eq!(completion.background(), 0);
    }
}
