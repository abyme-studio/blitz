//! Events cross one at a time; observations arrive once a frame; layout reads are synchronous.

mod common;

use keyboard_types::{Key, NamedKey};

fn settle(page: &mut common::Page) {
    for _ in 0..4 {
        page.tick(1.0 / 60.0);
    }
}

#[test]
fn a_click_reaches_on_click() {
    let mut page = common::open("events");
    assert_eq!(page.text_content("#clicks"), "clicks 0");
    page.click("#count");
    settle(&mut page);
    assert_eq!(page.text_content("#clicks"), "clicks 1");
    page.click("#count");
    settle(&mut page);
    assert_eq!(page.text_content("#clicks"), "clicks 2");
}

#[test]
fn prevent_default_on_keydown_stops_the_default() {
    let mut page = common::open("events");
    let trap = page.query("#trap").unwrap();
    let free = page.query("#free").unwrap();
    page.base_mut().set_focus_to(trap);
    page.press(Key::Named(NamedKey::Tab));
    settle(&mut page);
    assert_eq!(page.focused(), Some(trap), "Tab moved focus though keydown prevented it");
    page.base_mut().set_focus_to(free);
    page.press(Key::Named(NamedKey::Tab));
    settle(&mut page);
    assert_ne!(page.focused(), Some(free), "Tab did not move focus with nothing preventing it");
}

#[test]
fn a_resize_observer_hears_a_size_change_in_the_next_frame() {
    let mut page = common::open("events");
    settle(&mut page);
    assert_eq!(page.text_content("#seen"), "seen 100");
    page.click("#widen");
    settle(&mut page);
    assert_eq!(page.text_content("#seen"), "seen 300");
}

#[test]
fn a_style_write_then_a_rect_read_gives_the_new_size() {
    let mut page = common::open("events");
    page.click("#write-read");
    settle(&mut page);
    assert_eq!(page.text_content("#read"), "read 250");
}
