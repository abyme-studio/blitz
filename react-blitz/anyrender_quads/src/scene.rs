//! The painter: what blitz-paint draws, recorded as instances in call order (CSS paint order),
//! consecutive instances of one kind joined into one draw. Nothing here touches the GPU.

use std::sync::Arc;

use anyrender::recording::Scene;
use anyrender::{
    AntiAliasing, BoxShadowKind, Filter, MaskMode, NonUniformRoundedRect, NormalizedCoord, Paint,
    PaintRef, PaintScene, RenderContext,
};
use bytemuck::{Pod, Zeroable};
use kurbo::{Affine, Insets, Rect, Shape, Stroke};
use peniko::{BlendMode, Color, Compose, Fill, FontData, Mix, StyleRef};

use crate::atlas::{Atlas, GlyphAsk, Page, SUBPIXELS};
use crate::frame_times::{self, Encoding};

/// A box: a background, a border, corner radii, under a clip. GPUI's `Quad`, solid colours only.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Quad {
    /// x, y, width, height in device pixels.
    pub bounds: [f32; 4],
    pub clip: Clip,
    /// Straight sRGB.
    pub background: [f32; 4],
    pub border_color: [f32; 4],
    /// Top left, top right, bottom right, bottom left.
    pub radii: [f32; 4],
    /// Top, right, bottom, left.
    pub borders: [f32; 4],
}

/// A box shadow: the shadow's shape blurred by a gaussian, as GPUI draws it, and cut to its
/// area as anyrender's `BoxShadowGeometry` says.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Shadow {
    /// The quad drawn: every pixel the shadow can touch.
    pub bounds: [f32; 4],
    pub clip: Clip,
    /// The shadow's shape before the blur, x, y, width, height.
    pub shape: [f32; 4],
    pub shape_radii: [f32; 4],
    /// The box that casts it.
    pub cast: [f32; 4],
    pub cast_radii: [f32; 4],
    pub color: [f32; 4],
    /// The standard deviation, then the kind: 0 outset, 1 outset unclipped, 2 inset.
    pub params: [f32; 4],
}

/// A texture drawn as a quad: an opacity layer composited at its alpha.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Sprite {
    pub bounds: [f32; 4],
    pub clip: Clip,
    /// The alpha it is drawn at, then three of padding.
    pub params: [f32; 4],
}

/// A glyph or an image from the atlas: its texels drawn over `bounds`, a plain glyph's coverage
/// tinted by `color`, a colour glyph's or an image's texels at `color`'s alpha.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct AtlasSprite {
    pub bounds: [f32; 4],
    pub clip: Clip,
    /// x, y, width, height in the page's texels.
    pub uv: [f32; 4],
    /// Straight sRGB.
    pub color: [f32; 4],
    /// 0 coverage (the single-channel page), 1 premultiplied texels; then three of padding.
    pub params: [f32; 4],
}

/// Where an instance may draw: inside `rect` and inside the rounded box `round`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub(crate) struct Clip {
    /// x0, y0, x1, y1.
    pub rect: [f32; 4],
    /// x, y, width, height; radii all 0 means no rounded clip.
    pub round: [f32; 4],
    pub round_radii: [f32; 4],
}

impl Default for Clip {
    fn default() -> Self {
        Self { rect: [-1e7, -1e7, 1e7, 1e7], round: [0.0; 4], round_radii: [0.0; 4] }
    }
}

/// One draw, in paint order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Draw {
    Quads(std::ops::Range<u32>),
    Shadows(std::ops::Range<u32>),
    /// An opacity layer, by its index in [`Recording::layers`]: drawn into a texture of its own,
    /// then composited by its sprite.
    Layer { layer: usize, sprite: u32 },
    /// Glyphs and images, all from one page or one image's own texture.
    Sprites { range: std::ops::Range<u32>, page: Page },
}

#[derive(Debug, Default)]
pub(crate) struct Layer {
    pub draws: Vec<Draw>,
}

