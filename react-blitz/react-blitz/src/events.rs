//! Input events to JS, one at a time, synchronously: Blitz's event driver hands each DOM event to
//! `ReactDocument`'s handler, which sends it to the engine with its chain of JS ids and fields, and
//! takes back `preventDefault` and `stopPropagation`.

use blitz_dom::{Document, EventHandler, NodeId};
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta, DomEvent, DomEventData,
    EventState,
};
use keyboard_types::Modifiers;
use serde_json::{Value, json};

use crate::engine::ScriptEngine;
use crate::host::Shared;

/// The event kinds JS hears, by index: `react-blitz/js/src/events.ts` has the same list.
pub const EVENT_TYPES: &[&str] = &[
    "pointermove", "pointerdown", "pointerup", "pointercancel", "pointerenter", "pointerleave",
    "pointerover", "pointerout", "mousemove", "mousedown", "mouseup", "mouseenter", "mouseleave",
    "mouseover", "mouseout", "touchstart", "touchmove", "touchend", "touchcancel", "scroll", "wheel",
    "click", "contextmenu", "dblclick", "keypress", "keydown", "keyup", "input", "focus", "blur",
    "focusin", "focusout",
];

fn mods(m: &Modifiers) -> Value {
    json!({
        "shiftKey": m.shift(),
        "ctrlKey": m.ctrl(),
        "altKey": m.alt(),
        "metaKey": m.meta(),
    })
}

fn pointer(e: &BlitzPointerEvent) -> Value {
    let mut v = json!({
        "clientX": e.coords.client_x,
        "clientY": e.coords.client_y,
        "pageX": e.coords.page_x,
        "pageY": e.coords.page_y,
        "screenX": e.coords.screen_x,
        "screenY": e.coords.screen_y,
        "offsetX": e.element.x,
        "offsetY": e.element.y,
        "button": e.button as u8,
        "buttons": e.buttons.bits(),
        "pointerId": match e.id { BlitzPointerId::Mouse => 1, BlitzPointerId::Pen => 2, BlitzPointerId::Finger(n) => 10 + n },
        "pointerType": match e.id { BlitzPointerId::Mouse => "mouse", BlitzPointerId::Pen => "pen", BlitzPointerId::Finger(_) => "touch" },
        "isPrimary": e.is_primary,
        "pressure": e.details.pressure,
    });
    merge(&mut v, mods(&e.mods));
    v
}

fn key(e: &BlitzKeyEvent) -> Value {
    let mut v = json!({
        "key": e.key.to_string(),
        "code": e.code.to_string(),
        "repeat": e.is_auto_repeating,
        "isComposing": e.is_composing,
        "location": e.location as u32,
    });
    merge(&mut v, mods(&e.modifiers));
    v
}

fn merge(into: &mut Value, from: Value) {
    if let (Value::Object(a), Value::Object(b)) = (into, from) {
        a.extend(b);
    }
}

/// The fields of an event, as JS's event object takes them.
pub fn fields(data: &DomEventData) -> Value {
    use DomEventData::*;
    match data {
        PointerMove(e) | PointerDown(e) | PointerUp(e) | PointerCancel(e) | PointerEnter(e)
        | PointerLeave(e) | PointerOver(e) | PointerOut(e) | MouseMove(e) | MouseDown(e)
        | MouseUp(e) | MouseEnter(e) | MouseLeave(e) | MouseOver(e) | MouseOut(e)
        | TouchStart(e) | TouchMove(e) | TouchEnd(e) | TouchCancel(e) | Click(e)
        | ContextMenu(e) | DoubleClick(e) => pointer(e),
        Wheel(e) => {
            let (mode, x, y) = match e.delta {
                BlitzWheelDelta::Lines(x, y) => (1, x, y),
                BlitzWheelDelta::Pixels(x, y) => (0, x, y),
            };
            let mut v = json!({
                "deltaX": -x, "deltaY": -y, "deltaZ": 0, "deltaMode": mode,
                "clientX": e.coords.client_x, "clientY": e.coords.client_y,
                "buttons": e.buttons.bits(),
            });
            merge(&mut v, mods(&e.mods));
            v
        }
        KeyPress(e) | KeyDown(e) | KeyUp(e) => key(e),
        Input(e) => json!({ "value": e.value }),
        _ => json!({}),
    }
}

/// Sends each DOM event Blitz's driver makes to the engine.
pub(crate) struct ReactEventHandler<'a, E: ScriptEngine> {
    pub engine: &'a mut E,
    pub shared: &'a Shared,
}

impl<E: ScriptEngine> EventHandler for ReactEventHandler<'_, E> {
    fn handle_event(
        &mut self,
        chain: &[NodeId],
        event: &mut DomEvent,
        _doc: &mut dyn Document,
        state: &mut EventState,
    ) {
        let name = event.data.name();
        // Scrolls go in the next frame batch, as `scroll` events there.
        if let DomEventData::Scroll(_) = event.data {
            let mut s = self.shared.state.borrow_mut();
            if !s.scrolled.contains(&event.target) {
                s.scrolled.push(event.target);
            }
            s.frame_wanted = true;
            return;
        }
        let Some(kind) = EVENT_TYPES.iter().position(|t| *t == name) else {
            return;
        };
        // Blitz's chain holds elements only: an event that reaches the root element goes on to
        // the document, where `document`'s and `window`'s listeners are.
        let mut chain = chain.to_vec();
        {
            let doc = _doc.inner();
            let root = doc.root_node();
            if let Some(&last) = chain.last()
                && root.children.contains(&last)
            {
                chain.push(root.id);
            }
        }
        // Only when something in the chain (the document included) listens.
        {
            let s = self.shared.state.borrow();
            if !chain.iter().any(|&n| s.tree.listens(n, name)) {
                return;
            }
        }
        let ids: Vec<u32> = chain.iter().map(|&n| self.shared.js_id(n)).collect();
        let fields = serde_json::to_vec(&fields(&event.data)).unwrap();
        let flags = self.engine.dispatch(kind as u32, &ids, &fields);
        if flags.prevent_default {
            state.prevent_default();
        }
        if flags.stop_propagation {
            state.stop_propagation();
        }
        state.request_redraw();
    }
}
