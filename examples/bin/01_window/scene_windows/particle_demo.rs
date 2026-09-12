use crate::rfgui::time::Instant;
use crate::rfgui::ui::{
    PointerButton, PointerDownEvent, PointerMoveEvent, PointerUpEvent, RsxElementNode, RsxNode,
    ViewportHandle, component, use_viewport,
};
use crate::rfgui::view::base_component::PaintResourcePreparationContext;
use crate::rfgui::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, Renderable, UiBuildContext,
};
use crate::rfgui::view::frame_graph::FrameGraph;
use crate::rfgui::view::gpu_paint::{GpuPaintProgram, GpuPaintSource, GpuPaintSourceId};
use crate::rfgui::view::viewport::ViewportControl;
use crate::rfgui::view::{BuildCtx, ElementDescriptor, HostBuilder, host_builder_node};
use std::sync::Arc;

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// ═══════════════════════════════════════════════════════════════════════════════
// Particle System (CPU simulation)
// ═══════════════════════════════════════════════════════════════════════════════

const MAX_PARTICLES: usize = 1000;
const SPAWN_RATE: f32 = 100.0; // particles per second

/// 3D particle in normalised space. Projected to 2D for rendering.
struct Particle {
    x: f32,
    y: f32,
    z: f32,
    vx: f32,
    vy: f32,
    vz: f32,
    color: [f32; 4],
    size_norm: f32,
    life: f32,
    max_life: f32,
}

const GM: f32 = 0.055;
const SOFTENING2: f32 = 0.000004;
/// Perspective camera distance (normalised units).
const CAM_DIST: f32 = 1.5;

struct ParticleSystemInner {
    particles: Vec<Particle>,
    last_update: Instant,
    elapsed: f32,
    spawn_accumulator: f32,
    rng: u64,
    attractor: Option<(f32, f32)>,
    /// Central mass 3D position (z stays 0 — mass lives on screen plane).
    mass_x: f32,
    mass_y: f32,
    mass_vx: f32,
    mass_vy: f32,
    /// Left mouse button held → boost central mass.
    mass_boost: bool,
}

