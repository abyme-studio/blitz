//! A drawing back end for anyrender of instanced quads on wgpu, drawn as Zed's GPUI draws: a box,
//! its border, its shadow and its clip are one instance each, consecutive instances of one kind
//! are one draw, and a frame is one render pass, plus one per opacity layer. Unlike GPUI it keeps
//! the order blitz-paint draws in, which is CSS paint order.
//!
//! Glyphs and images are sprites from an atlas, rasterized once by `vello_cpu`. What its shaders
//! cannot draw (gradients, paths, layers with a filter or a path clip) is drawn by `vello_cpu` into
//! the atlas too, once, and reused while it stays the same. Masks are left out, and a backdrop
//! filter or a non-normal blend is drawn without its effect; each is counted by kind in
//! [`take_frame_times`].

mod atlas;
mod frame_times;
mod gpu;
mod image_renderer;
mod scene;
mod window_renderer;

pub use frame_times::{FrameTimes, take_frame_times};
pub use image_renderer::QuadsImageRenderer;
pub use scene::QuadsScenePainter;
pub use window_renderer::QuadsWindowRenderer;

/// The GPU adapter wgpu would draw on, by name, or why there is none.
pub fn adapter() -> Result<String, String> {
    let instance = wgpu::Instance::default();
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .map(|adapter| adapter.get_info().name)
        .map_err(|e| e.to_string())
}
