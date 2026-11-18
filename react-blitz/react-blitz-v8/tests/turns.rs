//! A window turns its page every few ms and after every event: a turn with nothing due waits for
//! nothing, and a timer that is due still fires in it.

mod common;

use std::time::{Duration, Instant};

use blitz_dom::Document;

use react_blitz::ReactDocument;
use react_blitz_v8::V8Engine;

fn timer(doc: &ReactDocument<V8Engine>) -> String {
    let base = doc.inner();
    let node = base.query_selector("#timer").unwrap().unwrap();
    base.get_node(node).unwrap().text_content()
}

#[test]
fn a_turn_with_no_wait_takes_no_time_and_still_fires_a_due_timer() {
    let mut page = common::open("turns").into_inner();
    assert_eq!(timer(&page), "waiting");
    page.engine_mut().wait = Duration::ZERO;
    let start = Instant::now();
    let mut turns = 0;
    while start.elapsed() < Duration::from_millis(20) {
        page.poll(None);
        turns += 1;
    }
    // Through tokio's timer each turn slept to the next ms: 20 at most in 20 ms.
    assert!(turns > 100, "{turns} turns in 20 ms: a turn with nothing due waited");
    assert_eq!(timer(&page), "waiting");
    std::thread::sleep(Duration::from_millis(40));
    for _ in 0..3 {
        page.poll(None);
        std::thread::sleep(Duration::from_millis(17));
    }
    assert_eq!(timer(&page), "fired");
}
