//! The atlas: glyphs rasterized once by `vello_cpu` and kept, as GPUI's sprite atlas keeps them.
//! A plain glyph goes to the single-channel page as coverage, tinted when drawn; a colour glyph
//! (an emoji) and an image go to the colour page, premultiplied. Pages are placed by `etagere`.
//! An image above [`OWN_TEXTURE`] px on a side gets a texture of its own. Nothing here touches the
//! GPU: what is new is queued as [`Upload`]s for the frame's render. What the shaders cannot
//! draw is drawn by `vello_cpu` into the colour page as one entry, or a texture of its own.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash as _, Hasher as _};

use anyrender::ImageRenderer as _;
use anyrender::recording::Scene;
use anyrender_vello_cpu::VelloCpuImageRenderer;

use etagere::{AtlasAllocator, size2};
use kurbo::Affine;
use peniko::{Color, FontData, ImageAlphaType, ImageData, ImageFormat};
use vello_cpu::{Glyph, PixmapMut, RenderContext, Resources};

use crate::frame_times;

/// Each page's side, in texels.
pub(crate) const PAGE: u32 = 2048;
/// An image wider or taller than this is a texture of its own.
pub(crate) const OWN_TEXTURE: u32 = 1024;
/// Horizontal positions a glyph is rasterized at within a pixel, as GPUI's four.
pub(crate) const SUBPIXELS: f64 = 4.0;
/// Space left around a glyph in its page, so sampling never reads its neighbour.
const GAP: i32 = 1;

/// Where a sprite's texels are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Page {
    /// The single-channel page: coverage.
    Mono,
    /// The colour page: premultiplied RGBA.
    Color,
    /// A texture of its own: an image's, by its blob's id, or a CPU drawing's, by its key.
    Image(u64),
}

/// Texels to write before the frame draws.
pub(crate) struct Upload {
    pub page: Page,
    /// x, y, width, height in texels.
    pub rect: [u32; 4],
    /// One byte a texel on [`Page::Mono`], four elsewhere.
    pub bytes: Vec<u8>,
}

/// A glyph or an image in a page: its texels, and where its top left sits from the point it
/// is drawn at, in device pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Entry {
    pub page: Page,
    /// x, y, width, height in texels.
    pub uv: [f32; 4],
    pub offset: [f32; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u64,
    index: u32,
    glyph: u32,
    /// The size in device pixels, as bits.
    size: u32,
    coords: Box<[i16]>,
    /// The embolden in device pixels, as bits.
    embolden: [u32; 2],
    /// The glyph transform's linear part, as bits, when there is one.
    skew: Option<[u64; 4]>,
    hint: bool,
    /// Which of the [`SUBPIXELS`] positions.
    subpixel: u8,
}

/// One glyph to place, as the painter asks for it.
pub(crate) struct GlyphAsk<'a> {
    pub font: &'a FontData,
    pub glyph: u32,
    /// Device pixels.
    pub size: f32,
    pub coords: &'a [i16],
    /// Device pixels.
    pub embolden: kurbo::Vec2,
    pub skew: Option<Affine>,
    pub hint: bool,
    /// Which of the [`SUBPIXELS`] positions.
    pub subpixel: u8,
}

pub(crate) struct Atlas {
    mono: AtlasAllocator,
    color: AtlasAllocator,
    glyphs: HashMap<GlyphKey, Option<Entry>>,
    images: HashMap<u64, Entry>,
    /// What the CPU drew, by a hash of its moved recording and size.
    drawn: HashMap<u64, Entry>,
    /// Textures of their own, by id, with their size; those drawn this frame; and those to free.
    pub(crate) own: HashMap<u64, (u32, u32)>,
    own_used: HashSet<u64>,
    pub(crate) freed: Vec<u64>,
    /// A page filled this frame: it is emptied at the next frame's start.
    full: bool,
    pub(crate) uploads: Vec<Upload>,
    raster: RenderContext,
    resources: Resources,
    scratch: Vec<u8>,
}

impl Default for Atlas {
    fn default() -> Self {
        Self {
            mono: AtlasAllocator::new(size2(PAGE as i32, PAGE as i32)),
            color: AtlasAllocator::new(size2(PAGE as i32, PAGE as i32)),
            glyphs: HashMap::new(),
            images: HashMap::new(),
            drawn: HashMap::new(),
            own: HashMap::new(),
            own_used: HashSet::new(),
            freed: Vec::new(),
            full: false,
            uploads: Vec::new(),
            raster: RenderContext::new(1, 1),
            resources: Resources::default(),
            scratch: Vec::new(),
        }
    }
}

