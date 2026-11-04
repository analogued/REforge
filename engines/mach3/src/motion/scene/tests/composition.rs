// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Whole-scene oracle over authored observations, independent of the production
//! painter, projection, gate geometry and clipping helpers. Expected positions
//! are specified in screen pixels; fixture input uses depth 64 (scale 1/4).
//! This validates scene selection/composition, not original artwork or timing.

use super::*;

const W: usize = 320;
const H: usize = 200;

struct Art {
    width: usize,
    height: usize,
    offset: [i16; 2],
    pixels: Vec<(u8, u8)>, // Unpacked (mask, colour), including non-alpha masks.
}

impl Art {
    fn new(width: usize, height: usize, offset: [i16; 2], seed: usize) -> Self {
        let pixels = (0..width * height)
            .map(|i| {
                let (x, y) = (i % width, i / width);
                (((x + y + seed) % 4) as u8, ((x / 2 + y + seed) % 4) as u8)
            })
            .collect();
        Self {
            width,
            height,
            offset,
            pixels,
        }
    }

    fn sprite(&self) -> Sprite {
        let mut descriptor = [0; 7];
        descriptor[5] = self.offset[0];
        descriptor[6] = self.offset[1];
        Sprite {
            width_bytes: self.width / 4,
            height: self.height,
            descriptor,
            color: pack(self.pixels.iter().map(|p| p.1)),
            mask: pack(self.pixels.iter().map(|p| p.0)),
        }
    }
}

// Fixture encoding only. Expected images stay unpacked throughout painting.
fn pack(pixels: impl IntoIterator<Item = u8>) -> Vec<u8> {
    let values: Vec<_> = pixels.into_iter().collect();
    values
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0] * 64 + p[1] * 16 + p[2] * 4 + p[3])
        .collect()
}

fn background() -> Vec<u8> {
    (0..W * H)
        .map(|i| ((i % W / 7 + i / W / 5) % 4) as u8)
        .collect()
}

struct Oracle {
    pixels: Vec<u8>,
}

impl Oracle {
    fn new() -> Self {
        Self {
            pixels: background(),
        }
    }

    fn sprite(&mut self, art: &Art, at: [i32; 2], rows: std::ops::Range<usize>) {
        for sy in rows {
            for sx in 0..art.width {
                let (x, y) = (at[0] + sx as i32, at[1] + sy as i32);
                // Explicit viewport contract; no production rectangle helper.
                if !(16..304).contains(&x) || !(44..158).contains(&y) {
                    continue;
                }
                let pixel = &mut self.pixels[y as usize * W + x as usize];
                let (mask, color) = art.pixels[sy * art.width + sx];
                // Independent per-bit truth rules, rather than a packed blit.
                for bit in [1, 2] {
                    if color & bit != 0 {
                        *pixel |= bit;
                    } else if mask & bit == 0 {
                        *pixel &= !bit;
                    }
                }
            }
        }
    }

    fn gate(&mut self, at: [i32; 2]) {
        // Authored gate: two-pixel stroke, twenty-pixel inner span and a
        // twenty-eight-pixel height. Union membership avoids sharing the
        // production rectangle construction or its repeated corner writes.
        for y in 44..158 {
            for x in 16..304 {
                let (gx, gy) = (x - at[0], y - at[1]);
                let top = (0..22).contains(&gx) && (0..2).contains(&gy);
                let legs =
                    ((0..2).contains(&gx) || (20..22).contains(&gx)) && (0..29).contains(&gy);
                if top || legs {
                    self.pixels[y as usize * W + x as usize] = if x % 2 == 0 { 2 } else { 1 };
                }
            }
        }
    }

    fn assert_frame(&self, frame: &Frame, label: &str) {
        let mut count = 0;
        let mut first = Vec::new();
        // Scan all 64,000 pixels; retain at most twelve mismatches. No images
        // or successful framebuffer series are written to disk.
        for (i, &expected) in self.pixels.iter().enumerate() {
            let byte = frame.packed()[i / 4];
            let actual = (byte / [64, 16, 4, 1][i % 4]) % 4;
            if actual != expected {
                count += 1;
                if first.len() < 12 {
                    first.push((i % W, i / W, expected, actual));
                }
            }
        }
        assert_eq!(count, 0, "{label}: (x, y, expected, actual) {first:?}");
    }
}

struct Fixture {
    assets: GraphicsAssets,
    art: BTreeMap<usize, Art>,
    shift: [i32; 2],
}