/// One frame's instances and draws, kept by the renderer and emptied each frame.
#[derive(Debug, Default)]
pub(crate) struct Recording {
    pub quads: Vec<Quad>,
    pub shadows: Vec<Shadow>,
    pub sprites: Vec<Sprite>,
    pub atlas_sprites: Vec<AtlasSprite>,
    pub layers: Vec<Layer>,
    pub root: Vec<Draw>,
    /// Kept from frame to frame: only the instances above are the frame's.
    pub atlas: Atlas,
}

impl Recording {
    pub(crate) fn clear(&mut self) {
        self.atlas_sprites.clear();
        self.quads.clear();
        self.shadows.clear();
        self.sprites.clear();
        self.layers.clear();
        self.root.clear();
    }
}

/// What a push did, for its pop to undo.
enum Pushed {
    /// The clip before it.
    Clip(Clip),
    /// An opacity layer: the clip before it, its index, and the alpha.
    Layer { clip: Clip, layer: usize, alpha: f32 },
}

/// A layer the shaders cannot draw (a filter, a clip to a path, a turn), recorded from its push to
/// its pop and drawn on the CPU as one atlas entry.
struct Capture {
    scene: Scene,
    /// Layers open in it, its own first.
    depth: u32,
    /// What it can cover, in device pixels.
    bounds: Rect,
}

pub struct QuadsScenePainter<'r> {
    pub(crate) recording: &'r mut Recording,
    capture: Option<Capture>,
    /// The draws being added to: the frame's, or the open layer's, innermost last.
    open: Vec<Vec<Draw>>,
    pushed: Vec<Pushed>,
    clip: Clip,
    /// The device size, for a layer's sprite.
    size: (f32, f32),
}

impl<'r> QuadsScenePainter<'r> {
    pub(crate) fn new(recording: &'r mut Recording, width: u32, height: u32) -> Self {
        recording.clear();
        recording.atlas.begin_frame();
        Self {
            recording,
            capture: None,
            open: vec![Vec::new()],
            pushed: Vec::new(),
            clip: Clip::default(),
            size: (width as f32, height as f32),
        }
    }

    /// Ends the frame: layers left open are closed, and the frame's draws are kept.
    pub(crate) fn finish(mut self) {
        while !self.pushed.is_empty() {
            self.pop_layer();
        }
        self.recording.root = self.open.pop().unwrap_or_default();
    }

    fn draws(&mut self) -> &mut Vec<Draw> {
        self.open.last_mut().expect("the frame's draws are open")
    }

    fn add_quad(&mut self, quad: Quad) {
        if quad.bounds[2] <= 0.0 || quad.bounds[3] <= 0.0 || !visible(&quad.bounds, &self.clip) {
            return;
        }
        let index = self.recording.quads.len() as u32;
        self.recording.quads.push(quad);
        frame_times::add(|t| t.quads += 1);
        match self.draws().last_mut() {
            Some(Draw::Quads(range)) if range.end == index => range.end += 1,
            _ => self.draws().push(Draw::Quads(index..index + 1)),
        }
    }

    fn add_shadow(&mut self, shadow: Shadow) {
        if !visible(&shadow.bounds, &self.clip) {
            return;
        }
        let index = self.recording.shadows.len() as u32;
        self.recording.shadows.push(shadow);
        frame_times::add(|t| t.shadows += 1);
        match self.draws().last_mut() {
            Some(Draw::Shadows(range)) if range.end == index => range.end += 1,
            _ => self.draws().push(Draw::Shadows(index..index + 1)),
        }
    }

    fn add_sprite(&mut self, sprite: AtlasSprite, page: Page) {
        if !visible(&sprite.bounds, &self.clip) {
            return;
        }
        let index = self.recording.atlas_sprites.len() as u32;
        self.recording.atlas_sprites.push(sprite);
        frame_times::add(|t| t.sprites += 1);
        match self.draws().last_mut() {
            Some(Draw::Sprites { range, page: p }) if range.end == index && *p == page => range.end += 1,
            _ => self.draws().push(Draw::Sprites { range: index..index + 1, page }),
        }
    }

    /// Draws what `record` paints on the CPU, as one atlas entry over `bounds` (device pixels).
    fn on_cpu(&mut self, bounds: Rect, record: impl FnOnce(&mut Scene)) {
        let mut scene = Scene::new();
        record(&mut scene);
        self.draw_on_cpu(&scene, bounds);
    }

