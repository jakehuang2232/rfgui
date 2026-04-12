struct ScreenUniform {
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
}

struct FragmentUniform {
    origin: vec2<f32>,
    clip_min: vec2<f32>,
    clip_max: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

@group(0) @binding(1)
var<storage, read> fragments: array<FragmentUniform>;

@group(1) @binding(0)
var glyph_atlas: texture_2d<f32>;

@group(1) @binding(1)
var glyph_sampler: sampler;

struct GlyphInstance {
    @location(0) local_pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) uv_min: vec2<f32>,
    @location(3) uv_max: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) opacity: f32,
    @location(6) fragment_index: u32,
}

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) opacity: f32,
    @location(3) pixel: vec2<f32>,
    @location(4) clip_min: vec2<f32>,
    @location(5) clip_max: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, glyph: GlyphInstance) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );

    let corner = corners[vertex_index];
    let fragment = fragments[glyph.fragment_index];
    let pixel = fragment.origin + glyph.local_pos + corner * glyph.size;
    let ndc_x = (pixel.x / screen.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel.y / screen.screen_size.y) * 2.0;

    var out: VsOut;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = glyph.uv_min + (glyph.uv_max - glyph.uv_min) * corner;
    out.color = glyph.color;
    out.opacity = glyph.opacity;
    out.pixel = pixel;
    out.clip_min = fragment.clip_min;
    out.clip_max = fragment.clip_max;
    return out;
}

@fragment
fn fs_mask(in: VsOut) -> @location(0) vec4<f32> {
    if in.pixel.x < in.clip_min.x || in.pixel.y < in.clip_min.y ||
       in.pixel.x > in.clip_max.x || in.pixel.y > in.clip_max.y {
        discard;
    }
    let coverage = textureSample(glyph_atlas, glyph_sampler, in.uv).r;
    let out_alpha = coverage * in.opacity * in.color.a;
    return vec4<f32>(in.color.rgb * out_alpha, out_alpha);
}

@fragment
fn fs_color(in: VsOut) -> @location(0) vec4<f32> {
    if in.pixel.x < in.clip_min.x || in.pixel.y < in.clip_min.y ||
       in.pixel.x > in.clip_max.x || in.pixel.y > in.clip_max.y {
        discard;
    }
    let texel = textureSample(glyph_atlas, glyph_sampler, in.uv);
    let out_alpha = texel.a * in.opacity * in.color.a;
    return vec4<f32>(texel.rgb * out_alpha, out_alpha);
}
