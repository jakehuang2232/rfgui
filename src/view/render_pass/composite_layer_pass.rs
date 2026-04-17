use rustc_hash::FxHashSet;
use crate::view::frame_graph::{CacheStats, ResourceCache, register_cache_stats};
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::TextureResource;
use crate::view::frame_graph::{BufferDesc, BufferReadUsage, BufferResource};
use crate::view::frame_graph::{
    FrameResourceContext, GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy,
    PrepareContext,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, logical_scissor_to_target_physical,
    render_target_origin, render_target_sample_count, render_target_view, resolve_texture_ref,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use std::sync::{Mutex, OnceLock};

const COMPOSITE_LAYER_RESOURCES: u64 = 201;

#[derive(Clone, Copy)]
pub struct LayerTag;
pub type LayerIn = InSlot<TextureResource, LayerTag>;

pub struct CompositeLayerPass {
    params: CompositeLayerParams,
    vertex_buffer: CompositeVertexBufferOut,
    index_buffer: CompositeIndexBufferOut,
    prepared_vertices: Vec<CompositeVertex>,
    prepared_indices: Vec<u32>,
    input: CompositeLayerInput,
    output: CompositeLayerOutput,
}

pub struct CompositeLayerParams {
    pub rect_pos: [f32; 2],
    pub rect_size: [f32; 2],
    pub corner_radii: [f32; 4], // [top_left, top_right, bottom_right, bottom_left]
    pub opacity: f32,
    pub scissor_rect: Option<[u32; 4]>,
    pub clear_target: bool,
}

#[derive(Clone, Copy)]
pub struct CompositeVertexBufferTag;
pub type CompositeVertexBufferOut = OutSlot<BufferResource, CompositeVertexBufferTag>;
#[derive(Clone, Copy)]
pub struct CompositeIndexBufferTag;
pub type CompositeIndexBufferOut = OutSlot<BufferResource, CompositeIndexBufferTag>;

#[derive(Default)]
pub struct CompositeLayerInput {
    pub layer: LayerIn,
    pub pass_context: RenderPassContext,
}

#[derive(Default)]
pub struct CompositeLayerOutput {
    pub render_target: RenderTargetOut,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct CompositeVertex {
    position: [f32; 2],
    screen_uv: [f32; 2],
    alpha: f32,
    _pad: f32,
}

struct CompositeLayerResources {
    pipeline_no_stencil: wgpu::RenderPipeline,
    pipeline_stencil_test: wgpu::RenderPipeline,
    debug_pipeline_no_stencil: wgpu::RenderPipeline,
    debug_pipeline_stencil_test: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline_format: wgpu::TextureFormat,
    pipeline_sample_count: u32,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct DebugVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl CompositeLayerPass {
    pub fn new(
        params: CompositeLayerParams,
        input: CompositeLayerInput,
        output: CompositeLayerOutput,
    ) -> Self {
        Self {
            params,
            vertex_buffer: CompositeVertexBufferOut::default(),
            index_buffer: CompositeIndexBufferOut::default(),
            prepared_vertices: Vec::new(),
            prepared_indices: Vec::new(),
            input,
            output,
        }
    }
}

impl GraphicsPass for CompositeLayerPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        self.vertex_buffer = builder.create_buffer(BufferDesc {
            size: 256 * 1024,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
            label: Some("CompositeLayer Vertex Buffer"),
        });
        self.index_buffer = builder.create_buffer(BufferDesc {
            size: 256 * 1024,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::INDEX,
            label: Some("CompositeLayer Index Buffer"),
        });
        builder.read_buffer(&self.vertex_buffer, BufferReadUsage::Vertex);
        builder.read_buffer(&self.index_buffer, BufferReadUsage::Index);
        if let Some(source) = self.input.layer.handle().map(OutSlot::with_handle) {
            builder.read_texture(&mut self.input.layer, &source);
        }
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            let _ = target;
            builder.write_color(
                &self.output.render_target,
                if self.params.clear_target {
                    GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0])
                } else {
                    GraphicsColorAttachmentOps::load()
                },
            );
        } else {
            builder.write_surface_color(if self.params.clear_target {
                GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0])
            } else {
                GraphicsColorAttachmentOps::load()
            });
        }
        self.params.scissor_rect = intersect_scissor_rects(
            self.input.pass_context.scissor_rect,
            self.params.scissor_rect,
        );
        if self.input.pass_context.uses_depth_stencil {
            if self.params.clear_target {
                builder.write_output_depth(
                    crate::view::frame_graph::AttachmentLoadOp::Clear,
                    Some(1.0),
                );
                builder.write_output_stencil(
                    crate::view::frame_graph::AttachmentLoadOp::Clear,
                    Some(0),
                );
            } else {
                builder.read_output_depth();
                builder.read_output_stencil();
            }
        }
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        let Some(layer_handle) = self.input.layer.handle() else {
            return;
        };
        let surface_size = ctx.viewport.surface_size();
        let target_meta =
            resolve_texture_ref(self.output.render_target.handle(), ctx, surface_size, None);
        let layer_meta = resolve_texture_ref(Some(layer_handle), ctx, surface_size, None);
        let (target_w, target_h) = target_meta.physical_size;
        let (layer_w, layer_h) = layer_meta.physical_size;
        let target_origin = self
            .output
            .render_target
            .handle()
            .and_then(|handle| render_target_origin(ctx, handle))
            .unwrap_or((0, 0));
        let layer_origin = render_target_origin(ctx, layer_handle).unwrap_or((0, 0));
        let scale = ctx.viewport.scale_factor();
        let scaled_rect_pos = [
            self.params.rect_pos[0] * scale - target_origin.0 as f32
                + target_meta.logical_origin.0 as f32,
            self.params.rect_pos[1] * scale - target_origin.1 as f32
                + target_meta.logical_origin.1 as f32,
        ];
        let scaled_rect_size = [
            self.params.rect_size[0] * scale,
            self.params.rect_size[1] * scale,
        ];
        let scaled_corner_radii = self.params.corner_radii.map(|radius| radius * scale);
        let (vertices, indices) = tessellate_composite_layer(
            scaled_rect_pos,
            scaled_rect_size,
            scaled_corner_radii,
            self.params.opacity,
            target_w as f32,
            target_h as f32,
            layer_w as f32,
            layer_h as f32,
            [
                target_origin.0 as f32 - layer_origin.0 as f32 + layer_meta.logical_origin.0 as f32,
                target_origin.1 as f32 - layer_origin.1 as f32 + layer_meta.logical_origin.1 as f32,
            ],
        );
        self.prepared_vertices = vertices;
        self.prepared_indices = indices;
        if let Some(handle) = self.vertex_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::cast_slice(&self.prepared_vertices));
        }
        if let Some(handle) = self.index_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::cast_slice(&self.prepared_indices));
        }
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        let Some(layer_handle) = self.input.layer.handle() else {
            return;
        };
        let Some(layer_view) = render_target_view(ctx.frame_resources(), layer_handle) else {
            return;
        };
        let surface_size = ctx.viewport().surface_size();
        let target_meta = resolve_texture_ref(
            self.output.render_target.handle(),
            ctx.frame_resources(),
            surface_size,
            None,
        );
        let (target_w, target_h) = target_meta.physical_size;
        let target_origin = self
            .output
            .render_target
            .handle()
            .and_then(|handle| render_target_origin(ctx.frame_resources(), handle))
            .unwrap_or((0, 0));
        let device = match ctx.viewport().device() {
            Some(device) => device.clone(),
            None => return,
        };
        let format = ctx.viewport().surface_format();
        let sample_count = self
            .output
            .render_target
            .handle()
            .and_then(|handle| render_target_sample_count(ctx.frame_resources(), handle))
            .unwrap_or_else(|| ctx.viewport().msaa_sample_count());
        with_composite_layer_resources_cache(|cache| {
            let resources = cache.get_or_insert_with(COMPOSITE_LAYER_RESOURCES, || {
                create_resources(&device, format, sample_count)
            });
            if resources.pipeline_format != format
                || resources.pipeline_sample_count != sample_count
            {
                *resources = create_resources(&device, format, sample_count);
            }

            if self.prepared_vertices.is_empty() || self.prepared_indices.is_empty() {
                return;
            }
            let Some(vertex_buffer) = self
                .vertex_buffer
                .handle()
                .and_then(|h| ctx.frame_resources().acquire_buffer(h))
            else {
                return;
            };
            let Some(index_buffer) = self
                .index_buffer
                .handle()
                .and_then(|h| ctx.frame_resources().acquire_buffer(h))
            else {
                return;
            };

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
                ],
            });
            let scissor_rect_physical = self.params.scissor_rect.and_then(|scissor_rect| {
                logical_scissor_to_target_physical(
                    ctx.viewport(),
                    scissor_rect,
                    target_origin,
                    (target_w, target_h),
                )
            });

            let debug_geometry_overlay = ctx.viewport().debug_options().geometry_overlay;
            let pipeline = if self.input.pass_context.stencil_clip_id.is_some() {
                &resources.pipeline_stencil_test
            } else {
                &resources.pipeline_no_stencil
            };
            if let Some([x, y, width, height]) = scissor_rect_physical {
                ctx.set_scissor_rect(x, y, width, height);
            } else {
                ctx.set_scissor_rect(0, 0, target_w, target_h);
            }
            if let Some(clip_id) = self.input.pass_context.stencil_clip_id {
                ctx.set_stencil_reference(clip_id as u32);
            } else {
                ctx.set_stencil_reference(0);
            }
            ctx.set_pipeline(pipeline);
            ctx.set_bind_group(0, &bind_group, &[]);
            ctx.set_vertex_buffer(0, vertex_buffer.slice(..));
            ctx.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            ctx.draw_indexed(0..self.prepared_indices.len() as u32, 0, 0..1);

            if debug_geometry_overlay {
                let (overlay_w, overlay_h) = ctx.viewport().surface_size();
                let (debug_vertices, debug_indices) = build_debug_overlay_geometry(
                    &self.prepared_vertices,
                    &self.prepared_indices,
                    [target_origin.0 as f32, target_origin.1 as f32],
                    overlay_w as f32,
                    overlay_h as f32,
                    [0.2, 1.0, 0.95, 0.95],
                    [0.2, 1.0, 0.35, 0.95],
                );
                if !debug_vertices.is_empty() && !debug_indices.is_empty() {
                    let debug_vertex_buffer = super::create_transient_buffer(
                        &device,
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Composite Debug Vertex Buffer"),
                            contents: bytemuck::cast_slice(&debug_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    let debug_index_buffer = super::create_transient_buffer(
                        &device,
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Composite Debug Index Buffer"),
                            contents: bytemuck::cast_slice(&debug_indices),
                            usage: wgpu::BufferUsages::INDEX,
                        },
                    );
                    let debug_pipeline = if self.input.pass_context.stencil_clip_id.is_some() {
                        &resources.debug_pipeline_stencil_test
                    } else {
                        &resources.debug_pipeline_no_stencil
                    };
                    ctx.set_pipeline(debug_pipeline);
                    ctx.set_vertex_buffer(0, debug_vertex_buffer.slice(..));
                    ctx.set_index_buffer(debug_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    ctx.draw_indexed(0..debug_indices.len() as u32, 0, 0..1);
                }
            }
        });
    }
}

