// Copyright (c) 2026 analogued. Project license has not yet been selected.
//! Explicit texture invalidation makes renderer teardown precede replacement.
use reforge_settings::Session;
use sdl3::{
    pixels::PixelFormat,
    rect::Rect,
    render::{BlendMode, ScaleMode, Texture, TextureCreator, WindowCanvas},
    video::WindowContext,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
pub(crate) struct Owner {
    pub creator: TextureCreator<WindowContext>,
    pub _gpu: Option<Rc<crate::hyllian::GpuDevice>>,
}
struct Resident {
    texture: Texture<'static>,
    _owner: Rc<Owner>,
    generation: usize,
}
type Slot = RefCell<Option<Resident>>;
#[derive(Clone)]
pub struct FrameCreator {
    pub(crate) owner: Rc<RefCell<Rc<Owner>>>,
    pub(crate) session: Rc<RefCell<Session>>,
    pub(crate) audio: crate::audio::Playback,
    // Resolved windowed multiplier, separate from the persisted Auto preference.
    pub(crate) windowed_scale: Rc<Cell<u32>>,
    generation: Rc<Cell<usize>>,
    textures: Rc<RefCell<Vec<Weak<Slot>>>>,
}
impl FrameCreator {
    pub(crate) fn new(
        canvas: &WindowCanvas,
        session: Session,
        windowed_scale: u32,
        gpu: Option<Rc<crate::hyllian::GpuDevice>>,
    ) -> Self {
        let audio = crate::audio::Playback::new(session.effective(&session.saved).audio);
        audio.focus(canvas.window().has_input_focus());
        Self {
            audio,
            owner: Rc::new(RefCell::new(Rc::new(Owner {
                creator: canvas.texture_creator(),
                _gpu: gpu,
            }))),
            session: Rc::new(RefCell::new(session)),
            windowed_scale: Rc::new(Cell::new(windowed_scale)),
            generation: Rc::new(Cell::new(0)),
            textures: Rc::new(RefCell::new(Vec::new())),
        }
    }
    pub fn audio_playback(&self) -> crate::audio::Playback {
        self.audio.clone()
    }
    pub(crate) fn generation(&self) -> usize {
        self.generation.get()
    }
    pub(crate) fn replace_owner(&self, owner: Rc<Owner>) {
        *self.owner.borrow_mut() = owner;
        self.generation.set(self.generation.get().wrapping_add(1));
    }
    pub(crate) fn release_textures(&self) {
        for slot in self.textures.borrow().iter().filter_map(Weak::upgrade) {
            slot.borrow_mut().take();
        }
        self.textures.borrow_mut().clear();
    }
    pub fn create_texture_streaming(
        &self,
        format: PixelFormat,
        width: u32,
        height: u32,
    ) -> Result<TextureLease, String> {
        let owner = self.owner.borrow().clone();
        let texture = owner
            .creator
            .create_texture_streaming(format, width, height)
            .map_err(|e| e.to_string())?;
        let raw = texture.raw();
        std::mem::forget(texture);
        // SAFETY: transfer sole texture ownership into a resident that retains its
        // renderer owner. The texture drops before that owner. Access is borrowed
        // through the slot; invalidation cannot race a live borrow on the SDL thread.
        let texture = unsafe { Texture::from_raw(raw) };
        let slot = Rc::new(RefCell::new(Some(Resident {
            texture,
            _owner: owner,
            generation: self.generation(),
        })));
        let mut textures = self.textures.borrow_mut();
        textures.retain(|weak| weak.strong_count() > 0);
        textures.push(Rc::downgrade(&slot));
        Ok(TextureLease { slot })
    }
}
/// An invalidatable texture; renderer transitions clear it before destroying SDL.
pub struct TextureLease {
    slot: Rc<Slot>,
}
impl TextureLease {
    pub(crate) fn belongs_to(&self, creator: &FrameCreator) -> bool {
        self.slot
            .borrow()
            .as_ref()
            .is_some_and(|r| r.generation == creator.generation())
    }
    pub(crate) fn with<R>(&self, f: impl FnOnce(&Texture<'static>) -> R) -> Result<R, String> {
        let slot = self.slot.borrow();
        let r = slot
            .as_ref()
            .ok_or("texture was invalidated by a renderer change")?;
        Ok(f(&r.texture))
    }
    pub fn update(
        &mut self,
        rect: Option<Rect>,
        pixels: &[u8],
        pitch: usize,
    ) -> Result<(), String> {
        let mut slot = self.slot.borrow_mut();
        slot.as_mut()
            .ok_or("texture was invalidated")?
            .texture
            .update(rect, pixels, pitch)
            .map_err(|e| e.to_string())
    }
    pub fn set_scale_mode(&mut self, mode: ScaleMode) {
        if let Some(r) = self.slot.borrow_mut().as_mut() {
            r.texture.set_scale_mode(mode);
        }
    }
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        if let Some(r) = self.slot.borrow_mut().as_mut() {
            r.texture.set_blend_mode(mode);
        }
    }
    pub fn raw(&self) -> *mut sdl3::sys::render::SDL_Texture {
        self.slot
            .borrow()
            .as_ref()
            .map_or(std::ptr::null_mut(), |r| r.texture.raw())
    }
}
