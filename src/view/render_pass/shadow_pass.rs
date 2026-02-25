use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use crate::view::render_pass::render_target::{render_target_size, render_target_view};
use wgpu::util::DeviceExt;

const SHADOW_RESOURCES: u64 = 203;

#[derive(Clone, Debug, Default)]
pub struct ShadowMesh {
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl ShadowMesh {
    pub fn new(vertices: Vec<[f32; 2]>, indices: Vec<u32>) -> Self {
        Self { vertices, indices }
    }

    pub fn rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Self {
        let w = width.max(0.0);
        let h = height.max(0.0);
        if w <= 0.0 || h <= 0.0 {
            return Self::default();
        }
        let r = radius.max(0.0).min(w * 0.5).min(h * 0.5);
        if r <= 0.001 {
            return Self {
                vertices: vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                indices: vec![0, 1, 2, 0, 2, 3],
            };
        }
        const ARC_SEGMENTS: usize = 6;
        let mut ring = Vec::with_capacity(ARC_SEGMENTS * 4 + 4);
        append_arc(
            &mut ring,
            [x + w - r, y + r],
            r,
            -std::f32::consts::FRAC_PI_2,
            0.0,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + w - r, y + h - r],
            r,
            0.0,
            std::f32::consts::FRAC_PI_2,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + r, y + h - r],
            r,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + r, y + r],
            r,
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
            ARC_SEGMENTS,
        );

        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let mut vertices = Vec::with_capacity(ring.len() + 1);
        vertices.push([cx, cy]);
        vertices.extend(ring.iter().copied());

        let mut indices = Vec::with_capacity(ring.len() * 3);
        let ring_start = 1_u32;
        let ring_len = ring.len() as u32;
        for i in 0..ring_len {
            let a = ring_start + i;
            let b = ring_start + ((i + 1) % ring_len);
            indices.extend_from_slice(&[0, a, b]);
        }
        Self { vertices, indices }
    }
}

fn append_arc(
    out: &mut Vec<[f32; 2]>,
    center: [f32; 2],
    radius: f32,
    start: f32,
    end: f32,
    segments: usize,
) {
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let a = start + (end - start) * t;
        out.push([center[0] + radius * a.cos(), center[1] + radius * a.sin()]);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShadowParams {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: [f32; 4],
    pub opacity: f32,
    pub spread: f32,
    pub clip_to_geometry: bool,
}

impl Default for ShadowParams {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            blur_radius: 0.0,
            color: [0.0, 0.0, 0.0, 1.0],
            opacity: 1.0,
            spread: 0.0,
            clip_to_geometry: false,
        }
    }
}

pub struct ShadowPass {
    mesh: ShadowMesh,
    params: ShadowParams,
    scissor_rect: Option<[u32; 4]>,
    color_target: Option<TextureHandle>,
    input: ShadowInput,
    output: ShadowOutput,
}

#[derive(Default)]
pub struct ShadowInput {
    pub render_target: RenderTargetIn,
}

