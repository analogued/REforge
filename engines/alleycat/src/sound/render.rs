// Copyright (c) 2026 analogued. Project license has not been selected.
//! Shared PIT/speaker execution and band-limited PCM; no game sequencing here.
use super::{Category, Write};
use reforge_audio::{
    PIT_HZ,
    category::{CategoryVolumes, SoundCategory},
    pit::CountOnePolicy,
    render::{CategorizedRenderer, Config},
    speaker::PcSpeaker,
};

pub const RATE: u32 = 48_000;
pub struct Renderer {
    speaker: PcSpeaker,
    renderer: CategorizedRenderer,
    category: SoundCategory,
}
impl Renderer {
    pub fn new(volumes: CategoryVolumes) -> Result<Self, String> {
        let mut renderer = CategorizedRenderer::new(
            Config {
                clock_hz: PIT_HZ as u32,
                sample_rate: RATE,
                chunk_ticks: (PIT_HZ / 100) as u32,
            },
            0,
        )
        .map_err(|e| e.to_string())?;
        renderer.set_volumes(volumes).map_err(|e| e.to_string())?;
        Ok(Self {
            speaker: PcSpeaker::with_mode3_count_one(0, [true; 3], CountOnePolicy::DosBoxWrap),
            renderer,
            category: SoundCategory::Effects,
        })
    }
    pub fn set_volumes(&mut self, volumes: CategoryVolumes) -> Result<(), String> {
        self.renderer
            .set_volumes(volumes)
            .map_err(|e| e.to_string())
    }
    fn advance(&mut self, target: u64, pcm: &mut Vec<i16>) -> Result<(), String> {
        while self.speaker.pit().tick() < target {
            let events = self.speaker.advance_to(target).map_err(|e| e.to_string())?;
            if let Some(edge) = events.drive {
                self.renderer
                    .level_at(
                        edge.tick,
                        self.category,
                        if edge.high { 8192 } else { 0 },
                        &mut |block| {
                            pcm.extend_from_slice(block);
                            Ok(())
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        writes: impl IntoIterator<Item = Write>,
        until: u64,
    ) -> Result<Vec<i16>, String> {
        let mut pcm = Vec::new();
        for write in writes {
            if write.tick > until || write.tick < self.speaker.pit().tick() {
                return Err("speaker event outside committed interval".into());
            }
            self.advance(write.tick, &mut pcm)?;
            self.category = match write.category {
                Category::Music => SoundCategory::Music,
                Category::Effects => SoundCategory::Effects,
            };
            let events = self
                .speaker
                .write_port(write.port, write.value)
                .map_err(|e| e.to_string())?;
            self.renderer
                .level_at(
                    write.tick,
                    self.category,
                    if self.speaker.drive() { 8192 } else { 0 },
                    &mut |block| {
                        pcm.extend_from_slice(block);
                        Ok(())
                    },
                )
                .map_err(|e| e.to_string())?;
            debug_assert!(events.tick == write.tick);
        }
        self.advance(until, &mut pcm)?;
        self.renderer
            .finish_until(until, &mut |block| {
                pcm.extend_from_slice(block);
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(pcm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::Sound;
    #[test]
    fn category_mute_changes_pcm_without_changing_the_port_program_or_sample_count() {
        let mut sound = Sound::default();
        sound.record(true);
        sound.tone(1000, Category::Music);
        sound.at(PIT_HZ / 10);
        sound.stop();
        sound.at(PIT_HZ / 5);
        sound.tone(1500, Category::Effects);
        sound.at(PIT_HZ * 3 / 10);
        sound.stop();
        let events: Vec<_> = sound.drain().collect();
        let end = PIT_HZ * 4 / 10;
        let normal = Renderer::new(CategoryVolumes::default())
            .unwrap()
            .render(events.clone(), end)
            .unwrap();
        let muted = Renderer::new(CategoryVolumes([0, 100, 100]))
            .unwrap()
            .render(events.clone(), end)
            .unwrap();
        assert_eq!(normal.len(), muted.len());
        assert!(normal[1000..4000].iter().any(|v| v.abs() > 100));
        assert!(muted[1000..4000].iter().all(|v| *v == 0));
        assert!(muted[11000..14000].iter().any(|v| v.abs() > 100));
        let mut split = Renderer::new(CategoryVolumes::default()).unwrap();
        let cutoff = PIT_HZ / 10;
        let mut pcm = split
            .render(events.iter().copied().filter(|e| e.tick <= cutoff), cutoff)
            .unwrap();
        pcm.extend(
            split
                .render(events.iter().copied().filter(|e| e.tick > cutoff), end)
                .unwrap(),
        );
        assert_eq!(normal, pcm);
    }
}
