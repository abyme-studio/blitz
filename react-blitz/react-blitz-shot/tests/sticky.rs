//! A sticky box stays at its inset while its scroller scrolls.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot};

#[test]
fn a_sticky_card_stays_at_the_top_of_a_scrolled_scroller() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/sticky");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/sticky"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling sticky failed");
    let image = shoot(&Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 100,
        height: 120,
        scale: 1.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    })
    .unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * image.width + x) * 4) as usize;
        <[u8; 4]>::try_from(&image.rgba[i..i + 4]).unwrap()
    };
    // Scrolled 300px, the card is still at the scroller's top right, over its blue.
    assert_eq!(at(90, 10), [200, 0, 0, 255], "the card scrolled away with the rows");
    assert_eq!(at(10, 10), [0, 0, 200, 255]);
}
