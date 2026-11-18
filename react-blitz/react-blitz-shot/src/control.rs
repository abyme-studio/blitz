//! `react-blitz-window --control`: a page on a stepped 60 Hz clock, steered by JSON lines on a Unix
//! socket. Its clock moves one frame only on `frame`, never on wall time. Input enters as Blitz's
//! `UiEvent`s through `ReactDocument::handle_ui_event`, the path a window's input takes. Each frame
//! is painted by the shot's CPU path ([`crate::paint`]), with no window.
//!
//! On connecting, the window says `{"ready","time","width","height","scale"}`. Then one answer line
//! per command line; a command it cannot do answers `{"error"}`:
//!
//! - `{"cmd":"move","x","y"}`, `{"cmd":"down","x","y","button"?}`, `{"cmd":"up","x","y","button"?}`:
//!   the pointer, in CSS pixels from the viewport's top left; `button` is `left` (the default),
//!   `right` or `middle`. Answers `{"ok":true}`.
//! - `{"cmd":"wheel","x","y","dx","dy","mode"?,"phase"?}`: a wheel at (x, y), the pointer moved
//!   there first when it is elsewhere (Blitz scrolls what the pointer is over); `dx`, `dy` as the
//!   DOM's `deltaX`, `deltaY` (positive scrolls down and right), in `pixels` (the default) or `lines`.
//!   `phase` is macOS's: `began`, `changed`, `ended`, `momentum` or `momentum-ended`; Blitz's wheel
//!   carries no phase, so momentum is its deltas, sent as a trackpad sends them. Answers `{"ok":true}`.
//! - `{"cmd":"keydown"|"keyup","key","code"?,"text"?}`: a key, by the DOM's `key` and `code`.
//!   Answers `{"ok":true}`.
//!   Every input command takes `"mods"`: any of `shift`, `ctrl`, `alt`, `meta`.
//! - `{"cmd":"frame"}`: step the clock one 60 Hz frame and draw it. Answers `{"frame","time"}`, time in ms.
//! - `{"cmd":"watch","selectors":[…]}`: the selectors `boxes` answers, read now and after each frame.
//!   Answers `{"ok":true}`.
//! - `{"cmd":"boxes"}`: each watched selector's first match as the last frame laid it out:
//!   `{"frame","boxes":[{"selector","x","y","width","height","opacity"} | {"selector","missing":true}]}`,
//!   the border box in CSS pixels from the viewport's top left, `opacity` its own times its ancestors'.
//! - `{"cmd":"shot","path"}`: the last frame, as a PNG at `path`. Answers `{"path","width","height"}`.
//! - `{"cmd":"busy"}`: the last frame's work in ms, by stage:
//!   `{"frame","js","style","layout","scene","encode","render","present","busy"}`, `busy` their sum
//!   ([`Phases`] says each; `encode` and `present` are 0, the CPU renderer timing no painter call
//!   and there being no window). `paint` is the scene and the render, as it was first answered.
//!   `counts` is how much ran ([`Counts`]):
//!   `{"styleTraversals","layoutNodes","textLayouts","glyphRuns","glyphs","glyphOutlines","fragmentsBuilt","fragmentsKept"}`.
//! - `{"cmd":"quit"}`: answers `{"ok":true}` and closes.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use atomic_refcell::AtomicRefCell;
use blitz_dom::{Document as _, DocumentConfig};
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta, BlitzWheelEvent, KeyState,
    MouseEventButton, MouseEventButtons, PointerCoords, PointerDetails, UiEvent,
};
use keyboard_types::{Code, Key, Location, Modifiers};
use react_blitz::{Clock, DistLoader, ReactDocument, ReactDocumentConfig};
use react_blitz_v8::V8Engine;
use serde_json::{Value, json};

use crate::phases::{Counts, Phases};
use crate::{Image, Shot, paint_timed, root_attr, write_png};

/// One 60 Hz frame, in seconds.
pub const FRAME: f64 = 1.0 / 60.0;

/// How long one turn of JS may run before the frame goes on without it: a guard against a page
/// that never stops, not a budget a page meets.
const TURN: Duration = Duration::from_secs(10);

/// A watched selector's box after a frame.
enum Watched {
    Laid { x: f64, y: f64, width: f64, height: f64, opacity: f32 },
    Missing,
}

