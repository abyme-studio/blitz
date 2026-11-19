//! An empty field shows its `placeholder` attribute in its `::placeholder` style and matches
//! `:placeholder-shown`; typing hides both, and emptying it again shows them. A field with a value
//! shows neither. Input enters as the window's does (`control`).

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::Shot;
use react_blitz_shot::control::Control;

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const GREEN: [u8; 3] = [0, 128, 0];

fn page() -> Control {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/placeholder");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/placeholder"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling placeholder failed");
    let shot = Shot {
        dist,
        page: "index.html".into(),
        query: String::new(),
        width: 300,
        height: 200,
        scale: 1.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    };
    Control::open(&shot).unwrap()
}

fn send(page: &mut Control, command: &str) {
    let answer = page.command(command);
    assert!(answer.get("error").is_none(), "{command}: {answer}");
}

fn frames(page: &mut Control, n: usize) {
    for _ in 0..n {
        send(page, r#"{"cmd":"frame"}"#);
    }
}

fn key(page: &mut Control, key: &str, text: Option<&str>) {
    let text = text.map(|t| format!(r#","text":"{t}""#)).unwrap_or_default();
    send(page, &format!(r#"{{"cmd":"keydown","key":"{key}","code":"{key}"{text}}}"#));
    send(page, &format!(r#"{{"cmd":"keyup","key":"{key}","code":"{key}"}}"#));
    frames(page, 3);
}

fn pixel(page: &Control, x: u32, y: u32) -> [u8; 3] {
    let image = page.image();
    let i = ((y * image.width + x) * 4) as usize;
    [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
}

/// Pixels of exactly `colour` in the rect from (x0, y0) to (x1, y1).
fn count(page: &Control, colour: [u8; 3], (x0, y0, x1, y1): (u32, u32, u32, u32)) -> usize {
    (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| pixel(page, x, y) == colour).count()
}

/// The textarea's content box, and the input's: each 260 by 60 inside a 4 px border and 4 px padding.
const EMPTY: (u32, u32, u32, u32) = (18, 18, 278, 78);
const FILLED: (u32, u32, u32, u32) = (18, 104, 278, 164);

#[test]
fn an_empty_field_shows_its_placeholder_until_typed_into() {
    let mut page = page();
    frames(&mut page, 2);
    assert!(count(&page, RED, EMPTY) > 50, "empty: the placeholder is drawn in its ::placeholder colour");
    assert_eq!(pixel(&page, 11, 40), BLUE, "empty: it matches :placeholder-shown");
    assert_eq!(count(&page, RED, FILLED), 0, "with a value, the input draws no placeholder");
    assert_eq!(pixel(&page, 11, 130), GREEN, "with a value, it does not match :placeholder-shown");

    send(&mut page, r#"{"cmd":"move","x":100,"y":40}"#);
    send(&mut page, r#"{"cmd":"down","x":100,"y":40}"#);
    send(&mut page, r#"{"cmd":"up","x":100,"y":40}"#);
    frames(&mut page, 2);
    key(&mut page, "a", Some("a"));
    assert_eq!(count(&page, RED, EMPTY), 0, "typed into: no placeholder");
    assert_eq!(pixel(&page, 11, 40), GREEN, "typed into: no longer :placeholder-shown");

    // Backspace is an Apple key binding on macOS, which `control` does not send: delete forwards.
    key(&mut page, "ArrowLeft", None);
    key(&mut page, "Delete", None);
    assert!(count(&page, RED, EMPTY) > 50, "emptied: the placeholder is back");
    assert_eq!(pixel(&page, 11, 40), BLUE, "emptied: :placeholder-shown again");
}
