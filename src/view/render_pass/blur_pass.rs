use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::composite_layer_pass::{LayerIn, LayerOut};
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use crate::view::render_pass::render_target::{render_target_size, render_target_view};
use wgpu::util::DeviceExt;

const BLUR_RESOURCES: u64 = 202;

pub struct BlurPass {
    blur_radius: f32,
    scissor_rect: Option<[u32; 4]>,
    color_target: Option<TextureHandle>,
    input: BlurInput,
    output: BlurOutput,
}

#[derive(Default)]
pub struct BlurInput {
    pub render_target: RenderTargetIn,
    pub layer: LayerIn,
}

#[derive(Default)]
pub struct BlurOutput {
    pub render_target: RenderTargetOut,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct BlurVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct BlurParams {
    texel_size: [f32; 2],
    radius: f32,
    _pad: f32,
}

struct BlurResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline_format: wgpu::TextureFormat,
}

impl BlurPass {
    pub fn new(layer: LayerOut, blur_radius: f32) -> Self {
        Self {
            blur_radius: blur_radius.max(0.0),
            scissor_rect: None,
            color_target: None,
            input: BlurInput {
                render_target: RenderTargetIn::default(),
                layer: InSlot::with_handle(layer.handle().unwrap()),
            },
            output: BlurOutput::default(),
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

impl RenderPass for BlurPass {
    type Input = BlurInput;
    type Output = BlurOutput;

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
        if layer_w == 0 || layer_h == 0 {
            return;
        }

        let device = match ctx.viewport.device() {
            Some(device) => device,
            None => return,
        };
        let format = ctx.viewport.surface_format();
        let resources = ctx
            .cache
            .get_or_insert_with::<BlurResources, _>(BLUR_RESOURCES, || {
                create_resources(device, format)
            });
        if resources.pipeline_format != format {
            *resources = create_resources(device, format);
        }

        let vertices = fullscreen_quad_vertices();
        let indices = fullscreen_quad_indices();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let params = BlurParams {
            texel_size: [1.0 / layer_w as f32, 1.0 / layer_h as f32],
            radius: self.blur_radius.max(0.0),
            _pad: 0.0,
        };
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur Params Buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur Bind Group"),
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });

        let scissor_rect_physical = self.scissor_rect.and_then(|scissor_rect| {
            ctx.viewport
                .logical_scissor_to_physical(scissor_rect, (target_w, target_h))
        });

        let parts = match ctx.viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
        };
        let color_view = offscreen_view.as_ref().unwrap_or(parts.view);

        let mut pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blur"),
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
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
}

impl RenderTargetPass for BlurPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        BlurPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        BlurPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.scissor_rect = intersect_scissor_rects(self.scissor_rect, scissor_rect);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        BlurPass::set_color_target(self, color_target);
    }
}

fn create_resources(device: &wgpu::Device, format: wgpu::TextureFormat) -> BlurResources {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Blur Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/layer_blur.wgsl").into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Blur Bind Group Layout"),
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

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Blur Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Blur Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Blur Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<BlurVertex>() as u64,
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
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    });

    BlurResources {
        pipeline,
        bind_group_layout,
        sampler,
        pipeline_format: format,
    }
}

fn fullscreen_quad_vertices() -> [BlurVertex; 4] {
    [
        BlurVertex {
            position: [-1.0, -1.0],
            uv: [0.0, 1.0],
        },
        BlurVertex {
            position: [1.0, -1.0],
            uv: [1.0, 1.0],
        },
        BlurVertex {
            position: [1.0, 1.0],
            uv: [1.0, 0.0],
        },
        BlurVertex {
            position: [-1.0, 1.0],
            uv: [0.0, 0.0],
        },
    ]
}

fn fullscreen_quad_indices() -> [u16; 6] {
    [0, 1, 2, 0, 2, 3]
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
