//! A drawing back end for anyrender of instanced quads on wgpu, drawn as Zed's GPUI draws: a box,
//! its border, its shadow and its clip are one instance each, consecutive instances of one kind
//! are one draw, and a frame is one render pass, plus one per opacity layer. Unlike GPUI it keeps
//! the order blitz-paint draws in, which is CSS paint order.
//!
//! What its shaders cannot draw yet (glyphs, images, gradients, paths, filters, masks) is left out
//! and counted by kind in [`take_frame_times`].

mod frame_times;
mod gpu;
mod image_renderer;
mod scene;
mod window_renderer;

pub use frame_times::{FrameTimes, take_frame_times};
pub use image_renderer::QuadsImageRenderer;
pub use scene::QuadsScenePainter;
pub use window_renderer::QuadsWindowRenderer;
