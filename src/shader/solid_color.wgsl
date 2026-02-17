struct RectUniform {
    // rect_pos_size: rect_pos.xy, rect_size.zw (pixel coordinates, top-left is the origin)
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
    // Rectangle composed of two triangles (6 vertices), with top-left at (0,0)
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

    // pixel coordinates (top-left origin) -> NDC
    let ndc_x = (pixel.x / screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel.y / screen_size.y) * 2.0;
    let ndc = vec2<f32>(ndc_x, ndc_y);

    return VertexOutput(vec4<f32>(ndc, 0.0, 1.0));
}

@fragment
fn fs_main(@builtin(position) frag_pos: vec4<f32>) -> @location(0) vec4<f32> {
    let opacity = rect.misc.x;
    return rect.fill_color * opacity;
}
