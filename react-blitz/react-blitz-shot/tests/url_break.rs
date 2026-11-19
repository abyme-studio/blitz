//! A long URL breaks where Chromium breaks it: after its "?", not after a "/" before a letter, as
//! UAX #14 alone would allow.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot};

#[test]
fn a_url_breaks_after_its_question_mark() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/url-break");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/url-break"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling url-break failed");
    let image = shoot(&Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 300,
        height: 80,
        scale: 1.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    })
    .unwrap();
    let inked = |x0: u32, x1: u32, y0: u32, y1: u32| {
        (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).any(|(x, y)| image.rgba[((y * image.width + x) * 4) as usize] < 128)
    };
    // The first line is "abyme:file?", about 80 px: nothing past 120 px. Broken after a "/" it
    // would hold "abyme:file?path=/abyme/fixtures/", about 250 px.
    assert!(inked(0, 60, 0, 20), "the first line has no text");
    assert!(!inked(120, 300, 0, 20), "the first line runs past its \"?\"");
    assert!(inked(120, 260, 20, 40), "the second line is not the path");
}