fn create_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> CompositeLayerResources {
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
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline_no_stencil = create_composite_pipeline(
        device,
        &pipeline_layout,
        &shader,
        format,
        sample_count,
        CompositeLayerStencilMode::Disabled,
    );
    let pipeline_stencil_test = create_composite_pipeline(
        device,
        &pipeline_layout,
        &shader,
        format,
        sample_count,
        CompositeLayerStencilMode::Test,
    );

    let debug_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Composite Debug Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/debug_color.wgsl").into()),
    });
    let debug_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Composite Debug Pipeline Layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let debug_pipeline_no_stencil = create_debug_pipeline(
        device,
        &debug_pipeline_layout,
        &debug_shader,
        format,
        sample_count,
        CompositeLayerStencilMode::Disabled,
    );
    let debug_pipeline_stencil_test = create_debug_pipeline(
        device,
        &debug_pipeline_layout,
        &debug_shader,
        format,
        sample_count,
        CompositeLayerStencilMode::Test,
    );

    CompositeLayerResources {
        pipeline_no_stencil,
        pipeline_stencil_test,
        debug_pipeline_no_stencil,
        debug_pipeline_stencil_test,
        bind_group_layout,
        sampler,
        pipeline_format: format,
        pipeline_sample_count: sample_count,
    }
}

