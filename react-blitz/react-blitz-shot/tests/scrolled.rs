//! A scroll offset set by page code (`scrollTop = n`) is heard as a `scroll` event, as in a browser.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot};

#[test]
fn a_scroll_the_page_sets_is_heard_as_a_scroll_event() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/scrolled");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/scrolled"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling scrolled failed");
    // The page is ready only once its listener heard the scroll: unheard, the shot times out.
    let image = shoot(&Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 320,
        height: 200,
        scale: 1.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(10),
    })
    .expect("the page heard its own scroll");
    let mark = image.boxes.iter().find(|b| b.path == "1").expect("the mark's box");
    assert_eq!(mark.width, 60.0, "the listener read the offset the page set");
}
