use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::{
    BufferDesc, BufferReadUsage, BufferResource, FrameGraph, FrameResourceContext,
};
use crate::view::frame_graph::{
    GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy,
    PrepareContext, TextureDesc,
};
use crate::view::render_pass::ClearPass;
use crate::view::render_pass::clear_pass::{ClearInput, ClearOutput, ClearParams};
use crate::view::render_pass::composite_layer_pass::LayerIn;
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, render_target_format, render_target_ref,
    render_target_view,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use wgpu::util::DeviceExt;

const BLUR_RESOURCES: u64 = 202;

pub struct BlurModuleParams {
    pub blur_radius: f32,
    pub intermediate_format: wgpu::TextureFormat,
}

#[derive(Default)]
pub struct BlurModuleInput {
    pub layer: LayerIn,
    pub pass_context: RenderPassContext,
}

#[derive(Default)]
pub struct BlurModuleOutput {
    pub render_target: RenderTargetOut,
}

struct BlurStagePass {
    params: BlurStageParams,
    upload_buffer: BlurBufferOut,
    input: BlurStageInput,
    output: BlurStageOutput,
}

struct BlurStageParams {
    blur_radius: f32,
    direction: [f32; 2],
    sigma: f32,
}

#[derive(Clone, Copy)]
struct BlurBufferTag;
type BlurBufferOut = OutSlot<BufferResource, BlurBufferTag>;

#[derive(Default)]
struct BlurStageInput {
    layer: LayerIn,
}

#[derive(Default)]
struct BlurStageOutput {
    render_target: RenderTargetOut,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct BlurVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct BlurParamsUniform {
    texel_size: [f32; 2],
    direction: [f32; 2],
    radius: f32,
    sigma: f32,
    source_uv_offset: [f32; 2],
    source_uv_scale: [f32; 2],
    target_origin: [f32; 2],
    target_scale: [f32; 2],
}

struct BlurResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    pipeline_format: wgpu::TextureFormat,
}

impl BlurStagePass {
    fn new(params: BlurStageParams, input: BlurStageInput, output: BlurStageOutput) -> Self {
        Self {
            params,
            upload_buffer: BlurBufferOut::default(),
            input,
            output,
        }
    }
}

impl GraphicsPass for BlurStagePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        if let Some(source) = self.input.layer.handle().map(OutSlot::with_handle) {
            builder.read_texture(&mut self.input.layer, &source);
        }
        self.upload_buffer = builder.create_buffer(BufferDesc {
            size: std::mem::size_of::<BlurParamsUniform>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("BlurModule Params"),
        });
        builder.read_buffer(&self.upload_buffer, BufferReadUsage::Uniform);
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            let _ = target;
            builder.write_color(
                &self.output.render_target,
                GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]),
            );
        } else {
            builder.write_surface_color(GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]));
        }
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        let Some(layer_handle) = self.input.layer.handle() else {
            return;
        };
        let surface_size = ctx.viewport.surface_size();
        let layer_ref = render_target_ref(ctx, layer_handle);
        let (layer_w, layer_h) = layer_ref
            .map(|texture_ref| texture_ref.physical_size())
            .unwrap_or(surface_size);
        let output_ref = self
            .output
            .render_target
            .handle()
            .and_then(|handle| render_target_ref(ctx, handle));
        if layer_w == 0 || layer_h == 0 {
            return;
        }
        let params = BlurParamsUniform {
            texel_size: [1.0 / layer_w as f32, 1.0 / layer_h as f32],
            direction: self.params.direction,
            radius: self.params.blur_radius,
            sigma: self.params.sigma,
            source_uv_offset: layer_ref
                .map(|texture_ref| [texture_ref.uv_offset_x(), texture_ref.uv_offset_y()])
                .unwrap_or([0.0, 0.0]),
            source_uv_scale: layer_ref
                .map(|texture_ref| [texture_ref.uv_scale_x(), texture_ref.uv_scale_y()])
                .unwrap_or([1.0, 1.0]),
            target_origin: output_ref
                .map(|texture_ref| [texture_ref.uv_offset_x(), texture_ref.uv_offset_y()])
                .unwrap_or([0.0, 0.0]),
            target_scale: output_ref
                .map(|texture_ref| [texture_ref.uv_scale_x(), texture_ref.uv_scale_y()])
                .unwrap_or([1.0, 1.0]),
        };
        if let Some(handle) = self.upload_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::bytes_of(&params));
        }
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        let Some(layer_handle) = self.input.layer.handle() else {
            return;
        };
        let Some(layer_view) = render_target_view(ctx.frame_resources(), layer_handle) else {
            return;
        };
        let Some(params_handle) = self.upload_buffer.handle() else {
            return;
        };
        let Some(params_buffer) = ctx.frame_resources().acquire_buffer(params_handle) else {
            return;
        };
        let Some(device) = ctx.viewport().device().cloned() else {
            return;
        };
        let format = match self.output.render_target.handle() {
            Some(handle) => render_target_format(ctx.frame_resources(), handle)
                .unwrap_or(ctx.viewport().surface_format()),
            None => ctx.viewport().surface_format(),
        };
        with_blur_resources_cache(|cache| {
            let resources =
                cache.get_or_insert_with(BLUR_RESOURCES, || create_resources(&device, format));
            if resources.pipeline_format != format {
                *resources = create_resources(&device, format);
            }
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("BlurModule Bind Group"),
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
                        resource: params_buffer.as_entire_binding(),
                    },
                ],
            });
            ctx.set_pipeline(&resources.pipeline);
            ctx.set_bind_group(0, &bind_group, &[]);
            ctx.set_vertex_buffer(0, resources.vertex_buffer.slice(..));
            ctx.set_index_buffer(resources.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            ctx.draw_indexed(0..resources.index_count, 0, 0..1);
        });
    }
}

