//! react-blitz-window --control, driven over its socket: a click reaches `onClick`, a drag's boxes
//! step once per `frame`, the clock moves a CSS animation one 60 Hz step per `frame`, and two runs
//! give identical shots.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

fn bundle(name: &str) -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = manifest.join("../../target/react-blitz-pages/shot").join(name);
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages").join(name))
        .arg(&out)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling {name} failed");
    out
}

/// A running react-blitz-window and its socket.
struct Window {
    child: Child,
    out: UnixStream,
    lines: BufReader<UnixStream>,
    hello: Value,
}

impl Window {
    fn open(dist: &Path, run: &str) -> Self {
        Self::open_at(dist, run, "")
    }

    /// The page opened with a query.
    fn open_at(dist: &Path, run: &str, query: &str) -> Self {
        let socket = std::env::temp_dir().join(format!("rbw-{}-{run}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let child = Command::new(env!("CARGO_BIN_EXE_react-blitz-window"))
            .arg(dist)
            .arg(query)
            .args(["--control", socket.to_str().unwrap(), "--size", "320x200", "--scale", "2", "--timeout", "20"])
            .spawn()
            .expect("react-blitz-window starts");
        let deadline = Instant::now() + Duration::from_secs(30);
        let stream = loop {
            match UnixStream::connect(&socket) {
                Ok(s) => break s,
                Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => panic!("no socket at {}: {e}", socket.display()),
            }
        };
        let mut lines = BufReader::new(stream.try_clone().unwrap());
        let mut hello = String::new();
        lines.read_line(&mut hello).unwrap();
        Self { child, out: stream, lines, hello: serde_json::from_str(&hello).unwrap() }
    }

    fn send(&mut self, command: Value) -> Value {
        writeln!(self.out, "{command}").unwrap();
        let mut line = String::new();
        self.lines.read_line(&mut line).unwrap();
        let answer: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{command} answered {line:?}: {e}"));
        assert!(answer.get("error").is_none(), "{command} answered {answer}");
        answer
    }

    fn frame(&mut self) -> Value {
        self.send(json!({ "cmd": "frame" }))
    }

    /// The watched selector's box, or nothing when it matches no element.
    fn get(&mut self, selector: &str) -> Option<Value> {
        let answer = self.send(json!({ "cmd": "boxes" }));
        let b = answer["boxes"].as_array().unwrap().iter().find(|b| b["selector"] == selector).cloned().unwrap();
        if b.get("missing").is_some() { None } else { Some(b) }
    }

    fn width(&mut self, selector: &str) -> f64 {
        self.get(selector).unwrap()["width"].as_f64().unwrap()
    }

    /// One wheel tick of `dy` px at (`x`, 30), its frame, and that frame's counts.
    fn wheel(&mut self, x: i32, dy: i32) -> Value {
        self.send(json!({ "cmd": "wheel", "x": x, "y": 30, "dx": 0, "dy": dy }));
        self.frame();
        self.send(json!({ "cmd": "busy" }))["counts"].clone()
    }

    fn close(mut self) {
        self.send(json!({ "cmd": "quit" }));
        let status = self.child.wait().unwrap();
        assert!(status.success(), "react-blitz-window exited {status}");
    }
}

/// The script both runs play: a click, a drag over four frames, two more frames; then the shot.
fn play(w: &mut Window, shot: &Path) {
    w.send(json!({ "cmd": "watch", "selectors": [".pane", ".badge", ".slider"] }));
    w.send(json!({ "cmd": "down", "x": 250, "y": 25 }));
    w.send(json!({ "cmd": "up", "x": 250, "y": 25 }));
    w.frame();
    w.send(json!({ "cmd": "down", "x": 100, "y": 40 }));
    for i in 1..=4 {
        w.send(json!({ "cmd": "move", "x": 100 + 10 * i, "y": 40, "mods": [] }));
        w.frame();
    }
    w.send(json!({ "cmd": "up", "x": 140, "y": 40 }));
    w.frame();
    w.frame();
    w.send(json!({ "cmd": "shot", "path": shot.to_str().unwrap() }));
}

#[test]
fn drive_steers_the_window_frame_by_frame() {
    let dist = bundle("window");
    let mut w = Window::open(&dist, "steer");
    assert_eq!(w.hello["ready"], true, "{}", w.hello);
    assert_eq!(w.hello["time"], 0.0);
    w.send(json!({ "cmd": "watch", "selectors": [".pane", ".badge", ".slider"] }));

    // A click reaches onClick: the badge it shows is there after the next frame, at its own
    // opacity times its parent's.
    assert!(w.get(".badge").is_none());
    w.send(json!({ "cmd": "move", "x": 250, "y": 25 }));
    w.send(json!({ "cmd": "down", "x": 250, "y": 25 }));
    w.send(json!({ "cmd": "up", "x": 250, "y": 25 }));
    let f = w.frame();
    assert_eq!(f["frame"], 1);
    assert!((f["time"].as_f64().unwrap() - 1000.0 / 60.0).abs() < 1e-9, "{f}");
    let badge = w.get(".badge").expect("the click showed the badge");
    assert!((badge["opacity"].as_f64().unwrap() - 0.25).abs() < 1e-6, "{badge}");
    assert_eq!((badge["x"].as_f64(), badge["y"].as_f64()), (Some(200.0), Some(50.0)), "{badge}");

    // A drag: the pane's box moves only on `frame`, once per frame, by the pointer's move.
    let start = w.width(".pane");
    assert_eq!(start, 100.0);
    w.send(json!({ "cmd": "down", "x": 100, "y": 40 }));
    for i in 1..=4 {
        w.send(json!({ "cmd": "move", "x": 100 + 10 * i, "y": 40 }));
        assert_eq!(w.width(".pane"), start + 10.0 * (i - 1) as f64, "the box moved before its frame");
        w.frame();
        assert_eq!(w.width(".pane"), start + 10.0 * i as f64, "frame {i}");
    }
    w.send(json!({ "cmd": "up", "x": 140, "y": 40 }));
    w.send(json!({ "cmd": "move", "x": 180, "y": 40 }));
    w.frame();
    assert_eq!(w.width(".pane"), 140.0, "the drag ended on up");

    // The clock: the slide (600 px in 1 s, linear) is 10 px further each frame, never on wall time.
    let x = |w: &mut Window| w.get(".slider").unwrap()["x"].as_f64().unwrap();
    let before = x(&mut w);
    std::thread::sleep(Duration::from_millis(100));
    w.send(json!({ "cmd": "watch", "selectors": [".pane", ".badge", ".slider"] }));
    assert_eq!(x(&mut w), before, "the page moved on wall time");
    let f = w.frame();
    let n = f["frame"].as_f64().unwrap();
    let after = x(&mut w);
    assert!((after - 10.0 * n).abs() < 0.01 && (after - before - 10.0).abs() < 0.01, "slide at {before} then {after}, frame {n}");

    // busy says the frame's work, as it was first answered.
    let busy = w.send(json!({ "cmd": "busy" }));
    for k in ["js", "style", "layout", "paint"] {
        assert!(busy[k].as_f64().is_some_and(|v| v >= 0.0), "{busy}");
    }
    assert!(busy["paint"].as_f64().unwrap() > 0.0, "{busy}");

    // A wheel, with its phases and momentum, is input as any other.
    for (dy, phase) in [(4, "began"), (8, "changed"), (0, "ended"), (6, "momentum"), (0, "momentum-ended")] {
        w.send(json!({ "cmd": "wheel", "x": 100, "y": 100, "dx": 0, "dy": dy, "phase": phase }));
    }
    w.send(json!({ "cmd": "keydown", "key": "a", "code": "KeyA" }));
    w.send(json!({ "cmd": "keyup", "key": "a", "code": "KeyA" }));
    w.frame();
    w.close();
}

/// One wheel tick's frame: seven stages, each a number, adding up to its busy, and its counts.
#[test]
fn a_wheel_ticks_frame_says_seven_stages_that_sum_to_its_busy() {
    let dist = bundle("window");
    let mut w = Window::open(&dist, "phases");
    w.frame();
    w.send(json!({ "cmd": "wheel", "x": 100, "y": 100, "dx": 0, "dy": 40 }));
    let frame = w.frame()["frame"].clone();
    let busy = w.send(json!({ "cmd": "busy" }));
    assert_eq!(busy["frame"], frame, "{busy}");
    let stage = |k: &str| busy[k].as_f64().unwrap_or_else(|| panic!("no {k} in {busy}"));
    let stages: Vec<f64> = react_blitz_shot::phases::STAGES.iter().map(|k| stage(k)).collect();
    assert_eq!(stages.len(), 7);
    assert!(stages.iter().all(|v| *v >= 0.0), "{busy}");
    let sum: f64 = stages.iter().sum();
    assert!((sum - stage("busy")).abs() < 1e-9, "the seven add to {sum}: {busy}");
    // The wheel was input, the resolve ran, the scene was built and turned into pixels; the CPU
    // renderer times no painter call, and no window took the pixels.
    assert!(stage("js") > 0.0 && stage("layout") > 0.0 && stage("scene") > 0.0 && stage("render") > 0.0, "{busy}");
    assert_eq!((stage("encode"), stage("present")), (0.0, 0.0), "{busy}");
    // How much ran, as numbers: the page's first frame laid out boxes and built text.
    for k in ["styleTraversals", "layoutNodes", "textLayouts", "glyphRuns", "glyphs", "fragmentsBuilt", "fragmentsKept"] {
        assert!(busy["counts"][k].as_u64().is_some(), "no count {k} in {busy}");
    }
    w.close();
}

/// A scroll with nothing else: the pointer has left the scroller's rows, three wheel ticks move it,
/// and no frame of them runs a style traversal or lays out a box; the picture moved all the same.
#[test]
fn a_scroll_with_nothing_else_runs_no_style_and_no_layout() {
    let dist = bundle("rows");
    let dir = std::env::temp_dir();
    let shot = |w: &mut Window, name: &str| {
        let path = dir.join(format!("rbw-{}-{name}.png", std::process::id()));
        w.send(json!({ "cmd": "shot", "path": path.to_str().unwrap() }));
        std::fs::read(path).unwrap()
    };
    let mut w = Window::open(&dist, "rows");
    // Over the scroller, right of its rows: a wheel there hovers no row.
    w.send(json!({ "cmd": "wheel", "x": 180, "y": 30, "dx": 0, "dy": 20 }));
    w.frame();
    let before = shot(&mut w, "rows-before");
    for _ in 0..3 {
        w.send(json!({ "cmd": "wheel", "x": 180, "y": 30, "dx": 0, "dy": 20 }));
        w.frame();
        let counts = w.send(json!({ "cmd": "busy" }))["counts"].clone();
        assert_eq!((counts["styleTraversals"].as_u64(), counts["layoutNodes"].as_u64()), (Some(0), Some(0)), "{counts}");
    }
    let after = shot(&mut w, "rows-after");
    w.close();
    // The same offset reached in one tick by a second window, which laid the page out afresh.
    let mut fresh = Window::open(&dist, "rows-fresh");
    fresh.send(json!({ "cmd": "wheel", "x": 180, "y": 30, "dx": 0, "dy": 80 }));
    fresh.frame();
    let full = shot(&mut fresh, "rows-full");
    fresh.close();
    assert!(before != after, "three ticks moved nothing");
    assert!(after == full, "the scrolled frames' pixels differ from a fresh window's at that offset");
}

/// A hover after the scroll: the pointer rests on a row, a tick moves the next row under it, and
/// that frame restyles: the new row is the wide one.
#[test]
fn a_row_scrolled_under_the_pointer_restyles_to_its_hover() {
    let dist = bundle("rows");
    let mut w = Window::open(&dist, "hover");
    w.send(json!({ "cmd": "watch", "selectors": [".r1", ".r2"] }));
    w.send(json!({ "cmd": "move", "x": 50, "y": 30, "mods": [] }));
    w.frame();
    assert_eq!((w.width(".r1"), w.width(".r2")), (150.0, 100.0));
    let counts = w.wheel(50, 20);
    assert!(counts["styleTraversals"].as_u64().unwrap() >= 1, "{counts}");
    assert_eq!((w.width(".r1"), w.width(".r2")), (100.0, 150.0));
    w.close();
}

/// A running animation makes a frame a full one, restyled once: the frame's second resolve, at the
/// same time, finds the styles as the first left them.
#[test]
fn a_running_animation_restyles_once_a_frame() {
    let dist = bundle("rows");
    let mut w = Window::open_at(&dist, "spin", "spin");
    w.frame();
    w.frame();
    for _ in 0..3 {
        // Right of the rows, so no hover changes.
        let counts = w.wheel(180, 20);
        assert_eq!(counts["styleTraversals"].as_u64(), Some(1), "{counts}");
        assert_eq!(counts["layoutNodes"].as_u64(), Some(0), "{counts}");
    }
    w.close();
}

/// Three wheel ticks before one frame: the offset holds all three, and a scroll listener ran once.
#[test]
fn ticks_before_a_frame_scroll_once() {
    let dist = bundle("scroll");
    let mut w = Window::open(&dist, "scroll");
    w.send(json!({ "cmd": "watch", "selectors": [".count", ".top"] }));
    w.frame();
    assert_eq!((w.width(".count"), w.width(".top")), (0.0, 0.0));
    for dy in [10, 20, 30] {
        w.send(json!({ "cmd": "wheel", "x": 100, "y": 50, "dx": 0, "dy": dy }));
    }
    // The frame after the ticks sends the scroll; one more shows what the listener set.
    w.frame();
    w.frame();
    assert_eq!(w.width(".top"), 60.0, "the offset the listener read");
    assert_eq!(w.width(".count"), 10.0, "the listener's runs, 10 px each");
    // A tick a frame is a run a frame.
    w.send(json!({ "cmd": "wheel", "x": 100, "y": 50, "dx": 0, "dy": 5 }));
    w.frame();
    w.frame();
    assert_eq!((w.width(".count"), w.width(".top")), (20.0, 65.0));
    w.close();
}

#[test]
fn two_runs_give_identical_shots() {
    let dist = bundle("window");
    let dir = std::env::temp_dir();
    let shots: Vec<PathBuf> = ["a", "b"]
        .iter()
        .map(|run| {
            let shot = dir.join(format!("rbw-{}-{run}.png", std::process::id()));
            let mut w = Window::open(&dist, run);
            play(&mut w, &shot);
            w.close();
            shot
        })
        .collect();
    let (a, b) = (std::fs::read(&shots[0]).unwrap(), std::fs::read(&shots[1]).unwrap());
    assert!(a.len() > 1000, "the shot is empty");
    assert!(a == b, "two runs of the same script gave different shots");
}