#[derive(Clone, Copy)]
enum CompositeLayerStencilMode {
    Disabled,
    Test,
}

fn create_composite_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    sample_count: u32,
    stencil_mode: CompositeLayerStencilMode,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("CompositeLayer Pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<CompositeVertex>() as u64,
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
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: (std::mem::size_of::<[f32; 2]>() * 2) as u64,
                        shader_location: 2,
                    },
                ],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: Some(composite_layer_depth_stencil_state(stencil_mode)),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

fn create_debug_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    sample_count: u32,
    stencil_mode: CompositeLayerStencilMode,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Composite Debug Pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<DebugVertex>() as u64,
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
            module: shader,
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
        depth_stencil: Some(composite_layer_depth_stencil_state(stencil_mode)),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

fn composite_layer_depth_stencil_state(mode: CompositeLayerStencilMode) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: match mode {
            CompositeLayerStencilMode::Disabled => wgpu::StencilState::default(),
            CompositeLayerStencilMode::Test => wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                read_mask: 0xFF,
                write_mask: 0x00,
            },
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

fn with_composite_layer_resources_cache<R>(
    f: impl FnOnce(&mut ResourceCache<CompositeLayerResources>) -> R,
) -> R {
    static STATS: CacheStats = CacheStats::new("composite_layer_pipeline");
    static CACHE: OnceLock<Mutex<ResourceCache<CompositeLayerResources>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        register_cache_stats(&STATS);
        Mutex::new(ResourceCache::with_stats(&STATS))
    });
    f(&mut cache.lock().unwrap())
}

