//! `react-blitz-window <dist> <query> [--control <socket>] [--size WxH] [--scale N]
//! [--scheme light|dark] [--page index.html] [--timeout SECONDS]`
//!
//! Without `--control`: `blitz-shell` with a `ReactDocument` over the build, on wall time, painted
//! by Vello's CPU renderer (the shot's) into a window; each frame's time and each turn of JS are
//! logged on stderr. With `--control`: the page on a stepped 60 Hz clock, steered by JSON lines on the
//! Unix socket (`react_blitz_shot::control` lists them), painted with no window, as the shot is.

use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::{Arc, Once};
use std::task::Context as TaskContext;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use anyrender::{RenderContext, WindowHandle, WindowRenderer};
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_dom::{DocGuard, DocGuardMut, Document, DocumentConfig};
use blitz_shell::{BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop};
use blitz_traits::events::UiEvent;
use blitz_traits::shell::{ColorScheme, Viewport};
use react_blitz::{Clock, DistLoader, ReactDocument, ReactDocumentConfig};
use react_blitz_shot::Shot;
use react_blitz_shot::control::{Control, serve};
use react_blitz_v8::V8Engine;

const USAGE: &str = "usage: react-blitz-window <dist> <query> [--control <socket>] [--size WxH] \
[--scale N] [--scheme light|dark] [--page index.html] [--timeout SECONDS]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("react-blitz-window: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut plain = Vec::new();
    let (mut width, mut height, mut scale) = (1280, 800, 1.0);
    let mut scheme = ColorScheme::Light;
    let mut page = "index.html".to_string();
    let mut timeout = 30.0;
    let mut control = None;
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().with_context(|| format!("{flag} needs a value\n{USAGE}"));
        match arg.as_str() {
            "--control" => control = Some(PathBuf::from(value("--control")?)),
            "--size" => {
                let v = value("--size")?;
                let (w, h) = v.split_once('x').with_context(|| format!("--size is WxH, not {v}"))?;
                width = w.parse().with_context(|| format!("--size width {w}"))?;
                height = h.parse().with_context(|| format!("--size height {h}"))?;
            }
            "--scale" => scale = value("--scale")?.parse().context("--scale is a number")?,
            "--scheme" => {
                scheme = match value("--scheme")?.as_str() {
                    "light" => ColorScheme::Light,
                    "dark" => ColorScheme::Dark,
                    other => bail!("--scheme is light or dark, not {other}"),
                }
            }
            "--page" => page = value("--page")?,
            "--timeout" => timeout = value("--timeout")?.parse().context("--timeout is seconds")?,
            flag if flag.starts_with("--") => bail!("no flag {flag}\n{USAGE}"),
            _ => plain.push(arg),
        }
    }
    let [dist, query] = <[String; 2]>::try_from(plain).map_err(|_| anyhow::anyhow!(USAGE))?;
    let shot = Shot {
        dist: PathBuf::from(dist),
        page,
        query: query.trim_start_matches('?').to_string(),
        width,
        height,
        scale,
        scheme,
        timeout: Duration::from_secs_f64(timeout),
    };
    match control {
        Some(socket) => serve(Control::open(&shot)?, &socket),
        None => window(&shot),
    }
}

/// The page in a window, on wall time.
fn window(shot: &Shot) -> anyhow::Result<()> {
    let loader = Rc::new(DistLoader::new(&shot.dist)?);
    let engine = V8Engine::new(loader)?;
    let doc = ReactDocument::open(
        ReactDocumentConfig {
            dist: shot.dist.clone(),
            page: shot.page.clone(),
            query: shot.query.clone(),
            document: DocumentConfig {
                viewport: Some(Viewport::new(shot.width, shot.height, shot.scale, shot.scheme)),
                ..Default::default()
            },
            clock: Clock::Wall,
        },
        engine,
    )?;
    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());
    let mut application = BlitzApplication::new(proxy, receiver);
    let attributes = winit::window::WindowAttributes::default()
        .with_title("react-blitz-window")
        .with_surface_size(winit::dpi::LogicalSize::new(shot.width, shot.height));
    let doc = Ticking { doc, tick: Once::new() };
    application.add_window(WindowConfig::with_attributes(Box::new(doc), Timed::new(VelloCpuWindowRenderer::new()), attributes));
    event_loop.run_app(application)?;
    Ok(())
}

/// The document, woken every few ms so JS's timers run between input and frames, as a browser's
/// loop runs them; each turn that ran JS is logged with its time.
struct Ticking {
    doc: ReactDocument<V8Engine>,
    tick: Once,
}

impl Document for Ticking {
    fn inner(&self) -> DocGuard<'_> {
        self.doc.inner()
    }

    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        self.doc.inner_mut()
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        self.doc.handle_ui_event(event);
    }

    fn poll(&mut self, cx: Option<TaskContext>) -> bool {
        if let Some(cx) = &cx {
            let waker = cx.waker().clone();
            self.tick.call_once(|| {
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_millis(4));
                        waker.wake_by_ref();
                    }
                });
            });
        }
        let start = Instant::now();
        let ran = self.doc.poll(cx);
        if ran {
            eprintln!("react-blitz-window: js {:.1} ms", start.elapsed().as_secs_f64() * 1000.0);
        }
        ran
    }
}

/// The window's renderer, logging each frame: its paint and the time since the frame before.
struct Timed {
    renderer: VelloCpuWindowRenderer,
    frame: u64,
    last: Option<Instant>,
}

impl Timed {
    fn new(renderer: VelloCpuWindowRenderer) -> Self {
        Self { renderer, frame: 0, last: None }
    }
}

impl RenderContext for Timed {}

impl WindowRenderer for Timed {
    type ScenePainter<'a>
        = <VelloCpuWindowRenderer as WindowRenderer>::ScenePainter<'a>
    where
        Self: 'a;

    fn resume<F: FnOnce() + 'static>(&mut self, window: Arc<dyn WindowHandle>, width: u32, height: u32, on_ready: F) {
        self.renderer.resume(window, width, height, on_ready);
    }

    fn complete_resume(&mut self) -> bool {
        self.renderer.complete_resume()
    }

    fn suspend(&mut self) {
        self.renderer.suspend();
    }

    fn is_active(&self) -> bool {
        self.renderer.is_active()
    }

    fn is_pending(&self) -> bool {
        self.renderer.is_pending()
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.renderer.set_size(width, height);
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        let start = Instant::now();
        self.renderer.render(draw_fn);
        let paint = start.elapsed().as_secs_f64() * 1000.0;
        let since = self.last.map(|l| format!("{:.1} ms", start.duration_since(l).as_secs_f64() * 1000.0));
        self.last = Some(start);
        self.frame += 1;
        eprintln!("react-blitz-window: frame {} paint {paint:.1} ms, since last {}", self.frame, since.as_deref().unwrap_or("-"));
    }
}