    /// `scene`, in device pixels, drawn on the CPU over `bounds` cut to the clip and the device,
    /// moved to its whole-pixel origin first so the same drawing scrolled is the same entry.
    fn draw_on_cpu(&mut self, scene: &Scene, bounds: Rect) {
        let (w, h) = self.size;
        let c = self.clip.rect;
        let area = Rect::new(c[0].max(0.0) as f64, c[1].max(0.0) as f64, c[2].min(w) as f64, c[3].min(h) as f64);
        let r = bounds.intersect(area);
        if r.width() <= 0.0 || r.height() <= 0.0 {
            return;
        }
        let (x0, y0) = (r.x0.floor(), r.y0.floor());
        let (pw, ph) = ((r.x1.ceil() - x0) as u32, (r.y1.ceil() - y0) as u32);
        let mut moved = Scene::new();
        scene.replay(&mut moved, Affine::translate((-x0, -y0)));
        let Some(entry) = self.recording.atlas.drawn(&moved, pw, ph) else { return };
        let clip = self.clip;
        let bounds = [x0 as f32, y0 as f32, pw as f32, ph as f32];
        self.add_sprite(AtlasSprite { bounds, clip, uv: entry.uv, color: [1.0; 4], params: [1.0, 0.0, 0.0, 0.0] }, entry.page);
    }

    /// Starts recording a layer for the CPU, `push` its push.
    fn capture(&mut self, transform: Affine, clip: &impl Shape, push: impl FnOnce(&mut Scene)) {
        let mut scene = Scene::new();
        push(&mut scene);
        let bounds = transform.transform_rect_bbox(clip.bounding_box());
        self.capture = Some(Capture { scene, depth: 1, bounds });
    }

    /// A box under `transform`, in device pixels; none for a transform that rotates or skews.
    fn device_box(&self, transform: Affine, shape: &NonUniformRoundedRect) -> Option<([f32; 4], [f32; 4])> {
        let (sx, sy, tx, ty) = axis_aligned(transform)?;
        let r = shape.rect;
        let bounds = [(r.x0 * sx + tx) as f32, (r.y0 * sy + ty) as f32, (r.width() * sx) as f32, (r.height() * sy) as f32];
        // GPUI's corners are circles: an elliptical corner takes the mean of its two radii.
        let s = (sx + sy) / 2.0;
        let corner = |v: kurbo::Vec2| ((v.x + v.y) / 2.0 * s) as f32;
        let radii = shape.radii;
        Some((bounds, [corner(radii.top_left), corner(radii.top_right), corner(radii.bottom_right), corner(radii.bottom_left)]))
    }

    /// A shape as a box, when it is a rect or a rounded rect.
    fn shape_box(shape: &impl Shape) -> Option<NonUniformRoundedRect> {
        if let Some(rect) = shape.as_rect() {
            return Some(NonUniformRoundedRect::new(rect, Default::default()));
        }
        let rounded = shape.as_rounded_rect()?;
        let r = rounded.radii();
        let v = |x: f64| kurbo::Vec2::new(x, x);
        Some(NonUniformRoundedRect::new(
            rounded.rect(),
            anyrender::NonUniformRoundedRectRadii {
                top_left: v(r.top_left),
                top_right: v(r.top_right),
                bottom_right: v(r.bottom_right),
                bottom_left: v(r.bottom_left),
            },
        ))
    }

    /// Narrows the clip to a box; false when the transform is not one the shaders draw.
    fn clip_to(&mut self, transform: Affine, shape: &NonUniformRoundedRect) -> bool {
        let Some((b, radii)) = self.device_box(transform, shape) else {
            return false;
        };
        let rect = [b[0], b[1], b[0] + b[2], b[1] + b[3]];
        let c = &mut self.clip;
        c.rect = [c.rect[0].max(rect[0]), c.rect[1].max(rect[1]), c.rect[2].min(rect[2]), c.rect[3].min(rect[3])];
        if radii.iter().any(|r| *r > 0.0) {
            if c.round_radii.iter().any(|r| *r > 0.0) {
                // One rounded clip at a time: the inner one wins, its outer corners may show.
                left_out("nested rounded clip");
            }
            c.round = b;
            c.round_radii = radii;
        }
        true
    }

