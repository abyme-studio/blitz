//! Paint a built React page offscreen: a `ReactDocument` with no window, run until its root
//! element has `data-preview-ready`, painted by `blitz_paint::paint_scene` into Vello's CPU
//! renderer, the paint path the window uses. [`control`] is `react-blitz-window --control`'s
//! stepped page, steered over a socket.

pub mod control;
pub mod phases;
pub mod tick;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use anyrender::{AntiAliasing, ImageRenderer, PaintScene as _};
use anyrender_vello::VelloImageRenderer;
use anyrender_quads::QuadsImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::util::Color;
use blitz_dom::{Document as _, DocumentConfig};
use blitz_paint::paint_scene;
use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta, BlitzWheelEvent, PointerCoords, UiEvent,
};
use blitz_traits::shell::{ColorScheme, Viewport};
use peniko::Fill;
use peniko::kurbo::Rect;
use react_blitz::{DistLoader, ReactDocument, ReactDocumentConfig};
use react_blitz_v8::V8Engine;

/// One shot: the build, its page and query, and the frame to paint.
pub struct Shot {
    pub dist: PathBuf,
    /// The page's HTML, from the build's root.
    pub page: String,
    /// The address's query, without its `?`.
    pub query: String,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub scheme: ColorScheme,
    /// How long the page may take to set `data-preview-ready`.
    pub timeout: Duration,
}

/// A painted frame: RGBA, `width * height * 4` bytes, in device pixels.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Every element's box under the page's `[data-preview-frame]` (else its `body`), as Blitz laid
    /// it out: what a browser's `getBoundingClientRect` says, from the frame's top left.
    pub boxes: Vec<LaidBox>,
}

/// An element's border box, in CSS pixels from the frame's top left.
pub struct LaidBox {
    /// Its place in the frame: each element's index among its parent's element children, from the
    /// frame, joined by `.` (the frame itself is ``): the key both renderers' boxes match on.
    pub path: String,
    pub tag: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Shot {
    /// The frame as Blitz takes it: its size in device pixels, and the scale.
    pub fn viewport(&self) -> Viewport {
        let device = |css: u32| (css as f32 * self.scale).round() as u32;
        Viewport::new(device(self.width), device(self.height), self.scale, self.scheme)
    }
}

/// The page's own clock steps a frame at a time, so the same page paints the same pixels.
const FRAME: f64 = 1.0 / 60.0;

pub fn shoot(shot: &Shot) -> anyhow::Result<Image> {
    let (mut doc, _) = open(shot)?;
    let boxes = laid_boxes(&*doc.inner());
    let (width, height, rgba) = paint(&mut doc.inner_mut(), shot.width, shot.height, shot.scale);
    Ok(Image { width, height, rgba, boxes })
}

/// The page, run until it is ready and laid out, and its clock.
fn open(shot: &Shot) -> anyhow::Result<(ReactDocument<V8Engine>, f64)> {
    let loader = Rc::new(DistLoader::new(&shot.dist)?);
    let engine = V8Engine::new(loader)?;
    let mut doc = ReactDocument::open(
        ReactDocumentConfig {
            dist: shot.dist.clone(),
            page: shot.page.clone(),
            query: shot.query.clone(),
            document: DocumentConfig {
                viewport: Some(shot.viewport()),
                ..Default::default()
            },
            clock: Default::default(),
        },
        engine,
    )?;

    let deadline = Instant::now() + shot.timeout;
    let mut time = 0.0;
    loop {
        doc.poll(None);
        doc.inner_mut().resolve(time);
        if let Some(error) = root_attr(&doc, "data-preview-error") {
            bail!("the page says it could not draw: {error}");
        }
        if root_attr(&doc, "data-preview-ready").is_some() {
            break;
        }
        if Instant::now() > deadline {
            bail!(
                "the page did not set data-preview-ready on its root element within {:?}",
                shot.timeout
            );
        }
        time += FRAME;
        std::thread::sleep(Duration::from_millis(2));
    }
    // One more turn, so what ready's commit asked for (a frame, a measure) is drawn.
    time += FRAME;
    doc.poll(None);
    doc.inner_mut().resolve(time);
    Ok((doc, time))
}

/// One frame of [`scroll_frames`], in ms.
#[derive(Clone, Copy)]
pub struct ScrollFrame {
    /// Building the scene from the paint tree.
    pub scene: f64,
    /// Turning the scene into pixels in memory: on the GPU, its draw and the copy back.
    pub raster: f64,
    /// The scroll containers' kept content: fragments built this frame, and drawn again as kept.
    pub fragments: blitz_paint::FragmentStats,
    /// Glyph outlines Vello encoded for the frame: glyphs its cache did not hold. 0 on the CPU
    /// renderer, which does not count them.
    pub glyph_outlines: u32,
    /// Inline layouts (Parley's) built since the frame before.
    pub text_layouts: u32,
    /// On the quads back end: instances drawn (quads and shadows), its draw calls, and the
    /// painter calls it left out. 0 on Vello.
    pub instances: u32,
    pub draws: u32,
    pub left_out: u32,
}

/// What paints an offscreen frame: Vello's CPU renderer, its GPU one, or the quads back end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raster {
    Cpu,
    Gpu,
    Quads,
}

