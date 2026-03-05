use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::{BufferDesc, BufferResource, DepIn, DepOut};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{render_target_size, render_target_view};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use wgpu::util::DeviceExt;

const SHADOW_RESOURCES: u64 = 203;
const SHADOW_TEMP_POOL: u64 = 204;
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
        let w = width.max(0.0);
        let h = height.max(0.0);
        if w <= 0.0 || h <= 0.0 {
            return Self::default();
        }
        let r = radius.max(0.0).min(w * 0.5).min(h * 0.5);
        if r <= 0.001 {
            return Self {
                vertices: vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                indices: vec![0, 1, 2, 0, 2, 3],
            };
        }
        const ARC_SEGMENTS: usize = 6;
        let mut ring = Vec::with_capacity(ARC_SEGMENTS * 4 + 4);
        append_arc(
            &mut ring,
            [x + w - r, y + r],
            r,
            -std::f32::consts::FRAC_PI_2,
            0.0,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + w - r, y + h - r],
            r,
            0.0,
            std::f32::consts::FRAC_PI_2,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + r, y + h - r],
            r,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut ring,
            [x + r, y + r],
            r,
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
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let a = start + (end - start) * t;
        out.push([center[0] + radius * a.cos(), center[1] + radius * a.sin()]);
    }
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

pub struct ShadowPass {
    mesh: ShadowMesh,
    params: ShadowParams,
    blur_downsample_params_buffer: ShadowBlurDownsampleParamsBufferOut,
    blur_h_params_buffer: ShadowBlurHParamsBufferOut,
    blur_v_params_buffer: ShadowBlurVParamsBufferOut,
    input: ShadowInput,
    output: ShadowOutput,
}

#[derive(Clone, Copy)]
pub struct ShadowBlurDownsampleParamsBufferTag;
pub type ShadowBlurDownsampleParamsBufferOut =
    OutSlot<BufferResource, ShadowBlurDownsampleParamsBufferTag>;
#[derive(Clone, Copy)]
pub struct ShadowBlurHParamsBufferTag;
pub type ShadowBlurHParamsBufferOut = OutSlot<BufferResource, ShadowBlurHParamsBufferTag>;
#[derive(Clone, Copy)]
pub struct ShadowBlurVParamsBufferTag;
pub type ShadowBlurVParamsBufferOut = OutSlot<BufferResource, ShadowBlurVParamsBufferTag>;

#[derive(Default)]
pub struct ShadowInput {
    pub dep: DepIn,
}

#[derive(Default)]
pub struct ShadowOutput {
    pub render_target: RenderTargetOut,
    pub mask_render_target: RenderTargetOut,
    pub dep: DepOut,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct FillVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct QuadVertex {
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
    _pad: [f32; 2],
}

pub(crate) struct ShadowResources {
    fill_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    quad_vertex_buffer: wgpu::Buffer,
    quad_index_buffer: wgpu::Buffer,
    quad_index_count: u32,
    composite_format: wgpu::TextureFormat,
    composite_sample_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ShadowTempKey {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    slot: u8,
}

struct ShadowTempEntry {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

#[derive(Clone)]
struct ShadowSurface {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ShadowFinalKey {
    digest: u64,
}

struct ShadowFinalEntry {
    shadow: ShadowSurface,
    mask: Option<ShadowSurface>,
    last_used_epoch: u64,
}

struct ShadowFinalCache {
    entries: HashMap<ShadowFinalKey, ShadowFinalEntry>,
    epoch: u64,
}

pub(crate) struct ShadowTempPool {
    entries: HashMap<ShadowTempKey, ShadowTempEntry>,
}

impl ShadowPass {
    pub fn new(
        mesh: ShadowMesh,
        params: ShadowParams,
        input: ShadowInput,
        output: ShadowOutput,
    ) -> Self {
        Self {
            mesh,
            params,
            blur_downsample_params_buffer: ShadowBlurDownsampleParamsBufferOut::default(),
            blur_h_params_buffer: ShadowBlurHParamsBufferOut::default(),
            blur_v_params_buffer: ShadowBlurVParamsBufferOut::default(),
            input,
            output,
        }
    }
}

impl RenderPass for ShadowPass {
    type Input = ShadowInput;
    type Output = ShadowOutput;

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
        self.blur_downsample_params_buffer = builder.create_buffer(BufferDesc {
            size: std::mem::size_of::<BlurParamsUniform>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("ShadowPass Blur Downsample Params"),
        });
        self.blur_h_params_buffer = builder.create_buffer(BufferDesc {
            size: std::mem::size_of::<BlurParamsUniform>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("ShadowPass Blur H Params"),
        });
        self.blur_v_params_buffer = builder.create_buffer(BufferDesc {
            size: std::mem::size_of::<BlurParamsUniform>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("ShadowPass Blur V Params"),
        });
        if let Some(handle) = self.input.dep.handle() {
            let source: DepOut = OutSlot::with_handle(handle);
            builder.read_dep(&mut self.input.dep, &source);
        }
        if self.output.render_target.handle().is_some() {
            builder.write_texture(&mut self.output.render_target);
        }
        if self.output.mask_render_target.handle().is_some() {
            builder.write_texture(&mut self.output.mask_render_target);
        }
        if self.output.dep.handle().is_some() {
            builder.write_dep(&mut self.output.dep);
        }
    }