impl Fixture {
    fn new(shift: [i32; 2]) -> Self {
        let mut assets = assets();
        let art = BTreeMap::from([
            (13, Art::new(16, 10, [2, 3], 0)),
            (12, Art::new(24, 14, [3, 6], 1)),
            (80, Art::new(20, 12, [1, 2], 2)),
            // Shadow X deliberately differs: rendering must use craft X's anchor.
            (18, Art::new(20, 5, [7, 1], 3)),
            (22, Art::new(20, 12, [1, 2], 1)),
            (23, Art::new(8, 6, [1, 3], 2)),
            (24, Art::new(12, 8, [0, 2], 3)),
            (90, Art::new(8, 3, [0, 0], 1)),
        ]);
        for (&source, image) in &art {
            assets.sprites[source] = image.sprite();
        }
        Self { assets, art, shift }
    }

    fn at(&self, x: i32, y: i32) -> [i32; 2] {
        [x + self.shift[0], y + self.shift[1]]
    }

    fn anchors(&self) -> [f64; 3] {
        [
            172.0 + f64::from(self.shift[0]),
            104.0 + f64::from(self.shift[1]),
            110.0 + f64::from(self.shift[1]),
        ]
    }

    fn layer(&self, slot: usize, source: usize, at: [i32; 2], partial: bool) -> Layer {
        let art = &self.art[&source];
        let sprite = art.sprite();
        let mut layer = Layer {
            slot,
            source,
            rect: Rect {
                x_bytes: at[0].div_euclid(4),
                y: at[1],
                width_bytes: art.width / 4,
                height: art.height,
            },
            color: sprite.color,
            mask: sprite.mask,
        };
        if partial {
            // A partial native pass carries neutralized rows, but Reforged's
            // eligible actor must still select complete artwork from assets.
            let end = (art.height - 2) * layer.rect.width_bytes;
            layer.color[..end].fill(0);
            layer.mask[..end].fill(255);
        }
        layer
    }

    fn object(&self, source: usize, at: [i32; 2], movement: u8) -> Object {
        let offset = self.art[&source].offset;
        Object {
            position: [
                64.0,
                f64::from((at[0] + i32::from(offset[0]) * 4 - 160) * 4),
                f64::from((106 - at[1] - i32::from(offset[1])) * 4),
            ],
            active: true,
            movement,
            birth: 1,
            projected_birth: 1,
            gate: None,
        }
    }

    fn observation(&self, step: u64, craft: usize, phase: u16) -> Observation {
        let mut f = observation(step);
        f.pose.coordinates = [14, 0, 0];
        f.pose.controls = 0;
        f.pose.velocity = [0, 0];
        f.pose.horizon = 0;
        f.pose.origin = 106;
        f.pose.phase = phase;
        let art = &self.art[&craft];
        f.pose.source = [craft, art.width, art.height, 18, 20, 5];
        f.state.static_lateral = 0;
        f.state.speeds = [0; 3];
        f.scene.base = Frame::from_packed(&pack(background())).unwrap();
        f.objects.clear();
        f.objects.insert(5, self.object(22, self.at(154, 101), 5));
        f.objects.insert(2, self.object(23, self.at(168, 101), 254));
        let mut gate = f.state.special[0];
        gate.coordinates = [
            64,
            (-8 + self.shift[0] * 4) as i16,
            (-40 - self.shift[1] * 4) as i16,
        ];
        gate.extents = [28, 80, 112];
        gate.rect.width_bytes = 8;
        gate.requested_sprite = 4;
        f.objects.insert(
            23,
            Object {
                position: gate.coordinates.map(f64::from),
                active: true,
                movement: 1,
                birth: 0,
                projected_birth: 0,
                gate: Some(gate),
            },
        );
        let mut gate_layer = layer(23, 4, 36, 90);
        gate_layer.color.fill(255); // A wrong fallback leaves an obvious artifact.
        f.scene.layers = vec![
            self.layer(1, 18, self.at(164, 109), false),
            self.layer(5, 22, self.at(152, 101), !step.is_multiple_of(3)),
            self.layer(2, 23, self.at(168, 101), false),
            self.layer(0, craft, self.at(148, 92), false),
            gate_layer,
            self.layer(29, 90, self.at(172, 108), false),
        ];
        f
    }

    fn expected(
        &self,
        craft: usize,
        enemy: Option<(usize, [i32; 2], bool)>,
        gate: Option<[i32; 2]>,
        damage: bool,
        shot: bool,
    ) -> Oracle {
        let mut out = Oracle::new();
        let craft_x = if craft == 12 { 160 } else { 164 };
        out.sprite(
            &self.art[&18],
            self.at(if damage { 164 } else { craft_x }, 109),
            0..5,
        );
        if let Some((source, at, partial)) = enemy {
            let art = &self.art[&source];
            out.sprite(
                art,
                at,
                if partial {
                    art.height - 2..art.height
                } else {
                    0..art.height
                },
            );
        }
        if shot {
            out.sprite(&self.art[&23], self.at(168, 101), 0..6);
        }
        let art = &self.art[&craft];
        let at = if damage {
            self.at(148, 92)
        } else {
            self.at(craft_x, if craft == 12 { 98 } else { 101 })
        };
        out.sprite(art, at, 0..art.height);
        if let Some(at) = gate {
            out.gate(at);
        }
        out.sprite(&self.art[&90], self.at(172, 108), 0..3);
        out
    }
}

