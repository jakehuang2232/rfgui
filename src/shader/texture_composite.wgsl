@group(0) @binding(0)
var source_tex: texture_2d<f32>;

@group(0) @binding(1)
var mask_tex: texture_2d<f32>;

@group(0) @binding(2)
var tex_sampler: sampler;

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
    let color = textureSample(source_tex, tex_sampler, in.uv);
    var alpha = color.a;
    if composite.data.x > 0.5 {
        alpha = alpha * textureSample(mask_tex, tex_sampler, in.uv).a;
    }
    alpha = alpha * clamp(composite.data.y, 0.0, 1.0);
    return vec4<f32>(color.rgb * alpha, alpha);
}
