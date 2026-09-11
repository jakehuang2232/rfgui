use super::*;
use crate::view::frame_graph::{GraphicsColorAttachmentOps, GraphicsPassBuilder};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use wgpu::util::DeviceExt;

// DrawRect no longer consumes buffers. Keep the generic GraphicsCtx cache
// gate using an actual indexed mesh, shared across 256 logical passes, with
// distinct dynamic uniform offsets and independently expected red/blue pixels.
pub(super) struct Resources {
    pipeline: wgpu::RenderPipeline,
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    group: wgpu::BindGroup,
    stride: u32,
}

impl Resources {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let stride = device.limits().min_uniform_buffer_offset_alignment.max(16);
        let mut colors = vec![0_u8; stride as usize * 256];
        for row in 0..16 {
            for col in 0..16 {
                let color: [f32; 4] = if (row + col) % 2 == 0 {
                    [1.0, 0.0, 0.0, 1.0]
                } else {
                    [0.0, 0.0, 1.0, 1.0]
                };
                let offset = (row * 16 + col) * stride as usize;
                colors[offset..offset + 16].copy_from_slice(bytemuck::cast_slice(&color));
            }
        }
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indexed grid colors"),
            contents: &colors,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform,
                    offset: 0,
                    size: wgpu::BufferSize::new(16),
                }),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None, source: wgpu::ShaderSource::Wgsl(r#"
                @group(0) @binding(0) var<uniform> color: vec4f;
                @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }
                @fragment fn fs() -> @location(0) vec4f { return color; }
            "#.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("indexed grid"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indexed grid quad"),
            contents: bytemuck::cast_slice(&[
                [-1.0_f32, -1.0],
                [1.0, -1.0],
                [1.0, 1.0],
                [-1.0, 1.0],
            ]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indexed grid indices"),
            contents: bytemuck::cast_slice(&[0_u16, 1, 2, 0, 2, 3]),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            pipeline,
            vertex,
            index,
            group,
            stride,
        }
    }
}

pub(super) struct Pass {
    pub(super) resources: std::sync::Arc<Resources>,
    pub(super) target: RenderTargetOut,
    pub(super) row: u32,
    pub(super) column: u32,
    pub(super) dpr: u32,
}

impl GraphicsPass for Pass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(
            crate::view::frame_graph::GraphicsPassMergePolicy::Mergeable,
        );
        builder.write_color(&self.target, GraphicsColorAttachmentOps::load());
    }
    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        ctx.set_pipeline(&self.resources.pipeline);
        ctx.set_vertex_buffer(0, self.resources.vertex.slice(..));
        ctx.set_index_buffer(self.resources.index.slice(..), wgpu::IndexFormat::Uint16);
        ctx.set_bind_group(
            0,
            &self.resources.group,
            &[(self.row * 16 + self.column) * self.resources.stride],
        );
        ctx.set_scissor_rect(
            self.column * 4 * self.dpr,
            self.row * 4 * self.dpr,
            4 * self.dpr,
            4 * self.dpr,
        );
        ctx.draw_indexed(0..6, 0, 0..1);
    }
}