    fn compile_upload(&mut self, ctx: &mut PassContext<'_, '_>) {
        let target_handle = self.output.render_target.handle();
        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = match target_handle {
            Some(handle) => render_target_size(ctx, handle).unwrap_or(surface_size),
            None => surface_size,
        };
        if target_w == 0
            || target_h == 0
            || self.mesh.vertices.len() < 3
            || self.mesh.indices.len() < 3
        {
            return;
        }
        let scale = ctx.viewport.scale_factor().max(0.0001);
        let base_vertices = self
            .mesh
            .vertices
            .iter()
            .map(|[x, y]| [x * scale, y * scale])
            .collect::<Vec<_>>();
        let mut shadow_vertices = base_vertices.clone();
        apply_spread(&mut shadow_vertices, (self.params.spread * scale).max(0.0));
        let offset = [self.params.offset_x * scale, self.params.offset_y * scale];
        for v in &mut shadow_vertices {
            v[0] += offset[0];
            v[1] += offset[1];
        }
        let Some((min_x, min_y, max_x, max_y)) = bounds(&shadow_vertices) else {
            return;
        };
        let blur_padding = ((self.params.blur_radius.max(0.0) * scale) * 1.5).ceil() as i32;
        let bx = (min_x.floor() as i32 - blur_padding).max(0);
        let by = (min_y.floor() as i32 - blur_padding).max(0);
        let br = (max_x.ceil() as i32 + blur_padding).min(target_w as i32);
        let bb = (max_y.ceil() as i32 + blur_padding).min(target_h as i32);
        if br <= bx || bb <= by {
            return;
        }
        let bw = (br - bx) as u32;
        let bh = (bb - by) as u32;
        if bw == 0 || bh == 0 {
            return;
        }
        let blur_radius_px = (self.params.blur_radius.max(0.0) * scale).max(0.0);
        let downsample = if blur_radius_px >= 28.0 {
            4_u32
        } else if blur_radius_px >= 12.0 {
            2_u32
        } else {
            1_u32
        };
        let blur_w = if downsample > 1 {
            (bw / downsample).max(1)
        } else {
            bw.max(1)
        };
        let blur_h = if downsample > 1 {
            (bh / downsample).max(1)
        } else {
            bh.max(1)
        };
        let effective_radius = if downsample > 1 {
            blur_radius_px / downsample as f32
        } else {
            blur_radius_px
        };
        let blur_downsample_params = BlurParamsUniform {
            texel_size: [1.0 / blur_w as f32, 1.0 / blur_h as f32],
            direction: [1.0, 0.0],
            radius: 0.0,
            sigma: 0.001,
            _pad: [0.0, 0.0],
        };
        let sigma = (effective_radius * 0.5).max(0.001);
        let blur_h_params = BlurParamsUniform {
            texel_size: [1.0 / blur_w as f32, 1.0 / blur_h as f32],
            direction: [1.0, 0.0],
            radius: effective_radius.max(0.0),
            sigma,
            _pad: [0.0, 0.0],
        };
        let blur_v_params = BlurParamsUniform {
            texel_size: [1.0 / blur_w as f32, 1.0 / blur_h as f32],
            direction: [0.0, 1.0],
            radius: effective_radius.max(0.0),
            sigma,
            _pad: [0.0, 0.0],
        };
        if let Some(handle) = self.blur_downsample_params_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::bytes_of(&blur_downsample_params));
        }
        if let Some(handle) = self.blur_h_params_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::bytes_of(&blur_h_params));
        }
        if let Some(handle) = self.blur_v_params_buffer.handle() {
            let _ = ctx.upload_buffer(handle, 0, bytemuck::bytes_of(&blur_v_params));
        }
    }

    fn execute(
        &mut self,
        ctx: &mut PassContext<'_, '_>,
        _render_pass: Option<&mut wgpu::RenderPass<'_>>,
    ) {
        let target_handle = self.output.render_target.handle();
        let geometry_started_at = Instant::now();
        if self.mesh.vertices.len() < 3 || self.mesh.indices.len() < 3 {
            return;
        }

        let Some(target_handle) = target_handle else {
            return;
        };
        let Some(offscreen_view) = render_target_view(ctx, target_handle) else {
            return;
        };
        let surface_size = ctx.viewport.surface_size();
        let (target_w, target_h) = render_target_size(ctx, target_handle).unwrap_or(surface_size);
        if target_w == 0 || target_h == 0 {
            return;
        }

        let scale = ctx.viewport.scale_factor().max(0.0001);
        let base_vertices = self
            .mesh
            .vertices
            .iter()
            .map(|[x, y]| [x * scale, y * scale])
            .collect::<Vec<_>>();
        let mut shadow_vertices = base_vertices.clone();
        apply_spread(&mut shadow_vertices, (self.params.spread * scale).max(0.0));
        let offset = [self.params.offset_x * scale, self.params.offset_y * scale];
        for v in &mut shadow_vertices {
            v[0] += offset[0];
            v[1] += offset[1];
        }

        let Some((min_x, min_y, max_x, max_y)) = bounds(&shadow_vertices) else {
            return;
        };
        let blur_padding = ((self.params.blur_radius.max(0.0) * scale) * 1.5).ceil() as i32;
        let bx = (min_x.floor() as i32 - blur_padding).max(0);
        let by = (min_y.floor() as i32 - blur_padding).max(0);
        let br = (max_x.ceil() as i32 + blur_padding).min(target_w as i32);
        let bb = (max_y.ceil() as i32 + blur_padding).min(target_h as i32);
        if br <= bx || bb <= by {
            return;
        }
        let bw = (br - bx) as u32;
        let bh = (bb - by) as u32;
        if bw == 0 || bh == 0 {
            return;
        }
        ctx.record_detail_timing(
            "execute/shadow/geometry",
            geometry_started_at.elapsed().as_secs_f64() * 1000.0,
        );

        let resources_started_at = Instant::now();
        let device = match ctx.viewport.device() {
            Some(device) => device.clone(),
            None => return,
        };
        let composite_format = ctx.viewport.surface_format();
        let intermediate_format = SHADOW_INTERMEDIATE_FORMAT;
        let sample_count = ctx.viewport.msaa_sample_count();
        let (
            fill_pipeline,
            blur_pipeline,
            blur_bind_group_layout,
            sampler,
            quad_vertex_buffer,
            quad_index_buffer,
            quad_index_count,
        ) = {
            let cache = shadow_resources_cache();
            let mut cache = cache.lock().unwrap();
            let resources = cache.get_or_insert_with(SHADOW_RESOURCES, || {
                create_resources(&device, intermediate_format, composite_format, sample_count)
            });
            if resources.composite_format != composite_format
                || resources.composite_sample_count != sample_count
            {
                *resources =
                    create_resources(&device, intermediate_format, composite_format, sample_count);
            }
            (
                resources.fill_pipeline.clone(),
                resources.blur_pipeline.clone(),
                resources.blur_bind_group_layout.clone(),
                resources.sampler.clone(),
                resources.quad_vertex_buffer.clone(),
                resources.quad_index_buffer.clone(),
                resources.quad_index_count,
            )
        };
        let shadow_tex_a_view =
            acquire_temp_texture_view(ctx, &device, bw, bh, intermediate_format, 0);
        let shadow_tex_b_view =
            acquire_temp_texture_view(ctx, &device, bw, bh, intermediate_format, 1);
        let mask_tex_view = if self.params.clip_to_geometry {
            Some(acquire_temp_texture_view(
                ctx,
                &device,
                bw,
                bh,
                intermediate_format,
                2,
            ))
        } else {
            None
        };
        ctx.record_detail_timing(
            "execute/shadow/resources",
            resources_started_at.elapsed().as_secs_f64() * 1000.0,
        );
        let blur_downsample_params_buffer = self
            .blur_downsample_params_buffer
            .handle()
            .and_then(|h| ctx.acquire_buffer(h));
        let blur_h_params_buffer = self
            .blur_h_params_buffer
            .handle()
            .and_then(|h| ctx.acquire_buffer(h));
        let blur_v_params_buffer = self
            .blur_v_params_buffer
            .handle()
            .and_then(|h| ctx.acquire_buffer(h));

        let fill_color = {
            let a = (self.params.color[3] * self.params.opacity).clamp(0.0, 1.0);
            [
                self.params.color[0],
                self.params.color[1],
                self.params.color[2],
                a,
            ]
        };
        let cache_key = shadow_final_cache_key(
            &self.mesh,
            &self.params,
            target_w,
            target_h,
            scale,
            [bx as i32, by as i32, bw as i32, bh as i32],
        );
        let (shadow_output_surface, mask_output_surface) = {
            let cache = shadow_final_cache();
            let mut cache = cache.lock().unwrap();
            if let Some(hit) = cache.get(cache_key) {
                (hit.shadow.clone(), hit.mask.clone())
            } else {
                drop(cache);
                let fill_and_blur_started_at = Instant::now();
                draw_mesh_fill(
                    ctx,
                    &fill_pipeline,
                    &shadow_tex_a_view.view,
                    bx as f32,
                    by as f32,
                    bw as f32,
                    bh as f32,
                    &shadow_vertices,
                    &self.mesh.indices,
                    fill_color,
                    [0.0, 0.0, 0.0, 0.0],
                );

                if self.params.clip_to_geometry {
                    draw_mesh_fill(
                        ctx,
                        &fill_pipeline,
                        &mask_tex_view.as_ref().unwrap().view,
                        bx as f32,
                        by as f32,
                        bw as f32,
                        bh as f32,
                        &base_vertices,
                        &self.mesh.indices,
                        [1.0, 1.0, 1.0, 1.0],
                        [0.0, 0.0, 0.0, 0.0],
                    );
                }

                let blur_radius_px = (self.params.blur_radius.max(0.0) * scale).max(0.0);
                let mut shadow_output_surface = shadow_tex_a_view.clone();
                if blur_radius_px > 0.001 {
                    let downsample = if blur_radius_px >= 28.0 {
                        4_u32
                    } else if blur_radius_px >= 12.0 {
                        2_u32
                    } else {
                        1_u32
                    };
                    let ds_w = (bw / downsample).max(1);
                    let ds_h = (bh / downsample).max(1);
                    let effective_radius = blur_radius_px / downsample as f32;
                    let (src, tmp, out, blur_w, blur_h) = if downsample > 1 {
                        let ds_a = acquire_temp_texture_view(
                            ctx,
                            &device,
                            ds_w,
                            ds_h,
                            intermediate_format,
                            3,
                        );
                        let ds_b = acquire_temp_texture_view(
                            ctx,
                            &device,
                            ds_w,
                            ds_h,
                            intermediate_format,
                            4,
                        );
                        blur_texture(
                            ctx,
                            &blur_pipeline,
                            &blur_bind_group_layout,
                            &sampler,
                            &quad_vertex_buffer,
                            &quad_index_buffer,
                        quad_index_count,
                        &shadow_tex_a_view.view,
                        &ds_a.view,
                        BlurParamsUniform {
                            texel_size: [1.0 / ds_w.max(1) as f32, 1.0 / ds_h.max(1) as f32],
                            direction: [1.0, 0.0],
                                radius: 0.0,
                                sigma: 0.001,
                                _pad: [0.0, 0.0],
                            },
                            blur_downsample_params_buffer.as_ref(),
                        );
                        (ds_a.clone(), ds_b.clone(), ds_a, ds_w, ds_h)
                    } else {
                        (
                            shadow_tex_a_view.clone(),
                            shadow_tex_b_view.clone(),
                            shadow_tex_a_view.clone(),
                            bw,
                            bh,
                        )
                    };
                    blur_texture(
                        ctx,
                        &blur_pipeline,
                        &blur_bind_group_layout,
                        &sampler,
                        &quad_vertex_buffer,
                        &quad_index_buffer,
                        quad_index_count,
                        &src.view,
                        &tmp.view,
                        BlurParamsUniform {
                            texel_size: [1.0 / blur_w.max(1) as f32, 1.0 / blur_h.max(1) as f32],
                            direction: [1.0, 0.0],
                            radius: effective_radius.max(0.0),
                            sigma: (effective_radius * 0.5).max(0.001),
                            _pad: [0.0, 0.0],
                        },
                        blur_h_params_buffer.as_ref(),
                    );
                    blur_texture(
                        ctx,
                        &blur_pipeline,
                        &blur_bind_group_layout,
                        &sampler,
                        &quad_vertex_buffer,
                        &quad_index_buffer,
                        quad_index_count,
                        &tmp.view,
                        &out.view,
                        BlurParamsUniform {
                            texel_size: [1.0 / blur_w.max(1) as f32, 1.0 / blur_h.max(1) as f32],
                            direction: [0.0, 1.0],
                            radius: effective_radius.max(0.0),
                            sigma: (effective_radius * 0.5).max(0.001),
                            _pad: [0.0, 0.0],
                        },
                        blur_v_params_buffer.as_ref(),
                    );
                    shadow_output_surface = out;
                }
                ctx.record_detail_timing(
                    "execute/shadow/fill_and_blur",
                    fill_and_blur_started_at.elapsed().as_secs_f64() * 1000.0,
                );

                let cached_shadow = create_shadow_cache_surface(
                    ctx,
                    &device,
                    &shadow_output_surface,
                    intermediate_format,
                    10,
                );
                let cached_mask = if self.params.clip_to_geometry {
                    mask_tex_view.as_ref().map(|mask| {
                        create_shadow_cache_surface(ctx, &device, mask, intermediate_format, 11)
                    })
                } else {
                    None
                };
                let cache = shadow_final_cache();
                let mut cache = cache.lock().unwrap();
                cache.insert(cache_key, cached_shadow.clone(), cached_mask.clone());
                (cached_shadow, cached_mask)
            }
        };

        let composite_started_at = Instant::now();
        blur_texture(
            ctx,
            &blur_pipeline,
            &blur_bind_group_layout,
            &sampler,
            &quad_vertex_buffer,
            &quad_index_buffer,
            quad_index_count,
            &shadow_output_surface.view,
            &offscreen_view,
            BlurParamsUniform {
                texel_size: [
                    1.0 / shadow_output_surface.width.max(1) as f32,
                    1.0 / shadow_output_surface.height.max(1) as f32,
                ],
                direction: [1.0, 0.0],
                radius: 0.0,
                sigma: 0.001,
                _pad: [0.0, 0.0],
            },
            None,
        );
        if let (Some(mask_handle), Some(mask_surface)) =
            (self.output.mask_render_target.handle(), mask_output_surface.as_ref())
        {
            if let Some(mask_view) = render_target_view(ctx, mask_handle) {
                blur_texture(
                    ctx,
                    &blur_pipeline,
                    &blur_bind_group_layout,
                    &sampler,
                    &quad_vertex_buffer,
                    &quad_index_buffer,
                    quad_index_count,
                    &mask_surface.view,
                    &mask_view,
                    BlurParamsUniform {
                        texel_size: [
                            1.0 / mask_surface.width.max(1) as f32,
                            1.0 / mask_surface.height.max(1) as f32,
                        ],
                        direction: [1.0, 0.0],
                        radius: 0.0,
                        sigma: 0.001,
                        _pad: [0.0, 0.0],
                    },
                    None,
                );
            }
        }
        ctx.record_detail_timing(
            "execute/shadow/composite",
            composite_started_at.elapsed().as_secs_f64() * 1000.0,
        );
    }
}

