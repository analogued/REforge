// Copyright (c) 2026 analogued. Project license has not been selected.
//! Background layers, clipped restoration, and expanding scene transitions.

use crate::{
    Error,
    assets::GraphicsAssets,
    frame::{FRAME_BYTES, Frame, HEIGHT, ROW_BYTES, Rect},
};

pub(crate) const VIEWPORT: Rect = Rect {
    x_bytes: 4,
    y: 44,
    width_bytes: 72,
    height: 114,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub alternate_background: bool,
    pub layer: u16,
    pub overlay_counter: u16,
}

#[derive(Clone, Debug)]
pub struct TransitionFrame {
    pub frame: Frame,
    pub clip: Rect,
    /// Number of bounded vertical retrace polling calls after this step.
    pub wait_count: u16,
    /// Whether this transition precedes the current ordinary render pass.
    pub before_pass: bool,
    pub palette_family: u8,
}

impl Presentation {
    pub fn world_layer(self) -> u16 {
        if self.overlay_counter as i16 > 0 {
            1
        } else {
            self.layer
        }
    }

    /// World passes share the layer selected at the beginning of the body.
    pub fn background(
        self,
        assets: &GraphicsAssets,
        strip_index: usize,
        strip_y: usize,
        render_pass: u8,
    ) -> Result<Frame, Error> {
        if !(1..=3).contains(&render_pass) {
            return Err(Error::InvalidData("invalid world render pass".into()));
        }
        self.background_with_layer(assets, strip_index, strip_y, self.world_layer())
    }

    /// HUD restoration uses layer zero; transitions use their new explicit layer.
    pub fn background_with_layer(
        self,
        assets: &GraphicsAssets,
        strip_index: usize,
        strip_y: usize,
        layer: u16,
    ) -> Result<Frame, Error> {
        if strip_index >= 12 || strip_y > HEIGHT - 15 {
            return Err(Error::InvalidData("invalid terrain strip placement".into()));
        }
        let source = if self.alternate_background {
            &assets.alternate
        } else {
            &assets.primary
        };
        let base = source
            .get(..FRAME_BYTES)
            .ok_or_else(|| Error::InvalidData("truncated scene background".into()))?;
        let mut packed = if matches!(layer, 1 | 2) {
            vec![0; FRAME_BYTES]
        } else {
            base.to_vec()
        };
        // The alternate picture replaces the base only. Every strip is primary.
        let start = FRAME_BYTES + strip_index * 72 * 15;
        let strip = assets
            .primary
            .get(start..start + 72 * 15)
            .ok_or_else(|| Error::InvalidData("missing terrain strip".into()))?;
        if layer != 1 {
            for row in 0..15 {
                let destination = (strip_y + row) * ROW_BYTES + 4;
                packed[destination..destination + 72]
                    .copy_from_slice(&strip[row * 72..(row + 1) * 72]);
            }
        }
        Frame::from_packed(&packed)
    }

    /// Display seven cumulative background replacements with the frozen world.
    pub fn transition_frames(
        self,
        assets: &GraphicsAssets,
        start: &Frame,
        strip_index: usize,
        strip_y: usize,
        mut draw_world: impl FnMut(&mut Frame, Rect) -> Result<(), Error>,
    ) -> Result<Vec<TransitionFrame>, Error> {
        let background = self.background_with_layer(assets, strip_index, strip_y, self.layer)?;
        let mut frame = start.clone();
        let mut result = Vec::with_capacity(7);
        for (left, top, right, bottom, wait_count) in [
            (36, 96, 44, 104, 10),
            (36, 94, 44, 106, 10),
            (34, 92, 46, 108, 10),
            (32, 86, 48, 114, 8),
            (26, 78, 54, 122, 5),
            (20, 68, 60, 132, 2),
            (4, 44, 76, 158, 2),
        ] {
            let clip = Rect {
                x_bytes: left,
                y: top,
                width_bytes: (right - left) as usize,
                height: (bottom - top) as usize,
            };
            let clip = dirty_region(clip, VIEWPORT)
                .ok_or_else(|| Error::InvalidData("empty transition region".into()))?;
            restore_background(&mut frame, &background, clip)?;
            draw_world(&mut frame, clip)?;
            result.push(TransitionFrame {
                frame: frame.clone(),
                clip,
                wait_count,
                before_pass: false,
                palette_family: 1,
            });
        }
        Ok(result)
    }
}

/// Clip first, then expand to the even byte and scanline boundaries of restores.
pub(crate) fn dirty_region(rect: Rect, viewport: Rect) -> Option<Rect> {
    let right = i64::from(rect.x_bytes).checked_add(i64::try_from(rect.width_bytes).ok()?)?;
    let bottom = i64::from(rect.y).checked_add(i64::try_from(rect.height).ok()?)?;
    let viewport_right =
        i64::from(viewport.x_bytes).checked_add(i64::try_from(viewport.width_bytes).ok()?)?;
    let viewport_bottom =
        i64::from(viewport.y).checked_add(i64::try_from(viewport.height).ok()?)?;
    let left = i64::from(rect.x_bytes).max(i64::from(viewport.x_bytes));
    let top = i64::from(rect.y).max(i64::from(viewport.y));
    let right = right.min(viewport_right);
    let bottom = bottom.min(viewport_bottom);
    if left >= right || top >= bottom {
        return None;
    }
    let left = left & !1;
    let top = top & !1;
    let right = (right + 1) & !1;
    let bottom = (bottom + 1) & !1;
    Some(Rect {
        x_bytes: i32::try_from(left).ok()?,
        y: i32::try_from(top).ok()?,
        width_bytes: usize::try_from(right - left).ok()?,
        height: usize::try_from(bottom - top).ok()?,
    })
}

