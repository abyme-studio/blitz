use std::time::Instant;

/// What JS answers for an input event.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EventFlags {
    pub prevent_default: bool,
    pub stop_propagation: bool,
}

/// A JS engine, as `ReactDocument` drives it. Nothing above this trait names an engine.
pub trait ScriptEngine {
    /// Load and run the module at `url` with `source`; its imports come from the engine's loader.
    fn load_module(&mut self, url: &str, source: &str) -> anyhow::Result<()>;
    /// The host calls JS reaches as `globalThis.__reactBlitz`.
    fn register(&mut self, host: Box<dyn HostCalls>);
    /// One input event: its kind (an index into [`crate::EVENT_TYPES`]), the chain of JS node ids
    /// from target to root, and its fields as JSON.
    fn dispatch(&mut self, kind: u32, chain: &[u32], fields: &[u8]) -> EventFlags;
    /// The frame batch, as JSON, after layout.
    fn frame(&mut self, batch: &[u8]);
    /// Run due timers and microtasks until none is ready or `deadline` passes. Answers whether
    /// anything ran.
    fn pump(&mut self, deadline: Instant) -> bool;
    /// When the earliest wall-clock timer falls due; nothing when none waits.
    fn next_due(&self) -> Option<Instant> {
        None
    }
}

/// The calls JS makes into the host: `react-blitz/js`'s `Bridge`, one for one. Ids are JS ids.
pub trait HostCalls {
    fn commit(&mut self, ops: &[u8], strings: &[u8]);
    fn rect(&mut self, id: u32) -> [f64; 4];
    fn metrics(&mut self, id: u32) -> [f64; 10];
    fn offset_parent(&mut self, id: u32) -> u32;
    fn scroll(&mut self, id: u32) -> [f64; 2];
    fn scroll_to(&mut self, id: u32, x: f64, y: f64);
    fn style(&mut self, id: u32, property: &str) -> String;
    fn focus(&mut self, id: u32);
    fn blur(&mut self, id: u32);
    fn focused(&mut self) -> u32;
    fn matches(&mut self, id: u32, selector: &str) -> bool;
    fn find(&mut self, id: u32, selector: &str) -> u32;
    fn find_all(&mut self, id: u32, selector: &str) -> Vec<u32>;
    fn observe(&mut self, id: u32, kind: &str, on: bool);
    fn frame(&mut self);
    fn viewport(&mut self) -> [f64; 3];
    /// The width `text` takes on one line in a font of the page's, in CSS pixels.
    fn text_width(&mut self, text: &str, family: &str, size: f64, weight: f64, italic: bool) -> f64;
    fn adopt(&mut self, which: &str) -> u32;
    fn at(&mut self, x: f64, y: f64) -> u32;
    fn tag(&mut self, id: u32) -> String;
    fn attr(&mut self, id: u32, name: &str) -> Option<String>;
    fn children(&mut self, id: u32) -> Vec<u32>;
    fn has_focus(&mut self) -> bool;
    /// The page's address, as `location.href`.
    fn address(&mut self) -> String;
    fn log(&mut self, level: &str, text: &str);
    /// Milliseconds on the page's clock: `performance.now()`.
    fn now(&mut self) -> f64;
    /// Whether the page's clock is stepped: its timers then fall due on that clock, not wall time.
    fn stepped(&mut self) -> bool;
}
