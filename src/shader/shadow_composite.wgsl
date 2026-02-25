@group(0) @binding(0)
var shadow_tex: texture_2d<f32>;

@group(0) @binding(1)
var mask_tex: texture_2d<f32>;

@group(0) @binding(2)
var shadow_sampler: sampler;

struct CompositeParams {
    data: vec4<f32>,
}

@group(0) @binding(3)
var<uniform> composite: CompositeParams;

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
    let c = textureSample(shadow_tex, shadow_sampler, in.uv);
    let m = textureSample(mask_tex, shadow_sampler, in.uv).a;
    let mask_alpha = mix(1.0, m, clamp(composite.data.x, 0.0, 1.0));
    return vec4<f32>(c.rgb, c.a * mask_alpha);
}
