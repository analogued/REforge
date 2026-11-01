// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ordinary room drawing, table support and the nonlethal room broom.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::collision::overlaps;
use crate::framebuffer::{Dimensions, Framebuffer, pixel_offset};
use crate::player::Player;

/// Measured quiet return paths, separate from active drawing allowances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuietWork {
    CageTick,
    CageReleased,
    CageIdle,
    SpiderTick,
    SpiderBlocked,
    SpiderRollback,
    PlantTick,
    PlantMiss,
    MouseTick,
    MouseCollected,
    MouseBlocked,
    TransferInactive,
    TransferTick,
    TransferCooldown,
}

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable runtime data segment is truncated".into())
    } else {
        Ok(())
    }
}

fn copy_sprite(
    data: &[u8],
    screen: &mut Framebuffer,
    source: u16,
    offset: u16,
    dimensions: u16,
) -> Result<(), String> {
    let dimensions = Dimensions(dimensions);
    screen.copy_rect(offset, dimensions, sprite_bytes(data, source, dimensions)?)
}

/// CS:2b24 uses bytes per row, unlike the generic word-width rectangle helpers.
pub(crate) fn draw_list(
    data: &mut [u8],
    screen: &mut Framebuffer,
    mut list: usize,
    base: u16,
) -> Result<(), String> {
    if list + 2 > data.len() {
        return Err("Room rectangle list exceeds executable data".into());
    }
    let dimensions = word(data, list);
    set_word(data, 0x2ac7, dimensions);
    set_word(data, 0x2acc, base);
    let width = usize::from(dimensions as u8);
    let rows = usize::from(dimensions >> 8);
    list += 2;
    loop {
        if list + 2 > data.len() {
            return Err("Unterminated room rectangle list".into());
        }
        let source = word(data, list);
        if source == 0xffff {
            return Ok(());
        }
        if list + 4 > data.len() {
            return Err("Room rectangle list exceeds executable data".into());
        }
        let destination = word(data, list + 2).wrapping_add(base);
        let start = usize::from(source);
        let bytes = data
            .get(start..start + width * rows)
            .ok_or("Room sprite exceeds executable data")?;
        screen.copy_bytes(destination, width, rows, bytes)?;
        set_word(data, 0x2ace, width as u16);
        data[0x2ad0] = 0;
        list += 4;
    }
}

/// CS:29a0 room wall/window background shared by the ordinary rooms.
fn draw_room_wall(data: &mut [u8], screen: &mut Framebuffer, base: u16) -> Result<(), String> {
    set_word(data, 0x267e, base);
    draw_list(data, screen, 0x251c, base)?;
    for offset in [0x0284, 0x1184] {
        screen.copy_bytes(base.wrapping_add(offset), 72, 1, &[0; 72])?;
    }
    screen.copy_bytes(base.wrapping_add(0x2284), 1, 95, &[0x2a; 95])?;
    screen.copy_bytes(base.wrapping_add(0x22cb), 1, 95, &[0xa8; 95])?;
    Ok(())
}

fn draw_furniture(
    data: &mut [u8],
    screen: &mut Framebuffer,
    base: u16,
    pointers: &[usize],
) -> Result<(), String> {
    set_word(data, 0x2634, base);
    for pointer in pointers {
        draw_list(data, screen, usize::from(word(data, *pointer)), base)?;
    }
    Ok(())
}

/// CS:283e..288c, after the transition's completed AA screen fill. No random
/// values are consumed here; the cage and HUD are initialized by their callers.
pub(crate) fn draw_birdcage_room(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check_data(data)?;
    screen.fill_visible(0xaa);
    draw_room_wall(data, screen, 0x0640)?;
    draw_list(data, screen, 0x2570, 0x0cb6)?;
    set_word(data, 0x2650, 248);
    data[0x2652] = 96;
    draw_furniture(data, screen, 0x140e, &[0x263c, 0x263a, 0x2638, 0x2636])?;
    for base in [0x1434, 0x143e] {
        draw_furniture(
            data,
            screen,
            base,
            &[0x2646, 0x2644, 0x2642, 0x2640, 0x263e],
        )?;
    }
    draw_furniture(data, screen, 0x16a0, &[0x264e, 0x264c, 0x264a, 0x2648])?;
    draw_list(data, screen, 0x2344, 0x1184)?;
    copy_sprite(data, screen, 0x1fe0, 0x0dd6, 0x1002)
}

/// CS:2909..2944: furniture and the fishbowl room's window reference.
pub(crate) fn draw_fishbowl_room(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check_data(data)?;
    screen.fill_visible(0xaa);
    draw_room_wall(data, screen, 0x0640)?;
    draw_list(data, screen, 0x2570, 0x0ca0)?;
    set_word(data, 0x2650, 160);
    data[0x2652] = 96;
    draw_furniture(data, screen, 0x1406, &[0x263c, 0x263a, 0x2638, 0x2636])?;
    draw_list(data, screen, 0x2344, 0x11c4)?;
    draw_furniture(
        data,
        screen,
        0x1422,
        &[0x2646, 0x2644, 0x2642, 0x2640, 0x263e],
    )?;
    draw_furniture(data, screen, 0x1690, &[0x264e, 0x264c, 0x264a, 0x2648])?;
    set_word(data, 0x2634, 0x16b6);
    draw_list(data, screen, 0x2384, 0x16b6)?;
    draw_list(data, screen, 0x238c, 0x16b6)?;
    data[0x2ae0] = 1;
    data[0x2ae2] = 0;
    Ok(())
}

