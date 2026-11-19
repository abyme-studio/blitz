// Quads, shadows and layer sprites, as GPUI draws them.
//
// Ported from Zed's GPUI, crates/gpui_wgpu/src/shaders.wgsl (quad_sdf, fs_quad, fs_shadow and
// their helpers), Copyright 2022 - 2025 Zed Industries, Inc., licensed under the Apache License,
// Version 2.0 (http://www.apache.org/licenses/LICENSE-2.0). Changed: solid colours only, no
// dashed borders, colours straight sRGB in and premultiplied out, a rounded clip beside GPUI's
// rect one, an outset shadow cut out of the box that casts it, and sprites that are a layer's
// texture at the device's size.

struct Globals {
    viewport_size: vec2<f32>,
    pad: vec2<f32>,
}

struct Clip {
    // x0, y0, x1, y1
    rect: vec4<f32>,
    // x, y, width, height of a rounded box, its radii all 0 when there is none
    round: vec4<f32>,
    round_radii: vec4<f32>,
}

struct Quad {
    bounds: vec4<f32>,
    clip: Clip,
    background: vec4<f32>,
    border_color: vec4<f32>,
    // top left, top right, bottom right, bottom left
    radii: vec4<f32>,
    // top, right, bottom, left
    borders: vec4<f32>,
}

struct Shadow {
    bounds: vec4<f32>,
    clip: Clip,
    shape: vec4<f32>,
    shape_radii: vec4<f32>,
    cast_box: vec4<f32>,
    cast_radii: vec4<f32>,
    color: vec4<f32>,
    // sigma, kind (0 outset, 1 outset unclipped, 2 inset)
    params: vec4<f32>,
}

struct Sprite {
    bounds: vec4<f32>,
    clip: Clip,
    // alpha
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> quads: array<Quad>;
@group(0) @binding(2) var<storage, read> shadows: array<Shadow>;
@group(0) @binding(3) var<storage, read> sprites: array<Sprite>;
@group(1) @binding(0) var layer: texture_2d<f32>;

const M_PI_F: f32 = 3.1415926;

fn unit_vertex(vertex_id: u32) -> vec2<f32> {
    return vec2<f32>(f32(vertex_id & 1u), 0.5 * f32(vertex_id & 2u));
}

fn to_device_position(unit: vec2<f32>, bounds: vec4<f32>) -> vec4<f32> {
    let position = unit * bounds.zw + bounds.xy;
    let device = position / globals.viewport_size * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    return vec4<f32>(device, 0.0, 1.0);
}

fn pick_corner_radius(center_to_point: vec2<f32>, radii: vec4<f32>) -> f32 {
    if (center_to_point.x < 0.0) {
        if (center_to_point.y < 0.0) {
            return radii.x;
        } else {
            return radii.w;
        }
    } else {
        if (center_to_point.y < 0.0) {
            return radii.y;
        } else {
            return radii.z;
        }
    }
}

fn quad_sdf_impl(corner_center_to_point: vec2<f32>, corner_radius: f32) -> f32 {
    if (corner_radius == 0.0) {
        return max(corner_center_to_point.x, corner_center_to_point.y);
    } else {
        let signed_distance_to_inset_quad =
            length(max(vec2<f32>(0.0), corner_center_to_point)) +
            min(0.0, max(corner_center_to_point.x, corner_center_to_point.y));
        return signed_distance_to_inset_quad - corner_radius;
    }
}

// Signed distance to a rounded box: positive outside, negative inside.
fn quad_sdf(point: vec2<f32>, bounds: vec4<f32>, radii: vec4<f32>) -> f32 {
    let half_size = bounds.zw / 2.0;
    let center = bounds.xy + half_size;
    let center_to_point = point - center;
    let corner_radius = pick_corner_radius(center_to_point, radii);
    let corner_to_point = abs(center_to_point) - half_size;
    return quad_sdf_impl(corner_to_point + corner_radius, corner_radius);
}

fn quarter_ellipse_sdf(point: vec2<f32>, radii: vec2<f32>) -> f32 {
    let circle_vec = point / radii;
    let unit_circle_sdf = length(circle_vec) - 1.0;
    return unit_circle_sdf * (radii.x + radii.y) * -0.5;
}

fn over(below: vec4<f32>, above: vec4<f32>) -> vec4<f32> {
    let alpha = above.a + below.a * (1.0 - above.a);
    if (alpha == 0.0) {
        return vec4<f32>(0.0);
    }
    let color = (above.rgb * above.a + below.rgb * below.a * (1.0 - above.a)) / alpha;
    return vec4<f32>(color, alpha);
}

fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * M_PI_F) * sigma);
}

