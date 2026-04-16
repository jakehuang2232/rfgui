use crate::rfgui::time::Instant;
use crate::rfgui::ui::{
    RsxNode, ViewportHandle, component, use_viewport,
};
use crate::rfgui::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, InlineMeasureContext,
    InlineNodeSize, InlinePlacement, Layoutable, LayoutConstraints, LayoutPlacement, Renderable,
    UiBuildContext,
};
use crate::rfgui::view::frame_graph::slot::{InSlot, OutSlot};
use crate::rfgui::view::frame_graph::texture_resource::{TextureDesc, TextureResource};
use crate::rfgui::view::frame_graph::{
    FrameGraph, FrameResourceContext, GraphicsColorAttachmentOps, GraphicsPassBuilder,
    GraphicsPassMergePolicy, PrepareContext,
};
use crate::rfgui::view::render_pass::draw_rect_pass::{RenderTargetOut, RenderTargetTag};
use crate::rfgui::view::render_pass::{GraphicsCtx, GraphicsPass};
use crate::rfgui::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams, TextureCompositePass,
};
use crate::rfgui::register_element_factory;

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use wgpu::util::DeviceExt;

// ═══════════════════════════════════════════════════════════════════════════════
// Particle System (CPU simulation)
// ═══════════════════════════════════════════════════════════════════════════════

const MAX_PARTICLES: usize = 600;
const SPAWN_RATE: f32 = 40.0; // particles per second

/// Each particle orbits the canvas center on an elliptical path.
/// Radii are stored as a normalised fraction (0..1) of the canvas
/// short edge so that orbits scale smoothly on resize.
struct Particle {
    /// Normalised semi-major axis (fraction of short edge * 0.5).
    rx_norm: f32,
    /// Normalised semi-minor axis.
    ry_norm: f32,
    /// Current orbital angle (radians).
    angle: f32,
    /// Angular velocity (radians/sec).
    omega: f32,
    /// Orbit tilt — rotation of the ellipse itself (radians).
    tilt: f32,
    color: [f32; 4],
    /// Normalised particle size (fraction of short edge * 0.5).
    size_norm: f32,
    life: f32,
    max_life: f32,
}

struct ParticleSystemInner {
    particles: Vec<Particle>,
    last_update: Instant,
    elapsed: f32,
    spawn_accumulator: f32,
    rng: u64,
}

impl ParticleSystemInner {
    fn new() -> Self {
        Self {
            particles: Vec::with_capacity(MAX_PARTICLES),
            last_update: Instant::now(),
            elapsed: 0.0,
            spawn_accumulator: 0.0,
            rng: 0xDEAD_BEEF_CAFE_1234,
        }
    }

    fn next_f32(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng & 0xFFFF) as f32 / 65535.0
    }

    fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h2 = h / 60.0;
        let x = c * (1.0 - (h2 % 2.0 - 1.0).abs());
        let (r1, g1, b1) = if h2 < 1.0 {
            (c, x, 0.0)
        } else if h2 < 2.0 {
            (x, c, 0.0)
        } else if h2 < 3.0 {
            (0.0, c, x)
        } else if h2 < 4.0 {
            (0.0, x, c)
        } else if h2 < 5.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        let m = l - c * 0.5;
        [r1 + m, g1 + m, b1 + m]
    }

    fn update(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32().min(0.05);
        self.last_update = now;
        self.elapsed += dt;

        // Advance orbits.
        for p in &mut self.particles {
            p.angle += p.omega * dt;
            p.life -= dt / p.max_life;
        }
        self.particles.retain(|p| p.life > 0.0);

        // Spawn new orbiting particles.
        self.spawn_accumulator += SPAWN_RATE * dt;
        let to_spawn = self.spawn_accumulator as usize;
        self.spawn_accumulator -= to_spawn as f32;

        for _ in 0..to_spawn {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            // Normalised orbit radius: 0.05 .. 0.9 of half-short-edge.
            let r_norm = 0.05 + self.next_f32() * 0.85;
            let eccentricity = 0.3 + self.next_f32() * 0.6;
            let rx_norm = r_norm;
            let ry_norm = r_norm * eccentricity;
            let tilt = self.next_f32() * std::f32::consts::TAU;
            let angle = self.next_f32() * std::f32::consts::TAU;

            // Kepler-ish: inner orbits faster.
            let omega_sign = if self.next_f32() > 0.3 { 1.0 } else { -1.0 };
            let omega = omega_sign * (0.4 + 1.5 / (r_norm * 4.0 + 0.5).sqrt());

            // Colour: inner=warm, outer=cool.
            let hue_base = r_norm * 240.0;
            let hue = hue_base + (self.next_f32() - 0.5) * 40.0;
            let [r, g, b] = Self::hsl_to_rgb(hue.rem_euclid(360.0), 0.85, 0.6);

            let max_life = 4.0 + self.next_f32() * 6.0;
            let size_norm = 0.005 + self.next_f32() * 0.012;

            self.particles.push(Particle {
                rx_norm,
                ry_norm,
                angle,
                omega,
                tilt,
                color: [r, g, b, 1.0],
                size_norm,
                life: 1.0,
                max_life,
            });
        }
    }

    fn to_vertex_data(&self, canvas_width: f32, canvas_height: f32) -> Vec<f32> {
        let cx = canvas_width * 0.5;
        let cy = canvas_height * 0.5;
        let half_short = canvas_width.min(canvas_height) * 0.5;
        let mut data = Vec::with_capacity(self.particles.len() * 8);
        for p in &self.particles {
            let rx = p.rx_norm * half_short;
            let ry = p.ry_norm * half_short;
            let size = p.size_norm * half_short;
            // Elliptical position in local orbit frame.
            let lx = rx * p.angle.cos();
            let ly = ry * p.angle.sin();
            // Rotate by orbit tilt.
            let cos_t = p.tilt.cos();
            let sin_t = p.tilt.sin();
            let px = cx + lx * cos_t - ly * sin_t;
            let py = cy + lx * sin_t + ly * cos_t;

            data.push(px);
            data.push(py);
            data.push(p.color[0]);
            data.push(p.color[1]);
            data.push(p.color[2]);
            data.push(p.color[3]);
            data.push(size);
            data.push(p.life);
        }
        data
    }
}