impl RenderTargetPass for ShadowPass {
    fn apply_clip(&mut self, _scissor_rect: Option<[u32; 4]>) {}
}

fn create_resources(
    device: &wgpu::Device,
    intermediate_format: wgpu::TextureFormat,
    composite_format: wgpu::TextureFormat,
    composite_sample_count: u32,
) -> ShadowResources {
    let fill_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Fill Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/shadow_fill.wgsl").into()),
    });
    let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shadow Blur Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/shadow_blur.wgsl").into()),
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
                format: intermediate_format,
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

    let blur_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow Blur Bind Group Layout"),
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
        label: Some("Shadow Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let (quad_vertices, quad_indices) = fullscreen_quad();
    let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Quad Vertex"),
        contents: bytemuck::cast_slice(&quad_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let quad_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Quad Index"),
        contents: bytemuck::cast_slice(&quad_indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let blur_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shadow Blur Pipeline Layout"),
        bind_group_layouts: &[&blur_bind_group_layout],
        immediate_size: 0,
    });
    let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Shadow Blur Pipeline"),
        layout: Some(&blur_layout),
        vertex: wgpu::VertexState {
            module: &blur_shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadVertex>() as u64,
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
            module: &blur_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: intermediate_format,
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
    ShadowResources {
        fill_pipeline,
        blur_pipeline,
        blur_bind_group_layout,
        sampler,
        quad_vertex_buffer,
        quad_index_buffer,
        quad_index_count: quad_indices.len() as u32,
        composite_format,
        composite_sample_count,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_mesh_fill(
    ctx: &mut PassContext<'_, '_>,
    pipeline: &wgpu::RenderPipeline,
    view: &wgpu::TextureView,
    bx: f32,
    by: f32,
    bw: f32,
    bh: f32,
    vertices: &[[f32; 2]],
    indices: &[u32],
    color: [f32; 4],
    clear_color: [f32; 4],
) {
    if bw <= 0.0 || bh <= 0.0 {
        return;
    }
    let Some(device) = ctx.viewport.device().cloned() else {
        return;
    };
    let mut fill_vertices = Vec::with_capacity(vertices.len());
    for [x, y] in vertices {
        let local_x = ((x - bx) / bw).clamp(0.0, 1.0);
        let local_y = ((y - by) / bh).clamp(0.0, 1.0);
        fill_vertices.push(FillVertex {
            position: [local_x * 2.0 - 1.0, 1.0 - local_y * 2.0],
            color,
        });
    }
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Fill Vertex Buffer"),
        contents: bytemuck::cast_slice(&fill_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Shadow Fill Index Buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let Some(parts) = ctx.viewport.frame_parts() else {
        return;
    };
    let mut pass = parts
        .encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Fill"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear_color[0] as f64,
                        g: clear_color[1] as f64,
                        b: clear_color[2] as f64,
                        a: clear_color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
    pass.set_pipeline(pipeline);
    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
}

impl ShadowTempPool {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl ShadowFinalCache {
    const MAX_ENTRIES: usize = 256;
    const EVICT_UNUSED_AFTER_EPOCHS: u64 = 240;

    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            epoch: 0,
        }
    }

    fn begin_frame(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
        self.evict();
    }

    fn get(&mut self, key: ShadowFinalKey) -> Option<&ShadowFinalEntry> {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used_epoch = self.epoch;
        }
        self.entries.get(&key)
    }

    fn insert(&mut self, key: ShadowFinalKey, shadow: ShadowSurface, mask: Option<ShadowSurface>) {
        self.entries.insert(
            key,
            ShadowFinalEntry {
                shadow,
                mask,
                last_used_epoch: self.epoch,
            },
        );
        self.evict();
    }

    fn evict(&mut self) {
        let epoch = self.epoch;
        self.entries.retain(|_, entry| {
            epoch.saturating_sub(entry.last_used_epoch) <= Self::EVICT_UNUSED_AFTER_EPOCHS
        });
        if self.entries.len() <= Self::MAX_ENTRIES {
            return;
        }
        let mut keys = self
            .entries
            .iter()
            .map(|(k, v)| (*k, v.last_used_epoch))
            .collect::<Vec<_>>();
        keys.sort_by_key(|(_, last)| *last);
        let remove_count = self.entries.len().saturating_sub(Self::MAX_ENTRIES);
        for (key, _) in keys.into_iter().take(remove_count) {
            self.entries.remove(&key);
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.epoch = 0;
    }
}

fn acquire_temp_texture_view(
    _ctx: &mut PassContext<'_, '_>,
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    slot: u8,
) -> ShadowSurface {
    let cache = shadow_temp_pool_cache();
    let mut cache = cache.lock().unwrap();
    let pool = cache.get_or_insert_with(SHADOW_TEMP_POOL, ShadowTempPool::new);
    let key = ShadowTempKey {
        width: width.max(1),
        height: height.max(1),
        format,
        slot,
    };
    let entry = pool.entries.entry(key).or_insert_with(|| {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow Temp Texture"),
            size: wgpu::Extent3d {
                width: key.width,
                height: key.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        ShadowTempEntry { texture, view }
    });
    ShadowSurface {
        texture: entry.texture.clone(),
        view: entry.view.clone(),
        width: key.width,
        height: key.height,
    }
}

struct ShadowResourcesCache {
    entries: HashMap<u64, ShadowResources>,
}

impl ShadowResourcesCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn get_or_insert_with<F: FnOnce() -> ShadowResources>(
        &mut self,
        key: u64,
        create: F,
    ) -> &mut ShadowResources {
        self.entries.entry(key).or_insert_with(create)
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

struct ShadowTempPoolCache {
    entries: HashMap<u64, ShadowTempPool>,
}

impl ShadowTempPoolCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn get_or_insert_with<F: FnOnce() -> ShadowTempPool>(
        &mut self,
        key: u64,
        create: F,
    ) -> &mut ShadowTempPool {
        self.entries.entry(key).or_insert_with(create)
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

fn shadow_resources_cache() -> &'static Mutex<ShadowResourcesCache> {
    static CACHE: OnceLock<Mutex<ShadowResourcesCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ShadowResourcesCache::new()))
}

fn shadow_temp_pool_cache() -> &'static Mutex<ShadowTempPoolCache> {
    static CACHE: OnceLock<Mutex<ShadowTempPoolCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ShadowTempPoolCache::new()))
}

