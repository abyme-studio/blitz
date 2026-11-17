//! React drawn by Blitz. [`ReactDocument`] is a Blitz [`Document`](blitz_dom::Document) over a
//! `BaseDocument` and a [`ScriptEngine`]: JS sends one commit per React commit, Rust sends input
//! events one at a time and observations once a frame.

mod document;
mod engine;
mod events;
mod frame;
mod host;
mod loader;

pub use document::{ReactDocument, ReactDocumentConfig};
pub use host::Host;
pub use engine::{EventFlags, HostCalls, ScriptEngine};
pub use events::EVENT_TYPES;
pub use host::{HostState, Shared};
pub use loader::{DistLoader, ModuleSource};