thread_local! {
    static PARTICLE_SYSTEM: RefCell<ParticleSystemInner> = RefCell::new(ParticleSystemInner::new());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ParticlePass — renders particles to an offscreen texture
// ═══════════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParticleUniforms {
    screen_size: [f32; 2],
    canvas_pos: [f32; 2],
    canvas_size: [f32; 2],
    time: f32,
    _pad: f32,
}

struct ParticlePassResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    sample_count: u32,
}

thread_local! {
    static PIPELINE_CACHE: RefCell<Option<ParticlePassResources>> = const { RefCell::new(None) };
}

fn get_or_create_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
    PIPELINE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(res) = cache.as_ref() {
            if res.format == format && res.sample_count == sample_count {
                return (res.bind_group_layout.clone(), res.pipeline.clone());
            }
        }

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/particle.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Particle BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Particle Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: (8 * size_of::<f32>()) as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 24,
                    shader_location: 2,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let bgl_clone = bind_group_layout.clone();
        let pipeline_clone = pipeline.clone();
        *cache = Some(ParticlePassResources {
            pipeline,
            bind_group_layout,
            format,
            sample_count,
        });
        (bgl_clone, pipeline_clone)
    })
}

/// Renders particles into an offscreen texture (clear + draw).
struct ParticlePass {
    uniforms: ParticleUniforms,
    vertex_data: Vec<f32>,
    particle_count: u32,
    offscreen_target: RenderTargetOut,
    surface_format: wgpu::TextureFormat,
    // Prepared GPU resources
    uniform_buffer: Option<wgpu::Buffer>,
    vertex_buffer: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
}

