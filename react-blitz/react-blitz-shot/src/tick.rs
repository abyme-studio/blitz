//! When a window turns its page with no event to bring the turn: an alarm set for the page's
//! next timer or frame, and silent while neither is due. A thread that woke the loop every few
//! ms, timer or none, was a twelfth of a scroll frame.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::task::Waker;
use std::time::{Duration, Instant};

use react_blitz::{ReactDocument, ScriptEngine};

/// The least time between two turns that nothing due asks for: the loop polls after every event.
const TICK: Duration = Duration::from_millis(4);
/// How soon to look again when a turn was due and the engine's own timer had not yet fired: its
/// wheel counts in whole ms.
const RETRY: Duration = Duration::from_millis(1);
/// The longest wait the alarm's thread takes whole.
const NEAR: Duration = Duration::from_millis(2);

#[derive(Default)]
struct Alarm {
    at: Mutex<Option<Instant>>,
    moved: Condvar,
    rings: AtomicU64,
}

/// The alarm for a page's turns, and when it was last turned.
pub struct Tick {
    alarm: Arc<Alarm>,
    thread: Once,
    turned: Instant,
}

impl Default for Tick {
    fn default() -> Self {
        Self { alarm: Arc::default(), thread: Once::new(), turned: Instant::now() }
    }
}

impl Tick {
    /// Whether to turn the page now: something of its own is due, or it was last turned a tick
    /// ago. `waker` is what the alarm wakes; the first one given is kept.
    pub fn due<E: ScriptEngine>(&mut self, doc: &ReactDocument<E>, waker: Option<&Waker>) -> bool {
        if let Some(waker) = waker {
            self.thread.call_once(|| {
                let (alarm, waker) = (Arc::clone(&self.alarm), waker.clone());
                std::thread::spawn(move || ring(&alarm, &waker));
            });
        }
        let now = Instant::now();
        let due = doc.next_turn().is_some_and(|at| at <= now) || now.duration_since(self.turned) >= TICK;
        if due {
            self.turned = now;
        }
        due
    }

    /// Set the alarm for the page's next turn, after a turn or in place of one; with nothing due
    /// it is cleared, and nothing wakes the loop but an event.
    pub fn rest<E: ScriptEngine>(&self, doc: &ReactDocument<E>) {
        let at = doc.next_turn().map(|at| at.max(Instant::now() + RETRY));
        let mut set = self.alarm.at.lock().unwrap();
        if *set != at {
            *set = at;
            self.alarm.moved.notify_one();
        }
    }

    /// How many times the alarm has rung.
    pub fn rings(&self) -> u64 {
        self.alarm.rings.load(Ordering::Relaxed)
    }
}

/// Wake `waker` each time the alarm's time comes; the alarm is then clear until it is set again.
fn ring(alarm: &Alarm, waker: &Waker) {
    let mut at = alarm.at.lock().unwrap();
    loop {
        at = match *at {
            None => alarm.moved.wait(at).unwrap(),
            Some(when) => {
                let now = Instant::now();
                if when > now {
                    // A long wait comes back some ms late (macOS, 2 to 5 ms on 120) and a short
                    // one within 0.3 ms: the time is neared by halves.
                    let left = when - now;
                    let wait = if left > NEAR { left / 2 } else { left };
                    alarm.moved.wait_timeout(at, wait).unwrap().0
                } else {
                    *at = None;
                    alarm.rings.fetch_add(1, Ordering::Relaxed);
                    waker.wake_by_ref();
                    at
                }
            }
        };
    }
}
