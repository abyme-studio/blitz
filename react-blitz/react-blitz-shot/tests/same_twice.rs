//! The same page shot twice gives the same pixels, and the shot holds the page's own CSS.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot};

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

fn shot(dist: PathBuf) -> Shot {
    Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 320,
        height: 200,
        scale: 2.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    }
}

/// The pixel at CSS point (x, y), at scale 2.
fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = (((y * 2) * width + x * 2) * 4) as usize;
    rgba[i..i + 4].try_into().unwrap()
}

#[test]
fn a_page_shot_twice_gives_identical_pixels() {
    let dist = bundle("still");
    let a = shoot(&shot(dist.clone())).unwrap();
    let b = shoot(&shot(dist)).unwrap();
    assert_eq!((a.width, a.height), (640, 400));
    assert!(a.rgba == b.rgba, "two shots of the same page differ");
}

#[test]
fn the_shot_holds_the_page_stylesheet() {
    let a = shoot(&shot(bundle("still"))).unwrap();
    // The body's background, from still.css, and the card's blue inside its margin.
    assert_eq!(pixel(&a.rgba, a.width, 4, 4), [0xf4, 0xf1, 0xea, 0xff]);
    assert_eq!(pixel(&a.rgba, a.width, 30, 40), [0x1f, 0x6f, 0xeb, 0xff]);
}
