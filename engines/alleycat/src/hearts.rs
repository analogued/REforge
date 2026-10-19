// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Scene 7 heart platforms, gift boxes, seven cats and nonlethal arrows.
//!
//! Actors and clocks remain in the executable's mutable data segment. The
//! caller removes the player image before drawing here, then imports changed
//! player fields and recomposes its existing image without selecting a frame.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer, pixel_offset};
use crate::player::Player;
use crate::room::{SupportCheck, TableSupport, draw_list, initialize_broom};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HeartStep {
    pub accepted: bool,
    pub drew: bool,
    pub player_changed: bool,
    pub success: bool,
    pub item_changed: bool,
    pub grid_changed: bool,
    pub work: Option<Work>,
}

/// Native call paths, separate from host compositor work and execution costs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Work {
    CatGate,
    CatHidden,
    CatArrow {
        moved: bool,
    },
    CatContact {
        goal: bool,
        restored: bool,
        moved: bool,
    },
    CatDraw {
        moving: bool,
        recoil: bool,
        pursued: bool,
        respawned: bool,
        restored: bool,
        active_gifts: u8,
        consumed: bool,
        arrow: bool,
        goal: bool,
    },
    ArrowGate,
    ArrowContact {
        moved: bool,
    },
    ArrowRetire {
        bottom: bool,
    },
    ArrowDraw {
        spawned: bool,
        toggle: bool,
    },
    PickupGate,
    Pickup {
        held: bool,
        slots: u8,
        active: u8,
        hit: bool,
        latched: bool,
        arrow: bool,
    },
    PlaceGate {
        holding: bool,
    },
    Place {
        blocked: bool,
        active: u8,
        arrow: bool,
        consumed: bool,
    },
}

fn check(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable hearts data segment is truncated".into())
    } else {
        Ok(())
    }
}

fn overlap(a: (u16, u8, u16, u8), b: (u16, u8, u16, u8)) -> bool {
    a.0.wrapping_add(a.2) >= b.0
        && a.0.saturating_sub(b.2) <= b.0
        && a.1.wrapping_add(a.3) >= b.1
        && a.1.saturating_sub(b.3) <= b.1
}

fn player_rect(data: &[u8]) -> (u16, u8, u16, u8) {
    (word(data, 0x579), data[0x57b], 24, 14)
}

fn cat_rect(data: &[u8]) -> (u16, u8, u16, u8) {
    (word(data, 0x4548), data[0x454b], 24, 12)
}

fn item_rect(data: &[u8], index: usize) -> (u16, u8, u16, u8) {
    (word(data, 0x2b5a + index * 2), data[0x2b6a + index], 24, 15)
}

fn arrow_rect(data: &[u8]) -> (u16, u8, u16, u8) {
    (word(data, 0x70f3), data[0x70f5], 16, 8)
}

fn copy_sprite(
    data: &mut [u8],
    screen: &mut Framebuffer,
    source: u16,
    offset: u16,
    dimensions: u16,
) -> Result<(), String> {
    let dimensions = Dimensions(dimensions);
    screen.copy_rect(offset, dimensions, sprite_bytes(data, source, dimensions)?)?;
    data[0x2ae0] = dimensions.0 as u8;
    data[0x2ae2] = 0;
    Ok(())
}

fn draw_tile(
    data: &mut [u8],
    screen: &mut Framebuffer,
    x: u16,
    y: u8,
    state: u8,
) -> Result<(), String> {
    if ![0, 2].contains(&state) {
        return Err("Heart platform has an invalid sprite state".into());
    }
    copy_sprite(
        data,
        screen,
        word(data, 0x2e20 + usize::from(state)),
        pixel_offset(x, y),
        0x0802,
    )
}

fn draw_item(
    data: &mut [u8],
    screen: &mut Framebuffer,
    index: usize,
    source: u16,
) -> Result<(), String> {
    let (x, y, _, _) = item_rect(data, index);
    copy_sprite(data, screen, source, pixel_offset(x, y), 0x0f03)
}

fn record_address(data: &[u8], index: usize) -> Result<usize, String> {
    if index >= 7 {
        return Err("Hearts cat index exceeds seven records".into());
    }
    let address = usize::from(word(data, 0x45a8 + index * 2));
    if address + 12 > data.len() {
        return Err("Hearts cat record exceeds executable data".into());
    }
    Ok(address)
}

fn load_cat(data: &mut [u8], index: usize) -> Result<(), String> {
    let address = record_address(data, index)?;
    data.copy_within(address..address + 12, 0x4548);
    Ok(())
}

fn save_cat(data: &mut [u8], index: usize) -> Result<(), String> {
    let address = record_address(data, index)?;
    data.copy_within(0x4548..0x4554, address);
    Ok(())
}

