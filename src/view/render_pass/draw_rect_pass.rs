use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::render_target::{render_target_size, render_target_view};
use wgpu::util::DeviceExt;

pub struct DrawRectPass {
    position: [f32; 2],
    size: [f32; 2],
    fill_color: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    border_radii: [f32; 4], // [top_left, top_right, bottom_right, bottom_left]
    opacity: f32,
    scissor_rect: Option<[u32; 4]>,
    color_target: Option<TextureHandle>,
    input: DrawRectInput,
    output: DrawRectOutput,
}

#[derive(Default)]
pub struct DrawRectInput {
    pub render_target: RenderTargetIn,
}

#[derive(Default)]
pub struct DrawRectOutput {
    pub render_target: RenderTargetOut,
}

impl DrawRectPass {
    pub fn new(position: [f32; 2], size: [f32; 2], color: [f32; 4], opacity: f32) -> Self {
        Self {
            position,
            size,
            fill_color: color,
            border_color: [0.0, 0.0, 0.0, 0.0],
            border_width: 0.0,
            border_radii: [0.0; 4],
            opacity,
            scissor_rect: None,
            color_target: None,
            input: DrawRectInput::default(),
            output: DrawRectOutput::default(),
        }
    }

    pub fn set_position(&mut self, position: [f32; 2]) {
        self.position = position;
    }

    pub fn set_size(&mut self, size: [f32; 2]) {
        self.size = size;
    }

    pub fn set_color(&mut self, color: [f32; 4]) {
        self.fill_color = color;
    }

    pub fn set_border_color(&mut self, color: [f32; 4]) {
        self.border_color = color;
    }

    pub fn set_border_width(&mut self, width: f32) {
        self.border_width = width.max(0.0);
    }

    pub fn set_border_radius(&mut self, radius: f32) {
        self.border_radii = [radius.max(0.0); 4];
    }

    pub fn set_border_radii(&mut self, radii: [f32; 4]) {
        self.border_radii = radii.map(|v| v.max(0.0));
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }

    pub fn set_input(&mut self, input: RenderTargetIn) {
        self.input.render_target = input;
    }

    pub fn set_output(&mut self, output: RenderTargetOut) {
        self.output.render_target = output;
    }

    pub fn set_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.scissor_rect = scissor_rect;
    }

    pub fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        self.color_target = color_target;
    }
}

impl RenderTargetPass for DrawRectPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        DrawRectPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        DrawRectPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.scissor_rect = intersect_scissor_rects(self.scissor_rect, scissor_rect);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        DrawRectPass::set_color_target(self, color_target);
    }
}

fn intersect_scissor_rects(a: Option<[u32; 4]>, b: Option<[u32; 4]>) -> Option<[u32; 4]> {
    match (a, b) {
        (None, None) => None,
        (Some(rect), None) | (None, Some(rect)) => Some(rect),
        (Some([ax, ay, aw, ah]), Some([bx, by, bw, bh])) => {
            let a_right = ax.saturating_add(aw);
            let a_bottom = ay.saturating_add(ah);
            let b_right = bx.saturating_add(bw);
            let b_bottom = by.saturating_add(bh);
            let left = ax.max(bx);
            let top = ay.max(by);
            let right = a_right.min(b_right);
            let bottom = a_bottom.min(b_bottom);
            if right <= left || bottom <= top {
                return None;
            }
            Some([left, top, right - left, bottom - top])
        }
    }
}

const RECT_RESOURCES: u64 = 1;

#[derive(Clone, Copy)]
pub struct RenderTargetTag;
pub type RenderTargetIn = InSlot<TextureResource, RenderTargetTag>;
pub type RenderTargetOut = OutSlot<TextureResource, RenderTargetTag>;

impl RenderPass for DrawRectPass {
    type Input = DrawRectInput;
    type Output = DrawRectOutput;

    fn input(&self) -> &Self::Input {
        &self.input
    }

    fn input_mut(&mut self) -> &mut Self::Input {
        &mut self.input
    }

