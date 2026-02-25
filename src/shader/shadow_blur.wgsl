@group(0) @binding(0)
var source_tex: texture_2d<f32>;

@group(0) @binding(1)
var source_sampler: sampler;

struct BlurParams {
    texel_size: vec2<f32>,
    direction: vec2<f32>,
    radius: f32,
    sigma: f32,
    _pad: vec2<f32>,
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if blur.radius <= 0.001 {
        return textureSample(source_tex, source_sampler, in.uv);
    }
    let sigma = max(blur.sigma, 0.001);
    let tap_count = i32(ceil(blur.radius));
    let axis = blur.direction * blur.texel_size;
    var sum = textureSample(source_tex, source_sampler, in.uv);
    var weight_sum = 1.0;
    // Fixed upper bound for WGSL loop; effective taps are bounded by tap_count.
    for (var i = 1; i <= 32; i = i + 1) {
        if i > tap_count {
            continue;
        }
        let x = f32(i);
        let w = exp(-0.5 * (x / sigma) * (x / sigma));
        let offset = axis * x;
        sum += textureSample(source_tex, source_sampler, in.uv + offset) * w;
        sum += textureSample(source_tex, source_sampler, in.uv - offset) * w;
        weight_sum += 2.0 * w;
    }
    return sum / weight_sum;
}
