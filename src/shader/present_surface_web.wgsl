@group(0) @binding(0)
var src_tex: texture_2d<f32>;

@group(0) @binding(1)
var src_sampler: sampler;

struct PresentSurfaceUniform {
    uv_offset: vec2<f32>,
    uv_scale: vec2<f32>,
}

@group(0) @binding(2)
var<uniform> params: PresentSurfaceUniform;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    let pos = positions[vertex_index];
    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = pos * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = params.uv_offset + in.uv * params.uv_scale;
    let c = textureSample(src_tex, src_sampler, uv);
    if c.a <= 0.000001 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let straight_rgb = c.rgb / c.a;
    return vec4<f32>(straight_rgb, c.a);
}