fn shadow_final_cache() -> &'static Mutex<ShadowFinalCache> {
    static CACHE: OnceLock<Mutex<ShadowFinalCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ShadowFinalCache::new()))
}

pub fn clear_shadow_resources_cache() {
    let resources = shadow_resources_cache();
    let mut resources = resources.lock().unwrap();
    resources.clear();
    let temp_pool = shadow_temp_pool_cache();
    let mut temp_pool = temp_pool.lock().unwrap();
    temp_pool.clear();
    let final_cache = shadow_final_cache();
    let mut final_cache = final_cache.lock().unwrap();
    final_cache.clear();
}

pub fn begin_shadow_resources_frame() {
    let cache = shadow_final_cache();
    let mut cache = cache.lock().unwrap();
    cache.begin_frame();
}

fn create_shadow_cache_surface(
    ctx: &mut PassContext<'_, '_>,
    device: &wgpu::Device,
    source: &ShadowSurface,
    format: wgpu::TextureFormat,
    slot: u8,
) -> ShadowSurface {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(match slot {
            10 => "Shadow Final Cache Texture",
            11 => "Shadow Final Mask Cache Texture",
            _ => "Shadow Final Cache Texture",
        }),
        size: wgpu::Extent3d {
            width: source.width.max(1),
            height: source.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let Some(parts) = ctx.viewport.frame_parts() else {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        return ShadowSurface {
            texture,
            view,
            width: source.width,
            height: source.height,
        };
    };
    parts.encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &source.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width: source.width.max(1),
            height: source.height.max(1),
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    ShadowSurface {
        texture,
        view,
        width: source.width,
        height: source.height,
    }
}

fn shadow_final_cache_key(
    mesh: &ShadowMesh,
    params: &ShadowParams,
    target_w: u32,
    target_h: u32,
    scale: f32,
    shadow_bounds: [i32; 4],
) -> ShadowFinalKey {
    let mut hasher = DefaultHasher::new();
    target_w.hash(&mut hasher);
    target_h.hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    shadow_bounds.hash(&mut hasher);
    for [x, y] in &mesh.vertices {
        x.to_bits().hash(&mut hasher);
        y.to_bits().hash(&mut hasher);
    }
    mesh.indices.hash(&mut hasher);
    params.offset_x.to_bits().hash(&mut hasher);
    params.offset_y.to_bits().hash(&mut hasher);
    params.blur_radius.to_bits().hash(&mut hasher);
    params.opacity.to_bits().hash(&mut hasher);
    params.spread.to_bits().hash(&mut hasher);
    params.clip_to_geometry.hash(&mut hasher);
    for c in params.color {
        c.to_bits().hash(&mut hasher);
    }
    ShadowFinalKey {
        digest: hasher.finish(),
    }
}

fn blur_texture(
    ctx: &mut PassContext<'_, '_>,
    blur_pipeline: &wgpu::RenderPipeline,
    blur_bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    quad_vertex_buffer: &wgpu::Buffer,
    quad_index_buffer: &wgpu::Buffer,
    quad_index_count: u32,
    input_view: &wgpu::TextureView,
    output_view: &wgpu::TextureView,
    params: BlurParamsUniform,
    params_buffer: Option<&wgpu::Buffer>,
) {
    let Some(device) = ctx.viewport.device().cloned() else {
        return;
    };
    let fallback_params_buffer;
    let params_binding = if let Some(buffer) = params_buffer {
        buffer.as_entire_binding()
    } else {
        fallback_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shadow Blur Params (Fallback)"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        eprintln!(
            "[warn] Shadow fallback: using temporary blur params buffer (framegraph buffer unavailable)"
        );
        fallback_params_buffer.as_entire_binding()
    };
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Shadow Blur Bind Group"),
        layout: blur_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(input_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params_binding,
            },
        ],
    });
    let Some(parts) = ctx.viewport.frame_parts() else {
        return;
    };
    let mut pass = parts
        .encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Blur"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
    pass.set_pipeline(blur_pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.set_vertex_buffer(0, quad_vertex_buffer.slice(..));
    pass.set_index_buffer(quad_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    pass.draw_indexed(0..quad_index_count, 0, 0..1);
}


fn fullscreen_quad() -> ([QuadVertex; 4], [u16; 6]) {
    (
        [
            QuadVertex {
                position: [-1.0, -1.0],
                uv: [0.0, 1.0],
            },
            QuadVertex {
                position: [1.0, -1.0],
                uv: [1.0, 1.0],
            },
            QuadVertex {
                position: [1.0, 1.0],
                uv: [1.0, 0.0],
            },
            QuadVertex {
                position: [-1.0, 1.0],
                uv: [0.0, 0.0],
            },
        ],
        [0, 1, 2, 0, 2, 3],
    )
}

fn bounds(vertices: &[[f32; 2]]) -> Option<(f32, f32, f32, f32)> {
    if vertices.is_empty() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for [x, y] in vertices {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return None;
    }
    Some((min_x, min_y, max_x, max_y))
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