#[derive(Default)]
pub struct ShadowOutput {
    pub render_target: RenderTargetOut,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct FillVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct QuadVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct BlurParamsUniform {
    texel_size: [f32; 2],
    direction: [f32; 2],
    radius: f32,
    sigma: f32,
    _pad: [f32; 2],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct CompositeParamsUniform {
    use_mask: f32,
    _pad: [f32; 3],
}

struct ShadowResources {
    fill_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    composite_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline_format: wgpu::TextureFormat,
}

impl ShadowPass {
    pub fn new(mesh: ShadowMesh, params: ShadowParams) -> Self {
        Self {
            mesh,
            params,
            scissor_rect: None,
            color_target: None,
            input: ShadowInput::default(),
            output: ShadowOutput::default(),
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

impl RenderPass for ShadowPass {
    type Input = ShadowInput;
    type Output = ShadowOutput;

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
        if self.mesh.vertices.len() < 3 || self.mesh.indices.len() < 3 {
            return;
        }

        let offscreen_view = match self.color_target {
            Some(handle) => render_target_view(ctx, handle),
            None => None,
        };
        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = match self.color_target {
            Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
            None => surface_size,
        };
        if target_w == 0 || target_h == 0 {
            return;
        }

        let scale = ctx.viewport.scale_factor().max(0.0001);
        let base_vertices = self
            .mesh
            .vertices
            .iter()
            .map(|[x, y]| [x * scale, y * scale])
            .collect::<Vec<_>>();
        let mut shadow_vertices = base_vertices.clone();
        apply_spread(
            &mut shadow_vertices,
            (self.params.spread * scale).max(0.0),
        );
        let offset = [self.params.offset_x * scale, self.params.offset_y * scale];
        for v in &mut shadow_vertices {
            v[0] += offset[0];
            v[1] += offset[1];
        }

        let Some((min_x, min_y, max_x, max_y)) = bounds(&shadow_vertices) else {
            return;
        };
        let blur_padding = ((self.params.blur_radius.max(0.0) * scale) * 1.5).ceil() as i32;
        let bx = (min_x.floor() as i32 - blur_padding).max(0);
        let by = (min_y.floor() as i32 - blur_padding).max(0);
        let br = (max_x.ceil() as i32 + blur_padding).min(target_w as i32);
        let bb = (max_y.ceil() as i32 + blur_padding).min(target_h as i32);
        if br <= bx || bb <= by {
            return;
        }
        let bw = (br - bx) as u32;
        let bh = (bb - by) as u32;
        if bw == 0 || bh == 0 {
            return;
        }

        let device = match ctx.viewport.device() {
            Some(device) => device,
            None => return,
        };
        let format = ctx.viewport.surface_format();
        let (
            fill_pipeline,
            blur_pipeline,
            composite_pipeline,
            blur_bind_group_layout,
            composite_bind_group_layout,
            sampler,
        ) = {
            let resources = ctx
                .cache
                .get_or_insert_with::<ShadowResources, _>(SHADOW_RESOURCES, || {
                    create_resources(device, format)
                });
            if resources.pipeline_format != format {
                *resources = create_resources(device, format);
            }
            (
                resources.fill_pipeline.clone(),
                resources.blur_pipeline.clone(),
                resources.composite_pipeline.clone(),
                resources.blur_bind_group_layout.clone(),
                resources.composite_bind_group_layout.clone(),
                resources.sampler.clone(),
            )
        };

        let shadow_tex_a = create_temp_texture(device, bw, bh, format, "Shadow A");
        let shadow_tex_a_view = shadow_tex_a.create_view(&wgpu::TextureViewDescriptor::default());
        let shadow_tex_b = create_temp_texture(device, bw, bh, format, "Shadow B");
        let shadow_tex_b_view = shadow_tex_b.create_view(&wgpu::TextureViewDescriptor::default());
        let mask_tex = create_temp_texture(device, bw, bh, format, "Shadow Mask");
        let mask_tex_view = mask_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let fill_color = {
            let a = (self.params.color[3] * self.params.opacity).clamp(0.0, 1.0);
            [self.params.color[0], self.params.color[1], self.params.color[2], a]
        };
        draw_mesh_fill(
            ctx,
            &fill_pipeline,
            &shadow_tex_a_view,
            bx as f32,
            by as f32,
            bw as f32,
            bh as f32,
            &shadow_vertices,
            &self.mesh.indices,
            fill_color,
            [0.0, 0.0, 0.0, 0.0],
        );

        if self.params.clip_to_geometry {
            draw_mesh_fill(
                ctx,
                &fill_pipeline,
                &mask_tex_view,
                bx as f32,
                by as f32,
                bw as f32,
                bh as f32,
                &base_vertices,
                &self.mesh.indices,
                [1.0, 1.0, 1.0, 1.0],
                [0.0, 0.0, 0.0, 0.0],
            );
        } else {
            clear_target(ctx, &mask_tex_view, [1.0, 1.0, 1.0, 1.0]);
        }

        let blur_radius_px = (self.params.blur_radius.max(0.0) * scale).max(0.0);
        let shadow_output_view = if blur_radius_px > 0.001 {
            blur_texture(
                ctx,
                &blur_pipeline,
                &blur_bind_group_layout,
                &sampler,
                &shadow_tex_a_view,
                &shadow_tex_b_view,
                bw,
                bh,
                blur_radius_px,
                [1.0, 0.0],
            );
            blur_texture(
                ctx,
                &blur_pipeline,
                &blur_bind_group_layout,
                &sampler,
                &shadow_tex_b_view,
                &shadow_tex_a_view,
                bw,
                bh,
                blur_radius_px,
                [0.0, 1.0],
            );
            &shadow_tex_a_view
        } else {
            &shadow_tex_a_view
        };

        let scissor_rect_physical = self.scissor_rect.and_then(|scissor_rect| {
            ctx.viewport
                .logical_scissor_to_physical(scissor_rect, (target_w, target_h))
        });
        composite_shadow(
            ctx,
            &composite_pipeline,
            &composite_bind_group_layout,
            &sampler,
            shadow_output_view,
            &mask_tex_view,
            offscreen_view.as_ref(),
            (target_w, target_h),
            [bx as f32, by as f32, bw as f32, bh as f32],
            scissor_rect_physical,
            self.params.clip_to_geometry,
        );
    }
}

impl RenderTargetPass for ShadowPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        ShadowPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        ShadowPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.scissor_rect = intersect_scissor_rects(self.scissor_rect, scissor_rect);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        ShadowPass::set_color_target(self, color_target);
    }
}

fn create_resources(device: &wgpu::Device, format: wgpu::TextureFormat) -> ShadowResources {
    let fill_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Fill Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/shadow_fill.wgsl").into()),
    });
    let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Blur Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/shadow_blur.wgsl").into()),
    });
    let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Composite Shader"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../shader/shadow_composite.wgsl").into(),
        ),
    });

    let fill_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shadow Fill Pipeline Layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let fill_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Shadow Fill Pipeline"),
        layout: Some(&fill_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &fill_shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<FillVertex>() as u64,
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
            module: &fill_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
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
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let blur_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Shadow Blur Bind Group Layout"),
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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let composite_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow Composite Bind Group Layout"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Shadow Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let blur_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shadow Blur Pipeline Layout"),
        bind_group_layouts: &[&blur_bind_group_layout],
        immediate_size: 0,
    });
    let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Shadow Blur Pipeline"),
        layout: Some(&blur_layout),
        vertex: wgpu::VertexState {
            module: &blur_shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadVertex>() as u64,
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
                ],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &blur_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shadow Composite Pipeline Layout"),
        bind_group_layouts: &[&composite_bind_group_layout],
        immediate_size: 0,
    });
    let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Shadow Composite Pipeline"),
        layout: Some(&composite_layout),
        vertex: wgpu::VertexState {
            module: &composite_shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadVertex>() as u64,
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
                ],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &composite_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
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
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    ShadowResources {
        fill_pipeline,
        blur_pipeline,
        composite_pipeline,
        blur_bind_group_layout,
        composite_bind_group_layout,
        sampler,
        pipeline_format: format,
    }
}