pub fn clear_composite_layer_resources_cache() {
    with_composite_layer_resources_cache(|cache| {
        cache.clear();
    });
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

fn tessellate_composite_layer(
    position: [f32; 2],
    size: [f32; 2],
    corner_radii: [f32; 4],
    opacity: f32,
    target_w: f32,
    target_h: f32,
    layer_w: f32,
    layer_h: f32,
    uv_origin_delta: [f32; 2],
) -> (Vec<CompositeVertex>, Vec<u32>) {
    let width = size[0].max(0.0);
    let height = size[1].max(0.0);
    if width <= 0.0
        || height <= 0.0
        || target_w <= 0.0
        || target_h <= 0.0
        || layer_w <= 0.0
        || layer_h <= 0.0
    {
        return (Vec::new(), Vec::new());
    }

    let alpha = opacity.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return (Vec::new(), Vec::new());
    }

    let radii = normalize_corner_radii(corner_radii, width, height);
    let max_outer_radius = radii.into_iter().fold(0.0f32, f32::max);
    let segments = corner_segments(max_outer_radius);
    let outer = rounded_rect_points(position[0], position[1], width, height, radii, segments);
    if outer.len() < 3 {
        return (Vec::new(), Vec::new());
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    append_convex_fan(
        &mut vertices,
        &mut indices,
        &outer,
        alpha,
        target_w,
        target_h,
        layer_w,
        layer_h,
        uv_origin_delta,
    );

    let aa_width = 1.0_f32;
    let outer_aa_radii = normalize_corner_radii(
        radii.map(|r| r + aa_width),
        width + aa_width * 2.0,
        height + aa_width * 2.0,
    );
    let outer_aa = rounded_rect_points(
        position[0] - aa_width,
        position[1] - aa_width,
        width + aa_width * 2.0,
        height + aa_width * 2.0,
        outer_aa_radii,
        segments,
    );
    append_ring(
        &mut vertices,
        &mut indices,
        &outer_aa,
        &outer,
        0.0,
        alpha,
        target_w,
        target_h,
        layer_w,
        layer_h,
        uv_origin_delta,
    );

    (vertices, indices)
}

fn corner_segments(radius: f32) -> u32 {
    ((std::f32::consts::FRAC_PI_2 * radius / 2.5).ceil() as u32).clamp(2, 64)
}

fn rounded_rect_points(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radii: [f32; 4],
    segments: u32,
) -> Vec<[f32; 2]> {
    let segments = segments.max(2);
    if radii.into_iter().all(|r| r <= 0.0) {
        return rectangle_points(x, y, width, height, segments);
    }

    let mut points = Vec::with_capacity((segments as usize) * 4);
    let corners = [
        (
            [x + radii[0], y + radii[0]],
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
            radii[0],
        ),
        (
            [x + width - radii[1], y + radii[1]],
            std::f32::consts::PI * 1.5,
            std::f32::consts::PI * 2.0,
            radii[1],
        ),
        (
            [x + width - radii[2], y + height - radii[2]],
            0.0,
            std::f32::consts::PI * 0.5,
            radii[2],
        ),
        (
            [x + radii[3], y + height - radii[3]],
            std::f32::consts::PI * 0.5,
            std::f32::consts::PI,
            radii[3],
        ),
    ];

    for (center, start, end, radius) in corners.iter() {
        if *radius <= 0.0 {
            let anchor = if *start < std::f32::consts::PI * 0.5 {
                [x + width, y + height]
            } else if *start < std::f32::consts::PI {
                [x, y + height]
            } else if *start < std::f32::consts::PI * 1.5 {
                [x, y]
            } else {
                [x + width, y]
            };
            for _ in 0..segments {
                points.push(anchor);
            }
            continue;
        }

        for step in 0..segments {
            let t = step as f32 / segments as f32;
            let angle = start + (end - start) * t;
            points.push([
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ]);
        }
    }

    points
}

fn normalize_corner_radii(mut radii: [f32; 4], width: f32, height: f32) -> [f32; 4] {
    for r in &mut radii {
        *r = r.max(0.0);
    }

    let w = width.max(0.0);
    let h = height.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return [0.0; 4];
    }

    let top = radii[0] + radii[1];
    let bottom = radii[3] + radii[2];
    let left = radii[0] + radii[3];
    let right = radii[1] + radii[2];

    let mut scale = 1.0_f32;
    if top > w {
        scale = scale.min(w / top);
    }
    if bottom > w {
        scale = scale.min(w / bottom);
    }
    if left > h {
        scale = scale.min(h / left);
    }
    if right > h {
        scale = scale.min(h / right);
    }

    if scale < 1.0 {
        for r in &mut radii {
            *r *= scale;
        }
    }

    radii
}

