use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetTag};
use crate::view::render_pass::render_target::render_target_view;

const PRESENT_SURFACE_RESOURCES: u64 = 401;

pub struct PresentSurfacePass {
    source_color_target: Option<TextureHandle>,
    input: PresentSurfaceInput,
    output: PresentSurfaceOutput,
}

#[derive(Default)]
pub struct PresentSurfaceInput {
    pub render_target: RenderTargetIn,
}

#[derive(Default)]
pub struct PresentSurfaceOutput {}

impl PresentSurfacePass {
    pub fn new() -> Self {
        Self {
            source_color_target: None,
            input: PresentSurfaceInput::default(),
            output: PresentSurfaceOutput::default(),
        }
    }

    pub fn set_input(&mut self, input: RenderTargetIn) {
        self.input.render_target = input;
    }

    pub fn set_source_color_target(&mut self, source_color_target: Option<TextureHandle>) {
        self.source_color_target = source_color_target;
    }
}

impl RenderPass for PresentSurfacePass {
    type Input = PresentSurfaceInput;
    type Output = PresentSurfaceOutput;

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
    }

    fn execute(&mut self, ctx: &mut PassContext<'_, '_>) {
        let src_view = if let Some(handle) = self.source_color_target {
            let Some(view) = render_target_view(ctx, handle) else {
                return;
            };
            view
        } else {
            let Some(input_handle) = self.input.render_target.handle() else {
                return;
            };
            let Some(view) = render_target_view(ctx, input_handle) else {
                return;
            };
            view
        };
        let Some(device) = ctx.viewport.device() else {
            return;
        };
        let format = ctx.viewport.surface_format();
        let resources = ctx
            .cache
            .get_or_insert_with::<PresentSurfaceResources, _>(PRESENT_SURFACE_RESOURCES, || {
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
        let Some(parts) = ctx.viewport.frame_parts() else {
            return;
        };
        let mut pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Present Surface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: parts.view,
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
            multisample: wgpu::MultisampleState::default(),
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