impl ParticleSystemInner {
    fn new() -> Self {
        Self {
            particles: Vec::with_capacity(MAX_PARTICLES),
            last_update: Instant::now(),
            elapsed: 0.0,
            spawn_accumulator: 0.0,
            rng: 0xDEAD_BEEF_CAFE_1234,
            attractor: None,
            mass_x: 0.5,
            mass_y: 0.5,
            mass_vx: 0.0,
            mass_vy: 0.0,
            mass_boost: false,
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

    fn set_attractor(&mut self, pos: Option<(f32, f32)>) {
        self.attractor = pos;
    }

    fn set_mass_boost(&mut self, v: bool) {
        self.mass_boost = v;
    }

    fn update(&mut self, now: Instant) {
        let dt = now.duration_since(self.last_update).as_secs_f32().min(0.05);
        self.last_update = now;
        self.elapsed += dt;

        // Central mass attracted to mouse (or center) by gravity + damping.
        let target = self.attractor.unwrap_or((0.5, 0.5));
        let spring = 30.0_f32; // spring stiffness
        let damping = 6.0_f32; // velocity damping
        let dx_m = target.0 - self.mass_x;
        let dy_m = target.1 - self.mass_y;
        self.mass_vx += dx_m * spring * dt;
        self.mass_vy += dy_m * spring * dt;
        self.mass_vx *= (1.0 - damping * dt).max(0.0);
        self.mass_vy *= (1.0 - damping * dt).max(0.0);
        self.mass_x += self.mass_vx * dt;
        self.mass_y += self.mass_vy * dt;

        let cx = self.mass_x;
        let cy = self.mass_y;

        // 3D gravity: mass at (cx, cy, 0).
        let cz = 0.0_f32;
        let gm_eff = if self.mass_boost { GM * 4.0 } else { GM };
        for p in &mut self.particles {
            let dx = cx - p.x;
            let dy = cy - p.y;
            let dz = cz - p.z;
            let r2 = dx * dx + dy * dy + dz * dz + SOFTENING2;
            let r = r2.sqrt();
            let a = gm_eff / r2;
            p.vx += a * dx / r * dt;
            p.vy += a * dy / r * dt;
            p.vz += a * dz / r * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.z += p.vz * dt;
            p.life -= dt / p.max_life;
        }
        self.particles.retain(|p| p.life > 0.0);

        // Spawn in random 3D orbits around mass.
        self.spawn_accumulator += SPAWN_RATE * dt;
        let to_spawn = self.spawn_accumulator as usize;
        self.spawn_accumulator -= to_spawn as f32;

        for _ in 0..to_spawn {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            let r = 0.04 + self.next_f32() * 0.38;

            // Random point on sphere at distance r from mass.
            let cos_phi = self.next_f32() * 2.0 - 1.0; // -1..1
            let sin_phi = (1.0 - cos_phi * cos_phi).sqrt();
            let theta = self.next_f32() * std::f32::consts::TAU;
            let px = cx + r * sin_phi * theta.cos();
            let py = cy + r * sin_phi * theta.sin();
            let pz = cz + r * cos_phi;

            // Circular orbit speed.
            let v_circ = (GM / (r + SOFTENING2.sqrt())).sqrt();
            // Random tangent direction perpendicular to radius vector.
            // Pick a random axis, cross with radius to get tangent.
            let rand_ax = self.next_f32() - 0.5;
            let rand_ay = self.next_f32() - 0.5;
            let rand_az = self.next_f32() - 0.5;
            let rx = px - cx;
            let ry = py - cy;
            let rz = pz - cz;
            // cross(rand, r)
            let tx = rand_ay * rz - rand_az * ry;
            let ty = rand_az * rx - rand_ax * rz;
            let tz = rand_ax * ry - rand_ay * rx;
            let t_len = (tx * tx + ty * ty + tz * tz).sqrt().max(0.0001);
            let perturb = 0.85 + self.next_f32() * 0.3;
            let v = v_circ * perturb;
            let vx = v * tx / t_len;
            let vy = v * ty / t_len;
            let vz = v * tz / t_len;

            let hue = (r / 0.42) * 240.0 + (self.next_f32() - 0.5) * 40.0;
            let [cr, cg, cb] = Self::hsl_to_rgb(hue.rem_euclid(360.0), 0.85, 0.6);
            let max_life = 5.0 + self.next_f32() * 8.0;
            let size_norm = 0.004 + self.next_f32() * 0.010;

            self.particles.push(Particle {
                x: px,
                y: py,
                z: pz,
                vx,
                vy,
                vz,
                color: [cr, cg, cb, 1.0],
                size_norm,
                life: 1.0,
                max_life,
            });
        }
    }

    fn to_vertex_data(&self, canvas_width: f32, canvas_height: f32) -> Vec<f32> {
        let half_short = canvas_width.min(canvas_height) * 0.5;
        let mut data = Vec::with_capacity(self.particles.len() * 8);
        for p in &self.particles {
            // Perspective projection: objects closer to camera appear larger.
            let depth = CAM_DIST - p.z; // camera at z = CAM_DIST, looking toward z=0
            let scale = if depth > 0.01 {
                CAM_DIST / depth
            } else {
                CAM_DIST / 0.01
            };
            // Project around canvas center.
            let cx = canvas_width * 0.5;
            let cy = canvas_height * 0.5;
            let px = cx + (p.x * canvas_width - cx) * scale;
            let py = cy + (p.y * canvas_height - cy) * scale;
            let size = p.size_norm * half_short * scale;
            // Depth fade: particles further away are slightly dimmer.
            let depth_fade = (scale * 0.7).clamp(0.3, 1.0);
            data.push(px);
            data.push(py);
            data.push(p.color[0] * depth_fade);
            data.push(p.color[1] * depth_fade);
            data.push(p.color[2] * depth_fade);
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

fn particle_program() -> Arc<GpuPaintProgram> {
    static PROGRAM: std::sync::OnceLock<Arc<GpuPaintProgram>> = std::sync::OnceLock::new();
    PROGRAM
        .get_or_init(|| {
            GpuPaintProgram::new(
                include_str!("../shaders/particle.wgsl").into(),
                32,
                wgpu::VertexStepMode::Instance,
                Arc::from([
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
                ]),
            )
            .expect("particle shader obeys the declared source interface")
        })
        .clone()
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
    dirty: DirtyFlags,
    prepared_frame: Option<u64>,
    content_revision: u64,
    source_id: GpuPaintSourceId,
    source: Option<GpuPaintSource>,
}

impl ParticleCanvas {
    pub fn new(id: u64) -> Self {
        // Sizes seeded at 0.0; `measure` fills target_w/target_h from
        // parent's percent base (width:100%, height:100% layout).
        Self {
            id,
            parent_id: None,
            offset_x: 0.0,
            offset_y: 0.0,
            layout_x: 0.0,
            layout_y: 0.0,
            layout_w: 0.0,
            layout_h: 0.0,
            target_w: 0.0,
            target_h: 0.0,
            should_render: true,
            dirty: DirtyFlags::ALL,
            prepared_frame: None,
            content_revision: 0,
            source_id: GpuPaintSourceId::new(),
            source: None,
        }
    }
}

impl Layoutable for ParticleCanvas {
    fn requires_paint_resource_preparation(&self) -> bool {
        true
    }
    fn prepare_paint_resources(&mut self, context: PaintResourcePreparationContext) {
        if self.prepared_frame == Some(context.frame_number) {
            return;
        }
        self.prepared_frame = Some(context.frame_number);
        ViewportHandle.request_redraw();
        if !self.should_render {
            self.source = None;
            return;
        }
        let extent = [
            (self.layout_w * context.device_scale).ceil() as u32,
            (self.layout_h * context.device_scale).ceil() as u32,
        ];
        self.content_revision = self
            .content_revision
            .checked_add(1)
            .expect("source revision exhausted");
        self.source = PARTICLE_SYSTEM.with(|system| {
            let mut system = system.borrow_mut();
            system.update(context.now);
            let vertices = system.to_vertex_data(extent[0] as f32, extent[1] as f32);
            let uniforms = ParticleUniforms {
                screen_size: extent.map(|n| n as f32),
                canvas_pos: [0.0; 2],
                canvas_size: extent.map(|n| n as f32),
                time: system.elapsed,
                _pad: 0.0,
            };
            Some(
                GpuPaintSource::new(
                    self.source_id,
                    self.content_revision,
                    extent,
                    context.device_scale,
                    particle_program(),
                    Arc::from(bytemuck::bytes_of(&uniforms)),
                    Arc::from(bytemuck::cast_slice(&vertices)),
                    6,
                    system.particles.len() as u32,
                )
                .expect("finite particle preparation"),
            )
        });
        // Simulation changes pixels only. Layout dirty must be clearable so
        // the containing scrollport can certify its final geometry.
        self.dirty = self.dirty.union(DirtyFlags::PAINT);
    }

    fn measure(&mut self, constraints: LayoutConstraints, _arena: &mut rfgui::view::NodeArena) {
        // width:100%, height:100% — use percent base (parent content size).
        if let Some(w) = constraints.percent_base_width {
            self.target_w = w;
        }
        if let Some(h) = constraints.percent_base_height {
            self.target_h = h;
        }
        self.dirty = self.dirty.without(DirtyFlags::LAYOUT);
    }

    fn place(&mut self, placement: LayoutPlacement, _arena: &mut rfgui::view::NodeArena) {
        self.layout_x = placement.parent_x;
        self.layout_y = placement.parent_y;
        self.layout_w = self.target_w;
        self.layout_h = self.target_h;
        self.should_render = self.layout_w > 0.0 && self.layout_h > 0.0;
        self.dirty = self.dirty.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn measured_size(&self) -> (f32, f32) {
        (self.target_w, self.target_h)
    }

    fn set_layout_width(&mut self, w: f32) {
        if self.target_w != w {
            self.target_w = w;
            self.dirty = self.dirty.union(DirtyFlags::PLACE);
        }
    }
    fn set_layout_height(&mut self, h: f32) {
        if self.target_h != h {
            self.target_h = h;
            self.dirty = self.dirty.union(DirtyFlags::PLACE);
        }
    }

    fn flex_props(&self) -> rfgui::view::base_component::FlexProps {
        rfgui::view::base_component::FlexProps {
            grow: 1.0,
            allows_cross_stretch_when_row: true,
            allows_cross_stretch_when_col: true,
            ..Default::default()
        }
    }
    fn cross_alignment_size(
        &self,
        is_row: bool,
        _: Option<f32>,
        _arena: &rfgui::view::NodeArena,
    ) -> f32 {
        if is_row { self.target_h } else { self.target_w }
    }
    fn inline_relative_position(&self) -> (f32, f32) {
        (self.offset_x, self.offset_y)
    }
    fn set_layout_offset(&mut self, x: f32, y: f32) {
        if [self.offset_x, self.offset_y] != [x, y] {
            self.offset_x = x;
            self.offset_y = y;
            self.dirty = self.dirty.union(DirtyFlags::PLACE);
        }
    }
}

impl EventTarget for ParticleCanvas {
    fn dispatch_pointer_move(
        &mut self,
        event: &mut PointerMoveEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::rfgui::view::node_arena::NodeArena,
        _self_key: crate::rfgui::view::node_arena::NodeKey,
    ) {
        let w = self.layout_w;
        let h = self.layout_h;
        if w > 0.0 && h > 0.0 {
            let nx = (event.pointer.local_x / w).clamp(0.0, 1.0);
            let ny = (event.pointer.local_y / h).clamp(0.0, 1.0);
            let boost = event.pointer.buttons.left;
            PARTICLE_SYSTEM.with(|sys| {
                let mut s = sys.borrow_mut();
                s.set_attractor(Some((nx, ny)));
                s.set_mass_boost(boost);
            });
        }
    }

    fn dispatch_pointer_down(
        &mut self,
        event: &mut PointerDownEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::rfgui::view::node_arena::NodeArena,
        _self_key: crate::rfgui::view::node_arena::NodeKey,
    ) {
        if event.pointer.button == Some(PointerButton::Left) {
            PARTICLE_SYSTEM.with(|sys| sys.borrow_mut().set_mass_boost(true));
        }
    }

    fn dispatch_pointer_up(
        &mut self,
        event: &mut PointerUpEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::rfgui::view::node_arena::NodeArena,
        _self_key: crate::rfgui::view::node_arena::NodeKey,
    ) {
        if event.pointer.button == Some(PointerButton::Left) {
            PARTICLE_SYSTEM.with(|sys| sys.borrow_mut().set_mass_boost(false));
        }
    }

    fn set_hovered(&mut self, hovered: bool) -> bool {
        if !hovered {
            PARTICLE_SYSTEM.with(|sys| {
                let mut s = sys.borrow_mut();
                s.set_attractor(None);
                s.set_mass_boost(false);
            });
        }
        false
    }
}

impl Renderable for ParticleCanvas {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut rfgui::view::NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        if self.should_render {
            if let Some(source) = &self.source {
                source.paint(
                    graph,
                    &mut ctx,
                    [self.layout_x, self.layout_y, self.layout_w, self.layout_h],
                );
            }
        }
        ctx.into_state()
    }
}

impl ElementTrait for ParticleCanvas {
    fn stable_id(&self) -> u64 {
        self.id
    }
    fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }
    fn set_parent_id(&mut self, id: Option<u64>) {
        self.parent_id = id;
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id,
            parent_id: self.parent_id,
            x: self.layout_x,
            y: self.layout_y,
            width: self.layout_w,
            height: self.layout_h,
            border_radius: 0.0,
            should_render: self.should_render,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn prepared_gpu_paint_source(&self) -> Option<&GpuPaintSource> {
        self.source.as_ref()
    }
    fn retained_paint_signature(&self) -> u64 {
        self.source.as_ref().map_or(0, GpuPaintSource::revision)
    }
    fn retained_paint_signature_is_complete(&self) -> bool {
        self.source.is_some() || !self.should_render
    }
    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty
    }
    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty = self.dirty.without(flags);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HostBuilder impl + ParticleDemo component
// ═══════════════════════════════════════════════════════════════════════════════

impl HostBuilder for ParticleCanvas {
    fn build_descriptor(
        _node: &RsxElementNode,
        path: &[u64],
        _ctx: &BuildCtx,
    ) -> Result<ElementDescriptor, String> {
        // Size defaults to 0 → filled by parent constraints during layout.
        Ok(ElementDescriptor::leaf(Box::new(ParticleCanvas::new(
            stable_id("ParticleCanvas", path),
        ))))
    }
}

#[component]
pub fn ParticleDemo() -> RsxNode {
    let viewport = use_viewport();
    viewport.request_redraw();
    host_builder_node::<ParticleCanvas>("ParticleCanvas")
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod preparation_tests;