fn rectangle_points(x: f32, y: f32, width: f32, height: f32, segments: u32) -> Vec<[f32; 2]> {
    let mut points = Vec::with_capacity((segments as usize) * 4);
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        points.push([x + width * t, y]);
    }
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        points.push([x + width, y + height * t]);
    }
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        points.push([x + width * (1.0 - t), y + height]);
    }
    for step in 0..segments {
        let t = step as f32 / segments as f32;
        points.push([x, y + height * (1.0 - t)]);
    }
    points
}

fn append_convex_fan(
    vertices: &mut Vec<CompositeVertex>,
    indices: &mut Vec<u32>,
    polygon: &[[f32; 2]],
    alpha: f32,
    target_w: f32,
    target_h: f32,
    layer_w: f32,
    layer_h: f32,
    uv_origin_delta: [f32; 2],
) {
    let cleaned = sanitize_polygon(polygon);
    if cleaned.len() < 3 {
        return;
    }
    for point in &cleaned {
        vertices.push(CompositeVertex {
            position: pixel_to_ndc(point[0], point[1], target_w, target_h),
            screen_uv: pixel_to_uv(
                point[0] + uv_origin_delta[0],
                point[1] + uv_origin_delta[1],
                layer_w,
                layer_h,
            ),
            alpha,
            _pad: 0.0,
        });
    }

    let base = (vertices.len() - cleaned.len()) as u32;
    for i in 1..(cleaned.len() - 1) {
        indices.extend_from_slice(&[base, base + i as u32, base + (i as u32 + 1)]);
    }
}

