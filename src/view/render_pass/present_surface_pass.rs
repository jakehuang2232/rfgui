use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::texture_resource::TextureResource;
use crate::view::frame_graph::{
    GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy,
};
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetTag};
use crate::view::render_pass::render_target::{render_target_ref, render_target_view};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use wgpu::util::DeviceExt;

const PRESENT_SURFACE_RESOURCES: u64 = 401;

pub struct PresentSurfacePass {
    params: PresentSurfaceParams,
    input: PresentSurfaceInput,
    uniform: PresentSurfaceUniform,
}

#[derive(Default)]
pub struct PresentSurfaceParams;

#[derive(Default)]
pub struct PresentSurfaceInput {
    pub source: RenderTargetIn,
}

#[derive(Default)]
pub struct PresentSurfaceOutput;

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct PresentSurfaceUniform {
    uv_offset: [f32; 2],
    uv_scale: [f32; 2],
}

impl PresentSurfacePass {
    pub fn new(
        params: PresentSurfaceParams,
        input: PresentSurfaceInput,
        output: PresentSurfaceOutput,
    ) -> Self {
        let _ = output;
        Self {
            params,
            input,
            uniform: PresentSurfaceUniform::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_snapshot(&self) -> PresentSurfacePassTestSnapshot {
        PresentSurfacePassTestSnapshot {
            source_handle: self.input.source.handle(),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PresentSurfacePassTestSnapshot {
    pub(crate) source_handle: Option<crate::view::frame_graph::texture_resource::TextureHandle>,
}

impl GraphicsPass for PresentSurfacePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::RequiresOwnPass);
        if let Some(handle) = self.input.source.handle() {
            let source: OutSlot<TextureResource, RenderTargetTag> = OutSlot::with_handle(handle);
            builder.read_texture(&mut self.input.source, &source);
        }
        builder.write_surface_color(GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]));
    }

    fn prepare(&mut self, ctx: &mut crate::view::frame_graph::PrepareContext<'_, '_>) {
        let Some(handle) = self.input.source.handle() else {
            return;
        };
        let Some(texture_ref) = render_target_ref(ctx, handle) else {
            return;
        };
        self.uniform = PresentSurfaceUniform {
            uv_offset: [texture_ref.uv_offset_x(), texture_ref.uv_offset_y()],
            uv_scale: [texture_ref.uv_scale_x(), texture_ref.uv_scale_y()],
        };
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        let _ = &self.params;
        let Some(input_handle) = self.input.source.handle() else {
            return;
        };
        let Some(src_view) = render_target_view(ctx.frame_resources(), input_handle) else {
            return;
        };
        let Some(device) = ctx.viewport().device().cloned() else {
            return;
        };
        let format = ctx.viewport().surface_format();
        with_present_surface_resources_cache(|cache| {
            let resources = cache.get_or_insert_scoped_with(
                ctx.viewport().render_resource_scope_id(),
                PRESENT_SURFACE_RESOURCES,
                || PresentSurfaceResources::new(&device, format),
            );
            if resources.pipeline_format != format {
                *resources = PresentSurfaceResources::new(&device, format);
            }

            let bind_group = resources.binding(&device, &src_view, self.uniform);
            ctx.set_pipeline(&resources.pipeline);
            ctx.set_bind_group(0, &bind_group, &[]);
            ctx.draw(0..3, 0..1);
            crate::view::render_pass::debug_overlay_pass::draw_debug_overlay(ctx, None);
        });
    }
}

struct PresentSurfaceResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline_format: wgpu::TextureFormat,
    bindings: Vec<PresentSurfaceBinding>,
}

struct PresentSurfaceBinding {
    source: wgpu::TextureView,
    uniform_bits: [u32; 4],
    group: wgpu::BindGroup,
}

impl PresentSurfaceResources {
    fn binding(
        &mut self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        uniform: PresentSurfaceUniform,
    ) -> wgpu::BindGroup {
        let uniform_bits = [
            uniform.uv_offset[0],
            uniform.uv_offset[1],
            uniform.uv_scale[0],
            uniform.uv_scale[1],
        ]
        .map(f32::to_bits);
        if let Some(index) = self
            .bindings
            .iter()
            .position(|entry| entry.source == *source && entry.uniform_bits == uniform_bits)
        {
            let entry = self.bindings.remove(index);
            let group = entry.group.clone();
            self.bindings.push(entry);
            return group;
        }
        // These UV values are immutable for this binding. Reuse across frames
        // avoids a per-frame staging copy and frame-graph buffer allocation.
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Present Surface Uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Present Surface Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        crate::ui::work_profile::count(|p| p.present_bind_group_creations += 1);
        // A small bounded LRU covers rotating frame targets without retaining
        // old resized targets indefinitely. The bind group owns its buffer ref.
        if self.bindings.len() == 8 {
            self.bindings.remove(0);
        }
        self.bindings.push(PresentSurfaceBinding {
            source: source.clone(),
            uniform_bits,
            group: group.clone(),
        });
        group
    }

    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Present Surface Shader"),
            source: wgpu::ShaderSource::Wgsl(present_surface_shader_source().into()),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            std::num::NonZeroU64::new(
                                std::mem::size_of::<PresentSurfaceUniform>() as u64
                            )
                            .unwrap(),
                        ),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Present Surface Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
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
            bindings: Vec::new(),
        }
    }
}

fn present_surface_shader_source() -> &'static str {
    include_str!("../../shader/present_surface.wgsl")
}

crate::static_resource_cache! {
    fn with_present_surface_resources_cache -> ResourceCache<PresentSurfaceResources>
        = stats("present_surface_pipeline")
}

pub fn clear_present_surface_resources_cache() {
    with_present_surface_resources_cache(|cache| {
        cache.clear();
    });
}

pub(super) fn release_scope(scope: u64) {
    with_present_surface_resources_cache(|cache| cache.clear_scope(scope));
}
