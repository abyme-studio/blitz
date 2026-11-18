//! A frame's work, by stage, and how much of each kind ran.

use std::fmt;

use blitz_dom::ResolveStats;
use serde_json::{Value, json};

/// What one frame cost, in ms: the seven stages of its busy time, as the thread that draws
/// spent them. A wait inside a stage (the display's, the GPU's) is part of it.
#[derive(Default, Clone, Copy, Debug)]
pub struct Phases {
    /// Input events and the page's turns since the frame before (timers, messages, frame
    /// callbacks, React's commit), less the style and layout a read from JS made them run.
    pub js: f64,
    /// Stylo: every resolve's style pass since the frame before.
    pub style: f64,
    /// The rest of every resolve: damage, box construction, layout, transforms, the paint tree.
    pub layout: f64,
    /// Building the scene from the paint tree, less the painter's own calls.
    pub scene: f64,
    /// The painter's calls: Vello encoding what the scene draws. 0 on a renderer that does not
    /// time them apart (the CPU ones), whose `scene` holds them.
    pub encode: f64,
    /// Turning the scene into pixels: Vello's passes recorded, submitted and awaited, or the CPU
    /// renderer's raster.
    pub render: f64,
    /// Handing the pixels to the window: its next texture, awaited, and the present; none offscreen.
    pub present: f64,
}

/// The names of the seven stages, in a frame's order.
pub const STAGES: [&str; 7] = ["js", "style", "layout", "scene", "encode", "render", "present"];

impl Phases {
    /// The stages in [`STAGES`]' order.
    pub fn stages(&self) -> [f64; 7] {
        [self.js, self.style, self.layout, self.scene, self.encode, self.render, self.present]
    }

    /// The frame's whole work: the seven, added.
    pub fn busy(&self) -> f64 {
        self.stages().iter().sum()
    }

    /// Add the resolves `stats` holds, which ran inside `within` ms of input or turns: their
    /// style and layout to those stages, the rest of `within` to `js`.
    pub fn add_script(&mut self, within: f64, stats: &ResolveStats) {
        let (style, layout) = (stats.style.as_secs_f64() * 1000.0, stats.layout.as_secs_f64() * 1000.0);
        self.style += style;
        self.layout += layout;
        self.js += (within - style - layout).max(0.0);
    }

    /// Add the redraw's own resolve, `within` ms long: its style to `style`, the rest to `layout`.
    pub fn add_resolve(&mut self, within: f64, stats: &ResolveStats) {
        let style = (stats.style.as_secs_f64() * 1000.0).min(within);
        self.style += style;
        self.layout += within - style;
    }

    pub fn json(&self) -> Value {
        let mut out = json!({ "busy": self.busy() });
        for (name, ms) in STAGES.iter().zip(self.stages()) {
            out[name] = json!(ms);
        }
        out
    }
}

impl fmt::Display for Phases {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "busy {:.1} ms:", self.busy())?;
        for (i, (name, ms)) in STAGES.iter().zip(self.stages()).enumerate() {
            write!(f, "{} {name} {ms:.2}", if i == 0 { "" } else { "," })?;
        }
        Ok(())
    }
}

/// How much of each kind of work a frame ran: what a cut is meant to bring to 0.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Counts {
    /// Resolves that ran Stylo's traversal.
    pub style_traversals: u32,
    /// Boxes a layout algorithm ran for.
    pub layout_nodes: u32,
    /// Inline layouts (Parley's) built.
    pub text_layouts: u32,
    /// Glyph runs drawn; 0 on a renderer that does not count them.
    pub glyph_runs: u32,
    /// Glyphs drawn; 0 on a renderer that does not count them.
    pub glyphs: u32,
    /// Scroll containers whose content was built into the scene anew; 0 on a renderer that
    /// keeps none (the CPU ones).
    pub fragments_built: u32,
    /// Scroll containers whose content was drawn from what an earlier frame built.
    pub fragments_kept: u32,
}

impl Counts {
    pub fn add_resolves(&mut self, stats: &ResolveStats) {
        self.style_traversals += stats.style_traversals;
        self.layout_nodes += stats.layout_nodes;
        self.text_layouts += stats.text_layouts;
    }

    /// Set what the paint just done built and kept.
    pub fn take_fragments(&mut self) {
        let fragments = blitz_paint::take_fragment_stats();
        self.fragments_built = fragments.built;
        self.fragments_kept = fragments.kept;
    }

    pub fn json(&self) -> Value {
        json!({
            "styleTraversals": self.style_traversals,
            "layoutNodes": self.layout_nodes,
            "textLayouts": self.text_layouts,
            "glyphRuns": self.glyph_runs,
            "glyphs": self.glyphs,
            "fragmentsBuilt": self.fragments_built,
            "fragmentsKept": self.fragments_kept,
        })
    }
}

impl fmt::Display for Counts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "style traversals {}, layout nodes {}, text layouts {}, glyph runs {}, glyphs {}, fragments built {}, kept {}",
            self.style_traversals,
            self.layout_nodes,
            self.text_layouts,
            self.glyph_runs,
            self.glyphs,
            self.fragments_built,
            self.fragments_kept
        )
    }
}