fn sanitize_polygon(polygon: &[[f32; 2]]) -> Vec<[f32; 2]> {
    const EPS: f32 = 1e-4;
    if polygon.len() < 3 {
        return polygon.to_vec();
    }

    let mut out = Vec::with_capacity(polygon.len());
    for &p in polygon {
        if out.last().is_none_or(|last: &[f32; 2]| {
            (last[0] - p[0]).abs() > EPS || (last[1] - p[1]).abs() > EPS
        }) {
            out.push(p);
        }
    }
    if out.len() >= 2 {
        let first = out[0];
        let last = out[out.len() - 1];
        if (first[0] - last[0]).abs() <= EPS && (first[1] - last[1]).abs() <= EPS {
            out.pop();
        }
    }
    out
}

fn append_ring(
    vertices: &mut Vec<CompositeVertex>,
    indices: &mut Vec<u32>,
    outer: &[[f32; 2]],
    inner: &[[f32; 2]],
    outer_alpha: f32,
    inner_alpha: f32,
    target_w: f32,
    target_h: f32,
    layer_w: f32,
    layer_h: f32,
    uv_origin_delta: [f32; 2],
) {
    let n = outer.len().min(inner.len());
    if n < 3 {
        return;
    }
    let base = vertices.len() as u32;
    for i in 0..n {
        let o = outer[i];
        let ii = inner[i];
        vertices.push(CompositeVertex {
            position: pixel_to_ndc(o[0], o[1], target_w, target_h),
            screen_uv: pixel_to_uv(
                o[0] + uv_origin_delta[0],
                o[1] + uv_origin_delta[1],
                layer_w,
                layer_h,
            ),
            alpha: outer_alpha,
            _pad: 0.0,
        });
        vertices.push(CompositeVertex {
            position: pixel_to_ndc(ii[0], ii[1], target_w, target_h),
            screen_uv: pixel_to_uv(
                ii[0] + uv_origin_delta[0],
                ii[1] + uv_origin_delta[1],
                layer_w,
                layer_h,
            ),
            alpha: inner_alpha,
            _pad: 0.0,
        });
    }

    for i in 0..n {
        let j = (i + 1) % n;
        let o0 = base + (i as u32) * 2;
        let i0 = o0 + 1;
        let o1 = base + (j as u32) * 2;
        let i1 = o1 + 1;
        indices.extend_from_slice(&[o0, i0, o1, i0, i1, o1]);
    }
}

fn pixel_to_ndc(x: f32, y: f32, screen_w: f32, screen_h: f32) -> [f32; 2] {
    [(x / screen_w) * 2.0 - 1.0, 1.0 - (y / screen_h) * 2.0]
}

fn ndc_to_pixel(pos: [f32; 2], screen_w: f32, screen_h: f32) -> [f32; 2] {
    [
        ((pos[0] + 1.0) * 0.5) * screen_w,
        ((1.0 - pos[1]) * 0.5) * screen_h,
    ]
}

fn pixel_to_uv(x: f32, y: f32, layer_w: f32, layer_h: f32) -> [f32; 2] {
    [x / layer_w, y / layer_h]
}

