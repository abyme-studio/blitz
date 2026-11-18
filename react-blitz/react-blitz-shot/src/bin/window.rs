//! `react-blitz-window <dist> <query> [--control <socket>] [--size WxH] [--scale N]
//! [--scheme light|dark] [--page index.html] [--timeout SECONDS] [--renderer cpu|gpu] [--bare]`
//!
//! Without `--control`: `blitz-shell` with a `ReactDocument` over the build, on wall time, painted
//! by Vello's GPU renderer or, with `--renderer cpu`, its CPU one (the shot's), into a window; each
//! frame's work by phase (`react_blitz_shot::phases`) and, every 5 s, V8's heap are logged on
//! stderr. `--bare` opens an empty document with no JS: what a Blitz window costs alone. With
//! `--control`: the page on a stepped 60 Hz clock, steered by JSON lines on the
//! Unix socket (`react_blitz_shot::control` lists them), painted with no window, as the shot is.

use std::cell::{Cell, RefCell};
use std::num::NonZero;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::{Arc, Once};
use std::task::Context as TaskContext;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use anyrender::{ImageRenderer, RenderContext, WindowHandle, WindowRenderer};
use anyrender_vello::VelloWindowRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{BaseDocument, DocGuard, DocGuardMut, Document, DocumentConfig};
use blitz_shell::{BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop};
use blitz_traits::events::UiEvent;
use blitz_traits::shell::{ColorScheme, Viewport};
use react_blitz::{Clock, DistLoader, ReactDocument, ReactDocumentConfig};
use react_blitz_shot::Shot;
use react_blitz_shot::control::{Control, serve};
use react_blitz_shot::phases::Phases;
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
    // The GPU by default: it has no copy of the frame to the window, a third of a scroll frame's work.
    let (mut gpu, mut bare) = (true, false);
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
        viewport: Some(shot.viewport()),
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
        application.add_window(WindowConfig::with_attributes(doc, Timed::new(Soft::new()), attributes));
        event_loop.run_app(application)?;
    }
    Ok(())
}

/// How often the page is turned for its timers.
const TICK: Duration = Duration::from_millis(4);

thread_local! {
    /// The work done since the last frame was drawn: the input and the turns, added as they run.
    static SINCE: RefCell<Phases> = RefCell::default();
    /// When the document was last borrowed to change: a redraw's first act, before its resolve.
    static BORROWED: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The part of the frame being drawn that handed its pixels to the window.
    static PRESENT: Cell<f64> = const { Cell::new(0.0) };
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// The document, woken every few ms so JS's timers run between input and frames, as a browser's
/// loop runs them; its input and turns are timed for the frame they lead to, and V8's heap is
/// logged every 5 s.
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
        BORROWED.set(Some(Instant::now()));
        self.doc.inner_mut()
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        let start = Instant::now();
        self.doc.handle_ui_event(event);
        SINCE.with_borrow_mut(|p| p.input += ms(start));
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
        SINCE.with_borrow_mut(|p| p.turn += ms(start));
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

/// The window's renderer, logging each frame: its work by phase, and the time since the frame
/// before. The resolve is the time from the redraw borrowing the document to its render; with the
/// GPU renderer the raster holds the present, which Vello does not time apart.
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
        let resolve = BORROWED.take().map_or(0.0, |at| start.duration_since(at).as_secs_f64() * 1000.0);
        let mut scene = 0.0;
        PRESENT.set(0.0);
        self.renderer.render(|painter| {
            let at = Instant::now();
            draw_fn(painter);
            scene = ms(at);
        });
        let present = PRESENT.take();
        let raster = ms(start) - scene - present;
        let phases = SINCE.with_borrow_mut(|p| Phases { resolve, scene, raster, present, ..std::mem::take(p) });
        let since = self.last.map(|l| format!("{:.1} ms", start.duration_since(l).as_secs_f64() * 1000.0));
        self.last = Some(start);
        self.frame += 1;
        eprintln!("react-blitz-window: frame {} {phases}; since last {}", self.frame, since.as_deref().unwrap_or("-"));
    }
}

/// Vello's CPU renderer into the window through softbuffer, as `softbuffer_window_renderer` does
/// it, with the copy to the window and its present timed apart from the raster.
struct Soft {
    // The surface before the window, so it is dropped first.
    surface: Option<softbuffer::Surface<Arc<dyn WindowHandle>, Arc<dyn WindowHandle>>>,
    renderer: VelloCpuImageRenderer,
    buffer: Vec<u8>,
    width: u32,
    height: u32,
}

impl Soft {
    fn new() -> Self {
        Self { surface: None, renderer: VelloCpuImageRenderer::new(0, 0), buffer: Vec::new(), width: 0, height: 0 }
    }
}

impl RenderContext for Soft {}

impl WindowRenderer for Soft {
    type ScenePainter<'a>
        = <VelloCpuImageRenderer as ImageRenderer>::ScenePainter<'a>
    where
        Self: 'a;

    fn resume<F: FnOnce() + 'static>(&mut self, window: Arc<dyn WindowHandle>, width: u32, height: u32, on_ready: F) {
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer takes the window");
        self.surface = Some(softbuffer::Surface::new(&context, window).expect("softbuffer makes a surface"));
        self.set_size(width, height);
        on_ready();
    }

    fn complete_resume(&mut self) -> bool {
        true
    }

    fn suspend(&mut self) {
        self.surface = None;
    }

    fn is_active(&self) -> bool {
        self.surface.is_some()
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        if let Some(surface) = &mut self.surface {
            surface.resize(NonZero::new(width.max(1)).unwrap(), NonZero::new(height.max(1)).unwrap()).expect("the surface resizes");
            self.renderer.resize(width, height);
        }
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        let Some(surface) = &mut self.surface else { return };
        let Ok(mut pixels) = surface.buffer_mut() else { return };
        let (width, height) = (self.width as f64, self.height as f64);
        self.renderer.render_to_vec(
            |painter| {
                use anyrender::PaintScene as _;
                painter.fill(
                    peniko::Fill::NonZero,
                    Default::default(),
                    peniko::Color::WHITE,
                    Default::default(),
                    &peniko::kurbo::Rect::new(0.0, 0.0, width, height),
                    anyrender::AntiAliasing::Enabled,
                );
                draw_fn(painter);
            },
            &mut self.buffer,
        );
        let start = Instant::now();
        let (chunks, _) = self.buffer.as_chunks::<4>();
        assert_eq!(chunks.len(), pixels.len());
        for (&[r, g, b, _], out) in chunks.iter().zip(pixels.iter_mut()) {
            *out = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        }
        pixels.present().expect("the window takes the frame");
        PRESENT.set(ms(start));
        self.renderer.reset();
    }
}