    fn output(&self) -> &Self::Output {
        &self.output
    }

    fn output_mut(&mut self) -> &mut Self::Output {
        &mut self.output
    }

    fn build(&mut self, builder: &mut BuildContext) {
        if let Some(handle) = self.input.render_target.handle() {
            let source: OutSlot<TextureResource, RenderTargetTag> = OutSlot::with_handle(handle);
            builder.read_texture(&mut self.input.render_target, &source);
        }
        if self.output.render_target.handle().is_some() {
            builder.write_texture(&mut self.output.render_target);
        }
    }

    fn execute(&mut self, ctx: &mut PassContext<'_, '_>) {
        let offscreen_view = match self.color_target {
            Some(handle) => render_target_view(ctx, handle),
            None => None,
        };
        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = match self.color_target {
            Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
            None => surface_size,
        };

        let viewport = &mut ctx.viewport;
        let scale = viewport.scale_factor();
        let device = match viewport.device() {
            Some(device) => device,
            None => return,
        };

        let format = viewport.surface_format();
        let resources = ctx
            .cache
            .get_or_insert_with::<DrawRectResources, _>(RECT_RESOURCES, || {
                create_draw_rect_resources(device, format)
            });
        if resources.pipeline_format != format {
            *resources = create_draw_rect_resources(device, format);
        }

        let scaled_position = [self.position[0] * scale, self.position[1] * scale];
        let scaled_size = [self.size[0] * scale, self.size[1] * scale];
        let scaled_border_width = self.border_width * scale;
        let scaled_border_radii = self.border_radii.map(|radius| radius * scale);

        let (vertices, indices) = tessellate_rounded_rect(
            scaled_position,
            scaled_size,
            self.fill_color,
            self.border_color,
            scaled_border_width,
            scaled_border_radii,
            self.opacity,
            target_w as f32,
            target_h as f32,
        );
        if vertices.is_empty() || indices.is_empty() {
            return;
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("DrawRect Vertex Buffer (Per Pass)"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("DrawRect Index Buffer (Per Pass)"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let scissor_rect_physical = self.scissor_rect.and_then(|scissor_rect| {
            viewport.logical_scissor_to_physical(scissor_rect, (target_w, target_h))
        });

        let parts = match viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
        };
        let color_view = offscreen_view.as_ref().unwrap_or(parts.view);
        let mut pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("DrawRect"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                    resolve_target: None,
                })],
                depth_stencil_attachment: parts
                    .depth_stencil_attachment(wgpu::LoadOp::Load, wgpu::LoadOp::Load),
                ..Default::default()
            });
        pass.set_pipeline(&resources.pipeline);
        if let Some([x, y, width, height]) = scissor_rect_physical {
            pass.set_scissor_rect(x, y, width, height);
        }
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct RectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

struct DrawRectResources {
    pipeline: wgpu::RenderPipeline,
    pipeline_format: wgpu::TextureFormat,
}

fn create_draw_rect_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> DrawRectResources {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("DrawRect Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/rect.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("DrawRect Pipeline Layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("DrawRect Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<RectVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: std::mem::size_of::<[f32; 2]>() as u64,
                        shader_location: 1,
                    },
                ],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Always,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Always,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                read_mask: 0xff,
                write_mask: 0x00,
            },
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    DrawRectResources {
        pipeline,
        pipeline_format: format,
    }
}

