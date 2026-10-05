use crate::view::frame_graph::texture_resource::TextureHandle;
use crate::view::frame_graph::{
    FrameGraph, GraphicsColorAttachmentOps, GraphicsPassBuilder, PersistentTextureKey, TextureDesc,
};
use crate::view::render_pass::GraphicsPass;
use crate::view::render_pass::blur_module::{
    BlurModuleInput, BlurModuleOutput, BlurModuleParams, blur_downsample_factor, build_blur_module,
};
use crate::view::render_pass::composite_layer_pass::LayerIn;
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{GraphicsPassContext, render_target_ref};
use crate::view::render_pass::texture_composite_pass::{
    NinePatchComposite, TextureCompositeInput, TextureCompositeMaskIn, TextureCompositeOutput,
    TextureCompositeParams, TextureCompositePass, TextureCompositeSourceIn,
};
use rustc_hash::{FxHashMap, FxHashSet};

const SHADOW_RESOURCES: u64 = 203;
const SHADOW_INTERMEDIATE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// A rounded rectangle that casts a shadow, in logical coordinates.
/// Radii are top-left, top-right, bottom-right, bottom-left and are
/// normalized to the rectangle when constructed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowShape {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radii: [f32; 4],
}

impl ShadowShape {
    pub fn rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Self {
        Self::rounded_rect_with_radii(x, y, width, height, [radius; 4])
    }

    pub fn rounded_rect_with_radii(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radii: [f32; 4],
    ) -> Self {
        let width = width.max(0.0);
        let height = height.max(0.0);
        Self {
            x,
            y,
            width,
            height,
            radii: normalize_corner_radii(radii, width, height),
        }
    }

    /// A drawable shape: finite, non-empty, with non-negative radii.
    pub fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .into_iter()
            .chain(self.radii)
            .all(f32::is_finite)
            && self.width > 0.0
            && self.height > 0.0
            && self.radii.iter().all(|radius| *radius >= 0.0)
    }

    pub fn translated(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    fn scaled(self, scale: f32) -> Self {
        Self {
            x: self.x * scale,
            y: self.y * scale,
            width: self.width * scale,
            height: self.height * scale,
            radii: self.radii.map(|radius| radius * scale),
        }
    }

    /// Triangle fan approximating the outline with six segments per corner.
    fn fill_mesh(&self) -> (Vec<[f32; 2]>, Vec<u32>) {
        let Self {
            x,
            y,
            width: w,
            height: h,
            radii: [tl, tr, br, bl],
        } = *self;
        if w <= 0.0 || h <= 0.0 {
            return (Vec::new(), Vec::new());
        }
        if tl <= 0.001 && tr <= 0.001 && br <= 0.001 && bl <= 0.001 {
            return (
                vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                vec![0, 1, 2, 0, 2, 3],
            );
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
        let mut vertices = Vec::with_capacity(ring.len() + 1);
        vertices.push([x + w * 0.5, y + h * 0.5]);
        vertices.extend(ring.iter().copied());
        let ring_len = ring.len() as u32;
        let mut indices = Vec::with_capacity(ring.len() * 3);
        for i in 0..ring_len {
            indices.extend_from_slice(&[0, 1 + i, 1 + (i + 1) % ring_len]);
        }
        (vertices, indices)
    }
}

/// Identity of one cached shadow template: a blurred (or, at zero blur,
/// plain) coverage mask of a rounded rectangle, independent of where it is
/// drawn and of its color. All values are physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShadowTemplateKey {
    shape_len_bits: [u32; 2],
    shape_offset_bits: [u32; 2],
    radii_bits: [u32; 4],
    blur_bits: u32,
    extent: [u32; 2],
}

impl ShadowTemplateKey {
    pub(crate) fn texture_desc(&self) -> TextureDesc {
        TextureDesc::new(
            self.extent[0],
            self.extent[1],
            SHADOW_INTERMEDIATE_FORMAT,
            wgpu::TextureDimension::D2,
        )
        .with_sample_count(1)
        .with_label("Shadow Template")
    }
}