/// CS:2800..283a: the sleeping-dog room's lower wall and furniture.
/// Milk bowls and sleeping dogs are initialized separately by CS:4b47.
pub(crate) fn draw_milk_room(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check_data(data)?;
    screen.fill_visible(0xaa);
    draw_room_wall(data, screen, 0)?;
    draw_list(data, screen, 0x2570, 0x064a)?;
    set_word(data, 0x2650, 72);
    data[0x2652] = 56;
    draw_furniture(data, screen, 0x0dd2, &[0x263c, 0x263a, 0x2638, 0x2636])?;
    draw_furniture(
        data,
        screen,
        0x0df6,
        &[0x2646, 0x2644, 0x2642, 0x2640, 0x263e],
    )?;
    copy_sprite(data, screen, 0x1fa0, 0x067e, 0x1002)?;
    draw_list(data, screen, 0x2344, 0x0b84)
}

/// CS:28be..2908 and CS:3bdb: bookcase furniture, plants and seeded books.
pub(crate) fn draw_bookcase_room(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check_data(data)?;
    screen.fill_visible(0xaa);
    draw_room_wall(data, screen, 0x0640)?;
    draw_list(data, screen, 0x2570, 0x0c90)?;
    set_word(data, 0x2650, 96);
    data[0x2652] = 96;
    draw_furniture(data, screen, 0x140c, &[0x263c, 0x263a, 0x2638, 0x2636])?;
    draw_furniture(
        data,
        screen,
        0x1418,
        &[0x2646, 0x2644, 0x2642, 0x2640, 0x263e],
    )?;
    for base in [0x1184, 0x11a2] {
        draw_list(data, screen, 0x2344, base)?;
    }
    draw_list(data, screen, 0x2624, 0)?;
    let mut destination = 0x066au16;
    for remaining in (1..=16).rev() {
        let mut selected = 0u8;
        set_word(data, 0x37a0, destination);
        for column in 0..16 {
            data[0x37a2] = selected;
            copy_sprite(
                data,
                screen,
                0x3730 + u16::from(selected),
                destination + column * 2,
                0x0801,
            )?;
            let next_column = (column + 1) * 2;
            selected = if next_column == 30 {
                0x20
            } else if next_column > 30 {
                break;
            } else if selected != 0x50 && remaining & 1 == 0 && next_random(data) as u8 >= 0x40 {
                0x10
            } else {
                let random = next_random(data) as u8;
                (random.wrapping_sub(next_column as u8) & 0x30) + 0x30
            };
        }
        destination += 0x0140;
    }
    set_word(data, 0x37a0, destination);
    data[0x2ae0] = 1;
    data[0x2ae2] = 0;
    Ok(())
}

/// CS:288d..28bd and CS:3f9e: furniture, cheese tiles and the difficulty's
/// paired holes. Actor placement follows player entry and broom initialization.
pub(crate) fn draw_cheese_room(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check_data(data)?;
    screen.fill_visible(0xaa);
    draw_room_wall(data, screen, 0x0640)?;
    draw_list(data, screen, 0x2570, 0x0cba)?;
    set_word(data, 0x2650, 264);
    data[0x2652] = 96;
    draw_furniture(data, screen, 0x1439, &[0x263c, 0x263a, 0x2638, 0x2636])?;
    set_word(data, 0x2634, 0x16c0);
    draw_list(data, screen, 0x2384, 0x16c0)?;
    draw_list(data, screen, 0x238c, 0x16c0)?;

    data[0x39e0] = 0;
    data[0x39e1] = 0;
    let mut destination = 0x0506u16;
    for row in 0..17 {
        set_word(data, 0x3cbf, destination);
        set_word(data, 0x3cc1, row as u16);
        for tile in 0..usize::from(data[0x3cae + row]) {
            let random = next_random(data) as u8;
            let source = if random > 0x30 {
                0x3aea
            } else if random & 4 != 0 {
                0x3af8
            } else {
                0x3b02
            };
            copy_sprite(data, screen, source, destination + tile as u16 * 2, 0x0801)?;
        }
        destination = destination.wrapping_add(0x140);
    }
    set_word(data, 0x3cbf, destination);
    set_word(data, 0x3cc1, 17);
    for list in [0x3c22, 0x3c3e, 0x3c9a, 0x3c56] {
        draw_list(data, screen, list, 0)?;
    }
    let dimensions = Dimensions(0x0102);
    let saved = screen.and_rect(0x08ec, dimensions, sprite_bytes(data, 0x3caa, dimensions)?)?;
    data[0x000e..0x0012].copy_from_slice(&saved);
    data[0x2ae0] = 2;
    data[0x2ae2] = 0;
    let pairing = 0x3cc3 + usize::from(word(data, 8) as u8 & 3) * 8;
    for pair in 0..8 {
        let packed = data[pairing + pair];
        data[0x3ce3 + pair * 2] = packed >> 4;
        data[0x3ce4 + pair * 2] = packed & 15;
    }
    data[0x3cf3..0x3d03].fill(0);
    Ok(())
}

/// CS:07a1's owned player fields. The caller establishes the player background
/// at 055f/0561 before drawing, as CS:1124 does between the sprite/timing writes.
/// Existing animation phase, speed and edge-entry countdown are preserved.
pub(crate) fn initialize_room_player(data: &mut [u8], scene: u16) -> Result<(), String> {
    check_data(data)?;
    if !matches!(scene, 0 | 1 | 3..=7) {
        return Err(
            "Player entry supports the alley, fishbowl, bookcase, cheese, birdcage, milk and hearts rooms".into(),
        );
    }
    let (x, y) = if scene == 0 {
        (word(data, 1), data[3])
    } else {
        (
            word(data, 0x5d9 + usize::from(scene) * 2),
            data[0x5e9 + usize::from(scene)],
        )
    };
    set_word(data, 0x579, x);
    data[0x57b] = y;
    data[0x57c] = y.wrapping_add(50);
    set_word(data, 0x55f, pixel_offset(x, y));
    let source = word(data, 0xfb2);
    let dimensions = word(data, 0xfbe);
    set_word(data, 0x569, source);
    set_word(data, 0x567, dimensions);
    set_word(data, 0x561, dimensions);
    data[0x571] = 1;
    data[0x56e] = 0;
    data[0x576] = 1;
    data[0x578] = 0x40;
    data[0x55b] = 10;
    if scene == 7 {
        data[0x55b] = 0;
    }
    for offset in [
        0x550, 0x55c, 0x55a, 0x583, 0x698, 0x699, 0x551, 0x584, 0x552, 0x553, 0x127c,
    ] {
        data[offset] = 0;
    }
    for offset in [0x554, 0x57d, 0x684] {
        set_word(data, offset, 0);
    }
    Ok(())
}

