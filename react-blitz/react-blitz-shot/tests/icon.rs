//! An inline SVG icon drawn by React paints, stroked in its `currentColor`.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot};

/// The 32 by 32 shot of the page in `tests/pages/<name>`.
fn shot(name: &str) -> react_blitz_shot::Image {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot").join(name);
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages").join(name))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling {name} failed");
    shoot(&Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 32,
        height: 32,
        scale: 1.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    })
    .unwrap()
}

#[test]
fn an_icon_paints_in_its_current_color() {
    let image = shot("icon");
    // The plus crosses at the icon's middle, (8, 8): the body's red there, white off the strokes.
    let at = |x: u32, y: u32| {
        let i = ((y * image.width + x) * 4) as usize;
        <[u8; 4]>::try_from(&image.rgba[i..i + 4]).unwrap()
    };
    assert_eq!(at(8, 8), [200, 0, 0, 255], "the icon's middle is not its red");
    assert_eq!(at(1, 1), [255, 255, 255, 255]);
}

/// A `color` mixed in oklab is handed to the SVG as sRGB, which it can parse: `oklab(…)` it cannot,
/// and the icon drew nothing.
#[test]
fn an_icon_paints_in_a_color_mixed_in_oklab() {
    let image = shot("icon-mix");
    let i = ((8 * image.width + 8) * 4) as usize;
    let [r, g, b, a] = <[u8; 4]>::try_from(&image.rgba[i..i + 4]).unwrap();
    assert!(r > 190 && g < 10 && b < 10 && a == 255, "the icon's middle is ({r}, {g}, {b}, {a}), not its red");
}
