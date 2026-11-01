// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Native diagnostic exports. Blank cells identify fields absent from the model.

use super::new_file;
use crate::{
    Error,
    frame::Rect,
    gameplay::{EndingSnapshot, EnemyState, SessionState, Snapshot, SpecialState},
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

pub(super) struct Diagnostics {
    state: BufWriter<File>,
    objects: BufWriter<File>,
    order: BufWriter<File>,
    session: BufWriter<File>,
    ending: BufWriter<File>,
}

impl Diagnostics {
    pub(super) fn new(destination: &Path) -> Result<Self, Error> {
        let mut state = new_file(&destination.join("state.tsv"))?;
        writeln!(
            state,
            "seq\tgame\tbody\tpass\tkind\tstrip_index\tstrip_y\tvertical_origin\trng_a\trng_b\trng_c\tscript_index\tscript_cursor\tscript_delay\tdifficulty\tprogression\tspeed_forward\tspeed_primary\tspeed_slow\tspeed_mode\tterrain_selector\tprimary_count\tenemy_target_lateral\tenemy_target_height\tfuel\tweapon_counter\tpending_score\tscore\tplayer_phase\tplayer_animation_row\tplayer_damage\tplayer_grace\tplayer_horizon\tplayer_velocity_lateral\tplayer_velocity_height\tplayer_orientation\tplayer_vertical_band\tplayer_input\tlives_requested\tlives_resolved\tmarker_requested\tmarker_previous\tmarker_width\tmarker_height\tbar_width\tbar_draw_width"
        )?;
        let mut objects = new_file(&destination.join("objects-normalized.tsv"))?;
        writeln!(
            objects,
            "seq\tgame\tbody\tpass\tkind\tslot\tdepth\tlateral\theight_world\textent_depth\textent_lateral\textent_height\tactive\tmovement\trequested_sprite\tresolved_sprite\tx_bytes\ty\twidth_bytes\theight\tphase\tscale\tanimation_phase\tvariant"
        )?;
        let mut order = new_file(&destination.join("draw-order.tsv"))?;
        writeln!(order, "seq\tgame\tbody\tpass\tkind\tposition\tslot")?;
        let mut session = new_file(&destination.join("session-state.tsv"))?;
        writeln!(
            session,
            "seq\tgame\tbody\tpass\tkind\trng_a\trng_b\trng_c\tbest_score\tdisplay_mode\tsound_disabled\tsound_poll_counter\tintro_audio_enabled"
        )?;
        let mut ending = new_file(&destination.join("ending-state.tsv"))?;
        writeln!(
            ending,
            "seq\tgame\tbody\tpass\tkind\ttitle\tstage\tdraw_index\thold_tick\tcycle\tbest_score\trng_a\trng_b\trng_c"
        )?;
        Ok(Self {
            session,
            ending,
            state,
            objects,
            order,
        })
    }

    pub(super) fn frame(
        &mut self,
        seq: u64,
        game: u64,
        body: u16,
        pass: u16,
        kind: &str,
        s: &Snapshot,
    ) -> Result<(), Error> {
        let prefix = format!("{seq}\t{game}\t{body}\t{pass}\t{kind}");
        let values: [i64; 41] = [
            s.strip_index as i64,
            s.strip_y as i64,
            s.vertical_origin.into(),
            s.random[0].into(),
            s.random[1].into(),
            s.random[2].into(),
            s.script_index.into(),
            s.script_cursor.into(),
            s.script_delay.into(),
            s.difficulty.into(),
            s.progression.into(),
            s.speeds[0].into(),
            s.speeds[1].into(),
            s.speeds[2].into(),
            s.speed_mode.into(),
            s.terrain_selector.into(),
            s.primary_count.into(),
            s.enemy_target[0].into(),
            s.enemy_target[1].into(),
            s.hud.fuel.into(),
            s.weapon.counter.into(),
            s.hud.pending_score.into(),
            s.hud.score.into(),
            s.player.phase.into(),
            s.player.animation_row.into(),
            s.player.damage.into(),
            s.player.grace.into(),
            s.player.horizon.into(),
            s.player.velocity[0].into(),
            s.player.velocity[1].into(),
            s.player.orientation.into(),
            s.player.vertical_band.into(),
            s.player.effective_input.into(),
            s.hud.lives_requested.into(),
            s.hud.lives_resolved.into(),
            s.hud.marker_requested.into(),
            s.hud.marker_previous.into(),
            s.hud.marker_width as i64,
            s.hud.marker_height as i64,
            s.hud.left_bar_width as i64,
            s.hud.left_bar_draw_width as i64,
        ];
        write!(self.state, "{prefix}")?;
        for value in values {
            write!(self.state, "\t{value}")?;
        }
        writeln!(self.state)?;
        let mut records: [Record; 29] = std::array::from_fn(|_| Record::default());
        records[0].coordinates(s.player.coordinates);
        records[0].sprite(
            s.player.requested_sprite,
            s.player.resolved_sprite,
            s.player.rect,
        );
        records[1].put(8, s.player.shadow_requested_sprite);
        records[1].put(9, s.player.shadow.resolved_sprite);
        records[1].rect(s.player.shadow.rect);
        for e in s.weapon.shots.iter().chain(&s.enemies) {
            records[e.slot].enemy(e);
        }
        for t in s.terrain {
            let r = &mut records[t.slot];
            r.put(0, t.depth);
            r.put(1, t.lateral);
            r.put(6, u8::from(t.active));
            r.put(7, t.movement);
            r.sprite(t.requested_sprite, t.resolved_sprite, t.rect);
            r.put(14, t.phase);
            r.put(16, t.animation_phase);
            r.put(17, t.kind);
        }
        for t in s.special {
            let r = &mut records[t.slot];
            r.coordinates([t.depth, t.lateral, t.height]);
            for (index, extent) in t.extents.iter().enumerate() {
                r.put(3 + index, extent);
            }
            r.put(6, u8::from(t.active));
            r.put(7, t.movement);
            r.sprite(t.requested_sprite, t.resolved_sprite, t.rect);
            r.put(14, t.phase);
            r.put(15, t.scale);
        }
        records[28].put(1, s.static_lateral);
        for (slot, record) in records.iter().enumerate() {
            write!(self.objects, "{prefix}\t{slot}")?;
            for cell in &record.0 {
                write!(self.objects, "\t{cell}")?;
            }
            writeln!(self.objects)?;
        }
        for (position, slot) in s.full_world_order.iter().enumerate() {
            writeln!(self.order, "{prefix}\t{position}\t{slot}")?;
        }
        Ok(())
    }

    pub(super) fn session(
        &mut self,
        prefix: &str,
        random: [u16; 3],
        session: &SessionState,
    ) -> Result<(), Error> {
        let mode = match session.controls.preferences.display_mode {
            crate::gameplay::DisplayMode::Color => 4,
            crate::gameplay::DisplayMode::Monochrome => 6,
        };
        writeln!(
            self.session,
            "{prefix}\t{}\t{}\t{}\t{}\t{mode}\t{}\t{}\t{}",
            random[0],
            random[1],
            random[2],
            session.best_score,
            u8::from(session.controls.preferences.sound_disabled),
            session.controls.sound_poll_counter,
            u8::from(session.controls.preferences.digitized_samples_enabled)
        )?;
        Ok(())
    }

    pub(super) fn ending(
        &mut self,
        prefix: &str,
        s: &EndingSnapshot,
        special: [SpecialState; 5],
    ) -> Result<(), Error> {
        // Ending has a separate world model, not a gameplay Snapshot. Keep a
        // keyed row for cross-phase comparisons and expose only known scalars.
        write!(self.state, "{prefix}")?;
        for index in 0..41 {
            if (3..6).contains(&index) {
                write!(self.state, "\t{}", s.random[index - 3])?;
            } else {
                write!(self.state, "\t")?;
            }
        }
        writeln!(self.state)?;
        writeln!(
            self.ending,
            "{prefix}\t{:?}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            s.title,
            s.stage,
            s.draw_index,
            s.hold_tick,
            s.cycle,
            s.best_score,
            s.random[0],
            s.random[1],
            s.random[2]
        )?;
        let mut records: [Record; 29] = std::array::from_fn(|index| {
            let mut record = Record::default();
            record.enemy(&s.world[index]);
            record
        });
        for t in special {
            for (index, extent) in t.extents.iter().enumerate() {
                records[t.slot].put(3 + index, extent);
            }
            records[t.slot].put(15, t.scale);
        }
        for (slot, record) in records.iter().enumerate() {
            write!(self.objects, "{prefix}\t{slot}")?;
            for cell in &record.0 {
                write!(self.objects, "\t{cell}")?;
            }
            writeln!(self.objects)?;
        }
        for (position, slot) in s.full_world_order.iter().enumerate() {
            writeln!(self.order, "{prefix}\t{position}\t{slot}")?;
        }
        Ok(())
    }

    pub(super) fn flush(&mut self) -> Result<(), Error> {
        self.session.flush()?;
        self.ending.flush()?;
        self.state.flush()?;
        self.objects.flush()?;
        self.order.flush()?;
        Ok(())
    }
}

#[derive(Default)]
struct Record([String; 18]);
impl Record {
    fn put(&mut self, index: usize, value: impl std::fmt::Display) {
        self.0[index] = value.to_string();
    }
    fn coordinates(&mut self, coordinates: [i16; 3]) {
        for (index, value) in coordinates.iter().enumerate() {
            self.put(index, value);
        }
    }
    fn rect(&mut self, rect: Rect) {
        self.put(10, rect.x_bytes);
        self.put(11, rect.y);
        self.put(12, rect.width_bytes);
        self.put(13, rect.height);
    }
    fn sprite(&mut self, requested: usize, resolved: usize, rect: Rect) {
        self.put(8, requested);
        self.put(9, resolved);
        self.rect(rect);
    }
    fn enemy(&mut self, e: &EnemyState) {
        self.coordinates([e.depth, e.lateral, e.height]);
        self.put(6, u8::from(e.active));
        self.put(7, e.movement);
        self.sprite(e.requested_sprite, e.resolved_sprite, e.rect);
        self.put(14, e.phase);
        self.put(16, e.animation_phase);
        self.put(17, e.variant);
    }
}
