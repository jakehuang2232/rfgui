use crate::view::frame_graph::ResourceCache;
use crate::view::frame_graph::{
    FrameGraph, GraphicsColorAttachmentOps, GraphicsPassBuilder, SampleCountPolicy, TextureDesc,
};
use crate::view::render_pass::blur_module::{
    BlurModuleInput, BlurModuleOutput, BlurModuleParams, build_blur_module,
};
use crate::view::render_pass::clear_pass::{ClearInput, ClearOutput, ClearParams};
use crate::view::render_pass::composite_layer_pass::LayerIn;
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{GraphicsPassContext, render_target_ref};
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeMaskIn, TextureCompositeOutput, TextureCompositeParams,
    TextureCompositePass, TextureCompositeSourceIn,
};
use crate::view::render_pass::{ClearPass, GraphicsPass};
use std::cell::RefCell;

const SHADOW_RESOURCES: u64 = 203;
const SHADOW_INTERMEDIATE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[derive(Clone, Debug, Default)]
pub struct ShadowMesh {
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl ShadowMesh {
    pub fn new(vertices: Vec<[f32; 2]>, indices: Vec<u32>) -> Self {
        Self { vertices, indices }
    }

    pub fn rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Self {
        Self::rounded_rect_with_radii(x, y, width, height, [radius, radius, radius, radius])
    }

    pub fn rounded_rect_with_radii(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radii: [f32; 4],
    ) -> Self {
        let w = width.max(0.0);
        let h = height.max(0.0);
        if w <= 0.0 || h <= 0.0 {
            return Self::default();
        }
        let [tl, tr, br, bl] = normalize_corner_radii(radii, w, h);
        if tl <= 0.001 && tr <= 0.001 && br <= 0.001 && bl <= 0.001 {
            return Self {
                vertices: vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                indices: vec![0, 1, 2, 0, 2, 3],
            };
        }
        const ARC_SEGMENTS: usize = 6;
        let mut ring = Vec::with_capacity(ARC_SEGMENTS * 4 + 4);
        append_arc(
            &mut ring,
            [x + w - tr, y + tr],
            tr,
            -std::f32::consts::FRAC_PI_2,
            0.0,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + w - br, y + h - br],
            br,
            0.0,
            std::f32::consts::FRAC_PI_2,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + bl, y + h - bl],
            bl,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + tl, y + tl],
            tl,
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
            ARC_SEGMENTS,
        );

        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        let mut vertices = Vec::with_capacity(ring.len() + 1);
        vertices.push([cx, cy]);
        vertices.extend(ring.iter().copied());

        let mut indices = Vec::with_capacity(ring.len() * 3);
        let ring_start = 1_u32;
        let ring_len = ring.len() as u32;
        for i in 0..ring_len {
            let a = ring_start + i;
            let b = ring_start + ((i + 1) % ring_len);
            indices.extend_from_slice(&[0, a, b]);
        }
        Self { vertices, indices }
    }
}

fn append_arc(
    out: &mut Vec<[f32; 2]>,
    center: [f32; 2],
    radius: f32,
    start: f32,
    end: f32,
    segments: usize,
) {
    if radius <= 0.001 {
        out.push(center);
        return;
    }
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let a = start + (end - start) * t;
        out.push([center[0] + radius * a.cos(), center[1] + radius * a.sin()]);
    }
}

fn normalize_corner_radii(radii: [f32; 4], width: f32, height: f32) -> [f32; 4] {
    let mut tl = radii[0].max(0.0);
    let mut tr = radii[1].max(0.0);
    let mut br = radii[2].max(0.0);
    let mut bl = radii[3].max(0.0);
    let w = width.max(0.0);
    let h = height.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let top = tl + tr;
    let bottom = bl + br;
    let left = tl + bl;
    let right = tr + br;
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
        tl *= scale;
        tr *= scale;
        br *= scale;
        bl *= scale;
    }
    [tl, tr, br, bl]
}

#[derive(Clone, Copy, Debug)]
pub struct ShadowParams {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: [f32; 4],
    pub opacity: f32,
    pub spread: f32,
    pub clip_to_geometry: bool,
}

impl Default for ShadowParams {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            blur_radius: 0.0,
            color: [0.0, 0.0, 0.0, 1.0],
            opacity: 1.0,
            spread: 0.0,
            clip_to_geometry: false,
        }
    }
}

