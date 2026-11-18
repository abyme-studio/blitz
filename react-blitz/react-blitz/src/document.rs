use std::path::PathBuf;
use std::rc::Rc;
use std::task::Context as TaskContext;
use std::time::{Duration, Instant};

use anyhow::Context;
use blitz_dom::{BaseDocument, DocGuard, DocGuardMut, Document, DocumentConfig, EventDriver};
use blitz_html::DocumentHtmlParser;
use blitz_traits::events::UiEvent;

use crate::engine::ScriptEngine;
use crate::events::ReactEventHandler;
use crate::frame;
use crate::host::{Host, Shared};
use crate::loader::{DistLoader, ModuleSource};
use crate::net::FileNetProvider;

/// A built page to open: its folder, the HTML file in it, and the query its address carries.
pub struct ReactDocumentConfig {
    pub dist: PathBuf,
    /// The page's HTML, from the folder's root: `index.html`.
    pub page: String,
    /// The address's query, without its `?`.
    pub query: String,
    /// The document's own settings; its `base_url` is set to the page's.
    pub document: DocumentConfig,
    /// What the page's clock follows.
    pub clock: Clock,
}

/// What the page's clock follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clock {
    /// Wall time, from when the document opened.
    #[default]
    Wall,
    /// Set by the embedder ([`ReactDocument::set_time`]), starting at 0: CSS animations,
    /// `requestAnimationFrame`, `performance.now()` and timers move only when it is set.
    Stepped,
}

/// Blitz's [`Document`] over a `BaseDocument` and a [`ScriptEngine`] running React. The stock
/// `blitz-shell` runs it unchanged.
pub struct ReactDocument<E: ScriptEngine> {
    shared: Rc<Shared>,
    engine: E,
}

impl<E: ScriptEngine> ReactDocument<E> {
    /// Parse the page's HTML (its stylesheets are read from the build's folder unless the config
    /// names a net provider), give the engine the host calls, and run
    /// the page's module script.
    pub fn open(config: ReactDocumentConfig, mut engine: E) -> anyhow::Result<Self> {
        let loader = DistLoader::new(&config.dist)?;
        let page_url = loader.url(&config.page);
        let html = std::fs::read_to_string(loader.root().join(&config.page))
            .with_context(|| format!("no {} in {}", config.page, config.dist.display()))?;

        let mut document = config.document;
        document.base_url = Some(page_url.clone());
        if document.net_provider.is_none() {
            document.net_provider = Some(FileNetProvider::shared());
        }
        let mut doc = BaseDocument::new(document);
        {
            let mut m = doc.mutate();
            DocumentHtmlParser::parse_into_mutator(&mut m, &html);
        }
        let entry = module_script(&doc).context("the page has no <script type=\"module\" src>")?;

        let address = if config.query.is_empty() {
            page_url.clone()
        } else {
            format!("{page_url}?{}", config.query)
        };
        let shared = Shared::new(doc, address);
        if config.clock == Clock::Stepped {
            shared.state.borrow_mut().clock = Some(0.0);
        }
        engine.register(Box::new(Host(Rc::clone(&shared))));

        let entry_url = loader.resolve(&entry, &page_url)?;
        let source = loader.load(&entry_url)?;
        engine.load_module(&entry_url, &source)?;

        let mut this = Self { shared, engine };
        this.run(Duration::from_millis(50));
        Ok(this)
    }

    /// What the engine and the host share: the document and JS's ids for it.
    pub fn shared(&self) -> &Rc<Shared> {
        &self.shared
    }

    pub fn engine_mut(&mut self) -> &mut E {
        &mut self.engine
    }

    /// Run JS's due work, then lay out and send the frame batch: one turn of the page.
    pub fn run(&mut self, budget: Duration) -> bool {
        let mut ran = self.engine.pump(Instant::now() + budget);
        self.shared.resolve();
        if let Some(batch) = frame::batch(&self.shared) {
            self.engine.frame(&batch);
            self.engine.pump(Instant::now() + budget);
            ran = true;
        }
        let wants_frame = self.shared.state.borrow().frame_wanted;
        if wants_frame {
            self.shared.doc.borrow().shell_provider.request_redraw();
        }
        ran
    }

    /// Seconds on the page's clock.
    pub fn time(&self) -> f64 {
        self.shared.now()
    }

    /// Set a stepped clock to `seconds`. A wall clock is left alone.
    pub fn set_time(&mut self, seconds: f64) {
        let mut state = self.shared.state.borrow_mut();
        if state.clock.is_some() {
            state.clock = Some(seconds);
        }
    }

    /// Whether JS asked for another frame.
    pub fn wants_frame(&self) -> bool {
        self.shared.state.borrow().frame_wanted
    }
}

/// The `src` of the page's first module script.
fn module_script(doc: &BaseDocument) -> Option<String> {
    let scripts = doc.query_selector_all("script[type=module][src]").ok()?;
    let node = doc.get_node(*scripts.first()?)?;
    node.attr(blitz_dom::local_name!("src")).map(str::to_string)
}

impl<E: ScriptEngine + 'static> Document for ReactDocument<E> {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::RefCell(self.shared.doc.borrow())
    }

    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::RefCell(self.shared.doc.borrow_mut())
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        let mut doc = Rc::clone(&self.shared.doc);
        let handler = ReactEventHandler {
            engine: &mut self.engine,
            shared: &self.shared,
        };
        let mut driver = EventDriver::new(&mut doc, handler);
        driver.handle_ui_event(event);
        drop(driver);
        // A discrete event's updates run in its microtasks.
        self.engine.pump(Instant::now() + Duration::from_millis(16));
        self.shared.doc.borrow().shell_provider.request_redraw();
    }

    fn poll(&mut self, _cx: Option<TaskContext>) -> bool {
        self.run(Duration::from_millis(8))
    }
}
