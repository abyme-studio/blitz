//! A box sized in container units, and a `@container` rule, answer to the container's size,
//! not the viewport's.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot};

#[test]
fn container_units_and_rules_follow_the_container() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/container");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/container"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling container failed");
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
    assert_eq!(size("0"), [200.0, 80.0], "the container");
    assert_eq!(size("0.0"), [100.0, 80.0], "50cqw by 100cqh of a 200 by 80 container");
    assert_eq!(size("0.1"), [40.0, 10.0], "the rule for a container 150px wide or more");
}