fn tessellate_rounded_rect(
    position: [f32; 2],
    size: [f32; 2],
    fill_color: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    border_radii: [f32; 4],
    opacity: f32,
    screen_w: f32,
    screen_h: f32,
) -> (Vec<RectVertex>, Vec<u32>) {
    let width = size[0].max(0.0);
    let height = size[1].max(0.0);
    if width <= 0.0 || height <= 0.0 || screen_w <= 0.0 || screen_h <= 0.0 {
        return (Vec::new(), Vec::new());
    }

    let radii = normalize_corner_radii(border_radii, width, height);
    let max_bw = (width.min(height)) * 0.5;
    let bw = border_width.clamp(0.0, max_bw);
    let border_enabled = bw > 0.0;
    let effective_opacity = opacity.clamp(0.0, 1.0);
    if effective_opacity <= 0.0 {
        return (Vec::new(), Vec::new());
    }

    let max_outer_radius = radii.into_iter().fold(0.0f32, f32::max);
    let segments = corner_segments(max_outer_radius);
    let outer = rounded_rect_points(position[0], position[1], width, height, radii, segments);
    if outer.len() < 3 {
        return (Vec::new(), Vec::new());
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let fill_rgba = [
        fill_color[0],
        fill_color[1],
        fill_color[2],
        fill_color[3] * effective_opacity,
    ];
    let border_rgba = [
        border_color[0],
        border_color[1],
        border_color[2],
        border_color[3] * effective_opacity,
    ];
    let silhouette_rgba = if border_enabled {
        border_rgba
    } else {
        fill_rgba
    };
    let aa_width = 1.0_f32;
    let outer_aa_radii = normalize_corner_radii(
        radii.map(|r| r + aa_width),
        width + aa_width * 2.0,
        height + aa_width * 2.0,
    );
    let outer_aa = rounded_rect_points(
        position[0] - aa_width,
        position[1] - aa_width,
        width + aa_width * 2.0,
        height + aa_width * 2.0,
        outer_aa_radii,
        segments,
    );

    if border_enabled {
        let inner_x = position[0] + bw;
        let inner_y = position[1] + bw;
        let inner_w = (width - bw * 2.0).max(0.0);
        let inner_h = (height - bw * 2.0).max(0.0);

        if inner_w > 0.0 && inner_h > 0.0 {
            let inner_radii =
                normalize_corner_radii(radii.map(|r| (r - bw).max(0.0)), inner_w, inner_h);
            let inner =
                rounded_rect_points(inner_x, inner_y, inner_w, inner_h, inner_radii, segments);
            append_convex_fan(
                &mut vertices,
                &mut indices,
                &inner,
                fill_rgba,
                screen_w,
                screen_h,
            );
            append_ring(
                &mut vertices,
                &mut indices,
                &outer,
                &inner,
                border_rgba,
                border_rgba,
                screen_w,
                screen_h,
            );
        } else {
            append_convex_fan(
                &mut vertices,
                &mut indices,
                &outer,
                border_rgba,
                screen_w,
                screen_h,
            );
        }
    } else {
        append_convex_fan(
            &mut vertices,
            &mut indices,
            &outer,
            fill_rgba,
            screen_w,
            screen_h,
        );
    }
    // Geometry AA: feather the outer silhouette to transparent.
    append_ring(
        &mut vertices,
        &mut indices,
        &outer_aa,
        &outer,
        [
            silhouette_rgba[0],
            silhouette_rgba[1],
            silhouette_rgba[2],
            0.0,
        ],
        silhouette_rgba,
        screen_w,
        screen_h,
    );

    (vertices, indices)
}

fn corner_segments(radius: f32) -> u32 {
    // Keep chord length around ~2.5px per segment for smoother large radii.
    ((std::f32::consts::FRAC_PI_2 * radius / 2.5).ceil() as u32).clamp(2, 64)
}

fn rounded_rect_points(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radii: [f32; 4],
    segments: u32,
) -> Vec<[f32; 2]> {
    let segments = segments.max(2);
    if radii.into_iter().all(|r| r <= 0.0) {
        return rectangle_points(x, y, width, height, segments);
    }
    let mut points = Vec::with_capacity((segments as usize) * 4);
    let corners = [
        (
            [x + radii[0], y + radii[0]],
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
            radii[0],
        ),
        (
            [x + width - radii[1], y + radii[1]],
            std::f32::consts::PI * 1.5,
            std::f32::consts::PI * 2.0,
            radii[1],
        ),
        (
            [x + width - radii[2], y + height - radii[2]],
            0.0,
            std::f32::consts::PI * 0.5,
            radii[2],
        ),
        (
            [x + radii[3], y + height - radii[3]],
            std::f32::consts::PI * 0.5,
            std::f32::consts::PI,
            radii[3],
        ),
    ];

    for (center, start, end, radius) in corners.iter() {
        if *radius <= 0.0 {
            let anchor = if *start < std::f32::consts::PI * 0.5 {
                [x + width, y + height]
            } else if *start < std::f32::consts::PI {
                [x, y + height]
            } else if *start < std::f32::consts::PI * 1.5 {
                [x, y]
            } else {
                [x + width, y]
            };
            for _ in 0..segments {
                points.push(anchor);
            }
            continue;
        }
        for step in 0..segments {
            let t = step as f32 / segments as f32;
            let angle = start + (end - start) * t;
            points.push([
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ]);
        }
    }
    points
}

fn normalize_corner_radii(mut radii: [f32; 4], width: f32, height: f32) -> [f32; 4] {
    for r in &mut radii {
        *r = r.max(0.0);
    }
    let w = width.max(0.0);
    let h = height.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return [0.0; 4];
    }
    let top = radii[0] + radii[1];
    let bottom = radii[3] + radii[2];
    let left = radii[0] + radii[3];
    let right = radii[1] + radii[2];
    let mut scale = 1.0_f32;
    if top > w {
        scale = scale.min(w / top);
    }
    if bottom > w {
        scale = scale.min(w / bottom);
    }
    if left > h {
        scale = scale.min(h / left);
    }
    if right > h {
        scale = scale.min(h / right);
    }
    if scale < 1.0 {
        for r in &mut radii {
            *r *= scale;
        }
    }

    radii
}

