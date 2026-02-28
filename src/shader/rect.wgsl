struct RectParams {
    outer_rect: vec4<f32>,    // min_x, min_y, max_x, max_y (pixel space)
    inner_rect: vec4<f32>,    // min_x, min_y, max_x, max_y (pixel space)
    outer_rx: vec4<f32>,      // TL, TR, BR, BL
    outer_ry: vec4<f32>,
    inner_rx: vec4<f32>,
    inner_ry: vec4<f32>,
    border_widths: vec4<f32>, // left, top, right, bottom
    flags: vec4<f32>,         // x: has_inner
    fill_color: vec4<f32>,
    border_left: vec4<f32>,
    border_top: vec4<f32>,
    border_right: vec4<f32>,
    border_bottom: vec4<f32>,
    screen_size: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> u: RectParams;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) pixel_pos: vec2<f32>,
}

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
    let outside = length(max(d, vec2<f32>(0.0)));
    let inside = min(max(d.x, d.y), 0.0);
    return outside + inside;
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

fn coverageRRect(p: vec2<f32>, rect: vec4<f32>, radii: Radii4) -> f32 {
    let d = sd_rrect(p, rect, radii);
    let aa = aa_width_px(p);
    return smoothstep(aa, -aa, d);
}

fn pickBorderColor(p: vec2<f32>) -> vec4<f32> {
    // Border side selection with narrow AA only at side junctions.
    // score = distance_to_edge / border_width_of_edge.
    let minp = u.outer_rect.xy;
    let maxp = u.outer_rect.zw;
    let b = u.border_widths; // left, top, right, bottom

    let dL = max(p.x - minp.x, 0.0);
    let dT = max(p.y - minp.y, 0.0);
    let dR = max(maxp.x - p.x, 0.0);
    let dB = max(maxp.y - p.y, 0.0);

    let inf = 1e6;
    let sL = select(inf, safe_div(dL, b.x, inf), b.x > 1e-6);
    let sT = select(inf, safe_div(dT, b.y, inf), b.y > 1e-6);
    let sR = select(inf, safe_div(dR, b.z, inf), b.z > 1e-6);
    let sB = select(inf, safe_div(dB, b.w, inf), b.w > 1e-6);

    // Branchless min/second-min over four scores.
    let s = vec4<f32>(sL, sT, sR, sB);
    let c = array<vec4<f32>, 4>(u.border_left, u.border_top, u.border_right, u.border_bottom);

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

    // Very narrow blend only when the two best scores are almost equal.
    let aa = max(max(fwidth(min_score), fwidth(second_score)) * 0.75, 1e-4);
    let t = smoothstep(-aa, aa, second_score - min_score);
    return mix(c1, c0, t);
}

@vertex
fn vs_main(@location(0) uv: vec2<f32>) -> VertexOut {
    var out: VertexOut;
    let p = mix(u.outer_rect.xy, u.outer_rect.zw, uv);
    let ndc = vec2<f32>(
        (p.x / u.screen_size.x) * 2.0 - 1.0,
        1.0 - (p.y / u.screen_size.y) * 2.0,
    );
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.pixel_pos = p;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let p = in.pixel_pos;

    let outer_r = Radii4(u.outer_rx, u.outer_ry);
    let inner_r = Radii4(u.inner_rx, u.inner_ry);

    let cov_outer = coverageRRect(p, u.outer_rect, outer_r);

    let has_inner = u.flags.x > 0.5;
    let cov_inner = select(0.0, coverageRRect(p, u.inner_rect, inner_r), has_inner);

    let border_mask = clamp(cov_outer - cov_inner, 0.0, 1.0);
    let fill_mask = cov_inner;

    let border_pm = premul(pickBorderColor(p)) * border_mask;
    let fill_pm = premul(u.fill_color) * fill_mask;

    return border_pm + fill_pm;
}
