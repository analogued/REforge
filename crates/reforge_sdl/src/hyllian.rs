// Copyright (c) 2026 analogued. Project license has not yet been selected.

//! SDL's GPU renderer bridge. The adapted third-party shader lives separately
//! under shaders/; engines and software renderers never execute this effect.

use reforge_settings::{Backend, Renderer, VideoSettings};
use std::{
    ffi::{CStr, CString},
    ptr::NonNull,
};

use sdl3::{
    pixels::Color,
    render::{Texture, WindowCanvas},
    sys::{gpu, properties, rect::SDL_FRect, render},
    video::Window,
};

use crate::Presentation;

// Vulkan consumes SPIR-V as 32-bit words; include_bytes! alone promises only
// byte alignment. Retain static storage with the required alignment explicitly.
#[repr(align(4))]
struct Spirv([u8; include_bytes!("../shaders/hyllian.spv").len()]);
static SPIRV: Spirv = Spirv(*include_bytes!("../shaders/hyllian.spv"));

pub(super) fn create_canvas_with(
    window: Window,
    presentation: Presentation,
    settings: VideoSettings,
) -> Result<(WindowCanvas, Option<Hyllian>), String> {
    let selected = if settings.renderer == Renderer::Auto
        && matches!(
            settings.backend,
            Backend::OpenGl | Backend::OpenGlEs | Backend::Direct3d11
        ) {
        Renderer::Sdl
    } else {
        settings.renderer
    };
    let canvas = match selected {
        Renderer::Software => {
            sdl3::render::create_renderer(window, Some(c"software")).map_err(|e| e.to_string())?
        }
        Renderer::Sdl if settings.backend != Backend::Auto => {
            let name = CString::new(settings.backend.as_str()).map_err(|e| e.to_string())?;
            sdl3::render::create_renderer(window, Some(&name)).map_err(|e| e.to_string())?
        }
        Renderer::Sdl => ordinary_canvas(window)?,
        Renderer::Auto | Renderer::Gpu => {
            if settings.renderer == Renderer::Auto
                && settings.backend == Backend::Auto
                && sdl3::hint::get("SDL_RENDER_DRIVER").is_some()
            {
                fallback_canvas(window)?
            } else {
                if settings.backend != Backend::Auto
                    && !matches!(
                        settings.backend,
                        Backend::Vulkan | Backend::Direct3d12 | Backend::Metal
                    )
                {
                    return Err("backend is not a GPU backend".into());
                }
                match gpu_canvas(window.clone(), settings.backend) {
                    Ok(canvas) => canvas,
                    Err(error)
                        if settings.renderer == Renderer::Auto
                            && settings.backend == Backend::Auto =>
                    {
                        eprintln!(
                            "REforge: GPU renderer unavailable: {error}; trying SDL selection"
                        );
                        fallback_canvas(window)?
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    };
    let effect = if presentation == Presentation::Hyllian {
        match Hyllian::new(&canvas) {
            Ok(effect) => Some(effect),
            Err(error) => {
                eprintln!("REforge: Hyllian unavailable: {error}; using plain pixels");
                None
            }
        }
    } else {
        None
    };
    Ok((canvas, effect))
}

fn ordinary_canvas(window: Window) -> Result<WindowCanvas, String> {
    for driver in sdl3::render::drivers().filter(|name| name != "gpu" && name != "software") {
        let name = CString::new(driver).map_err(|e| e.to_string())?;
        if let Ok(canvas) = sdl3::render::create_renderer(window.clone(), Some(&name)) {
            return Ok(canvas);
        }
    }
    sdl3::render::create_renderer(window, Some(c"software")).map_err(|e| e.to_string())
}

fn fallback_canvas(window: Window) -> Result<WindowCanvas, String> {
    match sdl3::render::create_renderer(window.clone(), None) {
        Ok(canvas) => Ok(canvas),
        Err(error) => {
            eprintln!("REforge: renderer unavailable: {error}; trying software rendering");
            sdl3::render::create_renderer(window, Some(c"software"))
                .map_err(|error| error.to_string())
        }
    }
}

/// Only this short-lived owner uses raw renderer-creation properties; the Rust
/// wrapper's property ID is private. Do not mutate process-wide renderer hints.
struct Properties(properties::SDL_PropertiesID);

impl Drop for Properties {
    fn drop(&mut self) {
        // SAFETY: this ID was created below on the same main thread and is
        // destroyed once. SDL has already consumed its values at this point.
        unsafe { properties::SDL_DestroyProperties(self.0) };
    }
}

fn gpu_canvas(window: Window, backend: Backend) -> Result<WindowCanvas, String> {
    // SAFETY: SDL video is initialized and this runs on the window's main thread.
    let props = Properties(unsafe { properties::SDL_CreateProperties() });
    if props.0 == 0 {
        return Err(sdl3::get_error().to_string());
    }
    // SAFETY: properties and window are live. SDL copies the names/values; no
    // Rust data is retained. Bundled formats allow SDL to choose Vulkan, Metal or Direct3D 12.
    let configured = unsafe {
        properties::SDL_SetStringProperty(
            props.0,
            render::SDL_PROP_RENDERER_CREATE_NAME_STRING,
            c"gpu".as_ptr(),
        ) && properties::SDL_SetPointerProperty(
            props.0,
            render::SDL_PROP_RENDERER_CREATE_WINDOW_POINTER,
            window.raw().cast(),
        ) && properties::SDL_SetBooleanProperty(
            props.0,
            render::SDL_PROP_RENDERER_CREATE_GPU_SHADERS_SPIRV_BOOLEAN,
            true,
        ) && properties::SDL_SetBooleanProperty(
            props.0,
            render::SDL_PROP_RENDERER_CREATE_GPU_SHADERS_DXIL_BOOLEAN,
            true,
        ) && properties::SDL_SetBooleanProperty(
            props.0,
            render::SDL_PROP_RENDERER_CREATE_GPU_SHADERS_MSL_BOOLEAN,
            true,
        )
    };
    if !configured {
        return Err(sdl3::get_error().to_string());
    }
    if backend != Backend::Auto {
        let driver = CString::new(backend.as_str()).map_err(|e| e.to_string())?;
        // SAFETY: these live creation properties are copied by SDL. Its GPU
        // renderer forwards GPU device creation properties and owns that device.
        if !unsafe {
            properties::SDL_SetStringProperty(
                props.0,
                gpu::SDL_PROP_GPU_DEVICE_CREATE_NAME_STRING,
                driver.as_ptr(),
            )
        } {
            return Err(sdl3::get_error().to_string());
        }
    }
    // SAFETY: all required properties were initialized, and window is live.
    let renderer = unsafe { render::SDL_CreateRendererWithProperties(props.0) };
    if renderer.is_null() {
        return Err(sdl3::get_error().to_string());
    }
    // SAFETY: SDL returned a new valid renderer for this window. Transfer its
    // sole ownership to the wrapper, which also retains the window lifetime.
    Ok(unsafe { WindowCanvas::from_window_and_renderer(window, renderer) })
}

pub(super) struct Hyllian {
    renderer: NonNull<render::SDL_Renderer>,
    device: NonNull<gpu::SDL_GPUDevice>,
    state: NonNull<render::SDL_GPURenderState>,
    shader: NonNull<gpu::SDL_GPUShader>,
}

impl Hyllian {
    pub(super) fn new(canvas: &WindowCanvas) -> Result<Self, String> {
        let renderer = NonNull::new(canvas.raw()).ok_or("missing SDL renderer")?;
        // SAFETY: canvas owns a live main-thread renderer. SDL returns a borrowed
        // device; non-GPU renderers return null and remain usable without shaders.
        let device = NonNull::new(unsafe { render::SDL_GetGPURendererDevice(renderer.as_ptr()) })
            .ok_or("selected renderer has no shader-capable GPU device")?;
        // SAFETY: the borrowed device remains alive with its owning renderer.
        let formats = unsafe { gpu::SDL_GetGPUShaderFormats(device.as_ptr()) };
        let (code, format, entrypoint): (&[u8], _, &CStr) =
            if formats & gpu::SDL_GPU_SHADERFORMAT_SPIRV != gpu::SDL_GPU_SHADERFORMAT_INVALID {
                (&SPIRV.0, gpu::SDL_GPU_SHADERFORMAT_SPIRV, c"main")
            } else if formats & gpu::SDL_GPU_SHADERFORMAT_DXIL != gpu::SDL_GPU_SHADERFORMAT_INVALID
            {
                (
                    include_bytes!("../shaders/hyllian.dxil"),
                    gpu::SDL_GPU_SHADERFORMAT_DXIL,
                    c"main",
                )
            } else if formats & gpu::SDL_GPU_SHADERFORMAT_MSL != gpu::SDL_GPU_SHADERFORMAT_INVALID {
                (
                    include_bytes!("../shaders/hyllian.msl"),
                    gpu::SDL_GPU_SHADERFORMAT_MSL,
                    c"main0",
                )
            } else {
                return Err("no bundled shader format supported by the selected device".into());
            };
        let info = gpu::SDL_GPUShaderCreateInfo {
            code_size: code.len(),
            code: code.as_ptr(),
            entrypoint: entrypoint.as_ptr(),
            format,
            stage: gpu::SDL_GPU_SHADERSTAGE_FRAGMENT,
            num_samplers: 1,
            num_uniform_buffers: 1,
            ..Default::default()
        };
        // SAFETY: shader bytes/entrypoint remain static; SDL consumes valid shader
        // metadata synchronously. Binding order matches shaders/hyllian.frag.
        let shader = NonNull::new(unsafe { gpu::SDL_CreateGPUShader(device.as_ptr(), &info) })
            .ok_or_else(|| sdl3::get_error().to_string())?;
        let create = render::SDL_GPURenderStateCreateInfo {
            fragment_shader: shader.as_ptr(),
            ..Default::default()
        };
        // SAFETY: renderer/device/shader are live and match, and no additional
        // resources are bound. This render state is owned by this object.
        let state = unsafe { render::SDL_CreateGPURenderState(renderer.as_ptr(), &create) };
        let Some(state) = NonNull::new(state) else {
            let error = sdl3::get_error().to_string();
            // SAFETY: shader was newly created and no state took ownership.
            unsafe { gpu::SDL_ReleaseGPUShader(device.as_ptr(), shader.as_ptr()) };
            return Err(error);
        };
        Ok(Self {
            renderer,
            device,
            state,
            shader,
        })
    }

    pub(super) fn paint(
        &mut self,
        canvas: &mut WindowCanvas,
        texture: &Texture<'_>,
        size: (u32, u32),
    ) -> Result<(), String> {
        let mut viewport = SDL_FRect::default();
        // SAFETY: the owning canvas/renderer is live and viewport is writable.
        if !unsafe { render::SDL_GetRenderLogicalPresentationRect(canvas.raw(), &mut viewport) } {
            return Err(sdl3::get_error().to_string());
        }
        // Two std140 vec4s, with no padding/uninitialized bytes. Actual drawable
        // coordinates keep mask placement correct after DPI and viewport changes.
        let uniforms = [
            size.0 as f32,
            size.1 as f32,
            0.0,
            0.0,
            viewport.x,
            viewport.y,
            viewport.w,
            viewport.h,
        ];
        canvas.set_draw_color(Color::RGB(0, 0, 0));
        canvas.clear();
        // SAFETY: state belongs to this renderer; SDL copies the 32 bytes before
        // returning. Shader uniforms declare precisely this layout in slot0.
        if !unsafe {
            render::SDL_SetGPURenderStateFragmentUniforms(
                self.state.as_ptr(),
                0,
                uniforms.as_ptr().cast(),
                32,
            )
        } {
            return Err(sdl3::get_error().to_string());
        }
        // SAFETY: state and renderer are alive on their original main thread.
        if !unsafe { render::SDL_SetGPURenderState(canvas.raw(), self.state.as_ptr()) } {
            return Err(sdl3::get_error().to_string());
        }
        let drawn = canvas
            .copy(texture, None, None)
            .map_err(|error| error.to_string());
        // Always restore the ordinary renderer, including on copy failure, so
        // modern engine overlays do not inherit the shader or its uniforms.
        // SAFETY: live renderer; null clears the custom state without ownership.
        let restored = unsafe { render::SDL_SetGPURenderState(canvas.raw(), std::ptr::null_mut()) };
        drawn?;
        if !restored {
            return Err(sdl3::get_error().to_string());
        }
        Ok(())
    }
}

impl Drop for Hyllian {
    fn drop(&mut self) {
        // SAFETY: Window drops this object before its canvas/device. Flush any
        // queued draws (including after a caller error), then release resources
        // exactly once on the main thread. Device ownership remains with SDL.
        unsafe {
            render::SDL_FlushRenderer(self.renderer.as_ptr());
            render::SDL_DestroyGPURenderState(self.state.as_ptr());
            gpu::SDL_ReleaseGPUShader(self.device.as_ptr(), self.shader.as_ptr());
        }
    }
}
