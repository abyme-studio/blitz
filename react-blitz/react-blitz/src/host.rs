use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use blitz_commit::Tree;
use blitz_dom::{BaseDocument, NodeData, NodeId, ScrollBehavior};

use crate::engine::HostCalls;

/// Ids the host gives nodes JS did not make start here, clear of JS's own.
const HOST_IDS: u32 = 1 << 31;

/// What `ReactDocument` and the host calls share: the document, and what JS knows of it.
pub struct Shared {
    pub doc: Rc<RefCell<BaseDocument>>,
    pub state: RefCell<HostState>,
}

pub struct HostState {
    pub tree: Tree,
    next_host_id: u32,
    /// Observed sizes: the last border and content size sent for each node.
    pub sizes: HashMap<u32, Option<[f64; 4]>>,
    /// Observed visibility: the last showing state sent for each node.
    pub visible: HashMap<u32, Option<bool>>,
    /// Nodes that scrolled since the last frame batch.
    pub scrolled: Vec<NodeId>,
    /// JS asked for a frame batch.
    pub frame_wanted: bool,
    pub address: String,
    pub started: Instant,
    /// The page's clock in seconds when it is stepped (`Clock::Stepped`): it moves only when the
    /// embedder sets it, never on wall time.
    pub clock: Option<f64>,
    /// Commits that did not apply, for a test or a log to read.
    pub errors: Vec<String>,
}

impl Shared {
    pub fn new(doc: BaseDocument, address: String) -> Rc<Self> {
        Rc::new(Self {
            doc: Rc::new(RefCell::new(doc)),
            state: RefCell::new(HostState {
                tree: Tree::new(),
                next_host_id: HOST_IDS,
                sizes: HashMap::new(),
                visible: HashMap::new(),
                scrolled: Vec::new(),
                frame_wanted: false,
                address,
                started: Instant::now(),
                clock: None,
                errors: Vec::new(),
            }),
        })
    }

    /// Seconds on the page's clock: the stepped clock when there is one, else since the document
    /// started. CSS animations, `requestAnimationFrame`'s time and `performance.now()` all read it.
    pub fn now(&self) -> f64 {
        let state = self.state.borrow();
        state.clock.unwrap_or_else(|| state.started.elapsed().as_secs_f64())
    }

    /// Resolve style and layout if anything is dirty.
    pub fn resolve(&self) {
        let now = self.now();
        self.doc.borrow_mut().resolve(now);
    }

    /// The JS id of a node, given one when JS has not seen it.
    pub fn js_id(&self, node: NodeId) -> u32 {
        let mut state = self.state.borrow_mut();
        if let Some(id) = state.tree.js_id(node) {
            return id;
        }
        let id = state.next_host_id;
        state.next_host_id += 1;
        state.tree.bind(id, node);
        id
    }

    pub fn node(&self, id: u32) -> Option<NodeId> {
        self.state.borrow().tree.node(id)
    }
}

/// The host calls over a [`Shared`]: what `ScriptEngine::register` takes.
pub struct Host(pub Rc<Shared>);

impl Host {
    fn read<T: Default>(&self, id: u32, f: impl FnOnce(&BaseDocument, NodeId) -> T) -> T {
        self.0.resolve();
        let Some(node) = self.0.node(id) else {
            return T::default();
        };
        let doc = self.0.doc.borrow();
        if doc.get_node(node).is_none() {
            return T::default();
        }
        f(&doc, node)
    }
}

fn scroll_offset(doc: &BaseDocument, node: NodeId) -> (f64, f64) {
    if doc.try_root_element().is_some_and(|root| root.id == node) {
        let p = doc.viewport_scroll();
        return (p.x, p.y);
    }
    doc.get_node(node)
        .filter(|n| n.element_data().is_some())
        .map(|n| {
            let p = n.scroll_offset();
            (p.x, p.y)
        })
        .unwrap_or((0.0, 0.0))
}

impl HostCalls for Host {
    fn commit(&mut self, ops: &[u8], strings: &[u8]) {
        let mut state = self.0.state.borrow_mut();
        let mut doc = self.0.doc.borrow_mut();
        if let Err(e) = state.tree.apply(&mut doc, ops, strings) {
            eprintln!("react-blitz: a commit did not apply: {e}");
            state.errors.push(e.to_string());
        }
    }

    fn rect(&mut self, id: u32) -> [f64; 4] {
        self.read(id, |doc, node| {
            doc.get_client_bounding_rect(node)
                .map(|r| [r.x, r.y, r.width, r.height])
                .unwrap_or_default()
        })
    }

    fn metrics(&mut self, id: u32) -> [f64; 10] {
        self.read(id, |doc, node| {
            let n = doc.get_node(node).unwrap();
            let layout = n.final_layout();
            let offset = doc.offset_rect(node);
            let is_root = doc.try_root_element().is_some_and(|r| r.id == node);
            let (cw, ch) = if is_root {
                let vp = doc.viewport();
                let s = vp.scale();
                (vp.window_size.0 as f32 / s, vp.window_size.1 as f32 / s)
            } else {
                (n.client_width(), n.client_height())
            };
            [
                offset.map_or(0.0, |r| r.width.round()),
                offset.map_or(0.0, |r| r.height.round()),
                cw.round() as f64,
                ch.round() as f64,
                n.scroll_width().round() as f64,
                n.scroll_height().round() as f64,
                layout.border.left.round() as f64,
                layout.border.top.round() as f64,
                offset.map_or(0.0, |r| r.x.round()),
                offset.map_or(0.0, |r| r.y.round()),
            ]
        })
    }

