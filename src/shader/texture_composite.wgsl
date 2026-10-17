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
    // knockout left, top, right, bottom from the layer origin; empty for none
    nine_patch_knockout_rect: vec4<f32>,
    // knockout top-left, top-right, bottom-right, bottom-left radii
    nine_patch_knockout_radii: vec4<f32>,
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
    @location(3) clip_w: f32,
) -> VertexOutput {
    var out: VertexOutput;
    // Lifting by w leaves the projected position as is, but makes the
    // rasterizer interpolate the coordinates below perspective-correctly.
    out.position = vec4<f32>(position * clip_w, 0.0, clip_w);
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

// Signed distance from `p` to a rounded rect given by its corners. As a
// box's edge is drawn: the distance to the rect, or within a corner's
// radius of both its sides, to that corner's arc. A radius may exceed half
// the box, as long as two on one side fit it.
fn sd_rounded_rect(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let half = (rect.zw - rect.xy) * 0.5;
    let d = abs(p - (rect.xy + half)) - half;
    let tl = rect.xy + vec2<f32>(radii.x, radii.x);
    if radii.x > 0.0 && p.x < tl.x && p.y < tl.y {
        return length(p - tl) - radii.x;
    }
    let tr = vec2<f32>(rect.z - radii.y, rect.y + radii.y);
    if radii.y > 0.0 && p.x > tr.x && p.y < tr.y {
        return length(p - tr) - radii.y;
    }
    let br = rect.zw - vec2<f32>(radii.z, radii.z);
    if radii.z > 0.0 && p.x > br.x && p.y > br.y {
        return length(p - br) - radii.z;
    }
    let bl = vec2<f32>(rect.x + radii.w, rect.w - radii.w);
    if radii.w > 0.0 && p.x < bl.x && p.y > bl.y {
        return length(p - bl) - radii.w;
    }
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
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
    let knockout = composite.nine_patch_knockout_rect;
    if knockout.z > knockout.x && knockout.w > knockout.y {
        // Antialiased over a pixel on either side, as a box's edge is.
        let d = sd_rounded_rect(
            position - composite.nine_patch_origin_split.xy,
            knockout,
            composite.nine_patch_knockout_radii,
        );
        coverage = coverage * (1.0 - smoothstep(1.0, -1.0, d));
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
