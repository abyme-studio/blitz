//! The shot says each element's box under the preview frame and each portal after it, in CSS pixels.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot};

#[test]
fn the_shot_says_the_frame_s_boxes_and_its_portals() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/boxes");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/boxes"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling boxes failed");
    // At scale 2: the boxes are CSS pixels still.
    let image = shoot(&Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 320,
        height: 200,
        scale: 2.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    })
    .unwrap();
    let got: Vec<_> = image.boxes.iter().map(|b| (b.path.as_str(), b.tag.as_str(), [b.x, b.y, b.width, b.height])).collect();
    let want = [
        ("", "div", [0.0, 0.0, 200.0, 100.0]),
        ("0", "div", [10.0, 10.0, 50.0, 20.0]),
        ("1", "div", [10.0, 40.0, 120.0, 30.0]),
        ("1.0", "div", [10.0, 40.0, 55.0, 30.0]),
        ("1.1", "section", [75.0, 40.0, 55.0, 30.0]),
        ("portal0", "div", [150.0, 150.0, 40.0, 20.0]),
    ];
    assert_eq!(got.len(), want.len(), "{got:?}");
    for ((path, tag, at), (wpath, wtag, wat)) in got.iter().zip(want) {
        assert_eq!((*path, *tag), (wpath, wtag), "{got:?}");
        assert!(at.iter().zip(wat).all(|(a, b)| (a - b).abs() < 0.01), "{path} is at {at:?}, not {wat:?}");
    }
}
