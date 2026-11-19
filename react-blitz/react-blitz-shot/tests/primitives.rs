//! Each primitive of the quads back end against Vello's GPU renderer, by pixel: the primitives page
//! with one primitive (its query's `p`), painted offscreen at scale 2 by both, compared by
//! pixelmatch's colour distance at its 0.1 threshold. Boxes, clips, images, layers and what the CPU
//! draws differ in no pixel, except on a curved edge, where at most a tenth of the edge's pixels may;
//! shadows and glyphs, which each renderer anti-aliases its own way, in at most 1% of the pixels of
//! their box. Both shots and the difference are written to
//! `target/react-blitz-pages/primitives/`.
//!
//! With no GPU adapter each test fails, saying it was skipped and why: it never passes unrun.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use anyrender_quads::QuadsImageRenderer;
use anyrender_vello::VelloImageRenderer;
use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Image, Offscreen, Shot};

/// pixelmatch's default threshold, as `scripts/stories` uses it.
const THRESHOLD: f64 = 0.1;
/// The page's background, #f4f1ea: a pixel of it in both shots is outside every primitive.
const BACKGROUND: [u8; 3] = [0xf4, 0xf1, 0xea];

fn dist() -> &'static PathBuf {
    static DIST: OnceLock<PathBuf> = OnceLock::new();
    DIST.get_or_init(|| {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dist = manifest.join("../../target/react-blitz-pages/shot/primitives");
        let status = Command::new("node")
            .arg(manifest.join("../js/scripts/bundle.mjs"))
            .arg(manifest.join("tests/pages/primitives"))
            .arg(&dist)
            .status()
            .expect("node runs the bundler");
        assert!(status.success(), "bundling primitives failed");
        dist
    })
}

fn shoot<R: anyrender::ImageRenderer>(kind: &str) -> Image {
    let shot = Shot {
        dist: dist().clone(),
        page: "index.html".into(),
        query: format!("p={kind}"),
        width: 240,
        height: 200,
        scale: 2.0,
        scheme: ColorScheme::Light,
        timeout: Duration::from_secs(20),
    };
    let mut page = Offscreen::<R>::open(&shot).unwrap();
    page.frame();
    page.image()
}

/// pixelmatch's colour distance of two RGBA pixels, each blended on white, squared: YIQ weighted.
fn distance(a: &[u8], b: &[u8]) -> f64 {
    let on_white = |p: &[u8], i: usize| 255.0 + (p[i] as f64 - 255.0) * p[3] as f64 / 255.0;
    let yiq = |p: &[u8]| {
        let (r, g, b) = (on_white(p, 0), on_white(p, 1), on_white(p, 2));
        (
            r * 0.29889531 + g * 0.58662247 + b * 0.11448223,
            r * 0.59597799 - g * 0.27417610 - b * 0.32180189,
            r * 0.21147017 - g * 0.52261711 + b * 0.31114694,
        )
    };
    let ((y1, i1, q1), (y2, i2, q2)) = (yiq(a), yiq(b));
    let (y, i, q) = (y1 - y2, i1 - i2, q1 - q2);
    0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q
}

struct Compared {
    /// Pixels past the threshold.
    off: usize,
    /// Of those, the ones on an edge in Vello's shot: a neighbour of the pixel is past the
    /// threshold from it there.
    off_edge: usize,
    /// Edge pixels in Vello's shot.
    edge: usize,
    /// Pixels of the primitive's box: those either shot draws off the background, bounded.
    area: usize,
}