#[derive(Clone)]
pub struct ShadowModuleSpec {
    pub mesh: ShadowMesh,
    pub params: ShadowParams,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub scale_factor: f32,
    pub pass_context: GraphicsPassContext,
    pub output: RenderTargetOut,
}

struct ShadowFillPass {
    mesh: ShadowMesh,
    color: [f32; 4],
    render_target: RenderTargetOut,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct FillVertex {
    position: [f32; 2],
    color: [f32; 4],
}

struct ShadowResources {
    fill_pipeline: wgpu::RenderPipeline,
}

impl GraphicsPass for ShadowFillPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_sample_count(SampleCountPolicy::Fixed(1));
        if let Some(target) = builder.texture_target(&self.render_target) {
            let _ = target;
            builder.write_color(&self.render_target, GraphicsColorAttachmentOps::load());
        }
    }

    fn execute(&mut self, ctx: &mut crate::view::render_pass::GraphicsCtx<'_, '_, '_, '_>) {
        if self.mesh.vertices.is_empty() || self.mesh.indices.is_empty() {
            return;
        }
        let Some(device) = ctx.viewport().device().cloned() else {
            return;
        };
        let surface_size = ctx.viewport().surface_size();
        let (target_w, target_h) = match self.render_target.handle() {
            Some(handle) => render_target_ref(ctx.frame_resources(), handle)
                .map(|texture_ref| texture_ref.physical_size())
                .unwrap_or(surface_size),
            None => surface_size,
        };
        if target_w == 0 || target_h == 0 {
            return;
        }
        let pipeline = with_shadow_resources_cache(|cache| {
            let resources =
                cache.get_or_insert_with(SHADOW_RESOURCES, || create_resources(&device));
            resources.fill_pipeline.clone()
        });
        encode_mesh_fill_into_pass(
            &device,
            &pipeline,
            ctx,
            target_w as f32,
            target_h as f32,
            &self.mesh.vertices,
            &self.mesh.indices,
            self.color,
        );
    }
}