fn erf(v: vec2<f32>) -> vec2<f32> {
    let s = sign(v);
    let a = abs(v);
    let r1 = 1.0 + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
    let r2 = r1 * r1;
    return s - s / (r2 * r2);
}

fn blur_along_x(x: f32, y: f32, sigma: f32, corner: f32, half_size: vec2<f32>) -> f32 {
    let delta = min(half_size.y - corner - abs(y), 0.0);
    let curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * erf((x + vec2<f32>(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

// How much of the pixel at `point` the clip lets through: 0 outside its rect, and the rounded
// box's antialiased edge.
fn clip_coverage(point: vec2<f32>, clip: Clip) -> f32 {
    if (point.x < clip.rect.x || point.y < clip.rect.y || point.x >= clip.rect.z || point.y >= clip.rect.w) {
        return 0.0;
    }
    if (all(clip.round_radii == vec4<f32>(0.0))) {
        return 1.0;
    }
    return saturate(0.5 - quad_sdf(point, clip.round, clip.round_radii));
}

// Straight colour at `coverage`, premultiplied for the blend.
fn premultiplied(color: vec4<f32>, coverage: f32) -> vec4<f32> {
    let alpha = color.a * coverage;
    return vec4<f32>(color.rgb * alpha, alpha);
}

// --- quads --- //

struct QuadVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) quad_id: u32,
}

@vertex
fn vs_quad(@builtin(vertex_index) vertex_id: u32, @builtin(instance_index) instance_id: u32) -> QuadVarying {
    let quad = quads[instance_id];
    var out = QuadVarying();
    out.position = to_device_position(unit_vertex(vertex_id), quad.bounds);
    out.quad_id = instance_id;
    return out;
}

@fragment
fn fs_quad(input: QuadVarying) -> @location(0) vec4<f32> {
    let quad = quads[input.quad_id];
    let clip = clip_coverage(input.position.xy, quad.clip);
    if (clip == 0.0) {
        discard;
    }
    let background_color = quad.background;

    let unrounded = all(quad.radii == vec4<f32>(0.0));
    if (all(quad.borders == vec4<f32>(0.0)) && unrounded) {
        return premultiplied(background_color, clip);
    }

    let size = quad.bounds.zw;
    let half_size = size / 2.0;
    let point = input.position.xy - quad.bounds.xy;
    let center_to_point = point - half_size;
    let antialias_threshold = 0.5;
    let corner_radius = pick_corner_radius(center_to_point, quad.radii);

    // The widths of the nearest borders.
    let border = vec2<f32>(
        select(quad.borders.y, quad.borders.w, center_to_point.x < 0.0),
        select(quad.borders.z, quad.borders.x, center_to_point.y < 0.0));
    let reduced_border = vec2<f32>(
        select(border.x, -antialias_threshold, border.x == 0.0),
        select(border.y, -antialias_threshold, border.y == 0.0));

    let corner_to_point = abs(center_to_point) - half_size;
    let corner_center_to_point = corner_to_point + corner_radius;
    let is_near_rounded_corner = corner_center_to_point.x >= 0.0 && corner_center_to_point.y >= 0.0;
    let straight_border_inner_corner_to_point = corner_to_point + reduced_border;
    let is_beyond_inner_straight_border =
        straight_border_inner_corner_to_point.x > 0.0 || straight_border_inner_corner_to_point.y > 0.0;
    let is_within_inner_straight_border =
        straight_border_inner_corner_to_point.x < -antialias_threshold &&
        straight_border_inner_corner_to_point.y < -antialias_threshold;

    if (is_within_inner_straight_border && !is_near_rounded_corner) {
        return premultiplied(background_color, clip);
    }

    let outer_sdf = quad_sdf_impl(corner_center_to_point, corner_radius);

    var inner_sdf = 0.0;
    if (corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0) {
        inner_sdf = -max(straight_border_inner_corner_to_point.x, straight_border_inner_corner_to_point.y);
    } else if (is_beyond_inner_straight_border) {
        inner_sdf = -1.0;
    } else if (reduced_border.x == reduced_border.y) {
        inner_sdf = -(outer_sdf + reduced_border.x);
    } else {
        let ellipse_radii = max(vec2<f32>(0.0), corner_radius - reduced_border);
        inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
    }

    let border_sdf = max(inner_sdf, outer_sdf);
    var color = background_color;
    if (border_sdf < antialias_threshold) {
        let blended_border = over(background_color, quad.border_color);
        color = mix(background_color, blended_border, saturate(antialias_threshold - inner_sdf));
    }
    return premultiplied(color, clip * saturate(antialias_threshold - outer_sdf));
}

// --- shadows --- //

struct ShadowVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) shadow_id: u32,
}

