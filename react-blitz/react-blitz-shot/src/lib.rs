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
    Ok(Image { width, height, rgba })
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