    fn push_clip(&mut self, transform: Affine, clip: &impl Shape) {
        let before = self.clip;
        match Self::shape_box(clip) {
            Some(shape) if self.clip_to(transform, &shape) => {}
            _ => left_out("clip to a path"),
        }
        self.pushed.push(Pushed::Clip(before));
    }
}

/// Whether `transform` is scale and translate only, which the shaders draw.
fn straight(transform: Affine) -> bool {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    b == 0.0 && c == 0.0 && a > 0.0 && d > 0.0
}

/// `transform` as scale and translate, when it neither rotates nor skews.
fn axis_aligned(transform: Affine) -> Option<(f64, f64, f64, f64)> {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    if b != 0.0 || c != 0.0 || a <= 0.0 || d <= 0.0 {
        left_out("a rotating or flipping transform");
        return None;
    }
    Some((a, d, e, f))
}

fn visible(bounds: &[f32; 4], clip: &Clip) -> bool {
    bounds[0] < clip.rect[2]
        && bounds[1] < clip.rect[3]
        && bounds[0] + bounds[2] > clip.rect[0]
        && bounds[1] + bounds[3] > clip.rect[1]
        && clip.rect[0] < clip.rect[2]
        && clip.rect[1] < clip.rect[3]
}

fn rgba(color: Color) -> [f32; 4] {
    color.components
}

/// Something this back end does not draw yet, counted by kind for the frame's log.
pub(crate) fn left_out(what: &'static str) {
    frame_times::left_out(what);
}

impl RenderContext for QuadsScenePainter<'_> {}

/// Inside a layer recorded for the CPU, a call goes to its recording.
macro_rules! captured {
    ($self:ident, $scene:ident => $call:expr) => {
        if let Some(capture) = $self.capture.as_mut() {
            let $scene = &mut capture.scene;
            $call;
            return;
        }
    };
}

