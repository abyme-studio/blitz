//! V8 through `deno_core`, behind react-blitz's [`ScriptEngine`]. The host calls are ops, taking a
//! commit's two arrays as `#[buffer] &[u8]`; a few web globals the page needs are written here in
//! `web.js`, over what `deno_core` gives (timers, `console`, `queueMicrotask`, `structuredClone`).

mod ops;

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context as TaskContext, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use anyhow::Context;
use deno_core::{
    JsRuntime, ModuleLoadResponse, ModuleLoader, ModuleSourceCode, ModuleSpecifier, ModuleType,
    PollEventLoopOptions, RequestedModuleType, ResolutionKind, RuntimeOptions,
};
use deno_error::JsErrorBox;
use react_blitz::{EventFlags, HostCalls, ModuleSource, ScriptEngine};

pub use ops::{EventIn, HostBox};

/// Whether the event loop asked to be polled again while it was polled.
struct Woke(AtomicBool);

impl Wake for Woke {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }
}

struct Loader(Rc<dyn ModuleSource>);

impl ModuleLoader for Loader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, deno_core::error::ModuleLoaderError> {
        let url = self
            .0
            .resolve(specifier, referrer)
            .map_err(|e| JsErrorBox::generic(e.to_string()))?;
        ModuleSpecifier::parse(&url).map_err(|e| JsErrorBox::generic(e.to_string()))
    }

    fn load(
        &self,
        specifier: &ModuleSpecifier,
        _referrer: Option<&deno_core::ModuleLoadReferrer>,
        options: deno_core::ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        let result = self
            .0
            .load(specifier.as_str())
            .map(|code| {
                let ty = if matches!(options.requested_module_type, RequestedModuleType::Json)
                    || specifier.path().ends_with(".json")
                {
                    ModuleType::Json
                } else {
                    ModuleType::JavaScript
                };
                deno_core::ModuleSource::new(
                    ty,
                    ModuleSourceCode::String(code.into()),
                    specifier,
                    None,
                )
            })
            .map_err(|e| JsErrorBox::generic(e.to_string()));
        ModuleLoadResponse::Sync(result)
    }
}

/// V8 as react-blitz's engine.
pub struct V8Engine {
    runtime: JsRuntime,
    tokio: tokio::runtime::Runtime,
    /// How long a turn waits for work about to fall due. A stepped page waits a moment, so a
    /// step sees it; a window turned every few ms waits not at all, or it never rests to draw.
    pub wait: Duration,
}

impl V8Engine {
    /// An isolate whose modules come from `modules`, with the web globals installed.
    pub fn new(modules: Rc<dyn ModuleSource>) -> anyhow::Result<Self> {
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        let _guard = tokio.enter();
        let mut runtime = JsRuntime::new(RuntimeOptions {
            module_loader: Some(Rc::new(Loader(modules))),
            extensions: vec![ops::extension()],
            ..Default::default()
        });
        runtime
            .execute_script("react-blitz:web.js", include_str!("web.js"))
            .map_err(|e| anyhow::anyhow!("web.js: {e}"))?;
        drop(_guard);
        Ok(Self { runtime, tokio, wait: Duration::from_millis(2) })
    }