/// The page scrolled by a wheel of `scroll` CSS pixels (down when positive) at its middle before
/// each of `frames` frames, each painted whole with no window by one renderer kept between them:
/// Vello's on the GPU or the CPU, or the quads back end. Answers each frame's times and the last frame.
pub fn scroll_frames(shot: &Shot, frames: usize, scroll: f64, raster: Raster) -> anyhow::Result<(Vec<ScrollFrame>, Image)> {
    match raster {
        Raster::Cpu => scroll_with::<VelloCpuImageRenderer>(shot, frames, scroll),
        Raster::Gpu => scroll_with::<VelloImageRenderer>(shot, frames, scroll),
        Raster::Quads => scroll_with::<QuadsImageRenderer>(shot, frames, scroll),
    }
}

fn scroll_with<R: ImageRenderer>(shot: &Shot, frames: usize, scroll: f64) -> anyhow::Result<(Vec<ScrollFrame>, Image)> {
    let mut page = Offscreen::<R>::open(shot)?;
    let (x, y) = (shot.width as f32 / 2.0, shot.height as f32 / 2.0);
    // Blitz scrolls what the pointer is over: put it at the middle before the first wheel.
    page.pointer(x, y);
    let mut times = Vec::with_capacity(frames);
    for _ in 0..frames {
        page.wheel(x, y, scroll);
        times.push(page.frame());
    }
    Ok((times, page.image()))
}

/// A page painted whole with no window, frame after frame, by one renderer kept between them.
pub struct Offscreen<R: ImageRenderer> {
    doc: ReactDocument<V8Engine>,
    renderer: R,
    time: f64,
    scale: f32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl<R: ImageRenderer> Offscreen<R> {
    pub fn open(shot: &Shot) -> anyhow::Result<Self> {
        let (doc, time) = open(shot)?;
        let width = (shot.width as f32 * shot.scale).round() as u32;
        let height = (shot.height as f32 * shot.scale).round() as u32;
        Ok(Self { doc, renderer: R::new(width, height), time, scale: shot.scale, width, height, rgba: Vec::new() })
    }

    /// Move the pointer to a point of the viewport, in CSS pixels, no button held.
    pub fn pointer(&mut self, x: f32, y: f32) {
        let at = self.doc.inner().viewport_scroll();
        let coords = PointerCoords { screen_x: x, screen_y: y, client_x: x, client_y: y, page_x: x + at.x as f32, page_y: y + at.y as f32 };
        self.doc.handle_ui_event(UiEvent::PointerMove(BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords,
            button: Default::default(),
            buttons: Default::default(),
            mods: Default::default(),
            details: Default::default(),
            element: Default::default(),
            active_pointers: Default::default(),
        }));
    }

