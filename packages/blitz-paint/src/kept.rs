//! A scroll container's content, kept between frames as a fragment of scene in the back end's
//! own form (`PaintScene::begin_fragment`), and drawn again under a translation while nothing
//! in it changed. A scroll then builds nothing in the container; a change in it, or a scroll
//! past what the fragment holds, builds it again.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use anyrender::{PaintScene, ResourceId};
use blitz_dom::{BaseDocument, NodeId, node::Node};
use kurbo::{Affine, Rect, Vec2};

/// How far past its scrollport, in physical pixels, a kept fragment holds the container's
/// content: a scroll within it builds nothing, and all of it is the renderer's work each frame.
const OVERSCAN: f64 = 256.0;
/// How many paints a fragment outlives its last use.
const UNUSED_PAINTS: u64 = 120;

/// A container's kept content.
struct Kept {
    fragment: ResourceId,
    /// The document's paint clock and epoch when it was built.
    clock: u64,
    epoch: u64,
    scale: f64,
    /// Where the scrolled content's origin was in the scene when it was built.
    origin: Vec2,
    /// The part of the scene it holds content for, as built.
    band: Rect,
    /// The paint clock when it was last drawn.
    used: u64,
}

/// What a paint did with fragments: `built` anew, `kept` from an earlier frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FragmentStats {
    pub built: u32,
    pub kept: u32,
}

thread_local! {
    /// Every kept fragment, by document and node.
    static KEPT: RefCell<HashMap<(usize, NodeId), Kept>> = RefCell::default();
    static ON: Cell<bool> = const { Cell::new(false) };
    static STATS: Cell<FragmentStats> = const { Cell::new(FragmentStats { built: 0, kept: 0 }) };
}

/// Keep fragments on this thread or, with false, build every frame whole, as a back end that
/// keeps none does. Off by default: on the chat's pages a kept fragment saved no time that a
/// run could measure, the scene being a tenth of a frame's work.
pub fn keep_fragments(keep: bool) {
    ON.set(keep);
}

/// The fragments built and kept since this was last called, on this thread.
pub fn take_fragment_stats() -> FragmentStats {
    STATS.take()
}

/// A fragment being built.
pub(crate) struct Building {
    pub(crate) band: Rect,
    origin: Vec2,
    outer_band: Option<Rect>,
    outer_unkeepable: bool,
}

/// One paint's view of the kept fragments.
pub(crate) struct KeptScene {
    doc: usize,
    clock: u64,
    epoch: u64,
    scale: f64,
    /// The band of the fragment being built, if one is, in screen coordinates: what stands in
    /// for the viewport there.
    band: Cell<Option<Rect>>,
    /// The fragment being built drew something that depends on where it is in the viewport.
    unkeepable: Cell<bool>,
}

fn translation(transform: Affine) -> Option<Vec2> {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    (a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0).then_some(Vec2::new(e, f))
}

impl KeptScene {
    /// Start a paint of `doc`: changes from here on count against what this paint keeps.
    pub(crate) fn new(doc: &BaseDocument, scale: f64) -> Self {
        Self {
            doc: doc.id(),
            clock: doc.tree().advance_paint_clock(),
            epoch: doc.tree().paint_epoch(),
            scale,
            band: Cell::new(None),
            unkeepable: Cell::new(false),
        }
    }

    /// The fragment being built holds content for this part of the scene, not the viewport.
    pub(crate) fn band(&self) -> Option<Rect> {
        self.band.get()
    }

    /// What is being drawn depends on where it sits in the viewport: it cannot be kept.
    pub(crate) fn unkeepable(&self) {
        self.unkeepable.set(true);
    }

    /// Whether `node`'s scrolled content is kept: a container with somewhere to scroll.
    fn keeps(node: &Node) -> bool {
        let size = node.final_layout().size;
        node.is_scroll_container()
            && (node.scroll_width() > size.width || node.scroll_height() > size.height)
    }

    /// Draw `node`'s kept content, if it still stands: nothing in it changed since it was
    /// built, and what shows through the scrollport (`visible`, in the scene) is inside what
    /// it holds. `transform` is the content's, scroll and all.
    pub(crate) fn append(
        &self,
        scene: &mut impl PaintScene,
        node: &Node,
        transform: Affine,
        visible: Rect,
    ) -> bool {
        let key = (self.doc, node.id);
        KEPT.with_borrow_mut(|kept| {
            let Some(entry) = kept.get_mut(&key) else {
                return false;
            };
            let shift = translation(transform).map(|now| now - entry.origin);
            let stands = ON.get() && shift.is_some_and(|shift| {
                let shown = visible - shift;
                node.paint_stamp.get() < entry.clock
                    && entry.epoch == self.epoch
                    && entry.scale == self.scale
                    && entry.band.contains_rect(shown)
            });
            if stands && scene.append_fragment(entry.fragment, Affine::translate(shift.unwrap())) {
                entry.used = self.clock;
                STATS.set(FragmentStats { kept: STATS.get().kept + 1, ..STATS.get() });
                return true;
            }
            scene.unregister_resource(entry.fragment);
            kept.remove(&key);
            false
        })
    }

    /// Start keeping `node`'s content, if it is a container to keep and the back end keeps
    /// fragments: what is drawn until [`Self::end`] goes into the fragment, culled to the
    /// band this answers and not to `visible`.
    pub(crate) fn begin(
        &self,
        scene: &mut impl PaintScene,
        node: &Node,
        transform: Affine,
        scrollport: Rect,
    ) -> Option<Building> {
        let origin = translation(transform)?;
        if !ON.get() || !Self::keeps(node) || !scene.begin_fragment() {
            return None;
        }
        let band = scrollport.inflate(OVERSCAN, OVERSCAN);
        Some(Building {
            band,
            origin,
            outer_band: self.band.replace(Some(band)),
            outer_unkeepable: self.unkeepable.replace(false),
        })
    }

    /// End the fragment and draw it where it was built. It is kept unless it drew something
    /// that depends on where it sits in the viewport.
    pub(crate) fn end(&self, scene: &mut impl PaintScene, node: &Node, building: Building) {
        let unkeepable = self.unkeepable.replace(building.outer_unkeepable);
        // What the outer fragment holds, this one included, is no more keepable than it.
        if unkeepable {
            self.unkeepable.set(true);
        }
        self.band.set(building.outer_band);
        let Some(fragment) = scene.end_fragment() else {
            return;
        };
        scene.append_fragment(fragment, Affine::IDENTITY);
        STATS.set(FragmentStats { built: STATS.get().built + 1, ..STATS.get() });
        if unkeepable {
            scene.unregister_resource(fragment);
            return;
        }
        let entry = Kept {
            fragment,
            clock: self.clock,
            epoch: self.epoch,
            scale: self.scale,
            origin: building.origin,
            band: building.band,
            used: self.clock,
        };
        if let Some(old) = KEPT.with_borrow_mut(|kept| kept.insert((self.doc, node.id), entry)) {
            scene.unregister_resource(old.fragment);
        }
    }

    /// Free this document's fragments that no paint has drawn for a while: their nodes are
    /// gone, or scroll no more.
    pub(crate) fn sweep(&self, scene: &mut impl PaintScene) {
        KEPT.with_borrow_mut(|kept| {
            kept.retain(|(doc, _), entry| {
                let stale = *doc == self.doc && entry.used + UNUSED_PAINTS < self.clock;
                if stale {
                    scene.unregister_resource(entry.fragment);
                }
                !stale
            });
        });
    }
}
