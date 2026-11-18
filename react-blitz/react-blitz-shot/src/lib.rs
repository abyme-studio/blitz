//! Paint a built React page offscreen: a `ReactDocument` with no window, run until its root
//! element has `data-preview-ready`, painted by `blitz_paint::paint_scene` into Vello's CPU
//! renderer, the paint path the window uses.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use anyrender::{AntiAliasing, PaintScene as _, render_to_buffer};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::util::Color;
use blitz_dom::{Document as _, DocumentConfig};
use blitz_paint::paint_scene;
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

/// The page's own clock steps a frame at a time, so the same page paints the same pixels.
const FRAME: f64 = 1.0 / 60.0;

pub fn shoot(shot: &Shot) -> anyhow::Result<Image> {
    let loader = Rc::new(DistLoader::new(&shot.dist)?);
    let engine = V8Engine::new(loader)?;
    let mut doc = ReactDocument::open(
        ReactDocumentConfig {
            dist: shot.dist.clone(),
            page: shot.page.clone(),
            query: shot.query.clone(),
            document: DocumentConfig {
                viewport: Some(Viewport::new(shot.width, shot.height, shot.scale, shot.scheme)),
                ..Default::default()
            },
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
    doc.poll(None);
    doc.inner_mut().resolve(time + FRAME);

    let boxes = laid_boxes(&*doc.inner());
    let width = (shot.width as f32 * shot.scale).round() as u32;
    let height = (shot.height as f32 * shot.scale).round() as u32;
    let mut inner = doc.inner_mut();
    let rgba = render_to_buffer::<VelloCpuImageRenderer, _>(
        |scene| {
            scene.fill(
                Fill::NonZero,
                Default::default(),
                Color::WHITE,
                Default::default(),
                &Rect::new(0.0, 0.0, width as f64, height as f64),
                AntiAliasing::Enabled,
            );
            paint_scene(scene, &mut inner, shot.scale as f64, width, height, 0, 0);
        },
        width,
        height,
    );
    Ok(Image { width, height, rgba, boxes })
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
    let Some(origin) = doc.get_client_bounding_rect(roots[0].0) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (root, key) in roots {
        let mut todo = vec![(root, key)];
        while let Some((id, path)) = todo.pop() {
            let name = tag(id);
            if let Some(r) = doc.get_client_bounding_rect(id) {
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

/// `boxes` as JSON: `[{"path","tag","x","y","width","height"}]`.
pub fn boxes_json(boxes: &[LaidBox]) -> String {
    let rows: Vec<String> = boxes
        .iter()
        .map(|b| format!(r#"{{"path":"{}","tag":"{}","x":{},"y":{},"width":{},"height":{}}}"#, b.path, b.tag, b.x, b.y, b.width, b.height))
        .collect();
    format!("[{}]", rows.join(","))
}

fn root_attr<D: blitz_dom::Document>(doc: &D, name: &str) -> Option<String> {
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
