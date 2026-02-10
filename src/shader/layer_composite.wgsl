struct CompositeUniform {
    rect_pos_size: vec4<f32>,
    screen_radius_opacity: vec4<f32>,
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
    let screen_size = uni.screen_radius_opacity.xy;

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

fn sd_round_rect(p: vec2<f32>, half: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let rect_size = uni.rect_pos_size.zw;
    let radius = uni.screen_radius_opacity.z;
    let opacity = clamp(uni.screen_radius_opacity.w, 0.0, 1.0);

    let sample_color = textureSample(layer_tex, layer_sampler, in.screen_uv);

    var mask = 1.0;
    if (radius > 0.0) {
        let half = rect_size * 0.5;
        let p = in.local_uv * rect_size - half;
        let r = clamp(radius, 0.0, min(half.x, half.y));
        let d = sd_round_rect(p, half, r);
        let aa = fwidth(d);
        mask = clamp(1.0 - smoothstep(0.0, aa, d), 0.0, 1.0);
    }

    return sample_color * (mask * opacity);
}
