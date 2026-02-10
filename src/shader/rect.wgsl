struct RectUniform {
    // rect_pos_size: rect_pos.xy, rect_size.zw (像素座標，左上角為原點)
    rect_pos_size: vec4<f32>,
    // screen_radius_border: screen_size.xy, border_radius.z, border_width.w
    screen_radius_border: vec4<f32>,
    fill_color: vec4<f32>,
    border_color: vec4<f32>,
    // misc: opacity.x, padding.yzw
    misc: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> rect: RectUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // 兩個三角形組成的矩形 (6 個頂點)，以左上角為 (0,0)
    var quad = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );

    let rect_pos = rect.rect_pos_size.xy;
    let rect_size = rect.rect_pos_size.zw;
    let screen_size = rect.screen_radius_border.xy;

    let local = quad[vertex_index];
    let pixel = rect_pos + local * rect_size;

    // 像素座標 (左上角原點) -> NDC
    let ndc_x = (pixel.x / screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel.y / screen_size.y) * 2.0;
    let ndc = vec2<f32>(ndc_x, ndc_y);

    return VertexOutput(vec4<f32>(ndc, 0.0, 1.0));
}

fn sd_round_rect(p: vec2<f32>, half: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fs_main(@builtin(position) frag_pos: vec4<f32>) -> @location(0) vec4<f32> {
    let rect_pos = rect.rect_pos_size.xy;
    let rect_size = rect.rect_pos_size.zw;
    let radius = rect.screen_radius_border.z;
    let border_width = rect.screen_radius_border.w;
    let opacity = rect.misc.x;

    let half = rect_size * 0.5;
    let center = rect_pos + half;
    let p = frag_pos.xy - center;

    let max_radius = min(half.x, half.y);
    let r = clamp(radius, 0.0, max_radius);

    let bw = clamp(border_width, 0.0, min(half.x, half.y));
    let inner_half = max(half - vec2<f32>(bw), vec2<f32>(0.0));
    let inner_radius = clamp(r - bw, 0.0, min(inner_half.x, inner_half.y));

    let outer_d = sd_round_rect(p, half, r);
    let inner_d = sd_round_rect(p, inner_half, inner_radius);

    let aa_outer = fwidth(outer_d);
    let aa_inner = fwidth(inner_d);

    let outer_alpha = 1.0 - smoothstep(0.0, aa_outer, outer_d);
    let inner_alpha = 1.0 - smoothstep(0.0, aa_inner, inner_d);

    let fill_alpha   = clamp(inner_alpha, 0.0, 1.0);
    let border_alpha = clamp(outer_alpha - inner_alpha, 0.0, 1.0);

    var color = rect.fill_color * fill_alpha + rect.border_color * border_alpha;
    color *= opacity;
    return color;
}
