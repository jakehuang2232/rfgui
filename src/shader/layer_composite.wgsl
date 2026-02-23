@group(0) @binding(0)
var layer_tex: texture_2d<f32>;

@group(0) @binding(1)
var layer_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) screen_uv: vec2<f32>,
    @location(1) alpha: f32,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) screen_uv: vec2<f32>,
    @location(2) alpha: f32,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.screen_uv = screen_uv;
    out.alpha = alpha;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sample_color = textureSample(layer_tex, layer_sampler, in.screen_uv);
    return sample_color * clamp(in.alpha, 0.0, 1.0);
}
