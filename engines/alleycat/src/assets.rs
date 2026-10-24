// Copyright (c) 2026 analogued. License has not yet been selected.

//! External assets from the supported executable, never bundled with the program.

use crate::cga::{Dimensions, Framebuffer};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const EXECUTABLE_SHA256: &str =
    "4979c8867826e08881d1d4bec95c95d6bc1ac70fd21050ccdb4f733a50d7bfdd";
pub const DATA_LENGTH: usize = 0x7130;

pub struct Assets {
    data: Vec<u8>,
}

impl Assets {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|error| format!("Cannot read executable assets: {error}"))?;
        Self::from_executable(&bytes)
    }

    pub fn from_executable(bytes: &[u8]) -> Result<Self, String> {
        let digest = Sha256::digest(bytes);
        let actual: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        if bytes.len() != 55_067 || actual != EXECUTABLE_SHA256 {
            return Err(format!(
                "Unsupported executable assets: SHA-256 {actual}; expected {EXECUTABLE_SHA256}"
            ));
        }
        Ok(Self {
            data: bytes[0x0300..0x7430].to_vec(),
        })
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn runtime_data(&self) -> Vec<u8> {
        self.data.clone()
    }
}

pub fn word(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

pub fn set_word(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

/// Advances the shared game random state once, including its original feedback.
pub fn next_random(data: &mut [u8]) -> u16 {
    let next = random_successor(word(data, 0x2ae5));
    set_word(data, 0x2ae5, next);
    next
}

pub(crate) fn random_successor(old: u16) -> u16 {
    let feedback = (((old as u8) ^ (old >> 8) as u8) >> 1) & 1;
    (old >> 1) | (u16::from(feedback) << 15)
}

pub fn sprite_bytes(data: &[u8], source: u16, dimensions: Dimensions) -> Result<&[u8], String> {
    let start = usize::from(source);
    data.get(start..start + dimensions.len())
        .ok_or_else(|| "Sprite exceeds executable data segment".into())
}

pub fn walking_source(data: &[u8], frame: u8, direction: u8) -> u16 {
    word(
        data,
        0x0f7a + (usize::from(frame % 6) + if direction == 0xff { 6 } else { 0 }) * 2,
    )
}

pub fn idle_sources(data: &mut [u8]) -> (u16, u16) {
    let top = usize::from(next_random(data) & 0x0e);
    let top = word(data, 0x0f92 + top);
    let bottom = usize::from(next_random(data) & 6);
    (top, word(data, 0x0fa2 + bottom))
}

fn blit(
    screen: &mut Framebuffer,
    data: &[u8],
    source: usize,
    offset: usize,
    dimensions: u16,
) -> Result<(), String> {
    let dimensions = Dimensions(dimensions);
    let bytes = sprite_bytes(data, source as u16, dimensions)?;
    screen.copy_rect(offset as u16, dimensions, bytes)
}

fn byte_list(screen: &mut Framebuffer, data: &[u8], mut list: usize) -> Result<(), String> {
    let dimensions = word(data, list);
    let width = usize::from(dimensions as u8);
    let rows = usize::from(dimensions >> 8);
    list += 2;
    loop {
        let source = word(data, list);
        if source == 0xffff {
            return Ok(());
        }
        let offset = word(data, list + 2);
        let start = usize::from(source);
        let bytes = data
            .get(start..start + width * rows)
            .ok_or("Rectangle list exceeds executable data")?;
        screen.copy_bytes(offset, width, rows, bytes)?;
        list += 4;
        if list + 4 > data.len() {
            return Err("Unterminated executable rectangle list".into());
        }
    }
}

fn draw_sidewalk(screen: &mut Framebuffer, data: &mut [u8]) -> Result<(), String> {
    // Consecutive eight-row patterns are deliberately rejected by the original.
    let mut previous = data[0x2ac4];
    for offset in (0x1040..0x1090).step_by(2) {
        let selected = loop {
            let candidate = next_random(data) & 0x30;
            if candidate as u8 != previous {
                break candidate;
            }
        };
        previous = selected as u8;
        blit(screen, data, 0x2904 + usize::from(selected), offset, 0x0801)?;
    }
    // These contiguous fills include only the lower visible part of each bank.
    let packed: Vec<u8> = [0x55, 0x56].into_iter().cycle().take(0xa00).collect();
    screen.copy_bytes(0x1180, 0xa00, 1, &packed)?;
    screen.copy_bytes(0x3180, 0xa00, 1, &packed)?;
    for source in (0x2944..0x296c).step_by(10) {
        for _ in 0..9 {
            let offset = usize::from(next_random(data) & 0x0776) + 0x12c0;
            blit(screen, data, source, offset, 0x0501)?;
        }
    }
    for _ in 0..5 {
        let offset = usize::from(next_random(data) & 0x3e) + 0x3a98;
        blit(screen, data, 0x296c, offset, 0x0501)?;
    }
    data[0x2ac4] = 0;
    Ok(())
}

fn draw_dumpsters(screen: &mut Framebuffer, data: &[u8], difficulty: usize) -> Result<(), String> {
    let mut list = 0x2a86 + usize::from(data[0x2ab2 + difficulty]);
    loop {
        let offset = usize::from(word(data, list));
        if offset == 0 {
            return Ok(());
        }
        let count = if offset < 0x1720 { 3 } else { 2 };
        blit(screen, data, 0x2976, offset, 0x0c05)?;
        let mut destination = offset + 0x1e0;
        for _ in 0..count {
            blit(screen, data, 0x29ee, destination, 0x0804)?;
            destination += 0x140;
        }
        blit(screen, data, 0x2a2e, destination, 0x0b04)?;
        list += 2;
    }
}

fn draw_window_half(data: &mut [u8], tile: &mut [u8; 64], column: usize) -> u8 {
    let selected = next_random(data) & 6;
    if selected == 6 {
        return 0;
    }
    let source = usize::from(word(data, 0x04d0 + usize::from(selected)));
    for row in 0..8 {
        tile[row * 4 + column..row * 4 + column + 2]
            .copy_from_slice(&data[source + row * 2..source + row * 2 + 2]);
    }
    1
}

pub(crate) fn clothes_tile(
    data: &mut [u8],
    threshold_low: u8,
    threshold_high: u8,
) -> ([u8; 64], u8) {
    let mut tile = [0xaa; 64];
    tile[4..8].fill(0x44);
    let random = next_random(data);
    let mut occupancy = 0;
    if random as u8 >= threshold_low && (random >> 8) as u8 > threshold_high {
        let selected = next_random(data) as u8;
        if selected < 0x18 {
            tile.copy_from_slice(&data[0x0490..0x04d0]);
            occupancy = 3;
        } else if selected < 0x60 {
            tile[..32].copy_from_slice(&data[0x0460..0x0480]);
            occupancy = 3;
        } else {
            occupancy = draw_window_half(data, &mut tile, 0) << 1;
            occupancy |= draw_window_half(data, &mut tile, 2);
        }
    }
    data[0x04d7..0x0517].copy_from_slice(&tile);
    data[0x0540] = occupancy;
    (tile, occupancy)
}

/// Initial scenery rejects repeated random patterns. States 0 and 1 cannot
/// escape zero, while every other state stays outside that absorbing pair.
pub(crate) fn validate_startup_seed(seed: u16) -> Result<(), String> {
    match seed {
        0 => Err("Alley random seed must be nonzero".into()),
        1 => Err("Alley random seed 1 reaches zero and cannot initialize scenery".into()),
        _ => Ok(()),
    }
}

/// Reconstructs the original alley initialization using the caller's mutable DS.
/// DS:0008 selects difficulty (0..7), DS:6df8 selects the sign, and DS:2ae5
/// supplies the shared random state. Window occupancy and moving support state
/// are written back for movement and subsequent random consumers.
/// Seeds 0 and 1 are rejected: the original pattern-rejection loop cannot
/// complete once the generator reaches zero.
pub fn initialize_alley(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        return Err("Executable runtime data segment is truncated".into());
    }
    let difficulty = usize::from(word(data, 8));
    if difficulty > 7 {
        return Err("Alley difficulty must be in 0..7".into());
    }
    validate_startup_seed(word(data, 0x2ae5))?;
    screen.fill_visible(0xaa);
    draw_sidewalk(screen, data)?;
    byte_list(screen, data, 0x28a0)?;
    let sign = word(data, 0x2ad1 + usize::from(word(data, 0x6df8) & 3) * 2);
    blit(screen, data, usize::from(sign), 0x1902, 0x0801)?;
    draw_dumpsters(screen, data, difficulty)?;
    for row in [0x03c5, 0x08c5, 0x0dc5] {
        for column in 0..4 {
            blit(screen, data, 0x2680, row + column * 0x14, 0x1005)?;
        }
    }
    data[0x1016..0x1025].fill(0);
    let threshold_low = data[0x2aba + difficulty];
    for (row, threshold_high) in [0x80, 0x30, 0].into_iter().enumerate() {
        for column in 0..20 {
            let (tile, occupancy) = clothes_tile(data, threshold_low, threshold_high);
            screen.copy_rect(
                (0x0140 + row * 0x0500 + column * 4) as u16,
                Dimensions(0x1002),
                &tile,
            )?;
            data[0x1016 + row * 5 + column / 4] |= occupancy << ((3 - column % 4) * 2);
        }
    }
    data[0x0525] = 0x10;
    set_word(data, 0x052f, 0);
    data[0x0531] = 1;
    data[0x2ac4] = 20;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_state_uses_xor_feedback_before_rotation() {
        let mut data = vec![0; DATA_LENGTH];
        set_word(&mut data, 0x2ae5, 0x0002);
        assert_eq!(next_random(&mut data), 0x8001);
        assert_eq!(next_random(&mut data), 0x4000);
        set_word(&mut data, 0x2ae5, 0x0202);
        assert_eq!(next_random(&mut data), 0x0101);
    }

    #[test]
    fn unrecognized_external_executable_is_rejected() {
        let error = Assets::from_executable(b"MZ authored synthetic input")
            .err()
            .unwrap();
        assert!(error.contains("Unsupported executable"));
        assert!(error.contains(EXECUTABLE_SHA256));
    }

    #[test]
    fn unusable_seeds_are_rejected_before_alley_data_or_framebuffer_changes() {
        for seed in [0, 1] {
            let mut data = vec![0x5a; DATA_LENGTH];
            set_word(&mut data, 8, 0);
            set_word(&mut data, 0x2ae5, seed);
            let original_data = data.clone();
            let mut framebuffer =
                Framebuffer::from_bytes(&[0xa5; crate::cga::MEMORY_SIZE]).unwrap();
            let error = initialize_alley(&mut data, &mut framebuffer).unwrap_err();
            assert!(error.contains("seed"));
            assert_eq!(data, original_data);
            assert_eq!(framebuffer.bytes(), &[0xa5; crate::cga::MEMORY_SIZE]);
        }
    }

    #[test]
    fn usable_random_states_never_enter_the_absorbing_pair() {
        assert_eq!(random_successor(0), 0);
        assert_eq!(random_successor(1), 0);
        for seed in 2..=u16::MAX {
            assert!(validate_startup_seed(seed).is_ok());
            assert!(random_successor(seed) >= 2);
        }
    }
}
