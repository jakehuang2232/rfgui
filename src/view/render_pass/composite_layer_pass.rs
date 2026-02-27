use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use crate::view::render_pass::render_target::{render_target_size, render_target_view};
use std::collections::HashSet;
use wgpu::util::DeviceExt;

const COMPOSITE_LAYER_RESOURCES: u64 = 201;

#[derive(Clone, Copy)]
pub struct LayerTag;
pub type LayerIn = InSlot<TextureResource, LayerTag>;
pub type LayerOut = OutSlot<TextureResource, LayerTag>;

pub struct CompositeLayerPass {
    rect_pos: [f32; 2],
    rect_size: [f32; 2],
    corner_radii: [f32; 4], // [top_left, top_right, bottom_right, bottom_left]
    opacity: f32,
    scissor_rect: Option<[u32; 4]>,
    color_target: Option<TextureHandle>,
    input: CompositeLayerInput,
    output: CompositeLayerOutput,
}

#[derive(Default)]
pub struct CompositeLayerInput {
    pub render_target: RenderTargetIn,
    pub layer: LayerIn,
}

#[derive(Default)]
pub struct CompositeLayerOutput {
    pub render_target: RenderTargetOut,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct CompositeVertex {
    position: [f32; 2],
    screen_uv: [f32; 2],
    alpha: f32,
    _pad: f32,
}

struct CompositeLayerResources {
    pipeline: wgpu::RenderPipeline,
    debug_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline_format: wgpu::TextureFormat,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct DebugVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl CompositeLayerPass {
    pub fn new(
        rect_pos: [f32; 2],
        rect_size: [f32; 2],
        corner_radii: [f32; 4],
        opacity: f32,
        layer: LayerOut,
    ) -> Self {
        Self {
            rect_pos,
            rect_size,
            corner_radii,
            opacity,
            scissor_rect: None,
            color_target: None,
            input: CompositeLayerInput {
                render_target: RenderTargetIn::default(),
                layer: InSlot::with_handle(layer.handle().unwrap()),
            },
            output: CompositeLayerOutput::default(),
        }
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

impl RenderPass for CompositeLayerPass {
    type Input = CompositeLayerInput;
    type Output = CompositeLayerOutput;

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
        let Some(layer_handle) = self.input.layer.handle() else {
            return;
        };
        let Some(layer_view) = render_target_view(ctx, layer_handle) else {
            return;
        };
        let offscreen_view = match self.color_target {
            Some(handle) => render_target_view(ctx, handle),
            None => None,
        };

        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = match self.color_target {
            Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
            None => surface_size,
        };
        let (layer_w, layer_h) = render_target_size(ctx, layer_handle).unwrap_or(surface_size);
        let scale = ctx.viewport.scale_factor();

        let device = match ctx.viewport.device() {
            Some(device) => device.clone(),
            None => return,
        };
        let format = ctx.viewport.surface_format();
        let resources = ctx
            .cache
            .get_or_insert_with::<CompositeLayerResources, _>(COMPOSITE_LAYER_RESOURCES, || {
                create_resources(&device, format)
            });
        if resources.pipeline_format != format {
            *resources = create_resources(&device, format);
        }

        let scaled_rect_pos = [self.rect_pos[0] * scale, self.rect_pos[1] * scale];
        let scaled_rect_size = [self.rect_size[0] * scale, self.rect_size[1] * scale];
        let scaled_corner_radii = self.corner_radii.map(|radius| radius * scale);

        let (vertices, indices) = tessellate_composite_layer(
            scaled_rect_pos,
            scaled_rect_size,
            scaled_corner_radii,
            self.opacity,
            target_w as f32,
            target_h as f32,
            layer_w as f32,
            layer_h as f32,
        );
        if vertices.is_empty() || indices.is_empty() {
            return;
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CompositeLayer Vertex Buffer (Per Pass)"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CompositeLayer Index Buffer (Per Pass)"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("CompositeLayer Bind Group"),
            layout: &resources.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&layer_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&resources.sampler),
                },
            ],
        });
        let scissor_rect_physical = self.scissor_rect.and_then(|scissor_rect| {
            ctx.viewport
                .logical_scissor_to_physical(scissor_rect, (target_w, target_h))
        });

        let debug_geometry_overlay = ctx.viewport.debug_geometry_overlay();
        let parts = match ctx.viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
        };
        let color_view = offscreen_view.as_ref().unwrap_or(parts.view);

        let mut pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CompositeLayer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                    resolve_target: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        if let Some([x, y, width, height]) = scissor_rect_physical {
            pass.set_scissor_rect(x, y, width, height);
        }
        pass.set_pipeline(&resources.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);

        if debug_geometry_overlay {
            let (debug_vertices, debug_indices) = build_debug_overlay_geometry(
                &vertices,
                &indices,
                target_w as f32,
                target_h as f32,
                [0.2, 1.0, 0.95, 0.95],
                [0.2, 1.0, 0.35, 0.95],
            );
            if !debug_vertices.is_empty() && !debug_indices.is_empty() {
                let debug_vertex_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Composite Debug Vertex Buffer"),
                        contents: bytemuck::cast_slice(&debug_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let debug_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Composite Debug Index Buffer"),
                    contents: bytemuck::cast_slice(&debug_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                pass.set_pipeline(&resources.debug_pipeline);
                pass.set_vertex_buffer(0, debug_vertex_buffer.slice(..));
                pass.set_index_buffer(debug_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..debug_indices.len() as u32, 0, 0..1);
            }
        }
    }
}