pub fn build_shadow_module(graph: &mut FrameGraph, spec: ShadowModuleSpec) -> bool {
    let scale = spec.scale_factor.max(0.0001);
    let base_vertices = spec
        .mesh
        .vertices
        .iter()
        .map(|[x, y]| [x * scale, y * scale])
        .collect::<Vec<_>>();
    let mut shadow_vertices = base_vertices.clone();
    apply_spread(&mut shadow_vertices, (spec.params.spread * scale).max(0.0));
    for v in &mut shadow_vertices {
        v[0] += spec.params.offset_x * scale;
        v[1] += spec.params.offset_y * scale;
    }
    let Some(first) = shadow_vertices.first().copied() else {
        return false;
    };
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first[0], first[1], first[0], first[1]);
    for [x, y] in shadow_vertices.iter().copied().skip(1) {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let blur_padding = ((spec.params.blur_radius.max(0.0) * scale) * 1.5).ceil();
    min_x -= blur_padding;
    min_y -= blur_padding;
    max_x += blur_padding;
    max_y += blur_padding;
    let target_w = spec.viewport_width as f32;
    let target_h = spec.viewport_height as f32;
    let bx = min_x.floor().max(0.0).min(target_w);
    let by = min_y.floor().max(0.0).min(target_h);
    let br = max_x.ceil().max(0.0).min(target_w);
    let bb = max_y.ceil().max(0.0).min(target_h);
    if br <= bx || bb <= by {
        return false;
    }

    let layer_w = (br - bx).max(1.0) as u32;
    let layer_h = (bb - by).max(1.0) as u32;
    let local_shadow_mesh = ShadowMesh::new(
        shadow_vertices
            .iter()
            .map(|[x, y]| [x - bx, y - by])
            .collect(),
        spec.mesh.indices.clone(),
    );
    let local_mask_mesh = ShadowMesh::new(
        base_vertices
            .iter()
            .map(|[x, y]| [x - bx, y - by])
            .collect(),
        spec.mesh.indices.clone(),
    );
    let shadow_layer = graph.declare_texture(
        TextureDesc::new(
            layer_w,
            layer_h,
            SHADOW_INTERMEDIATE_FORMAT,
            wgpu::TextureDimension::D2,
        )
        .with_origin(bx as u32, by as u32)
        .with_sample_count(1)
        .with_label("Shadow Layer"),
    );
    let shadow_mask_layer = if spec.params.clip_to_geometry {
        graph.declare_texture(
            TextureDesc::new(
                layer_w,
                layer_h,
                SHADOW_INTERMEDIATE_FORMAT,
                wgpu::TextureDimension::D2,
            )
            .with_origin(bx as u32, by as u32)
            .with_sample_count(1)
            .with_label("Shadow Mask Layer"),
        )
    } else {
        RenderTargetOut::default()
    };

    graph.add_graphics_pass(ClearPass::new(
        ClearParams::new([0.0, 0.0, 0.0, 0.0]),
        ClearInput {
            pass_context: spec.pass_context,
            clear_depth_stencil: false,
        },
        ClearOutput {
            render_target: shadow_layer,
            ..Default::default()
        },
    ));
    let shadow_fill_color = [
        spec.params.color[0],
        spec.params.color[1],
        spec.params.color[2],
        (spec.params.color[3] * spec.params.opacity).clamp(0.0, 1.0),
    ];
    graph.add_graphics_pass(ShadowFillPass {
        mesh: local_shadow_mesh,
        color: shadow_fill_color,
        render_target: shadow_layer,
    });
    if spec.params.clip_to_geometry {
        graph.add_graphics_pass(ClearPass::new(
            ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            ClearInput {
                pass_context: spec.pass_context,
                clear_depth_stencil: false,
            },
            ClearOutput {
                render_target: shadow_mask_layer,
                ..Default::default()
            },
        ));
        graph.add_graphics_pass(ShadowFillPass {
            mesh: local_mask_mesh,
            color: [1.0, 1.0, 1.0, 1.0],
            render_target: shadow_mask_layer,
        });
    }

    let blur_radius_px = (spec.params.blur_radius.max(0.0) * scale).max(0.0);
    let mut composite_source = shadow_layer;
    if blur_radius_px > 0.001 {
        let blurred = graph.declare_texture(
            TextureDesc::new(
                layer_w,
                layer_h,
                SHADOW_INTERMEDIATE_FORMAT,
                wgpu::TextureDimension::D2,
            )
            .with_origin(bx as u32, by as u32)
            .with_sample_count(1)
            .with_label("Shadow Layer / Blurred"),
        );
        let built = build_blur_module(
            graph,
            BlurModuleParams {
                blur_radius: blur_radius_px,
                intermediate_format: SHADOW_INTERMEDIATE_FORMAT,
            },
            BlurModuleInput {
                layer: shadow_layer
                    .handle()
                    .map(LayerIn::with_handle)
                    .unwrap_or_default(),
                pass_context: spec.pass_context,
            },
            BlurModuleOutput {
                render_target: blurred,
            },
        );
        if built {
            composite_source = blurred;
        }
    }

    graph.add_graphics_pass(TextureCompositePass::new(
        TextureCompositeParams {
            bounds: [
                bx / scale,
                by / scale,
                layer_w as f32 / scale,
                layer_h as f32 / scale,
            ],
            uv_bounds: Some([
                bx / scale,
                by / scale,
                layer_w as f32 / scale,
                layer_h as f32 / scale,
            ]),
            mask_uv_bounds: spec.params.clip_to_geometry.then_some([
                bx / scale,
                by / scale,
                layer_w as f32 / scale,
                layer_h as f32 / scale,
            ]),
            use_mask: spec.params.clip_to_geometry,
            source_is_premultiplied: true,
            opacity: 1.0,
            ..Default::default()
        },
        TextureCompositeInput {
            source: composite_source
                .handle()
                .map(TextureCompositeSourceIn::with_handle)
                .unwrap_or_default(),
            sampled_source_key: None,
            sampled_source_size: None,
            sampled_source_upload: None,
            sampled_upload_state_key: None,
            sampled_upload_generation: None,
            sampled_source_sampling: None,
            mask: shadow_mask_layer
                .handle()
                .map(TextureCompositeMaskIn::with_handle)
                .unwrap_or_default(),
            pass_context: spec.pass_context,
        },
        TextureCompositeOutput {
            render_target: spec.output,
        },
    ));
    true
}