    fn offset_parent(&mut self, id: u32) -> u32 {
        let parent = self.read(id, |doc, node| {
            doc.get_node(node).and_then(|n| n.offset_parent()).map(|p| p.id)
        });
        parent.map_or(0, |p| self.0.js_id(p))
    }

    fn scroll(&mut self, id: u32) -> [f64; 2] {
        self.read(id, |doc, node| {
            let (x, y) = scroll_offset(doc, node);
            [x, y]
        })
    }

    fn scroll_to(&mut self, id: u32, x: f64, y: f64) {
        self.0.resolve();
        let Some(node) = self.0.node(id) else { return };
        let mut doc = self.0.doc.borrow_mut();
        if doc.try_root_element().is_some_and(|r| r.id == node) {
            doc.set_viewport_scroll(blitz_dom::Point { x, y });
        } else {
            doc.scroll_to(node, x, y, ScrollBehavior::Auto);
        }
    }

    fn style(&mut self, id: u32, property: &str) -> String {
        self.read(id, |doc, node| doc.resolved_style_value(node, property))
    }

    fn focus(&mut self, id: u32) {
        let Some(node) = self.0.node(id) else { return };
        self.0.doc.borrow_mut().set_focus_to(node);
    }

    fn blur(&mut self, id: u32) {
        let Some(node) = self.0.node(id) else { return };
        let mut doc = self.0.doc.borrow_mut();
        if doc.get_focussed_node_id() == Some(node) {
            doc.clear_focus();
        }
    }

    fn focused(&mut self) -> u32 {
        let node = self.0.doc.borrow().get_focussed_node_id();
        node.map_or(0, |n| self.0.js_id(n))
    }

    fn matches(&mut self, id: u32, selector: &str) -> bool {
        let Some(node) = self.0.node(id) else { return false };
        self.0
            .doc
            .borrow()
            .matches_selector(node, selector)
            .unwrap_or(false)
    }

    fn find(&mut self, id: u32, selector: &str) -> u32 {
        let Some(node) = self.0.node(id) else { return 0 };
        let found = self.0.doc.borrow().query_selector_in(node, selector).ok().flatten();
        found.map_or(0, |n| self.0.js_id(n))
    }

    fn find_all(&mut self, id: u32, selector: &str) -> Vec<u32> {
        let Some(node) = self.0.node(id) else { return Vec::new() };
        let found = self
            .0
            .doc
            .borrow()
            .query_selector_all_in(node, selector)
            .unwrap_or_default();
        found.into_iter().map(|n| self.0.js_id(n)).collect()
    }

    fn observe(&mut self, id: u32, kind: &str, on: bool) {
        let mut state = self.0.state.borrow_mut();
        match (kind, on) {
            ("size", true) => {
                state.sizes.insert(id, None);
            }
            ("size", false) => {
                state.sizes.remove(&id);
            }
            ("visible", true) => {
                state.visible.insert(id, None);
            }
            ("visible", false) => {
                state.visible.remove(&id);
            }
            _ => {}
        }
        if on {
            state.frame_wanted = true;
            drop(state);
            self.0.doc.borrow().shell_provider.request_redraw();
        }
    }

    fn frame(&mut self) {
        self.0.state.borrow_mut().frame_wanted = true;
        self.0.doc.borrow().shell_provider.request_redraw();
    }

    fn viewport(&mut self) -> [f64; 3] {
        let doc = self.0.doc.borrow();
        let vp = doc.viewport();
        let s = vp.scale();
        [
            (vp.window_size.0 as f32 / s) as f64,
            (vp.window_size.1 as f32 / s) as f64,
            s as f64,
        ]
    }

    fn adopt(&mut self, which: &str) -> u32 {
        let node = {
            let doc = self.0.doc.borrow();
            match which {
                "document" => Some(doc.root_node().id),
                "html" => doc.try_root_element().map(|n| n.id),
                "head" => doc.find_head_node().map(|n| n.id),
                "body" => doc.find_body_node().map(|n| n.id),
                _ => None,
            }
        };
        node.map_or(0, |n| self.0.js_id(n))
    }

    fn at(&mut self, x: f64, y: f64) -> u32 {
        self.0.resolve();
        let node = self.0.doc.borrow().element_from_point(x as f32, y as f32);
        node.map_or(0, |n| self.0.js_id(n))
    }

    fn tag(&mut self, id: u32) -> String {
        let Some(node) = self.0.node(id) else { return String::new() };
        let doc = self.0.doc.borrow();
        match doc.get_node(node).map(|n| &n.data) {
            Some(NodeData::Element(el)) => el.name.local.to_string(),
            Some(NodeData::Document(_)) => "#document".to_string(),
            _ => String::new(),
        }
    }

    fn attr(&mut self, id: u32, name: &str) -> Option<String> {
        let node = self.0.node(id)?;
        let doc = self.0.doc.borrow();
        let el = doc.get_node(node)?.element_data()?;
        el.attrs()
            .iter()
            .find(|a| &*a.name.local == name)
            .map(|a| a.value.clone())
    }

    fn children(&mut self, id: u32) -> Vec<u32> {
        let Some(node) = self.0.node(id) else { return Vec::new() };
        let children = self
            .0
            .doc
            .borrow()
            .get_node(node)
            .map(|n| n.children.to_vec())
            .unwrap_or_default();
        children.into_iter().map(|c| self.0.js_id(c)).collect()
    }

    fn has_focus(&mut self) -> bool {
        true
    }

    fn address(&mut self) -> String {
        self.0.state.borrow().address.clone()
    }

    fn log(&mut self, level: &str, text: &str) {
        eprintln!("[js {level}] {text}");
    }

    fn now(&mut self) -> f64 {
        self.0.now() * 1000.0
    }

    fn stepped(&mut self) -> bool {
        self.0.state.borrow().clock.is_some()
    }
}