fn create_temp_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn clear_target(ctx: &mut PassContext<'_, '_>, view: &wgpu::TextureView, color: [f32; 4]) {
    let Some(parts) = ctx.viewport.frame_parts() else {
        return;
    };
    let _ = parts
        .encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Clear Temp"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: color[0] as f64,
                        g: color[1] as f64,
                        b: color[2] as f64,
                        a: color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_mesh_fill(
    ctx: &mut PassContext<'_, '_>,
    pipeline: &wgpu::RenderPipeline,
    view: &wgpu::TextureView,
    bx: f32,
    by: f32,
    bw: f32,
    bh: f32,
    vertices: &[[f32; 2]],
    indices: &[u32],
    color: [f32; 4],
    clear_color: [f32; 4],
) {
    if bw <= 0.0 || bh <= 0.0 {
        return;
    }
    let Some(device) = ctx.viewport.device() else {
        return;
    };
    let mut fill_vertices = Vec::with_capacity(vertices.len());
    for [x, y] in vertices {
        let local_x = ((x - bx) / bw).clamp(0.0, 1.0);
        let local_y = ((y - by) / bh).clamp(0.0, 1.0);
        fill_vertices.push(FillVertex {
            position: [local_x * 2.0 - 1.0, 1.0 - local_y * 2.0],
            color,
        });
    }
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Fill Vertex Buffer"),
        contents: bytemuck::cast_slice(&fill_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Fill Index Buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let Some(parts) = ctx.viewport.frame_parts() else {
        return;
    };
    let mut pass = parts
        .encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Fill"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear_color[0] as f64,
                        g: clear_color[1] as f64,
                        b: clear_color[2] as f64,
                        a: clear_color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
    pass.set_pipeline(pipeline);
    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
}

fn blur_texture(
    ctx: &mut PassContext<'_, '_>,
    blur_pipeline: &wgpu::RenderPipeline,
    blur_bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    input_view: &wgpu::TextureView,
    output_view: &wgpu::TextureView,
    width: u32,
    height: u32,
    blur_radius_px: f32,
    direction: [f32; 2],
) {
    let Some(device) = ctx.viewport.device() else {
        return;
    };
    let sigma = (blur_radius_px * 0.5).max(0.001);
    let params = BlurParamsUniform {
        texel_size: [1.0 / width.max(1) as f32, 1.0 / height.max(1) as f32],
        direction,
        radius: blur_radius_px.max(0.0),
        sigma,
        _pad: [0.0, 0.0],
    };
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Blur Params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Shadow Blur Bind Group"),
        layout: blur_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(input_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params_buffer.as_entire_binding(),
            },
        ],
    });
    let (vertices, indices) = fullscreen_quad();
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Blur Vertex"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Blur Index"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let Some(parts) = ctx.viewport.frame_parts() else {
        return;
    };
    let mut pass = parts
        .encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Blur"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
    pass.set_pipeline(blur_pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
}

