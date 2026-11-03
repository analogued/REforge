// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scene 1 bowl entry and scene 2 fish, eels and animated water surface.
//!
//! The caller owns the player image and its background. Ordinary eel contact
//! starts the original lethal presentation; fish collection and the
//! original completion/timeout immunity branches retain their ordering.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::collision::overlaps;
use crate::framebuffer::{Dimensions, Framebuffer, pixel_offset};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BowlStep {
    pub accepted: bool,
    pub drew: bool,
    pub entered: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ActorStep {
    pub accepted: bool,
    pub drew: bool,
    pub eel: bool,
    pub restored: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SurfaceStep {
    pub accepted: bool,
    pub drew: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ContactStep {
    pub drew: bool,
    pub visited: u8,
    pub tested: u8,
    pub restored_fish: u8,
    pub restored_eels: u8,
    pub contacts: u8,
    pub audible_requests: u8,
    pub collected: u8,
    pub hazard_contacts: u8,
    pub success: bool,
    pub player_changed: bool,
    pub wait_ticks: u16,
    pub failure: bool,
}

fn check(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable fishbowl data segment is truncated".into())
    } else {
        Ok(())
    }
}

fn copy_sprite(
    data: &mut [u8],
    screen: &mut Framebuffer,
    source: u16,
    dest: u16,
    dimensions: Dimensions,
) -> Result<(), String> {
    screen.copy_rect(dest, dimensions, sprite_bytes(data, source, dimensions)?)?;
    data[0x2ae0] = dimensions.0 as u8;
    data[0x2ae2] = 0;
    Ok(())
}

fn player_rect(data: &[u8]) -> (u16, u8, u16, u8) {
    (word(data, 0x579), data[0x57b], 24, 14)
}

/// The scene-2 final transition fills visible memory with 55. CS:279c then
/// replaces the top four rows with AA and draws forty seeded surface tiles.
pub(crate) fn draw_underwater(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    screen.fill_visible(0x55);
    screen.copy_bytes(0, 80, 4, &[0xaa; 320])?;
    set_word(data, 0x2654, 0);
    for slot in 0..40 {
        let phase = loop {
            let phase = next_random(data) as u8 & 0x18;
            if phase != data[0x2653] {
                break phase;
            }
        };
        data[0x2653] = phase;
        data[0x2656 + slot] = phase;
        copy_sprite(
            data,
            screen,
            0x2020 + u16::from(phase),
            0x00a0 + slot as u16 * 2,
            Dimensions(0x0401),
        )?;
        set_word(data, 0x2654, slot as u16 + 1);
    }
    Ok(())
}

/// CS:35c9 initializes slots in reverse order, preserving hazard animation
/// and actor/surface clocks. All placement and difficulty tables are external.
pub(crate) fn initialize_underwater(
    data: &mut [u8],
    _screen: &mut Framebuffer,
) -> Result<(), String> {
    check(data)?;
    set_word(data, 0x3411, 0);
    set_word(data, 0x3415, 0);
    data[0x3410] = 12;
    for slot in (0..24).rev() {
        data[0x348f + slot] = 1;
        data[0x34a7 + slot] = 0;
        data[0x3477 + slot] = data[0x34f1 + slot];
        data[0x342f + slot] = 1;
        data[0x3417 + slot] = if next_random(data) & 1 != 0 { 1 } else { 255 };
        let x = next_random(data) as u8;
        set_word(data, 0x3447 + slot * 2, u16::from(x));
    }
    let difficulty = usize::from(word(data, 8));
    if difficulty >= 8 {
        return Err("Underwater difficulty exceeds its hazard table".into());
    }
    let disabled = data[0x351c + difficulty];
    if disabled > 12 {
        return Err("Underwater disabled hazard count exceeds twelve".into());
    }
    for _ in 0..disabled {
        loop {
            let choice = next_random(data) as u8 & 15;
            if choice < 12 && data[0x34a7 + usize::from(choice) + 12] == 0 {
                data[0x34a7 + usize::from(choice) + 12] = 1;
                break;
            }
        }
    }
    Ok(())
}

/// CS:3850 checks bowl contact only on its six-tick animation opportunity.
pub(crate) fn advance_bowl(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<BowlStep, String> {
    check(data)?;
    if tick.wrapping_sub(word(data, 0x35da)) < 6 {
        return Ok(BowlStep::default());
    }
    set_word(data, 0x35da, tick);
    let phase = word(data, 0x35d8).wrapping_add(2);
    set_word(data, 0x35d8, phase);
    let source = word(data, 0x35d0 + usize::from(phase & 6));
    copy_sprite(data, screen, source, 0x15c9, Dimensions(0x0a02))?;
    let entered = overlaps((228, 138, 16, 10), player_rect(data));
    if entered {
        data[0x554] = 1;
    }
    Ok(BowlStep {
        accepted: true,
        drew: true,
        entered,
    })
}

fn actor_dimensions(slot: usize) -> Dimensions {
    Dimensions(if slot < 12 { 0x0601 } else { 0x0202 })
}

// CS:37c1 restores uniform water, rather than arbitrary saved pixels.
fn restore_actor(data: &mut [u8], screen: &mut Framebuffer, slot: usize) -> Result<bool, String> {
    if data[0x348f + slot] != 0 {
        return Ok(false);
    }
    copy_sprite(
        data,
        screen,
        0x3404,
        word(data, 0x34bf + slot * 2),
        actor_dimensions(slot),
    )?;
    Ok(true)
}

/// CS:3675 groups calls between slots zero and twelve. PCjr players near the
/// surface omit the midpoint latch and can advance all 24 slots in one tick.
pub(crate) fn advance_actors(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<ActorStep, String> {
    check(data)?;
    if tick == word(data, 0x3509) {
        return Ok(ActorStep::default());
    }
    set_word(data, 0x350b, tick);
    let cursor = word(data, 0x3415).wrapping_add(1);
    let slot = if cursor >= 24 {
        set_word(data, 0x3411, word(data, 0x3411) ^ 12);
        set_word(data, 0x3413, word(data, 0x3413).wrapping_add(8));
        set_word(data, 0x3509, tick);
        0
    } else {
        if cursor == 12 && (data[0x697] != 0xfd || data[0x57b] >= 48) {
            set_word(data, 0x3509, tick);
        }
        usize::from(cursor)
    };
    set_word(data, 0x3415, slot as u16);
    let mut result = ActorStep {
        accepted: true,
        eel: slot >= 12,
        ..ActorStep::default()
    };
    if data[0x34a7 + slot] != 0 {
        return Ok(result);
    }
    let random = next_random(data) as u8;
    if random <= 16 {
        data[0x3417 + slot] = if random & 1 != 0 { 1 } else { 255 };
        data[0x342f + slot] = if next_random(data) & 1 != 0 { 1 } else { 255 };
    }
    let step = if slot < 12 { 4 } else { 2 };
    let old_x = word(data, 0x3447 + slot * 2);
    let x = if data[0x3417 + slot] == 1 {
        let x = old_x.wrapping_add(step);
        if x >= 303 {
            data[0x3417 + slot] = 255;
            302
        } else {
            x
        }
    } else {
        let (x, borrowed) = old_x.overflowing_sub(step);
        if borrowed {
            data[0x3417 + slot] = 1;
            0
        } else {
            x
        }
    };
    set_word(data, 0x3447 + slot * 2, x);
    let minimum = data[0x34f1 + slot];
    let y = if data[0x342f + slot] == 1 {
        let y = data[0x3477 + slot].wrapping_add(1);
        let maximum = minimum.wrapping_add(24);
        if y > maximum {
            data[0x342f + slot] = 255;
            maximum
        } else {
            y
        }
    } else {
        let y = data[0x3477 + slot].wrapping_sub(1);
        if y < minimum {
            data[0x342f + slot] = 1;
            minimum
        } else {
            y
        }
    };
    data[0x3477 + slot] = y;
    let dest = pixel_offset(x, y);
    set_word(data, 0x34ef, dest);
    result.restored = restore_actor(data, screen, slot)?;
    set_word(data, 0x34bf + slot * 2, dest);
    data[0x348f + slot] = 0;
    let source = if slot >= 12 {
        0x3330 + (((slot as u16 * 8).wrapping_add(word(data, 0x3413))) & 0x18)
    } else {
        let phase = word(data, 0x3411) ^ if slot & 1 == 0 { 12 } else { 0 };
        0x3300 + phase + if data[0x3417 + slot] == 1 { 0 } else { 24 }
    };
    copy_sprite(data, screen, source, dest, actor_dimensions(slot))?;
    result.drew = true;
    Ok(result)
}

/// CS:37e5 advances forty opportunities after eight ticks, committing its
/// clock only at wrap. Tiles immediately beside a surface player are skipped.
pub(crate) fn advance_surface(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<SurfaceStep, String> {
    check(data)?;
    if tick.wrapping_sub(word(data, 0x350f)) < 8 {
        return Ok(SurfaceStep::default());
    }
    let cursor = word(data, 0x350d).wrapping_add(1);
    let slot = if cursor >= 40 {
        set_word(data, 0x350f, tick);
        0
    } else {
        usize::from(cursor)
    };
    set_word(data, 0x350d, slot as u16);
    let mut result = SurfaceStep {
        accepted: true,
        drew: false,
    };
    if data[0x57b] <= 7 {
        let (distance, borrowed) =
            ((word(data, 0x579) >> 2).wrapping_add(1)).overflowing_sub(slot as u16 * 2);
        if (if borrowed { !distance } else { distance }) < 4 {
            return Ok(result);
        }
    }
    data[0x2656 + slot] = data[0x2656 + slot].wrapping_add(8);
    let source = 0x2020 + u16::from(data[0x2656 + slot] & 0x18);
    copy_sprite(
        data,
        screen,
        source,
        0x00a0 + slot as u16 * 2,
        Dimensions(0x0401),
    )?;
    result.drew = true;
    Ok(result)
}

fn reactivate_hazards(data: &mut [u8]) {
    while data[0x351b] != 0 {
        let Some(slot) = (12..24).rev().find(|&slot| data[0x34a7 + slot] != 0) else {
            return;
        };
        data[0x34a7 + slot] = 0;
        let (x, direction) = if word(data, 0x579) > 160 {
            (0, 1)
        } else {
            (302, 255)
        };
        data[0x3417 + slot] = direction;
        set_word(data, 0x3447 + slot * 2, x);
        data[0x351b] -= 1;
    }
}

/// CS:34a0 scans fish before eels; the last fish can establish immunity for
/// later eel slots in this same call. Ordinary eel hits leave the eel active.
#[cfg(test)]
pub(crate) fn contacts(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    sound: &mut crate::sound::Sound,
) -> Result<ContactStep, String> {
    contacts_with_damage(data, screen, tick, false, sound)
}

pub(crate) fn contacts_with_damage(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    suppress_damage: bool,
    sound: &mut crate::sound::Sound,
) -> Result<ContactStep, String> {
    check(data)?;
    set_word(data, 0x3511, 0);
    data[0x351b] = 0;
    let mut result = ContactStep::default();
    for slot in 0..24 {
        set_word(data, 0x3511, slot as u16);
        result.visited += 1;
        if data[0x34a7 + slot] != 0 {
            continue;
        }
        result.tested += 1;
        let kind = if slot < 12 { 0 } else { 2 };
        if !overlaps(
            (
                word(data, 0x3447 + slot * 2),
                data[0x3477 + slot],
                word(data, 0x3513 + kind),
                data[0x3517 + kind],
            ),
            player_rect(data),
        ) {
            continue;
        }
        if slot >= 12 {
            result.hazard_contacts += 1;
            if data[0x553] == 0 && data[0x5f3] == 0 {
                if suppress_damage {
                    continue;
                }
                data[0x552] = 1;
                draw_eel_failure(data, screen)?;
                set_word(data, 0x3509, tick);
                result.drew = true;
                result.failure = true;
                result.wait_ticks = 13;
                return Ok(result);
            }
        }
        data[0x351b] = data[0x351b].wrapping_add(1);
        result.contacts += 1;
        result.audible_requests += u8::from(data[0] != 0);
        sound.request(data, tick, 0x05dc, 0x0425);
        let restored = restore_actor(data, screen, slot)?;
        result.drew |= restored;
        if restored {
            if slot < 12 {
                result.restored_fish += 1;
            } else {
                result.restored_eels += 1;
            }
        }
        data[0x34a7 + slot] = 1;
        if slot < 12 {
            result.collected += 1;
            data[0x3410] = data[0x3410].wrapping_sub(1);
            if data[0x3410] == 0 && data[0x5f3] == 0 {
                data[0x553] = 1;
            }
        }
    }
    set_word(data, 0x3511, 24);
    if data[0x351b] != 0 {
        reactivate_hazards(data);
    }
    result.success = data[0x553] != 0;
    Ok(result)
}

pub(crate) fn draw_eel_failure(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check(data)?;
    let x = word(data, 0x579).saturating_sub(8).min(278);
    let y = data[0x57b].min(180);
    copy_sprite(data, screen, 0x3350, pixel_offset(x, y), Dimensions(0x1205))
}

/// Development-only suppression also clears any eel images already on screen.
/// Positions and directions remain available if a later scene enables eels.
#[cfg(feature = "dev-tools")]
pub(crate) fn remove_eels(data: &mut [u8], screen: &mut Framebuffer) -> Result<bool, String> {
    check(data)?;
    let mut drew = false;
    for slot in 12..24 {
        drew |= restore_actor(data, screen, slot)?;
        data[0x34a7 + slot] = 1;
        data[0x348f + slot] = 1;
    }
    Ok(drew)
}

#[cfg(feature = "dev-tools")]
pub(crate) fn initialize_underwater_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    disable_eels: bool,
) -> Result<(), String> {
    initialize_underwater(data, screen)?;
    if disable_eels {
        remove_eels(data, screen)?;
    }
    Ok(())
}

#[cfg(feature = "dev-tools")]
pub(crate) fn advance_actors_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_eels: bool,
) -> Result<ActorStep, String> {
    if !disable_eels {
        return advance_actors(data, screen, tick);
    }
    let erased = remove_eels(data, screen)?;
    let mut result = advance_actors(data, screen, tick)?;
    result.drew |= erased;
    Ok(result)
}

/// With eels disabled, collection still scans every fish and retains the
/// objective and timeout rules. Its contact count cannot reactivate an eel.
#[cfg(all(feature = "dev-tools", test))]
pub(crate) fn contacts_dev(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_eels: bool,
    sound: &mut crate::sound::Sound,
) -> Result<ContactStep, String> {
    contacts_dev_with_damage(data, screen, tick, disable_eels, false, sound)
}

#[cfg(feature = "dev-tools")]
pub(crate) fn contacts_dev_with_damage(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
    disable_eels: bool,
    suppress_damage: bool,
    sound: &mut crate::sound::Sound,
) -> Result<ContactStep, String> {
    if !disable_eels {
        return contacts_with_damage(data, screen, tick, suppress_damage, sound);
    }
    let mut result = ContactStep {
        drew: remove_eels(data, screen)?,
        ..ContactStep::default()
    };
    set_word(data, 0x3511, 0);
    data[0x351b] = 0;
    for slot in 0..12 {
        set_word(data, 0x3511, slot as u16);
        result.visited += 1;
        if data[0x34a7 + slot] != 0 {
            continue;
        }
        result.tested += 1;
        if !overlaps(
            (
                word(data, 0x3447 + slot * 2),
                data[0x3477 + slot],
                word(data, 0x3513),
                data[0x3517],
            ),
            player_rect(data),
        ) {
            continue;
        }
        data[0x351b] = data[0x351b].wrapping_add(1);
        result.contacts += 1;
        result.collected += 1;
        result.audible_requests += u8::from(data[0] != 0);
        sound.request(data, tick, 0x05dc, 0x0425);
        let restored = restore_actor(data, screen, slot)?;
        result.drew |= restored;
        result.restored_fish += u8::from(restored);
        data[0x34a7 + slot] = 1;
        data[0x3410] = data[0x3410].wrapping_sub(1);
        if data[0x3410] == 0 && data[0x5f3] == 0 {
            data[0x553] = 1;
        }
    }
    set_word(data, 0x3511, 24);
    result.success = data[0x553] != 0;
    Ok(result)
}

#[cfg(test)]
mod tests;