impl Atlas {
    /// The frame's start: a page that filled last frame is emptied, and its glyphs drawn anew.
    pub(crate) fn begin_frame(&mut self) {
        // A texture of its own not drawn last frame is freed.
        let used = std::mem::take(&mut self.own_used);
        let gone: Vec<u64> = self.own.keys().filter(|id| !used.contains(id)).copied().collect();
        for id in gone {
            self.own.remove(&id);
            self.drawn.retain(|_, e| e.page != Page::Image(id));
            self.freed.push(id);
        }
        if self.full {
            self.mono.clear();
            self.color.clear();
            self.glyphs.clear();
            self.images.clear();
            self.drawn.clear();
            self.full = false;
            frame_times::add(|t| t.atlas_resets += 1);
        }
    }

    /// The glyph's entry, rasterized and placed the first time it is asked for; none for a glyph
    /// with no ink (a space) or one past a full page.
    pub(crate) fn glyph(&mut self, ask: &GlyphAsk<'_>) -> Option<Entry> {
        let key = GlyphKey {
            font: ask.font.data.id(),
            index: ask.font.index,
            glyph: ask.glyph,
            size: ask.size.to_bits(),
            coords: ask.coords.into(),
            embolden: [(ask.embolden.x as f32).to_bits(), (ask.embolden.y as f32).to_bits()],
            skew: ask.skew.map(|a| {
                let c = a.as_coeffs();
                [c[0].to_bits(), c[1].to_bits(), c[2].to_bits(), c[3].to_bits()]
            }),
            hint: ask.hint,
            subpixel: ask.subpixel,
        };
        if let Some(entry) = self.glyphs.get(&key) {
            return *entry;
        }
        let entry = self.rasterize(ask);
        if entry.is_some() || !self.full {
            self.glyphs.insert(key, entry);
        }
        entry
    }