fn add_decimal(data: &mut [u8], destination: usize, addend: &[u8]) {
    let mut carry = 0;
    for index in (0..7).rev() {
        let sum = data[destination + index] + addend[index] + carry;
        data[destination + index] = sum % 10;
        carry = u8::from(sum >= 10);
    }
}

/// CS:38ba..38be, before the initial cat-face presentation.
pub(crate) fn begin_room_completion(data: &mut [u8]) -> Result<(), String> {
    check_data(data)?;
    let progress = word(data, 0x414).wrapping_add(1);
    set_word(data, 0x414, progress);
    data[0x418] = 1;
    Ok(())
}

/// CS:391e/3974 ordinary-room score bookkeeping. Presentation is
/// scheduled separately, and its initial drawing precedes this tick sample.
pub(crate) fn award_room_completion(data: &mut [u8], scene: u16, tick: u16) -> Result<(), String> {
    check_data(data)?;
    if !(1..=6).contains(&scene) {
        return Err("Ordinary completion requires a room number in 1..6".into());
    }
    let elapsed = tick.wrapping_sub(word(data, 0x410));
    let time_bonus = if scene == 6 {
        2700u16.saturating_sub(elapsed)
    } else {
        1350u16.saturating_sub(elapsed) * 2
    };
    set_word(data, 0x3697, time_bonus);
    set_word(data, 0x368b, time_bonus);
    data[0x368d..0x3695].fill(0);
    let mut value = time_bonus;
    for index in (0..7).rev() {
        data[0x368d + index] = (value % 10) as u8;
        value /= 10;
    }
    let source = usize::from(word(data, 0x36cc + usize::from(scene) * 2));
    let base: [u8; 7] = data
        .get(source..source + 7)
        .ok_or("Room completion score exceeds executable data")?
        .try_into()
        .map_err(|_| "Room completion score is truncated")?;
    if base.iter().any(|digit| *digit > 9) {
        return Err("Room completion score has an invalid decimal digit".into());
    }
    add_decimal(data, 0x368d, &base);
    let award: [u8; 7] = data[0x368d..0x3694].try_into().unwrap();
    add_decimal(data, 0x1f82, &award);
    data[0x3699] = 2;
    set_word(data, 0x3722, 30);
    data[0x369e] = ((2700 - time_bonus) >> 4) as u8 & 0xf0;
    data[0x369f] = 1;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableSupport {
    pub x: u16,
    pub y: u8,
    pub state: u8,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SupportCheck {
    pub hit: Option<TableSupport>,
    pub exit: bool,
}

/// CS:16c6 support and neutral-exit result. Exit returns original CF=0, so it
/// must not be confused with a support hit. The caller clears DS:39e0 first.
pub(crate) fn resolve_scene_support(data: &[u8], player: &Player) -> Result<SupportCheck, String> {
    check_data(data)?;
    let scene = usize::from(word(data, 4));
    if !matches!(scene, 1 | 3..=6) {
        return Err(
            "Room support supports the fishbowl, bookcase, cheese, birdcage and milk rooms".into(),
        );
    }
    if player.vertical_direction == 1
        && overlaps(
            (
                word(data, 0x2650).wrapping_sub(4),
                data[0x2652].wrapping_sub(8),
                12,
                16,
            ),
            (player.x, player.y, 24, 14),
        )
    {
        return Ok(SupportCheck {
            hit: None,
            exit: true,
        });
    }
    // CS:3c43 precedes the ordinary furniture table in scene three. Shelf
    // support deliberately spans the bookcase, irrespective of book graphics.
    if scene == 3 {
        let x = player.x & 0xfffc;
        let shelf = player.y.wrapping_sub(2) & 0xf8;
        if (164..=280).contains(&x) && shelf & 8 != 0 && (40..=160).contains(&shelf) {
            return Ok(SupportCheck {
                hit: Some(TableSupport {
                    x,
                    y: shelf + 2,
                    state: 1,
                    index: 0,
                }),
                exit: false,
            });
        }
    }
    let mut index = usize::from(word(data, 0x1269 + scene * 2));
    loop {
        if index >= 96 {
            return Err("Room support index exceeds its parallel tables".into());
        }
        let y = data[0x1029 + index];
        if y == 0 {
            return Ok(SupportCheck::default());
        }
        let state = data[0x1089 + index];
        let x = word(data, 0x10e9 + index * 2);
        let span = word(data, 0x11a9 + index * 2);
        if player.y & 0xf8 == y
            && player.x & 0xfff8 >= x
            && player.x.saturating_sub(span) & 0xfffc <= x
        {
            return Ok(SupportCheck {
                hit: Some(TableSupport {
                    x: if state == 0 {
                        player.x
                    } else {
                        player.x & 0xfffc
                    },
                    y,
                    state,
                    index,
                }),
                exit: false,
            });
        }
        index += 1;
    }
}

/// CS:3405 clears the floor tracks and starts a hidden broom at (0,160).
pub(crate) fn initialize_broom(data: &mut [u8]) -> Result<(), String> {
    check_data(data)?;
    data[0x328e..0x32b6].fill(0);
    set_word(data, 0x32b6, 0xff);
    set_word(data, 0x327a, 0);
    set_word(data, 0x327d, 0);
    data[0x327f] = 160;
    data[0x3286] = 1;
    data[0x3280] = 0;
    data[0x3281] = 0;
    data[0x32eb] = next_random(data) as u8;
    data[0x32ea] = 0x6c;
    Ok(())
}

fn draw_track(data: &[u8], screen: &mut Framebuffer, index: usize) -> Result<(), String> {
    let source = 0x32b8 + u16::from(data[0x328e + index]) * 10;
    copy_sprite(data, screen, source, 0x1e00 + index as u16 * 2, 0x0501)
}

/// CS:3445 runs after horizontal control is latched and before its movement.
pub(crate) fn record_floor_track(
    data: &mut [u8],
    screen: &mut Framebuffer,
) -> Result<bool, String> {
    check_data(data)?;
    if data[0x57b] < 180 || data[0x56e] == 0 {
        return Ok(false);
    }
    let index = word(data, 0x579).wrapping_add(12) >> 3;
    if index > 39 || index == word(data, 0x32b6) {
        return Ok(false);
    }
    set_word(data, 0x32b6, index);
    let index = usize::from(index);
    if data[0x328e + index] >= 4 {
        return Ok(false);
    }
    data[0x328e + index] += 1;
    draw_track(data, screen, index)?;
    Ok(true)
}

pub(crate) fn restore_broom(data: &[u8], screen: &mut Framebuffer) -> Result<(), String> {
    if data[0x3286] == 0 {
        copy_sprite(data, screen, 0x31e8, word(data, 0x3282), 0x1e02)?;
    }
    Ok(())
}

pub(crate) fn draw_broom(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    let mut phase = usize::from(word(data, 0x327a));
    if phase + 0x3262 > data.len() {
        return Err("Broom phase exceeds its sprite table".into());
    }
    let mut source = word(data, 0x3260 + phase);
    if source == 0 {
        phase = 0;
        set_word(data, 0x327a, 0);
        source = word(data, 0x3260 + phase);
    }
    if source == 0 {
        return Err("Broom sprite table has no initial frame".into());
    }
    let dimensions = Dimensions(0x1e02);
    let offset = word(data, 0x3284);
    let background = screen.read_rect(offset, dimensions)?;
    let image = sprite_bytes(data, source, dimensions)?;
    let packed: Vec<u8> = image
        .iter()
        .zip(&background)
        .map(|(sprite, old)| sprite | old)
        .collect();
    data[0x31e8..0x3260].copy_from_slice(&background);
    set_word(data, 0x3282, offset);
    data[0x3286] = 0;
    set_word(data, 0x3287, 2);
    data[0x3289] = 0;
    screen.copy_rect(offset, dimensions, &packed)
}

/// CS:0872 pushes toward the room return point, using NOT-based separations.
fn begin_push(data: &mut [u8]) {
    set_word(data, 0x592a, 0x400);
    if data[0x584] != 0 {
        return;
    }
    data[0x576] = 8;
    data[0x571] = if data[0x57b] >= data[0x2652] { 0xff } else { 1 };
    let (mut distance, borrow) = word(data, 0x579).overflowing_sub(word(data, 0x2650));
    data[0x56e] = if !borrow && distance != 0 {
        0xff
    } else {
        distance = !distance;
        1
    };
    if distance >> 8 != 0 {
        distance = 0xff;
    }
    let amount = (!(distance as u8)).max(48);
    let decrement = amount.wrapping_sub(amount >> 2);
    data[0x578] = decrement;
    set_word(data, 0x572, u16::from(decrement >> 5));
    data[0x55c] = 0;
    data[0x39e0] = 0;
    data[0x577] = 1;
    data[0x55b] = 16;
    data[0x584] = 1;
}

fn broom_player_contact(data: &mut [u8]) -> bool {
    if data[0x1cb8] != 0 {
        return false;
    }
    // CS:33c8/47b0 uses the wider drinking image without starting a push.
    if word(data, 4) == 6 && data[0x44bd] != 0 {
        return overlaps(
            (word(data, 0x327d), data[0x327f], 16, 30),
            (
                word(data, 0x579).saturating_sub(8),
                data[0x57b].wrapping_add(3),
                40,
                14,
            ),
        );
    }
    let contact = overlaps(
        (word(data, 0x327d), data[0x327f], 16, 30),
        (word(data, 0x579), data[0x57b], 24, 14),
    );
    // CS:33f0..33fc blocks the broom during a cheese-hole transfer, but does
    // not replace the transfer's player state with the ordinary push response.
    if contact && !(word(data, 4) == 4 && data[0x39e1] != 0) {
        begin_push(data);
    }
    contact
}

fn broom_dog_contact(data: &[u8]) -> bool {
    (data[0x1cbf] | data[0x1cc0] | data[0x1cc1]) != 0
        && overlaps(
            (word(data, 0x327d), data[0x327f], 16, 30),
            (word(data, 0x1cc6), data[0x1cc8], 32, 15),
        )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BroomUpdate {
    pub contact: bool,
    pub drawn: bool,
    pub restored: bool,
}

/// CS:3150's accepted tick call. Player fields are read/written in DS; the caller
/// synchronizes its owned player before and after this nonlethal actor update.
pub(crate) fn advance_broom(
    data: &mut [u8],
    screen: &mut Framebuffer,
    tick: u16,
) -> Result<Option<BroomUpdate>, String> {
    check_data(data)?;
    let scene = usize::from(word(data, 4));
    if !matches!(scene, 1 | 3..=6) {
        return Err(
            "Broom update supports the fishbowl, bookcase, cheese, birdcage and milk rooms".into(),
        );
    }
    if tick.wrapping_sub(word(data, 0x328c)) < word(data, 0x32f2 + scene * 2) {
        return Ok(None);
    }
    set_word(data, 0x328c, tick);
    if broom_player_contact(data) {
        return Ok(Some(BroomUpdate {
            contact: true,
            ..BroomUpdate::default()
        }));
    }
    if broom_dog_contact(data) {
        return Ok(Some(BroomUpdate::default()));
    }
    let mut restored = false;
    data[0x32ea] = data[0x32ea].wrapping_add(1);
    let random = next_random(data) as u8;
    data[0x32eb] ^= data[0x32ea] & random;
    let (mut horizontal, borrow) = word(data, 0x327d).overflowing_sub(word(data, 0x579));
    data[0x32ed] = if borrow {
        horizontal = !horizontal;
        1
    } else {
        0xff
    };
    let (mut vertical, borrow) = data[0x327f].wrapping_add(20).overflowing_sub(data[0x57b]);
    data[0x32ee] = if borrow {
        vertical = !vertical;
        1
    } else {
        0xff
    };
    data[0x32ec] = ((horizontal >> 2) as u8).wrapping_add(vertical >> 1);
    let mut cursor = usize::from(word(data, 0x328a));
    if cursor >= 39 {
        cursor = 38;
        set_word(data, 0x328a, 38);
    }
    if data[0x328e + cursor] == 0 {
        set_word(data, 0x328a, (cursor as u16).wrapping_sub(1));
        let elapsed = (tick.wrapping_sub(word(data, 0x410)) >> 3) as u8;
        let distance = data[0x32ec].saturating_sub(elapsed);
        if distance >= data[0x32eb] {
            data[0x3281] = 1;
            let random = next_random(data) as u8;
            if random == 0 {
                data[0x3280] = 0;
            } else if random <= 7 {
                data[0x3280] = if random & 1 != 0 { 1 } else { 0xff };
            }
        } else {
            let choice = data[0x32eb] & 0x2f;
            if choice == 0 {
                data[0x3280] = if next_random(data) & 1 != 0 { 1 } else { 0xff };
                data[0x3281] = if next_random(data) & 1 != 0 { 1 } else { 0xff };
            } else if choice & 7 == 0 {
                data[0x3280] = data[0x32ed];
                data[0x3281] = data[0x32ee];
            }
        }
    } else {
        data[0x3281] = 1;
        let target = cursor as u16 * 8;
        let x = word(data, 0x327d);
        if x != target {
            data[0x3280] = if x < target { 1 } else { 0xff };
        } else {
            data[0x3280] = 0;
            if data[0x327f] == 165 {
                data[0x3281] = 0;
                if [6, 18].contains(&word(data, 0x327a)) {
                    copy_sprite(data, screen, 0x31e8, word(data, 0x3282), 0x1e02)?;
                    restored = true;
                    data[0x328e + cursor] -= 1;
                    draw_track(data, screen, cursor)?;
                    data[0x3286] = 1;
                }
            }
        }
    }
    let old_x = word(data, 0x327d);
    let old_y = data[0x327f];
    set_word(data, 0x32ef, old_x);
    data[0x32f1] = old_y;
    let x = match data[0x3280] {
        0 => old_x,
        1 => old_x.wrapping_add(8).min(304),
        _ => old_x.saturating_sub(8),
    } & 0xfff8;
    let y = match data[0x3281] {
        0 => old_y,
        1 => old_y.wrapping_add(2).min(165),
        _ => old_y.saturating_sub(2),
    };
    set_word(data, 0x327d, x);
    data[0x327f] = y;
    set_word(data, 0x3284, pixel_offset(x, y));
    let contact = broom_player_contact(data);
    if contact || broom_dog_contact(data) {
        data[0x3280] = 0;
        data[0x3281] = 0;
        set_word(data, 0x327d, old_x);
        data[0x327f] = old_y;
        return Ok(Some(BroomUpdate {
            contact,
            drawn: false,
            restored,
        }));
    }
    restored |= data[0x3286] == 0;
    restore_broom(data, screen)?;
    let phase = word(data, 0x327a).wrapping_add(2);
    set_word(data, 0x327a, phase);
    draw_broom(data, screen)?;
    Ok(Some(BroomUpdate {
        contact: false,
        drawn: true,
        restored,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 4, 5);
        set_word(&mut data, 0x2ae5, 0xfa59);
        set_word(&mut data, 0x32fc, 1);
        set_word(&mut data, 0x2650, 248);
        data[0x2652] = 96;
        data
    }

    #[test]
    fn bookcase_shelves_use_aligned_geometry_and_window_exit_precedence() {
        let mut data = data();
        set_word(&mut data, 4, 3);
        set_word(&mut data, 0x2650, 96);
        data[0x2652] = 96;
        for (x, expected) in [
            (163, None),
            (164, Some(164)),
            (167, Some(164)),
            (280, Some(280)),
            (283, Some(280)),
            (284, None),
        ] {
            let mut player = Player::at_alley_floor(x);
            player.y = 42;
            let result = resolve_scene_support(&data, &player).unwrap();
            assert_eq!(
                result.hit.map(|hit| (hit.x, hit.y, hit.state)),
                expected.map(|x| (x, 42, 1))
            );
            assert!(!result.exit);
        }
        for (y, expected) in [
            (41, None),
            (42, Some(42)),
            (49, Some(42)),
            (50, None),
            (58, Some(58)),
            (154, Some(154)),
            (161, Some(154)),
            (162, None),
            (0, None),
        ] {
            let mut player = Player::at_alley_floor(200);
            player.y = y;
            assert_eq!(
                resolve_scene_support(&data, &player)
                    .unwrap()
                    .hit
                    .map(|hit| hit.y),
                expected
            );
        }
        set_word(&mut data, 0x2650, 204);
        data[0x2652] = 50;
        let mut player = Player::at_alley_floor(200);
        player.y = 42;
        player.vertical_direction = 1;
        assert_eq!(
            resolve_scene_support(&data, &player).unwrap(),
            SupportCheck {
                hit: None,
                exit: true
            }
        );
        player.vertical_direction = 0xff;
        assert!(resolve_scene_support(&data, &player).unwrap().hit.is_some());
    }

    #[test]
    fn milk_room_uses_its_lower_window_external_furniture_and_own_entry_table() {
        let mut data = data();
        set_word(&mut data, 4, 6);
        for list in [0x251c, 0x2570, 0x2344, 0x6000] {
            set_word(&mut data, list, 0x0101);
            set_word(&mut data, list + 2, 0xffff);
        }
        for pointer in [
            0x263c, 0x263a, 0x2638, 0x2636, 0x2646, 0x2644, 0x2642, 0x2640, 0x263e,
        ] {
            set_word(&mut data, pointer, 0x6000);
        }
        data[0x1fa0..0x1fe0].fill(0x55);
        let mut screen = Framebuffer::default();
        draw_milk_room(&mut data, &mut screen).unwrap();
        assert_eq!(
            (word(&data, 0x267e), word(&data, 0x2650), data[0x2652]),
            (0, 72, 56)
        );
        assert_eq!(word(&data, 0x2634), 0x0df6);
        assert_eq!(
            screen.read_rect(0x067e, Dimensions(0x1002)).unwrap(),
            [0x55; 64]
        );
        assert_eq!(word(&data, 0x2ae5), 0xfa59);
        set_word(&mut data, 0x5e5, 240);
        data[0x5ef] = 56;
        set_word(&mut data, 0xfb2, 0x6100);
        set_word(&mut data, 0xfbe, 0x0d03);
        initialize_room_player(&mut data, 6).unwrap();
        assert_eq!(
            (word(&data, 0x579), data[0x57b], data[0x57c], data[0x55b]),
            (240, 56, 106, 10)
        );
    }

    #[test]
    fn milk_support_and_broom_use_scene_six_external_tables() {
        let mut data = data();
        set_word(&mut data, 4, 6);
        set_word(&mut data, 0x2650, 72);
        data[0x2652] = 56;
        set_word(&mut data, 0x1275, 90);
        data[0x1029 + 90] = 136;
        data[0x1089 + 90] = 1;
        set_word(&mut data, 0x10e9 + 180, 144);
        set_word(&mut data, 0x11a9 + 180, 40);
        let mut player = Player::at_alley_floor(147);
        player.y = 143;
        let support = resolve_scene_support(&data, &player).unwrap().hit.unwrap();
        assert_eq!(
            (support.x, support.y, support.state, support.index),
            (144, 136, 1, 90)
        );
        player.x = 72;
        player.y = 56;
        player.vertical_direction = 1;
        assert!(resolve_scene_support(&data, &player).unwrap().exit);
        set_word(&mut data, 0x32fe, 3);
        set_word(&mut data, 0x327d, 100);
        data[0x327f] = 100;
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        assert!(
            advance_broom(&mut data, &mut Framebuffer::default(), 2)
                .unwrap()
                .is_none()
        );
        assert!(
            advance_broom(&mut data, &mut Framebuffer::default(), 3)
                .unwrap()
                .unwrap()
                .contact
        );
    }

    #[test]
    fn cheese_room_draw_uses_external_tiles_lists_and_difficulty_pairings() {
        let mut data = data();
        set_word(&mut data, 4, 4);
        set_word(&mut data, 8, 5);
        // Empty authored lists isolate the scenery's tile/pairing contracts.
        for list in [
            0x251c, 0x2570, 0x2384, 0x238c, 0x3c22, 0x3c3e, 0x3c9a, 0x3c56, 0x6000,
        ] {
            set_word(&mut data, list, 0x0101);
            set_word(&mut data, list + 2, 0xffff);
        }
        for pointer in [0x263c, 0x263a, 0x2638, 0x2636] {
            set_word(&mut data, pointer, 0x6000);
        }
        data[0x3cae] = 1;
        data[0x3aea..0x3b12].fill(0x55);
        data[0x3cca..0x3cd3].copy_from_slice(&[0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]);
        data[0x3caa..0x3cae].fill(0xff);
        data[0x3cf3..0x3d03].fill(0xff);
        let mut screen = Framebuffer::default();
        draw_cheese_room(&mut data, &mut screen).unwrap();
        assert_eq!((word(&data, 0x2650), data[0x2652]), (264, 96));
        assert_eq!(word(&data, 0x2ae5), 0xfd2c);
        assert_eq!(
            screen.read_rect(0x0506, Dimensions(0x0801)).unwrap(),
            [0x55; 16]
        );
        assert_eq!(
            &data[0x3ce3..0x3cf3],
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0]
        );
        assert_eq!(&data[0x3cf3..0x3d03], &[0; 16]);
        assert_eq!((word(&data, 0x3cbf), word(&data, 0x3cc1)), (0x1a46, 17));
        assert_eq!(&data[0x000e..0x0012], &[0; 4]);
        // The final AND mask preserves black background pixels; a transparent
        // copy of this all-ones fixture would incorrectly paint them white.
        assert_eq!(
            screen.read_rect(0x08ec, Dimensions(0x0102)).unwrap(),
            [0; 4]
        );
    }

    #[test]
    fn cheese_broom_uses_its_own_external_acceptance_gate() {
        let mut data = data();
        set_word(&mut data, 4, 4);
        set_word(&mut data, 0x32fa, 3);
        set_word(&mut data, 0x327d, 100);
        data[0x327f] = 100;
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        assert!(
            advance_broom(&mut data, &mut Framebuffer::default(), 2)
                .unwrap()
                .is_none()
        );
        assert!(
            advance_broom(&mut data, &mut Framebuffer::default(), 3)
                .unwrap()
                .unwrap()
                .contact
        );
        assert_eq!(word(&data, 0x328c), 3);
    }

    #[test]
    fn room_entry_keeps_inherited_phase_speed_and_original_random_state() {
        let mut data = data();
        set_word(&mut data, 0x5e3, 240);
        data[0x5ee] = 96;
        set_word(&mut data, 0xfb2, 0x6100);
        set_word(&mut data, 0xfbe, 0x0d03);
        set_word(&mut data, 0x572, 7);
        data[0x577] = 99;
        data[0x558] = 2;
        initialize_room_player(&mut data, 5).unwrap();
        assert_eq!(
            (word(&data, 0x579), data[0x57b], data[0x57c]),
            (240, 96, 146)
        );
        assert_eq!(
            (word(&data, 0x569), word(&data, 0x567), word(&data, 0x561)),
            (0x6100, 0x0d03, 0x0d03)
        );
        assert_eq!((word(&data, 0x572), data[0x577], data[0x558]), (7, 99, 2));
        assert_eq!(word(&data, 0x2ae5), 0xfa59);
    }

    #[test]
    fn completion_uses_external_base_score_and_wrapping_elapsed_ticks() {
        let mut data = data();
        set_word(&mut data, 0x36d6, 0x6100);
        data[0x6100..0x6107].copy_from_slice(&[0, 0, 0, 3, 5, 0, 0]);
        set_word(&mut data, 0x410, 0xfff0);
        data[0x1f80] = 3;
        begin_room_completion(&mut data).unwrap();
        award_room_completion(&mut data, 5, 84).unwrap();
        assert_eq!(word(&data, 0x3697), 2500);
        assert_eq!(&data[0x1f82..0x1f89], &[0, 0, 0, 6, 0, 0, 0]);
        assert_eq!((word(&data, 0x414), data[0x418]), (1, 1));
        assert_eq!((data[0x1f80], word(&data, 0x2ae5)), (3, 0xfa59));
        assert_eq!(&data[0x1f89..0x1f90], &[0; 7]);
    }

    #[test]
    fn exhausted_completion_time_adds_only_base_with_decimal_overflow() {
        let mut data = data();
        set_word(&mut data, 0x36d6, 0x6100);
        data[0x6100..0x6107].copy_from_slice(&[0, 0, 0, 3, 5, 0, 0]);
        data[0x1f82..0x1f89].copy_from_slice(&[9, 9, 9, 9, 0, 0, 0]);
        award_room_completion(&mut data, 5, 1351).unwrap();
        assert_eq!(word(&data, 0x3697), 0);
        assert_eq!(&data[0x1f82..0x1f89], &[0, 0, 0, 2, 5, 0, 0]);
    }

    #[test]
    fn ordinary_completion_uses_each_rooms_external_base_and_milk_time_rule() {
        let mut data = data();
        set_word(&mut data, 0x36d4, 0x6100);
        set_word(&mut data, 0x36d8, 0x6107);
        data[0x6100..0x6107].copy_from_slice(&[0, 0, 0, 1, 5, 0, 0]);
        data[0x6107..0x610e].copy_from_slice(&[0, 0, 0, 2, 5, 0, 0]);
        award_room_completion(&mut data, 4, 100).unwrap();
        assert_eq!(word(&data, 0x3697), 2500);
        assert_eq!(&data[0x368d..0x3694], &[0, 0, 0, 4, 0, 0, 0]);
        award_room_completion(&mut data, 6, 100).unwrap();
        assert_eq!(word(&data, 0x3697), 2600);
        assert_eq!(&data[0x368d..0x3694], &[0, 0, 0, 5, 1, 0, 0]);
        assert_eq!(&data[0x1f82..0x1f89], &[0, 0, 0, 9, 1, 0, 0]);
        assert_eq!(
            word(&data, 0x414),
            0,
            "Scoring must not begin another presentation"
        );
    }

    #[test]
    fn room_table_hit_keeps_zero_state_x_and_aligns_attached_x() {
        let mut data = data();
        set_word(&mut data, 0x1273, 62);
        data[0x1029 + 62] = 136;
        set_word(&mut data, 0x10e9 + 124, 128);
        set_word(&mut data, 0x11a9 + 124, 40);
        let mut player = Player::at_alley_floor(163);
        player.y = 139;
        assert_eq!(
            resolve_scene_support(&data, &player)
                .unwrap()
                .hit
                .unwrap()
                .x,
            163
        );
        data[0x1089 + 62] = 1;
        let hit = resolve_scene_support(&data, &player).unwrap().hit.unwrap();
        assert_eq!((hit.x, hit.y, hit.state), (160, 136, 1));
        player.x = 172;
        assert!(resolve_scene_support(&data, &player).unwrap().hit.is_none());
    }

    #[test]
    fn descending_exit_precedes_overlapping_room_support() {
        let mut data = data();
        data[0x1029] = 96;
        set_word(&mut data, 0x10e9, 248);
        set_word(&mut data, 0x11a9, 8);
        let mut player = Player::at_alley_floor(248);
        player.y = 96;
        player.vertical_direction = 1;
        assert_eq!(
            resolve_scene_support(&data, &player).unwrap(),
            SupportCheck {
                hit: None,
                exit: true
            }
        );
        player.vertical_direction = 0xff;
        assert!(resolve_scene_support(&data, &player).unwrap().hit.is_some());
    }

    #[test]
    fn broom_contact_starts_push_toward_window_without_random_draw_or_life_loss() {
        let mut data = data();
        set_word(&mut data, 0x327d, 100);
        data[0x327f] = 100;
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        data[0x1f80] = 3;
        let update = advance_broom(&mut data, &mut Framebuffer::default(), 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            update,
            BroomUpdate {
                contact: true,
                drawn: false,
                restored: false,
            }
        );
        assert_eq!(
            (
                data[0x56e],
                data[0x571],
                data[0x576],
                data[0x55b],
                data[0x584]
            ),
            (1, 0xff, 8, 16, 1)
        );
        assert_eq!(word(&data, 0x2ae5), 0xfa59);
        assert_eq!((data[0x1f80], data[0x552]), (3, 0));
    }

    #[test]
    fn broom_blocks_a_cheese_transfer_without_replacing_its_player_motion() {
        let mut data = data();
        set_word(&mut data, 4, 4);
        set_word(&mut data, 0x327d, 100);
        data[0x327f] = 100;
        set_word(&mut data, 0x579, 100);
        data[0x57b] = 100;
        data[0x39e1] = 7;
        data[0x39e0] = 4;
        let before = data.clone();
        assert!(broom_player_contact(&mut data));
        assert_eq!(data, before);
        data[0x39e1] = 0;
        assert!(broom_player_contact(&mut data));
        assert_eq!((data[0x584], data[0x39e0]), (1, 0));
    }

    #[test]
    fn drinking_broom_contact_uses_the_wider_image_without_starting_a_push() {
        let mut data = data();
        set_word(&mut data, 4, 6);
        set_word(&mut data, 0x327d, 100);
        data[0x327f] = 100;
        set_word(&mut data, 0x579, 124);
        data[0x57b] = 100;
        data[0x44bd] = 1;
        let before = data.clone();
        // The ordinary 24-pixel image misses; the drinking image reaches left.
        assert!(broom_player_contact(&mut data));
        assert_eq!(data, before);
        set_word(&mut data, 0x579, 125);
        let before = data.clone();
        assert!(!broom_player_contact(&mut data));
        assert_eq!(data, before);
    }

    #[test]
    fn floor_tracks_count_distinct_old_positions_and_stop_at_four() {
        let mut data = data();
        set_word(&mut data, 0x579, 80);
        data[0x57b] = 180;
        data[0x56e] = 1;
        let mut screen = Framebuffer::default();
        assert!(record_floor_track(&mut data, &mut screen).unwrap());
        assert_eq!((word(&data, 0x32b6), data[0x3299]), (11, 1));
        assert!(!record_floor_track(&mut data, &mut screen).unwrap());
        data[0x3299] = 4;
        set_word(&mut data, 0x32b6, 0);
        assert!(!record_floor_track(&mut data, &mut screen).unwrap());
        assert_eq!(data[0x3299], 4);
    }

    #[test]
    fn broom_renderer_uses_or_and_saves_the_complete_old_rectangle() {
        let mut data = data();
        set_word(&mut data, 0x3260, 0x6100);
        data[0x6100..0x6178].fill(0x55);
        set_word(&mut data, 0x3284, 0x140);
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        draw_broom(&mut data, &mut screen).unwrap();
        assert_eq!(&data[0x31e8..0x3260], &[0xaa; 120]);
        assert_eq!(
            screen.read_rect(0x140, Dimensions(0x1e02)).unwrap(),
            [0xff; 120]
        );
        restore_broom(&data, &mut screen).unwrap();
        assert_eq!(
            screen.read_rect(0x140, Dimensions(0x1e02)).unwrap(),
            [0xaa; 120]
        );
    }

    #[test]
    fn broom_moves_toward_a_floor_track_once_per_accepted_tick() {
        let mut data = data();
        set_word(&mut data, 0x327d, 80);
        data[0x327f] = 160;
        data[0x3286] = 1;
        set_word(&mut data, 0x328a, 10);
        data[0x3298] = 1;
        set_word(&mut data, 0x579, 200);
        data[0x57b] = 180;
        set_word(&mut data, 0x3260, 0x6100);
        set_word(&mut data, 0x3262, 0x6100);
        data[0x6100..0x6178].fill(0x55);
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        assert_eq!(
            advance_broom(&mut data, &mut screen, 1).unwrap(),
            Some(BroomUpdate {
                contact: false,
                drawn: true,
                restored: false,
            })
        );
        assert_eq!((word(&data, 0x327d), data[0x327f]), (80, 162));
        assert_eq!(word(&data, 0x2ae5), 0xfd2c);
        let previous_data = data.clone();
        let previous_screen = screen.bytes().to_vec();
        assert!(advance_broom(&mut data, &mut screen, 1).unwrap().is_none());
        assert_eq!(data, previous_data);
        assert_eq!(screen.bytes().as_slice(), previous_screen);
        let next = advance_broom(&mut data, &mut screen, 2).unwrap().unwrap();
        assert!(next.drawn && next.restored);
        assert_eq!((word(&data, 0x327d), data[0x327f]), (80, 164));
    }

    #[test]
    fn cleaning_restores_the_old_broom_before_replacing_the_floor_track() {
        let mut data = data();
        set_word(&mut data, 0x327d, 80);
        data[0x327f] = 165;
        set_word(&mut data, 0x3284, pixel_offset(80, 165));
        set_word(&mut data, 0x328a, 10);
        data[0x3298] = 1;
        set_word(&mut data, 0x579, 200);
        data[0x57b] = 180;
        set_word(&mut data, 0x327a, 6);
        set_word(&mut data, 0x3266, 0x6100);
        set_word(&mut data, 0x3268, 0x6100);
        data[0x6100..0x6178].fill(0x55);
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xaa);
        draw_broom(&mut data, &mut screen).unwrap();
        let step = advance_broom(&mut data, &mut screen, 1).unwrap().unwrap();
        assert!(step.drawn && step.restored);
        assert_eq!(data[0x3298], 0);
        assert_eq!(word(&data, 0x327a), 8);
        // Cleaning hides the old image before the normal redraw. Its saved
        // background must contain the cleaned track, not the previous broom.
        let clean_track = &data[0x32b8..0x32c2];
        let saved = &data[0x31e8..0x3260];
        assert_eq!(&saved[108..110], &clean_track[..2]);
    }
}
