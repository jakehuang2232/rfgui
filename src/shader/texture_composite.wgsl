@group(0) @binding(0)
var source_tex: texture_2d<f32>;

@group(0) @binding(1)
var mask_tex: texture_2d<f32>;

@group(0) @binding(2)
var tex_sampler: sampler;

struct CompositeParams {
    // use_mask, source_is_premultiplied, opacity, nine_patch
    data: vec4<f32>,
    // layer origin (target pixels), split
    nine_patch_origin_split: vec4<f32>,
    // stretch, mask split
    nine_patch_stretch_mask_split: vec4<f32>,
    // mask stretch, mask offset
    nine_patch_mask_stretch_offset: vec4<f32>,
    tint: vec4<f32>,
}

@group(0) @binding(3)
var<uniform> composite: CompositeParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) source_uv: vec2<f32>,
    @location(1) mask_uv: vec2<f32>,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) source_uv: vec2<f32>,
    @location(2) mask_uv: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.source_uv = source_uv;
    out.mask_uv = mask_uv;
    return out;
}

// Layer pixel `p` reads `p` below `split`, `split` for `stretch` pixels,
// then `p - stretch`.
fn nine_patch_texel(p: vec2<f32>, split: vec2<f32>, stretch: vec2<f32>) -> vec2<f32> {
    let middle = select(p, split, p >= split);
    return select(middle, p - stretch, p >= split + stretch);
}

fn nine_patch_coverage(
    tex: texture_2d<f32>,
    p: vec2<f32>,
    split: vec2<f32>,
    stretch: vec2<f32>,
) -> f32 {
    let size = vec2<f32>(textureDimensions(tex));
    if any(p < vec2<f32>(0.0)) || any(p >= size + stretch) {
        return 0.0;
    }
    let texel = clamp(nine_patch_texel(p, split, stretch), vec2<f32>(0.0), size - 1.0);
    return textureLoad(tex, vec2<i32>(texel), 0).a;
}

fn nine_patch_fragment(position: vec2<f32>) -> vec4<f32> {
    let p = floor(position - composite.nine_patch_origin_split.xy);
    var coverage = nine_patch_coverage(
        source_tex,
        p,
        composite.nine_patch_origin_split.zw,
        composite.nine_patch_stretch_mask_split.xy,
    );
    if composite.data.x > 0.5 {
        coverage = coverage * nine_patch_coverage(
            mask_tex,
            p - composite.nine_patch_mask_stretch_offset.zw,
            composite.nine_patch_stretch_mask_split.zw,
            composite.nine_patch_mask_stretch_offset.xy,
        );
    }
    return composite.tint * (coverage * clamp(composite.data.z, 0.0, 1.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if composite.data.w > 0.5 {
        return nine_patch_fragment(in.position.xy);
    }
    let color = textureSample(source_tex, tex_sampler, in.source_uv);
    var factor = 1.0;
    if composite.data.x > 0.5 {
        factor = factor * textureSample(mask_tex, tex_sampler, in.mask_uv).a;
    }
    let source_is_premultiplied = composite.data.y > 0.5;
    factor = factor * clamp(composite.data.z, 0.0, 1.0);
    let alpha = color.a * factor;
    if source_is_premultiplied {
        return vec4<f32>(color.rgb * factor, alpha);
    }
    return vec4<f32>(color.rgb * alpha, alpha);
}
