//! A frame's work, by phase.

use std::fmt;

use serde_json::{Value, json};

/// What one frame cost, in ms: the six parts of its busy time.
#[derive(Default, Clone, Copy, Debug)]
pub struct Phases {
    /// Input events since the frame before, and the JS their listeners ran.
    pub input: f64,
    /// The page's turns since the frame before: timers, messages, frame callbacks, React's commit.
    pub turn: f64,
    /// Style, layout and the paint tree.
    pub resolve: f64,
    /// Building the scene from the paint tree.
    pub scene: f64,
    /// Turning the scene into pixels.
    pub raster: f64,
    /// Handing the pixels to the window; none offscreen.
    pub present: f64,
}

impl Phases {
    /// The frame's whole work: the six, added.
    pub fn busy(&self) -> f64 {
        self.input + self.turn + self.resolve + self.scene + self.raster + self.present
    }

    pub fn json(&self) -> Value {
        json!({
            "input": self.input,
            "turn": self.turn,
            "resolve": self.resolve,
            "scene": self.scene,
            "raster": self.raster,
            "present": self.present,
            "busy": self.busy(),
        })
    }
}

impl fmt::Display for Phases {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "busy {:.1} ms: input {:.1}, turn {:.1}, resolve {:.1}, scene {:.1}, raster {:.1}, present {:.1}",
            self.busy(),
            self.input,
            self.turn,
            self.resolve,
            self.scene,
            self.raster,
            self.present
        )
    }
}
