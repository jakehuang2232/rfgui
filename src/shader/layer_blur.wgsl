@group(0) @binding(0)
var layer_tex: texture_2d<f32>;

@group(0) @binding(1)
var layer_sampler: sampler;

struct BlurParams {
    texel_size: vec2<f32>,
    radius: f32,
    _pad: f32,
}

@group(0) @binding(2)
var<uniform> blur: BlurParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn sample_blur(uv: vec2<f32>) -> vec4<f32> {
    if blur.radius <= 0.001 {
        return textureSample(layer_tex, layer_sampler, uv);
    }
    let delta = blur.texel_size * blur.radius;
    var sum = vec4<f32>(0.0);
    sum += textureSample(layer_tex, layer_sampler, uv + vec2<f32>(-delta.x, -delta.y));
    sum += textureSample(layer_tex, layer_sampler, uv + vec2<f32>(0.0, -delta.y));
    sum += textureSample(layer_tex, layer_sampler, uv + vec2<f32>(delta.x, -delta.y));
    sum += textureSample(layer_tex, layer_sampler, uv + vec2<f32>(-delta.x, 0.0));
    sum += textureSample(layer_tex, layer_sampler, uv);
    sum += textureSample(layer_tex, layer_sampler, uv + vec2<f32>(delta.x, 0.0));
    sum += textureSample(layer_tex, layer_sampler, uv + vec2<f32>(-delta.x, delta.y));
    sum += textureSample(layer_tex, layer_sampler, uv + vec2<f32>(0.0, delta.y));
    sum += textureSample(layer_tex, layer_sampler, uv + vec2<f32>(delta.x, delta.y));
    return sum / 9.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return sample_blur(in.uv);
}
