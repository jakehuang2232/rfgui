struct Radii4 {
    rx: vec4<f32>,
    ry: vec4<f32>,
}

fn premul(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn safe_div(a: f32, b: f32, fallback: f32) -> f32 {
    if abs(b) < 1e-6 {
        return fallback;
    }
    return a / b;
}

fn aa_width_px(p: vec2<f32>) -> f32 {
    // Keep AA in pixel space and independent from piecewise distance derivatives.
    let fw = fwidth(p);
    return max(max(fw.x, fw.y), 1e-4);
}

fn sd_axis_aligned_rect(p: vec2<f32>, rect: vec4<f32>) -> f32 {
    let minp = rect.xy;
    let maxp = rect.zw;
    let c = (minp + maxp) * 0.5;
    let h = (maxp - minp) * 0.5;
    let d = abs(p - c) - h;
    let dx = max(d.x, 0.0);
    let dy = max(d.y, 0.0);
    if dx > 0.0 && dy > 0.0 {
        return length(vec2<f32>(dx, dy));
    }
    return max(d.x, d.y);
}

fn sd_ellipse(local_p: vec2<f32>, r: vec2<f32>) -> f32 {
    let rr = max(r, vec2<f32>(1e-4));
    let ap = abs(local_p);
    let k0 = length(ap / rr);
    let k1 = length(ap / (rr * rr));
    if k1 < 1e-6 {
        return length(ap) - max(rr.x, rr.y);
    }
    return (k0 - 1.0) * safe_div(k0, k1, 0.0);
}

fn sd_rrect(p: vec2<f32>, rect: vec4<f32>, radii: Radii4) -> f32 {
    let minp = rect.xy;
    let maxp = rect.zw;
    let w = maxp.x - minp.x;
    let h = maxp.y - minp.y;
    if w <= 0.0 || h <= 0.0 {
        return 1e6;
    }

    var d = sd_axis_aligned_rect(p, rect);

    let in_tl = p.x < (minp.x + radii.rx.x) && p.y < (minp.y + radii.ry.x) && radii.rx.x > 0.0 && radii.ry.x > 0.0;
    let in_tr = p.x > (maxp.x - radii.rx.y) && p.y < (minp.y + radii.ry.y) && radii.rx.y > 0.0 && radii.ry.y > 0.0;
    let in_br = p.x > (maxp.x - radii.rx.z) && p.y > (maxp.y - radii.ry.z) && radii.rx.z > 0.0 && radii.ry.z > 0.0;
    let in_bl = p.x < (minp.x + radii.rx.w) && p.y > (maxp.y - radii.ry.w) && radii.rx.w > 0.0 && radii.ry.w > 0.0;

    if in_tl {
        let c = vec2<f32>(minp.x + radii.rx.x, minp.y + radii.ry.x);
        d = sd_ellipse(p - c, vec2<f32>(radii.rx.x, radii.ry.x));
    } else if in_tr {
        let c = vec2<f32>(maxp.x - radii.rx.y, minp.y + radii.ry.y);
        d = sd_ellipse(p - c, vec2<f32>(radii.rx.y, radii.ry.y));
    } else if in_br {
        let c = vec2<f32>(maxp.x - radii.rx.z, maxp.y - radii.ry.z);
        d = sd_ellipse(p - c, vec2<f32>(radii.rx.z, radii.ry.z));
    } else if in_bl {
        let c = vec2<f32>(minp.x + radii.rx.w, maxp.y - radii.ry.w);
        d = sd_ellipse(p - c, vec2<f32>(radii.rx.w, radii.ry.w));
    }

    return d;
}

fn coverage_rrect(p: vec2<f32>, rect: vec4<f32>, radii: Radii4) -> f32 {
    let d = sd_rrect(p, rect, radii);
    let aa = aa_width_px(p);
    return smoothstep(aa, -aa, d);
}

fn coverage_rect(p: vec2<f32>, rect: vec4<f32>) -> f32 {
    let d = sd_axis_aligned_rect(p, rect);
    let aa = aa_width_px(p);
    return smoothstep(aa, -aa, d);
}

// Per-side border color pick. Branchless min/second-min over 4 edges.
fn pick_border_side(
    p: vec2<f32>,
    outer_rect: vec4<f32>,
    border_widths: vec4<f32>,
    c_l: vec4<f32>,
    c_t: vec4<f32>,
    c_r: vec4<f32>,
    c_b: vec4<f32>,
) -> vec4<f32> {
    let minp = outer_rect.xy;
    let maxp = outer_rect.zw;
    let b = border_widths;

    let dL = max(p.x - minp.x, 0.0);
    let dT = max(p.y - minp.y, 0.0);
    let dR = max(maxp.x - p.x, 0.0);
    let dB = max(maxp.y - p.y, 0.0);

    let inf = 1e6;
    let sL = select(inf, safe_div(dL, b.x, inf), b.x > 1e-6);
    let sT = select(inf, safe_div(dT, b.y, inf), b.y > 1e-6);
    let sR = select(inf, safe_div(dR, b.z, inf), b.z > 1e-6);
    let sB = select(inf, safe_div(dB, b.w, inf), b.w > 1e-6);

    let s = vec4<f32>(sL, sT, sR, sB);
    let c = array<vec4<f32>, 4>(c_l, c_t, c_r, c_b);

    let min_score = min(min(s.x, s.y), min(s.z, s.w));
    let is_min = vec4<f32>(
        1.0 - step(1e-6, abs(s.x - min_score)),
        1.0 - step(1e-6, abs(s.y - min_score)),
        1.0 - step(1e-6, abs(s.z - min_score)),
        1.0 - step(1e-6, abs(s.w - min_score)),
    );
    let min_mask = is_min / max(dot(is_min, vec4<f32>(1.0)), 1.0);
    let c0 = c[0] * min_mask.x + c[1] * min_mask.y + c[2] * min_mask.z + c[3] * min_mask.w;

    let s_no_min = vec4<f32>(
        mix(s.x, inf, is_min.x),
        mix(s.y, inf, is_min.y),
        mix(s.z, inf, is_min.z),
        mix(s.w, inf, is_min.w),
    );
    let second_score = min(min(s_no_min.x, s_no_min.y), min(s_no_min.z, s_no_min.w));
    let is_second = vec4<f32>(
        1.0 - step(1e-6, abs(s_no_min.x - second_score)),
        1.0 - step(1e-6, abs(s_no_min.y - second_score)),
        1.0 - step(1e-6, abs(s_no_min.z - second_score)),
        1.0 - step(1e-6, abs(s_no_min.w - second_score)),
    );
    let second_mask = is_second / max(dot(is_second, vec4<f32>(1.0)), 1.0);
    let c1 = c[0] * second_mask.x + c[1] * second_mask.y + c[2] * second_mask.z + c[3] * second_mask.w;

    let aa = max(max(fwidth(min_score), fwidth(second_score)) * 0.75, 1e-4);
    let t = smoothstep(-aa, aa, second_score - min_score);
    return mix(c1, c0, t);
}


struct RectParams {
    outer_rect: vec4<f32>,    // min_x, min_y, max_x, max_y (pixel space)
    inner_rect: vec4<f32>,
    outer_rx: vec4<f32>,      // TL, TR, BR, BL
    outer_ry: vec4<f32>,
    inner_rx: vec4<f32>,
    inner_ry: vec4<f32>,
    border_widths: vec4<f32>, // left, top, right, bottom
    flags: vec4<f32>,         // x: has_inner, y: depth
    fill_color: vec4<f32>,
    border_left: vec4<f32>,
    border_top: vec4<f32>,
    border_right: vec4<f32>,
    border_bottom: vec4<f32>,
    screen_size: vec4<f32>,
    // x: gradient_kind (1=linear, 2=radial, 3=conic)
    // y: stop_count, z: repeating, w: stops_start_index
    gradient_info: vec4<f32>,
    // Linear: [p0.xy, p1.xy]; Radial: [cx,cy,rx,ry]; Conic: [cx,cy,from_rad,_]
    gradient_axis: vec4<f32>,
    border_gradient_info: vec4<f32>,
    border_gradient_axis: vec4<f32>,
    _pad_tail: array<vec4<f32>, 14>,
}

struct GradientStop {
    color: vec4<f32>,
    pos: vec4<f32>, // x: position (0..1)
}

@group(0) @binding(0)
var<uniform> u: RectParams;

@group(0) @binding(1)
var<storage, read> gradient_stops: array<GradientStop>;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) pixel_pos: vec2<f32>,
}

