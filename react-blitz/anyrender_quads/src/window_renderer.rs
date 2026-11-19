use std::sync::Arc;
use std::time::Instant;

use anyrender::{RenderContext, WindowHandle, WindowRenderer};
use wgpu::{CompositeAlphaMode, PresentMode, TextureFormat, TextureUsages};
use wgpu_context::{SurfaceRenderer, SurfaceRendererConfiguration, TextureConfiguration, WGPUContext};

use crate::frame_times;
use crate::gpu::{FORMAT, Gpu};
use crate::scene::{QuadsScenePainter, Recording};

struct Active {
    surface: SurfaceRenderer<'static>,
    gpu: Gpu,
}

/// Draws a scene into a window, on white, as Vello's window renderer does by default.
pub struct QuadsWindowRenderer {
    // The surface before the window, so it is dropped first.
    active: Option<Active>,
    window: Option<Arc<dyn WindowHandle>>,
    context: WGPUContext,
    recording: Recording,
}

impl QuadsWindowRenderer {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { active: None, window: None, context: WGPUContext::new(), recording: Recording::default() }
    }
}

impl RenderContext for QuadsWindowRenderer {}

impl WindowRenderer for QuadsWindowRenderer {
    type ScenePainter<'a>
        = QuadsScenePainter<'a>
    where
        Self: 'a;

    fn resume<F: FnOnce() + 'static>(&mut self, window: Arc<dyn WindowHandle>, width: u32, height: u32, on_ready: F) {
        if self.active.is_some() {
            return;
        }
        self.window = Some(window.clone());
        let surface = pollster::block_on(self.context.create_surface_renderer(
            window,
            SurfaceRendererConfiguration {
                usage: TextureUsages::RENDER_ATTACHMENT,
                formats: vec![TextureFormat::Rgba8Unorm, TextureFormat::Bgra8Unorm],
                width,
                height,
                present_mode: PresentMode::AutoVsync,
                desired_maximum_frame_latency: 1,
                alpha_mode: CompositeAlphaMode::Auto,
                view_formats: vec![],
            },
            // Drawn into a texture of the pipelines' format, then copied to the window's.
            Some(TextureConfiguration {
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                format: FORMAT,
                alpha_conversion: None,
            }),
        ))
        .expect("Error creating the window's surface");
        let gpu = Gpu::new(surface.device());
        self.active = Some(Active { surface, gpu });
        on_ready();
    }

    fn complete_resume(&mut self) -> bool {
        self.active.is_some()
    }

    fn suspend(&mut self) {
        self.active = None;
    }

    fn is_active(&self) -> bool {
        self.active.is_some()
    }

    fn set_size(&mut self, width: u32, height: u32) {
        if let Some(active) = &mut self.active {
            active.surface.resize(width, height);
        }
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        let Some(active) = &mut self.active else { return };
        let (width, height) = (active.surface.config.width, active.surface.config.height);
        let mut painter = QuadsScenePainter::new(&mut self.recording, width, height);
        draw_fn(&mut painter);
        painter.finish();

        let step = Instant::now();
        let Ok(view) = active.surface.target_texture_view() else {
            active.surface.clear_surface_texture();
            return;
        };
        let acquire = step.elapsed();
        active.gpu.render(active.surface.device(), active.surface.queue(), &mut self.recording, &view, (width, height), wgpu::Color::WHITE);
        drop(view);

        let step = Instant::now();
        if active.surface.maybe_blit_and_present().is_err() {
            return;
        }
        let present = step.elapsed();
        let step = Instant::now();
        let _ = active.surface.device().poll(wgpu::PollType::wait_indefinitely());
        let wait = step.elapsed();
        frame_times::add(|t| {
            t.acquire += acquire;
            t.present += present;
            t.wait += wait;
        });
    }
}
