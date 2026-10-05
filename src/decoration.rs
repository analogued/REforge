// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Nonhazardous alley decorations and the original decimal score display.

use crate::assets::{DATA_LENGTH, next_random, set_word, sprite_bytes, word};
use crate::cga::{Dimensions, Framebuffer};

const LIVES: usize = 0x1f80;
const PREVIOUS_LIVES: usize = 0x1f81;
const CURRENT_SCORE: usize = 0x1f82;
const HIGH_SCORE: usize = 0x1f89;
const SCORE_DIGITS: usize = 7;
const DIGIT_DIMENSIONS: Dimensions = Dimensions(0x0801);

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        return Err("Executable runtime data segment is truncated".into());
    }
    Ok(())
}

fn table_word(data: &[u8], offset: usize) -> Result<u16, String> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or("Decoration table exceeds executable data segment")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Startup clears both seven-byte score arrays; new-game entry supplies three
/// lives. The lives display cache must be invalidated whenever scenery is drawn.
pub(crate) fn initialize_score_state(data: &mut [u8]) -> Result<(), String> {
    check_data(data)?;
    data[LIVES] = 3;
    data[PREVIOUS_LIVES] = 0xff;
    data[CURRENT_SCORE..CURRENT_SCORE + SCORE_DIGITS].fill(0);
    data[HIGH_SCORE..HIGH_SCORE + SCORE_DIGITS].fill(0);
    Ok(())
}

/// Draws CS:5400's external destination list after base alley initialization.
/// The weighted choices are byte offsets into the source/dimension word tables,
/// and every destination consumes exactly one shared random value.
pub(crate) fn draw_ground_objects(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check_data(data)?;
    let difficulty = usize::from(word(data, 8) & 7);
    let mut list = usize::from(word(data, 0x5908 + difficulty * 2));
    let choices = difficulty * 16;
    set_word(data, 0x5918, choices as u16);
    loop {
        let destination = table_word(data, list)?;
        if destination == 0xffff {
            return Ok(());
        }
        let random = usize::from(next_random(data) & 0x0e);
        let selected = usize::from(table_word(data, 0x5888 + choices + random)?);
        let dimensions = Dimensions(table_word(data, 0x5858 + selected)?);
        let source = table_word(data, 0x584c + selected)?;
        screen.copy_rect(
            destination,
            dimensions,
            sprite_bytes(data, source, dimensions)?,
        )?;
        list += 2;
    }
}

fn draw_digit(data: &[u8], screen: &mut Framebuffer, digit: u8, offset: u16) -> Result<(), String> {
    let source = 0x2720 + u16::from(digit) * 16;
    screen.copy_rect(
        offset,
        DIGIT_DIMENSIONS,
        sprite_bytes(data, source, DIGIT_DIMENSIONS)?,
    )
}

/// CS:2739 advances the destination and source scratch pointers after each
/// glyph. After the third glyph it skips two packed bytes, leaving that region
/// untouched rather than drawing a separator or replacing the background.
fn draw_score(
    data: &mut [u8],
    screen: &mut Framebuffer,
    score: usize,
    offset: u16,
) -> Result<(), String> {
    set_word(data, 0x1f90, offset);
    set_word(data, 0x1f93, score as u16);
    data[0x1f92] = 0;
    for index in 0..SCORE_DIGITS {
        let destination = word(data, 0x1f90);
        let source = usize::from(word(data, 0x1f93));
        draw_digit(data, screen, data[source], destination)?;
        set_word(data, 0x1f90, destination + if index == 2 { 4 } else { 2 });
        set_word(data, 0x1f93, (source + 1) as u16);
        data[0x1f92] += 1;
    }
    Ok(())
}