@vertex
fn vs_main(@location(0) uv: vec2<f32>) -> VertexOut {
    var out: VertexOut;
    let p = mix(u.outer_rect.xy, u.outer_rect.zw, uv);
    let ndc = vec2<f32>(
        (p.x / u.screen_size.x) * 2.0 - 1.0,
        1.0 - (p.y / u.screen_size.y) * 2.0,
    );
#ifdef OPAQUE
    out.position = vec4<f32>(ndc, u.flags.y, 1.0);
#else
    out.position = vec4<f32>(ndc, 0.0, 1.0);
#endif
    out.pixel_pos = p;
    return out;
}

fn cov_outer_of(p: vec2<f32>) -> f32 {
#ifdef ROUNDED
    return coverage_rrect(p, u.outer_rect, Radii4(u.outer_rx, u.outer_ry));
#else
    return coverage_rect(p, u.outer_rect);
#endif
}

fn cov_inner_of(p: vec2<f32>) -> f32 {
#ifdef ROUNDED
    return coverage_rrect(p, u.inner_rect, Radii4(u.inner_rx, u.inner_ry));
#else
    return coverage_rect(p, u.inner_rect);
#endif
}

fn gradient_stop_color(start: u32, i: u32) -> vec4<f32> {
    return gradient_stops[start + i].color;
}

