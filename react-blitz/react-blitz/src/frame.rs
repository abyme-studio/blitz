//! The frame batch: once a frame, after layout, what JS observes (sizes, visibility, scrolls,
//! animation and transition ends) and its frame callbacks, in one call.

use serde_json::{Value, json};

use crate::host::Shared;

/// The batch for this frame, or nothing when JS has nothing to hear and asked for no frame.
pub fn batch(shared: &Shared) -> Option<Vec<u8>> {
    let time = shared.now() * 1000.0;
    let doc = shared.doc.borrow();
    let mut state = shared.state.borrow_mut();
    let state = &mut *state;

    let mut sizes = Vec::new();
    for (&id, last) in state.sizes.iter_mut() {
        let Some(node) = state.tree.node(id).and_then(|n| doc.get_node(n)) else {
            continue;
        };
        let l = node.final_layout();
        let (bw, bh) = (l.size.width as f64, l.size.height as f64);
        let cw = bw - (l.padding.left + l.padding.right + l.border.left + l.border.right) as f64;
        let ch = bh - (l.padding.top + l.padding.bottom + l.border.top + l.border.bottom) as f64;
        let now = [bw, bh, cw.max(0.0), ch.max(0.0)];
        if *last != Some(now) {
            *last = Some(now);
            sizes.push(json!([id, now[0], now[1], now[2], now[3]]));
        }
    }

    let vp = doc.viewport();
    let s = vp.scale();
    let (vw, vh) = (
        (vp.window_size.0 as f32 / s) as f64,
        (vp.window_size.1 as f32 / s) as f64,
    );
    let mut visible = Vec::new();
    for (&id, last) in state.visible.iter_mut() {
        let Some(node) = state.tree.node(id) else { continue };
        let Some(r) = doc.get_client_bounding_rect(node) else { continue };
        let (x0, y0) = (r.x.max(0.0), r.y.max(0.0));
        let (x1, y1) = ((r.x + r.width).min(vw), (r.y + r.height).min(vh));
        let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
        let area = r.width * r.height;
        let showing = inter > 0.0 || (area == 0.0 && x0 <= x1 && y0 <= y1);
        let ratio = if area > 0.0 { inter / area } else if showing { 1.0 } else { 0.0 };
        if *last != Some(showing) {
            *last = Some(showing);
            visible.push(json!([id, showing as u8, ratio, r.x, r.y, r.width, r.height]));
        }
    }

    let mut scroll = Vec::new();
    for node in state.scrolled.drain(..) {
        let Some(id) = state.tree.js_id(node) else { continue };
        let Some(n) = doc.get_node(node) else { continue };
        let p = n.scroll_offset();
        scroll.push(json!([id, p.x, p.y]));
    }

    let ends: Vec<Value> = Vec::new();

    if !state.frame_wanted && sizes.is_empty() && visible.is_empty() && scroll.is_empty() {
        return None;
    }
    state.frame_wanted = false;
    Some(
        serde_json::to_vec(&json!({
            "time": time,
            "sizes": sizes,
            "visible": visible,
            "scroll": scroll,
            "ends": ends,
        }))
        .unwrap(),
    )
}