fn compare(kind: &str, quads: &Image, vello: &Image) -> Compared {
    assert_eq!((quads.width, quads.height), (vello.width, vello.height));
    let (w, h) = (quads.width as usize, quads.height as usize);
    let most = 35215.0 * THRESHOLD * THRESHOLD;
    let mut diff = vec![0u8; w * h * 4];
    let mut off = 0;
    let (mut off_edge, mut edge) = (0, 0);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (a, b) = (&quads.rgba[i..i + 4], &vello.rgba[i..i + 4]);
            if a[..3] != BACKGROUND || b[..3] != BACKGROUND {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
            let past = distance(a, b) > most;
            off += past as usize;
            let on_edge = (y.saturating_sub(1)..(y + 2).min(h))
                .flat_map(|ny| (x.saturating_sub(1)..(x + 2).min(w)).map(move |nx| (ny * w + nx) * 4))
                .any(|j| distance(b, &vello.rgba[j..j + 4]) > most);
            edge += on_edge as usize;
            off_edge += (past && on_edge) as usize;
            // As pixelmatch draws it: red where past, else the shot faded.
            let grey = (255.0 - 0.1 * (255.0 - a[0] as f64)) as u8;
            diff[i..i + 4].copy_from_slice(&if past { [255, 0, 0, 255] } else { [grey, grey, grey, 255] });
        }
    }
    let out = dist().join("../../primitives");
    std::fs::create_dir_all(&out).unwrap();
    for (name, rgba) in [("quads", &quads.rgba), ("vello", &vello.rgba), ("diff", &diff)] {
        write_png(&out.join(format!("{kind}-{name}.png")), quads.width, quads.height, rgba);
    }
    let area = if x1 > x0 { (x1 - x0) * (y1 - y0) } else { 0 };
    Compared { off, off_edge, edge, area }
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn both(kind: &str) -> Compared {
    if let Err(why) = anyrender_quads::adapter() {
        panic!("skipped, not passed: no GPU adapter ({why}); these tests need the Mac's GPU");
    }
    let quads = shoot::<QuadsImageRenderer>(kind);
    let vello = shoot::<VelloImageRenderer>(kind);
    let compared = compare(kind, &quads, &vello);
    assert!(compared.area > 0, "{kind}: neither renderer drew anything off the background");
    compared
}

/// A box, clip, image, layer or what the CPU draws: no pixel past the threshold.
fn exact(kind: &str) {
    let Compared { off, area, .. } = both(kind);
    assert_eq!(off, 0, "{kind}: {off} of {area} pixels differ past {THRESHOLD}");
}

/// A shadow or glyphs: at most 1% of the pixels of its box past the threshold.
fn close(kind: &str) {
    let Compared { off, area, .. } = both(kind);
    let share = off as f64 / area as f64;
    assert!(share <= 0.01, "{kind}: {off} of {area} pixels ({:.2}%) differ past {THRESHOLD}", share * 100.0);
}

/// Curved edges: each rasterizer covers a pixel of a curve its own way (Vello's own CPU and GPU
/// renderers differ there too), so a pixel on an edge may differ; one off the edges may not, and
/// at most a tenth of the edge's pixels: a box shifted by a pixel differs in about a quarter.
fn curved(kind: &str) {
    let Compared { off, off_edge, edge, .. } = both(kind);
    assert_eq!(off - off_edge, 0, "{kind}: {} pixels off the edges differ past {THRESHOLD}", off - off_edge);
    let share = off_edge as f64 / edge as f64;
    assert!(share <= 0.1, "{kind}: {off_edge} of {edge} edge pixels ({:.1}%) differ past {THRESHOLD}", share * 100.0);
    eprintln!("{kind}: {off_edge} of {edge} edge pixels ({:.1}%) differ, none off the edges", share * 100.0);
}

#[test]
fn a_rect() {
    exact("rect");
}

#[test]
fn a_rounded_rect_with_four_radii() {
    curved("rounded");
}

#[test]
fn a_1_px_border_of_radius_8() {
    curved("border");
}

#[test]
fn a_box_shadow_with_blur_and_spread() {
    close("shadow");
}

#[test]
fn an_inset_shadow() {
    close("inset");
}

#[test]
fn a_rect_clip() {
    exact("clip");
}

#[test]
fn a_rounded_clip() {
    curved("rounded-clip");
}

#[test]
fn an_image_scaled() {
    exact("image");
}

#[test]
fn an_opacity_layer() {
    exact("opacity");
}

#[test]
fn a_run_of_text() {
    close("text");
}

#[test]
fn an_emoji() {
    close("emoji");
}

#[test]
fn a_path_on_the_cpu() {
    curved("path");
}

#[test]
fn a_gradient_on_the_cpu() {
    curved("gradient");
}

#[test]
fn a_filter_layer_on_the_cpu() {
    exact("filter");
}
