// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Finish resizing before composing exact preview pixels off the UI thread.
use super::{Compositor, TextureKey};
use reforge_sdl::{egui::ColorImage, preview::Frame, settings::UiMetrics};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

const SETTLE_TIME: Duration = Duration::from_millis(120);

struct Request {
    generation: u64,
    key: TextureKey,
    source: Option<Arc<Frame>>,
}

struct Reply {
    generation: u64,
    key: TextureKey,
    image: ColorImage,
}

struct Worker {
    requests: mpsc::Sender<Request>,
    completed: mpsc::Receiver<Reply>,
    busy: bool,
}
impl Worker {
    fn spawn() -> Option<Self> {
        let (requests, receive) = mpsc::channel::<Request>();
        let (send, completed) = mpsc::channel();
        thread::Builder::new()
            .name("launcher-preview-render".into())
            .spawn(move || {
                let mut compositor = Compositor::default();
                while let Ok(request) = receive.recv() {
                    let image = compositor.render(&request.key, request.source.as_deref());
                    if send
                        .send(Reply {
                            generation: request.generation,
                            key: request.key,
                            image,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            requests,
            completed,
            busy: false,
        })
    }
}

pub(super) struct Resize {
    metrics: UiMetrics,
    last_frame: Instant,
    settled_at: Instant,
    generation: u64,
    worker: Option<Worker>,
    ready: Option<Reply>,
}
impl Resize {
    pub(super) fn new(metrics: UiMetrics, now: Instant) -> Self {
        Self {
            metrics,
            last_frame: now,
            settled_at: now,
            generation: 0,
            worker: Worker::spawn(),
            ready: None,
        }
    }

    pub(super) fn begin_frame(&mut self, metrics: UiMetrics, now: Instant) {
        // Egui can repeat a layout pass within one launcher frame.
        if now == self.last_frame && metrics == self.metrics {
            return;
        }
        self.last_frame = now;
        if self.metrics != metrics {
            self.metrics = metrics;
            self.settled_at = now + SETTLE_TIME;
            self.generation += 1;
        }

        // Take at most one result per frame. Keep it only if the current draw
        // asks for that exact key, including source, style and pixel dimensions.
        self.ready = None;
        if let Some(worker) = &mut self.worker {
            match worker.completed.try_recv() {
                Ok(reply) => {
                    worker.busy = false;
                    if reply.generation == self.generation {
                        self.ready = Some(reply);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.worker = None,
            }
        }
    }

    pub(super) fn take_ready(&mut self, key: &TextureKey) -> Option<ColorImage> {
        if self.ready.as_ref().is_some_and(|reply| &reply.key == key) {
            return self.ready.take().map(|reply| reply.image);
        }
        None
    }

    pub(super) fn defer(&mut self, key: &TextureKey, source: Option<&Arc<Frame>>) -> bool {
        if self.last_frame < self.settled_at {
            return true;
        }
        let Some(worker) = &mut self.worker else {
            return false;
        };
        // One outstanding request bounds memory and lets a new size replace
        // pending work without waiting on a queue of intermediate sizes.
        if !worker.busy {
            worker.busy = worker
                .requests
                .send(Request {
                    generation: self.generation,
                    key: key.clone(),
                    source: source.cloned(),
                })
                .is_ok();
            if !worker.busy {
                self.worker = None;
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::preview::{Cache, FrameKey};
    use reforge_sdl::{DisplayGeometry, egui};

    #[test]
    fn reuses_compatible_textures_until_the_latest_resize_has_settled() {
        let ctx = egui::Context::default();
        let mut cache = Cache::default();
        let source = Arc::new(
            Frame::new(
                DisplayGeometry::new((2, 2), (4, 3)),
                [128, 64, 32, 255].repeat(4),
            )
            .unwrap(),
        );
        let key = TextureKey {
            frame: FrameKey {
                width: 80,
                height: 60,
                density: 1.0_f32.to_bits(),
                radius: 2,
                light: true,
                dimmed: false,
            },
            colours: vec![],
            image: Some(("game".into(), false)),
        };
        let original = cache.texture(&ctx, key.clone(), Some(&source)).unwrap();
        let mut metrics = UiMetrics {
            drawable: (880, 560),
            window: (880, 560),
            pixels_per_point: 1.0,
        };
        let now = Instant::now();
        let (requests, submitted) = mpsc::channel();
        let (send, completed) = mpsc::channel();
        cache.resize = Some(Resize {
            metrics,
            last_frame: now,
            settled_at: now,
            generation: 0,
            worker: Some(Worker {
                requests,
                completed,
                busy: false,
            }),
            ready: None,
        });
        let mut resized = key.clone();
        resized.frame.width += 20;
        metrics.drawable.0 += 20;
        let dragging = now + Duration::from_millis(16);
        cache
            .resize
            .as_mut()
            .unwrap()
            .begin_frame(metrics, dragging);
        let fallback = cache.texture(&ctx, resized.clone(), Some(&source)).unwrap();
        assert_eq!(fallback.id(), original.id());
        let mut different_style = resized.clone();
        different_style.image.as_mut().unwrap().1 = true;
        assert!(
            cache
                .texture(&ctx, different_style, Some(&source))
                .is_none()
        );
        assert!(matches!(
            submitted.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));

        let paused = dragging + SETTLE_TIME;
        cache.resize.as_mut().unwrap().begin_frame(metrics, paused);
        assert_eq!(
            cache
                .texture(&ctx, resized.clone(), Some(&source))
                .unwrap()
                .id(),
            original.id()
        );
        let first_request = submitted.try_recv().unwrap();
        assert_eq!(first_request.key, resized);
        assert!(Arc::ptr_eq(first_request.source.as_ref().unwrap(), &source));
        let complete = |request: Request| {
            let image = Compositor::default().render(&request.key, request.source.as_deref());
            send.send(Reply {
                generation: request.generation,
                key: request.key,
                image,
            })
            .unwrap();
        };
        complete(first_request);

        // Resize again before the completed artwork reaches the UI.
        let moved_again = paused + Duration::from_millis(16);
        metrics.drawable.1 += 30;
        resized.frame.height += 30;
        cache
            .resize
            .as_mut()
            .unwrap()
            .begin_frame(metrics, moved_again);
        assert!(cache.resize.as_ref().unwrap().ready.is_none());
        assert_eq!(
            cache
                .texture(&ctx, resized.clone(), Some(&source))
                .unwrap()
                .id(),
            original.id()
        );
        assert!(matches!(
            submitted.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));

        let settled = moved_again + SETTLE_TIME;
        cache.resize.as_mut().unwrap().begin_frame(metrics, settled);
        let _ = cache.texture(&ctx, resized.clone(), Some(&source)).unwrap();
        let latest = submitted.try_recv().unwrap();
        assert_eq!(latest.key, resized);
        complete(latest);
        cache.resize.as_mut().unwrap().begin_frame(metrics, settled);
        assert_eq!(
            cache
                .texture(&ctx, resized.clone(), Some(&source))
                .unwrap()
                .id(),
            original.id(),
            "an additional layout pass must not import another reply"
        );
        cache
            .resize
            .as_mut()
            .unwrap()
            .begin_frame(metrics, settled + Duration::from_millis(16));
        let exact = cache.texture(&ctx, resized.clone(), Some(&source)).unwrap();
        assert_ne!(exact.id(), original.id());
        assert_eq!(exact.size(), resized.frame.size());
        assert_eq!(cache.textures.len(), 2);
        assert!(matches!(
            submitted.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }
    #[test]
    fn cold_theme_changes_keep_artwork_visible_and_reuse_processing_across_worker_jobs() {
        let ctx = egui::Context::default();
        let source = Arc::new(
            Frame::new(
                DisplayGeometry::new((2, 2), (4, 3)),
                [80, 120, 200, 255].repeat(4),
            )
            .unwrap(),
        );
        let original_key = TextureKey {
            frame: FrameKey {
                width: 80,
                height: 60,
                density: 1.0_f32.to_bits(),
                radius: 2,
                light: false,
                dimmed: false,
            },
            colours: vec![],
            image: Some(("game".into(), true)),
        };
        let mut compositor = Compositor::default();
        let mut cache = Cache::default();
        let original = cache.upload(
            &ctx,
            original_key.clone(),
            compositor.render(&original_key, Some(&source)),
        );
        let metrics = UiMetrics {
            drawable: (880, 560),
            window: (880, 560),
            pixels_per_point: 1.0,
        };
        let now = Instant::now();
        let (requests, submitted) = mpsc::channel();
        let (send, completed) = mpsc::channel();
        cache.resize = Some(Resize {
            metrics,
            last_frame: now,
            settled_at: now,
            generation: 0,
            worker: Some(Worker {
                requests,
                completed,
                busy: false,
            }),
            ready: None,
        });
        let mut theme = original_key.clone();
        theme.frame.radius = 6;
        theme.frame.light = true;
        assert_eq!(
            cache
                .texture(&ctx, theme.clone(), Some(&source))
                .unwrap()
                .id(),
            original.id()
        );
        let request = submitted.try_recv().unwrap();
        assert_eq!(request.key, theme);
        // Change theme again before the first composition completes.
        theme.frame.radius = 3;
        assert_eq!(
            cache
                .texture(&ctx, theme.clone(), Some(&source))
                .unwrap()
                .id(),
            original.id()
        );
        let first = compositor.render(&request.key, request.source.as_deref());
        send.send(Reply {
            generation: request.generation,
            key: request.key,
            image: first,
        })
        .unwrap();
        cache
            .resize
            .as_mut()
            .unwrap()
            .begin_frame(metrics, now + Duration::from_millis(16));
        assert_eq!(
            cache
                .texture(&ctx, theme.clone(), Some(&source))
                .unwrap()
                .id(),
            original.id()
        );
        let request = submitted.try_recv().unwrap();
        assert_eq!(request.key, theme);
        let image = compositor.render(&request.key, request.source.as_deref());
        send.send(Reply {
            generation: request.generation,
            key: request.key,
            image,
        })
        .unwrap();
        cache
            .resize
            .as_mut()
            .unwrap()
            .begin_frame(metrics, now + Duration::from_millis(32));
        let updated = cache.texture(&ctx, theme.clone(), Some(&source)).unwrap();
        assert_ne!(updated.id(), original.id());
        assert_eq!(
            compositor.processed.renders, 1,
            "frame-only jobs must not resize or filter artwork again"
        );
        assert_eq!(
            cache
                .texture(&ctx, original_key, Some(&source))
                .unwrap()
                .id(),
            original.id()
        );
        assert_eq!(
            cache
                .texture(&ctx, theme.clone(), Some(&source))
                .unwrap()
                .id(),
            updated.id()
        );
        assert!(matches!(
            submitted.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        // An actual CRT or source change cannot borrow an unrelated preview.
        theme.image.as_mut().unwrap().1 = false;
        assert!(cache.texture(&ctx, theme, Some(&source)).is_none());
    }
    #[test]
    fn switching_games_uses_only_the_selected_games_cached_artwork() {
        let ctx = egui::Context::default();
        let source = Arc::new(
            Frame::new(
                DisplayGeometry::new((2, 2), (4, 3)),
                [80, 120, 200, 255].repeat(4),
            )
            .unwrap(),
        );
        let second_source = Arc::new(
            Frame::new(
                DisplayGeometry::new((2, 2), (4, 3)),
                [200, 50, 80, 255].repeat(4),
            )
            .unwrap(),
        );
        let mut cache = Cache::default();
        let mut first = TextureKey {
            frame: FrameKey {
                width: 80,
                height: 60,
                density: 1.0_f32.to_bits(),
                radius: 2,
                light: false,
                dimmed: false,
            },
            colours: vec![],
            image: Some(("first-game".into(), false)),
        };
        let mut second = first.clone();
        second.image.as_mut().unwrap().0 = "second-game".into();
        let first_texture = cache.texture(&ctx, first.clone(), Some(&source)).unwrap();
        let second_texture = cache
            .texture(&ctx, second.clone(), Some(&second_source))
            .unwrap();
        // Another view has also cached this game at a larger size. The
        // original size should win over the more recently used larger image.
        let mut larger = second.clone();
        larger.frame.width = 100;
        larger.frame.height = 75;
        let _ = cache.texture(&ctx, larger, Some(&second_source)).unwrap();
        cache.resize = Some(Resize::new(
            UiMetrics {
                drawable: (880, 560),
                window: (880, 560),
                pixels_per_point: 1.0,
            },
            Instant::now(),
        ));
        first.frame.radius = 6;
        first.frame.light = true;
        second.frame = first.frame;
        assert_eq!(
            cache.texture(&ctx, first, Some(&source)).unwrap().id(),
            first_texture.id()
        );
        assert_eq!(
            cache
                .texture(&ctx, second.clone(), Some(&second_source))
                .unwrap()
                .id(),
            second_texture.id()
        );
        second.image.as_mut().unwrap().0 = "never-viewed-game".into();
        assert!(
            cache.texture(&ctx, second, Some(&second_source)).is_none(),
            "an uncached game's first render must not borrow a different game's picture"
        );
    }
}