    /// A wheel of `scroll` CSS pixels (down when positive) at a point of the viewport.
    pub fn wheel(&mut self, x: f32, y: f32, scroll: f64) {
        let at = self.doc.inner().viewport_scroll();
        self.doc.handle_ui_event(UiEvent::Wheel(BlitzWheelEvent {
            // Blitz's wheel, as the window makes it from winit's: positive scrolls up.
            delta: BlitzWheelDelta::Pixels(0.0, -scroll),
            phase: Default::default(),
            source: Default::default(),
            coords: PointerCoords { screen_x: x, screen_y: y, client_x: x, client_y: y, page_x: x + at.x as f32, page_y: y + at.y as f32 },
            buttons: Default::default(),
            mods: Default::default(),
            element: Default::default(),
        }));
    }

    /// The next frame: the page turned and resolved a display frame on, then painted.
    pub fn frame(&mut self) -> ScrollFrame {
        self.time += FRAME;
        self.doc.poll(None);
        self.doc.inner_mut().resolve(self.time);
        let text_layouts = self.doc.inner_mut().take_resolve_stats().text_layouts;

        let start = Instant::now();
        let mut scene_ms = 0.0;
        let mut inner = self.doc.inner_mut();
        let (width, height, scale) = (self.width, self.height, self.scale);
        blitz_paint::take_fragment_stats();
        self.renderer.render_to_vec(
            |scene| {
                let at = Instant::now();
                scene.fill(
                    Fill::NonZero,
                    Default::default(),
                    Color::WHITE,
                    Default::default(),
                    &Rect::new(0.0, 0.0, width as f64, height as f64),
                    AntiAliasing::Enabled,
                );
                paint_scene(scene, &mut inner, scale as f64, width, height, 0, 0);
                scene_ms = at.elapsed().as_secs_f64() * 1000.0;
            },
            &mut self.rgba,
        );
        self.renderer.reset();
        let quads = anyrender_quads::take_frame_times();
        ScrollFrame {
            scene: scene_ms,
            raster: start.elapsed().as_secs_f64() * 1000.0 - scene_ms,
            fragments: blitz_paint::take_fragment_stats(),
            glyph_outlines: anyrender_vello::take_frame_times().glyph_outlines,
            text_layouts,
            instances: quads.quads + quads.shadows,
            draws: quads.draws,
            left_out: quads.left_out.values().sum(),
        }
    }

    /// The last frame painted.
    pub fn image(&self) -> Image {
        Image { width: self.width, height: self.height, rgba: self.rgba.clone(), boxes: Vec::new() }
    }
}

/// The middle of `values`, sorted: of an even count, the upper of its middle two.
pub fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

/// Paint the document's frame as it stands, on white, by `blitz_paint::paint_scene` into Vello's
/// CPU renderer: `width` and `height` in CSS pixels; answers the device size and its RGBA.
pub fn paint(doc: &mut blitz_dom::BaseDocument, width: u32, height: u32, scale: f32) -> (u32, u32, Vec<u8>) {
    let (width, height, rgba, _, _) = paint_timed(doc, width, height, scale);
    (width, height, rgba)
}

/// [`paint`], and its two parts in ms: building the scene, then turning it into pixels.
pub fn paint_timed(doc: &mut blitz_dom::BaseDocument, width: u32, height: u32, scale: f32) -> (u32, u32, Vec<u8>, f64, f64) {
    let width = (width as f32 * scale).round() as u32;
    let height = (height as f32 * scale).round() as u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    let start = Instant::now();
    let mut scene_ms = 0.0;
    let mut renderer = VelloCpuImageRenderer::new(width, height);
    renderer.render_to_vec(
        |scene| {
            let at = Instant::now();
            scene.fill(
                Fill::NonZero,
                Default::default(),
                Color::WHITE,
                Default::default(),
                &Rect::new(0.0, 0.0, width as f64, height as f64),
                AntiAliasing::Enabled,
            );
            paint_scene(scene, doc, scale as f64, width, height, 0, 0);
            scene_ms = at.elapsed().as_secs_f64() * 1000.0;
        },
        &mut rgba,
    );
    let raster_ms = start.elapsed().as_secs_f64() * 1000.0 - scene_ms;
    (width, height, rgba, scene_ms, raster_ms)
}