impl PaintScene for QuadsScenePainter<'_> {
    fn reset(&mut self) {
        self.capture = None;
        self.recording.clear();
        self.open = vec![Vec::new()];
        self.pushed.clear();
        self.clip = Clip::default();
    }

    fn push_layer(
        &mut self,
        fill: Fill,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
        filter: Option<Arc<Filter>>,
        backdrop_filter: Option<Arc<Filter>>,
    ) {
        let _encoding = Encoding::start();
        let blend = blend.into();
        if let Some(capture) = self.capture.as_mut() {
            capture.scene.push_layer(fill, blend, alpha, transform, clip, filter, backdrop_filter);
            capture.depth += 1;
            return;
        }
        // Drawn without the effect, and logged.
        if blend.mix != Mix::Normal || blend.compose != Compose::SrcOver {
            left_out("a blend other than normal");
        }
        if backdrop_filter.is_some() {
            left_out("a backdrop filter");
        }
        let boxed = Self::shape_box(clip).filter(|_| straight(transform));
        let Some(shape) = boxed.filter(|_| filter.is_none()) else {
            self.capture(transform, clip, |scene| scene.push_layer(fill, BlendMode::default(), alpha, transform, clip, filter, None));
            return;
        };
        if alpha >= 1.0 {
            let before = self.clip;
            self.clip_to(transform, &shape);
            self.pushed.push(Pushed::Clip(before));
            return;
        }
        let before = self.clip;
        self.clip_to(transform, &shape);
        let layer = self.recording.layers.len();
        self.recording.layers.push(Layer::default());
        self.open.push(Vec::new());
        frame_times::add(|t| t.layers += 1);
        self.pushed.push(Pushed::Layer { clip: before, layer, alpha });
    }

    fn push_clip_layer(&mut self, fill: Fill, transform: Affine, clip: &impl Shape) {
        let _encoding = Encoding::start();
        if let Some(capture) = self.capture.as_mut() {
            capture.scene.push_clip_layer(fill, transform, clip);
            capture.depth += 1;
            return;
        }
        match Self::shape_box(clip).filter(|_| straight(transform)) {
            Some(shape) => {
                let before = self.clip;
                self.clip_to(transform, &shape);
                self.pushed.push(Pushed::Clip(before));
            }
            None => self.capture(transform, clip, |scene| scene.push_clip_layer(fill, transform, clip)),
        }
    }

    fn push_mask_layer(&mut self, fill: Fill, mode: MaskMode, transform: Affine, clip: &impl Shape) {
        let _encoding = Encoding::start();
        if let Some(capture) = self.capture.as_mut() {
            capture.scene.push_mask_layer(fill, mode, transform, clip);
            capture.depth += 1;
            return;
        }
        // A mask applies to what was drawn before it, in the layer under it, which has already gone
        // to the GPU: drawn as a clip, and logged.
        left_out("a mask");
        self.push_clip(transform, clip);
    }

    fn pop_layer(&mut self) {
        let _encoding = Encoding::start();
        if let Some(capture) = self.capture.as_mut() {
            capture.scene.pop_layer();
            capture.depth -= 1;
            if capture.depth == 0 {
                let capture = self.capture.take().expect("open above");
                frame_times::add(|t| t.cpu_layers += 1);
                self.draw_on_cpu(&capture.scene, capture.bounds);
            }
            return;
        }
        match self.pushed.pop() {
            Some(Pushed::Clip(before)) => self.clip = before,
            Some(Pushed::Layer { clip: before, layer, alpha }) => {
                let draws = self.open.pop().unwrap_or_default();
                // The layer's texture is the device's size; its sprite covers what its clip lets
                // through, under the clip it was pushed in.
                let inner = self.clip;
                self.clip = before;
                self.recording.layers[layer].draws = draws;
                let (w, h) = self.size;
                let x0 = inner.rect[0].max(0.0);
                let y0 = inner.rect[1].max(0.0);
                let x1 = inner.rect[2].min(w);
                let y1 = inner.rect[3].min(h);
                if x1 <= x0 || y1 <= y0 {
                    return;
                }
                let sprite = self.recording.sprites.len() as u32;
                self.recording.sprites.push(Sprite { bounds: [x0, y0, x1 - x0, y1 - y0], clip: before, params: [alpha, 0.0, 0.0, 0.0] });
                self.draws().push(Draw::Layer { layer, sprite });
            }
            None => {}
        }
    }

    fn stroke<'a>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        paint: impl Into<PaintRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
        anti_aliasing: AntiAliasing,
    ) {
        let _encoding = Encoding::start();
        let paint = paint.into();
        captured!(self, scene => scene.stroke(style, transform, paint, brush_transform, shape, anti_aliasing));
        let boxed = Self::shape_box(shape).filter(|_| straight(transform) && style.dash_pattern.is_empty());
        let (Paint::Solid(color), Some(boxed)) = (&paint, boxed) else {
            // A stroke reaches half its width out, more at a mitre: room for both.
            let s = transform.as_coeffs().iter().take(4).fold(0.0f64, |m, c| m.max(c.abs()));
            let reach = style.width * s * style.miter_limit.max(1.0) + 2.0;
            let bounds = transform.transform_rect_bbox(shape.bounding_box()).inflate(reach, reach);
            self.on_cpu(bounds, |scene| scene.stroke(style, transform, paint, brush_transform, shape, anti_aliasing));
            return;
        };
        let color = *color;
        // A stroke straddles the edge: a border of its width around the shape grown by half.
        let half = style.width / 2.0;
        let outer = boxed.spread(half);
        let Some((bounds, radii)) = self.device_box(transform, &outer) else { return };
        let (sx, sy, _, _) = axis_aligned(transform).unwrap_or((1.0, 1.0, 0.0, 0.0));
        let (wx, wy) = ((style.width * sx) as f32, (style.width * sy) as f32);
        let clip = self.clip;
        self.add_quad(Quad { bounds, clip, background: [0.0; 4], border_color: rgba(color), radii, borders: [wy, wx, wy, wx] });
    }

    fn fill<'a>(
        &mut self,
        style: Fill,
        transform: Affine,
        paint: impl Into<PaintRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
        anti_aliasing: AntiAliasing,
    ) {
        let _encoding = Encoding::start();
        let paint = paint.into();
        captured!(self, scene => scene.fill(style, transform, paint, brush_transform, shape, anti_aliasing));
        match (&paint, Self::shape_box(shape).filter(|_| straight(transform))) {
            (Paint::Solid(color), Some(boxed)) => {
                let color = *color;
                self.fill_box(transform, color, &boxed);
            }
            (Paint::Custom(_), _) => left_out("a custom paint"),
            _ => {
                let bounds = transform.transform_rect_bbox(shape.bounding_box());
                self.on_cpu(bounds, |scene| scene.fill(style, transform, paint, brush_transform, shape, anti_aliasing));
            }
        }
    }

    fn draw_glyphs<'a, 's: 'a>(
        &'a mut self,
        font: &'a FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [NormalizedCoord],
        embolden: kurbo::Vec2,
        style: impl Into<StyleRef<'a>>,
        paint: impl Into<PaintRef<'a>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = anyrender::Glyph> + Clone,
        _anti_aliasing: AntiAliasing,
    ) {
        let _encoding = Encoding::start();
        let style = style.into();
        let paint = paint.into();
        captured!(self, scene => scene.draw_glyphs(font, font_size, hint, normalized_coords, embolden, style, paint, brush_alpha, transform, glyph_transform, glyphs, _anti_aliasing));
        let count = glyphs.clone().count() as u32;
        frame_times::add(|t| {
            t.glyph_runs += 1;
            t.glyphs += count;
        });
        if !matches!(style, StyleRef::Fill(_)) {
            left_out("stroked glyphs");
            return;
        }
        let Paint::Solid(color) = paint else {
            left_out("glyphs not of one colour");
            return;
        };
        let Some((sx, sy, tx, ty)) = axis_aligned(transform) else { return };
        if (sx - sy).abs() > 1e-6 {
            left_out("glyphs scaled unevenly");
            return;
        }
        let mut color = rgba(color);
        color[3] *= brush_alpha;
        let clip = self.clip;
        let skew = glyph_transform.filter(|g| *g != Affine::IDENTITY);
        for glyph in glyphs {
            // The pen in device pixels: a whole pixel down, a quarter pixel across, as GPUI.
            let x = glyph.x as f64 * sx + tx;
            let y = (glyph.y as f64 * sy + ty).round();
            let mut whole = x.floor();
            let mut subpixel = ((x - whole) * SUBPIXELS).round();
            if subpixel >= SUBPIXELS {
                whole += 1.0;
                subpixel = 0.0;
            }
            let ask = GlyphAsk {
                font,
                glyph: glyph.id,
                size: (font_size as f64 * sx) as f32,
                coords: normalized_coords,
                embolden: embolden * sx,
                skew,
                hint,
                subpixel: subpixel as u8,
            };
            let Some(entry) = self.recording.atlas.glyph(&ask) else { continue };
            let bounds = [whole as f32 + entry.offset[0], y as f32 + entry.offset[1], entry.uv[2], entry.uv[3]];
            let colored = entry.page != Page::Mono;
            let tint = if colored { [1.0, 1.0, 1.0, color[3]] } else { color };
            self.add_sprite(AtlasSprite { bounds, clip, uv: entry.uv, color: tint, params: [colored as u32 as f32, 0.0, 0.0, 0.0] }, entry.page);
        }
    }

    fn draw_box_shadow(
        &mut self,
        transform: Affine,
        box_shape: &NonUniformRoundedRect,
        offset: kurbo::Vec2,
        spread: f64,
        std_dev: f64,
        color: Color,
        kind: BoxShadowKind,
    ) {
        let _encoding = Encoding::start();
        captured!(self, scene => scene.draw_box_shadow(transform, box_shape, offset, spread, std_dev, color, kind));
        let geometry = anyrender::BoxShadowGeometry::new(box_shape, offset, spread, std_dev, kind);
        let Some((shape, shape_radii)) = self.device_box(transform, &geometry.shadow) else { return };
        let Some((cast, cast_radii)) = self.device_box(transform, box_shape) else { return };
        let (sx, sy, _, _) = axis_aligned(transform).unwrap_or((1.0, 1.0, 0.0, 0.0));
        let sigma = (geometry.std_dev * (sx + sy) / 2.0) as f32;
        let bounds = match kind {
            BoxShadowKind::Inset => cast,
            _ => {
                let grow = 3.0 * sigma;
                [shape[0] - grow, shape[1] - grow, shape[2] + 2.0 * grow, shape[3] + 2.0 * grow]
            }
        };
        let kind = match kind {
            BoxShadowKind::Outset => 0.0,
            BoxShadowKind::OutsetUnclipped => 1.0,
            BoxShadowKind::Inset => 2.0,
        };
        let clip = self.clip;
        self.add_shadow(Shadow { bounds, clip, shape, shape_radii, cast, cast_radii, color: rgba(color), params: [sigma, kind, 0.0, 0.0] });
    }

    fn fill_box(&mut self, transform: Affine, color: Color, shape: &NonUniformRoundedRect) {
        let _encoding = Encoding::start();
        captured!(self, scene => scene.fill_box(transform, color, shape));
        if !straight(transform) {
            let bounds = transform.transform_rect_bbox(shape.rect);
            self.on_cpu(bounds, |scene| scene.fill_box(transform, color, shape));
            return;
        }
        let Some((bounds, radii)) = self.device_box(transform, shape) else { return };
        let clip = self.clip;
        self.add_quad(Quad { bounds, clip, background: rgba(color), border_color: [0.0; 4], radii, borders: [0.0; 4] });
    }

    fn draw_border(
        &mut self,
        transform: Affine,
        color: Color,
        border_box: &NonUniformRoundedRect,
        widths: Insets,
        paths: impl FnOnce(&mut Self),
    ) where
        Self: Sized,
    {
        let _encoding = Encoding::start();
        // Inside a recording, or turned: its paths, which go where any fill goes.
        if self.capture.is_some() || !straight(transform) {
            paths(self);
            return;
        }
        let Some((bounds, radii)) = self.device_box(transform, border_box) else { return };
        let (sx, sy, _, _) = axis_aligned(transform).unwrap_or((1.0, 1.0, 0.0, 0.0));
        let borders = [(widths.y0 * sy) as f32, (widths.x1 * sx) as f32, (widths.y1 * sy) as f32, (widths.x0 * sx) as f32];
        let clip = self.clip;
        self.add_quad(Quad { bounds, clip, background: [0.0; 4], border_color: rgba(color), radii, borders });
    }

    fn push_box_clip(&mut self, transform: Affine, shape: &NonUniformRoundedRect) {
        let _encoding = Encoding::start();
        if self.capture.is_some() || !straight(transform) {
            self.push_clip_layer(Fill::NonZero, transform, &shape.to_path(0.1));
            return;
        }
        let before = self.clip;
        self.clip_to(transform, shape);
        self.pushed.push(Pushed::Clip(before));
    }

    fn draw_image(&mut self, image: peniko::ImageBrushRef, transform: Affine) {
        let _encoding = Encoding::start();
        captured!(self, scene => scene.draw_image(image, transform));
        if !straight(transform) {
            let bounds = transform.transform_rect_bbox(Rect::new(0.0, 0.0, image.image.width as f64, image.image.height as f64));
            self.on_cpu(bounds, |scene| scene.draw_image(image, transform));
            return;
        }
        let Some((sx, sy, tx, ty)) = axis_aligned(transform) else { return };
        let Some(entry) = self.recording.atlas.image(image.image) else { return };
        let bounds = [tx as f32, ty as f32, (image.image.width as f64 * sx) as f32, (image.image.height as f64 * sy) as f32];
        let clip = self.clip;
        let alpha = image.sampler.alpha;
        self.add_sprite(AtlasSprite { bounds, clip, uv: entry.uv, color: [1.0, 1.0, 1.0, alpha], params: [1.0, 0.0, 0.0, 0.0] }, entry.page);
    }
}