    fn rasterize(&mut self, ask: &GlyphAsk<'_>) -> Option<Entry> {
        // A box wide enough for any glyph of this size, the pen at a known point in it.
        let em = ask.size as f64 + 2.0 * ask.embolden.x.abs().max(ask.embolden.y.abs());
        let side = (em * 3.0).ceil().clamp(4.0, 1024.0) as u16;
        let pen = (em, em * 2.0);
        let x = pen.0 + ask.subpixel as f64 / SUBPIXELS;
        let (w, h) = (side as usize, side as usize);
        self.raster.reset_and_resize(side, side);
        self.raster.set_transform(Affine::translate((x, pen.1)));
        self.raster.set_paint(Color::WHITE);
        let _ = self
            .raster
            .glyph_run(&mut self.resources, ask.font)
            .font_size(ask.size)
            .hint(ask.hint)
            .normalized_coords(ask.coords)
            .font_embolden(glifo::FontEmbolden::new(kurbo::Diagonal2::new(ask.embolden.x, ask.embolden.y)))
            .glyph_transform(ask.skew.unwrap_or_default())
            .fill_glyphs(std::iter::once(Glyph { id: ask.glyph, x: 0.0, y: 0.0 }));
        self.raster.flush();
        self.scratch.clear();
        self.scratch.resize(w * h * 4, 0);
        self.raster.render(PixmapMut::new(side, side, &mut self.scratch).expect("the scratch fits"), &mut self.resources);
        frame_times::add(|t| t.glyphs_added += 1);

        // Its ink's bounds, and whether it has a colour of its own: drawn in white, a plain glyph
        // is grey everywhere (r = g = b = a, premultiplied).
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        let mut colored = false;
        for y in 0..h {
            for x in 0..w {
                let px = &self.scratch[(y * w + x) * 4..][..4];
                if px[3] == 0 {
                    continue;
                }
                colored |= px[0] != px[3] || px[1] != px[3] || px[2] != px[3];
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
        if x1 <= x0 {
            return None;
        }
        let (gw, gh) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let page = if colored { Page::Color } else { Page::Mono };
        let at = self.place(page, gw, gh)?;
        let bytes = if colored {
            (y0..y1).flat_map(|y| self.scratch[(y * w + x0) * 4..(y * w + x1) * 4].iter().copied()).collect()
        } else {
            (y0..y1).flat_map(|y| (x0..x1).map(move |x| (y, x))).map(|(y, x)| self.scratch[(y * w + x) * 4 + 3]).collect()
        };
        self.uploads.push(Upload { page, rect: [at.0, at.1, gw, gh], bytes });
        Some(Entry {
            page,
            uv: [at.0 as f32, at.1 as f32, gw as f32, gh as f32],
            // From the glyph's whole-pixel origin to its ink's top left.
            offset: [x0 as f32 - pen.0 as f32, y0 as f32 - pen.1 as f32],
        })
    }

    /// The image's entry, placed the first time it is drawn.
    pub(crate) fn image(&mut self, image: &ImageData) -> Option<Entry> {
        let id = image.data.id();
        if let Some(entry) = self.images.get(&id) {
            return Some(*entry);
        }
        if self.own.contains_key(&id) {
            self.own_used.insert(id);
            return Some(Entry { page: Page::Image(id), uv: [0.0, 0.0, image.width as f32, image.height as f32], offset: [0.0; 2] });
        }
        let bytes = premultiplied_rgba(image);
        frame_times::add(|t| t.images_added += 1);
        if image.width > OWN_TEXTURE || image.height > OWN_TEXTURE {
            self.own.insert(id, (image.width, image.height));
            self.own_used.insert(id);
            self.uploads.push(Upload { page: Page::Image(id), rect: [0, 0, image.width, image.height], bytes });
            return Some(Entry { page: Page::Image(id), uv: [0.0, 0.0, image.width as f32, image.height as f32], offset: [0.0; 2] });
        }
        let at = self.place(Page::Color, image.width, image.height)?;
        self.uploads.push(Upload { page: Page::Color, rect: [at.0, at.1, image.width, image.height], bytes });
        let entry = Entry { page: Page::Color, uv: [at.0 as f32, at.1 as f32, image.width as f32, image.height as f32], offset: [0.0; 2] };
        self.images.insert(id, entry);
        Some(entry)
    }

    /// What `scene` paints over `width` by `height` device pixels from its origin, drawn by
    /// `vello_cpu` the first time and kept, so the same drawing scrolled is drawn once.
    pub(crate) fn drawn(&mut self, scene: &Scene, width: u32, height: u32) -> Option<Entry> {
        let mut hasher = DefaultHasher::new();
        (width, height).hash(&mut hasher);
        format!("{:?}", scene.commands).hash(&mut hasher);
        let key = hasher.finish();
        if let Some(entry) = self.drawn.get(&key) {
            if let Page::Image(id) = entry.page {
                self.own_used.insert(id);
            }
            return Some(*entry);
        }
        let mut renderer = VelloCpuImageRenderer::new(width, height);
        let mut bytes = Vec::new();
        renderer.render_to_vec(|painter| scene.replay(painter, Affine::IDENTITY), &mut bytes);
        frame_times::add(|t| t.cpu_added += 1);
        let uv = [0.0, 0.0, width as f32, height as f32];
        let entry = if width > OWN_TEXTURE || height > OWN_TEXTURE {
            self.own.insert(key, (width, height));
            self.own_used.insert(key);
            self.uploads.push(Upload { page: Page::Image(key), rect: [0, 0, width, height], bytes });
            Entry { page: Page::Image(key), uv, offset: [0.0; 2] }
        } else {
            let at = self.place(Page::Color, width, height)?;
            self.uploads.push(Upload { page: Page::Color, rect: [at.0, at.1, width, height], bytes });
            Entry { page: Page::Color, uv: [at.0 as f32, at.1 as f32, width as f32, height as f32], offset: [0.0; 2] }
        };
        self.drawn.insert(key, entry);
        Some(entry)
    }

    /// Room in a page; none when it is full, which empties it at the next frame.
    fn place(&mut self, page: Page, w: u32, h: u32) -> Option<(u32, u32)> {
        let allocator = match page {
            Page::Mono => &mut self.mono,
            _ => &mut self.color,
        };
        match allocator.allocate(size2(w as i32 + GAP, h as i32 + GAP)) {
            Some(a) => Some((a.rectangle.min.x as u32, a.rectangle.min.y as u32)),
            None => {
                self.full = true;
                crate::scene::left_out("a glyph or image past a full atlas page");
                None
            }
        }
    }
}

/// An image's texels as premultiplied RGBA8.
fn premultiplied_rgba(image: &ImageData) -> Vec<u8> {
    let mut bytes = image.data.data().to_vec();
    if image.format == ImageFormat::Bgra8 {
        for px in bytes.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
    }
    if image.alpha_type == ImageAlphaType::Alpha {
        for px in bytes.chunks_exact_mut(4) {
            let a = px[3] as u32;
            for c in &mut px[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
    bytes
}

impl std::fmt::Debug for Atlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Atlas").field("glyphs", &self.glyphs.len()).field("images", &self.images.len()).field("drawn", &self.drawn.len()).field("full", &self.full).finish()
    }
}
