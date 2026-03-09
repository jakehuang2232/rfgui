use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::{DepIn, DepOut, PassContext, ResourceCache};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    render_target_bundle, render_target_size, render_target_view,
};
use std::sync::{Mutex, OnceLock};
use wgpu::util::DeviceExt;

const DEBUG_OVERLAY_RESOURCES: u64 = 402;

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct DebugOverlayVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

pub struct DebugOverlayPass {
    input: DebugOverlayInput,
    output: DebugOverlayOutput,
}

#[derive(Default)]
pub struct DebugOverlayInput {
    pub dep: DepIn,
}

#[derive(Default)]
pub struct DebugOverlayOutput {
    pub render_target: RenderTargetOut,
    pub dep: DepOut,
}

impl DebugOverlayPass {
    pub fn new(input: DebugOverlayInput, output: DebugOverlayOutput) -> Self {
        Self { input, output }
    }
}

impl RenderPass for DebugOverlayPass {
    type Input = DebugOverlayInput;
    type Output = DebugOverlayOutput;

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
        if let Some(handle) = self.input.dep.handle() {
            let source: DepOut = OutSlot::with_handle(handle);
            builder.read_dep(&mut self.input.dep, &source);
        }
        if self.output.render_target.handle().is_some() {
            builder.write_texture(&mut self.output.render_target);
        }
        if self.output.dep.handle().is_some() {
            builder.write_dep(&mut self.output.dep);
        }
    }

    fn execute(
        &mut self,
        ctx: &mut PassContext<'_, '_>,
        _render_pass: Option<&mut wgpu::RenderPass<'_>>,
    ) {
        if !ctx.viewport.debug_geometry_overlay() {
            let _ = ctx.viewport.take_debug_overlay_geometry();
            return;
        }
        let (vertices, indices) = ctx.viewport.take_debug_overlay_geometry();
        if vertices.is_empty() || indices.is_empty() {
            return;
        }

        let Some(device) = ctx.viewport.device().cloned() else {
            return;
        };
        let format = ctx.viewport.surface_format();
        let sample_count = ctx.viewport.msaa_sample_count();
        let cache = debug_overlay_resources_cache();
        let mut cache = cache.lock().unwrap();
        let resources = cache.get_or_insert_with(DEBUG_OVERLAY_RESOURCES, || {
            DebugOverlayResources::new(&device, format, sample_count)
        });
        if resources.pipeline_format != format || resources.pipeline_sample_count != sample_count {
            *resources = DebugOverlayResources::new(&device, format, sample_count);
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Frame Debug Overlay Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Frame Debug Overlay Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let surface_size = ctx.viewport.surface_size();
        let (offscreen_view, offscreen_msaa_view, target_w, target_h) =
            match self.output.render_target.handle() {
                Some(handle) => {
                    if let Some(bundle) = render_target_bundle(ctx, handle) {
                        (
                            Some(bundle.view),
                            bundle.msaa_view,
                            bundle.size.0,
                            bundle.size.1,
                        )
                    } else {
                        (
                            render_target_view(ctx, handle),
                            None,
                            render_target_size(ctx, handle).unwrap_or(surface_size).0,
                            render_target_size(ctx, handle).unwrap_or(surface_size).1,
                        )
                    }
                }
                None => (None, None, surface_size.0, surface_size.1),
            };

        let Some(parts) = ctx.viewport.frame_parts() else {
            return;
        };
        let (color_view, resolve_target) =
            match (offscreen_view.as_ref(), offscreen_msaa_view.as_ref()) {
                (Some(resolve_view), Some(msaa_view)) => (msaa_view, Some(resolve_view)),
                (Some(resolve_view), None) => (resolve_view, None),
                (None, _) => {
                    let surface_resolve = if sample_count > 1 {
                        parts.resolve_view
                    } else {
                        None
                    };
                    (parts.view, surface_resolve)
                }
            };

        let mut pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Debug Overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                    resolve_target,
                })],
                depth_stencil_attachment: parts
                    .depth_stencil_attachment(wgpu::LoadOp::Load, wgpu::LoadOp::Load),
                ..Default::default()
            });
        pass.set_pipeline(&resources.pipeline);
        pass.set_scissor_rect(0, 0, target_w, target_h);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
}

struct DebugOverlayResources {
    pipeline: wgpu::RenderPipeline,
    pipeline_format: wgpu::TextureFormat,
    pipeline_sample_count: u32,
}

impl DebugOverlayResources {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Debug Overlay Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../shader/debug_overlay.wgsl").into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Debug Overlay Pipeline Layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Debug Overlay Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<DebugOverlayVertex>() as u64,
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            pipeline_format: format,
            pipeline_sample_count: sample_count,
        }
    }
}

fn debug_overlay_resources_cache() -> &'static Mutex<ResourceCache<DebugOverlayResources>> {
    static CACHE: OnceLock<Mutex<ResourceCache<DebugOverlayResources>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ResourceCache::new()))
}
