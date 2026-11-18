//! `scrollbar-gutter: stable` keeps the scrollbar's width free at a scroller's right, and
//! none at its bottom.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot};

#[test]
fn a_stable_gutter_keeps_the_scrollbars_width() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/gutter");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/gutter"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling gutter failed");
    let image = shoot(&Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 320,
        height: 200,
        scale: 1.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    })
    .unwrap();
    let size = |path: &str| {
        let b = image.boxes.iter().find(|b| b.path == path).unwrap_or_else(|| panic!("no box {path}"));
        [b.width, b.height]
    };
    assert_eq!(size("0.0"), [189.0, 10.0], "thin: 11px kept");
    assert_eq!(size("1.0"), [185.0, 10.0], "auto: 15px kept");
    assert_eq!(size("2.0"), [200.0, 10.0], "scrollbar-width none: no gutter");
    assert_eq!(size("3.0"), [200.0, 10.0], "no scrollbar-gutter: none kept");
    assert_eq!(size("0"), [200.0, 40.0], "the scroller keeps its size, and no gutter at its bottom");
}