impl GraphicsPass for ParticlePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::RequiresOwnPass);
        // Clear the offscreen texture then render particles into it.
        builder.write_color(
            &self.offscreen_target,
            GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]),
        );
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        if self.particle_count == 0 {
            return;
        }
        let viewport = ctx.viewport();
        let device = viewport.device().expect("no GPU device");
        let format = self.surface_format;

        let (bgl, _) = get_or_create_resources(device, format, 1);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Particle Uniforms"),
            contents: bytemuck::bytes_of(&self.uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particle BG"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Particle Vertices"),
            contents: bytemuck::cast_slice(&self.vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        self.uniform_buffer = Some(uniform_buffer);
        self.vertex_buffer = Some(vertex_buffer);
        self.bind_group = Some(bind_group);
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        if self.particle_count == 0 {
            return;
        }
        let Some(bind_group) = &self.bind_group else {
            return;
        };
        let Some(vertex_buffer) = &self.vertex_buffer else {
            return;
        };

        let device = ctx.viewport().device().expect("no GPU device");
        let (_, pipeline) = get_or_create_resources(device, self.surface_format, 1);

        ctx.set_pipeline(&pipeline);
        ctx.set_bind_group(0, bind_group, &[]);
        ctx.set_vertex_buffer(0, vertex_buffer.slice(..));
        ctx.draw(0..6, 0..self.particle_count);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ParticleCanvas — ElementTrait impl
// ═══════════════════════════════════════════════════════════════════════════════

fn stable_id(tag: &str, path: &[u64]) -> u64 {
    let mut hasher = DefaultHasher::new();
    tag.hash(&mut hasher);
    path.hash(&mut hasher);
    hasher.finish()
}

pub struct ParticleCanvas {
    id: u64,
    parent_id: Option<u64>,
    // position (relative to parent)
    offset_x: f32,
    offset_y: f32,
    // layout results
    layout_x: f32,
    layout_y: f32,
    layout_w: f32,
    layout_h: f32,
    // measured
    target_w: f32,
    target_h: f32,
    should_render: bool,
}

impl ParticleCanvas {
    pub fn new(id: u64, w: f32, h: f32) -> Self {
        Self {
            id,
            parent_id: None,
            offset_x: 0.0,
            offset_y: 0.0,
            layout_x: 0.0,
            layout_y: 0.0,
            layout_w: w,
            layout_h: h,
            target_w: w,
            target_h: h,
            should_render: true,
        }
    }
}

impl Layoutable for ParticleCanvas {
    fn measure(&mut self, constraints: LayoutConstraints) {
        // width:100%, height:100% — use percent base (parent content size).
        if let Some(w) = constraints.percent_base_width {
            self.target_w = w;
        }
        if let Some(h) = constraints.percent_base_height {
            self.target_h = h;
        }
    }

    fn place(&mut self, placement: LayoutPlacement) {
        self.layout_x = placement.parent_x;
        self.layout_y = placement.parent_y;
        self.layout_w = self.target_w;
        self.layout_h = self.target_h;
        self.should_render = self.layout_w > 0.0 && self.layout_h > 0.0;
    }

    fn measured_size(&self) -> (f32, f32) {
        (self.target_w, self.target_h)
    }

    fn set_layout_width(&mut self, w: f32) {
        self.target_w = w;
    }
    fn set_layout_height(&mut self, h: f32) {
        self.target_h = h;
    }

    fn allows_cross_stretch(&self, _is_row: bool) -> bool {
        true
    }
    fn cross_alignment_size(&self, is_row: bool, _: Option<f32>) -> f32 {
        if is_row { self.target_h } else { self.target_w }
    }
    fn flex_grow(&self) -> f32 { 1.0 }
    fn flex_shrink(&self) -> f32 { 1.0 }
    fn flex_basis(&self) -> rfgui::SizeValue { rfgui::SizeValue::Auto }
    fn flex_main_size(&self, _is_row: bool) -> rfgui::SizeValue { rfgui::SizeValue::Auto }
    fn flex_has_explicit_min_main_size(&self, _: bool) -> bool { false }
    fn flex_auto_min_main_size(&self, _: bool) -> Option<f32> { None }
    fn flex_min_main_size(&self, _: bool) -> rfgui::SizeValue { rfgui::SizeValue::Auto }
    fn flex_max_main_size(&self, _: bool) -> rfgui::SizeValue { rfgui::SizeValue::Auto }
    fn inline_relative_position(&self) -> (f32, f32) { (self.offset_x, self.offset_y) }
    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.offset_x = x;
        self.offset_y = y;
    }
    fn measure_inline(&mut self, ctx: InlineMeasureContext) {
        self.measure(LayoutConstraints {
            max_width: ctx.first_available_width, max_height: 1_000_000.0,
            viewport_width: ctx.viewport_width, viewport_height: ctx.viewport_height,
            percent_base_width: ctx.percent_base_width, percent_base_height: ctx.percent_base_height,
        });
    }
    fn get_inline_nodes_size(&self) -> Vec<InlineNodeSize> {
        vec![InlineNodeSize { width: self.target_w, height: self.target_h }]
    }
    fn place_inline(&mut self, p: InlinePlacement) {
        self.set_layout_offset(p.offset_x, p.offset_y);
        self.place(LayoutPlacement {
            parent_x: p.parent_x, parent_y: p.parent_y,
            visual_offset_x: p.visual_offset_x, visual_offset_y: p.visual_offset_y,
            available_width: p.available_width, available_height: p.available_height,
            viewport_width: p.viewport_width, viewport_height: p.viewport_height,
            percent_base_width: p.percent_base_width, percent_base_height: p.percent_base_height,
        });
    }
}

impl EventTarget for ParticleCanvas {}

