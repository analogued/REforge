// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Original clothesline gates, garment selection, support shifts and cat carry.

use crate::assets::{DATA_LENGTH, clothes_tile, next_random, set_word, word};
use crate::framebuffer::{Dimensions, Framebuffer};
use crate::player::Player;

/// CS:0658: an airborne cat in this row inhibits scrolling. An attached cat
/// permits scrolling and sets the carry flag. Bounds use secondary Y, not Y.
fn permits_motion(data: &mut [u8], player: &Player, row: usize) -> bool {
    data[0x4d6] = 0;
    if player.vertical_coordinate >= data[0x53d + row]
        && player.vertical_coordinate < data[0x53a + row]
    {
        if player.support_state == 0 {
            return false;
        }
        data[0x4d6] = 1;
    }
    true
}

/// CS:0633 rotates a five-byte row with carry, consuming the original external
/// start-index table. The middle row traverses its bytes in reverse order.
fn insert_support_bit(data: &mut [u8], row: usize, mut carry: bool) -> Result<(), String> {
    let start = usize::from(data[0x541 + row]);
    for index in 0..5 {
        let column = if row == 1 {
            start.checked_sub(index)
        } else {
            start.checked_add(index)
        }
        .filter(|column| *column < 15)
        .ok_or("Clothes support row exceeds its occupancy table")?;
        let value = &mut data[0x1016 + column];
        let next = if row == 1 {
            *value & 0x80 != 0
        } else {
            *value & 1 != 0
        };
        *value = if row == 1 {
            (*value << 1) | u8::from(carry)
        } else {
            (*value >> 1) | (u8::from(carry) << 7)
        };
        carry = next;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Scroll {
    pub generated_tile: bool,
}

/// Runs one scheduled CS:04a0 call. A completed four-pixel band scroll is
/// independent of whether a player movement update was accepted.
pub(crate) fn advance(
    data: &mut [u8],
    screen: &mut Framebuffer,
    player: &mut Player,
    tick: u16,
    retrace: bool,
) -> Result<Option<Scroll>, String> {
    if data.len() < DATA_LENGTH {
        return Err("Executable runtime data segment is truncated".into());
    }
    data[0x531] = data[0x531].wrapping_sub(1);
    if data[0x531] != 0 {
        return Ok(None);
    }
    data[0x531] = 1;
    if retrace || player.clothes_motion_disabled() || data[0x1673] != 0 {
        return Ok(None);
    }
    if tick == word(data, 0x544) {
        return Ok(None);
    }
    set_word(data, 0x544, tick);
    let difficulty = usize::from(word(data, 8));
    let mut row = usize::from(word(data, 0x52f));
    if difficulty > 7 || row > 2 {
        return Err("Clothes difficulty or row index exceeds its table".into());
    }
    data[0x531] = data[0x532 + difficulty];
    if player.y <= 0x60 {
        data[0x531] >>= 2;
    }

    let phase = data[0x525].wrapping_add(data[0x529 + row]);
    let generate = if !permits_motion(data, player, row) {
        if phase < 4 {
            return Ok(None);
        }
        // Crossing an unfinished row stops its movement. At the tile boundary
        // the original forcibly selects a different row without another check.
        loop {
            let candidate = usize::from(next_random(data) & 3);
            if candidate < 3 && candidate != row {
                row = candidate;
                break;
            }
        }
        set_word(data, 0x52f, row as u16);
        true
    } else {
        data[0x525] = phase;
        if phase >= 4 {
            if next_random(data) as u8 <= 0x40 {
                loop {
                    let candidate = usize::from(next_random(data) & 3);
                    if candidate < 3 && permits_motion(data, player, candidate) {
                        row = candidate;
                        break;
                    }
                }
                set_word(data, 0x52f, row as u16);
            }
            true
        } else {
            false
        }
    };

    if generate {
        data[0x525] = data[0x526 + row];
        let low = data[0x2aba + difficulty];
        let high = data[0x52c + row];
        let (_, occupancy) = clothes_tile(data, low, high);
        if row == 1 {
            insert_support_bit(data, row, occupancy & 2 != 0)?;
            data[0x540] >>= 1;
            insert_support_bit(data, row, occupancy & 1 != 0)?;
        } else {
            data[0x540] >>= 1;
            insert_support_bit(data, row, occupancy & 1 != 0)?;
            let bit = data[0x540] & 1 != 0;
            data[0x540] >>= 1;
            insert_support_bit(data, row, bit)?;
        }
    }

    if data[0x4d6] != 0 {
        let old = word(data, 0x55f);
        set_word(
            data,
            0x55f,
            if row == 1 {
                old.wrapping_sub(1)
            } else {
                old.wrapping_add(1)
            },
        );
        player.carry_with_clothes(row == 1);
    }
    let insertion = word(data, 0x51d + row * 2);
    set_word(data, 0x523, insertion);
    let start = if row == 1 {
        insertion.checked_sub(79)
    } else {
        Some(insertion)
    }
    .ok_or("Clothes band starts outside the framebuffer")?;
    let dimensions = Dimensions(0x1028);
    let mut band = screen.read_rect(start, dimensions)?;
    let phase = usize::from(data[0x525]);
    for (line, bytes) in band.chunks_exact_mut(80).enumerate() {
        let edge = data[0x4d7 + phase + line * 4];
        if row == 1 {
            bytes.copy_within(1..80, 0);
            bytes[79] = edge;
        } else {
            bytes.copy_within(0..79, 1);
            bytes[0] = edge;
        }
    }
    screen.copy_rect(start, dimensions, &band)?;
    Ok(Some(Scroll {
        generated_tile: generate,
    }))
}
