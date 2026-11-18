//! What a document's resolves cost, for an embedder that logs its frames.

use std::time::Duration;

/// The work of every [`resolve`](crate::BaseDocument::resolve) since the stats were last taken
/// ([`take_resolve_stats`](crate::BaseDocument::take_resolve_stats)).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct ResolveStats {
    /// Time in the style pass: flushing the stylist and Stylo's traversal.
    pub style: Duration,
    /// Time in the rest of the resolve: damage, box construction, layout, transforms and the
    /// paint tree.
    pub layout: Duration,
    /// How many resolves ran Stylo's traversal: a resolve with no dirty style runs none.
    pub style_traversals: u32,
    /// How many boxes a layout algorithm ran for: a box whose cached layout answered counts none.
    pub layout_nodes: u32,
    /// How many inline layouts (Parley's) were built.
    pub text_layouts: u32,
}
