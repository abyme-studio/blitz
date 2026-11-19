//! A popover on real input, as UI SDK's menu opens one: a click, or Enter on the focused button,
//! calls `showPopover`; the open popover matches `:popover-open` and is drawn and hit in the top
//! layer, above a fixed cover with a z-index and outside its parent's clip; a click outside closes
//! it, through the document's listeners. Input enters as the window's does (`control`).

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use blitz_traits::shell::ColorScheme;
use react_blitz_shot::Shot;
use react_blitz_shot::control::Control;

const GREEN: [u8; 3] = [0, 128, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const WHITE: [u8; 3] = [255, 255, 255];

fn page() -> Control {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../target/react-blitz-pages/shot/popover");
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages/popover"))
        .arg(&dist)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling popover failed");
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

fn click(page: &mut Control, x: u32, y: u32) {
    send(page, &format!(r#"{{"cmd":"move","x":{x},"y":{y}}}"#));
    send(page, &format!(r#"{{"cmd":"down","x":{x},"y":{y}}}"#));
    frames(page, 1);
    send(page, &format!(r#"{{"cmd":"up","x":{x},"y":{y}}}"#));
    frames(page, 3);
}

fn key(page: &mut Control, key: &str) {
    send(page, &format!(r#"{{"cmd":"keydown","key":"{key}","code":"{key}"}}"#));
    send(page, &format!(r#"{{"cmd":"keyup","key":"{key}","code":"{key}"}}"#));
    frames(page, 3);
}

/// The popover's middle, in the last frame: (100, 110), under the cover.
fn popover_pixel(page: &Control) -> [u8; 3] {
    let image = page.image();
    let i = ((110 * image.width + 100) * 4) as usize;
    [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
}

#[test]
fn a_click_opens_it_above_everything_and_a_click_on_it_reaches_it() {
    let mut page = page();
    assert_eq!(popover_pixel(&page), WHITE, "closed, the cover shows");
    click(&mut page, 40, 35);
    assert_eq!(popover_pixel(&page), GREEN, "open: :popover-open, drawn over the cover and out of its clip");
    click(&mut page, 100, 110);
    assert_eq!(popover_pixel(&page), BLUE, "the click went to the popover, not the cover above it in the tree");
}

#[test]
fn a_click_outside_closes_it() {
    let mut page = page();
    click(&mut page, 40, 35);
    assert_eq!(popover_pixel(&page), GREEN);
    click(&mut page, 280, 20);
    assert_eq!(popover_pixel(&page), WHITE, "light dismiss: the click outside reached the document");
}

#[test]
fn enter_on_the_focused_button_opens_it() {
    let mut page = page();
    key(&mut page, "Tab");
    key(&mut page, "Enter");
    assert_eq!(popover_pixel(&page), GREEN, "Enter activated the button, as a click");
    key(&mut page, "Escape");
    assert_eq!(popover_pixel(&page), WHITE, "Escape closed it");
}