/// Renders real DS score/lives state with external 8x8 digit glyphs. Scores
/// retain all seven decimal bytes; this does not award points or lose lives.
pub(crate) fn draw_hud(data: &mut [u8], screen: &mut Framebuffer) -> Result<(), String> {
    check_data(data)?;
    draw_score(data, screen, HIGH_SCORE, 0x12ca)?;
    draw_score(data, screen, CURRENT_SCORE, 0x143c)?;
    let lives = data[LIVES];
    if lives != data[PREVIOUS_LIVES] {
        draw_digit(data, screen, lives, 0x1260)?;
        data[PREVIOUS_LIVES] = lives;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_initializes_lives_and_all_seven_score_bytes() {
        let mut data = vec![0xa5; DATA_LENGTH];
        initialize_score_state(&mut data).unwrap();
        assert_eq!(data[LIVES], 3);
        assert_eq!(data[PREVIOUS_LIVES], 0xff);
        assert_eq!(&data[CURRENT_SCORE..HIGH_SCORE + SCORE_DIGITS], &[0; 14]);
        assert_eq!(data[0x1f90], 0xa5);
    }

    #[test]
    fn ground_uses_external_weighted_choices_and_one_random_value_per_destination() {
        let mut data = vec![0; DATA_LENGTH];
        // A masked difficulty of three selects its own list and choice row.
        set_word(&mut data, 8, 11);
        set_word(&mut data, 0x2ae5, 2);
        set_word(&mut data, 0x5908 + 3 * 2, 0x6000);
        for (index, destination) in [0x1c00, 0x1c10, 0xffff].into_iter().enumerate() {
            set_word(&mut data, 0x6000 + index * 2, destination);
        }
        // RNG 0002 -> 8001 -> 4000; both index the first weighted choice.
        set_word(&mut data, 0x5888 + 3 * 16, 4);
        set_word(&mut data, 0x584c + 4, 0x6100);
        set_word(&mut data, 0x5858 + 4, 0x0202);
        data[0x6100..0x6108].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let mut screen = Framebuffer::default();
        draw_ground_objects(&mut data, &mut screen).unwrap();
        assert_eq!(word(&data, 0x2ae5), 0x4000);
        assert_eq!(word(&data, 0x5918), 48);
        for offset in [0x1c00, 0x1c10] {
            assert_eq!(
                screen.read_rect(offset, Dimensions(0x0202)).unwrap(),
                [1, 2, 3, 4, 5, 6, 7, 8]
            );
        }
    }

    #[test]
    fn score_glyphs_use_seven_digits_and_preserve_the_separator_background() {
        let mut data = vec![0; DATA_LENGTH];
        initialize_score_state(&mut data).unwrap();
        for digit in 0..10 {
            data[0x2720 + digit * 16..0x2720 + (digit + 1) * 16].fill(digit as u8);
        }
        data[CURRENT_SCORE..CURRENT_SCORE + SCORE_DIGITS].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7]);
        data[HIGH_SCORE..HIGH_SCORE + SCORE_DIGITS].copy_from_slice(&[7, 6, 5, 4, 3, 2, 1]);
        let mut screen = Framebuffer::default();
        screen.fill_visible(0xa5);
        draw_hud(&mut data, &mut screen).unwrap();
        for (base, expected) in [
            (0x12ca, [7, 6, 5, 4, 3, 2, 1]),
            (0x143c, [1, 2, 3, 4, 5, 6, 7]),
        ] {
            for (index, digit) in expected.into_iter().enumerate() {
                let offset = base + (index * 2 + if index >= 3 { 2 } else { 0 }) as u16;
                assert_eq!(
                    screen.read_rect(offset, DIGIT_DIMENSIONS).unwrap(),
                    [digit; 16]
                );
            }
            assert_eq!(
                screen.read_rect(base + 6, DIGIT_DIMENSIONS).unwrap(),
                [0xa5; 16]
            );
        }
        assert_eq!(screen.read_rect(0x1260, DIGIT_DIMENSIONS).unwrap(), [3; 16]);
        assert_eq!(word(&data, 0x1f90), 0x144c);
        assert_eq!(word(&data, 0x1f93), (CURRENT_SCORE + SCORE_DIGITS) as u16);
        assert_eq!(data[0x1f92], 7);
        // Cached lives are not redrawn; changing the state invalidates the cache.
        screen
            .copy_rect(0x1260, DIGIT_DIMENSIONS, &[0xa5; 16])
            .unwrap();
        draw_hud(&mut data, &mut screen).unwrap();
        assert_eq!(
            screen.read_rect(0x1260, DIGIT_DIMENSIONS).unwrap(),
            [0xa5; 16]
        );
        data[LIVES] = 2;
        draw_hud(&mut data, &mut screen).unwrap();
        assert_eq!(screen.read_rect(0x1260, DIGIT_DIMENSIONS).unwrap(), [2; 16]);
    }

    #[test]
    fn truncated_data_and_unterminated_destination_lists_report_errors() {
        let mut data = vec![0; DATA_LENGTH - 1];
        assert!(initialize_score_state(&mut data).is_err());
        assert!(draw_hud(&mut data, &mut Framebuffer::default()).is_err());
        data.push(0);
        set_word(&mut data, 0x5908, (DATA_LENGTH - 2) as u16);
        set_word(&mut data, 0x584c, 0x6100);
        set_word(&mut data, 0x5858, 0x0101);
        let error = draw_ground_objects(&mut data, &mut Framebuffer::default()).unwrap_err();
        assert!(error.contains("table exceeds executable data"));
    }
}