@vertex
fn vs_shadow(@builtin(vertex_index) vertex_id: u32, @builtin(instance_index) instance_id: u32) -> ShadowVarying {
    let shadow = shadows[instance_id];
    var out = ShadowVarying();
    out.position = to_device_position(unit_vertex(vertex_id), shadow.bounds);
    out.shadow_id = instance_id;
    return out;
}

@fragment
fn fs_shadow(input: ShadowVarying) -> @location(0) vec4<f32> {
    let shadow = shadows[input.shadow_id];
    let clip = clip_coverage(input.position.xy, shadow.clip);
    if (clip == 0.0) {
        discard;
    }
    let sigma = shadow.params.x;
    let kind = u32(shadow.params.y);
    let half_size = shadow.shape.zw / 2.0;
    let center = shadow.shape.xy + half_size;
    let center_to_point = input.position.xy - center;
    let corner_radius = pick_corner_radius(center_to_point, shadow.shape_radii);

    var alpha: f32;
    if (sigma == 0.0) {
        alpha = saturate(0.5 - quad_sdf(input.position.xy, shadow.shape, shadow.shape_radii));
    } else {
        let low = center_to_point.y - half_size.y;
        let high = center_to_point.y + half_size.y;
        let start = clamp(-3.0 * sigma, low, high);
        let end = clamp(3.0 * sigma, low, high);
        let step = (end - start) / 4.0;
        var y = start + step * 0.5;
        alpha = 0.0;
        for (var i = 0; i < 4; i += 1) {
            let blur = blur_along_x(center_to_point.x, center_to_point.y - y, sigma, corner_radius, half_size);
            alpha += blur * gaussian(y, sigma) * step;
            y += step;
        }
    }

    let cast_distance = quad_sdf(input.position.xy, shadow.cast_box, shadow.cast_radii);
    if (kind == 2u) {
        // Inset: the complement of the blurred hole, inside the box.
        alpha = (1.0 - alpha) * saturate(0.5 - cast_distance);
    } else if (kind == 0u) {
        // Outset: never under the box that casts it.
        alpha *= saturate(0.5 + cast_distance);
    }
    return premultiplied(shadow.color, clip * alpha);
}

// --- layer sprites --- //

struct SpriteVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) sprite_id: u32,
}

@vertex
fn vs_sprite(@builtin(vertex_index) vertex_id: u32, @builtin(instance_index) instance_id: u32) -> SpriteVarying {
    let sprite = sprites[instance_id];
    var out = SpriteVarying();
    out.position = to_device_position(unit_vertex(vertex_id), sprite.bounds);
    out.sprite_id = instance_id;
    return out;
}

@fragment
fn fs_sprite(input: SpriteVarying) -> @location(0) vec4<f32> {
    let sprite = sprites[input.sprite_id];
    let clip = clip_coverage(input.position.xy, sprite.clip);
    if (clip == 0.0) {
        discard;
    }
    // The layer's texture is premultiplied, at the device's size: one texel per pixel.
    let texel = textureLoad(layer, vec2<i32>(input.position.xy), 0);
    return texel * (sprite.params.x * clip);
}
