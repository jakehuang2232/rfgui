use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::TextureResource;
use crate::view::frame_graph::{
    AttachmentTarget, FrameResourceContext, GraphicsColorAttachmentDescriptor,
    GraphicsDepthAspectDescriptor, GraphicsDepthStencilAttachmentDescriptor,
    GraphicsPassRecordingMode, GraphicsRecordContext, GraphicsStencilAspectDescriptor, PassBuilder,
};
use crate::view::frame_graph::{BufferDesc, BufferResource, PrepareContext, ResourceCache};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    render_target_attachment_view, render_target_msaa_view, render_target_size, render_target_view,
};
use std::sync::{Mutex, OnceLock};
use wgpu::util::DeviceExt;

const TEXTURE_COMPOSITE_RESOURCES: u64 = 205;

pub struct TextureCompositePass {
    params: TextureCompositeParams,
    stencil_clip_id: Option<u8>,
    depth_stencil_target: Option<AttachmentTarget>,
    uniform_buffer: TextureCompositeUniformBufferOut,
    vertex_buffer: TextureCompositeVertexBufferOut,
    index_buffer: TextureCompositeIndexBufferOut,
    input: TextureCompositeInput,
    output: TextureCompositeOutput,
}

#[derive(Clone, Copy, Debug)]
pub struct TextureCompositeParams {
    pub bounds: [f32; 4],
    pub use_mask: bool,
    pub opacity: f32,
    pub scissor_rect: Option<[u32; 4]>,
}