/// Templates a frame graph may read without producing them, and the ones it
/// declared. A viewport supplies the resident set before build.
#[derive(Default)]
pub(crate) struct ShadowTemplateFrame {
    pub(crate) resident: FxHashSet<ShadowTemplateKey>,
    declared: FxHashMap<ShadowTemplateKey, TextureHandle>,
}

/// One axis of a shadow layer and its nine-patch template, in physical
/// pixels. The template keeps every pixel within `radius + influence` of
/// either shape edge; between them the coverage is constant along this axis,
/// so the destination repeats template texel `split` for `stretch` pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TemplateAxis {
    origin: i64,
    layer: u32,
    template: u32,
    shape_offset: f32,
    shape_len: f32,
    split: u32,
    stretch: u32,
}

impl TemplateAxis {
    fn new(
        start: f32,
        len: f32,
        radius: f32,
        pad: u32,
        influence: f32,
        align: u32,
    ) -> Option<Self> {
        let origin = start.floor() - pad as f32;
        if ![start, len, radius, origin].into_iter().all(f32::is_finite)
            || len <= 0.0
            || origin.abs() > 1.0e9
            || len > 1.0e7
        {
            return None;
        }
        let shape_offset = start - origin;
        let layer = (shape_offset + len).ceil() as u32 + pad;
        let min_shape = 2.0 * (radius + influence) + 4.0;
        let stretch = if len >= min_shape + align as f32 {
            ((len - min_shape) / align as f32).floor() as u32 * align
        } else {
            0
        };
        let shape_len = len - stretch as f32;
        let template = ((shape_offset + shape_len).ceil() as u32 + pad).div_ceil(align) * align;
        let split = if stretch > 0 {
            (shape_offset + radius + influence).ceil() as u32
        } else {
            template
        };
        Some(Self {
            origin: origin as i64,
            layer,
            template,
            shape_offset,
            shape_len,
            split,
            stretch,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct TemplatePlan {
    key: ShadowTemplateKey,
    x: TemplateAxis,
    y: TemplateAxis,
    radii: [f32; 4],
    blur: f32,
}

impl TemplatePlan {
    /// `shape` and `blur` in physical pixels.
    fn new(shape: ShadowShape, blur: f32) -> Option<Self> {
        if !shape.is_valid() || !blur.is_finite() || blur < 0.0 {
            return None;
        }
        let blurred = blur > 0.001;
        let pad = (blur * 1.5).ceil() as u32;
        let (influence, align) = if blurred {
            let downsample = blur_downsample_factor(blur);
            ((blur.ceil() as u32 + 2 * downsample + 2) as f32, downsample)
        } else {
            (1.0, 1)
        };
        let radius = shape.radii.into_iter().fold(0.0_f32, f32::max);
        let x = TemplateAxis::new(shape.x, shape.width, radius, pad, influence, align)?;
        let y = TemplateAxis::new(shape.y, shape.height, radius, pad, influence, align)?;
        let blur = if blurred { blur } else { 0.0 };
        Some(Self {
            key: ShadowTemplateKey {
                shape_len_bits: [x.shape_len, y.shape_len].map(f32::to_bits),
                shape_offset_bits: [x.shape_offset, y.shape_offset].map(f32::to_bits),
                radii_bits: shape.radii.map(f32::to_bits),
                blur_bits: blur.to_bits(),
                extent: [x.template, y.template],
            },
            x,
            y,
            radii: shape.radii,
            blur,
        })
    }

    /// Declares this template in `graph`, producing it unless the viewport
    /// reported it resident.
    fn ensure(&self, graph: &mut FrameGraph, pass_context: GraphicsPassContext) -> TextureHandle {
        if let Some(handle) = graph.shadow_templates.declared.get(&self.key) {
            return *handle;
        }
        let output: RenderTargetOut = graph.declare_persistent_texture_internal(
            self.key.texture_desc(),
            PersistentTextureKey::ShadowTemplate(self.key),
        );
        let handle = output.handle().expect("declared shadow template");
        graph.shadow_templates.declared.insert(self.key, handle);
        if graph.shadow_templates.resident.contains(&self.key) {
            return handle;
        }
        let (vertices, indices) = ShadowShape {
            x: self.x.shape_offset,
            y: self.y.shape_offset,
            width: self.x.shape_len,
            height: self.y.shape_len,
            radii: self.radii,
        }
        .fill_mesh();
        let fill = |render_target| ShadowFillPass {
            vertices,
            indices,
            color: [1.0; 4],
            render_target,
            template: Some(self.key),
        };
        if self.blur == 0.0 {
            graph.add_graphics_pass(fill(output));
            return handle;
        }
        let coverage = graph.declare_texture(
            TextureDesc::new(
                self.key.extent[0],
                self.key.extent[1],
                SHADOW_INTERMEDIATE_FORMAT,
                wgpu::TextureDimension::D2,
            )
            .with_sample_count(1)
            .with_label("Shadow Template / Coverage"),
        );
        let coverage_handle = coverage.handle().expect("declared shadow coverage");
        graph.add_graphics_pass(fill(coverage));
        let built = build_blur_module(
            graph,
            BlurModuleParams {
                blur_radius: self.blur,
                intermediate_format: SHADOW_INTERMEDIATE_FORMAT,
            },
            BlurModuleInput {
                layer: LayerIn::with_handle(coverage_handle),
                pass_context,
            },
            BlurModuleOutput {
                render_target: output,
            },
        );
        debug_assert!(built, "a declared coverage layer always blurs");
        handle
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShadowParams {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: [f32; 4],
    pub opacity: f32,
    /// Show the shadow only inside the unshifted shape.
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
            clip_to_geometry: false,
        }
    }
}

#[derive(Clone)]
pub struct ShadowModuleSpec {
    pub shape: ShadowShape,
    pub params: ShadowParams,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub scale_factor: f32,
    pub pass_context: GraphicsPassContext,
    pub output: RenderTargetOut,
}

/// Fills a template's coverage. A template producer reports its write so
/// the viewport can trust the template once the frame is submitted.
pub(crate) struct ShadowFillPass {
    vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
    color: [f32; 4],
    render_target: RenderTargetOut,
    template: Option<ShadowTemplateKey>,
}

#[cfg(test)]
impl ShadowFillPass {
    pub(crate) fn test_snapshot(&self) -> ShadowFillPassTestSnapshot {
        ShadowFillPassTestSnapshot {
            vertices_bits: self
                .vertices
                .iter()
                .map(|vertex| vertex.map(f32::to_bits))
                .collect(),
            indices: self.indices.clone(),
            color_bits: self.color.map(f32::to_bits),
            render_target: self.render_target.handle(),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShadowFillPassTestSnapshot {
    pub(crate) vertices_bits: Vec<[u32; 2]>,
    pub(crate) indices: Vec<u32>,
    pub(crate) color_bits: [u32; 4],
    pub(crate) render_target: Option<crate::view::frame_graph::texture_resource::TextureHandle>,
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
        if let Some(target) = builder.texture_target(&self.render_target) {
            let _ = target;
            // This pass owns a newly declared shadow/mask scratch target.
            // Clear in its attachment load instead of opening a separate
            // clear-only pass immediately before this fill.
            builder.write_color(
                &self.render_target,
                GraphicsColorAttachmentOps::clear([0.; 4]),
            );
        }
    }

    fn execute(&mut self, ctx: &mut crate::view::render_pass::GraphicsCtx<'_, '_, '_, '_>) {
        // Executing a producer means its whole chain into the template is
        // live: every later stage consumes this pass's output.
        if let Some(key) = self.template {
            ctx.viewport().note_shadow_template_written(key);
            crate::ui::work_profile::count(|p| p.shadow_template_builds += 1);
        }
        if self.vertices.is_empty() || self.indices.is_empty() {
            return;
        }
        let Some(device) = ctx.viewport().device().cloned() else {
            ctx.mark_execution_failed();
            return;
        };
        let surface_size = ctx.viewport().surface_size();
        let (target_w, target_h) = match self.render_target.handle() {
            Some(handle) => {
                let Some(texture_ref) = render_target_ref(ctx.frame_resources(), handle) else {
                    ctx.mark_execution_failed();
                    return;
                };
                texture_ref.physical_size()
            }
            None => surface_size,
        };
        if target_w == 0 || target_h == 0 {
            return;
        }
        let pipeline = with_shadow_resources_cache(|cache| {
            let resources = cache.get_or_insert_scoped_with(
                ctx.viewport().render_resource_scope_id(),
                SHADOW_RESOURCES,
                || create_resources(&device),
            );
            resources.fill_pipeline.clone()
        });
        encode_mesh_fill_into_pass(
            &device,
            &pipeline,
            ctx,
            target_w as f32,
            target_h as f32,
            &self.vertices,
            &self.indices,
            self.color,
        );
    }
}

/// Draws a shadow as one stretched nine-patch of a cached template. Like
/// Firefox WebRender and WebKit's tiled shadows, the blur runs once per
/// template, not per frame: moving or resizing a shadow only redraws it.
pub fn build_shadow_module(graph: &mut FrameGraph, spec: ShadowModuleSpec) -> bool {
    let scale = spec.scale_factor.max(0.0001);
    if !spec.shape.is_valid() {
        return false;
    }
    let shadow_shape = spec
        .shape
        .translated(spec.params.offset_x, spec.params.offset_y)
        .scaled(scale);
    let Some(shadow) = TemplatePlan::new(shadow_shape, spec.params.blur_radius.max(0.0) * scale)
    else {
        return false;
    };
    // Skip a layer entirely outside the target, as the per-frame blur did.
    let target = [spec.viewport_width as i64, spec.viewport_height as i64];
    let visible = |axis: TemplateAxis, extent: i64| {
        axis.origin.clamp(0, extent) < (axis.origin + axis.layer as i64).clamp(0, extent)
    };
    if !visible(shadow.x, target[0]) || !visible(shadow.y, target[1]) {
        return false;
    }
    let mask = if spec.params.clip_to_geometry {
        let Some(mask) = TemplatePlan::new(spec.shape.scaled(scale), 0.0) else {
            return false;
        };
        Some(mask)
    } else {
        None
    };

    let source = shadow.ensure(graph, spec.pass_context);
    let mask_source = mask.map(|mask| mask.ensure(graph, spec.pass_context));
    let alpha = (spec.params.color[3] * spec.params.opacity).clamp(0.0, 1.0);
    let [r, g, b, _] = spec.params.color;
    let offset = |mask: i64, source: i64| i32::try_from(mask - source).unwrap_or(0);
    graph.add_graphics_pass(TextureCompositePass::new(
        TextureCompositeParams {
            bounds: [
                shadow.x.origin as f32 / scale,
                shadow.y.origin as f32 / scale,
                shadow.x.layer as f32 / scale,
                shadow.y.layer as f32 / scale,
            ],
            use_mask: mask.is_some(),
            source_is_premultiplied: true,
            opacity: 1.0,
            nine_patch: Some(NinePatchComposite {
                split: [shadow.x.split, shadow.y.split],
                stretch: [shadow.x.stretch, shadow.y.stretch],
                mask_split: mask.map_or([0; 2], |mask| [mask.x.split, mask.y.split]),
                mask_stretch: mask.map_or([0; 2], |mask| [mask.x.stretch, mask.y.stretch]),
                mask_offset: mask.map_or([0; 2], |mask| {
                    [
                        offset(mask.x.origin, shadow.x.origin),
                        offset(mask.y.origin, shadow.y.origin),
                    ]
                }),
                tint: [r * alpha, g * alpha, b * alpha, alpha],
            }),
            ..Default::default()
        },
        TextureCompositeInput::from_render_target(
            TextureCompositeSourceIn::with_handle(source),
            mask_source
                .map(TextureCompositeMaskIn::with_handle)
                .unwrap_or_default(),
            spec.pass_context,
        ),
        TextureCompositeOutput {
            render_target: spec.output,
        },
    ));
    true
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

crate::static_resource_cache! {
    fn with_shadow_resources_cache -> ResourceCache<ShadowResources> = stats("shadow_pipeline")
}

pub fn clear_shadow_resources_cache() {
    with_shadow_resources_cache(|cache| {
        cache.clear();
    });
}

pub(super) fn release_scope(scope: u64) {
    with_shadow_resources_cache(|cache| cache.clear_scope(scope));
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
            buffers: &[Some(wgpu::VertexBufferLayout {
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
            })],
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

#[cfg(test)]
mod tests;
