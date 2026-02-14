struct CompositeUniform {
    rect_pos_size: vec4<f32>,
    screen_size_opacity: vec4<f32>,
    corner_radii: vec4<f32>, // top_left, top_right, bottom_right, bottom_left
}

@group(0) @binding(0)
var layer_tex: texture_2d<f32>;

@group(0) @binding(1)
var layer_sampler: sampler;

@group(0) @binding(2)
var<uniform> uni: CompositeUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) screen_uv: vec2<f32>,
    @location(1) local_uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var quad = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );

    let rect_pos = uni.rect_pos_size.xy;
    let rect_size = uni.rect_pos_size.zw;
    let screen_size = uni.screen_size_opacity.xy;

    let local = quad[vertex_index];
    let pixel = rect_pos + local * rect_size;

    let ndc_x = (pixel.x / screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel.y / screen_size.y) * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.screen_uv = pixel / screen_size;
    out.local_uv = local;
    return out;
}

fn sd_round_rect_4(p: vec2<f32>, half: vec2<f32>, radii: vec4<f32>) -> f32 {
    var r = radii.x; // top_left
    if (p.x >= 0.0 && p.y < 0.0) {
        r = radii.y; // top_right
    } else if (p.x >= 0.0 && p.y >= 0.0) {
        r = radii.z; // bottom_right
    } else if (p.x < 0.0 && p.y >= 0.0) {
        r = radii.w; // bottom_left
    }
    let q = abs(p) - half + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn coverage_from_distance(d: f32) -> f32 {
    let aa = max(fwidth(d), 0.0001);
    return clamp(1.0 - smoothstep(-aa, aa, d), 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let rect_size = uni.rect_pos_size.zw;
    let opacity = clamp(uni.screen_size_opacity.z, 0.0, 1.0);

    let sample_color = textureSample(layer_tex, layer_sampler, in.screen_uv);
    let sample_alpha = clamp(sample_color.a, 0.0, 1.0);
    let sample_rgb = select(
        vec3<f32>(0.0, 0.0, 0.0),
        sample_color.rgb / sample_alpha,
        sample_alpha > 0.0001
    );

    var mask = 1.0;
    let half = rect_size * 0.5;
    let p = in.local_uv * rect_size - half;
    let r_tl = clamp(uni.corner_radii.x, 0.0, min(half.x, half.y));
    let r_tr = clamp(uni.corner_radii.y, 0.0, min(half.x, half.y));
    let r_br = clamp(uni.corner_radii.z, 0.0, min(half.x, half.y));
    let r_bl = clamp(uni.corner_radii.w, 0.0, min(half.x, half.y));
    let d = sd_round_rect_4(p, half, vec4<f32>(r_tl, r_tr, r_br, r_bl));
    mask = coverage_from_distance(d);

    let out_alpha = sample_alpha * mask * opacity;
    return vec4<f32>(sample_rgb, out_alpha);
}
