use super::*;
use crate::view::base_component::UiBuildContext;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::{
    BufferDesc, BufferReadUsage, BufferResource, FrameGraph, FrameResourceContext,
    GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy, PrepareContext,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams, TextureCompositePass,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};

impl GpuPaintSource {
    pub(crate) fn emit(
        &self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        params: TextureCompositeParams,
    ) {
        let output = if let Some((source, handle)) = graph.gpu_paint_sources.get(&self.id.get()) {
            assert_eq!(
                source, self,
                "a source identity cannot describe two payloads in one graph"
            );
            OutSlot::with_handle(*handle)
        } else {
            let output = graph.declare_persistent_texture_internal(self.descriptor(), self.key());
            graph
                .gpu_paint_sources
                .insert(self.id.get(), (self.clone(), output.handle().unwrap()));
            graph.add_graphics_pass(SourcePass {
                source: self.clone(),
                output,
                uniform: Default::default(),
                vertices: Default::default(),
                uploaded: false,
                reuse: false,
                resources: None,
            });
            output
        };
        let target = ctx.current_target().unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
        graph.add_graphics_pass(TextureCompositePass::new(
            params,
            TextureCompositeInput::from_render_target(
                InSlot::with_handle(output.handle().expect("declared source")),
                Default::default(),
                ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: target,
            },
        ));
    }
}
struct SourcePass {
    source: GpuPaintSource,
    output: RenderTargetOut,
    uniform: OutSlot<BufferResource, ()>,
    vertices: OutSlot<BufferResource, ()>,
    uploaded: bool,
    reuse: bool,
    resources: Option<Arc<SourceResources>>,
}
impl GraphicsPass for SourcePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::RequiresOwnPass);
        builder.write_color(&self.output, GraphicsColorAttachmentOps::load());
        self.uniform = builder.create_buffer(BufferDesc {
            size: self.source.uniforms.len() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("GPU Paint Uniform"),
        });
        self.vertices = builder.create_buffer(BufferDesc {
            size: (self.source.vertices.len() as u64).max(4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
            label: Some("GPU Paint Vertices"),
        });
        builder.read_buffer(&self.uniform, BufferReadUsage::Uniform);
        builder.read_buffer(&self.vertices, BufferReadUsage::Vertex);
    }
    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        let (reuse, resources) = ctx.viewport().prepare_gpu_paint_source(&self.source);
        self.reuse = reuse;
        self.resources = Some(resources);
        if reuse {
            return;
        }
        self.uploaded = ctx.upload_buffer(self.uniform.handle().unwrap(), 0, &self.source.uniforms)
            && (self.source.vertices.is_empty()
                || ctx.upload_buffer(self.vertices.handle().unwrap(), 0, &self.source.vertices));
    }
    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        if self.reuse {
            ctx.viewport()
                .note_gpu_paint_source_reused(self.source.id.get());
            return;
        }
        if !self.uploaded {
            ctx.mark_execution_failed();
            return;
        }
        let resources = self.resources.as_ref().expect("prepared resources");
        // Load preserves a warm source. A changed source is explicitly cleared
        // by a full-target overwrite before producer blending; no stale trails.
        ctx.set_pipeline(&resources.clear);
        ctx.draw(0..3, 0..1);
        if self.source.instance_count == 0 || self.source.vertex_count == 0 {
            ctx.viewport()
                .note_gpu_paint_source_written(self.source.id.get());
            return;
        }
        let Some(uniform) = ctx
            .frame_resources()
            .acquire_buffer(self.uniform.handle().unwrap())
        else {
            ctx.mark_execution_failed();
            return;
        };
        let Some(vertices) = ctx
            .frame_resources()
            .acquire_buffer(self.vertices.handle().unwrap())
        else {
            ctx.mark_execution_failed();
            return;
        };
        let resources = self.resources.as_ref().expect("prepared resources");
        let device = ctx.viewport().device().expect("active GPU frame");
        let bgl = &resources.bgl;
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU Paint Uniform"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        ctx.set_pipeline(&resources.pipeline);
        ctx.set_bind_group(0, &binding, &[]);
        ctx.set_vertex_buffer(0, vertices.slice(..));
        ctx.draw(0..self.source.vertex_count, 0..self.source.instance_count);
        ctx.viewport()
            .note_gpu_paint_source_written(self.source.id.get());
    }
}

pub(crate) struct SourceResources {
    bgl: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    clear: wgpu::RenderPipeline,
}
impl SourceResources {
    pub(crate) fn new(device: &wgpu::Device, program: &GpuPaintProgram) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Paint Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&program.shader)),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GPU Paint Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(program.uniform_size),
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU Paint Pipeline Layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GPU Paint Source"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: program.stride,
                    step_mode: program.step,
                    attributes: &program.attributes,
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let clear_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Paint Clear"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                r#"
            @vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
                var points = array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));
                return vec4(points[i],0.,1.);
            }
            @fragment fn fs_main() -> @location(0) vec4<f32> { return vec4(0.); }
        "#,
            )),
        });
        let clear = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GPU Paint Clear"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &clear_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &clear_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            bgl,
            pipeline,
            clear,
        }
    }
}
