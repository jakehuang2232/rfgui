use crate::view::frame_graph::ResourceCache;
use crate::view::render_pass::GraphicsCtx;
use crate::view::render_pass::render_target::{render_target_ref, render_target_sample_count};
use std::sync::{Mutex, OnceLock};
use wgpu::util::DeviceExt;

const DEBUG_OVERLAY_RESOURCES: u64 = 402;

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct DebugOverlayVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

pub(crate) fn draw_debug_overlay(
    ctx: &mut GraphicsCtx<'_, '_, '_, '_>,
    target_handle: Option<crate::view::frame_graph::texture_resource::TextureHandle>,
) {
    if !ctx.viewport().debug_overlay_enabled() {
        let _ = ctx.viewport().take_debug_overlay_geometry();
        return;
    }
    let (vertices, indices) = ctx.viewport().take_debug_overlay_geometry();
    if vertices.is_empty() || indices.is_empty() {
        return;
    }

    let Some(device) = ctx.viewport().device().cloned() else {
        return;
    };
    let format = ctx.viewport().surface_format();
    let sample_count = match target_handle {
        Some(handle) => render_target_sample_count(ctx.frame_resources(), handle).unwrap_or(1),
        None => 1,
    };
    let uses_depth_stencil = target_handle.is_some();
    let cache = debug_overlay_resources_cache();
    let mut cache = cache.lock().unwrap();
    let resources = cache.get_or_insert_with(DEBUG_OVERLAY_RESOURCES, || {
        DebugOverlayResources::new(&device, format, sample_count, uses_depth_stencil)
    });
    if resources.pipeline_format != format
        || resources.pipeline_sample_count != sample_count
        || resources.uses_depth_stencil != uses_depth_stencil
    {
        *resources = DebugOverlayResources::new(&device, format, sample_count, uses_depth_stencil);
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

    let surface_size = ctx.viewport().surface_size();
    let (target_w, target_h) = match target_handle {
        Some(handle) => render_target_ref(ctx.frame_resources(), handle)
            .map(|texture_ref| texture_ref.physical_size())
            .unwrap_or(surface_size),
        None => surface_size,
    };
    ctx.set_pipeline(&resources.pipeline);
    ctx.set_scissor_rect(0, 0, target_w, target_h);
    ctx.set_vertex_buffer(0, vertex_buffer.slice(..));
    ctx.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    ctx.draw_indexed(0..indices.len() as u32, 0, 0..1);
}

struct DebugOverlayResources {
    pipeline: wgpu::RenderPipeline,
    pipeline_format: wgpu::TextureFormat,
    pipeline_sample_count: u32,
    uses_depth_stencil: bool,
}

impl DebugOverlayResources {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        sample_count: u32,
        uses_depth_stencil: bool,
    ) -> Self {
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
            depth_stencil: uses_depth_stencil.then_some(wgpu::DepthStencilState {
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
            uses_depth_stencil,
        }
    }
}

fn debug_overlay_resources_cache() -> &'static Mutex<ResourceCache<DebugOverlayResources>> {
    static CACHE: OnceLock<Mutex<ResourceCache<DebugOverlayResources>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ResourceCache::new()))
}