/// The frame's boxes and every element's under it, then each portal's: each element child of `body`
/// after the one that holds the frame, keyed `portal<i>`. An `<svg>`'s insides are its picture, not boxes.
/// `get_client_bounding_rect` answers in CSS pixels, at any scale.
fn laid_boxes(doc: &blitz_dom::BaseDocument) -> Vec<LaidBox> {
    let elements = |id: blitz_dom::NodeId| -> Vec<blitz_dom::NodeId> {
        doc.get_node(id)
            .map(|n| n.children.iter().copied().filter(|c| doc.get_node(*c).is_some_and(|k| k.element_data().is_some())).collect())
            .unwrap_or_default()
    };
    let tag = |id: blitz_dom::NodeId| doc.get_node(id).and_then(|n| n.element_data()).map(|e| e.name.local.to_string()).unwrap_or_default();
    let Some(body) = doc.try_root_element().and_then(|root| elements(root.id).into_iter().find(|c| tag(*c) == "body")) else {
        return Vec::new();
    };
    let frame = doc
        .tree()
        .iter()
        .find(|(_, n)| n.element_data().is_some_and(|e| e.attrs().iter().any(|a| &*a.name.local == "data-preview-frame")))
        .map(|(id, _)| id);
    // The body's child that holds the frame: what follows it are the portals.
    let holder = frame.and_then(|f| {
        let mut at = f;
        while let Some(parent) = doc.get_node(at).and_then(|n| n.parent) {
            if parent == body {
                return Some(at);
            }
            at = parent;
        }
        None
    });
    let mut roots = vec![(frame.unwrap_or(body), String::new())];
    if let Some(holder) = holder {
        let kids = elements(body);
        let after = kids.iter().position(|k| *k == holder).map_or(0, |i| i + 1);
        roots.extend(kids[after..].iter().enumerate().map(|(i, k)| (*k, format!("portal{i}"))));
    }
    let rect = |id: blitz_dom::NodeId| laid_rect(doc, id);
    let Some(origin) = rect(roots[0].0) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (root, key) in roots {
        let mut todo = vec![(root, key)];
        while let Some((id, path)) = todo.pop() {
            let name = tag(id);
            if let Some(r) = rect(id) {
                out.push(LaidBox {
                    path: path.clone(),
                    tag: name.clone(),
                    x: r.x - origin.x,
                    y: r.y - origin.y,
                    width: r.width,
                    height: r.height,
                });
            }
            if name == "svg" {
                continue;
            }
            for (i, kid) in elements(id).into_iter().enumerate().rev() {
                todo.push((kid, if path.is_empty() { i.to_string() } else { format!("{path}.{i}") }));
            }
        }
    }
    out
}

/// A node's border box from the viewport's top left, in CSS pixels. Blitz can keep a box's
/// containing block after that node is gone (a subtree hidden, then its parent replaced), and its
/// bounding rect then panics: such a box is unlaid, 0×0 as a browser says.
pub(crate) fn laid_rect(doc: &blitz_dom::BaseDocument, id: blitz_dom::NodeId) -> Option<blitz_dom::BoundingRect> {
    let mut at = doc.get_node(id).and_then(|n| n.containing_block());
    while let Some(cb) = at {
        let Some(node) = doc.get_node(cb) else {
            return Some(blitz_dom::BoundingRect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 });
        };
        at = node.containing_block();
    }
    doc.get_client_bounding_rect(id)
}

/// `boxes` as JSON: `[{"path","tag","x","y","width","height"}]`.
pub fn boxes_json(boxes: &[LaidBox]) -> String {
    let rows: Vec<String> = boxes
        .iter()
        .map(|b| format!(r#"{{"path":"{}","tag":"{}","x":{},"y":{},"width":{},"height":{}}}"#, b.path, b.tag, b.x, b.y, b.width, b.height))
        .collect();
    format!("[{}]", rows.join(","))
}

/// The page's root element's attribute `name`.
pub fn root_attr<D: blitz_dom::Document>(doc: &D, name: &str) -> Option<String> {
    let inner = doc.inner();
    let root = inner.try_root_element()?;
    root.attrs()?
        .iter()
        .find(|a| &*a.name.local == name)
        .map(|a| a.value.clone())
}

/// Write `image` as an 8-bit RGBA PNG.
pub fn write_png(image: &Image, out: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::create(out).with_context(|| format!("cannot write {}", out.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image.rgba)?;
    writer.finish()?;
    Ok(())
}
