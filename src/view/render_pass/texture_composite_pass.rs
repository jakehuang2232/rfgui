use crate::view::ImageSampling;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::TextureResource;
use crate::view::frame_graph::{
    BufferDesc, BufferResource, CacheStats, PrepareContext, ResourceCache, register_cache_stats,
};
use crate::view::frame_graph::{
    BufferReadUsage, FrameResourceContext, GraphicsColorAttachmentOps, GraphicsPassBuilder,
    GraphicsPassMergePolicy, GraphicsRecordContext,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, ResolvedTextureRef,
    logical_scissor_to_target_physical, render_target_sample_count, render_target_view,
    resolve_texture_ref,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use std::sync::{Mutex, OnceLock};
use std::sync::Arc;

const TEXTURE_COMPOSITE_RESOURCES: u64 = 205;

pub struct TextureCompositePass {
    params: TextureCompositeParams,
    uniform_buffer: TextureCompositeUniformBufferOut,
    vertex_buffer: TextureCompositeVertexBufferOut,
    index_buffer: TextureCompositeIndexBufferOut,
    input: TextureCompositeInput,
    output: TextureCompositeOutput,
}

#[derive(Clone, Copy, Debug)]
pub struct TextureCompositeParams {
    pub bounds: [f32; 4],
    pub quad_positions: Option<[[f32; 2]; 4]>,
    pub uv_bounds: Option<[f32; 4]>,
    pub mask_uv_bounds: Option<[f32; 4]>,
    pub use_mask: bool,
    pub source_is_premultiplied: bool,
    pub opacity: f32,
    pub scissor_rect: Option<[u32; 4]>,
}

impl Default for TextureCompositeParams {
    fn default() -> Self {
        Self {
            bounds: [0.0, 0.0, 0.0, 0.0],
            quad_positions: None,
            uv_bounds: None,
            mask_uv_bounds: None,
            use_mask: false,
            source_is_premultiplied: false,
            opacity: 1.0,
            scissor_rect: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct TextureCompositeSourceTag;
pub type TextureCompositeSourceIn = InSlot<TextureResource, TextureCompositeSourceTag>;
#[derive(Clone, Copy)]
pub struct TextureCompositeMaskTag;
pub type TextureCompositeMaskIn = InSlot<TextureResource, TextureCompositeMaskTag>;

#[derive(Clone, Copy)]
pub struct TextureCompositeUniformBufferTag;
pub type TextureCompositeUniformBufferOut =
    OutSlot<BufferResource, TextureCompositeUniformBufferTag>;
#[derive(Clone, Copy)]
pub struct TextureCompositeVertexBufferTag;
pub type TextureCompositeVertexBufferOut = OutSlot<BufferResource, TextureCompositeVertexBufferTag>;
#[derive(Clone, Copy)]
pub struct TextureCompositeIndexBufferTag;
pub type TextureCompositeIndexBufferOut = OutSlot<BufferResource, TextureCompositeIndexBufferTag>;

#[derive(Default)]
pub struct TextureCompositeInput {
    pub source: TextureCompositeSourceIn,
    pub sampled_source_key: Option<u64>,
    pub sampled_source_size: Option<(u32, u32)>,
    pub sampled_source_upload: Option<Arc<[u8]>>,
    pub sampled_upload_state_key: Option<u64>,
    pub sampled_upload_generation: Option<u64>,
    pub sampled_source_sampling: Option<ImageSampling>,
    pub mask: TextureCompositeMaskIn,
    pub pass_context: RenderPassContext,
}

#[derive(Default)]
pub struct TextureCompositeOutput {
    pub render_target: RenderTargetOut,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct TextureCompositeUniform {
    use_mask: f32,
    source_is_premultiplied: f32,
    opacity: f32,
    _pad: f32,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct CompositeVertex {
    position: [f32; 2],
    source_uv: [f32; 2],
    mask_uv: [f32; 2],
}

struct TextureCompositeResources {
    pipeline_no_depth: wgpu::RenderPipeline,
    pipeline_depth_no_stencil: wgpu::RenderPipeline,
    pipeline_stencil_test: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    linear_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    pipeline_format: wgpu::TextureFormat,
    pipeline_sample_count: u32,
}

impl TextureCompositePass {
    pub fn new(
        params: TextureCompositeParams,
        input: TextureCompositeInput,
        output: TextureCompositeOutput,
    ) -> Self {
        Self {
            params,
            uniform_buffer: TextureCompositeUniformBufferOut::default(),
            vertex_buffer: TextureCompositeVertexBufferOut::default(),
            index_buffer: TextureCompositeIndexBufferOut::default(),
            input,
            output,
        }
    }
}

impl GraphicsPass for TextureCompositePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        self.uniform_buffer = builder.create_buffer(BufferDesc {
            size: std::mem::size_of::<TextureCompositeUniform>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("TextureComposite Uniform"),
        });
        self.vertex_buffer = builder.create_buffer(BufferDesc {
            size: (std::mem::size_of::<CompositeVertex>() * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
            label: Some("TextureComposite Vertex"),
        });
        self.index_buffer = builder.create_buffer(BufferDesc {
            size: (std::mem::size_of::<u16>() * 6) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::INDEX,
            label: Some("TextureComposite Index"),
        });
        builder.read_buffer(&self.uniform_buffer, BufferReadUsage::Uniform);
        builder.read_buffer(&self.vertex_buffer, BufferReadUsage::Vertex);
        builder.read_buffer(&self.index_buffer, BufferReadUsage::Index);

        self.params.scissor_rect = intersect_scissor_rects(
            self.input.pass_context.scissor_rect,
            self.params.scissor_rect,
        );
        if let Some(handle) = self.input.source.handle() {
            let source: OutSlot<TextureResource, TextureCompositeSourceTag> =
                OutSlot::with_handle(handle);
            builder.read_texture(&mut self.input.source, &source);
        }
        if let Some(handle) = self.input.mask.handle() {
            let source: OutSlot<TextureResource, TextureCompositeMaskTag> =
                OutSlot::with_handle(handle);
            builder.read_texture(&mut self.input.mask, &source);
        }
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            let _ = target;
            builder.write_color(
                &self.output.render_target,
                GraphicsColorAttachmentOps::load(),
            );
        } else {
            builder.write_surface_color(GraphicsColorAttachmentOps::load());
        }
        let stencil_clip_id = self.input.pass_context.stencil_clip_id;
        if self.input.pass_context.uses_depth_stencil {
            builder.read_output_depth();
            builder.read_output_stencil();
        }
        let _ = stencil_clip_id;
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        let surface_size = ctx.viewport.surface_size();
        let target_meta =
            resolve_target_meta(self.output.render_target.handle(), ctx, surface_size, None);
        let source_meta = resolve_target_meta(
            self.input.source.handle(),
            ctx,
            surface_size,
            self.input.sampled_source_size,
        );
        let mask_meta = resolve_target_meta(
            self.input.mask.handle(),
            ctx,
            source_meta.physical_size,
            None,
        )
        .with_fallback_origin(source_meta.global_origin)
        .with_fallback_logical_origin(source_meta.logical_origin);
        let (target_w, target_h) = target_meta.physical_size;
        if target_w == 0 || target_h == 0 {
            return;
        }

        let scale = ctx.viewport.scale_factor();
        let bounds = resolve_bounds(
            self.params.bounds,
            scale,
            target_w as f32,
            target_h as f32,
            target_meta.global_origin_f32(),
            target_meta.logical_origin_f32(),
        );
        let uv_bounds = resolve_uv_bounds(
            self.params.uv_bounds,
            if self.input.sampled_source_key.is_none() && self.input.source.handle().is_some() {
                scale
            } else {
                1.0
            },
            source_meta.physical_size.0 as f32,
            source_meta.physical_size.1 as f32,
            source_meta.global_origin_f32(),
            source_meta.logical_origin_f32(),
        );
        let mask_uv_bounds = resolve_uv_bounds(
            self.params.mask_uv_bounds.or(self.params.uv_bounds),
            scale,
            mask_meta.physical_size.0 as f32,
            mask_meta.physical_size.1 as f32,
            mask_meta.global_origin_f32(),
            mask_meta.logical_origin_f32(),
        );

        let uniform = TextureCompositeUniform {
            use_mask: if self.params.use_mask { 1.0 } else { 0.0 },
            source_is_premultiplied: if self.params.source_is_premultiplied {
                1.0
            } else {
                0.0
            },
            opacity: self.params.opacity.clamp(0.0, 1.0),
            _pad: 0.0,
        };
        let (vertices, indices) = texture_composite_geometry(
            self.params.quad_positions,
            bounds,
            scale,
            target_w as f32,
            target_h as f32,
            target_meta.global_origin_f32(),
            target_meta.logical_origin_f32(),
            uv_bounds,
            mask_uv_bounds,
        );

        if let Some(handle) = self.uniform_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::bytes_of(&uniform));
        }
        if let Some(handle) = self.vertex_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::cast_slice(&vertices));
        }
        if let Some(handle) = self.index_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::cast_slice(&indices));
        }
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        let source_view = if let Some(source_handle) = self.input.source.handle() {
            let Some(source_view) = render_target_view(ctx.frame_resources(), source_handle) else {
                return;
            };
            source_view
        } else if let Some(sampled_key) = self.input.sampled_source_key {
            if let (Some(bytes), Some((width, height))) = (
                self.input.sampled_source_upload.as_ref(),
                self.input.sampled_source_size,
            ) {
                if ctx.viewport().upload_sampled_texture_rgba(
                    sampled_key,
                    width,
                    height,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    bytes.as_ref(),
                ) {
                    if let (Some(state_key), Some(generation)) = (
                        self.input.sampled_upload_state_key,
                        self.input.sampled_upload_generation,
                    ) {
                        crate::view::image_resource::mark_uploaded(state_key, generation);
                        crate::view::svg_resource::mark_uploaded(state_key, generation);
                    }
                }
            }
            let Some(source_view) = ctx.viewport().sampled_texture_view(sampled_key) else {
                return;
            };
            source_view
        } else {
            return;
        };
        let mask_view = self
            .input
            .mask
            .handle()
            .and_then(|h| render_target_view(ctx.frame_resources(), h));

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
        with_texture_composite_resources_cache(|cache| {
            let resources = cache.get_or_insert_with(TEXTURE_COMPOSITE_RESOURCES, || {
                create_resources(&device, format, sample_count)
            });
            if resources.pipeline_format != format
                || resources.pipeline_sample_count != sample_count
            {
                *resources = create_resources(&device, format, sample_count);
            }

            let acquired_uniform_buffer = self
                .uniform_buffer
                .handle()
                .and_then(|h| ctx.frame_resources().acquire_buffer(h));
            let fallback_uniform_buffer;
            let uniform_binding = if let Some(buffer) = acquired_uniform_buffer.as_ref() {
                buffer.as_entire_binding()
            } else {
                fallback_uniform_buffer = super::create_transient_buffer(
                    &device,
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("TextureComposite Uniform (Fallback)"),
                        contents: bytemuck::bytes_of(&TextureCompositeUniform {
                            use_mask: if self.params.use_mask { 1.0 } else { 0.0 },
                            source_is_premultiplied: if self.params.source_is_premultiplied {
                                1.0
                            } else {
                                0.0
                            },
                            opacity: self.params.opacity.clamp(0.0, 1.0),
                            _pad: 0.0,
                        }),
                        usage: wgpu::BufferUsages::UNIFORM,
                    },
                );
                fallback_uniform_buffer.as_entire_binding()
            };

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("TextureComposite Bind Group"),
                layout: &resources.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            mask_view.as_ref().unwrap_or(&source_view),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(
                            match self.input.sampled_source_sampling {
                                Some(ImageSampling::Nearest) => &resources.nearest_sampler,
                                _ => &resources.linear_sampler,
                            },
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: uniform_binding,
                    },
                ],
            });

            let acquired_vertex_buffer = self
                .vertex_buffer
                .handle()
                .and_then(|h| ctx.frame_resources().acquire_buffer(h));
            let acquired_index_buffer = self
                .index_buffer
                .handle()
                .and_then(|h| ctx.frame_resources().acquire_buffer(h));
            let fallback_vertex_buffer;
            let fallback_index_buffer;
            let (vertex_buffer, index_buffer): (&wgpu::Buffer, &wgpu::Buffer) =
                if let (Some(vb), Some(ib)) = (
                    acquired_vertex_buffer.as_ref(),
                    acquired_index_buffer.as_ref(),
                ) {
                    (vb, ib)
                } else {
                    let surface_size = ctx.viewport().surface_size();
                    let target_meta = resolve_target_meta(
                        self.output.render_target.handle(),
                        ctx.frame_resources(),
                        surface_size,
                        None,
                    );
                    let scale = ctx.viewport().scale_factor();
                    let bounds = resolve_bounds(
                        self.params.bounds,
                        scale,
                        target_meta.physical_size.0 as f32,
                        target_meta.physical_size.1 as f32,
                        target_meta.global_origin_f32(),
                        target_meta.logical_origin_f32(),
                    );
                    let source_meta = resolve_target_meta(
                        self.input.source.handle(),
                        ctx.frame_resources(),
                        surface_size,
                        None,
                    );
                    let mask_meta = resolve_target_meta(
                        self.input.mask.handle(),
                        ctx.frame_resources(),
                        source_meta.physical_size,
                        None,
                    )
                    .with_fallback_origin(source_meta.global_origin)
                    .with_fallback_logical_origin(source_meta.logical_origin);
                    let source_uv_bounds = resolve_uv_bounds(
                        self.params.uv_bounds,
                        scale,
                        source_meta.physical_size.0 as f32,
                        source_meta.physical_size.1 as f32,
                        source_meta.global_origin_f32(),
                        source_meta.logical_origin_f32(),
                    );
                    let mask_uv_bounds = resolve_uv_bounds(
                        self.params.mask_uv_bounds.or(self.params.uv_bounds),
                        scale,
                        mask_meta.physical_size.0 as f32,
                        mask_meta.physical_size.1 as f32,
                        mask_meta.global_origin_f32(),
                        mask_meta.logical_origin_f32(),
                    );
                    let (vertices, indices) = texture_composite_geometry(
                        self.params.quad_positions,
                        bounds,
                        scale,
                        target_meta.physical_size.0 as f32,
                        target_meta.physical_size.1 as f32,
                        target_meta.global_origin_f32(),
                        target_meta.logical_origin_f32(),
                        source_uv_bounds,
                        mask_uv_bounds,
                    );
                    fallback_vertex_buffer = super::create_transient_buffer(
                        &device,
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("TextureComposite Vertex (Fallback)"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    fallback_index_buffer = super::create_transient_buffer(
                        &device,
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("TextureComposite Index (Fallback)"),
                            contents: bytemuck::cast_slice(&indices),
                            usage: wgpu::BufferUsages::INDEX,
                        },
                    );
                    (&fallback_vertex_buffer, &fallback_index_buffer)
                };

            let surface_size = ctx.viewport().surface_size();
            let target_meta = resolve_target_meta(
                self.output.render_target.handle(),
                ctx.frame_resources(),
                surface_size,
                None,
            );
            let (target_w, target_h) = target_meta.physical_size;
            let scissor_rect_physical = self.params.scissor_rect.and_then(|scissor_rect| {
                logical_scissor_to_target_physical(
                    ctx.viewport(),
                    scissor_rect,
                    target_meta.global_origin,
                    (target_w, target_h),
                )
            });
            let pipeline = match (
                self.input.pass_context.uses_depth_stencil,
                self.input.pass_context.stencil_clip_id.is_some(),
            ) {
                (true, true) => &resources.pipeline_stencil_test,
                (true, false) => &resources.pipeline_depth_no_stencil,
                (false, _) => &resources.pipeline_no_depth,
            };
            encode_pass(
                ctx,
                pipeline,
                &bind_group,
                vertex_buffer,
                index_buffer,
                (target_w, target_h),
                scissor_rect_physical,
                self.input.pass_context.stencil_clip_id,
            );
        });
    }
}