fn build_debug_overlay_geometry(
    vertices: &[CompositeVertex],
    indices: &[u32],
    global_origin: [f32; 2],
    screen_w: f32,
    screen_h: f32,
    edge_color: [f32; 4],
    point_color: [f32; 4],
) -> (Vec<DebugVertex>, Vec<u32>) {
    let mut out_vertices = Vec::new();
    let mut out_indices = Vec::new();
    if vertices.is_empty() || indices.len() < 3 {
        return (out_vertices, out_indices);
    }

    let mut edges: FxHashSet<(u32, u32)> = FxHashSet::default();
    for tri in indices.chunks_exact(3) {
        let a = tri[0];
        let b = tri[1];
        let c = tri[2];
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let key = if u < v { (u, v) } else { (v, u) };
            edges.insert(key);
        }
    }

    for (u, v) in edges {
        let a = vertices[u as usize].position;
        let b = vertices[v as usize].position;
        append_debug_line_quad(
            &mut out_vertices,
            &mut out_indices,
            add_origin(ndc_to_pixel(a, screen_w, screen_h), global_origin),
            add_origin(ndc_to_pixel(b, screen_w, screen_h), global_origin),
            1.5,
            edge_color,
            screen_w,
            screen_h,
        );
    }

    for v in vertices {
        append_debug_point_quad(
            &mut out_vertices,
            &mut out_indices,
            add_origin(ndc_to_pixel(v.position, screen_w, screen_h), global_origin),
            2.5,
            point_color,
            screen_w,
            screen_h,
        );
    }

    (out_vertices, out_indices)
}

fn add_origin(point: [f32; 2], origin: [f32; 2]) -> [f32; 2] {
    [point[0] + origin[0], point[1] + origin[1]]
}

fn append_debug_line_quad(
    vertices: &mut Vec<DebugVertex>,
    indices: &mut Vec<u32>,
    p0: [f32; 2],
    p1: [f32; 2],
    thickness_px: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 1e-5 {
        return;
    }
    let nx = -dy / len;
    let ny = dx / len;
    let hw = thickness_px * 0.5;
    let o = [nx * hw, ny * hw];
    let quad = [
        [p0[0] + o[0], p0[1] + o[1]],
        [p0[0] - o[0], p0[1] - o[1]],
        [p1[0] - o[0], p1[1] - o[1]],
        [p1[0] + o[0], p1[1] + o[1]],
    ];
    append_debug_quad(vertices, indices, quad, color, screen_w, screen_h);
}

fn append_debug_point_quad(
    vertices: &mut Vec<DebugVertex>,
    indices: &mut Vec<u32>,
    center: [f32; 2],
    size_px: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let h = size_px * 0.5;
    let quad = [
        [center[0] - h, center[1] - h],
        [center[0] + h, center[1] - h],
        [center[0] + h, center[1] + h],
        [center[0] - h, center[1] + h],
    ];
    append_debug_quad(vertices, indices, quad, color, screen_w, screen_h);
}

fn append_debug_quad(
    vertices: &mut Vec<DebugVertex>,
    indices: &mut Vec<u32>,
    quad: [[f32; 2]; 4],
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let base = vertices.len() as u32;
    for p in quad {
        vertices.push(DebugVertex {
            position: pixel_to_ndc(p[0], p[1], screen_w, screen_h),
            color,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_rounded_points_zero_corner_keeps_fixed_topology() {
        let segments = 16;
        let pts = rounded_rect_points(0.0, 0.0, 100.0, 100.0, [0.0, 10.0, 10.0, 10.0], segments);
        assert_eq!(pts.len(), (segments * 4) as usize);
    }

    #[test]
    fn composite_tessellate_asymmetric_radius_produces_geometry() {
        let (vertices, indices) = tessellate_composite_layer(
            [0.0, 0.0],
            [150.0, 150.0],
            [10.0, 32.0, 10.0, 135.0],
            1.0,
            800.0,
            600.0,
            800.0,
            600.0,
            [0.0, 0.0],
        );
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
        assert_eq!(indices.len() % 3, 0);
    }

    #[test]
    fn composite_append_ring_tolerates_mismatched_topology() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let outer = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let inner = vec![[10.0, 10.0], [90.0, 10.0], [90.0, 90.0]];
        append_ring(
            &mut vertices,
            &mut indices,
            &outer,
            &inner,
            1.0,
            1.0,
            800.0,
            600.0,
            800.0,
            600.0,
            [0.0, 0.0],
        );
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }
}
