use anyrender::{ImageRenderer, RenderContext};
use wgpu::TextureUsages;
use wgpu_context::{BufferRenderer, BufferRendererConfig, WGPUContext};

use crate::gpu::Gpu;
use crate::scene::{QuadsScenePainter, Recording};

/// Draws a scene with no window, into RGBA8, unpremultiplied as `anyrender_vello`'s is.
pub struct QuadsImageRenderer {
    buffer_renderer: BufferRenderer,
    gpu: Gpu,
    recording: Recording,
}

impl RenderContext for QuadsImageRenderer {}

impl ImageRenderer for QuadsImageRenderer {
    type ScenePainter<'a>
        = QuadsScenePainter<'a>
    where
        Self: 'a;

    fn new(width: u32, height: u32) -> Self {
        let mut context = WGPUContext::new();
        let buffer_renderer = pollster::block_on(context.create_buffer_renderer(BufferRendererConfig {
            width,
            height,
            usage: TextureUsages::RENDER_ATTACHMENT,
        }))
        .expect("No compatible device found");
        let gpu = Gpu::new(buffer_renderer.device());
        Self { buffer_renderer, gpu, recording: Recording::default() }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.buffer_renderer.resize(width, height);
    }

    fn reset(&mut self) {
        self.recording.clear();
    }

    fn render_to_vec<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F, vec: &mut Vec<u8>) {
        let size = self.buffer_renderer.size();
        vec.resize((size.width * size.height * 4) as usize, 0);
        self.render(draw_fn, vec);
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F, buffer: &mut [u8]) {
        let size = self.buffer_renderer.size();
        let mut painter = QuadsScenePainter::new(&mut self.recording, size.width, size.height);
        draw_fn(&mut painter);
        painter.finish();
        let view = self.buffer_renderer.target_texture_view();
        self.gpu.render(
            self.buffer_renderer.device(),
            self.buffer_renderer.queue(),
            &self.recording,
            &view,
            (size.width, size.height),
            wgpu::Color::TRANSPARENT,
        );
        self.buffer_renderer.copy_texture_to_buffer(buffer);
        unpremultiply(buffer);
    }
}

fn unpremultiply(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}
