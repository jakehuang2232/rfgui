use crate::view::frame_graph::ResourceCache;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::texture_resource::TextureResource;
use crate::view::frame_graph::{
    GraphicsColorAttachmentDescriptor, GraphicsPassBuilder, GraphicsPassMergePolicy,
    GraphicsRecordContext,
};
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetTag};
use crate::view::render_pass::render_target::render_target_view;
use crate::view::render_pass::{GraphicsEncodeScope, GraphicsPass};
use std::sync::{Mutex, OnceLock};

const PRESENT_SURFACE_RESOURCES: u64 = 401;

pub struct PresentSurfacePass {
    params: PresentSurfaceParams,
    input: PresentSurfaceInput,
}

#[derive(Default)]
pub struct PresentSurfaceParams;

#[derive(Default)]
pub struct PresentSurfaceInput {
    pub source: RenderTargetIn,
}

#[derive(Default)]
pub struct PresentSurfaceOutput;

impl PresentSurfacePass {
    pub fn new(
        params: PresentSurfaceParams,
        input: PresentSurfaceInput,
        output: PresentSurfaceOutput,
    ) -> Self {
        let _ = output;
        Self { params, input }
    }
}

impl GraphicsPass for PresentSurfacePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::RequiresOwnPass);
        if let Some(handle) = self.input.source.handle() {
            let source: OutSlot<TextureResource, RenderTargetTag> = OutSlot::with_handle(handle);
            builder.read_texture(&mut self.input.source, &source);
        }
        builder.write_surface_color(GraphicsColorAttachmentDescriptor::clear(
            builder.surface_target(),
            [0.0, 0.0, 0.0, 0.0],
        ));
    }

    fn encode(
        &mut self,
        ctx: &mut GraphicsRecordContext<'_, '_>,
        scope: GraphicsEncodeScope<'_, '_>,
    ) {
        let _ = &self.params;
        let Some(input_handle) = self.input.source.handle() else {
            return;
        };
        let Some(src_view) = render_target_view(ctx, input_handle) else {
            return;
        };
        let Some(device) = ctx.viewport.device() else {
            return;
        };
        let format = ctx.viewport.surface_format();
        let cache = present_surface_resources_cache();
        let mut cache = cache.lock().unwrap();
        let resources = cache.get_or_insert_with(PRESENT_SURFACE_RESOURCES, || {
            PresentSurfaceResources::new(device, format)
        });
        if resources.pipeline_format != format {
            *resources = PresentSurfaceResources::new(device, format);
        }

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Present Surface Bind Group"),
            layout: &resources.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&resources.sampler),
                },
            ],
        });
        match scope {
            GraphicsEncodeScope::Render(pass) => {
                pass.set_pipeline(&resources.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            GraphicsEncodeScope::Command(encoder) => {
                let msaa_enabled = ctx.viewport.msaa_sample_count() > 1;
                let Some(parts) = ctx.viewport.frame_parts() else {
                    return;
                };
                let present_target = if msaa_enabled {
                    parts.resolve_view.unwrap_or(parts.view)
                } else {
                    parts.view
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Present Surface"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: present_target,
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
                pass.set_pipeline(&resources.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
    }
}

struct PresentSurfaceResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline_format: wgpu::TextureFormat,
}

impl PresentSurfaceResources {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Present Surface Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../shader/present_surface.wgsl").into(),
            ),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Present Surface BGL"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Present Surface Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Present Surface Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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
                ..Default::default()
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Present Surface Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            pipeline_format: format,
        }
    }
}

fn present_surface_resources_cache() -> &'static Mutex<ResourceCache<PresentSurfaceResources>> {
    static CACHE: OnceLock<Mutex<ResourceCache<PresentSurfaceResources>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ResourceCache::new()))
}

pub fn clear_present_surface_resources_cache() {
    let cache = present_surface_resources_cache();
    let mut cache = cache.lock().unwrap();
    cache.clear();
}
