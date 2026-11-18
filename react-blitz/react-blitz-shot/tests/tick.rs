//! A window's page is turned when a timer or a frame of its own is due, and not otherwise: the
//! alarm (`react_blitz_shot::tick`) under a loop that, like the window's, polls only when woken.

use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Wake, Waker};
use std::thread::Thread;
use std::time::{Duration, Instant};

use blitz_dom::{Document, DocumentConfig};
use blitz_traits::shell::{ColorScheme, Viewport};
use react_blitz::{Clock, DistLoader, ReactDocument, ReactDocumentConfig};
use react_blitz_shot::tick::Tick;
use react_blitz_v8::V8Engine;

/// The loop's waker: what the alarm wakes.
struct Woken(AtomicBool, Thread);

impl Wake for Woken {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Release);
        self.1.unpark();
    }
}

/// A page on wall time, its alarm, and the loop's waker.
struct Page {
    doc: ReactDocument<V8Engine>,
    tick: Tick,
    woken: Arc<Woken>,
}

impl Page {
    fn open(name: &str) -> Self {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dist = manifest.join("../../target/react-blitz-pages/shot").join(name);
        let status = Command::new("node")
            .arg(manifest.join("../js/scripts/bundle.mjs"))
            .arg(manifest.join("tests/pages").join(name))
            .arg(&dist)
            .status()
            .expect("node runs the bundler");
        assert!(status.success(), "bundling {name} failed");
        let mut engine = V8Engine::new(Rc::new(DistLoader::new(&dist).unwrap())).unwrap();
        engine.wait = Duration::ZERO;
        let doc = ReactDocument::open(
            ReactDocumentConfig {
                dist,
                page: "index.html".into(),
                query: String::new(),
                document: DocumentConfig { viewport: Some(Viewport::new(320, 200, 1.0, ColorScheme::Light)), ..Default::default() },
                clock: Clock::Wall,
            },
            engine,
        )
        .unwrap();
        // The window's first poll comes from its first event.
        let woken = Arc::new(Woken(AtomicBool::new(true), std::thread::current()));
        Self { doc, tick: Tick::default(), woken }
    }

    /// Run the loop for `time`: a poll each time it is woken, as the window's. Answers how many
    /// times the alarm woke it.
    fn run(&mut self, time: Duration) -> u64 {
        let (end, before) = (Instant::now() + time, self.tick.rings());
        let waker = Waker::from(Arc::clone(&self.woken));
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            if !self.woken.0.swap(false, Ordering::AcqRel) {
                std::thread::park_timeout(left);
                continue;
            }
            if self.tick.due(&self.doc, Some(&waker)) {
                self.doc.poll(None);
            }
            self.tick.rest(&self.doc);
        }
        self.tick.rings() - before
    }

    fn late(&self) -> String {
        let base = self.doc.inner();
        let node = base.query_selector("#late").unwrap().unwrap();
        base.get_node(node).unwrap().text_content()
    }
}

#[test]
fn no_tick_runs_while_no_timer_is_due() {
    let mut page = Page::open("timer");
    page.run(Duration::from_millis(300));
    assert_eq!(page.run(Duration::from_millis(300)), 0, "the alarm rang on a page with nothing due");
}

#[test]
fn a_timer_fires_within_4_ms_of_its_time_on_few_ticks() {
    let mut page = Page::open("timer");
    page.run(Duration::from_millis(300));
    let mut lates = Vec::new();
    for _ in 0..5 {
        page.doc.engine_mut().eval("arm", "globalThis.arm(120)".into()).unwrap();
        // An event's poll follows the script that set the timer.
        page.woken.0.store(true, Ordering::Release);
        let rings = page.run(Duration::from_millis(200));
        // One for the timer, one for the frame after its commit, and a retry or two.
        assert!((1..=6).contains(&rings), "{rings} rings for one timer");
        lates.push(page.late().parse::<f64>().expect("the timer fired"));
    }
    lates.sort_by(f64::total_cmp);
    // The middle of five: one late start under a busy machine is not the alarm's.
    assert!((0.0..4.0).contains(&lates[2]), "the timers fired {lates:?} ms after their time");
}