/// CS:300f, 3405, 6100 and 4f59. The dormant room broom consumes a random
/// value between platform and cat initialization even though it never moves.
pub(crate) fn initialize(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<(), String> {
    check(data)?;
    screen.fill_visible(0x55);
    draw_list(data, screen, 0x2e24, 0)?;
    for row in 0..7 {
        let y = 191 - row as u8 * 24;
        data[0x2e8a] = y;
        set_word(data, 0x2e8b, row as u16);
        for column in 0..16 {
            let x = 32 + column as u16 * 16;
            set_word(data, 0x2e88, x);
            let state = if row == 0 {
                0
            } else {
                next_random(data) as u8 & 2
            };
            draw_tile(data, screen, x, y, state)?;
            let index = usize::from(data[0x2bdb + row]) + column;
            data[0x2be2 + index] = state;
        }
        set_word(data, 0x2e88, 288);
    }
    set_word(data, 0x2e8b, 7);
    data[0x2e8a] = 23;
    set_word(data, 0x2e8d, 0xffff);
    set_word(data, 0x2e94, 0xffff);
    data[0x2b72..0x2b7a].fill(0);
    if word(data, 0x414) == 0 {
        set_word(data, 0x414, 1);
    }
    let count = usize::from(word(data, 0x414).min(8));
    for index in (0..count).rev() {
        data[0x2b72 + index] = 1;
        data[0x2b6a + index] = 176;
        let x = word(data, 0x2b4a + index * 2);
        set_word(data, 0x2b5a + index * 2, x);
        draw_item(data, screen, index, 0x2af0)?;
    }
    initialize_broom(data)?;
    data[0x70f2] = 0;
    for index in 0..7 {
        set_word(data, 0x45b6, index as u16);
        let x = (next_random(data) & 127) + 96;
        set_word(data, 0x4548, x);
        data[0x454a] = 0;
        data[0x454b] = data[0x2bd4 + index].wrapping_add(3);
        data[0x454e] = 1;
        set_word(data, 0x4551, 0);
        data[0x4553] = 0;
        set_word(
            data,
            0x454f,
            if index == 0 { nonzero_tick(tick) } else { 0 },
        );
        save_cat(data, index)?;
    }
    set_word(data, 0x45b6, 0);
    Ok(())
}

/// CS:30fa includes two undrawn edge cells in each 18-cell row. Preserve the
/// original shifted X+7 lookup and row+5 landing, independent of tile outlines.
pub(crate) fn resolve_support(data: &[u8], player: &Player) -> Result<SupportCheck, String> {
    check(data)?;
    let y = player.y.wrapping_sub(5) & 0xf8;
    for row in (0..7).rev() {
        if y != data[0x2bd4 + row] {
            continue;
        }
        let column = (player.x.wrapping_add(7) >> 4).saturating_sub(2).min(17);
        let index = usize::from(data[0x2bdb + row]) + usize::from(column);
        return Ok(SupportCheck {
            hit: (data[0x2be2 + index] == 0).then_some(TableSupport {
                x: player.x,
                y: y.wrapping_add(5),
                state: 0,
                index,
            }),
            exit: false,
        });
    }
    Ok(SupportCheck::default())
}

fn nonzero_tick(tick: u16) -> u16 {
    if tick == 0 { 0xffff } else { tick }
}

fn restore_cat(data: &[u8], screen: &mut Framebuffer) -> Result<bool, String> {
    if data[0x454e] != 0 {
        return Ok(false);
    }
    screen.copy_rect(word(data, 0x454c), Dimensions(0x0c03), &[0x55; 72])?;
    Ok(true)
}

fn draw_cat(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    let source = if data[0x454a] == 0 {
        0x4500
    } else {
        let mut phase = word(data, 0x4551);
        if data[0x4553] != 0 {
            phase = (phase & 2) + 12;
        }
        if data[0x454a] == 0xff {
            phase += 16;
        }
        word(data, 0x4a60 + usize::from(phase))
    };
    let dimensions = Dimensions(0x0c03);
    let offset = word(data, 0x45ba);
    let previous = screen.read_rect(offset, dimensions)?;
    let pixels = sprite_bytes(data, source, dimensions)?;
    let packed: Vec<u8> = pixels.iter().zip(previous).map(|(a, b)| a | b).collect();
    screen.copy_rect(offset, dimensions, &packed)?;
    set_word(data, 0x454c, offset);
    data[0x454e] = 0;
    Ok(())
}

/// CS:4dd0 contact leaves the existing support marker and vertical phase alone.
pub(crate) fn cat_contact(data: &mut [u8], tick: u16, sound: &mut crate::sound::Sound) -> bool {
    if !overlap(player_rect(data), cat_rect(data)) {
        return false;
    }
    if word(data, 0x45b6) == 6 {
        data[0x553] = 1;
    } else {
        data[0x55b] = 4;
        data[0x571] = 1;
        data[0x576] = 4;
        data[0x578] = 8;
        data[0x4553] = 4;
        data[0x454a] = if word(data, 0x4548) > word(data, 0x579) {
            1
        } else {
            0xff
        };
        sound.request(data, tick, 0x0ce4, 0x123b);
    }
    true
}

fn cat_arrow_contact(data: &[u8]) -> bool {
    data[0x70f2] != 0 && overlap(arrow_rect(data), cat_rect(data))
}

/// CS:4e75 consumes one item, scanning the active slots from seven down to zero.
pub(crate) fn consume_item(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<bool, String> {
    Ok(consume_item_counted(data, screen, tick)?.0)
}

fn consume_item_counted(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<(bool, u8), String> {
    let mut active = 0;
    for index in (0..8).rev() {
        if data[0x2b72 + index] == 0 {
            continue;
        }
        active += 1;
        if !overlap(item_rect(data, index), cat_rect(data)) {
            continue;
        }
        if data[0x45be] == 0 && data[0x70f2] != 0 {
            restore_arrow(data, screen)?;
        }
        restore_cat(data, screen)?;
        data[0x2b72 + index] = 0;
        draw_item(data, screen, index, 0x2b7a)?;
        if data[0x45be] == 0 && data[0x70f2] != 0 {
            draw_arrow(data, screen)?;
        }
        let hidden = if word(data, 0x45b6) == 6 {
            0
        } else {
            nonzero_tick(tick)
        };
        set_word(data, 0x454f, hidden);
        return Ok((true, active));
    }
    Ok((false, active))
}

/// CS:4c10 commits its gate baseline at indices 1, 4 and 0; the other cats
/// can advance within the same tick. Hidden-cat returns do not save scratch.
pub(crate) fn advance_cat(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<HeartStep, String> {
    check(data)?;
    let mut result = HeartStep::default();
    if tick == word(data, 0x45b8) {
        result.work = Some(Work::CatGate);
        return Ok(result);
    }
    let mut index = word(data, 0x45b6).wrapping_add(1);
    if index >= 7 {
        index = 0;
    }
    set_word(data, 0x45b6, index);
    if [0, 1, 4].contains(&index) {
        set_word(data, 0x45b8, tick);
    }
    result.accepted = true;
    load_cat(data, usize::from(index))?;
    if cat_arrow_contact(data) {
        result.work = Some(Work::CatArrow { moved: false });
        return Ok(result);
    }
    let hidden = word(data, 0x454f);
    let respawned = hidden != 0;
    if hidden != 0 {
        let difficulty = usize::from(word(data, 8));
        if difficulty > 7 {
            return Err("Hearts difficulty exceeds seven".into());
        }
        let mut duration = word(data, 0x45c7 + difficulty * 2);
        if index == 0 {
            duration = duration.wrapping_mul(2);
        }
        if tick.wrapping_sub(hidden) < duration {
            result.work = Some(Work::CatHidden);
            return Ok(result);
        }
        set_word(data, 0x454f, 0);
        data[0x454e] = 1;
        set_word(data, 0x4548, if word(data, 0x579) > 160 { 36 } else { 264 });
        data[0x454a] = 0;
    }
    let restored = data[0x454e] == 0;
    if cat_contact(data, tick, sound) {
        result.work = Some(Work::CatContact {
            goal: index == 6,
            restored,
            moved: false,
        });
        result.player_changed = true;
        result.success = data[0x553] != 0;
        result.drew = restore_cat(data, screen)?;
        save_cat(data, usize::from(index))?;
        return Ok(result);
    }
    let mut pursued = false;
    if data[0x4553] != 0 {
        data[0x4553] -= 1;
        if data[0x4553] == 0 {
            data[0x454a] = if data[0x454a] == 0xff { 1 } else { 0xff };
        }
    } else if data[0x454b] <= data[0x57b] {
        let pursue = if index == 6 && data[0x57b] < 40 {
            true
        } else {
            let random = next_random(data) as u8;
            let difficulty = usize::from(word(data, 8));
            if difficulty > 7 {
                return Err("Hearts difficulty exceeds seven".into());
            }
            random <= data[0x45bf + difficulty]
        };
        if pursue {
            pursued = true;
            let x = word(data, 0x4548) & 0x0ff8;
            let target = word(data, 0x579) & 0x0ff8;
            let mut direction = if x == target {
                0
            } else if x < target {
                1
            } else {
                0xff
            };
            if data[0x57b] >= 40 && index == 6 {
                direction = if direction == 0xff { 1 } else { 0xff };
            }
            data[0x454a] = direction;
        }
    }
    let step = if data[0x4553] != 0 { 4 } else { 8 };
    set_word(data, 0x45bc, step);
    let moving = data[0x454a] != 0;
    let recoil = data[0x4553] != 0;
    let old_x = word(data, 0x4548);
    if data[0x454a] == 0 {
        let random = next_random(data) as u8;
        if random <= 16 {
            data[0x454a] = if random & 1 != 0 { 1 } else { 0xff };
        }
    } else {
        let x = if data[0x454a] == 1 {
            let x = old_x.wrapping_add(step);
            if x >= 267 {
                data[0x454a] = 0xff;
                data[0x4553] = 0;
                266
            } else {
                x
            }
        } else {
            let (x, borrow) = old_x.overflowing_sub(step);
            if borrow || x <= 36 {
                data[0x454a] = 1;
                data[0x4553] = 0;
                37
            } else {
                x
            }
        };
        set_word(data, 0x4548, x);
        let phase = word(data, 0x4551).wrapping_add(2);
        set_word(data, 0x4551, if phase >= 12 { 0 } else { phase });
        if data[0x4553] == 0 && next_random(data) as u8 <= 8 {
            data[0x454a] = 0;
        }
    }
    set_word(data, 0x45ba, pixel_offset(word(data, 0x4548), data[0x454b]));
    if cat_arrow_contact(data) {
        result.work = Some(Work::CatArrow { moved: true });
        return Ok(result);
    }
    if cat_contact(data, tick, sound) {
        result.work = Some(Work::CatContact {
            goal: index == 6,
            restored,
            moved: true,
        });
        result.player_changed = true;
        result.success = data[0x553] != 0;
        result.drew = restore_cat(data, screen)?;
    } else {
        restore_cat(data, screen)?;
        draw_cat(data, screen)?;
        data[0x45be] = 0;
        let arrow = data[0x70f2] != 0;
        let (consumed, active_gifts) = consume_item_counted(data, screen, tick)?;
        result.item_changed = consumed;
        result.work = Some(Work::CatDraw {
            moving,
            recoil,
            pursued,
            respawned,
            restored,
            active_gifts,
            consumed: result.item_changed,
            arrow,
            goal: index == 6,
        });
        result.drew = true;
    }
    save_cat(data, usize::from(index))?;
    Ok(result)
}

fn restore_arrow(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    if data[0x70f7] == 0 {
        copy_sprite(data, screen, 0x70cc, word(data, 0x70f8), 0x0802)?;
    }
    Ok(())
}

/// Development-only retirement removes an active image and its interaction
/// flag. An inactive arrow's old buffer may already have been restored.
#[cfg(feature = "dev-tools")]
pub(crate) fn remove_arrow(data: &mut [u8], screen: &mut Framebuffer) -> Result<bool, String> {
    check(data)?;
    let drew = data[0x70f2] != 0 && data[0x70f7] == 0;
    if data[0x70f2] != 0 {
        restore_arrow(data, screen)?;
    }
    data[0x70f2] = 0;
    data[0x70f7] = 1;
    Ok(drew)
}

fn draw_arrow(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    let mut source = (word(data, 0x70f0) & 0x01e0).wrapping_add(0x6f30);
    if data[0x70f6] != 0xff {
        source += 0xc0;
    }
    let offset = word(data, 0x70fa);
    let dimensions = Dimensions(0x0802);
    let previous = screen.zero_transparent_rect(
        offset,
        dimensions,
        sprite_bytes(data, source, dimensions)?,
    )?;
    data[0x70cc..0x70ec].copy_from_slice(&previous);
    set_word(data, 0x70f8, offset);
    data[0x70f7] = 0;
    data[0x2ae0] = 2;
    data[0x2ae2] = 0;
    let last_sprite_word = word(data, usize::from(source) + dimensions.len() - 2);
    set_word(data, 0x2ae3, last_sprite_word);
    Ok(())
}

/// CS:62a6 does not retire the arrow; CS:6106 owns its active flag.
pub(crate) fn arrow_contact(data: &mut [u8], tick: u16, sound: &mut crate::sound::Sound) -> bool {
    if data[0x70f2] == 0 || !overlap(arrow_rect(data), player_rect(data)) {
        return false;
    }
    data[0x571] = 1;
    data[0x576] = 2;
    data[0x578] = 32;
    data[0x55b] = 8;
    sound.request(data, tick, 0x091d, 0x0ce4);
    true
}

/// CS:6245's cell latch prevents a second toggle while crossing the same tile.
pub(crate) fn toggle_platform(data: &mut [u8], screen: &mut Framebuffer) -> Result<bool, String> {
    let y = data[0x70f5].wrapping_sub(8) & 0xf8;
    for row in (0..7).rev() {
        if y != data[0x2bd4 + row] {
            continue;
        }
        let Some(column) = (word(data, 0x70f3) >> 4).checked_sub(2) else {
            return Ok(false);
        };
        if column >= 16 {
            return Ok(false);
        }
        let index = usize::from(data[0x2bdb + row]) + usize::from(column);
        if word(data, 0x70ec) == index as u16 {
            return Ok(false);
        }
        set_word(data, 0x70ec, index as u16);
        data[0x2be2 + index] ^= 2;
        restore_arrow(data, screen)?;
        draw_tile(
            data,
            screen,
            word(data, 0x70fc + usize::from(column) * 2),
            data[0x7120 + row],
            data[0x2be2 + index],
        )?;
        draw_arrow(data, screen)?;
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn advance_arrow(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<HeartStep, String> {
    check(data)?;
    let mut result = HeartStep::default();
    if tick == word(data, 0x70ee) {
        result.work = Some(Work::ArrowGate);
        return Ok(result);
    }
    set_word(data, 0x70ee, tick);
    result.accepted = true;
    if arrow_contact(data, tick, sound) {
        restore_arrow(data, screen)?;
        data[0x70f2] = 0;
        result.work = Some(Work::ArrowContact { moved: false });
        result.player_changed = true;
        result.drew = true;
        return Ok(result);
    }
    let spawned = data[0x70f2] == 0;
    if spawned {
        let choice = loop {
            let choice = next_random(data) & 31;
            if choice <= 25 {
                break choice;
            }
        };
        if choice >= 16 {
            let choice = usize::from(choice - 16);
            data[0x70f6] = if choice < 5 { 1 } else { 0xff };
            data[0x70f5] = 6;
            let x = word(data, 0x70b8 + choice * 2).wrapping_add(4);
            set_word(data, 0x70f3, x);
        } else {
            set_word(data, 0x70f3, if choice & 8 == 0 { 12 } else { 288 });
            data[0x70f6] = if choice & 8 == 0 { 1 } else { 0xff };
            data[0x70f5] = data[0x70b0 + usize::from(choice & 7)].wrapping_add(8);
        }
        data[0x70f2] = 1;
        data[0x70f7] = 1;
        set_word(data, 0x70f0, 0);
        set_word(data, 0x70ec, 0xffff);
    }
    if word(data, 0x70f0) < 160 {
        set_word(data, 0x70f0, word(data, 0x70f0).wrapping_add(4));
    }
    data[0x70f5] = data[0x70f5].wrapping_add(2);
    let bottom = data[0x70f5] > 191;
    let mut retire = bottom;
    if !retire {
        if data[0x70f6] == 1 {
            let x = word(data, 0x70f3).wrapping_add(5);
            set_word(data, 0x70f3, x);
            retire = x >= 300;
        } else {
            let (x, borrow) = word(data, 0x70f3).overflowing_sub(5);
            set_word(data, 0x70f3, x);
            retire = borrow;
        }
    }
    if !retire {
        set_word(data, 0x70fa, pixel_offset(word(data, 0x70f3), data[0x70f5]));
        if arrow_contact(data, tick, sound) {
            result.work = Some(Work::ArrowContact { moved: true });
            result.player_changed = true;
            retire = true;
        }
    }
    if retire {
        if result.work.is_none() {
            result.work = Some(Work::ArrowRetire { bottom });
        }
        data[0x70f2] = 0;
        restore_arrow(data, screen)?;
    } else {
        result.grid_changed = toggle_platform(data, screen)?;
        result.work = Some(Work::ArrowDraw {
            spawned,
            toggle: result.grid_changed,
        });
        restore_arrow(data, screen)?;
        draw_arrow(data, screen)?;
    }
    result.drew = true;
    Ok(result)
}

pub(crate) fn pickup_item(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<HeartStep, String> {
    check(data)?;
    let mut result = HeartStep::default();
    if tick == word(data, 0x2e8f) {
        result.work = Some(Work::PickupGate);
        return Ok(result);
    }
    set_word(data, 0x2e8f, tick);
    result.accepted = true;
    let held = word(data, 0x2e8d) < 8;
    let mut slots = 0;
    let mut active = 0;
    let arrow = data[0x70f2] != 0;
    if !held {
        for index in (0..8).rev() {
            slots += 1;
            active += u8::from(data[0x2b72 + index] != 0);
            if data[0x2b72 + index] == 0 || !overlap(item_rect(data, index), player_rect(data)) {
                continue;
            }
            if word(data, 0x2e94) == index as u16 {
                result.work = Some(Work::Pickup {
                    held,
                    slots,
                    active,
                    hit: false,
                    latched: true,
                    arrow,
                });
                return Ok(result);
            }
            if data[0x70f2] != 0 {
                restore_arrow(data, screen)?;
            }
            data[0x2b72 + index] = 0;
            set_word(data, 0x2e8d, index as u16);
            draw_item(data, screen, index, 0x2b7a)?;
            if data[0x70f2] != 0 {
                draw_arrow(data, screen)?;
            }
            sound.request(data, tick, 0x03e8, 0x0349);
            result.work = Some(Work::Pickup {
                held,
                slots,
                active,
                hit: true,
                latched: false,
                arrow,
            });
            result.item_changed = true;
            result.drew = true;
            return Ok(result);
        }
    }
    result.work = Some(Work::Pickup {
        held,
        slots,
        active,
        hit: false,
        latched: false,
        arrow,
    });
    set_word(data, 0x2e94, 0xffff);
    Ok(result)
}

pub(crate) fn place_item(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<HeartStep, String> {
    check(data)?;
    let mut result = HeartStep::default();
    let item = usize::from(word(data, 0x2e8d));
    if item >= 8 || data[0x69a] != 0 {
        result.work = Some(Work::PlaceGate { holding: item < 8 });
        return Ok(result);
    }
    result.accepted = true;
    set_word(data, 0x2e92, 0xffff);
    data[0x2e91] = 0xff;
    for row in (0..7).rev() {
        let (mut distance, borrow) = data[0x57b].overflowing_sub(data[0x2bd4 + row]);
        if borrow {
            distance = !distance;
        }
        if distance <= data[0x2e91] {
            data[0x2e91] = distance;
            set_word(data, 0x2e92, row as u16);
        }
    }
    let y = data[0x2bd4 + usize::from(word(data, 0x2e92))];
    let x = word(data, 0x579).min(263) & 0x0ffc;
    data[0x2b6a + item] = y;
    data[0x2e98] = y;
    set_word(data, 0x2b5a + item * 2, x);
    set_word(data, 0x2e96, x);
    let mut active = 0;
    for other in (0..8).rev() {
        active += u8::from(other != item && data[0x2b72 + other] != 0);
        if other != item
            && data[0x2b72 + other] != 0
            && overlap(item_rect(data, other), item_rect(data, item))
        {
            result.work = Some(Work::Place {
                blocked: true,
                active,
                arrow: false,
                consumed: false,
            });
            return Ok(result);
        }
    }
    let arrow = data[0x70f2] != 0;
    if arrow {
        restore_arrow(data, screen)?;
    }
    set_word(data, 0x2e94, item as u16);
    data[0x2b72 + item] = 1;
    draw_item(data, screen, item, 0x2af0)?;
    set_word(data, 0x2e8d, 0xffff);
    data[0x45be] = 1;
    let mut consumed = false;
    let previous = word(data, 0x45b6);
    for index in 0..7 {
        set_word(data, 0x45b6, index as u16);
        load_cat(data, index)?;
        if word(data, 0x454f) == 0 {
            consumed |= consume_item(data, screen, tick)?;
            save_cat(data, index)?;
        }
    }
    set_word(data, 0x45b6, previous);
    if data[0x70f2] != 0 {
        draw_arrow(data, screen)?;
    }
    sound.request(data, tick, 0x03e8, 0x04a5);
    result.work = Some(Work::Place {
        blocked: false,
        active,
        arrow,
        consumed,
    });
    result.item_changed = true;
    result.drew = true;
    Ok(result)
}

/// Remove only the six rival images. The goal cat and the caller's current
/// scratch record remain intact; hidden records also exclude gift interaction.
#[cfg(feature = "dev-tools")]
pub(crate) fn remove_rivals(data: &mut [u8], screen: &mut Framebuffer) -> Result<bool, String> {
    check(data)?;
    let scratch = data[0x4548..0x4554].to_vec();
    let mut drew = false;
    for index in 0..6 {
        let address = record_address(data, index)?;
        load_cat(data, index)?;
        drew |= restore_cat(data, screen)?;
        data[0x454e] = 1;
        set_word(data, 0x454f, 0xffff);
        save_cat(data, index)?;
        debug_assert_eq!(data[address + 6], 1);
    }
    data[0x4548..0x4554].copy_from_slice(&scratch);
    Ok(drew)
}

#[cfg(feature = "dev-tools")]
pub(crate) fn initialize_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_rivals: bool,
) -> Result<(), String> {
    initialize(data, screen, tick)?;
    if disable_rivals {
        remove_rivals(data, screen)?;
    }
    Ok(())
}

/// Keep the native grouped scheduler, including its skipped opportunities, but
/// process movement and player contact only for the goal cat when requested.
#[cfg(feature = "dev-tools")]
pub(crate) fn advance_cat_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_rivals: bool,
    sound: &mut crate::sound::Sound,
) -> Result<HeartStep, String> {
    if !disable_rivals {
        return advance_cat(data, screen, tick, sound);
    }
    let erased = remove_rivals(data, screen)?;
    if tick == word(data, 0x45b8) {
        return Ok(HeartStep {
            drew: erased,
            ..HeartStep::default()
        });
    }
    let next = word(data, 0x45b6).wrapping_add(1);
    let index = if next >= 7 { 0 } else { next };
    if index == 6 {
        let mut result = advance_cat(data, screen, tick, sound)?;
        result.drew |= erased;
        return Ok(result);
    }
    set_word(data, 0x45b6, index);
    if [0, 1, 4].contains(&index) {
        set_word(data, 0x45b8, tick);
    }
    Ok(HeartStep {
        accepted: true,
        drew: erased,
        ..HeartStep::default()
    })
}

#[cfg(feature = "dev-tools")]
pub(crate) fn place_item_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_rivals: bool,
    sound: &mut crate::sound::Sound,
) -> Result<HeartStep, String> {
    if !disable_rivals {
        return place_item(data, screen, tick, sound);
    }
    let erased = remove_rivals(data, screen)?;
    let mut result = place_item(data, screen, tick, sound)?;
    result.drew |= erased;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_blocking_preserves_unsaved_cat_motion_and_skips_gift_work() {
        for (arrow_x, moved, scratch_x) in [(100, false, 90), (120, true, 98)] {
            let mut data = data();
            set_word(&mut data, 0x45b6, 2);
            let record = record_address(&data, 3).unwrap();
            set_word(&mut data, record, 90);
            data[record + 2] = 1;
            data[record + 3] = 107;
            set_word(&mut data, record + 7, 0);
            data[0x70f2] = 1;
            set_word(&mut data, 0x70f3, arrow_x);
            data[0x70f5] = 107;
            data[0x57b] = 56;
            let mut screen = Framebuffer::default();
            let before = screen.bytes().to_vec();
            let step = advance_cat(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert_eq!(step.work, Some(Work::CatArrow { moved }));
            assert!(!step.drew && !step.item_changed);
            assert_eq!(word(&data, 0x4548), scratch_x);
            assert_eq!(word(&data, record), 90);
            assert_eq!(screen.bytes().as_slice(), before);
        }
    }

    #[test]
    fn pickup_work_stops_at_the_first_active_hit_without_consuming_more_gifts() {
        let mut data = data();
        for slot in [6, 7] {
            data[0x2b72 + slot] = 1;
            set_word(&mut data, 0x2b5a + slot * 2, 200);
            data[0x2b6a + slot] = 120;
        }
        let step = pickup_item(
            &mut data,
            &mut Framebuffer::default(),
            1,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(matches!(
            step.work,
            Some(Work::Pickup {
                slots: 1,
                active: 1,
                hit: true,
                ..
            })
        ));
        assert_eq!(word(&data, 0x2e8d), 7);
        assert_eq!((data[0x2b72 + 6], data[0x2b72 + 7]), (1, 0));
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn removing_arrows_restores_active_images_and_clears_cat_interactions() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        screen.fill_visible(0x55);
        data[0x70f2] = 1;
        data[0x70f7] = 0;
        set_word(&mut data, 0x70f3, 100);
        data[0x70f5] = 100;
        let offset = pixel_offset(100, 100);
        set_word(&mut data, 0x70f8, offset);
        data[0x70cc..0x70ec].fill(0x33);
        screen
            .copy_rect(offset, Dimensions(0x0802), &[0xff; 32])
            .unwrap();
        set_word(&mut data, 0x4548, 100);
        data[0x454b] = 100;
        assert!(cat_arrow_contact(&data));
        assert!(remove_arrow(&mut data, &mut screen).unwrap());
        assert!(!cat_arrow_contact(&data));
        assert_eq!((data[0x70f2], data[0x70f7]), (0, 1));
        assert_eq!(
            screen.read_rect(offset, Dimensions(0x0802)).unwrap(),
            vec![0x33; 32]
        );
        screen
            .copy_rect(offset, Dimensions(0x0802), &[0x11; 32])
            .unwrap();
        data[0x70f7] = 0;
        assert!(!remove_arrow(&mut data, &mut screen).unwrap());
        assert_eq!(
            screen.read_rect(offset, Dimensions(0x0802)).unwrap(),
            vec![0x11; 32]
        );
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn removing_rivals_erases_only_their_images_and_preserves_goal_and_scratch() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        screen.fill_visible(0x55);
        for index in 0..7 {
            let record = record_address(&data, index).unwrap();
            let offset = pixel_offset(40 + index as u16 * 32, 80);
            set_word(&mut data, record + 4, offset);
            data[record + 6] = 0;
            screen
                .copy_rect(offset, Dimensions(0x0c03), &[0xff; 72])
                .unwrap();
        }
        let scratch = data[0x4548..0x4554].to_vec();
        let goal = record_address(&data, 6).unwrap();
        let goal_record = data[goal..goal + 12].to_vec();
        assert!(remove_rivals(&mut data, &mut screen).unwrap());
        assert_eq!(data[0x4548..0x4554], scratch);
        assert_eq!(data[goal..goal + 12], goal_record);
        for index in 0..7 {
            let record = record_address(&data, index).unwrap();
            let pixels = screen
                .read_rect(word(&data, record + 4), Dimensions(0x0c03))
                .unwrap();
            assert_eq!(pixels, vec![if index == 6 { 0xff } else { 0x55 }; 72]);
            if index != 6 {
                assert_eq!((data[record + 6], word(&data, record + 7)), (1, 0xffff));
            }
        }
        assert!(!remove_rivals(&mut data, &mut screen).unwrap());
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn disabled_rivals_skip_motion_and_contact_but_goal_contact_still_succeeds() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        initialize_dev(&mut data, &mut screen, 1, true).unwrap();
        let rival_records = data[0x4554..0x459c].to_vec();
        let random = word(&data, 0x2ae5);
        set_word(&mut data, 0x45b8, 0);
        let step = advance_cat_dev(
            &mut data,
            &mut screen,
            2,
            true,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(step.accepted && !step.drew && !step.player_changed);
        assert_eq!(data[0x4554..0x459c], rival_records);
        assert_eq!(word(&data, 0x2ae5), random);
        let goal = record_address(&data, 6).unwrap();
        set_word(&mut data, goal, 200);
        data[goal + 3] = 120;
        set_word(&mut data, goal + 7, 0);
        set_word(&mut data, 0x45b6, 5);
        let step = advance_cat_dev(
            &mut data,
            &mut screen,
            3,
            true,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(step.accepted && step.success);
        assert_eq!(data[0x553], 1);
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn absent_rivals_cannot_consume_placed_gifts_and_goal_still_can() {
        for goal_overlaps in [false, true] {
            let mut data = data();
            let mut screen = Framebuffer::default();
            screen.fill_visible(0x55);
            set_word(&mut data, 0x2e8d, 0);
            set_word(&mut data, 0x579, 100);
            data[0x57b] = 175;
            for index in 0..7 {
                let record = record_address(&data, index).unwrap();
                set_word(
                    &mut data,
                    record,
                    if index == 6 && !goal_overlaps {
                        264
                    } else {
                        100
                    },
                );
                data[record + 3] = 179;
                set_word(&mut data, record + 7, 0);
            }
            let result = place_item_dev(
                &mut data,
                &mut screen,
                10,
                true,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert!(result.item_changed);
            assert_eq!(data[0x2b72], if goal_overlaps { 0 } else { 1 });
            for index in 0..6 {
                let record = record_address(&data, index).unwrap();
                assert_eq!(word(&data, record + 7), 0xffff);
            }
        }
    }

    #[cfg(feature = "dev-tools")]
    #[test]
    fn development_false_wrappers_preserve_default_hearts_state_and_pixels() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        let mut alternate = data.clone();
        let mut alternate_screen = screen.clone();
        initialize(&mut data, &mut screen, 5).unwrap();
        initialize_dev(&mut alternate, &mut alternate_screen, 5, false).unwrap();
        assert_eq!(data, alternate);
        assert_eq!(screen.bytes(), alternate_screen.bytes());
        assert_eq!(
            advance_cat(
                &mut data,
                &mut screen,
                6,
                &mut crate::sound::Sound::default()
            )
            .unwrap(),
            advance_cat_dev(
                &mut alternate,
                &mut alternate_screen,
                6,
                false,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
        );
        assert_eq!(data, alternate);
        assert_eq!(screen.bytes(), alternate_screen.bytes());
        assert_eq!(
            place_item(
                &mut data,
                &mut screen,
                6,
                &mut crate::sound::Sound::default()
            )
            .unwrap(),
            place_item_dev(
                &mut alternate,
                &mut alternate_screen,
                6,
                false,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
        );
        assert_eq!(data, alternate);
        assert_eq!(screen.bytes(), alternate_screen.bytes());
    }

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 4, 7);
        set_word(&mut data, 0x2ae5, 0xfa59);
        data[0x2bd4..0x2bdb].copy_from_slice(&[176, 152, 128, 104, 80, 56, 32]);
        data[0x2bdb..0x2be2].copy_from_slice(&[0, 18, 36, 54, 72, 90, 108]);
        set_word(&mut data, 0x2e20, 0x6100);
        set_word(&mut data, 0x2e22, 0x6120);
        set_word(&mut data, 0x2e24, 0x0101);
        set_word(&mut data, 0x2e26, 0xffff);
        data[0x6100..0x6120].fill(0x11);
        data[0x6120..0x6140].fill(0x22);
        data[0x2af0..0x2b4a].fill(0x33);
        data[0x2b7a..0x2bd4].fill(0x55);
        set_word(&mut data, 0x2e8d, 0xffff);
        set_word(&mut data, 0x2e94, 0xffff);
        for index in 0..8 {
            set_word(&mut data, 0x2b4a + index * 2, 40 + index as u16 * 24);
        }
        for row in 0..7 {
            set_word(&mut data, 0x45a8 + row * 2, 0x4554 + row as u16 * 12);
            data[0x7120 + row] = 191 - row as u8 * 24;
            set_word(&mut data, 0x45c7 + row * 2, 540);
            let record = 0x4554 + row * 12;
            data[record + 6] = 1;
            set_word(&mut data, record + 7, 100);
        }
        set_word(&mut data, 0x45d5, 140);
        for column in 0..16 {
            set_word(&mut data, 0x70fc + column * 2, 32 + column as u16 * 16);
        }
        set_word(&mut data, 0x579, 200);
        data[0x57b] = 120;
        data
    }

    #[test]
    fn initialization_preserves_edge_cells_and_clocks_and_clamps_only_item_count() {
        let mut data = data();
        data[0x2bf2] = 99;
        data[0x2bf3] = 100;
        set_word(&mut data, 0x414, 11);
        set_word(&mut data, 0x45b8, 0x9876);
        set_word(&mut data, 0x70ee, 0x1234);
        set_word(&mut data, 0x2e8f, 0x5678);
        let mut screen = Framebuffer::default();
        initialize(&mut data, &mut screen, 0).unwrap();
        assert_eq!((data[0x2bf2], data[0x2bf3]), (99, 100));
        assert_eq!(word(&data, 0x414), 11);
        assert_eq!(&data[0x2b72..0x2b7a], &[1; 8]);
        assert_eq!(
            (
                word(&data, 0x45b8),
                word(&data, 0x70ee),
                word(&data, 0x2e8f)
            ),
            (0x9876, 0x1234, 0x5678)
        );
        assert_eq!((word(&data, 0x2e8d), word(&data, 0x2e94)), (0xffff, 0xffff));
        for index in 0..7 {
            let record = record_address(&data, index).unwrap();
            assert!((96..=223).contains(&word(&data, record)));
            assert_eq!(data[record + 3], 179 - index as u8 * 24);
            assert_eq!(word(&data, record + 7), if index == 0 { 0xffff } else { 0 });
        }
    }

    #[test]
    fn hearts_support_uses_shifted_x_and_undrawn_edge_cells() {
        let mut data = data();
        let mut player = Player::at_alley_floor(41);
        player.y = 181;
        data[0x2be3] = 2;
        assert!(resolve_support(&data, &player).unwrap().hit.is_none());
        player.x = 40;
        let hit = resolve_support(&data, &player).unwrap().hit.unwrap();
        assert_eq!((hit.x, hit.y, hit.index), (40, 181, 0));
        player.x = 310;
        assert_eq!(
            resolve_support(&data, &player).unwrap().hit.unwrap().index,
            17
        );
        data[0x2bf3] = 2;
        assert!(resolve_support(&data, &player).unwrap().hit.is_none());
    }

    #[test]
    fn cat_contacts_keep_lives_support_phase_and_inclusive_edges() {
        let mut data = data();
        set_word(&mut data, 0x4548, 100);
        data[0x454b] = 35;
        set_word(&mut data, 0x579, 124);
        data[0x57b] = 47;
        data[0x55c] = 9;
        data[0x577] = 73;
        data[0x1f80] = 3;
        set_word(&mut data, 0x45b6, 5);
        assert!(cat_contact(
            &mut data,
            200,
            &mut crate::sound::Sound::default()
        ));
        assert_eq!(
            (data[0x571], data[0x576], data[0x578], data[0x55b]),
            (1, 4, 8, 4)
        );
        assert_eq!(
            (data[0x55c], data[0x577], data[0x1f80], data[0x553]),
            (9, 73, 3, 0)
        );
        set_word(&mut data, 0x579, 125);
        assert!(!cat_contact(
            &mut data,
            200,
            &mut crate::sound::Sound::default()
        ));
        set_word(&mut data, 0x579, 124);
        set_word(&mut data, 0x45b6, 6);
        assert!(cat_contact(
            &mut data,
            200,
            &mut crate::sound::Sound::default()
        ));
        assert_eq!((data[0x553], data[0x1f80]), (1, 3));
    }

    #[test]
    fn hidden_cats_commit_only_the_group_boundaries_and_do_not_save_scratch() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        let calls = [
            (101, Some(1)),
            (101, None),
            (102, Some(2)),
            (102, Some(3)),
            (102, Some(4)),
            (102, None),
            (103, Some(5)),
            (103, Some(6)),
            (103, Some(0)),
            (103, None),
        ];
        let old_records = data[0x4554..0x45a8].to_vec();
        for (tick, expected) in calls {
            let update = advance_cat(
                &mut data,
                &mut screen,
                tick,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert_eq!(update.accepted, expected.is_some());
            if let Some(index) = expected {
                assert_eq!(word(&data, 0x45b6), index);
            }
            assert!(!update.drew);
        }
        assert_eq!(&data[0x4554..0x45a8], &old_records);
    }

    #[test]
    fn pickup_is_automatic_and_recent_placement_blocks_only_first_matching_slot() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        set_word(&mut data, 0x2b5a, 100);
        data[0x2b6a] = 176;
        data[0x2b72] = 1;
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 176;
        data[0x69a] = 128;
        set_word(&mut data, 0x2e94, 0);
        assert!(
            !pickup_item(
                &mut data,
                &mut screen,
                1,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
            .item_changed
        );
        set_word(&mut data, 0x2e94, 0xffff);
        assert!(
            pickup_item(
                &mut data,
                &mut screen,
                2,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
            .item_changed
        );
        assert_eq!((data[0x2b72], word(&data, 0x2e8d)), (0, 0));
        let before = data.clone();
        assert!(
            !pickup_item(
                &mut data,
                &mut screen,
                2,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
            .accepted
        );
        assert_eq!(data, before);
    }

    #[test]
    fn blocked_placement_keeps_carried_item_but_updates_candidate_before_overlap() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        set_word(&mut data, 0x2e8d, 0);
        set_word(&mut data, 0x579, 290);
        data[0x57b] = 175;
        data[0x2b73] = 1;
        set_word(&mut data, 0x2b5c, 260);
        data[0x2b6b] = 176;
        let update = place_item(
            &mut data,
            &mut screen,
            5,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert!(update.accepted && !update.item_changed);
        assert_eq!(
            (word(&data, 0x2b5a), data[0x2b6a], word(&data, 0x2e8d)),
            (260, 176, 0)
        );
        data[0x2b73] = 0;
        assert!(
            place_item(
                &mut data,
                &mut screen,
                5,
                &mut crate::sound::Sound::default()
            )
            .unwrap()
            .item_changed
        );
        assert_eq!(
            (data[0x2b72], word(&data, 0x2e8d), word(&data, 0x2e94)),
            (1, 0xffff, 0)
        );
    }

    #[test]
    fn gift_consumption_hides_ordinary_cat_at_nonzero_tick_but_leaves_goal_visible() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        set_word(&mut data, 0x4548, 100);
        data[0x454b] = 179;
        data[0x454e] = 1;
        data[0x45be] = 1;
        set_word(&mut data, 0x2b5a, 100);
        data[0x2b6a] = 176;
        for (cat, hidden) in [(5, 0xffff), (6, 0)] {
            data[0x2b72] = 1;
            set_word(&mut data, 0x45b6, cat);
            assert!(consume_item(&mut data, &mut screen, 0).unwrap());
            assert_eq!((data[0x2b72], word(&data, 0x454f)), (0, hidden));
        }
    }

    #[test]
    fn arrows_toggle_both_ways_and_preserve_cell_latch_across_repeated_calls() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        data[0x70f5] = 184;
        data[0x70f7] = 1;
        set_word(&mut data, 0x70f3, 32);
        set_word(&mut data, 0x70fa, pixel_offset(32, 184));
        set_word(&mut data, 0x70ec, 0xffff);
        assert!(toggle_platform(&mut data, &mut screen).unwrap());
        assert_eq!((data[0x2be2], word(&data, 0x70ec)), (2, 0));
        assert!(!toggle_platform(&mut data, &mut screen).unwrap());
        assert_eq!(data[0x2be2], 2);
        set_word(&mut data, 0x70ec, 0xffff);
        assert!(toggle_platform(&mut data, &mut screen).unwrap());
        assert_eq!(data[0x2be2], 0);
    }

    #[test]
    fn arrow_motion_rejects_same_tick_and_retires_before_bottom_horizontal_move() {
        let mut data = data();
        let mut screen = Framebuffer::default();
        data[0x70f2] = 1;
        data[0x70f7] = 1;
        data[0x70f6] = 1;
        data[0x70f5] = 180;
        set_word(&mut data, 0x70f3, 32);
        set_word(&mut data, 0x70ec, 0xffff);
        for tick in 1..=5 {
            let update = advance_arrow(
                &mut data,
                &mut screen,
                tick,
                &mut crate::sound::Sound::default(),
            )
            .unwrap();
            assert!(update.accepted);
            assert_eq!(
                (word(&data, 0x70f3), data[0x70f5]),
                (32 + tick * 5, 180 + tick as u8 * 2)
            );
            let old_data = data.clone();
            let old_screen = screen.bytes().to_vec();
            assert!(
                !advance_arrow(
                    &mut data,
                    &mut screen,
                    tick,
                    &mut crate::sound::Sound::default()
                )
                .unwrap()
                .accepted
            );
            assert_eq!(data, old_data);
            assert_eq!(screen.bytes().as_slice(), &old_screen);
        }
        advance_arrow(
            &mut data,
            &mut screen,
            6,
            &mut crate::sound::Sound::default(),
        )
        .unwrap();
        assert_eq!(
            (word(&data, 0x70f3), data[0x70f5], data[0x70f2]),
            (57, 192, 0)
        );
    }
}