pub fn build_blur_module(
    graph: &mut FrameGraph,
    params: BlurModuleParams,
    input: BlurModuleInput,
    output: BlurModuleOutput,
) -> bool {
    let Some(source_handle) = input.layer.handle() else {
        return false;
    };
    let Some(source_desc) = graph.texture_desc(source_handle) else {
        return false;
    };
    let (source_origin_x, source_origin_y) = source_desc.origin();
    let source_w = source_desc.width().max(1);
    let source_h = source_desc.height().max(1);
    let blur_radius = params.blur_radius.max(0.0);
    if blur_radius <= 0.001 {
        graph.add_graphics_pass(BlurStagePass::new(
            BlurStageParams {
                blur_radius: 0.0,
                direction: [1.0, 0.0],
                sigma: 0.001,
            },
            BlurStageInput { layer: input.layer },
            BlurStageOutput {
                render_target: output.render_target,
            },
        ));
        return true;
    }

    let downsample = if blur_radius >= 28.0 {
        4_u32
    } else if blur_radius >= 12.0 {
        2_u32
    } else {
        1_u32
    };
    let effective_radius = if downsample > 1 {
        blur_radius / downsample as f32
    } else {
        blur_radius
    };
    let sigma = (effective_radius * 0.5).max(0.001);

    let mut blur_source = input.layer;
    let mut blur_width = source_w;
    let mut blur_height = source_h;
    if downsample > 1 {
        let ds_w = (source_w / downsample).max(1);
        let ds_h = (source_h / downsample).max(1);
        let downsampled = graph.declare_texture(
            TextureDesc::new(
                ds_w,
                ds_h,
                params.intermediate_format,
                wgpu::TextureDimension::D2,
            )
            .with_origin(source_origin_x, source_origin_y)
            .with_sample_count(1)
            .with_label("Blur Intermediate / Downsample"),
        );
        graph.add_graphics_pass(ClearPass::new(
            ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            ClearInput {
                pass_context: input.pass_context,
                clear_depth_stencil: false,
            },
            ClearOutput {
                render_target: downsampled,
                ..Default::default()
            },
        ));
        graph.add_graphics_pass(BlurStagePass::new(
            BlurStageParams {
                blur_radius: 0.0,
                direction: [1.0, 0.0],
                sigma: 0.001,
            },
            BlurStageInput { layer: blur_source },
            BlurStageOutput {
                render_target: downsampled,
            },
        ));
        blur_source = downsampled
            .handle()
            .map(LayerIn::with_handle)
            .unwrap_or_default();
        blur_width = ds_w;
        blur_height = ds_h;
    }

    let blur_h_target = graph.declare_texture(
        TextureDesc::new(
            blur_width,
            blur_height,
            params.intermediate_format,
            wgpu::TextureDimension::D2,
        )
        .with_origin(source_origin_x, source_origin_y)
        .with_sample_count(1)
        .with_label("Blur Intermediate / Horizontal"),
    );
    graph.add_graphics_pass(ClearPass::new(
        ClearParams::new([0.0, 0.0, 0.0, 0.0]),
        ClearInput {
            pass_context: input.pass_context,
            clear_depth_stencil: false,
        },
        ClearOutput {
            render_target: blur_h_target,
            ..Default::default()
        },
    ));
    graph.add_graphics_pass(BlurStagePass::new(
        BlurStageParams {
            blur_radius: effective_radius,
            direction: [1.0, 0.0],
            sigma,
        },
        BlurStageInput { layer: blur_source },
        BlurStageOutput {
            render_target: blur_h_target,
        },
    ));
    graph.add_graphics_pass(BlurStagePass::new(
        BlurStageParams {
            blur_radius: effective_radius,
            direction: [0.0, 1.0],
            sigma,
        },
        BlurStageInput {
            layer: blur_h_target
                .handle()
                .map(LayerIn::with_handle)
                .unwrap_or_default(),
        },
        BlurStageOutput {
            render_target: output.render_target,
        },
    ));
    true
}

fn create_resources(device: &wgpu::Device, format: wgpu::TextureFormat) -> BlurResources {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Blur Module Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/shadow_blur.wgsl").into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Blur Module Bind Group Layout"),
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
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Blur Module Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Blur Module Pipeline Layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Blur Module Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<BlurVertex>() as u64,
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
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    });
    let vertices = [
        BlurVertex {
            position: [-1.0, -1.0],
            uv: [0.0, 1.0],
        },
        BlurVertex {
            position: [1.0, -1.0],
            uv: [1.0, 1.0],
        },
        BlurVertex {
            position: [1.0, 1.0],
            uv: [1.0, 0.0],
        },
        BlurVertex {
            position: [-1.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0_u16, 1, 2, 0, 2, 3];
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Blur Module Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Blur Module Index Buffer"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    BlurResources {
        pipeline,
        bind_group_layout,
        sampler,
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
        pipeline_format: format,
    }
}

crate::static_resource_cache! {
    fn with_blur_resources_cache -> ResourceCache<BlurResources> = stats("blur_module_pipeline")
}

pub fn clear_blur_resources_cache() {
    with_blur_resources_cache(|cache| {
        cache.clear();
    });
}