#[allow(clippy::too_many_arguments)]
fn composite_shadow(
    ctx: &mut PassContext<'_, '_>,
    composite_pipeline: &wgpu::RenderPipeline,
    composite_bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    shadow_view: &wgpu::TextureView,
    mask_view: &wgpu::TextureView,
    offscreen_view: Option<&wgpu::TextureView>,
    target_size: (u32, u32),
    bounds: [f32; 4],
    scissor_rect_physical: Option<[u32; 4]>,
    clip_to_geometry: bool,
) {
    let Some(device) = ctx.viewport.device() else {
        return;
    };
    let params = CompositeParamsUniform {
        use_mask: if clip_to_geometry { 1.0 } else { 0.0 },
        _pad: [0.0; 3],
    };
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Composite Params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Shadow Composite Bind Group"),
        layout: composite_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(mask_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: params_buffer.as_entire_binding(),
            },
        ],
    });
    let (quad_vertices, quad_indices) =
        quad_for_bounds(bounds, target_size.0 as f32, target_size.1 as f32);
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Composite Vertex"),
        contents: bytemuck::cast_slice(&quad_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Composite Index"),
        contents: bytemuck::cast_slice(&quad_indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let Some(parts) = ctx.viewport.frame_parts() else {
        return;
    };
    let target_view = offscreen_view.unwrap_or(parts.view);
    let mut pass = parts
        .encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
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
    if let Some([x, y, w, h]) = scissor_rect_physical {
        pass.set_scissor_rect(x, y, w, h);
    }
    pass.set_pipeline(composite_pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    pass.draw_indexed(0..quad_indices.len() as u32, 0, 0..1);
}

fn fullscreen_quad() -> ([QuadVertex; 4], [u16; 6]) {
    (
        [
            QuadVertex {
                position: [-1.0, -1.0],
                uv: [0.0, 1.0],
            },
            QuadVertex {
                position: [1.0, -1.0],
                uv: [1.0, 1.0],
            },
            QuadVertex {
                position: [1.0, 1.0],
                uv: [1.0, 0.0],
            },
            QuadVertex {
                position: [-1.0, 1.0],
                uv: [0.0, 0.0],
            },
        ],
        [0, 1, 2, 0, 2, 3],
    )
}

fn quad_for_bounds(bounds: [f32; 4], target_w: f32, target_h: f32) -> ([QuadVertex; 4], [u16; 6]) {
    let x = bounds[0];
    let y = bounds[1];
    let w = bounds[2];
    let h = bounds[3];
    let left = (x / target_w) * 2.0 - 1.0;
    let right = ((x + w) / target_w) * 2.0 - 1.0;
    let top = 1.0 - (y / target_h) * 2.0;
    let bottom = 1.0 - ((y + h) / target_h) * 2.0;
    (
        [
            QuadVertex {
                position: [left, bottom],
                uv: [0.0, 1.0],
            },
            QuadVertex {
                position: [right, bottom],
                uv: [1.0, 1.0],
            },
            QuadVertex {
                position: [right, top],
                uv: [1.0, 0.0],
            },
            QuadVertex {
                position: [left, top],
                uv: [0.0, 0.0],
            },
        ],
        [0, 1, 2, 0, 2, 3],
    )
}

fn bounds(vertices: &[[f32; 2]]) -> Option<(f32, f32, f32, f32)> {
    if vertices.is_empty() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for [x, y] in vertices {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return None;
    }
    Some((min_x, min_y, max_x, max_y))
}

fn apply_spread(vertices: &mut [[f32; 2]], spread: f32) {
    if spread.abs() <= 0.0001 || vertices.is_empty() {
        return;
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    for [x, y] in vertices.iter() {
        cx += *x;
        cy += *y;
    }
    let inv = 1.0 / vertices.len() as f32;
    cx *= inv;
    cy *= inv;
    for v in vertices.iter_mut() {
        let dx = v[0] - cx;
        let dy = v[1] - cy;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.0001 {
            continue;
        }
        v[0] += (dx / len) * spread;
        v[1] += (dy / len) * spread;
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
