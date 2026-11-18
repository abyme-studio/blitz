//! The host calls as ops, one for one with `react-blitz/js`'s `Bridge`.

use std::time::Instant;

use deno_core::{OpState, op2};
use ::react_blitz::HostCalls;
use serde::Serialize;

/// The host calls, kept in the isolate's `OpState`.
pub struct HostBox(pub Box<dyn HostCalls>);

/// The event being dispatched, and JS's answer (1: preventDefault, 2: stopPropagation).
pub struct EventIn {
    pub kind: u32,
    pub chain: Vec<u32>,
    pub fields: String,
    pub flags: u32,
}

/// The frame batch being sent.
pub struct FrameIn(pub String);

struct Started(Instant);

fn host(state: &mut OpState) -> &mut dyn HostCalls {
    &mut *state.borrow_mut::<HostBox>().0
}

#[op2(fast)]
fn op_rb_commit(state: &mut OpState, #[buffer] ops: &[u8], #[buffer] strings: &[u8]) {
    host(state).commit(ops, strings);
}

#[op2]
#[serde]
fn op_rb_rect(state: &mut OpState, id: u32) -> Vec<f64> {
    host(state).rect(id).to_vec()
}

#[op2]
#[serde]
fn op_rb_metrics(state: &mut OpState, id: u32) -> Vec<f64> {
    host(state).metrics(id).to_vec()
}

#[op2(fast)]
fn op_rb_offset_parent(state: &mut OpState, id: u32) -> u32 {
    host(state).offset_parent(id)
}

#[op2]
#[serde]
fn op_rb_scroll(state: &mut OpState, id: u32) -> Vec<f64> {
    host(state).scroll(id).to_vec()
}

#[op2(fast)]
fn op_rb_scroll_to(state: &mut OpState, id: u32, x: f64, y: f64) {
    host(state).scroll_to(id, x, y);
}

#[op2]
#[string]
fn op_rb_style(state: &mut OpState, id: u32, #[string] property: &str) -> String {
    host(state).style(id, property)
}

#[op2(fast)]
fn op_rb_focus(state: &mut OpState, id: u32) {
    host(state).focus(id);
}

#[op2(fast)]
fn op_rb_blur(state: &mut OpState, id: u32) {
    host(state).blur(id);
}

#[op2(fast)]
fn op_rb_focused(state: &mut OpState) -> u32 {
    host(state).focused()
}

#[op2(fast)]
fn op_rb_matches(state: &mut OpState, id: u32, #[string] selector: &str) -> bool {
    host(state).matches(id, selector)
}

#[op2(fast)]
fn op_rb_find(state: &mut OpState, id: u32, #[string] selector: &str) -> u32 {
    host(state).find(id, selector)
}

#[op2]
#[serde]
fn op_rb_find_all(state: &mut OpState, id: u32, #[string] selector: &str) -> Vec<u32> {
    host(state).find_all(id, selector)
}

#[op2(fast)]
fn op_rb_observe(state: &mut OpState, id: u32, #[string] kind: &str, on: bool) {
    host(state).observe(id, kind, on);
}

#[op2(fast)]
fn op_rb_frame(state: &mut OpState) {
    host(state).frame();
}

#[op2]
#[serde]
fn op_rb_viewport(state: &mut OpState) -> Vec<f64> {
    host(state).viewport().to_vec()
}

#[op2(fast)]
fn op_rb_adopt(state: &mut OpState, #[string] which: &str) -> u32 {
    host(state).adopt(which)
}

#[op2(fast)]
fn op_rb_at(state: &mut OpState, x: f64, y: f64) -> u32 {
    host(state).at(x, y)
}

#[op2]
#[string]
fn op_rb_tag(state: &mut OpState, id: u32) -> String {
    host(state).tag(id)
}

/// The attribute's value, as a list of none or one.
#[op2]
#[serde]
fn op_rb_attr(state: &mut OpState, id: u32, #[string] name: &str) -> Vec<String> {
    host(state).attr(id, name).into_iter().collect()
}

#[op2]
#[serde]
fn op_rb_children(state: &mut OpState, id: u32) -> Vec<u32> {
    host(state).children(id)
}

#[op2(fast)]
fn op_rb_has_focus(state: &mut OpState) -> bool {
    host(state).has_focus()
}

#[op2]
#[string]
fn op_rb_address(state: &mut OpState) -> String {
    host(state).address()
}

