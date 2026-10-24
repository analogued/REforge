// Copyright 2026 analogued. License selection is pending.

//! Accepted-update movement for the alley and ordinary room player paths.
//!
//! The caller supplies the current external game data segment, including alley
//! support and sprite tables. Timing, actors, collision effects,
//! sound, and idle sprite randomness belong to the caller.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Controls {
    /// -1 means left/up, 0 means neutral, 1 means right/down.
    pub x: i8,
    pub y: i8,
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Sprite {
    pub source: u16,
    /// Low byte: words per row. High byte: row count.
    pub dimensions: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Draw {
    Keep,
    /// Restore the current player background without drawing a replacement.
    Erase,
    /// Select and draw the original two idle pieces using the shared PRNG.
    Idle,
    Sprite(Sprite),
    /// Copy each visible row from a wider original source row before drawing.
    StridedSprite {
        sprite: Sprite,
        stride_bytes: usize,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Support {
    #[default]
    None,
    Floor,
    LowerList,
    Band96,
    UpperBitfield,
    RoomTable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeartsClipKind {
    Bottom,
    Top,
    Hidden,
}

/// Actual clipping work with a valid visible saved-image history in scene seven.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeartsClippingWork {
    pub kind: HeartsClipKind,
    pub restored_rows: u8,
    pub drawn_rows: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateOutcome {
    pub draw: Draw,
    pub landed: bool,
    pub support: Support,
    /// CS:164f starts the fence timer, even if controls leave it this update.
    pub fence_entered: bool,
    /// A lower-list hit occurred before a possible vertical transition.
    pub lower_support: bool,
    /// The room adapter actually drew a floor mark during this accepted update.
    pub floor_mark: bool,
    /// An airborne update ran the native support search before drawing.
    pub airborne_support_checked: bool,
    /// Grounded movement found a state-zero ordinary-room table before controls.
    pub room_surface_hit: bool,
    /// Entries visited before a grounded state-zero or airborne room-table hit.
    /// The bookcase's special shelf path and non-table results report zero.
    pub room_surface_entries: u8,
    pub hearts_clipping: Option<HeartsClippingWork>,
    /// The original 070d edge initializer ran during this update.
    pub edge_reset: bool,
    /// The lower-support caller tested the bin cat on this update.
    pub bin_contact_checked: bool,
    /// Logical input written by the bin cat, retained until the next poll.
    pub forced_controls: Option<Controls>,
    /// A native projectile checkpoint interrupted movement before drawing.
    pub projectile_contact: bool,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub x: u16,
    pub y: u8,
    pub vertical_coordinate: u8,
    pub horizontal_direction: u8,
    pub vertical_direction: u8,
    pub horizontal_step: u16,
    pub vertical_step: u8,
    pub phase: u8,
    pub phase_decrement: u8,
    /// Original upper-support state: 0 free, 1 newly attached, 2 attached.
    pub support_state: u8,
    /// Original band-96 state at data offset 0550.
    pub band_state: u8,
    pub animation_frame: u8,
    previous_direction: u8,
    support_delay: u8,
    support_disabled: u8,
    pub entry_state: u8,
    pub entry_delay: u8,
    alternate_animation: u8,
    alternate_phase: u16,
    idle_counter: u8,
    previous_dimensions: u16,
    /// CS:0583 suppresses restoring a player image while it is fully clipped.
    image_hidden: bool,
    airborne_sprite: Sprite,
    last_support: Support,
    room_exit_requested: bool,
    /// Scene-four support-table hole identity at DS:39e0 (zero off a hole).
    hole_marker: u8,
}

fn word(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

impl Player {
    /// The visible saved image has the retained transition shape, without actor animation.
    pub(crate) fn has_retained_airborne_image(&self) -> bool {
        !self.image_hidden
            && self.alternate_animation == 0
            && self.previous_dimensions == self.airborne_sprite.dimensions
    }

    /// Complete twelve-row image covered by the fence-fall caller timing fit.
    pub(crate) fn has_retained_fence_fall_image(&self) -> bool {
        self.band_state == 2
            && self.previous_dimensions == 0x0c03
            && self.has_retained_airborne_image()
    }

    /// Saved walking/idle shapes covered by ordinary supported-idle fits.
    pub(crate) fn has_retained_idle_image(&self) -> bool {
        !self.image_hidden
            && self.alternate_animation == 0
            && matches!(self.previous_dimensions, 0x0b03 | 0x0c02)
    }

    pub(crate) fn has_retained_walking_image(&self) -> bool {
        !self.image_hidden && self.alternate_animation == 0 && self.previous_dimensions == 0x0b03
    }

    /// Selected lower-edge clipping shape on the Hearts player path.
    pub(crate) fn is_hearts_ten_row_clip(&self, sprite: Sprite) -> bool {
        self.alternate_animation == 0
            && self.y == 189
            && self.airborne_sprite.dimensions == 0x0c03
            && sprite.source == self.airborne_sprite.source
            && sprite.dimensions == 0x0a03
    }

    fn hearts_retained_clip_rows(&self) -> Option<u8> {
        if self.image_hidden
            || self.alternate_animation != 0
            || !matches!(self.airborne_sprite.dimensions, 0x0c03 | 0x0d03)
            || self.y != self.vertical_coordinate.saturating_sub(50)
        {
            return None;
        }
        let rows = (self.airborne_sprite.dimensions >> 8) as u8;
        let hidden = 50u8.saturating_sub(self.vertical_coordinate) + self.y.saturating_sub(187);
        let visible = rows.checked_sub(hidden).filter(|rows| *rows != 0)?;
        (self.previous_dimensions == (u16::from(visible) << 8 | 3)).then_some(visible)
    }

    /// Whether drawing retains the complete transition image without actor animation.
    pub(crate) fn is_unclipped_airborne_sprite(&self, sprite: Sprite) -> bool {
        self.alternate_animation == 0 && sprite == self.airborne_sprite
    }

    /// CS:2567 drops an attached cat without resetting its airborne phase,
    /// vertical step, or sprite history, and without removing a life.
    pub(crate) fn knock_down_from_mouse(&mut self) {
        self.support_state = 0;
        self.support_delay = 17;
        self.vertical_direction = 1;
        self.horizontal_direction = 0;
        self.last_support = Support::None;
    }

    pub(crate) fn clothes_motion_disabled(&self) -> bool {
        self.support_disabled != 0
    }

    /// CS:04a0 carries an attached cat with its garment. At either edge it
    /// releases support without losing a life or moving X beyond the boundary.
    pub(crate) fn carry_with_clothes(&mut self, left: bool) {
        let next = if left {
            self.x.checked_sub(4).filter(|x| *x >= 8)
        } else {
            Some(self.x.wrapping_add(4)).filter(|x| *x < 0x123)
        };
        if let Some(next) = next {
            self.x = next;
        } else {
            self.support_delay = 17;
            self.vertical_direction = 1;
            self.vertical_step = 1;
            self.phase_decrement = 0x18;
            self.horizontal_step = (self.horizontal_step & 0xff00) | 1;
            self.support_state = 0;
            self.last_support = Support::None;
        }
    }

    /// Start after the original alley entry has completed.
    pub fn at_alley_floor(x: u16) -> Self {
        Self {
            x: x.clamp(8, 290),
            y: 180,
            vertical_coordinate: 230,
            horizontal_direction: 0,
            vertical_direction: 0,
            horizontal_step: 2,
            vertical_step: 1,
            phase: 8,
            phase_decrement: 0,
            support_state: 0,
            band_state: 0,
            animation_frame: 0,
            previous_direction: 0,
            support_delay: 0,
            support_disabled: 0,
            entry_state: 0,
            entry_delay: 0,
            alternate_animation: 0,
            alternate_phase: 0,
            idle_counter: 0,
            previous_dimensions: 0,
            image_hidden: false,
            airborne_sprite: Sprite {
                source: 0,
                dimensions: 0,
            },
            last_support: Support::Floor,
            room_exit_requested: false,
            hole_marker: 0,
        }
    }

    /// Import state from a data-segment capture. No original data is embedded.
    pub fn from_data(data: &[u8]) -> Result<Self, &'static str> {
        Self::check_data(data)?;
        if word(data, 4) > 7 {
            return Err("player core supports scene numbers zero through seven only");
        }
        Ok(Self {
            x: word(data, 0x579),
            y: data[0x57b],
            vertical_coordinate: data[0x57c],
            horizontal_direction: data[0x56e],
            previous_direction: data[0x56f],
            vertical_direction: data[0x571],
            horizontal_step: word(data, 0x572),
            vertical_step: data[0x576],
            phase: data[0x577],
            phase_decrement: data[0x578],
            support_state: data[0x55c],
            band_state: data[0x550],
            animation_frame: data[0x56b],
            support_delay: data[0x55b],
            support_disabled: data[0x55a],
            entry_state: data[0x558],
            entry_delay: data[0x559],
            alternate_animation: data[0x584],
            alternate_phase: word(data, 0x585),
            idle_counter: data[0x56d],
            previous_dimensions: word(data, 0x561),
            image_hidden: data[0x583] != 0,
            airborne_sprite: Sprite {
                source: word(data, 0x569),
                dimensions: word(data, 0x567),
            },
            last_support: Support::None,
            room_exit_requested: false,
            hole_marker: data.get(0x39e0).copied().unwrap_or(0),
        })
    }

    /// Mirror owned movement state for original-style room actor helpers.
    pub(crate) fn write_data(&self, data: &mut [u8]) -> Result<(), &'static str> {
        Self::check_data(data)?;
        for (offset, value) in [
            (0x579, self.x),
            (0x572, self.horizontal_step),
            (0x561, self.previous_dimensions),
            (0x569, self.airborne_sprite.source),
            (0x567, self.airborne_sprite.dimensions),
            (0x585, self.alternate_phase),
        ] {
            crate::assets::set_word(data, offset, value);
        }
        for (offset, value) in [
            (0x57b, self.y),
            (0x57c, self.vertical_coordinate),
            (0x56e, self.horizontal_direction),
            (0x56f, self.previous_direction),
            (0x571, self.vertical_direction),
            (0x576, self.vertical_step),
            (0x577, self.phase),
            (0x578, self.phase_decrement),
            (0x55c, self.support_state),
            (0x550, self.band_state),
            (0x56b, self.animation_frame),
            (0x55b, self.support_delay),
            (0x55a, self.support_disabled),
            (0x558, self.entry_state),
            (0x559, self.entry_delay),
            (0x584, self.alternate_animation),
            (0x56d, self.idle_counter),
            (0x583, u8::from(self.image_hidden)),
        ] {
            data[offset] = value;
        }
        if data.len() > 0x39e0 {
            data[0x39e0] = self.hole_marker;
        }
        Ok(())
    }

    pub(crate) fn apply_actor_state(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let mut updated = Self::from_data(data)?;
        updated.last_support = if updated.vertical_direction != 0 && updated.support_state == 0 {
            Support::None
        } else {
            self.last_support
        };
        *self = updated;
        Ok(())
    }

    pub(crate) fn take_room_exit(&mut self) -> bool {
        std::mem::take(&mut self.room_exit_requested)
    }

    pub(crate) fn floor_track_due(&self, controls: Controls) -> bool {
        self.entry_state == 0
            && self.support_state == 0
            && self.vertical_direction == 0
            && self.y >= 180
            && controls.y != -1
            && controls.x != 0
    }

    fn check_data(data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 0x1028 {
            Err("external data segment is too short for alley player tables")
        } else if matches!(word(data, 4), 1..=7) && data.len() < crate::assets::DATA_LENGTH {
            Err("external data segment is too short for room player tables")
        } else {
            Ok(())
        }
    }

    /// Advance exactly one accepted update. This does not poll a host clock.
    pub fn update(
        &mut self,
        controls: Controls,
        data: &[u8],
    ) -> Result<UpdateOutcome, &'static str> {
        self.update_with_projectiles(controls, data, false)
    }

    /// Stop at the original projectile checkpoints before drawing or clipping.
    /// The caller commits the contact response using the resulting position.
    pub(crate) fn update_with_projectiles(
        &mut self,
        mut controls: Controls,
        data: &[u8],
        projectile_enabled: bool,
    ) -> Result<UpdateOutcome, &'static str> {
        Self::check_data(data)?;
        self.room_exit_requested = false;
        if word(data, 4) == 2 {
            return Err("Swimming requires the timed underwater player path");
        }
        if !(-1..=1).contains(&controls.x) || !(-1..=1).contains(&controls.y) {
            return Err("control directions must be -1, 0, or 1");
        }
        if self.alternate_animation != 0 && !matches!(word(data, 4), 1..=7) {
            return Err("collision-effect animation is outside the player core");
        }
        let mut outcome = UpdateOutcome {
            draw: Draw::Keep,
            landed: false,
            support: self.last_support,
            fence_entered: false,
            lower_support: false,
            floor_mark: false,
            airborne_support_checked: false,
            room_surface_hit: false,
            room_surface_entries: 0,
            hearts_clipping: None,
            edge_reset: false,
            bin_contact_checked: false,
            forced_controls: None,
            projectile_contact: false,
        };
        // CS:092d..0939 gates all movement during a paired-hole transfer.
        // Its sprite and background are owned by the separate room helper.
        if word(data, 4) == 4 && data[0x39e1] != 0 {
            return Ok(outcome);
        }
        // CS:093b..0947: drinking owns the cat image and suppresses ordinary
        // movement after the caller has committed its accepted tick/countdown.
        if word(data, 4) == 6 && data[0x44bd] != 0 {
            return Ok(outcome);
        }
        // CS:0bac precedes death, entry and supported-state early returns.
        if self.projectile_checkpoint(data, &mut outcome, projectile_enabled) {
            return Ok(outcome);
        }
        if data.get(0x1cb8).copied().unwrap_or(0) != 0 {
            return Ok(outcome);
        }
        if self.entry_state != 0 {
            if self.entry_delay != 0 {
                if data.get(0x1cbf).copied().unwrap_or(0) == 0 {
                    self.entry_delay -= 1;
                }
                return Ok(outcome);
            }
            self.entry_state = self.entry_state.wrapping_sub(1);
            if self.entry_state != 0 {
                let mut sprite = self.select_walk_sprite(data);
                let crop_words = self.entry_state;
                sprite.dimensions = 0x0b00 | u16::from(3u8.wrapping_sub(crop_words));
                if self.horizontal_direction == 255 {
                    self.x = 296 + u16::from(crop_words) * 8;
                } else {
                    self.x = 0;
                    sprite.source = sprite.source.wrapping_add(u16::from(crop_words) * 2);
                }
                // CS:0c00 follows entry cropping, before the new image is drawn.
                if self.projectile_checkpoint(data, &mut outcome, projectile_enabled) {
                    return Ok(outcome);
                }
                self.previous_dimensions = sprite.dimensions;
                self.image_hidden = false;
                outcome.draw = Draw::StridedSprite {
                    sprite,
                    stride_bytes: 6,
                };
                return Ok(outcome);
            }
            self.horizontal_step = 8;
            self.advance_x(data);
            // The original falls through to ordinary controls on this same update.
        }
        if self.support_state != 0 {
            if self.support_state == 1 {
                self.support_state = 2;
                self.horizontal_step = 6;
                outcome.draw = self.draw_sprite(Sprite {
                    source: 0x9da,
                    dimensions: 0x0e03,
                });
            }
            if controls.y != 0 {
                self.request_transition(controls, data);
                outcome.support = Support::None;
            }
            return Ok(outcome);
        }
        if self.vertical_direction != 0 {
            self.advance_airborne(data, &mut outcome, projectile_enabled)?;
            return Ok(outcome);
        }

        if self.y < 180 || word(data, 4) == 7 {
            let was_on_fence = self.band_state == 1;
            let (support, entries) = self.check_support(data)?;
            outcome.fence_entered = support == Support::Band96 && !was_on_fence;
            outcome.room_surface_entries = entries;
            if support == Support::None {
                self.horizontal_direction = 0;
                self.vertical_direction = 1;
                self.start_fall(data);
                self.last_support = Support::None;
                outcome.support = Support::None;
                return Ok(outcome);
            }
            self.last_support = support;
            outcome.room_surface_hit = matches!(word(data, 4), 1 | 3..=6)
                && support == Support::RoomTable
                && self.support_state == 0;
            outcome.lower_support = support == Support::LowerList;
            outcome.bin_contact_checked = word(data, 4) == 0;
            if outcome.bin_contact_checked && crate::bin_cat::overlaps(data, self) {
                // Predict the one shared draw without mutating the read-only
                // player tables. Alley commits it before any later consumer.
                let random = crate::assets::random_successor(word(data, 0x2ae5));
                controls = Controls {
                    x: if random & 1 != 0 { 1 } else { -1 },
                    y: 1,
                };
                outcome.forced_controls = Some(controls);
            }
        } else {
            self.last_support = Support::Floor;
        }
        outcome.support = self.last_support;
        self.previous_direction = self.horizontal_direction;
        self.horizontal_direction = controls.x as u8;
        self.vertical_direction = controls.y as u8;
        if self.vertical_direction != 0 {
            if self.vertical_direction == 1 && self.y >= 180 {
                self.vertical_direction = 0;
                self.alternate_animation = 0;
            } else {
                self.start_vertical_transition(data);
                outcome.support = Support::None;
                return Ok(outcome);
            }
        }
        self.advance_x(data);
        if self.horizontal_direction == 0 {
            self.horizontal_step = 2;
            self.phase = 8;
            if self.previous_dimensions == 0x0c02 {
                self.idle_counter = self.idle_counter.wrapping_add(1);
                if self.idle_counter & 7 != 0 {
                    return Ok(outcome);
                }
            }
            // CS:1069 tests contact after the idle phase gate and before RNG.
            if self.projectile_checkpoint(data, &mut outcome, projectile_enabled) {
                return Ok(outcome);
            }
            self.previous_dimensions = 0x0c02;
            self.image_hidden = false;
            outcome.draw = Draw::Idle;
        } else {
            let sprite = self.select_walk_sprite(data);
            // CS:0f6d follows horizontal motion and walking sprite selection.
            if self.projectile_checkpoint(data, &mut outcome, projectile_enabled) {
                return Ok(outcome);
            }
            outcome.draw = self.draw_sprite(sprite);
        }
        Ok(outcome)
    }

    fn projectile_checkpoint(
        &self,
        data: &[u8],
        outcome: &mut UpdateOutcome,
        enabled: bool,
    ) -> bool {
        if enabled && crate::projectiles::overlaps(data, self) {
            outcome.projectile_contact = true;
            outcome.draw = Draw::Keep;
            true
        } else {
            false
        }
    }

    fn draw_sprite(&mut self, sprite: Sprite) -> Draw {
        self.previous_dimensions = sprite.dimensions;
        self.image_hidden = false;
        Draw::Sprite(sprite)
    }

    fn select_walk_sprite(&mut self, data: &[u8]) -> Sprite {
        if self.horizontal_direction != self.previous_direction {
            self.horizontal_step = 2;
        }
        if self.horizontal_step < 8 {
            self.phase = self.phase.wrapping_sub(1);
            if self.phase & 3 == 0 {
                self.horizontal_step += 1;
            }
        }
        self.animation_frame = self.animation_frame.wrapping_add(1);
        if self.animation_frame >= 6 {
            self.animation_frame = 0;
        }
        let index = usize::from(self.animation_frame)
            + if self.horizontal_direction == 255 {
                6
            } else {
                0
            };
        Sprite {
            source: word(data, 0xf7a + index * 2),
            dimensions: 0x0b03,
        }
    }

    fn initialize_alley_edge(&mut self) {
        let right_edge = self.x >= 160;
        self.x = if right_edge { 296 } else { 0 };
        self.horizontal_direction = if right_edge { 255 } else { 1 };
        self.y = 180;
        self.vertical_coordinate = 230;
        self.entry_state = 3;
        self.entry_delay = 12;
        self.previous_dimensions = 0x0b03;
        self.vertical_direction = 0;
        self.horizontal_step = 2;
        self.vertical_step = 1;
        self.support_delay = 0;
        self.band_state = 0;
        self.support_state = 0;
        self.support_disabled = 0;
        self.alternate_animation = 0;
        self.image_hidden = false;
        self.last_support = Support::Floor;
    }

    fn request_transition(&mut self, controls: Controls, data: &[u8]) {
        self.previous_direction = self.horizontal_direction;
        self.horizontal_direction = controls.x as u8;
        self.vertical_direction = controls.y as u8;
        self.start_vertical_transition(data);
    }

    fn start_vertical_transition(&mut self, data: &[u8]) {
        if self.vertical_direction == 1 {
            self.start_fall(data);
        } else {
            self.support_delay = 0;
            let old_step = self.horizontal_step as u8;
            if old_step > 2 {
                // The original changes AL only, preserving the high byte.
                self.horizontal_step =
                    (self.horizontal_step & 0xff00) | u16::from(old_step.wrapping_sub(2));
            }
            self.vertical_step = 8;
            self.phase_decrement = (old_step ^ 15).wrapping_shl(4);
            if self.band_state == 1 {
                self.band_state = 2;
            }
            self.initialize_transition_sprite(data);
        }
    }

    fn start_fall(&mut self, data: &[u8]) {
        self.vertical_step = 1;
        self.phase_decrement = 0x20;
        self.support_delay = 8;
        if self.band_state == 1 {
            self.band_state = 0;
        }
        self.initialize_transition_sprite(data);
    }

    fn initialize_transition_sprite(&mut self, data: &[u8]) {
        self.phase = 1;
        self.support_state = 0;
        self.hole_marker = 0;
        let direction = self.horizontal_direction.wrapping_add(1).wrapping_mul(2);
        let index = usize::from(direction) + if self.vertical_direction == 255 { 0 } else { 6 };
        self.airborne_sprite = Sprite {
            source: word(data, 0xfaa + index),
            dimensions: word(data, 0xfb6 + index),
        };
        self.last_support = Support::None;
    }

    /// CS:0fc9 returns carry when movement crosses a scene's horizontal limit.
    /// Scene seven keeps the cat inside the decorative cupid columns.
    fn advance_x(&mut self, data: &[u8]) -> bool {
        let (left, right_exclusive) = if word(data, 4) == 7 {
            (36, 271)
        } else {
            (8, 291)
        };
        match self.horizontal_direction {
            0 => false,
            1 => {
                let next = self.x.wrapping_add(self.horizontal_step);
                self.x = if next >= right_exclusive {
                    right_exclusive - 1
                } else {
                    next
                };
                next >= right_exclusive
            }
            _ => {
                let (next, borrow) = self.x.overflowing_sub(self.horizontal_step);
                self.x = if borrow || next < left { left } else { next };
                borrow || next < left
            }
        }
    }

    fn advance_airborne(
        &mut self,
        data: &[u8],
        outcome: &mut UpdateOutcome,
        projectile_enabled: bool,
    ) -> Result<(), &'static str> {
        let restored_rows = if word(data, 4) == 7 {
            self.hearts_retained_clip_rows()
        } else {
            None
        };
        if self.advance_x(data) {
            self.horizontal_direction = 0;
            self.vertical_step = 2;
            self.vertical_direction = 1;
            self.support_delay = 0;
        } else {
            let (phase, borrow) = self.phase.overflowing_sub(self.phase_decrement);
            self.phase = phase;
            if borrow {
                if self.vertical_direction == 1 {
                    if self.vertical_step < 4 {
                        self.vertical_step += 1;
                    }
                } else if self.vertical_step > 1 {
                    self.vertical_step -= 1;
                } else {
                    self.vertical_direction = 1;
                }
            }
        }
        let mut support = Support::None;
        if self.support_disabled == 0 {
            if self.support_delay != 0 {
                self.support_delay -= 1;
            }
            if self.support_delay == 0 && self.vertical_direction == 1 {
                outcome.airborne_support_checked = true;
                let was_on_fence = self.band_state == 1;
                let (hit, entries) = self.check_support(data)?;
                outcome.fence_entered = hit == Support::Band96 && !was_on_fence;
                support = hit;
                outcome.room_surface_entries = entries;
            }
        }
        if support != Support::None {
            self.finish_landing();
        } else if self.vertical_direction == 1 {
            let next = self.vertical_coordinate.wrapping_add(self.vertical_step);
            if word(data, 4) == 7 {
                // CS:0d0e..0d20: scene seven has no floor at Y180. Falling
                // continues below its bottom hearts until coordinate248.
                self.vertical_coordinate = next.min(248);
                if next >= 248 {
                    self.room_exit_requested = true;
                }
            } else if next > 230 {
                self.vertical_coordinate = 230;
                self.band_state = 0;
                self.finish_landing();
                support = Support::Floor;
            } else {
                self.vertical_coordinate = next;
            }
        } else {
            let (next, borrow) = self.vertical_coordinate.overflowing_sub(self.vertical_step);
            self.vertical_coordinate = if borrow { 0 } else { next };
            if borrow {
                self.vertical_direction = 1;
                self.vertical_step = 1;
            }
        }
        self.y = self.vertical_coordinate.saturating_sub(50);
        self.last_support = support;
        outcome.support = support;
        outcome.landed = support != Support::None;
        outcome.lower_support = support == Support::LowerList;
        // CS:0d73 checks the updated position before alternate animation,
        // top/band clipping and the alley-edge reset at CS:0e1b.
        if self.projectile_checkpoint(data, outcome, projectile_enabled) {
            return Ok(());
        }
        let mut sprite = self.airborne_sprite;
        if self.alternate_animation != 0 {
            self.alternate_phase = self.alternate_phase.wrapping_add(2);
            let index = usize::from(self.alternate_phase & 14);
            sprite = Sprite {
                source: word(data, 0xfc2 + index),
                dimensions: word(data, 0xfd2 + index),
            };
        }
        let mut clipping = None;
        let hidden_rows = 50u8.saturating_sub(self.vertical_coordinate);
        if hidden_rows != 0 {
            let rows = (sprite.dimensions >> 8) as u8;
            if hidden_rows >= rows {
                self.image_hidden = true;
                outcome.draw = Draw::Erase;
                outcome.hearts_clipping = restored_rows.map(|restored_rows| HeartsClippingWork {
                    kind: HeartsClipKind::Hidden,
                    restored_rows,
                    drawn_rows: 0,
                });
                return Ok(());
            }
            sprite.source = if self.alternate_animation != 0 {
                self.airborne_sprite.source
            } else {
                sprite.source
            }
            .wrapping_add(u16::from(hidden_rows) * (sprite.dimensions & 255) * 2);
            sprite.dimensions -= u16::from(hidden_rows) << 8;
            clipping = Some(HeartsClipKind::Top);
        }
        // CS:0dde..0e1a clips below Y187 in scene seven, versus Y94 for
        // the alley's band-two fall. Only the latter resets to an alley edge.
        let clip_y = if word(data, 4) == 7 {
            Some(187)
        } else if word(data, 4) == 0 && self.band_state == 2 {
            Some(94)
        } else {
            None
        };
        if let Some(clip_y) = clip_y.filter(|boundary| self.y >= *boundary) {
            let rows = (sprite.dimensions >> 8) as u8;
            let hidden_rows = self.y - clip_y;
            if hidden_rows >= rows {
                if word(data, 4) == 7 {
                    self.room_exit_requested = true;
                    outcome.draw = Draw::Erase;
                    return Ok(());
                }
                self.initialize_alley_edge();
                outcome.draw = Draw::Erase;
                outcome.edge_reset = true;
                outcome.support = Support::Floor;
                return Ok(());
            }
            sprite.dimensions -= u16::from(hidden_rows) << 8;
            self.vertical_step = 2;
            if hidden_rows != 0 {
                clipping = Some(HeartsClipKind::Bottom);
            }
        }
        outcome.draw = self.draw_sprite(sprite);
        if let (Some(kind), Some(restored_rows)) = (clipping, restored_rows) {
            outcome.hearts_clipping = Some(HeartsClippingWork {
                kind,
                restored_rows,
                drawn_rows: (sprite.dimensions >> 8) as u8,
            });
        }
        Ok(())
    }

    fn finish_landing(&mut self) {
        self.vertical_direction = 0;
        self.horizontal_step = 2;
        self.support_delay = 0;
        self.support_disabled = 0;
        self.alternate_animation = 0;
    }

    fn check_support(&mut self, data: &[u8]) -> Result<(Support, u8), &'static str> {
        if word(data, 4) == 0 {
            return Ok((self.check_alley_support(data), 0));
        }
        if word(data, 4) != 7 {
            self.hole_marker = 0;
        }
        let check = if word(data, 4) == 7 {
            crate::hearts::resolve_support(data, self)
        } else {
            crate::room::resolve_scene_support(data, self)
        }
        .map_err(|_| "invalid room support tables")?;
        self.room_exit_requested |= check.exit;
        if let Some(hit) = check.hit {
            self.y = hit.y;
            self.vertical_coordinate = hit.y.wrapping_add(50);
            self.support_state = hit.state;
            if hit.state != 0 {
                self.x &= 0xfffc;
            }
            if word(data, 4) == 4 && (39..55).contains(&hit.index) {
                self.hole_marker = (hit.index - 38) as u8;
            }
            let scene = usize::from(word(data, 4));
            let count_table =
                hit.state == 0 || (self.vertical_direction == 1 && matches!(scene, 1 | 4..=6));
            let entries = if matches!(scene, 1 | 3..=6) && count_table {
                // The resolver returns the first hit, so its index includes
                // every visited entry from this room's native table start.
                (hit.index - usize::from(word(data, 0x1269 + scene * 2)) + 1) as u8
            } else {
                0
            };
            Ok((Support::RoomTable, entries))
        } else {
            Ok((Support::None, 0))
        }
    }

    fn check_alley_support(&mut self, data: &[u8]) -> Support {
        let band = self.y & 0xf8;
        if band == 0x60 {
            if self.band_state >= 2 {
                return Support::None;
            }
            self.y = band;
            self.vertical_coordinate = band.wrapping_add(50);
            self.band_state = 1;
            return Support::Band96;
        }
        let list_band = self.y.wrapping_add(2) & 0xf8;
        let difficulty = usize::from(word(data, 8));
        // Eight difficulty entries select four shared, terminated bin lists.
        if difficulty < 8 {
            let mut index = usize::from(data[0x1006 + difficulty]);
            // The accepted input's lists end before their index table.
            while 0xff0 + index < 0x1006 {
                let entry = data[0xff0 + index];
                if entry == 0 {
                    break;
                }
                index += 1;
                let target_band = if entry & 0x80 != 0 { 0x88 } else { 0x90 };
                let target_x = u16::from(entry & 0x7f) * 4;
                if list_band == target_band
                    && (self.x & 0xfff8) >= target_x
                    && (self.x.wrapping_sub(15) & 0xfff8) <= target_x
                {
                    self.y = target_band - 2;
                    self.vertical_coordinate = self.y + 50;
                    return Support::LowerList;
                }
            }
        }
        let row = match band {
            0x08 => 0,
            0x28 => 1,
            0x48 => 2,
            _ => return Support::None,
        };
        let mut x = self.x;
        if row == usize::from(word(data, 0x52f)) && data[0x525] <= 3 {
            x = if row == 1 {
                x.wrapping_sub(u16::from(data[0x525] + 1) * 4)
            } else {
                x.wrapping_add(u16::from(4 - data[0x525]) * 4)
            };
        }
        let position = x.wrapping_add(10);
        let byte_index = usize::from(position >> 6) + usize::from(data[0x1025 + row]);
        let mask = 0x80 >> ((position >> 3) & 7);
        if data
            .get(0x1016 + byte_index)
            .is_some_and(|byte| byte & mask != 0)
        {
            self.y = band;
            self.vertical_coordinate = band + 50;
            self.x &= 0xfff8;
            self.support_state = 1;
            Support::UpperBitfield
        } else {
            Support::None
        }
    }
}