fn rectangle_points(x: f32, y: f32, width: f32, height: f32, segments: u32) -> Vec<[f32; 2]> {
    let mut points = Vec::with_capacity((segments as usize) * 4);
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        points.push([x + width * t, y]);
    }
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        points.push([x + width, y + height * t]);
    }
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        points.push([x + width * (1.0 - t), y + height]);
    }
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        points.push([x, y + height * (1.0 - t)]);
    }
    points
}

fn append_convex_fan(
    vertices: &mut Vec<RectVertex>,
    indices: &mut Vec<u32>,
    polygon: &[[f32; 2]],
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let cleaned = sanitize_polygon(polygon);
    if cleaned.len() < 3 {
        return;
    }
    for point in &cleaned {
        vertices.push(RectVertex {
            position: pixel_to_ndc(point[0], point[1], screen_w, screen_h),
            color,
        });
    }

    let base = (vertices.len() - cleaned.len()) as u32;
    for tri in triangulate_polygon_indices(&cleaned) {
        indices.extend_from_slice(&[
            base + tri[0] as u32,
            base + tri[1] as u32,
            base + tri[2] as u32,
        ]);
    }
}

fn triangulate_polygon_indices(polygon: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let n = polygon.len();
    if n < 3 {
        return Vec::new();
    }
    if n == 3 {
        return vec![[0, 1, 2]];
    }

    let ccw = polygon_signed_area(polygon) >= 0.0;
    let mut verts: Vec<usize> = if ccw {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    let mut guard = 0usize;
    let max_guard = n * n;

    while verts.len() > 3 && guard < max_guard {
        guard += 1;
        let m = verts.len();
        let mut ear_found = false;
        for i in 0..m {
            let prev = verts[(i + m - 1) % m];
            let curr = verts[i];
            let next = verts[(i + 1) % m];
            if !is_convex_ccw(polygon[prev], polygon[curr], polygon[next]) {
                continue;
            }
            let mut has_inside = false;
            for &other in &verts {
                if other == prev || other == curr || other == next {
                    continue;
                }
                if point_in_triangle(polygon[other], polygon[prev], polygon[curr], polygon[next]) {
                    has_inside = true;
                    break;
                }
            }
            if has_inside {
                continue;
            }
            out.push([prev, curr, next]);
            verts.remove(i);
            ear_found = true;
            break;
        }
        if !ear_found {
            break;
        }
    }

    if verts.len() == 3 {
        out.push([verts[0], verts[1], verts[2]]);
    }
    out
}

fn sanitize_polygon(polygon: &[[f32; 2]]) -> Vec<[f32; 2]> {
    const EPS: f32 = 1e-4;
    if polygon.len() < 3 {
        return polygon.to_vec();
    }

    let mut out = Vec::with_capacity(polygon.len());
    for &p in polygon {
        if out.last().is_none_or(|last: &[f32; 2]| {
            (last[0] - p[0]).abs() > EPS || (last[1] - p[1]).abs() > EPS
        }) {
            out.push(p);
        }
    }
    if out.len() >= 2 {
        let first = out[0];
        let last = out[out.len() - 1];
        if (first[0] - last[0]).abs() <= EPS && (first[1] - last[1]).abs() <= EPS {
            out.pop();
        }
    }
    out
}

fn polygon_signed_area(polygon: &[[f32; 2]]) -> f32 {
    let mut area = 0.0f32;
    for i in 0..polygon.len() {
        let j = (i + 1) % polygon.len();
        area += polygon[i][0] * polygon[j][1] - polygon[j][0] * polygon[i][1];
    }
    area * 0.5
}

fn is_convex_ccw(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    cross(a, b, c) > 1e-5
}

fn point_in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let c1 = cross(a, b, p);
    let c2 = cross(b, c, p);
    let c3 = cross(c, a, p);
    (c1 >= -1e-5 && c2 >= -1e-5 && c3 >= -1e-5) || (c1 <= 1e-5 && c2 <= 1e-5 && c3 <= 1e-5)
}

fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn append_ring(
    vertices: &mut Vec<RectVertex>,
    indices: &mut Vec<u32>,
    outer: &[[f32; 2]],
    inner: &[[f32; 2]],
    outer_color: [f32; 4],
    inner_color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let n = outer.len().min(inner.len());
    if n < 3 {
        return;
    }
    let base = vertices.len() as u32;
    for i in 0..n {
        let o = outer[i];
        let ii = inner[i];
        vertices.push(RectVertex {
            position: pixel_to_ndc(o[0], o[1], screen_w, screen_h),
            color: outer_color,
        });
        vertices.push(RectVertex {
            position: pixel_to_ndc(ii[0], ii[1], screen_w, screen_h),
            color: inner_color,
        });
    }

    for i in 0..n {
        let j = (i + 1) % n;
        let o0 = base + (i as u32) * 2;
        let i0 = o0 + 1;
        let o1 = base + (j as u32) * 2;
        let i1 = o1 + 1;
        indices.extend_from_slice(&[o0, i0, o1, i0, i1, o1]);
    }
}

fn pixel_to_ndc(x: f32, y: f32, screen_w: f32, screen_h: f32) -> [f32; 2] {
    [(x / screen_w) * 2.0 - 1.0, 1.0 - (y / screen_h) * 2.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_points_zero_corner_keeps_fixed_topology() {
        let segments = 16;
        let pts = rounded_rect_points(0.0, 0.0, 100.0, 100.0, [0.0, 10.0, 10.0, 10.0], segments);
        assert_eq!(pts.len(), (segments * 4) as usize);
    }

    #[test]
    fn tessellate_asymmetric_radius_with_border_produces_geometry() {
        let (vertices, indices) = tessellate_rounded_rect(
            [0.0, 0.0],
            [150.0, 150.0],
            [0.38, 0.68, 0.94, 1.0],
            [0.13, 0.15, 0.17, 1.0],
            20.0,
            [10.0, 32.0, 10.0, 135.0],
            1.0,
            800.0,
            600.0,
        );
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
        assert_eq!(indices.len() % 3, 0);
    }

    #[test]
    fn append_ring_tolerates_mismatched_topology() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let outer = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let inner = vec![[10.0, 10.0], [90.0, 10.0], [90.0, 90.0]];
        append_ring(
            &mut vertices,
            &mut indices,
            &outer,
            &inner,
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            800.0,
            600.0,
        );
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }
}
