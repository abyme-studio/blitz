//! What the last frames cost, by step, and what they drew, for an embedder that logs its frames:
//! the same steps as `anyrender_vello::take_frame_times`, and this back end's own counts.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// The frames drawn on this thread since the last [`take_frame_times`], added.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct FrameTimes {
    /// Time inside the painter's calls: recording what the scene's builder draws.
    pub encode: Duration,
    /// Waiting for the window's next texture.
    pub acquire: Duration,
    /// Instances uploaded and the frame's passes recorded and submitted.
    pub render: Duration,
    /// The copy to the window, if any, and the present.
    pub present: Duration,
    /// Waiting for the device to finish the frame.
    pub wait: Duration,
    pub glyph_runs: u32,
    pub glyphs: u32,
    /// Instances drawn, by kind.
    pub quads: u32,
    pub shadows: u32,
    /// Opacity layers, each a texture of its own.
    pub layers: u32,
    /// Instanced draw calls and render passes.
    pub draws: u32,
    pub passes: u32,
    /// Glyphs and images rasterized or converted into the atlas, new this frame.
    pub glyphs_added: u32,
    pub images_added: u32,
    /// Glyph and image sprites drawn.
    pub sprites: u32,
    /// Pages emptied because one filled.
    pub atlas_resets: u32,
    /// What was not drawn, by kind, and how often.
    pub left_out: BTreeMap<&'static str, u32>,
}

thread_local! {
    static FRAME: RefCell<FrameTimes> = RefCell::new(FrameTimes::default());
    /// How many painter calls are under way: one that calls another is timed once.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// The times and counts of the frames drawn on this thread since the last call, added.
pub fn take_frame_times() -> FrameTimes {
    FRAME.with(|f| std::mem::take(&mut *f.borrow_mut()))
}

pub(crate) fn add(f: impl FnOnce(&mut FrameTimes)) {
    FRAME.with(|times| f(&mut times.borrow_mut()));
}

pub(crate) fn left_out(what: &'static str) {
    add(|t| *t.left_out.entry(what).or_default() += 1);
}

/// Times one painter call into `encode`, from its making to its drop; a call made inside
/// another is part of the outer one.
pub(crate) struct Encoding(Option<Instant>);

impl Encoding {
    pub(crate) fn start() -> Self {
        let depth = DEPTH.get();
        DEPTH.set(depth + 1);
        Self((depth == 0).then(Instant::now))
    }
}

impl Drop for Encoding {
    fn drop(&mut self) {
        DEPTH.set(DEPTH.get() - 1);
        if let Some(started) = self.0 {
            let took = started.elapsed();
            add(|t| t.encode += took);
        }
    }
}