#[test]
fn whole_scene_matches_independent_painter_across_partial_passes_and_viewport_edges() {
    for dx in [-148, 0, 128] {
        for dy in [-60, 0, 50] {
            let fixture = Fixture::new([dx, dy]);
            let mut tracker = Tracker::default();
            let expected = fixture.expected(
                13,
                Some((22, fixture.at(154, 101), false)),
                Some(fixture.at(158, 88)),
                false,
                true,
            );
            for step in 0..3 {
                let f = fixture.observation(step, 13, 0);
                let at = f.at;
                tracker.observe(f, at, false);
                let frame = tracker
                    .render(at, Some(fixture.anchors()), &fixture.assets)
                    .unwrap()
                    .unwrap();
                expected.assert_frame(&frame, &format!("shift {dx},{dy}, pass {}", step + 1));
            }
        }
    }
}

#[test]
fn whole_scene_banking_damage_and_recovery_replace_artwork_without_fragments() {
    let fixture = Fixture::new([0, 0]);
    let mut tracker = Tracker::default();
    for (step, (craft, phase)) in [(13, 0), (12, 0), (80, 1), (13, 0)].into_iter().enumerate() {
        let f = fixture.observation(step as u64, craft, phase);
        let at = f.at;
        tracker.observe(f, at, false);
        let frame = tracker
            .render(at, Some(fixture.anchors()), &fixture.assets)
            .unwrap()
            .unwrap();
        fixture
            .expected(
                craft,
                Some((22, fixture.at(154, 101), false)),
                Some(fixture.at(158, 88)),
                phase != 0,
                true,
            )
            .assert_frame(&frame, &format!("artwork step {step}"));
    }
}

#[test]
fn whole_scene_retirement_cached_lifetime_and_resolved_reuse_select_correct_images() {
    let fixture = Fixture::new([0, 0]);
    let mut tracker = Tracker::default();
    for step in 0..6 {
        let mut f = fixture.observation(step, 13, 0);
        let mut enemy = Some((22, fixture.at(154, 101), false));
        if step == 1 {
            f.objects.get_mut(&5).unwrap().active = false;
            f.scene.layers.retain(|l| l.slot != 5);
            enemy = None;
        } else if step >= 2 {
            let mut replacement = fixture.object(24, fixture.at(170, 104), 5);
            replacement.birth = 2;
            replacement.projected_birth = if step == 2 { 1 } else { 2 };
            f.objects.insert(5, replacement);
            if step == 2 {
                enemy = Some((22, fixture.at(152, 101), true));
            } else {
                f.scene.layers[1] = fixture.layer(5, 24, fixture.at(168, 104), true);
                enemy = Some((24, fixture.at(170, 104), false));
            }
        }
        if step >= 3 {
            f.objects.get_mut(&2).unwrap().active = false;
            f.scene.layers.retain(|l| l.slot != 2);
        }
        let gate = if step == 4 {
            f.objects.get_mut(&23).unwrap().active = false;
            None
        } else if step == 5 {
            let o = f.objects.get_mut(&23).unwrap();
            o.position[1] -= 48.0;
            o.gate.as_mut().unwrap().coordinates[1] -= 48;
            Some(fixture.at(146, 88))
        } else {
            Some(fixture.at(158, 88))
        };
        let at = f.at;
        tracker.observe(f, at, false);
        let frame = tracker
            .render(at, Some(fixture.anchors()), &fixture.assets)
            .unwrap()
            .unwrap();
        fixture
            .expected(13, enemy, gate, false, step < 3)
            .assert_frame(&frame, &format!("lifetime step {step}"));
    }
}

#[test]
fn whole_scene_enemy_crossings_preserve_declared_painter_order() {
    let fixture = Fixture::new([0, 0]);
    // A known-position sweep, not a temporal smoothness test. Each scene
    // begins without prediction history, so expected locations stay authored.
    for x in [140, 152, 164, 176, 188] {
        let mut tracker = Tracker::default();
        let mut f = fixture.observation(0, 13, 0);
        f.objects.insert(5, fixture.object(22, [x, 101], 5));
        let at = f.at;
        tracker.observe(f, at, false);
        let frame = tracker
            .render(at, Some(fixture.anchors()), &fixture.assets)
            .unwrap()
            .unwrap();
        fixture
            .expected(
                13,
                Some((22, [x, 101], false)),
                Some([158, 88]),
                false,
                true,
            )
            .assert_frame(&frame, &format!("enemy crossing x={x}"));
    }
}
