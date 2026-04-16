// Particle effect shader — renders soft, glowing circular particles.
//
// Each particle is an instanced fullscreen quad. The vertex shader positions
// a small quad around the particle center; the fragment shader computes a
// soft circular shape with exponential glow falloff.

struct Uniforms {
    screen_size: vec2<f32>,
    canvas_pos: vec2<f32>,
    canvas_size: vec2<f32>,
    time: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexIn {
    // Per-instance data
    @location(0) pos: vec2<f32>,        // particle center (relative to canvas)
    @location(1) color: vec4<f32>,      // RGBA colour
    @location(2) size_life: vec2<f32>,  // (radius in px, life 0→1 where 1=alive)
}

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,         // -1..1 within the quad
    @location(1) color: vec4<f32>,
    @location(2) life: f32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    instance: VertexIn,
) -> VertexOut {
    // Generate a quad from 6 vertices (two triangles).
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0),
        vec2( 1.0, -1.0),
        vec2(-1.0,  1.0),
        vec2(-1.0,  1.0),
        vec2( 1.0, -1.0),
        vec2( 1.0,  1.0),
    );
    let corner = corners[vi];

    let radius = instance.size_life.x;
    let life = instance.size_life.y;

    // Pixel position of this vertex.
    let pixel = u.canvas_pos + instance.pos + corner * radius;

    // Convert to clip space: pixel → NDC.
    let ndc = vec2(
        pixel.x / u.screen_size.x * 2.0 - 1.0,
        1.0 - pixel.y / u.screen_size.y * 2.0,
    );

    var out: VertexOut;
    out.clip_pos = vec4(ndc, 0.0, 1.0);
    out.uv = corner;
    out.color = instance.color;
    out.life = life;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Distance from quad centre (0 at centre, 1 at edge).
    let dist = length(in.uv);

    // Soft circle with glow.
    let core = smoothstep(1.0, 0.4, dist);             // hard-ish inner circle
    let glow = exp(-dist * dist * 3.0) * 0.6;          // soft outer glow
    let alpha = (core + glow) * in.life;

    // Premultiplied alpha output.
    let rgb = in.color.rgb * alpha;
    return vec4(rgb, alpha);
}