pub(crate) fn restore_background(
    frame: &mut Frame,
    background: &Frame,
    clip: Rect,
) -> Result<(), Error> {
    let mut packed = frame.packed().to_vec();
    let left = i64::from(clip.x_bytes).clamp(0, ROW_BYTES as i64) as usize;
    let top = i64::from(clip.y).clamp(0, HEIGHT as i64) as usize;
    let right = i64::from(clip.x_bytes)
        .saturating_add(i64::try_from(clip.width_bytes).unwrap_or(i64::MAX))
        .clamp(0, ROW_BYTES as i64) as usize;
    let bottom = i64::from(clip.y)
        .saturating_add(i64::try_from(clip.height).unwrap_or(i64::MAX))
        .clamp(0, HEIGHT as i64) as usize;
    if left < right {
        for row in top..bottom {
            let start = row * ROW_BYTES + left;
            let end = row * ROW_BYTES + right;
            packed[start..end].copy_from_slice(&background.packed()[start..end]);
        }
    }
    *frame = Frame::from_packed(&packed)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> GraphicsAssets {
        let mut primary = vec![0x55; FRAME_BYTES + 12 * 72 * 15];
        primary[FRAME_BYTES + 5 * 72 * 15..FRAME_BYTES + 6 * 72 * 15].fill(0xe4);
        GraphicsAssets {
            primary,
            alternate: vec![0x1b; FRAME_BYTES],
            sprites: Vec::new(),
        }
    }

    #[test]
    fn alternate_base_retains_primary_strip_and_world_layers() {
        let assets = assets();
        let mut state = Presentation {
            alternate_background: true,
            ..Presentation::default()
        };
        let frame = state.background(&assets, 5, 92, 1).unwrap();
        assert_eq!(frame.packed()[80 * 50 + 10], 0x1b);
        assert_eq!(frame.packed()[80 * 92 + 4], 0xe4);
        state.layer = 2;
        let frame = state.background(&assets, 5, 92, 2).unwrap();
        assert_eq!(frame.packed()[80 * 50 + 10], 0);
        assert_eq!(frame.packed()[80 * 92 + 4], 0xe4);
        state.overlay_counter = 1;
        assert!(
            state
                .background(&assets, 5, 92, 3)
                .unwrap()
                .packed()
                .iter()
                .all(|&b| b == 0)
        );
        assert_eq!(
            state
                .background_with_layer(&assets, 5, 92, 0)
                .unwrap()
                .packed()[0],
            0x1b
        );
        state.overlay_counter = 0x8000;
        assert_eq!(state.world_layer(), 2);
    }

    #[test]
    fn transition_replaces_cumulative_regions_and_preserves_surroundings() {
        let start = Frame::from_packed(&vec![0xaa; FRAME_BYTES]).unwrap();
        let mut clips = Vec::new();
        let frames = Presentation::default()
            .transition_frames(&assets(), &start, 5, 92, |_, clip| {
                clips.push(clip);
                Ok(())
            })
            .unwrap();
        assert_eq!(frames.len(), 7);
        assert_eq!(
            frames
                .iter()
                .map(|step| step.wait_count)
                .collect::<Vec<_>>(),
            [10, 10, 10, 8, 5, 2, 2]
        );
        assert_eq!(frames[0].frame.packed()[96 * 80 + 36], 0xe4);
        assert_eq!(frames[0].frame.packed()[96 * 80 + 44], 0xaa);
        assert_eq!(frames[0].clip.width_bytes, 8);
        let last = &frames[6].frame;
        assert_eq!(last.packed()[44 * 80 + 4], 0x55);
        assert_eq!(last.packed()[157 * 80 + 75], 0x55);
        assert_eq!(last.packed()[158 * 80 + 4], 0xaa);
        assert_eq!(last.packed()[44 * 80 + 3], 0xaa);
        assert_eq!(clips[6].height, 114);
    }

    #[test]
    fn dirty_regions_clip_and_round_to_restore_boundaries() {
        let lower = Rect {
            y: 89,
            height: 69,
            ..VIEWPORT
        };
        let rounded = dirty_region(lower, VIEWPORT).unwrap();
        assert_eq!((rounded.y, rounded.height), (88, 70));
        assert!(dirty_region(Rect { y: 158, ..VIEWPORT }, VIEWPORT).is_none());
    }

    #[test]
    fn invalid_sources_and_passes_return_errors() {
        let state = Presentation::default();
        let mut assets = assets();
        assert!(state.background(&assets, 12, 92, 1).is_err());
        assert!(state.background(&assets, 0, 186, 1).is_err());
        assert!(state.background(&assets, 0, 92, 0).is_err());
        assets.alternate.clear();
        assert!(
            Presentation {
                alternate_background: true,
                ..state
            }
            .background(&assets, 0, 92, 1)
            .is_err()
        );
    }
}
