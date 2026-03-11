use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::{
    FrameResourceContext, GraphicsColorAttachmentDescriptor, GraphicsRecordContext, PassBuilder,
    PrepareContext,
};
use crate::view::frame_graph::ResourceCache;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::{BufferDesc, BufferResource};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::composite_layer_pass::LayerIn;
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{render_target_size, render_target_view};
use std::sync::{Mutex, OnceLock};
use wgpu::util::DeviceExt;

const BLUR_RESOURCES: u64 = 202;

pub struct BlurPass {
    params: BlurPassParams,
    upload_buffer: BlurBufferOut,
    input: BlurInput,
    output: BlurOutput,
}

pub struct BlurPassParams {
    pub blur_radius: f32,
    pub scissor_rect: Option<[u32; 4]>,
}

impl BlurPassParams {
    pub fn new(blur_radius: f32) -> Self {
        Self {
            blur_radius,
            scissor_rect: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct BlurBufferTag;
pub type BlurBufferOut = OutSlot<BufferResource, BlurBufferTag>;

#[derive(Default)]
pub struct BlurInput {
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
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    pipeline_format: wgpu::TextureFormat,
}

impl BlurPass {
    pub fn new(params: BlurPassParams, input: BlurInput, output: BlurOutput) -> Self {
        Self {
            params: BlurPassParams {
                blur_radius: params.blur_radius.max(0.0),
                scissor_rect: params.scissor_rect,
            },
            upload_buffer: BlurBufferOut::default(),
            input,
            output,
        }
    }
}

impl RenderPass for BlurPass {
    fn setup(&mut self, builder: &mut PassBuilder<'_>) {
        self.upload_buffer = builder.create_buffer(BufferDesc {
            size: 64 * 1024,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("BlurPass Upload Buffer"),
        });
        builder.declare_uniform_buffer(&self.upload_buffer);
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            builder.declare_color_attachment(
                &self.output.render_target,
                GraphicsColorAttachmentDescriptor::load(target),
            );
        } else {
            builder.declare_surface_color_attachment(GraphicsColorAttachmentDescriptor::load(
                builder.surface_target(),
            ));
        }
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        let Some(layer_handle) = self.input.layer.handle() else {
            return;
        };
        let surface_size = ctx.viewport.surface_size();
        let (layer_w, layer_h) = render_target_size(ctx, layer_handle).unwrap_or(surface_size);
        if layer_w == 0 || layer_h == 0 {
            return;
        }
        let params = BlurParams {
            texel_size: [1.0 / layer_w as f32, 1.0 / layer_h as f32],
            radius: self.params.blur_radius.max(0.0),
            _pad: 0.0,
        };
        if let Some(handle) = self.upload_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::bytes_of(&params));
        }
    }

    fn record(&mut self, ctx: &mut GraphicsRecordContext<'_, '_, '_>) {
        let Some(layer_handle) = self.input.layer.handle() else {
            return;
        };
        let Some(layer_view) = render_target_view(ctx, layer_handle) else {
            return;
        };
        let offscreen_view = match self.output.render_target.handle() {
            Some(handle) => render_target_view(ctx, handle),
            None => None,
        };

        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = match self.output.render_target.handle() {
            Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
            None => surface_size,
        };
        let (layer_w, layer_h) = render_target_size(ctx, layer_handle).unwrap_or(surface_size);
        if layer_w == 0 || layer_h == 0 {
            return;
        }
        let Some(params_handle) = self.upload_buffer.handle() else {
            return;
        };
        let Some(params_buffer) = ctx.acquire_buffer(params_handle) else {
            return;
        };

        let device = match ctx.viewport.device() {
            Some(device) => device,
            None => return,
        };
        let format = ctx.viewport.surface_format();
        let cache = blur_resources_cache();
        let mut cache = cache.lock().unwrap();
        let resources =
            cache.get_or_insert_with(BLUR_RESOURCES, || create_resources(device, format));
        if resources.pipeline_format != format {
            *resources = create_resources(device, format);
        }

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

        let scissor_rect_physical = self.params.scissor_rect.and_then(|scissor_rect| {
            ctx.viewport
                .logical_scissor_to_physical(scissor_rect, (target_w, target_h))
        });

        let msaa_enabled = ctx.viewport.msaa_sample_count() > 1;
        let parts = match ctx.viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
        };
        let surface_color_view = if msaa_enabled {
            parts.resolve_view.unwrap_or(parts.view)
        } else {
            parts.view
        };
        let color_view = offscreen_view.as_ref().unwrap_or(surface_color_view);

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
        pass.set_vertex_buffer(0, resources.vertex_buffer.slice(..));
        pass.set_index_buffer(resources.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..resources.index_count, 0, 0..1);
    }
}

impl RenderTargetPass for BlurPass {
    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.params.scissor_rect = intersect_scissor_rects(self.params.scissor_rect, scissor_rect);
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

    BlurResources {
        pipeline,
        bind_group_layout,
        sampler,
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
        pipeline_format: format,
    }
}

fn blur_resources_cache() -> &'static Mutex<ResourceCache<BlurResources>> {
    static CACHE: OnceLock<Mutex<ResourceCache<BlurResources>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ResourceCache::new()))
}

pub fn clear_blur_resources_cache() {
    let cache = blur_resources_cache();
    let mut cache = cache.lock().unwrap();
    cache.clear();
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
