//! Across scroll frames glyphs come from a cache: a second frame at a new offset encodes no glyph
//! outline (Vello's resolver keeps them) and builds no Parley layout (each node keeps its own),
//! and draws the glyphs where the CPU renderer draws them. On Vello's GPU renderer, with no window.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyrender::ImageRenderer;
use anyrender_vello::VelloImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use react_blitz_shot::{Offscreen, ScrollFrame, Shot};

/// The page's first frame, then a frame after a wheel of 40 px, and that frame's pixels.
fn scrolled<R: ImageRenderer>() -> (ScrollFrame, ScrollFrame, Vec<u8>) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/glyphs");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/glyphs"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling the page failed");
    let shot = Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 320,
        height: 200,
        scale: 2.0,
        scheme: blitz_traits::shell::ColorScheme::Light,
        timeout: Duration::from_secs(20),
    };
    let mut page = Offscreen::<R>::open(&shot).unwrap();
    page.pointer(160.0, 100.0);
    let first = page.frame();
    page.wheel(160.0, 100.0, 40.0);
    let second = page.frame();
    (first, second, page.image().rgba)
}

#[test]
fn a_second_frame_at_a_new_offset_encodes_no_glyph_and_builds_no_layout() {
    let (first, second, gpu) = scrolled::<VelloImageRenderer>();
    assert!(first.glyph_outlines > 0, "the first frame encodes the glyphs it draws");
    assert_eq!(second.glyph_outlines, 0, "the scrolled frame's glyphs come from Vello's cache");
    assert_eq!(second.text_layouts, 0, "and its text from each node's kept layout");

    let (_, _, cpu) = scrolled::<VelloCpuImageRenderer>();
    assert_eq!(gpu.len(), cpu.len());
    // The two antialias a glyph's edge differently, the GPU's the heavier: an edge pixel may be
    // a shade apart, and a glyph missing or out of place would be the whole range apart.
    let apart = |a: &[u8], b: &[u8]| a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
    let worst = gpu.chunks(4).zip(cpu.chunks(4)).map(|(a, b)| apart(a, b)).max().unwrap_or(0);
    assert!(worst <= 128, "a pixel is {worst} of 255 from the CPU renderer's");
    assert!(gpu.chunks(4).any(|p| p[0] < 0x60), "the text is drawn");
}