impl Default for TextureCompositeParams {
    fn default() -> Self {
        Self {
            bounds: [0.0, 0.0, 0.0, 0.0],
            use_mask: false,
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
    pub mask: TextureCompositeMaskIn,
}

#[derive(Default)]
pub struct TextureCompositeOutput {
    pub render_target: RenderTargetOut,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct TextureCompositeUniform {
    use_mask: f32,
    opacity: f32,
    _pad: [f32; 2],
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct CompositeVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

struct TextureCompositeResources {
    pipeline_no_stencil: wgpu::RenderPipeline,
    pipeline_stencil_test: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
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
            stencil_clip_id: None,
            depth_stencil_target: None,
            uniform_buffer: TextureCompositeUniformBufferOut::default(),
            vertex_buffer: TextureCompositeVertexBufferOut::default(),
            index_buffer: TextureCompositeIndexBufferOut::default(),
            input,
            output,
        }
    }

    pub fn set_depth_stencil_target(&mut self, depth_stencil_target: Option<AttachmentTarget>) {
        self.depth_stencil_target = depth_stencil_target;
    }
}

impl RenderPass for TextureCompositePass {
    fn setup(&mut self, builder: &mut PassBuilder<'_>) {
        builder.set_graphics_recording_mode(GraphicsPassRecordingMode::InlineOrStandalone);
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
        builder.declare_uniform_buffer(&self.uniform_buffer);
        builder.declare_vertex_buffer(&self.vertex_buffer);
        builder.declare_index_buffer(&self.index_buffer);

        if let Some(handle) = self.input.source.handle() {
            let source: OutSlot<TextureResource, TextureCompositeSourceTag> =
                OutSlot::with_handle(handle);
            builder.declare_sampled_texture(&mut self.input.source, &source);
        }
        if let Some(handle) = self.input.mask.handle() {
            let source: OutSlot<TextureResource, TextureCompositeMaskTag> =
                OutSlot::with_handle(handle);
            builder.declare_sampled_texture(&mut self.input.mask, &source);
        }
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            builder.declare_color_attachment(
                &self.output.render_target,
                GraphicsColorAttachmentDescriptor::load(target),
            );
        } else {
            builder.declare_surface_color_attachment(GraphicsColorAttachmentDescriptor::load(
                builder.surface_target(),
            ));
        }
        if let Some(target) = self.depth_stencil_target {
            builder.declare_depth_stencil_attachment(GraphicsDepthStencilAttachmentDescriptor {
                target,
                depth: Some(GraphicsDepthAspectDescriptor::read()),
                stencil: Some(GraphicsStencilAspectDescriptor::read()),
            });
        }
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = match self.output.render_target.handle() {
            Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
            None => surface_size,
        };
        if target_w == 0 || target_h == 0 {
            return;
        }

        let scale = ctx.viewport.scale_factor();
        let bounds = resolve_bounds(self.params.bounds, scale, target_w as f32, target_h as f32);

        let uniform = TextureCompositeUniform {
            use_mask: if self.params.use_mask { 1.0 } else { 0.0 },
            opacity: self.params.opacity.clamp(0.0, 1.0),
            _pad: [0.0, 0.0],
        };
        let (vertices, indices) = quad_for_bounds(bounds, target_w as f32, target_h as f32);

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

    fn record_standalone(
        &mut self,
        ctx: &mut GraphicsRecordContext<'_, '_>,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let Some(source_handle) = self.input.source.handle() else {
            return;
        };
        let Some(source_view) = render_target_view(ctx, source_handle) else {
            return;
        };
        let mask_view = self
            .input
            .mask
            .handle()
            .and_then(|h| render_target_view(ctx, h));

        let device = match ctx.viewport.device() {
            Some(device) => device.clone(),
            None => return,
        };
        let format = ctx.viewport.surface_format();
        let sample_count = ctx.viewport.msaa_sample_count();
        let cache = texture_composite_resources_cache();
        let mut cache = cache.lock().unwrap();
        let resources = cache.get_or_insert_with(TEXTURE_COMPOSITE_RESOURCES, || {
            create_resources(&device, format, sample_count)
        });
        if resources.pipeline_format != format || resources.pipeline_sample_count != sample_count {
            *resources = create_resources(&device, format, sample_count);
        }

        let acquired_uniform_buffer = self
            .uniform_buffer
            .handle()
            .and_then(|h| ctx.acquire_buffer(h));
        let fallback_uniform_buffer;
        let uniform_binding = if let Some(buffer) = acquired_uniform_buffer.as_ref() {
            buffer.as_entire_binding()
        } else {
            fallback_uniform_buffer =
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("TextureComposite Uniform (Fallback)"),
                    contents: bytemuck::bytes_of(&TextureCompositeUniform {
                        use_mask: if self.params.use_mask { 1.0 } else { 0.0 },
                        opacity: self.params.opacity.clamp(0.0, 1.0),
                        _pad: [0.0, 0.0],
                    }),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
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
                    resource: wgpu::BindingResource::Sampler(&resources.sampler),
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
            .and_then(|h| ctx.acquire_buffer(h));
        let acquired_index_buffer = self
            .index_buffer
            .handle()
            .and_then(|h| ctx.acquire_buffer(h));
        let fallback_vertex_buffer;
        let fallback_index_buffer;
        let (vertex_buffer, index_buffer): (&wgpu::Buffer, &wgpu::Buffer) = if let (
            Some(vb),
            Some(ib),
        ) = (
            acquired_vertex_buffer.as_ref(),
            acquired_index_buffer.as_ref(),
        ) {
            (vb, ib)
        } else {
            let surface_size = ctx.viewport.surface_size();
            let (target_w, target_h) = match self.output.render_target.handle() {
                Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
                None => surface_size,
            };
            let scale = ctx.viewport.scale_factor();
            let bounds =
                resolve_bounds(self.params.bounds, scale, target_w as f32, target_h as f32);
            let (vertices, indices) = quad_for_bounds(bounds, target_w as f32, target_h as f32);
            fallback_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("TextureComposite Vertex (Fallback)"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            fallback_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("TextureComposite Index (Fallback)"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            (&fallback_vertex_buffer, &fallback_index_buffer)
        };

        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = match self.output.render_target.handle() {
            Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
            None => surface_size,
        };
        let scissor_rect_physical = self.params.scissor_rect.and_then(|scissor_rect| {
            ctx.viewport
                .logical_scissor_to_physical(scissor_rect, (target_w, target_h))
        });

        let pipeline = if self.stencil_clip_id.is_some() {
            &resources.pipeline_stencil_test
        } else {
            &resources.pipeline_no_stencil
        };

        let offscreen_view = self
            .output
            .render_target
            .handle()
            .and_then(|h| render_target_view(ctx, h));
        let offscreen_msaa_view = self
            .output
            .render_target
            .handle()
            .and_then(|h| render_target_msaa_view(ctx, h));
        let depth_stencil_view = match self.depth_stencil_target {
            Some(AttachmentTarget::Texture(handle)) => render_target_attachment_view(ctx, handle),
            Some(AttachmentTarget::Surface) | None => None,
        };
        let msaa_enabled = ctx.viewport.msaa_sample_count() > 1;
        let Some(parts) = ctx.viewport.frame_parts() else {
            return;
        };
        let surface_resolve = if msaa_enabled {
            parts.resolve_view
        } else {
            None
        };
        let (color_view, resolve_target) =
            match (offscreen_view.as_ref(), offscreen_msaa_view.as_ref()) {
                (Some(resolve_view), Some(msaa_view)) => (msaa_view, Some(resolve_view)),
                (Some(resolve_view), None) => (resolve_view, None),
                (None, _) => (parts.view, surface_resolve),
            };

        let depth_stencil_attachment = match self.depth_stencil_target {
            Some(AttachmentTarget::Surface) => {
                parts.depth_stencil_attachment(wgpu::LoadOp::Load, wgpu::LoadOp::Load)
            }
            Some(AttachmentTarget::Texture(_)) => {
                depth_stencil_view
                    .as_ref()
                    .map(|view| wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                    })
            }
            None => None,
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("TextureComposite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target,
            })],
            depth_stencil_attachment,
            ..Default::default()
        });
        encode(
            &mut pass,
            pipeline,
            &bind_group,
            vertex_buffer,
            index_buffer,
            (target_w, target_h),
            scissor_rect_physical,
            self.stencil_clip_id,
        );
    }

    fn record_inline(
        &mut self,
        ctx: &mut GraphicsRecordContext<'_, '_>,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let Some(source_handle) = self.input.source.handle() else {
            return;
        };
        let Some(source_view) = render_target_view(ctx, source_handle) else {
            return;
        };
        let mask_view = self
            .input
            .mask
            .handle()
            .and_then(|h| render_target_view(ctx, h));

        let device = match ctx.viewport.device() {
            Some(device) => device.clone(),
            None => return,
        };
        let format = ctx.viewport.surface_format();
        let sample_count = ctx.viewport.msaa_sample_count();
        let cache = texture_composite_resources_cache();
        let mut cache = cache.lock().unwrap();
        let resources = cache.get_or_insert_with(TEXTURE_COMPOSITE_RESOURCES, || {
            create_resources(&device, format, sample_count)
        });
        if resources.pipeline_format != format || resources.pipeline_sample_count != sample_count {
            *resources = create_resources(&device, format, sample_count);
        }

        let acquired_uniform_buffer = self
            .uniform_buffer
            .handle()
            .and_then(|h| ctx.acquire_buffer(h));
        let fallback_uniform_buffer;
        let uniform_binding = if let Some(buffer) = acquired_uniform_buffer.as_ref() {
            buffer.as_entire_binding()
        } else {
            fallback_uniform_buffer =
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("TextureComposite Uniform (Fallback)"),
                    contents: bytemuck::bytes_of(&TextureCompositeUniform {
                        use_mask: if self.params.use_mask { 1.0 } else { 0.0 },
                        opacity: self.params.opacity.clamp(0.0, 1.0),
                        _pad: [0.0, 0.0],
                    }),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
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
                    resource: wgpu::BindingResource::Sampler(&resources.sampler),
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
            .and_then(|h| ctx.acquire_buffer(h));
        let acquired_index_buffer = self
            .index_buffer
            .handle()
            .and_then(|h| ctx.acquire_buffer(h));
        let fallback_vertex_buffer;
        let fallback_index_buffer;
        let (vertex_buffer, index_buffer): (&wgpu::Buffer, &wgpu::Buffer) = if let (
            Some(vb),
            Some(ib),
        ) = (
            acquired_vertex_buffer.as_ref(),
            acquired_index_buffer.as_ref(),
        ) {
            (vb, ib)
        } else {
            let surface_size = ctx.viewport.surface_size();
            let (target_w, target_h) = match self.output.render_target.handle() {
                Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
                None => surface_size,
            };
            let scale = ctx.viewport.scale_factor();
            let bounds =
                resolve_bounds(self.params.bounds, scale, target_w as f32, target_h as f32);
            let (vertices, indices) = quad_for_bounds(bounds, target_w as f32, target_h as f32);
            fallback_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("TextureComposite Vertex (Fallback)"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            fallback_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("TextureComposite Index (Fallback)"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            (&fallback_vertex_buffer, &fallback_index_buffer)
        };

        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = match self.output.render_target.handle() {
            Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
            None => surface_size,
        };
        let scissor_rect_physical = self.params.scissor_rect.and_then(|scissor_rect| {
            ctx.viewport
                .logical_scissor_to_physical(scissor_rect, (target_w, target_h))
        });
        let pipeline = if self.stencil_clip_id.is_some() {
            &resources.pipeline_stencil_test
        } else {
            &resources.pipeline_no_stencil
        };
        encode(
            pass,
            pipeline,
            &bind_group,
            vertex_buffer,
            index_buffer,
            (target_w, target_h),
            scissor_rect_physical,
            self.stencil_clip_id,
        );
    }
}

impl RenderTargetPass for TextureCompositePass {
    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.params.scissor_rect = intersect_scissor_rects(self.params.scissor_rect, scissor_rect);
    }

    fn apply_stencil_clip(&mut self, clip_id: Option<u8>) {
        self.stencil_clip_id = clip_id;
    }

    fn set_depth_stencil_target(&mut self, depth_stencil_target: Option<AttachmentTarget>) {
        self.set_depth_stencil_target(depth_stencil_target);
    }
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
    let sample_count = ctx.viewport.msaa_sample_count();
    let cache = texture_composite_resources_cache();
    let mut cache = cache.lock().unwrap();
    let resources = cache.get_or_insert_with(TEXTURE_COMPOSITE_RESOURCES, || {
        create_resources(&device, format, sample_count)
    });
    if resources.pipeline_format != format || resources.pipeline_sample_count != sample_count {
        *resources = create_resources(&device, format, sample_count);
    }

    let uniform = TextureCompositeUniform {
        use_mask: if use_mask { 1.0 } else { 0.0 },
        opacity: opacity.clamp(0.0, 1.0),
        _pad: [0.0, 0.0],
    };
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("TextureComposite Immediate Uniform"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let (vertices, indices) = quad_for_bounds(bounds, target_size.0 as f32, target_size.1 as f32);
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("TextureComposite Immediate Vertex"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
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
                resource: wgpu::BindingResource::Sampler(&resources.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    let msaa_enabled = ctx.viewport.msaa_sample_count() > 1;
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
    encode(
        &mut pass,
        &resources.pipeline_no_stencil,
        &bind_group,
        &vertex_buffer,
        &index_buffer,
        target_size,
        scissor_rect_physical,
        None,
    );
}

fn encode(
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

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("TextureComposite Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("TextureComposite Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        immediate_size: 0,
    });

    let pipeline_no_stencil = create_pipeline(
        device,
        &pipeline_layout,
        &shader,
        format,
        sample_count,
        TextureCompositeStencilMode::Disabled,
    );
    let pipeline_stencil_test = create_pipeline(
        device,
        &pipeline_layout,
        &shader,
        format,
        sample_count,
        TextureCompositeStencilMode::Test,
    );

    TextureCompositeResources {
        pipeline_no_stencil,
        pipeline_stencil_test,
        bind_group_layout,
        sampler,
        pipeline_format: format,
        pipeline_sample_count: sample_count,
    }
}

#[derive(Clone, Copy)]
enum TextureCompositeStencilMode {
    Disabled,
    Test,
}

fn create_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    sample_count: u32,
    stencil_mode: TextureCompositeStencilMode,
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
        depth_stencil: Some(texture_composite_depth_stencil_state(stencil_mode)),
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
    mode: TextureCompositeStencilMode,
) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::Always,
        stencil: match mode {
            TextureCompositeStencilMode::Disabled => wgpu::StencilState::default(),
            TextureCompositeStencilMode::Test => wgpu::StencilState {
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

fn quad_for_bounds(
    bounds: [f32; 4],
    target_w: f32,
    target_h: f32,
) -> ([CompositeVertex; 4], [u16; 6]) {
    let x = bounds[0];
    let y = bounds[1];
    let w = bounds[2].max(0.0);
    let h = bounds[3].max(0.0);
    let left = (x / target_w) * 2.0 - 1.0;
    let right = ((x + w) / target_w) * 2.0 - 1.0;
    let top = 1.0 - (y / target_h) * 2.0;
    let bottom = 1.0 - ((y + h) / target_h) * 2.0;
    (
        [
            CompositeVertex {
                position: [left, bottom],
                uv: [0.0, 1.0],
            },
            CompositeVertex {
                position: [right, bottom],
                uv: [1.0, 1.0],
            },
            CompositeVertex {
                position: [right, top],
                uv: [1.0, 0.0],
            },
            CompositeVertex {
                position: [left, top],
                uv: [0.0, 0.0],
            },
        ],
        [0, 1, 2, 0, 2, 3],
    )
}

fn resolve_bounds(bounds: [f32; 4], scale: f32, target_w: f32, target_h: f32) -> [f32; 4] {
    let scaled = [
        bounds[0] * scale,
        bounds[1] * scale,
        bounds[2] * scale,
        bounds[3] * scale,
    ];
    if scaled[2] <= 0.0 || scaled[3] <= 0.0 {
        [0.0, 0.0, target_w, target_h]
    } else {
        scaled
    }
}

fn texture_composite_resources_cache() -> &'static Mutex<ResourceCache<TextureCompositeResources>> {
    static CACHE: OnceLock<Mutex<ResourceCache<TextureCompositeResources>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ResourceCache::new()))
}

pub fn clear_texture_composite_resources_cache() {
    let cache = texture_composite_resources_cache();
    let mut cache = cache.lock().unwrap();
    cache.clear();
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