fn resolve_target_meta(
    handle: Option<crate::view::frame_graph::texture_resource::TextureHandle>,
    ctx: &mut impl FrameResourceContext,
    fallback_size: (u32, u32),
    sampled_size: Option<(u32, u32)>,
) -> ResolvedTextureRef {
    resolve_texture_ref(handle, ctx, fallback_size, sampled_size)
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub(crate) fn composite_immediate(
    ctx: &mut GraphicsRecordContext<'_, '_>,
    source_view: &wgpu::TextureView,
    mask_view: Option<&wgpu::TextureView>,
    offscreen_view: Option<&wgpu::TextureView>,
    offscreen_msaa_view: Option<&wgpu::TextureView>,
    target_size: (u32, u32),
    bounds: [f32; 4],
    scissor_rect_physical: Option<[u32; 4]>,
    use_mask: bool,
    opacity: f32,
) {
    let Some(device) = ctx.viewport.device().cloned() else {
        return;
    };
    let format = ctx.viewport.surface_format();
    let sample_count = if offscreen_msaa_view.is_some() {
        ctx.viewport.msaa_sample_count()
    } else {
        1
    };
    with_texture_composite_resources_cache(|cache| {
        let resources = cache.get_or_insert_with(TEXTURE_COMPOSITE_RESOURCES, || {
            create_resources(&device, format, sample_count)
        });
        if resources.pipeline_format != format || resources.pipeline_sample_count != sample_count {
            *resources = create_resources(&device, format, sample_count);
        }

        let uniform = TextureCompositeUniform {
            use_mask: if use_mask { 1.0 } else { 0.0 },
            source_is_premultiplied: 0.0,
            opacity: opacity.clamp(0.0, 1.0),
            _pad: 0.0,
        };
        let uniform_buffer = super::create_transient_buffer(
            &device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("TextureComposite Immediate Uniform"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            },
        );
        let (vertices, indices) = quad_for_bounds(
            bounds,
            target_size.0 as f32,
            target_size.1 as f32,
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        );
        let vertex_buffer = super::create_transient_buffer(
            &device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("TextureComposite Immediate Vertex"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            },
        );
        let index_buffer = super::create_transient_buffer(
            &device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("TextureComposite Immediate Index"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("TextureComposite Immediate Bind Group"),
            layout: &resources.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(mask_view.unwrap_or(source_view)),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&resources.linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let msaa_enabled = sample_count > 1;
        let Some(parts) = ctx.viewport.frame_parts() else {
            return;
        };
        let surface_resolve = if msaa_enabled {
            parts.resolve_view
        } else {
            None
        };
        let (color_view, resolve_target) = match (offscreen_view, offscreen_msaa_view) {
            (Some(resolve_view), Some(msaa_view)) => (msaa_view, Some(resolve_view)),
            (Some(resolve_view), None) => (resolve_view, None),
            (None, _) => (parts.view, surface_resolve),
        };
        let mut pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("TextureComposite Immediate"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                    resolve_target,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        encode_raw_pass(
            &mut pass,
            &resources.pipeline_no_depth,
            &bind_group,
            &vertex_buffer,
            &index_buffer,
            target_size,
            scissor_rect_physical,
            None,
        );
    });
}

fn encode_raw_pass(
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    vertex_buffer: &wgpu::Buffer,
    index_buffer: &wgpu::Buffer,
    target_size: (u32, u32),
    scissor_rect_physical: Option<[u32; 4]>,
    stencil_clip_id: Option<u8>,
) {
    if let Some([x, y, w, h]) = scissor_rect_physical {
        pass.set_scissor_rect(x, y, w, h);
    } else {
        pass.set_scissor_rect(0, 0, target_size.0, target_size.1);
    }
    if let Some(clip_id) = stencil_clip_id {
        pass.set_stencil_reference(clip_id as u32);
    } else {
        pass.set_stencil_reference(0);
    }
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    pass.draw_indexed(0..6, 0, 0..1);
}

fn encode_pass(
    ctx: &mut GraphicsCtx<'_, '_, '_, '_>,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    vertex_buffer: &wgpu::Buffer,
    index_buffer: &wgpu::Buffer,
    target_size: (u32, u32),
    scissor_rect_physical: Option<[u32; 4]>,
    stencil_clip_id: Option<u8>,
) {
    if let Some([x, y, w, h]) = scissor_rect_physical {
        ctx.set_scissor_rect(x, y, w, h);
    } else {
        ctx.set_scissor_rect(0, 0, target_size.0, target_size.1);
    }
    if let Some(clip_id) = stencil_clip_id {
        ctx.set_stencil_reference(clip_id as u32);
    } else {
        ctx.set_stencil_reference(0);
    }
    ctx.set_pipeline(pipeline);
    ctx.set_bind_group(0, bind_group, &[]);
    ctx.set_vertex_buffer(0, vertex_buffer.slice(..));
    ctx.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    ctx.draw_indexed(0..6, 0, 0..1);
}

fn create_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> TextureCompositeResources {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("TextureComposite Shader"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../shader/texture_composite.wgsl").into(),
        ),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("TextureComposite Bind Group Layout"),
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
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
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

    let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("TextureComposite Linear Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("TextureComposite Nearest Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("TextureComposite Pipeline Layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline_no_depth = create_pipeline(
        device,
        &pipeline_layout,
        &shader,
        format,
        sample_count,
        TextureCompositeDepthMode::None,
    );
    let pipeline_depth_no_stencil = create_pipeline(
        device,
        &pipeline_layout,
        &shader,
        format,
        sample_count,
        TextureCompositeDepthMode::DepthNoStencil,
    );
    let pipeline_stencil_test = create_pipeline(
        device,
        &pipeline_layout,
        &shader,
        format,
        sample_count,
        TextureCompositeDepthMode::DepthStencilTest,
    );

    TextureCompositeResources {
        pipeline_no_depth,
        pipeline_depth_no_stencil,
        pipeline_stencil_test,
        bind_group_layout,
        linear_sampler,
        nearest_sampler,
        pipeline_format: format,
        pipeline_sample_count: sample_count,
    }
}

#[derive(Clone, Copy)]
enum TextureCompositeDepthMode {
    None,
    DepthNoStencil,
    DepthStencilTest,
}

fn create_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    sample_count: u32,
    depth_mode: TextureCompositeDepthMode,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("TextureComposite Pipeline"),
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
                        format: wgpu::VertexFormat::Float32x2,
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
        depth_stencil: texture_composite_depth_stencil_state(depth_mode),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

fn texture_composite_depth_stencil_state(
    mode: TextureCompositeDepthMode,
) -> Option<wgpu::DepthStencilState> {
    match mode {
        TextureCompositeDepthMode::None => None,
        TextureCompositeDepthMode::DepthNoStencil => Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        TextureCompositeDepthMode::DepthStencilTest => Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
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
            bias: wgpu::DepthBiasState::default(),
        }),
    }
}

fn quad_for_bounds(
    bounds: [f32; 4],
    target_w: f32,
    target_h: f32,
    source_uv_bounds: [f32; 4],
    mask_uv_bounds: [f32; 4],
) -> ([CompositeVertex; 4], [u16; 6]) {
    let x = bounds[0];
    let y = bounds[1];
    let w = bounds[2].max(0.0);
    let h = bounds[3].max(0.0);
    let source_uv_left = source_uv_bounds[0];
    let source_uv_top = source_uv_bounds[1];
    let source_uv_right = source_uv_bounds[0] + source_uv_bounds[2].max(0.0);
    let source_uv_bottom = source_uv_bounds[1] + source_uv_bounds[3].max(0.0);
    let mask_uv_left = mask_uv_bounds[0];
    let mask_uv_top = mask_uv_bounds[1];
    let mask_uv_right = mask_uv_bounds[0] + mask_uv_bounds[2].max(0.0);
    let mask_uv_bottom = mask_uv_bounds[1] + mask_uv_bounds[3].max(0.0);
    let left = (x / target_w) * 2.0 - 1.0;
    let right = ((x + w) / target_w) * 2.0 - 1.0;
    let top = 1.0 - (y / target_h) * 2.0;
    let bottom = 1.0 - ((y + h) / target_h) * 2.0;
    (
        [
            CompositeVertex {
                position: [left, bottom],
                source_uv: [source_uv_left, source_uv_bottom],
                mask_uv: [mask_uv_left, mask_uv_bottom],
            },
            CompositeVertex {
                position: [right, bottom],
                source_uv: [source_uv_right, source_uv_bottom],
                mask_uv: [mask_uv_right, mask_uv_bottom],
            },
            CompositeVertex {
                position: [right, top],
                source_uv: [source_uv_right, source_uv_top],
                mask_uv: [mask_uv_right, mask_uv_top],
            },
            CompositeVertex {
                position: [left, top],
                source_uv: [source_uv_left, source_uv_top],
                mask_uv: [mask_uv_left, mask_uv_top],
            },
        ],
        [0, 1, 2, 0, 2, 3],
    )
}

fn resolve_quad_positions(
    quad_positions: [[f32; 2]; 4],
    scale: f32,
    target_origin: [f32; 2],
    target_logical_origin: [f32; 2],
) -> [[f32; 2]; 4] {
    quad_positions.map(|point| {
        [
            point[0] * scale - target_origin[0] + target_logical_origin[0],
            point[1] * scale - target_origin[1] + target_logical_origin[1],
        ]
    })
}

fn quad_for_positions(
    positions: [[f32; 2]; 4],
    target_w: f32,
    target_h: f32,
    source_uv_bounds: [f32; 4],
    mask_uv_bounds: [f32; 4],
) -> ([CompositeVertex; 4], [u16; 6]) {
    let source_uv_left = source_uv_bounds[0];
    let source_uv_top = source_uv_bounds[1];
    let source_uv_right = source_uv_bounds[0] + source_uv_bounds[2].max(0.0);
    let source_uv_bottom = source_uv_bounds[1] + source_uv_bounds[3].max(0.0);
    let mask_uv_left = mask_uv_bounds[0];
    let mask_uv_top = mask_uv_bounds[1];
    let mask_uv_right = mask_uv_bounds[0] + mask_uv_bounds[2].max(0.0);
    let mask_uv_bottom = mask_uv_bounds[1] + mask_uv_bounds[3].max(0.0);
    (
        [
            CompositeVertex {
                position: [
                    (positions[0][0] / target_w) * 2.0 - 1.0,
                    1.0 - (positions[0][1] / target_h) * 2.0,
                ],
                source_uv: [source_uv_left, source_uv_bottom],
                mask_uv: [mask_uv_left, mask_uv_bottom],
            },
            CompositeVertex {
                position: [
                    (positions[1][0] / target_w) * 2.0 - 1.0,
                    1.0 - (positions[1][1] / target_h) * 2.0,
                ],
                source_uv: [source_uv_right, source_uv_bottom],
                mask_uv: [mask_uv_right, mask_uv_bottom],
            },
            CompositeVertex {
                position: [
                    (positions[2][0] / target_w) * 2.0 - 1.0,
                    1.0 - (positions[2][1] / target_h) * 2.0,
                ],
                source_uv: [source_uv_right, source_uv_top],
                mask_uv: [mask_uv_right, mask_uv_top],
            },
            CompositeVertex {
                position: [
                    (positions[3][0] / target_w) * 2.0 - 1.0,
                    1.0 - (positions[3][1] / target_h) * 2.0,
                ],
                source_uv: [source_uv_left, source_uv_top],
                mask_uv: [mask_uv_left, mask_uv_top],
            },
        ],
        [0, 1, 2, 0, 2, 3],
    )
}

fn texture_composite_geometry(
    quad_positions: Option<[[f32; 2]; 4]>,
    bounds: [f32; 4],
    scale: f32,
    target_w: f32,
    target_h: f32,
    target_origin: [f32; 2],
    target_logical_origin: [f32; 2],
    source_uv_bounds: [f32; 4],
    mask_uv_bounds: [f32; 4],
) -> ([CompositeVertex; 4], [u16; 6]) {
    if let Some(quad_positions) = quad_positions {
        quad_for_positions(
            resolve_quad_positions(quad_positions, scale, target_origin, target_logical_origin),
            target_w,
            target_h,
            source_uv_bounds,
            mask_uv_bounds,
        )
    } else {
        quad_for_bounds(bounds, target_w, target_h, source_uv_bounds, mask_uv_bounds)
    }
}

fn resolve_bounds(
    bounds: [f32; 4],
    scale: f32,
    target_w: f32,
    target_h: f32,
    target_origin: [f32; 2],
    target_logical_origin: [f32; 2],
) -> [f32; 4] {
    let scaled = [
        bounds[0] * scale - target_origin[0] + target_logical_origin[0],
        bounds[1] * scale - target_origin[1] + target_logical_origin[1],
        bounds[2] * scale,
        bounds[3] * scale,
    ];
    if scaled[2] <= 0.0 || scaled[3] <= 0.0 {
        [0.0, 0.0, target_w, target_h]
    } else {
        scaled
    }
}

fn resolve_uv_bounds(
    uv_bounds: Option<[f32; 4]>,
    scale: f32,
    source_w: f32,
    source_h: f32,
    source_origin: [f32; 2],
    source_logical_origin: [f32; 2],
) -> [f32; 4] {
    let Some(bounds) = uv_bounds else {
        return [0.0, 0.0, 1.0, 1.0];
    };
    if source_w <= 0.0 || source_h <= 0.0 {
        return [0.0, 0.0, 1.0, 1.0];
    }
    let scaled = [
        bounds[0] * scale - source_origin[0] + source_logical_origin[0],
        bounds[1] * scale - source_origin[1] + source_logical_origin[1],
        bounds[2] * scale,
        bounds[3] * scale,
    ];
    [
        scaled[0] / source_w,
        scaled[1] / source_h,
        scaled[2] / source_w,
        scaled[3] / source_h,
    ]
}

fn with_texture_composite_resources_cache<R>(
    f: impl FnOnce(&mut ResourceCache<TextureCompositeResources>) -> R,
) -> R {
    static STATS: CacheStats = CacheStats::new("texture_composite_pipeline");
    static CACHE: OnceLock<Mutex<ResourceCache<TextureCompositeResources>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        register_cache_stats(&STATS);
        Mutex::new(ResourceCache::with_stats(&STATS))
    });
    f(&mut cache.lock().unwrap())
}

pub(crate) fn clear_texture_composite_resources_cache() {
    with_texture_composite_resources_cache(|cache| {
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

#[cfg(test)]
mod tests {
    fn assert_rgba_close(actual: [f32; 4], expected: [f32; 4]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        }
    }

    fn composite_sample(color: [f32; 4], factor: f32, source_is_premultiplied: bool) -> [f32; 4] {
        let alpha = color[3] * factor;
        if source_is_premultiplied {
            [
                color[0] * factor,
                color[1] * factor,
                color[2] * factor,
                alpha,
            ]
        } else {
            [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha]
        }
    }

    #[test]
    fn premultiplied_sources_do_not_apply_alpha_twice() {
        let premultiplied_color = [0.30, 0.12, 0.06, 0.60];
        let out = composite_sample(premultiplied_color, 0.5, true);
        assert_rgba_close(out, [0.15, 0.06, 0.03, 0.30]);
    }

    #[test]
    fn straight_alpha_sources_still_convert_to_premultiplied_output() {
        let straight_alpha_color = [1.0, 0.4, 0.2, 0.60];
        let out = composite_sample(straight_alpha_color, 0.5, false);
        assert_rgba_close(out, [0.30, 0.12, 0.06, 0.30]);
    }
}