/// A page on a stepped clock, as `--control` drives it.
pub struct Control {
    doc: ReactDocument<V8Engine>,
    width: u32,
    height: u32,
    scale: f32,
    frame: u64,
    /// The pointer's buttons held, as a window keeps them.
    buttons: MouseEventButtons,
    /// Where the pointer is, once anything has put it somewhere: Blitz scrolls what it hovers.
    pointer: Option<(f32, f32)>,
    /// JS's ms since the last frame: the input's.
    input_js: f64,
    busy: Phases,
    /// How much the last frame ran.
    counts: Counts,
    image: Image,
    selectors: Vec<String>,
    boxes: Vec<Watched>,
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

impl Control {
    /// Open the page on a stepped clock at 0, run it (the clock still) until its root element says
    /// `data-preview-ready` or `shot.timeout` passes, and draw that frame.
    pub fn open(shot: &Shot) -> anyhow::Result<Self> {
        let loader = Rc::new(DistLoader::new(&shot.dist)?);
        let engine = V8Engine::new(loader)?;
        let doc = ReactDocument::open(
            ReactDocumentConfig {
                dist: shot.dist.clone(),
                page: shot.page.clone(),
                query: shot.query.clone(),
                document: DocumentConfig {
                    viewport: Some(shot.viewport()),
                    ..Default::default()
                },
                clock: Clock::Stepped,
            },
            engine,
        )?;
        let mut this = Self {
            doc,
            width: shot.width,
            height: shot.height,
            scale: shot.scale,
            frame: 0,
            buttons: MouseEventButtons::None,
            pointer: None,
            input_js: 0.0,
            busy: Phases::default(),
            counts: Counts::default(),
            image: Image { width: 0, height: 0, rgba: Vec::new(), boxes: Vec::new() },
            selectors: Vec::new(),
            boxes: Vec::new(),
        };
        let deadline = Instant::now() + shot.timeout;
        while !this.ready() && Instant::now() < deadline {
            if let Some(error) = root_attr(&this.doc, "data-preview-error") {
                bail!("the page says it could not draw: {error}");
            }
            if !this.doc.run(TURN) {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        this.draw();
        Ok(this)
    }

    /// Whether the page's root element says `data-preview-ready`.
    pub fn ready(&self) -> bool {
        root_attr(&self.doc, "data-preview-ready").is_some()
    }

    /// The page's clock, in ms.
    pub fn time(&self) -> f64 {
        self.doc.time() * 1000.0
    }

    /// The last frame, painted.
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// Step the clock one frame and draw it.
    pub fn step(&mut self) {
        self.frame += 1;
        self.doc.set_time(self.frame as f64 * FRAME);
        self.draw();
    }

    /// One frame at the clock as it stands: JS's due work and frame callbacks, style, layout, paint.
    fn draw(&mut self) {
        let t = self.doc.time();
        let input = std::mem::take(&mut self.input_js);
        let start = Instant::now();
        self.doc.run(TURN);
        let turn = ms(start);
        let (mut busy, mut counts) = (Phases::default(), Counts::default());
        let script = self.doc.inner_mut().take_resolve_stats();
        busy.add_script(input + turn, &script);
        counts.add_resolves(&script);

        let start = Instant::now();
        self.doc.inner_mut().resolve(t);
        let resolve = ms(start);
        let redraw = self.doc.inner_mut().take_resolve_stats();
        busy.add_resolve(resolve, &redraw);
        counts.add_resolves(&redraw);

        let (width, height, rgba, scene, render) = paint_timed(&mut self.doc.inner_mut(), self.width, self.height, self.scale);
        self.image = Image { width, height, rgba, boxes: Vec::new() };
        self.busy = Phases { scene, render, ..busy };
        counts.take_fragments();
        self.counts = counts;
        self.read_boxes();
    }

    fn read_boxes(&mut self) {
        let inner = self.doc.inner();
        self.boxes = self
            .selectors
            .iter()
            .map(|selector| {
                let Some(id) = inner.query_selector(selector).ok().flatten() else {
                    return Watched::Missing;
                };
                let Some(r) = crate::laid_rect(&inner, id) else {
                    return Watched::Missing;
                };
                let mut opacity = 1.0;
                let mut at = Some(id);
                while let Some(node) = at.and_then(|n| inner.get_node(n)) {
                    if let Some(style) = node.primary_styles() {
                        opacity *= style.get_effects().opacity;
                    }
                    at = node.parent;
                }
                Watched::Laid { x: r.x, y: r.y, width: r.width, height: r.height, opacity }
            })
            .collect();
    }

    fn input(&mut self, event: UiEvent) {
        let start = Instant::now();
        self.doc.handle_ui_event(event);
        self.input_js += ms(start);
    }

    /// Do one command line and answer it.
    pub fn command(&mut self, line: &str) -> Value {
        match self.try_command(line) {
            Ok(answer) => answer,
            Err(e) => json!({ "error": format!("{e:#}") }),
        }
    }

    fn try_command(&mut self, line: &str) -> anyhow::Result<Value> {
        let c: Value = serde_json::from_str(line).context("a command is one JSON object per line")?;
        let cmd = c["cmd"].as_str().context("a command needs \"cmd\"")?;
        let num = |k: &str| c[k].as_f64().with_context(|| format!("{cmd} needs a number \"{k}\""));
        let mods = mods(&c["mods"])?;
        let ok = json!({ "ok": true });
        Ok(match cmd {
            "move" | "down" | "up" => {
                let (x, y) = (num("x")? as f32, num("y")? as f32);
                let button = match c["button"].as_str().unwrap_or("left") {
                    "left" => MouseEventButton::Main,
                    "right" => MouseEventButton::Secondary,
                    "middle" => MouseEventButton::Auxiliary,
                    other => bail!("button is left, right or middle, not {other}"),
                };
                match cmd {
                    "down" => self.buttons |= button.into(),
                    "up" => self.buttons &= !MouseEventButtons::from(button),
                    _ => {}
                }
                self.pointer = Some((x, y));
                let event = BlitzPointerEvent {
                    id: BlitzPointerId::Mouse,
                    is_primary: true,
                    coords: self.coords(x, y),
                    button: if cmd == "move" { Default::default() } else { button },
                    buttons: self.buttons,
                    mods,
                    details: PointerDetails::default(),
                    element: Default::default(),
                    active_pointers: Arc::new(AtomicRefCell::new(Vec::new())),
                };
                self.input(match cmd {
                    "down" => UiEvent::PointerDown(event),
                    "up" => UiEvent::PointerUp(event),
                    _ => UiEvent::PointerMove(event),
                });
                ok
            }
            "wheel" => {
                let (x, y) = (num("x")? as f32, num("y")? as f32);
                let (dx, dy) = (c["dx"].as_f64().unwrap_or(0.0), c["dy"].as_f64().unwrap_or(0.0));
                match c["phase"].as_str() {
                    None | Some("began" | "changed" | "ended" | "momentum" | "momentum-ended") => {}
                    Some(other) => bail!("phase is began, changed, ended, momentum or momentum-ended, not {other}"),
                }
                // Blitz's wheel, as the window makes it from winit's: positive scrolls up and left.
                let delta = match c["mode"].as_str().unwrap_or("pixels") {
                    "pixels" => BlitzWheelDelta::Pixels(-dx, -dy),
                    "lines" => BlitzWheelDelta::Lines(-dx, -dy),
                    other => bail!("mode is pixels or lines, not {other}"),
                };
                // A wheel is at the pointer: one somewhere else moves there first, as a hand would.
                if self.pointer != Some((x, y)) {
                    self.pointer = Some((x, y));
                    self.input(UiEvent::PointerMove(BlitzPointerEvent {
                        id: BlitzPointerId::Mouse,
                        is_primary: true,
                        coords: self.coords(x, y),
                        button: Default::default(),
                        buttons: self.buttons,
                        mods,
                        details: PointerDetails::default(),
                        element: Default::default(),
                        active_pointers: Arc::new(AtomicRefCell::new(Vec::new())),
                    }));
                }
                let event = BlitzWheelEvent {
                    delta,
                    phase: Default::default(),
                    source: Default::default(),
                    coords: self.coords(x, y),
                    buttons: self.buttons,
                    mods,
                    element: Default::default(),
                };
                self.input(UiEvent::Wheel(event));
                ok
            }
            "keydown" | "keyup" => {
                let key = c["key"].as_str().context("a key needs \"key\"")?;
                let key = Key::from_str(key).unwrap_or_else(|_| Key::Character(key.into()));
                let code = c["code"].as_str().and_then(|s| Code::from_str(s).ok()).unwrap_or(Code::Unidentified);
                let down = cmd == "keydown";
                let text = if down {
                    c["text"].as_str().map(Into::into).or_else(|| match &key {
                        Key::Character(s) => Some(s.as_str().into()),
                        _ => None,
                    })
                } else {
                    None
                };
                let event = BlitzKeyEvent {
                    key,
                    code,
                    modifiers: mods,
                    location: Location::Standard,
                    is_auto_repeating: false,
                    is_composing: false,
                    state: if down { KeyState::Pressed } else { KeyState::Released },
                    text,
                };
                self.input(if down { UiEvent::KeyDown(event) } else { UiEvent::KeyUp(event) });
                ok
            }
            "frame" => {
                self.step();
                json!({ "frame": self.frame, "time": self.time() })
            }
            "watch" => {
                let list = c["selectors"].as_array().context("watch needs \"selectors\", a list")?;
                self.selectors = list
                    .iter()
                    .map(|s| s.as_str().map(str::to_string).context("each selector is a string"))
                    .collect::<anyhow::Result<_>>()?;
                self.read_boxes();
                ok
            }
            "boxes" => {
                let boxes: Vec<Value> = self
                    .selectors
                    .iter()
                    .zip(&self.boxes)
                    .map(|(selector, b)| match b {
                        Watched::Laid { x, y, width, height, opacity } => json!({
                            "selector": selector, "x": x, "y": y, "width": width, "height": height, "opacity": opacity,
                        }),
                        Watched::Missing => json!({ "selector": selector, "missing": true }),
                    })
                    .collect();
                json!({ "frame": self.frame, "boxes": boxes })
            }
            "shot" => {
                let path = c["path"].as_str().context("shot needs \"path\"")?;
                write_png(&self.image, Path::new(path))?;
                json!({ "path": path, "width": self.image.width, "height": self.image.height })
            }
            "busy" => {
                let b = self.busy;
                let mut answer = b.json();
                answer["frame"] = json!(self.frame);
                answer["paint"] = json!(b.scene + b.encode + b.render);
                answer["counts"] = self.counts.json();
                answer
            }
            "quit" => ok,
            other => bail!("no command {other}: move, down, up, wheel, keydown, keyup, frame, watch, boxes, shot, busy, quit"),
        })
    }

    /// A point in CSS pixels from the viewport's top left, as the window says where its pointer is.
    fn coords(&self, x: f32, y: f32) -> PointerCoords {
        let scroll = self.doc.inner().viewport_scroll();
        PointerCoords {
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
            page_x: x + scroll.x as f32,
            page_y: y + scroll.y as f32,
        }
    }

    /// What the window says on connecting.
    pub fn hello(&self) -> Value {
        json!({
            "ready": self.ready(),
            "time": self.time(),
            "width": self.width,
            "height": self.height,
            "scale": self.scale,
        })
    }
}

fn mods(v: &Value) -> anyhow::Result<Modifiers> {
    let mut m = Modifiers::empty();
    let Some(list) = v.as_array() else { return Ok(m) };
    for name in list {
        m |= match name.as_str().unwrap_or("") {
            "shift" => Modifiers::SHIFT,
            "ctrl" => Modifiers::CONTROL,
            "alt" => Modifiers::ALT,
            "meta" => Modifiers::META,
            other => bail!("mods are shift, ctrl, alt and meta, not {other}"),
        };
    }
    Ok(m)
}

/// Listen on `socket`, take one connection, and answer its lines until it closes or says `quit`.
pub fn serve(mut control: Control, socket: &Path) -> anyhow::Result<()> {
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket).with_context(|| format!("cannot listen on {}", socket.display()))?;
    let (stream, _) = listener.accept()?;
    let _ = std::fs::remove_file(socket);
    let mut out = stream.try_clone()?;
    writeln!(out, "{}", control.hello())?;
    for line in BufReader::new(stream).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let answer = control.command(&line);
        writeln!(out, "{answer}")?;
        let quit = serde_json::from_str::<Value>(&line).is_ok_and(|c| c["cmd"] == "quit");
        if quit {
            break;
        }
    }
    Ok(())
}
