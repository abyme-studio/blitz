//! The quads back end (`--renderer quads`) draws a page's first frame offscreen: its boxes in
//! their colours, and the glyphs it cannot draw yet counted as left out.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Raster, Shot, scroll_frames};

#[test]
fn quads_draw_the_still_page_s_boxes_and_count_its_glyphs_left_out() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/quads-still");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/still"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling still failed");
    let shot = Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 320,
        height: 200,
        scale: 1.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    };
    let (frames, image) = scroll_frames(&shot, 1, 0.0, Raster::Quads).unwrap();
    let frame = frames[0];
    assert!(frame.instances >= 4, "body, card and two chips: {} instances", frame.instances);
    assert!(frame.draws >= 1, "at least one instanced draw");
    assert!(frame.left_out >= 1, "the card's text is left out until the glyph atlas: {}", frame.left_out);

    let at = |x: u32, y: u32| {
        let i = ((y * image.width + x) * 4) as usize;
        [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2], image.rgba[i + 3]]
    };
    assert_eq!(at(4, 4), [0xf4, 0xf1, 0xea, 0xff], "the body's background");
    assert_eq!(at(290, 36), [0x1f, 0x6f, 0xeb, 0xff], "the card, right of its text");
    assert_eq!(at(40, 66), [0xff, 0xd3, 0x3d, 0xff], "the narrow chip");
    assert_eq!(at(250, 66), [0x2d, 0xa4, 0x4e, 0xff], "the wide chip");
}