impl RenderTargetPass for CompositeLayerPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        CompositeLayerPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        CompositeLayerPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.scissor_rect = intersect_scissor_rects(self.scissor_rect, scissor_rect);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        CompositeLayerPass::set_color_target(self, color_target);
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

fn create_resources(device: &wgpu::Device, format: wgpu::TextureFormat) -> CompositeLayerResources {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("CompositeLayer Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/layer_composite.wgsl").into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("CompositeLayer Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("CompositeLayer Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        // Use nearest sampling for 1:1 UI compositing to avoid edge bleed halos.
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("CompositeLayer Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("CompositeLayer Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<CompositeVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: std::mem::size_of::<[f32; 2]>() as u64,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: (std::mem::size_of::<[f32; 2]>() * 2) as u64,
                        shader_location: 2,
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
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let debug_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Composite Debug Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/rect.wgsl").into()),
    });
    let debug_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Composite Debug Pipeline Layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let debug_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Composite Debug Pipeline"),
        layout: Some(&debug_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &debug_shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<DebugVertex>() as u64,
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
            module: &debug_shader,
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
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    CompositeLayerResources {
        pipeline,
        debug_pipeline,
        bind_group_layout,
        sampler,
        pipeline_format: format,
    }
}

fn tessellate_composite_layer(
    position: [f32; 2],
    size: [f32; 2],
    corner_radii: [f32; 4],
    opacity: f32,
    target_w: f32,
    target_h: f32,
    layer_w: f32,
    layer_h: f32,
) -> (Vec<CompositeVertex>, Vec<u32>) {
    let width = size[0].max(0.0);
    let height = size[1].max(0.0);
    if width <= 0.0
        || height <= 0.0
        || target_w <= 0.0
        || target_h <= 0.0
        || layer_w <= 0.0
        || layer_h <= 0.0
    {
        return (Vec::new(), Vec::new());
    }

    let alpha = opacity.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return (Vec::new(), Vec::new());
    }

    let radii = normalize_corner_radii(corner_radii, width, height);
    let max_outer_radius = radii.into_iter().fold(0.0f32, f32::max);
    let segments = corner_segments(max_outer_radius);
    let outer = rounded_rect_points(position[0], position[1], width, height, radii, segments);
    if outer.len() < 3 {
        return (Vec::new(), Vec::new());
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    append_convex_fan(
        &mut vertices,
        &mut indices,
        &outer,
        alpha,
        target_w,
        target_h,
        layer_w,
        layer_h,
    );

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
    append_ring(
        &mut vertices,
        &mut indices,
        &outer_aa,
        &outer,
        0.0,
        alpha,
        target_w,
        target_h,
        layer_w,
        layer_h,
    );

    (vertices, indices)
}

fn corner_segments(radius: f32) -> u32 {
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
    vertices: &mut Vec<CompositeVertex>,
    indices: &mut Vec<u32>,
    polygon: &[[f32; 2]],
    alpha: f32,
    target_w: f32,
    target_h: f32,
    layer_w: f32,
    layer_h: f32,
) {
    let cleaned = sanitize_polygon(polygon);
    if cleaned.len() < 3 {
        return;
    }
    for point in &cleaned {
        vertices.push(CompositeVertex {
            position: pixel_to_ndc(point[0], point[1], target_w, target_h),
            screen_uv: pixel_to_uv(point[0], point[1], layer_w, layer_h),
            alpha,
            _pad: 0.0,
        });
    }

    let base = (vertices.len() - cleaned.len()) as u32;
    for i in 1..(cleaned.len() - 1) {
        indices.extend_from_slice(&[base, base + i as u32, base + (i as u32 + 1)]);
    }
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

fn append_ring(
    vertices: &mut Vec<CompositeVertex>,
    indices: &mut Vec<u32>,
    outer: &[[f32; 2]],
    inner: &[[f32; 2]],
    outer_alpha: f32,
    inner_alpha: f32,
    target_w: f32,
    target_h: f32,
    layer_w: f32,
    layer_h: f32,
) {
    let n = outer.len().min(inner.len());
    if n < 3 {
        return;
    }
    let base = vertices.len() as u32;
    for i in 0..n {
        let o = outer[i];
        let ii = inner[i];
        vertices.push(CompositeVertex {
            position: pixel_to_ndc(o[0], o[1], target_w, target_h),
            screen_uv: pixel_to_uv(o[0], o[1], layer_w, layer_h),
            alpha: outer_alpha,
            _pad: 0.0,
        });
        vertices.push(CompositeVertex {
            position: pixel_to_ndc(ii[0], ii[1], target_w, target_h),
            screen_uv: pixel_to_uv(ii[0], ii[1], layer_w, layer_h),
            alpha: inner_alpha,
            _pad: 0.0,
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

fn ndc_to_pixel(pos: [f32; 2], screen_w: f32, screen_h: f32) -> [f32; 2] {
    [((pos[0] + 1.0) * 0.5) * screen_w, ((1.0 - pos[1]) * 0.5) * screen_h]
}

fn pixel_to_uv(x: f32, y: f32, layer_w: f32, layer_h: f32) -> [f32; 2] {
    [x / layer_w, y / layer_h]
}

fn build_debug_overlay_geometry(
    vertices: &[CompositeVertex],
    indices: &[u32],
    screen_w: f32,
    screen_h: f32,
    edge_color: [f32; 4],
    point_color: [f32; 4],
) -> (Vec<DebugVertex>, Vec<u32>) {
    let mut out_vertices = Vec::new();
    let mut out_indices = Vec::new();
    if vertices.is_empty() || indices.len() < 3 {
        return (out_vertices, out_indices);
    }

    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in indices.chunks_exact(3) {
        let a = tri[0];
        let b = tri[1];
        let c = tri[2];
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let key = if u < v { (u, v) } else { (v, u) };
            edges.insert(key);
        }
    }

    for (u, v) in edges {
        let a = vertices[u as usize].position;
        let b = vertices[v as usize].position;
        append_debug_line_quad(
            &mut out_vertices,
            &mut out_indices,
            ndc_to_pixel(a, screen_w, screen_h),
            ndc_to_pixel(b, screen_w, screen_h),
            1.5,
            edge_color,
            screen_w,
            screen_h,
        );
    }

    for v in vertices {
        append_debug_point_quad(
            &mut out_vertices,
            &mut out_indices,
            ndc_to_pixel(v.position, screen_w, screen_h),
            2.5,
            point_color,
            screen_w,
            screen_h,
        );
    }

    (out_vertices, out_indices)
}

fn append_debug_line_quad(
    vertices: &mut Vec<DebugVertex>,
    indices: &mut Vec<u32>,
    p0: [f32; 2],
    p1: [f32; 2],
    thickness_px: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 1e-5 {
        return;
    }
    let nx = -dy / len;
    let ny = dx / len;
    let hw = thickness_px * 0.5;
    let o = [nx * hw, ny * hw];
    let quad = [
        [p0[0] + o[0], p0[1] + o[1]],
        [p0[0] - o[0], p0[1] - o[1]],
        [p1[0] - o[0], p1[1] - o[1]],
        [p1[0] + o[0], p1[1] + o[1]],
    ];
    append_debug_quad(vertices, indices, quad, color, screen_w, screen_h);
}

fn append_debug_point_quad(
    vertices: &mut Vec<DebugVertex>,
    indices: &mut Vec<u32>,
    center: [f32; 2],
    size_px: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let h = size_px * 0.5;
    let quad = [
        [center[0] - h, center[1] - h],
        [center[0] + h, center[1] - h],
        [center[0] + h, center[1] + h],
        [center[0] - h, center[1] + h],
    ];
    append_debug_quad(vertices, indices, quad, color, screen_w, screen_h);
}

fn append_debug_quad(
    vertices: &mut Vec<DebugVertex>,
    indices: &mut Vec<u32>,
    quad: [[f32; 2]; 4],
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let base = vertices.len() as u32;
    for p in quad {
        vertices.push(DebugVertex {
            position: pixel_to_ndc(p[0], p[1], screen_w, screen_h),
            color,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_rounded_points_zero_corner_keeps_fixed_topology() {
        let segments = 16;
        let pts = rounded_rect_points(0.0, 0.0, 100.0, 100.0, [0.0, 10.0, 10.0, 10.0], segments);
        assert_eq!(pts.len(), (segments * 4) as usize);
    }

    #[test]
    fn composite_tessellate_asymmetric_radius_produces_geometry() {
        let (vertices, indices) = tessellate_composite_layer(
            [0.0, 0.0],
            [150.0, 150.0],
            [10.0, 32.0, 10.0, 135.0],
            1.0,
            800.0,
            600.0,
            800.0,
            600.0,
        );
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
        assert_eq!(indices.len() % 3, 0);
    }

    #[test]
    fn composite_append_ring_tolerates_mismatched_topology() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let outer = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let inner = vec![[10.0, 10.0], [90.0, 10.0], [90.0, 90.0]];
        append_ring(
            &mut vertices,
            &mut indices,
            &outer,
            &inner,
            1.0,
            1.0,
            800.0,
            600.0,
            800.0,
            600.0,
        );
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }
}
