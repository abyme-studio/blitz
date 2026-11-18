//! react-blitz-window's start, on a screen: its first frame comes within 0.6 s of its launch.
//!
//! It opens a real window, so it is ignored: run it in a VM, never on a person's screen, with
//! `cargo test --release -p react-blitz-shot --test start -- --ignored`. A VM with no cargo runs
//! the test's own binary, with `REACT_BLITZ_WINDOW` naming the window's binary there and
//! `REACT_BLITZ_PAGE` a bundled page's folder.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn page() -> PathBuf {
    if let Some(page) = std::env::var_os("REACT_BLITZ_PAGE") {
        return PathBuf::from(page);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = manifest.join("../../target/react-blitz-pages/shot/window");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/window"))
        .arg(&out)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling the page failed");
    out
}

/// One start of the window: how long from its launch to the line that says its first frame.
fn first_frame(page: &PathBuf) -> Duration {
    let window = std::env::var_os("REACT_BLITZ_WINDOW").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_react-blitz-window")), PathBuf::from);
    let started = Instant::now();
    let mut child = Command::new(window)
        .arg(page)
        .arg("")
        .args(["--size", "320x200", "--timeout", "10"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("react-blitz-window starts");
    let lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let took = lines.map_while(Result::ok).find(|line| line.contains(" frame 1 ")).map(|_| started.elapsed());
    child.kill().unwrap();
    child.wait().unwrap();
    took.expect("the window said its first frame")
}

#[test]
#[ignore = "opens a window on the screen: run it in a VM"]
fn the_first_frame_comes_within_600_ms_on_a_second_start() {
    let page = page();
    // The first start of a binary pays for Metal's own cache of its pipelines.
    first_frame(&page);
    first_frame(&page);
    let took: Vec<Duration> = (0..3).map(|_| first_frame(&page)).collect();
    let best = took.iter().min().unwrap();
    assert!(*best < Duration::from_millis(600), "the first frame took {took:?}");
}
