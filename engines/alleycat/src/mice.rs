// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! Clothesline mouse selection, movement, contact and respawn from CS:2330/237b.
//! Images describe completed actor changes; the alley owns their composition.

use crate::assets::{DATA_LENGTH, next_random, set_word, word};
use crate::framebuffer::pixel_offset;
use crate::player::Player;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Blend {
    And,
    ZeroTransparent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Image {
    pub source: u16,
    pub dimensions: u16,
    pub offset: u16,
    pub blend: Blend,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Contact {
    #[default]
    None,
    /// The overlap helper returns carry for inactive mice and secondary Y < 38.
    Blocked,
    Knockdown {
        effect: Image,
    },
    Caught {
        points: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Update {
    pub row: usize,
    pub contact: Contact,
    pub changed: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Mice {
    images: [Option<Image>; 3],
}

fn check_data(data: &[u8]) -> Result<(), String> {
    if data.len() < DATA_LENGTH {
        Err("Executable runtime data segment is truncated".into())
    } else {
        Ok(())
    }
}

fn entry(player_x: u16) -> (u16, u8) {
    if player_x > 160 { (0, 1) } else { (300, 0xff) }
}

/// CS:2e29, including the saturating lower bounds and wrapping upper bounds.
fn overlaps(mouse_x: u16, mouse_y: u8, player: &Player) -> bool {
    mouse_x.wrapping_add(16) >= player.x
        && mouse_x.saturating_sub(24) <= player.x
        && mouse_y.wrapping_add(8) >= player.y
        && mouse_y.saturating_sub(14) <= player.y
}

impl Mice {
    pub(crate) fn initialize(data: &mut [u8], player_x: u16) -> Result<Self, String> {
        check_data(data)?;
        set_word(data, 0x1f6c, 0);
        let (x, direction) = entry(player_x);
        for row in 0..3 {
            set_word(data, 0x1f30 + row * 2, x);
            data[0x1f3c + row] = direction;
            data[0x1f48 + row] = 1;
            data[0x1f50 + row] = 0;
        }
        Self::from_data(data)
    }

    /// Recover visible actor choices when replaying a captured data segment.
    pub(crate) fn from_data(data: &[u8]) -> Result<Self, String> {
        check_data(data)?;
        let mut mice = Self::default();
        for row in 0..3 {
            if data[0x1f48 + row] == 0 {
                mice.images[row] = Some(if data[0x1f50 + row] != 0 {
                    Self::caught_image(data, row)
                } else {
                    Image {
                        source: Self::source(data, row),
                        dimensions: 0x0802,
                        offset: word(data, 0x1f42 + row * 2),
                        blend: Blend::And,
                    }
                });
            }
        }
        Ok(mice)
    }

    pub(crate) fn images(&self) -> &[Option<Image>; 3] {
        &self.images
    }

    fn source(data: &[u8], row: usize) -> u16 {
        let direction = data[0x1f3c + row];
        if direction == 0 {
            0x1e30
        } else {
            0x1e50
                + if data[0x1f4d + row] & 1 == 0 { 0x20 } else { 0 }
                + if direction == 1 { 0 } else { 0x40 }
        }
    }

    fn caught_image(data: &[u8], row: usize) -> Image {
        Image {
            source: word(data, 0x1f5f + row * 2),
            dimensions: 0x0802,
            offset: word(data, 0x1f42 + row * 2),
            blend: Blend::ZeroTransparent,
        }
    }

    fn contact(&mut self, data: &mut [u8], row: usize, player: &Player, tick: u16) -> Contact {
        let x = word(data, 0x1f30 + row * 2);
        if !overlaps(x, data[0x1f36 + row], player) {
            return Contact::None;
        }
        if data[0x1f50 + row] != 0 || player.vertical_coordinate < 38 {
            return Contact::Blocked;
        }
        if player.support_state != 0 {
            let offset = word(data, 0x1f42 + row * 2).wrapping_sub(if x >= 16 { 4 } else { 0 });
            set_word(data, 0x1f67, offset);
            set_word(data, 0x1f65, tick);
            Contact::Knockdown {
                effect: Image {
                    source: 0x1d70,
                    dimensions: 0x0806,
                    offset,
                    blend: Blend::ZeroTransparent,
                },
            }
        } else {
            data[0x1f50 + row] = 1;
            data[0x1f3c + row] = 1;
            set_word(data, 0x1f53 + row * 2, tick);
            self.images[row] = Some(Self::caught_image(data, row));
            Contact::Caught {
                points: data[0x1f39 + row],
            }
        }
    }

    /// Run a scheduled mouse call. Player/score responses and the blocking
    /// knockdown presentation are returned to the caller. Projectiles are absent.
    pub(crate) fn advance(
        &mut self,
        data: &mut [u8],
        player: &Player,
        tick: u16,
    ) -> Result<Option<Update>, String> {
        check_data(data)?;
        if word(data, 0x1f65) == tick {
            return Ok(None);
        }
        set_word(data, 0x1f65, tick);
        if player.clothes_motion_disabled() {
            return Ok(None);
        }
        let row = match word(data, 0x1f6c).wrapping_add(1) {
            value if value < 3 => usize::from(value),
            _ => 0,
        };
        set_word(data, 0x1f6c, row as u16);
        let previous_image = self.images[row];
        let contact = self.contact(data, row, player, tick);
        if contact != Contact::None {
            return Ok(Some(Update {
                row,
                contact,
                changed: self.images[row] != previous_image,
            }));
        }
        if data[0x1f50 + row] != 0 {
            if tick.wrapping_sub(word(data, 0x1f53 + row * 2)) < 54 {
                return Ok(Some(Update {
                    row,
                    contact: Contact::None,
                    changed: false,
                }));
            }
            let (x, direction) = entry(player.x);
            set_word(data, 0x1f30 + row * 2, x);
            data[0x1f50 + row] = 0;
            data[0x1f3c + row] = direction;
        }
        let previous_direction = data[0x1f3c + row];
        data[0x1f3f + row] = previous_direction;
        let special = data[0x1664] != 0;
        let mut speed = if special {
            12
        } else if player.y <= 96 {
            8
        } else {
            4
        };
        set_word(data, 0x1f69, speed);
        let direction = if (special || row == usize::from(word(data, 0x52f)))
            && previous_direction == 0
        {
            if next_random(data) & 1 != 0 { 1 } else { 0xff }
        } else {
            let y = data[0x1f36 + row];
            let pursues =
                if player.support_state != 0 && y <= player.y && y.wrapping_add(16) >= player.y {
                    let difficulty = usize::from(word(data, 8));
                    if difficulty > 7 {
                        return Err("Mouse difficulty exceeds its table".into());
                    }
                    next_random(data) as u8 <= data[0x1f6e + difficulty]
                } else {
                    false
                };
            if pursues {
                speed = 12;
                set_word(data, 0x1f69, speed);
                if word(data, 0x1f30 + row * 2) < player.x {
                    1
                } else {
                    0xff
                }
            } else {
                let threshold = if player.y <= 96 {
                    24
                } else if previous_direction != 0 {
                    40
                } else {
                    16
                };
                let random = next_random(data) as u8;
                if random > threshold {
                    previous_direction
                } else if previous_direction != 0 {
                    0
                } else if random & 1 != 0 {
                    1
                } else {
                    0xff
                }
            }
        };
        let old_x = word(data, 0x1f30 + row * 2);
        let (x, direction) = match direction {
            0 => (old_x, 0),
            1 => {
                let x = old_x.wrapping_add(speed);
                if x >= 303 { (302, 0xff) } else { (x, 1) }
            }
            _ => old_x.checked_sub(speed).map_or((0, 1), |x| (x, direction)),
        };
        set_word(data, 0x1f30 + row * 2, x);
        data[0x1f3c + row] = direction;
        let offset = pixel_offset(x, data[0x1f36 + row]);
        set_word(data, 0x1f4b, offset);
        if data[0x1f48 + row] == 0 && (direction | previous_direction) != 0 {
            self.images[row] = None;
        }
        let contact = self.contact(data, row, player, tick);
        if contact == Contact::None {
            data[0x1f48 + row] = 0;
            if direction != 0 || previous_direction != 0 {
                if direction != 0 {
                    data[0x1f4d + row] = data[0x1f4d + row].wrapping_add(1);
                }
                set_word(data, 0x1f42 + row * 2, offset);
                self.images[row] = Some(Image {
                    source: Self::source(data, row),
                    dimensions: 0x0802,
                    offset,
                    blend: Blend::And,
                });
            }
        }
        Ok(Some(Update {
            row,
            contact,
            changed: self.images[row] != previous_image,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Vec<u8> {
        let mut data = vec![0; DATA_LENGTH];
        data[0x1f36..0x1f39].copy_from_slice(&[0, 32, 64]);
        data[0x1f39..0x1f3c].copy_from_slice(&[9, 7, 5]);
        set_word(&mut data, 0x2ae5, 0xfa59);
        data
    }

    #[test]
    fn initializer_keeps_tables_timer_and_shared_seed() {
        let mut data = data();
        set_word(&mut data, 0x1f65, 77);
        let mice = Mice::initialize(&mut data, 160).unwrap();
        assert_eq!(word(&data, 0x1f30), 300);
        assert_eq!(&data[0x1f3c..0x1f3f], &[0xff; 3]);
        assert_eq!(word(&data, 0x2ae5), 0xfa59);
        assert_eq!(word(&data, 0x1f65), 77);
        assert_eq!(mice.images(), &[None; 3]);
    }

    #[test]
    fn supported_contact_precedes_movement_and_randomness() {
        let mut data = data();
        let mut mice = Mice::initialize(&mut data, 180).unwrap();
        set_word(&mut data, 0x1f32, 80);
        set_word(&mut data, 0x1f44, 0x514);
        let mut player = Player::at_alley_floor(80);
        player.y = 32;
        player.support_state = 2;
        let update = mice.advance(&mut data, &player, 1).unwrap().unwrap();
        assert!(matches!(
            update.contact,
            Contact::Knockdown {
                effect: Image { offset: 0x510, .. }
            }
        ));
        assert_eq!(word(&data, 0x1f32), 80);
        assert_eq!(word(&data, 0x2ae5), 0xfa59);
        assert_eq!(player.vertical_step, 1);
    }

    #[test]
    fn airborne_contact_catches_then_respawns_after_wrapping_54_ticks() {
        let mut data = data();
        let mut mice = Mice::initialize(&mut data, 180).unwrap();
        set_word(&mut data, 0x1f32, 80);
        set_word(&mut data, 0x1f61, 0x1ee0);
        let mut player = Player::at_alley_floor(80);
        player.y = 32;
        let update = mice.advance(&mut data, &player, 0xfff0).unwrap().unwrap();
        assert_eq!(update.contact, Contact::Caught { points: 7 });
        assert_eq!(data[0x1f51], 1);
        assert_eq!(mice.images()[1].unwrap().source, 0x1ee0);
        player.y = 180;
        set_word(&mut data, 0x1f6c, 0);
        mice.advance(&mut data, &player, 37).unwrap();
        assert_eq!(data[0x1f51], 1);
        set_word(&mut data, 0x1f6c, 0);
        mice.advance(&mut data, &player, 38).unwrap();
        assert_eq!(data[0x1f51], 0);
    }

    #[test]
    fn stopped_mouse_keeps_last_image_without_incrementing_phase() {
        let mut data = data();
        let mut mice = Mice::initialize(&mut data, 180).unwrap();
        let player = Player::at_alley_floor(180);
        data[0x1f3d] = 0;
        data[0x1f48 + 1] = 0;
        set_word(&mut data, 0x1f44, 0x500);
        set_word(&mut data, 0x2ae5, 0xffff);
        mice.images[1] = Some(Image {
            source: 0x1e30,
            dimensions: 0x0802,
            offset: 0x500,
            blend: Blend::And,
        });
        let image = mice.images[1];
        let update = mice.advance(&mut data, &player, 1).unwrap().unwrap();
        assert!(!update.changed);
        assert_eq!(mice.images[1], image);
        assert_eq!(data[0x1f4e], 0);
    }

    #[test]
    fn movement_can_contact_after_consuming_its_random_draw() {
        let mut data = data();
        let mut mice = Mice::initialize(&mut data, 180).unwrap();
        set_word(&mut data, 0x1f32, 60);
        set_word(&mut data, 0x1f44, 0x500);
        data[0x1f49] = 0;
        let mut player = Player::at_alley_floor(80);
        player.y = 32;
        player.support_state = 2;
        // Pursuit is declined; the second draw preserves rightward motion.
        let update = mice.advance(&mut data, &player, 1).unwrap().unwrap();
        assert_eq!(word(&data, 0x1f32), 68);
        assert!(matches!(update.contact, Contact::Knockdown { .. }));
        assert_ne!(word(&data, 0x2ae5), 0xfa59);
        assert_eq!(mice.images()[1], None);
        // Contact preceded recording a new saved sprite position.
        assert_eq!(word(&data, 0x1f44), 0x500);
    }

    #[test]
    fn inclusive_contact_edges_use_the_original_lower_bounds() {
        let mut player = Player::at_alley_floor(80);
        player.y = 40;
        assert!(overlaps(64, 32, &player));
        assert!(!overlaps(63, 32, &player));
        assert!(overlaps(104, 32, &player));
        assert!(!overlaps(105, 32, &player));
        player.x = 0;
        player.y = 0;
        assert!(overlaps(0, 0, &player));
        assert!(overlaps(24, 14, &player));
        assert!(!overlaps(25, 14, &player));
        assert!(!overlaps(24, 15, &player));
    }

    #[test]
    fn exact_tick_gate_updates_timer_before_disabled_player_gate() {
        let mut data = data();
        data[0x55a] = 1;
        data[0x57c] = 230;
        let player = Player::from_data(&data).unwrap();
        let mut mice = Mice::initialize(&mut data, 8).unwrap();
        assert!(mice.advance(&mut data, &player, 1).unwrap().is_none());
        assert_eq!(word(&data, 0x1f65), 1);
        assert_eq!(word(&data, 0x1f6c), 0);
    }
}