fn with_shadow_resources_cache<R>(f: impl FnOnce(&mut ResourceCache<ShadowResources>) -> R) -> R {
    thread_local! {
        static CACHE: RefCell<ResourceCache<ShadowResources>> = RefCell::new(ResourceCache::new());
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        f(&mut cache)
    })
}

pub fn clear_shadow_resources_cache() {
    with_shadow_resources_cache(|cache| {
        cache.clear();
    });
}

pub fn begin_shadow_resources_frame() {}

fn create_resources(device: &wgpu::Device) -> ShadowResources {
    let fill_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Fill Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/shadow_fill.wgsl").into()),
    });
    let fill_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shadow Fill Pipeline Layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let fill_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Shadow Fill Pipeline"),
        layout: Some(&fill_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &fill_shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<FillVertex>() as u64,
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
            module: &fill_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: SHADOW_INTERMEDIATE_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
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
    ShadowResources { fill_pipeline }
}

fn encode_mesh_fill_into_pass(
    device: &wgpu::Device,
    pipeline: &wgpu::RenderPipeline,
    ctx: &mut crate::view::render_pass::GraphicsCtx<'_, '_, '_, '_>,
    target_w: f32,
    target_h: f32,
    vertices: &[[f32; 2]],
    indices: &[u32],
    color: [f32; 4],
) {
    if vertices.is_empty() || indices.is_empty() || target_w <= 0.0 || target_h <= 0.0 {
        return;
    }
    let vertex_buffer = super::create_transient_buffer(
        &device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Shadow Fill Vertex Buffer"),
            contents: bytemuck::cast_slice(
                &vertices
                    .iter()
                    .map(|position| FillVertex {
                        position: [
                            (position[0] / target_w).clamp(0.0, 1.0) * 2.0 - 1.0,
                            1.0 - (position[1] / target_h).clamp(0.0, 1.0) * 2.0,
                        ],
                        color,
                    })
                    .collect::<Vec<_>>(),
            ),
            usage: wgpu::BufferUsages::VERTEX,
        },
    );
    let index_buffer = super::create_transient_buffer(
        &device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Shadow Fill Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        },
    );
    ctx.set_pipeline(pipeline);
    ctx.set_vertex_buffer(0, vertex_buffer.slice(..));
    ctx.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    ctx.draw_indexed(0..indices.len() as u32, 0, 0..1);
}

fn apply_spread(vertices: &mut [[f32; 2]], spread: f32) {
    if spread.abs() <= 0.0001 || vertices.is_empty() {
        return;
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    for [x, y] in vertices.iter() {
        cx += *x;
        cy += *y;
    }
    let inv = 1.0 / vertices.len() as f32;
    cx *= inv;
    cy *= inv;
    for v in vertices.iter_mut() {
        let dx = v[0] - cx;
        let dy = v[1] - cy;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.0001 {
            continue;
        }
        v[0] += (dx / len) * spread;
        v[1] += (dy / len) * spread;
    }
}

#[cfg(test)]
mod tests {
    use super::ShadowMesh;

    #[test]
    fn rounded_rect_uniform_matches_per_corner_api() {
        let uniform = ShadowMesh::rounded_rect(10.0, 20.0, 120.0, 70.0, 14.0);
        let per_corner =
            ShadowMesh::rounded_rect_with_radii(10.0, 20.0, 120.0, 70.0, [14.0, 14.0, 14.0, 14.0]);
        assert_eq!(uniform.vertices, per_corner.vertices);
        assert_eq!(uniform.indices, per_corner.indices);
    }

    #[test]
    fn rounded_rect_per_corner_uses_distinct_corner_radii() {
        let mesh =
            ShadowMesh::rounded_rect_with_radii(0.0, 0.0, 100.0, 60.0, [30.0, 10.0, 20.0, 5.0]);
        assert!(mesh.vertices.len() > 4);
        let first_ring = mesh.vertices[1];
        let last_ring = mesh.vertices[mesh.vertices.len() - 1];
        assert!((first_ring[0] - 90.0).abs() < 0.001);
        assert!((first_ring[1] - 0.0).abs() < 0.001);
        assert!((last_ring[0] - 30.0).abs() < 0.001);
        assert!((last_ring[1] - 0.0).abs() < 0.001);
    }
}
