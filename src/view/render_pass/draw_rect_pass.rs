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
    border_side_colors: [[f32; 4]; 4], // [left, right, top, bottom]
    use_border_side_colors: bool,
    border_widths: [f32; 4], // [left, right, top, bottom]
    border_radii: [[f32; 2]; 4], // [top_left, top_right, bottom_right, bottom_left] each is [rx, ry]
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
            border_side_colors: [[0.0, 0.0, 0.0, 0.0]; 4],
            use_border_side_colors: false,
            border_widths: [0.0; 4],
            border_radii: [[0.0, 0.0]; 4],
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
        self.border_side_colors = [color; 4];
        self.use_border_side_colors = false;
    }

    pub fn set_border_side_colors(
        &mut self,
        left: [f32; 4],
        right: [f32; 4],
        top: [f32; 4],
        bottom: [f32; 4],
    ) {
        self.border_side_colors = [left, right, top, bottom];
        self.use_border_side_colors = true;
    }

    pub fn set_border_width(&mut self, width: f32) {
        self.border_widths = [width.max(0.0); 4];
    }

    pub fn set_border_widths(&mut self, left: f32, right: f32, top: f32, bottom: f32) {
        self.border_widths = [left.max(0.0), right.max(0.0), top.max(0.0), bottom.max(0.0)];
    }

    pub fn set_border_radius(&mut self, radius: f32) {
        let r = radius.max(0.0);
        self.border_radii = [[r, r]; 4];
    }

    pub fn set_border_radii(&mut self, radii: [f32; 4]) {
        self.border_radii = radii.map(|v| {
            let r = v.max(0.0);
            [r, r]
        });
    }

    pub fn set_border_radii_xy(&mut self, radii: [[f32; 2]; 4]) {
        self.border_radii = radii.map(|v| [v[0].max(0.0), v[1].max(0.0)]);
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
            Some(device) => device.clone(),
            None => return,
        };

        let format = viewport.surface_format();
        let resources = ctx
            .cache
            .get_or_insert_with::<DrawRectResources, _>(RECT_RESOURCES, || {
                create_draw_rect_resources(&device, format)
            });
        if resources.pipeline_format != format {
            *resources = create_draw_rect_resources(&device, format);
        }

        let scaled_position = [self.position[0] * scale, self.position[1] * scale];
        let scaled_size = [self.size[0] * scale, self.size[1] * scale];
        let scaled_border_widths = self.border_widths.map(|v| v * scale);
        let scaled_border_radii = self
            .border_radii
            .map(|r| [r[0].max(0.0) * scale, r[1].max(0.0) * scale]);

        let border_side_colors = if self.use_border_side_colors {
            self.border_side_colors
        } else {
            [self.border_color; 4]
        };

        let params = build_rect_params(
            scaled_position,
            scaled_size,
            scaled_border_widths,
            scaled_border_radii,
            self.fill_color,
            border_side_colors,
            self.opacity,
            target_w as f32,
            target_h as f32,
        );
        if params.outer_rect[2] <= params.outer_rect[0] || params.outer_rect[3] <= params.outer_rect[1] {
            return;
        }

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("DrawRect Params Buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DrawRect Bind Group"),
            layout: &resources.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
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
        pass.set_bind_group(0, &bind_group, &[]);
        if let Some([x, y, width, height]) = scissor_rect_physical {
            pass.set_scissor_rect(x, y, width, height);
        }
        pass.set_vertex_buffer(0, resources.vertex_buffer.slice(..));
        pass.set_index_buffer(resources.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..resources.index_count, 0, 0..1);
    }
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct QuadVertex {
    uv: [f32; 2],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct RectParams {
    // [min_x, min_y, max_x, max_y] in physical pixels
    outer_rect: [f32; 4],
    inner_rect: [f32; 4],
    // corner order: TL, TR, BR, BL
    outer_rx: [f32; 4],
    outer_ry: [f32; 4],
    inner_rx: [f32; 4],
    inner_ry: [f32; 4],
    // [left, top, right, bottom]
    border_widths: [f32; 4],
    // flags.x: has_inner (0/1), flags.yzw reserved
    flags: [f32; 4],
    // linear-space, straight alpha (premultiply in shader)
    fill_color: [f32; 4],
    border_left: [f32; 4],
    border_top: [f32; 4],
    border_right: [f32; 4],
    border_bottom: [f32; 4],
    // [w, h, inv_w, inv_h]
    screen_size: [f32; 4],
}

struct DrawRectResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
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

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("DrawRect Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("DrawRect Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("DrawRect Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
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

    let quad_vertices = [
        QuadVertex { uv: [0.0, 0.0] },
        QuadVertex { uv: [1.0, 0.0] },
        QuadVertex { uv: [1.0, 1.0] },
        QuadVertex { uv: [0.0, 1.0] },
    ];
    let quad_indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("DrawRect Quad Vertex Buffer"),
        contents: bytemuck::cast_slice(&quad_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("DrawRect Quad Index Buffer"),
        contents: bytemuck::cast_slice(&quad_indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    DrawRectResources {
        pipeline,
        bind_group_layout,
        vertex_buffer,
        index_buffer,
        index_count: quad_indices.len() as u32,
        pipeline_format: format,
    }
}

type CornerRadii = [[f32; 2]; 4]; // TL, TR, BR, BL

fn build_rect_params(
    position: [f32; 2],
    size: [f32; 2],
    border_widths_lr_tb: [f32; 4], // [left,right,top,bottom]
    mut outer_radii: CornerRadii,
    mut fill_color: [f32; 4],
    border_side_colors_lr_tb: [[f32; 4]; 4], // [left,right,top,bottom]
    opacity: f32,
    screen_w: f32,
    screen_h: f32,
) -> RectParams {
    let width = size[0].max(0.0);
    let height = size[1].max(0.0);

    let outer_min = [position[0], position[1]];
    let outer_max = [position[0] + width, position[1] + height];

    let max_bw = width.min(height) * 0.5;
    let b_left = border_widths_lr_tb[0].clamp(0.0, max_bw);
    let b_right = border_widths_lr_tb[1].clamp(0.0, max_bw);
    let b_top = border_widths_lr_tb[2].clamp(0.0, max_bw);
    let b_bottom = border_widths_lr_tb[3].clamp(0.0, max_bw);

    normalize_corner_radii_css_xy(&mut outer_radii, width, height);

    let inner_min = [outer_min[0] + b_left, outer_min[1] + b_top];
    let inner_max = [outer_max[0] - b_right, outer_max[1] - b_bottom];
    let inner_w = (inner_max[0] - inner_min[0]).max(0.0);
    let inner_h = (inner_max[1] - inner_min[1]).max(0.0);

    let mut inner_radii = [
        [
            (outer_radii[0][0] - b_left).max(0.0),
            (outer_radii[0][1] - b_top).max(0.0),
        ],
        [
            (outer_radii[1][0] - b_right).max(0.0),
            (outer_radii[1][1] - b_top).max(0.0),
        ],
        [
            (outer_radii[2][0] - b_right).max(0.0),
            (outer_radii[2][1] - b_bottom).max(0.0),
        ],
        [
            (outer_radii[3][0] - b_left).max(0.0),
            (outer_radii[3][1] - b_bottom).max(0.0),
        ],
    ];

    let has_inner = inner_w > 0.0 && inner_h > 0.0;
    if has_inner {
        normalize_corner_radii_css_xy(&mut inner_radii, inner_w, inner_h);
    } else {
        inner_radii = [[0.0, 0.0]; 4];
    }

    let opacity = opacity.clamp(0.0, 1.0);
    fill_color[3] *= opacity;

    let mut border_left = border_side_colors_lr_tb[0];
    let mut border_right = border_side_colors_lr_tb[1];
    let mut border_top = border_side_colors_lr_tb[2];
    let mut border_bottom = border_side_colors_lr_tb[3];
    border_left[3] *= opacity;
    border_right[3] *= opacity;
    border_top[3] *= opacity;
    border_bottom[3] *= opacity;

    RectParams {
        outer_rect: [outer_min[0], outer_min[1], outer_max[0], outer_max[1]],
        inner_rect: [inner_min[0], inner_min[1], inner_max[0], inner_max[1]],
        outer_rx: [
            outer_radii[0][0],
            outer_radii[1][0],
            outer_radii[2][0],
            outer_radii[3][0],
        ],
        outer_ry: [
            outer_radii[0][1],
            outer_radii[1][1],
            outer_radii[2][1],
            outer_radii[3][1],
        ],
        inner_rx: [
            inner_radii[0][0],
            inner_radii[1][0],
            inner_radii[2][0],
            inner_radii[3][0],
        ],
        inner_ry: [
            inner_radii[0][1],
            inner_radii[1][1],
            inner_radii[2][1],
            inner_radii[3][1],
        ],
        border_widths: [b_left, b_top, b_right, b_bottom],
        flags: [if has_inner { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        fill_color,
        border_left,
        border_top,
        border_right,
        border_bottom,
        screen_size: [screen_w, screen_h, 1.0 / screen_w.max(1.0), 1.0 / screen_h.max(1.0)],
    }
}

fn normalize_corner_radii_css_xy(radii: &mut CornerRadii, width: f32, height: f32) {
    let w = width.max(0.0);
    let h = height.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        *radii = [[0.0, 0.0]; 4];
        return;
    }

    for r in radii.iter_mut() {
        r[0] = r[0].max(0.0);
        r[1] = r[1].max(0.0);
    }

    let sum_top_x = radii[0][0] + radii[1][0];
    let sum_bottom_x = radii[3][0] + radii[2][0];
    let sum_left_y = radii[0][1] + radii[3][1];
    let sum_right_y = radii[1][1] + radii[2][1];

    let sx = [
        if sum_top_x > 0.0 { w / sum_top_x } else { 1.0 },
        if sum_bottom_x > 0.0 {
            w / sum_bottom_x
        } else {
            1.0
        },
    ]
    .into_iter()
    .fold(1.0_f32, f32::min)
    .min(1.0);

    let sy = [
        if sum_left_y > 0.0 { h / sum_left_y } else { 1.0 },
        if sum_right_y > 0.0 {
            h / sum_right_y
        } else {
            1.0
        },
    ]
    .into_iter()
    .fold(1.0_f32, f32::min)
    .min(1.0);

    for r in radii.iter_mut() {
        r[0] *= sx;
        r[1] *= sy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_smaller_than_border_clamps_inner_radii_and_inner_rect_safely() {
        let params = build_rect_params(
            [10.0, 20.0],
            [20.0, 16.0],
            [12.0, 11.0, 13.0, 10.0], // left, right, top, bottom
            [[6.0, 5.0], [4.0, 4.0], [7.0, 6.0], [5.0, 3.0]],
            [0.2, 0.3, 0.4, 1.0],
            [
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                [1.0, 1.0, 0.0, 1.0],
            ],
            1.0,
            800.0,
            600.0,
        );

        assert_eq!(params.flags[0], 0.0, "inner rect should be disabled when collapsed");
        for &v in params.inner_rx.iter().chain(params.inner_ry.iter()) {
            assert!(v >= 0.0);
            assert_eq!(v, 0.0);
        }
        assert!(params.inner_rect[2] <= params.inner_rect[0] || params.inner_rect[3] <= params.inner_rect[1]);
    }

    #[test]
    fn css_radius_normalization_scales_xy_to_avoid_overlap() {
        // width=100, height=60.
        // x sums: top=140, bottom=140 => sx = 100/140 = 0.7142857...
        // y sums: left=90, right=90 => sy = 60/90 = 0.6666666...
        let params = build_rect_params(
            [0.0, 0.0],
            [100.0, 60.0],
            [4.0, 4.0, 4.0, 4.0],
            [[70.0, 45.0], [70.0, 45.0], [70.0, 45.0], [70.0, 45.0]],
            [0.1, 0.2, 0.3, 1.0],
            [[0.4, 0.4, 0.4, 1.0]; 4],
            1.0,
            1000.0,
            800.0,
        );

        let sx = 100.0 / 140.0;
        let sy = 60.0 / 90.0;
        let expected_rx = 70.0 * sx;
        let expected_ry = 45.0 * sy;

        for i in 0..4 {
            assert!((params.outer_rx[i] - expected_rx).abs() < 1e-4);
            assert!((params.outer_ry[i] - expected_ry).abs() < 1e-4);
        }

        // Ensure adjacent sums are clamped to bounds after normalization.
        let top_sum = params.outer_rx[0] + params.outer_rx[1];
        let bottom_sum = params.outer_rx[3] + params.outer_rx[2];
        let left_sum = params.outer_ry[0] + params.outer_ry[3];
        let right_sum = params.outer_ry[1] + params.outer_ry[2];
        assert!(top_sum <= 100.0 + 1e-4);
        assert!(bottom_sum <= 100.0 + 1e-4);
        assert!(left_sum <= 60.0 + 1e-4);
        assert!(right_sum <= 60.0 + 1e-4);
    }
}
