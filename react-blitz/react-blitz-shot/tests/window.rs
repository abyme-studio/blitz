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
        let socket = std::env::temp_dir().join(format!("rbw-{}-{run}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let child = Command::new(env!("CARGO_BIN_EXE_react-blitz-window"))
            .arg(dist)
            .arg("")
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

/// One wheel tick's frame: six phases, each a number, adding up to its busy.
#[test]
fn a_wheel_ticks_frame_says_six_phases_that_sum_to_its_busy() {
    let dist = bundle("window");
    let mut w = Window::open(&dist, "phases");
    w.frame();
    w.send(json!({ "cmd": "wheel", "x": 100, "y": 100, "dx": 0, "dy": 40 }));
    let frame = w.frame()["frame"].clone();
    let busy = w.send(json!({ "cmd": "busy" }));
    assert_eq!(busy["frame"], frame, "{busy}");
    let phases: Vec<f64> = ["input", "turn", "resolve", "scene", "raster", "present"]
        .iter()
        .map(|k| busy[k].as_f64().unwrap_or_else(|| panic!("no {k} in {busy}")))
        .collect();
    assert!(phases.iter().all(|v| *v >= 0.0), "{busy}");
    let sum: f64 = phases.iter().sum();
    assert!((sum - busy["busy"].as_f64().unwrap()).abs() < 1e-9, "the six add to {sum}: {busy}");
    // The wheel was input, the scene was built and turned into pixels, and no window took them.
    assert!(busy["input"].as_f64().unwrap() > 0.0 && busy["scene"].as_f64().unwrap() > 0.0, "{busy}");
    assert!(busy["raster"].as_f64().unwrap() > 0.0, "{busy}");
    assert_eq!(busy["present"], 0.0, "{busy}");
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
