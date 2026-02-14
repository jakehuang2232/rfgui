use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use crate::view::render_pass::render_target::render_target_view;
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
struct CompositeUniform {
    rect_pos_size: [f32; 4],
    screen_size_opacity: [f32; 4],
    corner_radii: [f32; 4],
}

struct CompositeLayerResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline_format: wgpu::TextureFormat,
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

        let device = match ctx.viewport.device() {
            Some(device) => device,
            None => return,
        };
        let format = ctx.viewport.surface_format();
        let resources = ctx
            .cache
            .get_or_insert_with::<CompositeLayerResources, _>(COMPOSITE_LAYER_RESOURCES, || {
                create_resources(device, format)
            });
        if resources.pipeline_format != format {
            *resources = create_resources(device, format);
        }

        let (screen_w, screen_h) = ctx.viewport.surface_size();
        let uniform = CompositeUniform {
            rect_pos_size: [
                self.rect_pos[0],
                self.rect_pos[1],
                self.rect_size[0],
                self.rect_size[1],
            ],
            screen_size_opacity: [screen_w as f32, screen_h as f32, self.opacity, 0.0],
            corner_radii: self.corner_radii,
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CompositeLayer Uniform Buffer"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

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
        if let Some([x, y, width, height]) = self.scissor_rect {
            pass.set_scissor_rect(x, y, width, height);
        }
        pass.set_pipeline(&resources.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..6, 0..1);
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
        CompositeLayerPass::set_scissor_rect(self, scissor_rect);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        CompositeLayerPass::set_color_target(self, color_target);
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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<CompositeUniform>() as u64,
                    ),
                },
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
            buffers: &[],
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
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    CompositeLayerResources {
        pipeline,
        bind_group_layout,
        sampler,
        pipeline_format: format,
    }
}