    /// Run a script in the page's global scope, for a test.
    pub fn eval(&mut self, name: &'static str, source: String) -> anyhow::Result<()> {
        let _guard = self.tokio.enter();
        self.runtime
            .execute_script(name, source)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    /// V8's heap in bytes: used, total, physical, external, malloced.
    pub fn heap(&mut self) -> [usize; 5] {
        let stats = self.runtime.v8_isolate().get_heap_statistics();
        [
            stats.used_heap_size(),
            stats.total_heap_size(),
            stats.total_physical_size(),
            stats.external_memory(),
            stats.malloced_memory(),
        ]
    }

    /// Run what is ready, and what falls due within a moment, never past the deadline. Answers
    /// whether work was left when it stopped.
    fn run_loop(&mut self, deadline: Instant) -> bool {
        if self.wait.is_zero() {
            return self.run_ready(deadline);
        }
        let runtime = &mut self.runtime;
        let until = deadline.min(Instant::now() + self.wait);
        self.tokio.block_on(async {
            let run = runtime.run_event_loop(PollEventLoopOptions::default());
            match tokio::time::timeout_at(until.into(), run).await {
                Ok(Err(e)) => {
                    eprintln!("react-blitz: {e}");
                    false
                }
                Ok(Ok(())) => false,
                Err(_) => true,
            }
        })
    }

    /// Run what is ready now and wait for nothing. A wait of zero through tokio's timer still
    /// sleeps to its next tick, a millisecond: three events a frame made that the frame's input
    /// time. Here the loop is polled by hand, and again only while it woke itself for more.
    fn run_ready(&mut self, deadline: Instant) -> bool {
        let runtime = &mut self.runtime;
        self.tokio.block_on(async {
            loop {
                // Turn tokio's timer without sleeping, so a timer that is due fires.
                tokio::task::yield_now().await;
                let woke = Arc::new(Woke(AtomicBool::new(false)));
                let waker = Waker::from(Arc::clone(&woke));
                let polled = runtime.poll_event_loop(&mut TaskContext::from_waker(&waker), PollEventLoopOptions::default());
                match polled {
                    Poll::Ready(Ok(())) => return false,
                    Poll::Ready(Err(e)) => {
                        eprintln!("react-blitz: {e}");
                        return false;
                    }
                    Poll::Pending if woke.0.load(Ordering::Relaxed) && Instant::now() < deadline => {}
                    Poll::Pending => return true,
                }
            }
        })
    }

    fn call(&mut self, name: &'static str, source: &'static str) {
        let _guard = self.tokio.enter();
        if let Err(e) = self.runtime.execute_script(name, source) {
            eprintln!("react-blitz: {name}: {e}");
        }
    }
}

impl ScriptEngine for V8Engine {
    fn load_module(&mut self, url: &str, source: &str) -> anyhow::Result<()> {
        let spec = ModuleSpecifier::parse(url).with_context(|| format!("module url {url}"))?;
        let runtime = &mut self.runtime;
        self.tokio.block_on(async {
            let id = runtime
                .load_main_es_module_from_code(&spec, source.to_string())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let eval = runtime.mod_evaluate(id);
            runtime
                .with_event_loop_promise(Box::pin(eval), PollEventLoopOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))
        })
    }

    fn register(&mut self, host: Box<dyn HostCalls>) {
        self.runtime.op_state().borrow_mut().put(HostBox(host));
    }

    fn dispatch(&mut self, kind: u32, chain: &[u32], fields: &[u8]) -> EventFlags {
        self.runtime.op_state().borrow_mut().put(EventIn {
            kind,
            chain: chain.to_vec(),
            fields: String::from_utf8_lossy(fields).into_owned(),
            flags: 0,
        });
        self.call("react-blitz:dispatch", "globalThis.__reactBlitzEngine.dispatch()");
        let flags = self
            .runtime
            .op_state()
            .borrow_mut()
            .try_take::<EventIn>()
            .map_or(0, |e| e.flags);
        EventFlags {
            prevent_default: flags & 1 != 0,
            stop_propagation: flags & 2 != 0,
        }
    }

    fn frame(&mut self, batch: &[u8]) {
        self.runtime
            .op_state()
            .borrow_mut()
            .put(ops::FrameIn(String::from_utf8_lossy(batch).into_owned()));
        self.call("react-blitz:frame", "globalThis.__reactBlitzEngine.frame()");
    }

    fn pump(&mut self, deadline: Instant) -> bool {
        let mut ran = self.run_loop(deadline);
        // A stepped clock's timers, one at a time, each followed by its microtasks.
        while Instant::now() < deadline {
            self.call("react-blitz:timers", "globalThis.__reactBlitzTimers()");
            if self.runtime.op_state().borrow_mut().try_take::<ops::TimerRan>().is_none() {
                break;
            }
            ran = true;
            self.run_loop(deadline);
        }
        ran
    }

    fn next_due(&self) -> Option<Instant> {
        let state = self.runtime.op_state();
        let state = state.borrow();
        state.try_borrow::<ops::TimersDue>()?.0.values().min().copied()
    }
}