impl Renderable for ParticleCanvas {
    fn build(&mut self, graph: &mut FrameGraph, ctx: UiBuildContext) -> BuildState {
        if !self.should_render {
            return ctx.into_state();
        }

        // Keep animating.
        ViewportHandle.request_redraw();

        let viewport = ctx.viewport();
        let format = viewport.target_format();
        let scale = viewport.scale_factor();
        let canvas_w = self.layout_w;
        let canvas_h = self.layout_h;
        let canvas_x = self.layout_x;
        let canvas_y = self.layout_y;

        // 1. Declare an offscreen texture at physical pixel resolution.
        let tex_w = (canvas_w * scale).ceil() as u32;
        let tex_h = (canvas_h * scale).ceil() as u32;

        let offscreen: OutSlot<TextureResource, RenderTargetTag> = graph.declare_texture(
            TextureDesc::new(
                tex_w,
                tex_h,
                format,
                wgpu::TextureDimension::D2,
            )
            .with_label("ParticleCanvas Offscreen"),
        );

        // 2. Update particle state and add particle render pass.
        //    Particle system works in physical pixels so rendering is crisp.
        let phys_w = tex_w as f32;
        let phys_h = tex_h as f32;
        PARTICLE_SYSTEM.with(|sys| {
            let mut sys = sys.borrow_mut();
            sys.update();
            let vertex_data = sys.to_vertex_data(phys_w, phys_h);
            let count = sys.particles.len() as u32;
            let time = sys.elapsed;

            graph.add_graphics_pass(ParticlePass {
                uniforms: ParticleUniforms {
                    screen_size: [phys_w, phys_h],
                    canvas_pos: [0.0, 0.0],
                    canvas_size: [phys_w, phys_h],
                    time,
                    _pad: 0.0,
                },
                vertex_data,
                particle_count: count,
                offscreen_target: offscreen,
                surface_format: format,
                uniform_buffer: None,
                vertex_buffer: None,
                bind_group: None,
            });
        });

        // 3. Composite the offscreen texture onto the output target.
        let output_target = ctx.current_target().unwrap_or_default();

        // Connect the offscreen OutSlot → composite input InSlot.
        let source_handle = offscreen.handle().expect("offscreen has no handle");
        let source_in: InSlot<TextureResource, _> = InSlot::with_handle(source_handle);

        graph.add_graphics_pass(TextureCompositePass::new(
            TextureCompositeParams {
                bounds: [canvas_x, canvas_y, canvas_w, canvas_h],
                quad_positions: None,
                uv_bounds: None,
                mask_uv_bounds: None,
                use_mask: false,
                source_is_premultiplied: true,
                opacity: 1.0,
                scissor_rect: None,
            },
            TextureCompositeInput {
                source: source_in,
                sampled_source_key: None,
                sampled_source_size: None,
                sampled_source_upload: None,
                sampled_upload_state_key: None,
                sampled_upload_generation: None,
                sampled_source_sampling: None,
                mask: Default::default(),
                pass_context: Default::default(),
            },
            TextureCompositeOutput {
                render_target: output_target,
            },
        ));

        ctx.into_state()
    }
}

impl ElementTrait for ParticleCanvas {
    fn id(&self) -> u64 { self.id }
    fn parent_id(&self) -> Option<u64> { self.parent_id }
    fn set_parent_id(&mut self, id: Option<u64>) { self.parent_id = id; }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id, parent_id: self.parent_id,
            x: self.layout_x, y: self.layout_y,
            width: self.layout_w, height: self.layout_h,
            border_radius: 0.0, should_render: self.should_render,
        }
    }

    fn children(&self) -> Option<&[Box<dyn ElementTrait>]> { None }
    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]> { None }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn promotion_self_signature(&self) -> u64 {
        // Always changing → prevents promotion from caching stale frames.
        use std::sync::atomic::{AtomicU64, Ordering};
        static CTR: AtomicU64 = AtomicU64::new(1);
        CTR.fetch_add(1, Ordering::Relaxed)
    }

    fn local_dirty_flags(&self) -> DirtyFlags { DirtyFlags::ALL }
    fn clear_local_dirty_flags(&mut self, _: DirtyFlags) {}
}

// ═══════════════════════════════════════════════════════════════════════════════
// Factory registration + ParticleDemo component
// ═══════════════════════════════════════════════════════════════════════════════

pub fn register_particle_canvas() {
    register_element_factory(
        "ParticleCanvas",
        Arc::new(|_node, path| {
            // Size defaults to 0 → will be filled by parent constraints during layout.
            Ok(Box::new(ParticleCanvas::new(stable_id("ParticleCanvas", path), 0.0, 0.0)))
        }),
    );
}

#[component]
pub fn ParticleDemo() -> RsxNode {
    let viewport = use_viewport();
    viewport.request_redraw();
    RsxNode::element("ParticleCanvas")
}
