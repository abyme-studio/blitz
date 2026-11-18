//! With `keep_fragments(true)`, a scroll container's content is kept between frames as a fragment
//! of Vello's scene: a scroll builds nothing in it and draws the same pixels as a frame built
//! whole, and a hover builds only the container it is in. On Vello's GPU renderer, with no window.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyrender_vello::VelloImageRenderer;
use blitz_paint::FragmentStats;
use react_blitz_shot::{Offscreen, Shot};

fn page() -> Offscreen<VelloImageRenderer> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/kept");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/kept"))
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
    Offscreen::open(&shot).unwrap()
}

/// Three frames, each after a wheel of 40 px over the first scroller: each frame's fragments,
/// and the last frame's pixels.
fn scrolled(keep: bool) -> (Vec<FragmentStats>, Vec<u8>) {
    blitz_paint::keep_fragments(keep);
    let mut page = page();
    page.pointer(75.0, 50.0);
    page.frame();
    let stats = (0..3)
        .map(|_| {
            page.wheel(75.0, 50.0, 40.0);
            page.frame().fragments
        })
        .collect();
    blitz_paint::keep_fragments(false);
    (stats, page.image().rgba)
}

#[test]
fn a_scroll_builds_no_fragment_and_draws_what_a_whole_frame_draws() {
    let (kept, pixels) = scrolled(true);
    // The other scroller's is kept too: the scroll is not its change.
    assert_eq!(kept, vec![FragmentStats { built: 0, kept: 2 }; 3]);
    let (whole, expected) = scrolled(false);
    assert_eq!(whole, vec![FragmentStats::default(); 3]);
    assert!(pixels == expected, "the kept frame's pixels differ from a frame built whole");
    assert!(pixels.chunks(4).any(|p| p[..3] == [0x1f, 0x6f, 0xeb]), "the rows are drawn");
}

#[test]
fn a_hover_builds_only_the_fragment_it_is_in() {
    blitz_paint::keep_fragments(true);
    let mut page = page();
    page.pointer(75.0, 150.0);
    assert_eq!(page.frame().fragments, FragmentStats { built: 2, kept: 0 }, "the first frame builds both");
    page.wheel(75.0, 50.0, 40.0);
    assert_eq!(page.frame().fragments, FragmentStats { built: 0, kept: 2 });
    // Onto a row of the second scroller: its colour changes, and nothing in the first.
    page.pointer(225.0, 50.0);
    assert_eq!(page.frame().fragments, FragmentStats { built: 1, kept: 1 });
    assert_eq!(page.frame().fragments, FragmentStats { built: 0, kept: 2 }, "and it is kept again");
}