fn gradient_stop_position(start: u32, i: u32) -> f32 {
    return gradient_stops[start + i].pos.x;
}

fn gradient_t_of(info: vec4<f32>, axis: vec4<f32>, p: vec2<f32>) -> f32 {
    let kind = info.x;
    if (kind < 1.5) {
        // linear
        let p0 = axis.xy;
        let p1 = axis.zw;
        let d = p1 - p0;
        let len2 = max(dot(d, d), 1e-6);
        return dot(p - p0, d) / len2;
    } else if (kind < 2.5) {
        // radial
        let c = axis.xy;
        let rxry = max(axis.zw, vec2<f32>(1e-4));
        return length((p - c) / rxry);
    } else {
        // conic
        let c = axis.xy;
        let from_angle = axis.z;
        let v = p - c;
        let a = atan2(v.x, -v.y);
        let TAU = 6.28318530718;
        var t = (a - from_angle) / TAU;
        return t - floor(t);
    }
}

fn sample_gradient(info: vec4<f32>, axis: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    let count_f = max(info.y, 1.0);
    let count = u32(count_f);
    let repeating = info.z > 0.5;
    let start = u32(info.w);
    var t = gradient_t_of(info, axis, p);
    if (repeating) {
        t = t - floor(t);
    } else {
        t = clamp(t, 0.0, 1.0);
    }
    if (count <= 1u) {
        return gradient_stop_color(start, 0u);
    }
    let first_pos = gradient_stop_position(start, 0u);
    if (t <= first_pos) {
        return gradient_stop_color(start, 0u);
    }
    let last_pos = gradient_stop_position(start, count - 1u);
    if (t >= last_pos) {
        return gradient_stop_color(start, count - 1u);
    }
    var result: vec4<f32> = gradient_stop_color(start, count - 1u);
    for (var i: u32 = 0u; i < count - 1u; i = i + 1u) {
        let p0 = gradient_stop_position(start, i);
        let p1 = gradient_stop_position(start, i + 1u);
        if (t >= p0 && t <= p1) {
            let span = max(p1 - p0, 1e-6);
            let k = (t - p0) / span;
            result = mix(
                gradient_stop_color(start, i),
                gradient_stop_color(start, i + 1u),
                k,
            );
            break;
        }
    }
    return result;
}

fn fill_at(p: vec2<f32>) -> vec4<f32> {
#ifdef HAS_GRADIENT
    return sample_gradient(u.gradient_info, u.gradient_axis, p);
#else
    return u.fill_color;
#endif
}

#ifndef BORDER_NONE
fn border_color_of(p: vec2<f32>) -> vec4<f32> {
#ifdef HAS_BORDER_GRADIENT
    return sample_gradient(u.border_gradient_info, u.border_gradient_axis, p);
#else
#ifdef BORDER_UNIFORM
    return u.border_left;
#else
    return pick_border_side(
        p,
        u.outer_rect,
        u.border_widths,
        u.border_left,
        u.border_top,
        u.border_right,
        u.border_bottom,
    );
#endif
#endif
}
#endif

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let p = in.pixel_pos;

#ifdef PASS_FILL_ONLY
    // Fill only: cov_inner area with fill_color.
    let cov = cov_inner_of(p);
    if cov <= 1e-5 {
        discard;
    }
    return premul(fill_at(p)) * cov;

#else
#ifdef PASS_BORDER_ONLY
    // Border only: annulus = cov_outer - cov_inner.
    let cov_outer = cov_outer_of(p);
    let cov_inner = cov_inner_of(p);
    let border_mask = clamp(cov_outer - cov_inner, 0.0, 1.0);
    if border_mask <= 1e-5 {
        discard;
    }
#ifdef BORDER_NONE
    return vec4<f32>(0.0);
#else
    return premul(border_color_of(p)) * border_mask;
#endif

#else
    // Combined.
    let cov_outer = cov_outer_of(p);
#ifdef BORDER_NONE
    let cov_inner = cov_outer;
#else
    let has_inner = u.flags.x > 0.5;
    var cov_inner = 0.0;
    if has_inner {
        cov_inner = cov_inner_of(p);
    }
#endif

    let border_mask = clamp(cov_outer - cov_inner, 0.0, 1.0);
    let fill_mask = cov_inner;
    let shape_mask = max(border_mask, fill_mask);
    // Discard fragments outside the shape so the stencil Increment/Decrement
    // passes (which share this shader) only touch stencil inside the rrect.
    if shape_mask <= 1e-5 {
        discard;
    }

    var out = vec4<f32>(0.0);
#ifndef BORDER_NONE
    out = out + premul(border_color_of(p)) * border_mask;
#endif
#ifdef HAS_FILL
    out = out + premul(fill_at(p)) * fill_mask;
#endif
    return out;
#endif
#endif
}
