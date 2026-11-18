//! What the window renderer's last frame cost, by step, for an embedder that logs its frames.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// One frame of [`VelloWindowRenderer`](crate::VelloWindowRenderer), by step.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct FrameTimes {
    /// Time inside the painter's calls: Vello encoding what the scene's builder draws.
    pub encode: Duration,
    /// Waiting for the window's next texture.
    pub acquire: Duration,
    /// Vello's `render_to_texture`: its passes recorded and submitted.
    pub render: Duration,
    /// The copy to the window, if any, and the present.
    pub present: Duration,
    /// Waiting for the device to finish the frame.
    pub wait: Duration,
    /// Glyph runs drawn.
    pub glyph_runs: u32,
    /// Glyphs drawn.
    pub glyphs: u32,
}

thread_local! {
    static FRAME: Cell<FrameTimes> = const { Cell::new(FrameTimes {
        encode: Duration::ZERO,
        acquire: Duration::ZERO,
        render: Duration::ZERO,
        present: Duration::ZERO,
        wait: Duration::ZERO,
        glyph_runs: 0,
        glyphs: 0,
    }) };
}

/// The times of the frames drawn on this thread since the last call, added.
pub fn take_frame_times() -> FrameTimes {
    FRAME.take()
}

pub(crate) fn add(f: impl FnOnce(&mut FrameTimes)) {
    let mut times = FRAME.get();
    f(&mut times);
    FRAME.set(times);
}

thread_local! {
    /// How many painter calls are under way: one that calls another is timed once.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
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