#[op2(fast)]
fn op_rb_log(state: &mut OpState, #[string] level: &str, #[string] text: &str) {
    if state.has::<HostBox>() {
        host(state).log(level, text);
    } else {
        eprintln!("[js {level}] {text}");
    }
}

#[derive(Serialize)]
struct EventOut {
    kind: u32,
    chain: Vec<u32>,
    fields: String,
}

#[op2]
#[serde]
fn op_rb_event_in(state: &mut OpState) -> Option<EventOut> {
    let e = state.try_borrow::<EventIn>()?;
    Some(EventOut {
        kind: e.kind,
        chain: e.chain.clone(),
        fields: e.fields.clone(),
    })
}

#[op2(fast)]
fn op_rb_event_out(state: &mut OpState, flags: u32) {
    if let Some(e) = state.try_borrow_mut::<EventIn>() {
        e.flags = flags;
    }
}

#[op2]
#[string]
fn op_rb_frame_in(state: &mut OpState) -> String {
    state.try_take::<FrameIn>().map(|f| f.0).unwrap_or_default()
}

/// Milliseconds on the page's clock: `performance.now()`. Before the host is registered, since
/// the isolate started.
#[op2(fast)]
fn op_rb_now(state: &mut OpState) -> f64 {
    if let Some(host) = state.try_borrow_mut::<HostBox>() {
        return host.0.now();
    }
    if !state.has::<Started>() {
        state.put(Started(Instant::now()));
    }
    state.borrow::<Started>().0.elapsed().as_secs_f64() * 1000.0
}

/// Whether the page's clock is stepped: a timer made now falls due on it.
#[op2(fast)]
fn op_rb_stepped(state: &mut OpState) -> bool {
    state.try_borrow_mut::<HostBox>().is_some_and(|h| h.0.stepped())
}

/// A stepped timer ran: the engine's pump runs microtasks, then looks for the next.
pub struct TimerRan;

#[op2(fast)]
fn op_rb_timer_ran(state: &mut OpState) {
    state.put(TimerRan);
}

#[op2(fast)]
fn op_rb_random(#[buffer] buf: &mut [u8]) {
    let _ = getrandom::fill(buf);
}

#[derive(Serialize)]
struct UrlParts {
    href: String,
    protocol: String,
    username: String,
    password: String,
    host: String,
    hostname: String,
    port: String,
    pathname: String,
    search: String,
    hash: String,
    origin: String,
}

/// `new URL(href, base)`'s parts, or nothing when it does not parse.
#[op2]
#[serde]
fn op_rb_url(#[string] href: &str, #[string] base: &str) -> Option<UrlParts> {
    let url = if base.is_empty() {
        url::Url::parse(href)
    } else {
        url::Url::parse(base).and_then(|b| b.join(href))
    }
    .ok()?;
    let opt = |s: Option<&str>, pre: &str| s.filter(|s| !s.is_empty()).map(|s| format!("{pre}{s}")).unwrap_or_default();
    Some(UrlParts {
        href: url.to_string(),
        protocol: format!("{}:", url.scheme()),
        username: url.username().to_string(),
        password: url.password().unwrap_or("").to_string(),
        host: match (url.host_str(), url.port()) {
            (Some(h), Some(p)) => format!("{h}:{p}"),
            (Some(h), None) => h.to_string(),
            _ => String::new(),
        },
        hostname: url.host_str().unwrap_or("").to_string(),
        port: url.port().map(|p| p.to_string()).unwrap_or_default(),
        pathname: url.path().to_string(),
        search: opt(url.query(), "?"),
        hash: opt(url.fragment(), "#"),
        origin: url.origin().ascii_serialization(),
    })
}

deno_core::extension!(
    rb_ext,
    ops = [
        op_rb_commit, op_rb_rect, op_rb_metrics, op_rb_offset_parent, op_rb_scroll, op_rb_scroll_to,
        op_rb_style, op_rb_focus, op_rb_blur, op_rb_focused, op_rb_matches, op_rb_find,
        op_rb_find_all, op_rb_observe, op_rb_frame, op_rb_viewport, op_rb_adopt, op_rb_at, op_rb_tag,
        op_rb_attr, op_rb_children, op_rb_has_focus, op_rb_address, op_rb_log, op_rb_event_in,
        op_rb_event_out, op_rb_frame_in, op_rb_now, op_rb_stepped, op_rb_timer_ran, op_rb_random,
        op_rb_url,
    ],
);

pub fn extension() -> deno_core::Extension {
    rb_ext::init()
}
