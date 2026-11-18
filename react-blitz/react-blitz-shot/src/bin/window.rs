//! `react-blitz-window <dist> <query> [--control <socket>] [--size WxH] [--scale N]
//! [--scheme light|dark] [--page index.html] [--timeout SECONDS] [--renderer cpu|gpu] [--bare]`
//!
//! Without `--control`: `blitz-shell` with a `ReactDocument` over the build, on wall time, painted
//! by Vello's CPU renderer (the shot's) or, with `--renderer gpu`, its GPU one, into a window; each
//! frame's time, each turn of JS and, every 5 s, V8's heap are logged on stderr. `--bare` opens an
//! empty document with no JS: what a Blitz window costs alone. With `--control`: the page on a stepped 60 Hz clock, steered by JSON lines on the
//! Unix socket (`react_blitz_shot::control` lists them), painted with no window, as the shot is.

use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::{Arc, Once};
use std::task::Context as TaskContext;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use anyrender::{RenderContext, WindowHandle, WindowRenderer};
use anyrender_vello::VelloWindowRenderer;
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_dom::{BaseDocument, DocGuard, DocGuardMut, Document, DocumentConfig};
use blitz_shell::{BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop};
use blitz_traits::events::UiEvent;
use blitz_traits::shell::{ColorScheme, Viewport};
use react_blitz::{Clock, DistLoader, ReactDocument, ReactDocumentConfig};
use react_blitz_shot::Shot;
use react_blitz_shot::control::{Control, serve};
use react_blitz_v8::V8Engine;

const USAGE: &str = "usage: react-blitz-window <dist> <query> [--control <socket>] [--size WxH] \
[--scale N] [--scheme light|dark] [--page index.html] [--timeout SECONDS] [--renderer cpu|gpu] [--bare]";

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
    let (mut gpu, mut bare) = (false, false);
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
            "--renderer" => {
                gpu = match value("--renderer")?.as_str() {
                    "cpu" => false,
                    "gpu" => true,
                    other => bail!("--renderer is cpu or gpu, not {other}"),
                }
            }
            "--bare" => bare = true,
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
        None => window(&shot, gpu, bare),
    }
}

/// The page in a window, on wall time; `bare`, an empty document in its place.
fn window(shot: &Shot, gpu: bool, bare: bool) -> anyhow::Result<()> {
    let document = DocumentConfig {
        viewport: Some(Viewport::new(shot.width, shot.height, shot.scale, shot.scheme)),
        ..Default::default()
    };
    let doc: Box<dyn Document> = if bare {
        Box::new(BaseDocument::new(document))
    } else {
        let loader = Rc::new(DistLoader::new(&shot.dist)?);
        let mut engine = V8Engine::new(loader)?;
        engine.wait = Duration::ZERO;
        let doc = ReactDocument::open(
            ReactDocumentConfig {
                dist: shot.dist.clone(),
                page: shot.page.clone(),
                query: shot.query.clone(),
                document,
                clock: Clock::Wall,
            },
            engine,
        )?;
        Box::new(Ticking { doc, tick: Once::new(), heap: Instant::now(), turned: Instant::now() })
    };
    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());
    let attributes = winit::window::WindowAttributes::default()
        .with_title("react-blitz-window")
        .with_surface_size(winit::dpi::LogicalSize::new(shot.width, shot.height));
    if gpu {
        let mut application = BlitzApplication::new(proxy, receiver);
        application.add_window(WindowConfig::with_attributes(doc, Timed::new(VelloWindowRenderer::new()), attributes));
        event_loop.run_app(application)?;
    } else {
        let mut application = BlitzApplication::new(proxy, receiver);
        application.add_window(WindowConfig::with_attributes(doc, Timed::new(VelloCpuWindowRenderer::new()), attributes));
        event_loop.run_app(application)?;
    }
    Ok(())
}

/// How often the page is turned for its timers.
const TICK: Duration = Duration::from_millis(4);

/// The document, woken every few ms so JS's timers run between input and frames, as a browser's
/// loop runs them; each turn that ran JS is logged with its time, and V8's heap every 5 s.
struct Ticking {
    doc: ReactDocument<V8Engine>,
    tick: Once,
    heap: Instant,
    /// When the page was last turned: the shell polls after every event, and a turn costs a ms.
    turned: Instant,
}

impl Document for Ticking {
    fn inner(&self) -> DocGuard<'_> {
        self.doc.inner()
    }

    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        self.doc.inner_mut()
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        let start = Instant::now();
        self.doc.handle_ui_event(event);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        if ms >= 1.0 {
            eprintln!("react-blitz-window: event {ms:.1} ms");
        }
    }

    fn poll(&mut self, cx: Option<TaskContext>) -> bool {
        if let Some(cx) = &cx {
            let waker = cx.waker().clone();
            self.tick.call_once(|| {
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(TICK);
                        waker.wake_by_ref();
                    }
                });
            });
        }
        let start = Instant::now();
        if start.duration_since(self.turned) < TICK {
            return false;
        }
        self.turned = start;
        let ran = self.doc.poll(cx);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        if ms >= 1.0 {
            eprintln!("react-blitz-window: js {ms:.1} ms");
        }
        if self.heap.elapsed() >= Duration::from_secs(5) {
            self.heap = Instant::now();
            let nodes = self.doc.inner().tree().len();
            let [used, total, physical, external, malloced] = self.doc.engine_mut().heap().map(|b| b as f64 / 1e6);
            eprintln!(
                "react-blitz-window: v8 heap used {used:.1} MB, total {total:.1}, physical {physical:.1}, external {external:.1}, malloced {malloced:.1}; {nodes} nodes of {} bytes",
                std::mem::size_of::<blitz_dom::Node>()
            );
        }
        ran
    }
}

/// The window's renderer, logging each frame: its paint, the part of it that built the scene, and
/// the time since the frame before.
struct Timed<R> {
    renderer: R,
    frame: u64,
    last: Option<Instant>,
}

impl<R> Timed<R> {
    fn new(renderer: R) -> Self {
        Self { renderer, frame: 0, last: None }
    }
}

impl<R: WindowRenderer + 'static> RenderContext for Timed<R> {}

impl<R: WindowRenderer + 'static> WindowRenderer for Timed<R> {
    type ScenePainter<'a>
        = R::ScenePainter<'a>
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
        let mut scene = 0.0;
        self.renderer.render(|painter| {
            let at = Instant::now();
            draw_fn(painter);
            scene = at.elapsed().as_secs_f64() * 1000.0;
        });
        let paint = start.elapsed().as_secs_f64() * 1000.0;
        let since = self.last.map(|l| format!("{:.1} ms", start.duration_since(l).as_secs_f64() * 1000.0));
        self.last = Some(start);
        self.frame += 1;
        eprintln!(
            "react-blitz-window: frame {} paint {paint:.1} ms, since last {}, scene {scene:.1} ms",
            self.frame,
            since.as_deref().unwrap_or("-")
        );
    }
}
