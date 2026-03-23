use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::frame_graph::{
    GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy, PrepareContext,
};
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, logical_scissor_to_target_physical,
    render_target_origin, render_target_sample_count, resolve_texture_ref,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use std::collections::HashSet;
use std::num::NonZeroU64;
use std::sync::{Mutex, OnceLock};
use wgpu::util::DeviceExt;

#[derive(Default, Clone, Copy, Debug)]
pub struct RectPassParams {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub fill_color: [f32; 4],
    pub opacity: f32,
    pub border_widths: [f32; 4],
    pub border_radii: [[f32; 2]; 4],
    pub color_write_enabled: bool,
    pub border_color: [f32; 4],
    pub border_side_colors: [[f32; 4]; 4],
    pub use_border_side_colors: bool,
    pub depth: f32,
}

impl RectPassParams {
    pub fn set_border_color(&mut self, color: [f32; 4]) {
        self.border_color = color;
        self.border_side_colors = [color; 4];
        self.use_border_side_colors = false;
    }

    pub fn set_border_side_colors(
        &mut self,
        left: [f32; 4],
        right: [f32; 4],
        top: [f32; 4],
        bottom: [f32; 4],
    ) {
        self.border_side_colors = [left, right, top, bottom];
        self.use_border_side_colors = true;
    }

    pub fn set_border_width(&mut self, width: f32) {
        self.border_widths = [width.max(0.0); 4];
    }

    pub fn set_border_widths(&mut self, left: f32, right: f32, top: f32, bottom: f32) {
        self.border_widths = [left.max(0.0), right.max(0.0), top.max(0.0), bottom.max(0.0)];
    }

    pub fn set_border_radius(&mut self, radius: f32) {
        let r = radius.max(0.0);
        self.border_radii = [[r, r]; 4];
    }

    pub fn set_border_radii(&mut self, radii: [f32; 4]) {
        self.border_radii = radii.map(|v| {
            let r = v.max(0.0);
            [r, r]
        });
    }

    pub fn set_border_radii_xy(&mut self, radii: [[f32; 2]; 4]) {
        self.border_radii = radii.map(|v| [v[0].max(0.0), v[1].max(0.0)]);
    }
}

pub struct DrawRectPass {
    params: RectPassParams,
    scissor_rect: Option<[u32; 4]>,
    stencil_mode: RectStencilMode,
    color_write_enabled: bool,
    clear_target: bool,
    render_mode: RectRenderMode,
    prepared_bind_group: Option<wgpu::BindGroup>,
    prepared_dynamic_offset: u32,
    input: DrawRectInput,
    output: DrawRectOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RectStencilMode {
    Disabled,
    Test { clip_id: u8 },
    Increment { clip_id: u8 },
    Decrement { clip_id: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RectRenderMode {
    Combined,
    FillOnly,
    BorderOnly,
}

#[derive(Default)]
pub struct DrawRectInput {
    pub render_target: RenderTargetIn,
    pub pass_context: RenderPassContext,
}

#[derive(Default)]
pub struct DrawRectOutput {
    pub render_target: RenderTargetOut,
}

pub struct OpaqueRectPass {
    inner: DrawRectPass,
    depth_order: u32,
}

const OPAQUE_RECT_DEPTH_BUCKETS: u32 = 1 << 20;

impl DrawRectPass {
    fn trace_name(&self) -> &'static str {
        match self.stencil_mode {
            RectStencilMode::Increment { .. } => "DrawRectPass::StencilIncrement",
            RectStencilMode::Decrement { .. } => "DrawRectPass::StencilDecrement",
            RectStencilMode::Test { .. } => match self.render_mode {
                RectRenderMode::Combined => "DrawRectPass::StencilTestCombined",
                RectRenderMode::FillOnly => "DrawRectPass::StencilTestFill",
                RectRenderMode::BorderOnly => "DrawRectPass::StencilTestBorder",
            },
            RectStencilMode::Disabled => match self.render_mode {
                RectRenderMode::Combined => "DrawRectPass::Combined",
                RectRenderMode::FillOnly => "DrawRectPass::FillOnly",
                RectRenderMode::BorderOnly => "DrawRectPass::BorderOnly",
            },
        }
    }

    pub(crate) fn draw_rect_input_mut(&mut self) -> &mut DrawRectInput {
        &mut self.input
    }

    pub(crate) fn draw_rect_output_mut(&mut self) -> &mut DrawRectOutput {
        &mut self.output
    }

    fn inherit_stencil_clip_if_needed(&mut self) {
        if matches!(self.stencil_mode, RectStencilMode::Disabled)
            && let Some(clip_id) = self.input.pass_context.stencil_clip_id
        {
            self.set_stencil_test(clip_id);
        }
    }

    pub fn new(params: RectPassParams, input: DrawRectInput, output: DrawRectOutput) -> Self {
        Self {
            params,
            scissor_rect: None,
            stencil_mode: RectStencilMode::Disabled,
            color_write_enabled: true,
            clear_target: false,
            render_mode: RectRenderMode::Combined,
            prepared_bind_group: None,
            prepared_dynamic_offset: 0,
            input,
            output,
        }
    }

    pub fn set_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.scissor_rect = scissor_rect;
    }

    pub fn set_stencil_test(&mut self, clip_id: u8) {
        self.stencil_mode = RectStencilMode::Test { clip_id };
    }

    pub fn set_stencil_increment(&mut self, clip_id: u8) {
        self.stencil_mode = RectStencilMode::Increment { clip_id };
    }

    pub fn set_stencil_decrement(&mut self, clip_id: u8) {
        self.stencil_mode = RectStencilMode::Decrement { clip_id };
    }

    pub fn set_clear_target(&mut self, clear_target: bool) {
        self.clear_target = clear_target;
    }

    pub fn set_color_write_enabled(&mut self, enabled: bool) {
        self.color_write_enabled = enabled;
    }

    pub fn set_depth_stencil_target(&mut self, enabled: bool) {
        self.input.pass_context.uses_depth_stencil = enabled;
    }

    pub fn set_render_mode(&mut self, mode: RectRenderMode) {
        self.render_mode = mode;
    }

    pub fn set_fill_only(&mut self) {
        self.render_mode = RectRenderMode::FillOnly;
    }

    pub fn set_border_only(&mut self) {
        self.render_mode = RectRenderMode::BorderOnly;
    }

    pub fn set_input(&mut self, input: RenderTargetIn) {
        self.input.render_target = input;
    }

    pub fn set_output(&mut self, output: RenderTargetOut) {
        self.output.render_target = output;
    }

    pub fn set_border_width(&mut self, width: f32) {
        self.params.set_border_width(width);
    }

    pub fn set_border_widths(&mut self, left: f32, right: f32, top: f32, bottom: f32) {
        self.params.set_border_widths(left, right, top, bottom);
    }

    pub fn set_border_radius(&mut self, radius: f32) {
        self.params.set_border_radius(radius);
    }

    pub fn set_border_radii(&mut self, radii: [f32; 4]) {
        self.params.set_border_radii(radii);
    }

    pub fn set_border_side_colors(
        &mut self,
        left: [f32; 4],
        right: [f32; 4],
        top: [f32; 4],
        bottom: [f32; 4],
    ) {
        self.params.set_border_side_colors(left, right, top, bottom);
    }

    pub fn is_opaque_candidate(&self) -> bool {
        const OPAQUE_THRESHOLD: f32 = 0.999;
        if !self.color_write_enabled {
            return false;
        }
        if !matches!(
            self.stencil_mode,
            RectStencilMode::Disabled | RectStencilMode::Test { .. }
        ) {
            return false;
        }
        let opacity = self.params.opacity.clamp(0.0, 1.0);
        if opacity < OPAQUE_THRESHOLD {
            return false;
        }
        if !matches!(self.render_mode, RectRenderMode::BorderOnly)
            && self.params.fill_color[3].clamp(0.0, 1.0) < OPAQUE_THRESHOLD
        {
            return false;
        }
        let side_colors = if self.params.use_border_side_colors {
            self.params.border_side_colors
        } else {
            [self.params.border_color; 4]
        };
        let side_widths = self.params.border_widths;
        if !matches!(self.render_mode, RectRenderMode::FillOnly) {
            for i in 0..4 {
                if side_widths[i] <= 0.0 {
                    continue;
                }
                if side_colors[i][3].clamp(0.0, 1.0) < OPAQUE_THRESHOLD {
                    return false;
                }
            }
        }
        true
    }

    pub fn into_opaque(self) -> OpaqueRectPass {
        OpaqueRectPass::from_draw_rect_pass(self)
    }

    pub fn snapshot_draw(&self) -> DrawRectDraw {
        DrawRectDraw {
            position: self.params.position,
            size: self.params.size,
            fill_color: self.params.fill_color,
            border_color: self.params.border_color,
            border_side_colors: self.params.border_side_colors,
            use_border_side_colors: self.params.use_border_side_colors,
            border_widths: self.params.border_widths,
            border_radii: self.params.border_radii,
            opacity: self.params.opacity,
            depth: self.params.depth,
            scissor_rect: intersect_scissor_rects(
                self.input.pass_context.scissor_rect,
                self.scissor_rect,
            ),
            stencil_mode: self.stencil_mode,
            color_write_enabled: self.color_write_enabled,
            color_target: self.output.render_target.handle(),
            render_mode: self.render_mode,
        }
    }

    fn compile_upload_uniform(
        &mut self,
        ctx: &mut PrepareContext<'_, '_>,
        variant: RectShaderVariant,
    ) {
        let surface_size = ctx.viewport.surface_size();
        let target_meta =
            resolve_texture_ref(self.output.render_target.handle(), ctx, surface_size, None);
        let (target_w, target_h) = target_meta.physical_size;
        let target_origin = self
            .output
            .render_target
            .handle()
            .and_then(|target| render_target_origin(ctx, target))
            .unwrap_or((0, 0));
        let scale = ctx.viewport.scale_factor();
        let scaled_position = [
            self.params.position[0] * scale - target_origin.0 as f32
                + target_meta.logical_origin.0 as f32,
            self.params.position[1] * scale - target_origin.1 as f32
                + target_meta.logical_origin.1 as f32,
        ];
        let scaled_size = [self.params.size[0] * scale, self.params.size[1] * scale];
        let scaled_border_widths = self.params.border_widths.map(|v| v * scale);
        let scaled_border_radii = self
            .params
            .border_radii
            .map(|r| [r[0].max(0.0) * scale, r[1].max(0.0) * scale]);
        let border_side_colors = if self.params.use_border_side_colors {
            self.params.border_side_colors
        } else {
            [self.params.border_color; 4]
        };
        let params = build_rect_params(
            scaled_position,
            scaled_size,
            scaled_border_widths,
            scaled_border_radii,
            self.params.fill_color,
            border_side_colors,
            self.params.opacity,
            self.params.depth,
            target_w as f32,
            target_h as f32,
        );
        if ctx.viewport.debug_geometry_overlay() {
            let (overlay_w, overlay_h) = ctx.viewport.surface_size();
            let (debug_vertices, debug_indices) = build_rect_debug_overlay_geometry(
                params,
                [
                    target_origin.0 as f32 - target_meta.logical_origin.0 as f32,
                    target_origin.1 as f32 - target_meta.logical_origin.1 as f32,
                ],
                overlay_w as f32,
                overlay_h as f32,
                [0.95, 0.2, 0.95, 0.95],
                [1.0, 0.9, 0.25, 0.95],
            );
            if !debug_vertices.is_empty() && !debug_indices.is_empty() {
                let overlay_vertices: Vec<
                    crate::view::render_pass::debug_overlay_pass::DebugOverlayVertex,
                > = debug_vertices
                    .into_iter()
                    .map(|vertex| {
                        crate::view::render_pass::debug_overlay_pass::DebugOverlayVertex {
                            position: vertex.position,
                            color: vertex.color,
                        }
                    })
                    .collect();
                ctx.viewport
                    .push_debug_overlay_geometry(&overlay_vertices, &debug_indices);
            }
        }
        let Some((buffer, dynamic_offset)) = ctx.viewport.upload_draw_rect_uniform(
            bytemuck::bytes_of(&params),
            RECT_UNIFORM_SLOT_SIZE,
            RECT_UNIFORM_SLOT_SIZE * RECT_UNIFORM_SLOT_COUNT as u64,
        ) else {
            self.prepared_bind_group = None;
            self.prepared_dynamic_offset = 0;
            return;
        };

        let Some(device) = ctx.viewport.device().cloned() else {
            self.prepared_bind_group = None;
            self.prepared_dynamic_offset = 0;
            return;
        };
        let format = ctx.viewport.surface_format();
        let sample_count = self
            .output
            .render_target
            .handle()
            .and_then(|handle| render_target_sample_count(ctx, handle))
            .unwrap_or_else(|| ctx.viewport.msaa_sample_count());
        let (stencil_class, _) = stencil_class_and_reference(self.stencil_mode);
        let cache_key = rect_resource_cache_key(
            variant,
            stencil_class,
            self.color_write_enabled,
            self.render_mode,
        );
        let cache = draw_rect_resources_cache();
        let mut cache = cache.lock().unwrap();
        let resources = cache.get_or_insert_with(cache_key, || {
            create_draw_rect_resources(
                &device,
                format,
                sample_count,
                variant,
                stencil_class,
                self.color_write_enabled,
                self.render_mode,
            )
        });
        if resources.pipeline_format != format
            || resources.pipeline_sample_count != sample_count
            || resources.variant != variant
            || resources.stencil_class != stencil_class
            || resources.color_write_enabled != self.color_write_enabled
            || resources.render_mode != self.render_mode
        {
            *resources = create_draw_rect_resources(
                &device,
                format,
                sample_count,
                variant,
                stencil_class,
                self.color_write_enabled,
                self.render_mode,
            );
        }
        self.prepared_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DrawRect Bind Group (Prepared)"),
            layout: &resources.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: Some(NonZeroU64::new(RECT_UNIFORM_SLOT_SIZE).unwrap()),
                }),
            }],
        }));
        self.prepared_dynamic_offset = dynamic_offset;
    }
}

impl OpaqueRectPass {
    pub(crate) fn draw_rect_input_mut(&mut self) -> &mut DrawRectInput {
        &mut self.inner.input
    }

    pub(crate) fn draw_rect_output_mut(&mut self) -> &mut DrawRectOutput {
        &mut self.inner.output
    }

    pub fn from_draw_rect_pass(pass: DrawRectPass) -> Self {
        let mut opaque = Self {
            inner: pass,
            depth_order: 0,
        };
        opaque.apply_depth_order();
        opaque
    }

    pub fn set_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.inner.set_scissor_rect(scissor_rect);
    }

    pub fn set_input(&mut self, input: RenderTargetIn) {
        self.inner.set_input(input);
    }

    pub fn set_output(&mut self, output: RenderTargetOut) {
        self.inner.set_output(output);
    }

    pub fn set_depth_order(&mut self, depth_order: u32) {
        self.depth_order = depth_order;
        self.apply_depth_order();
    }

    fn apply_depth_order(&mut self) {
        let clamped_order = self
            .depth_order
            .min(OPAQUE_RECT_DEPTH_BUCKETS.saturating_sub(1));
        let t = (clamped_order as f32 + 0.5) / OPAQUE_RECT_DEPTH_BUCKETS as f32;
        self.inner.params.depth = (1.0 - t).clamp(0.0, 1.0);
    }
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

const RECT_RESOURCES_BASE: u64 = 10;
pub(crate) const RECT_UNIFORM_SLOT_SIZE: u64 = 256;
const RECT_UNIFORM_SLOT_COUNT: u32 = 4096;

#[derive(Clone, Copy)]
pub struct RenderTargetTag;
pub type RenderTargetIn = InSlot<TextureResource, RenderTargetTag>;
pub type RenderTargetOut = OutSlot<TextureResource, RenderTargetTag>;
pub type AlphaRectPass = DrawRectPass;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RectShaderVariant {
    Alpha,
    Opaque,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum RectStencilClass {
    None,
    Test,
    Increment,
    Decrement,
}

#[derive(Clone, Copy)]
pub struct DrawRectDraw {
    position: [f32; 2],
    size: [f32; 2],
    fill_color: [f32; 4],
    border_color: [f32; 4],
    border_side_colors: [[f32; 4]; 4],
    use_border_side_colors: bool,
    border_widths: [f32; 4],
    border_radii: [[f32; 2]; 4],
    opacity: f32,
    depth: f32,
    scissor_rect: Option<[u32; 4]>,
    stencil_mode: RectStencilMode,
    color_write_enabled: bool,
    color_target: Option<TextureHandle>,
    render_mode: RectRenderMode,
}

impl GraphicsPass for DrawRectPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        self.inherit_stencil_clip_if_needed();
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            let _ = target;
            builder.write_color(
                &self.output.render_target,
                if self.clear_target {
                    GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0])
                } else {
                    GraphicsColorAttachmentOps::load()
                },
            );
        } else {
            builder.write_surface_color(if self.clear_target {
                GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0])
            } else {
                GraphicsColorAttachmentOps::load()
            });
        }
        if self.input.pass_context.uses_depth_stencil {
            if self.clear_target {
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
        self.compile_upload_uniform(ctx, RectShaderVariant::Alpha);
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        encode_draw_rect_into_existing_pass(self, ctx, RectShaderVariant::Alpha);
    }

    fn name(&self) -> &'static str {
        self.trace_name()
    }
}

impl GraphicsPass for OpaqueRectPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        self.inner.inherit_stencil_clip_if_needed();
        if let Some(target) = builder.texture_target(&self.inner.output.render_target) {
            let _ = target;
            builder.write_color(
                &self.inner.output.render_target,
                if self.inner.clear_target {
                    GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0])
                } else {
                    GraphicsColorAttachmentOps::load()
                },
            );
        } else {
            builder.write_surface_color(if self.inner.clear_target {
                GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0])
            } else {
                GraphicsColorAttachmentOps::load()
            });
        }
        if self.inner.input.pass_context.uses_depth_stencil {
            if self.inner.clear_target {
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
        self.inner
            .compile_upload_uniform(ctx, RectShaderVariant::Opaque);
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        encode_draw_rect_into_existing_pass(&mut self.inner, ctx, RectShaderVariant::Opaque);
    }

    fn name(&self) -> &'static str {
        match self.inner.stencil_mode {
            RectStencilMode::Increment { .. } => "OpaqueRectPass::StencilIncrement",
            RectStencilMode::Decrement { .. } => "OpaqueRectPass::StencilDecrement",
            RectStencilMode::Test { .. } => match self.inner.render_mode {
                RectRenderMode::Combined => "OpaqueRectPass::StencilTestCombined",
                RectRenderMode::FillOnly => "OpaqueRectPass::StencilTestFill",
                RectRenderMode::BorderOnly => "OpaqueRectPass::StencilTestBorder",
            },
            RectStencilMode::Disabled => match self.inner.render_mode {
                RectRenderMode::Combined => "OpaqueRectPass::Combined",
                RectRenderMode::FillOnly => "OpaqueRectPass::FillOnly",
                RectRenderMode::BorderOnly => "OpaqueRectPass::BorderOnly",
            },
        }
    }
}

fn rect_resource_cache_key(
    variant: RectShaderVariant,
    stencil_class: RectStencilClass,
    color_write_enabled: bool,
    render_mode: RectRenderMode,
) -> u64 {
    let variant_id = match variant {
        RectShaderVariant::Alpha => 0_u64,
        RectShaderVariant::Opaque => 1_u64,
    };
    let stencil_id = match stencil_class {
        RectStencilClass::None => 0_u64,
        RectStencilClass::Test => 1_u64,
        RectStencilClass::Increment => 2_u64,
        RectStencilClass::Decrement => 3_u64,
    };
    let color_id = if color_write_enabled { 1_u64 } else { 0_u64 };
    let mode_id = match render_mode {
        RectRenderMode::Combined => 0_u64,
        RectRenderMode::FillOnly => 1_u64,
        RectRenderMode::BorderOnly => 2_u64,
    };
    RECT_RESOURCES_BASE + variant_id * 1000 + stencil_id * 100 + color_id * 10 + mode_id
}

fn stencil_class_and_reference(stencil_mode: RectStencilMode) -> (RectStencilClass, Option<u8>) {
    match stencil_mode {
        RectStencilMode::Disabled => (RectStencilClass::None, None),
        RectStencilMode::Test { clip_id } => (RectStencilClass::Test, Some(clip_id)),
        RectStencilMode::Increment { clip_id } => (RectStencilClass::Increment, Some(clip_id)),
        RectStencilMode::Decrement { clip_id } => (RectStencilClass::Decrement, Some(clip_id)),
    }
}

fn encode_draw_rect_into_existing_pass(
    pass_def: &mut DrawRectPass,
    ctx: &mut GraphicsCtx<'_, '_, '_, '_>,
    variant: RectShaderVariant,
) {
    let draw = pass_def.snapshot_draw();
    let surface_size = ctx.viewport().surface_size();
    let target_meta =
        resolve_texture_ref(draw.color_target, ctx.frame_resources(), surface_size, None);
    let (target_w, target_h) = target_meta.physical_size;
    let target_origin = draw
        .color_target
        .and_then(|handle| render_target_origin(ctx.frame_resources(), handle))
        .unwrap_or((0, 0));
    let scale = ctx.viewport().scale_factor();
    let device = match ctx.viewport().device() {
        Some(device) => device.clone(),
        None => return,
    };
    let format = ctx.viewport().surface_format();
    let sample_count = draw
        .color_target
        .and_then(|handle| render_target_sample_count(ctx.frame_resources(), handle))
        .unwrap_or_else(|| ctx.viewport().msaa_sample_count());
    let scaled_position = [
        draw.position[0] * scale - target_origin.0 as f32 + target_meta.logical_origin.0 as f32,
        draw.position[1] * scale - target_origin.1 as f32 + target_meta.logical_origin.1 as f32,
    ];
    let scaled_size = [draw.size[0] * scale, draw.size[1] * scale];
    let scaled_border_widths = draw.border_widths.map(|v| v * scale);
    let scaled_border_radii = draw
        .border_radii
        .map(|r| [r[0].max(0.0) * scale, r[1].max(0.0) * scale]);
    let border_side_colors = if draw.use_border_side_colors {
        draw.border_side_colors
    } else {
        [draw.border_color; 4]
    };
    let params = build_rect_params(
        scaled_position,
        scaled_size,
        scaled_border_widths,
        scaled_border_radii,
        draw.fill_color,
        border_side_colors,
        draw.opacity,
        draw.depth,
        target_w as f32,
        target_h as f32,
    );
    if params.outer_rect[2] <= params.outer_rect[0] || params.outer_rect[3] <= params.outer_rect[1]
    {
        return;
    }
    let (stencil_class, stencil_reference) = stencil_class_and_reference(draw.stencil_mode);
    let cache_key = rect_resource_cache_key(
        variant,
        stencil_class,
        draw.color_write_enabled,
        draw.render_mode,
    );
    let (pipeline, bind_group_layout, vertex_buffer, index_buffer, index_count) = {
        let cache = draw_rect_resources_cache();
        let mut cache = cache.lock().unwrap();
        let resources = cache.get_or_insert_with(cache_key, || {
            create_draw_rect_resources(
                &device,
                format,
                sample_count,
                variant,
                stencil_class,
                draw.color_write_enabled,
                draw.render_mode,
            )
        });
        if resources.pipeline_format != format
            || resources.pipeline_sample_count != sample_count
            || resources.variant != variant
            || resources.stencil_class != stencil_class
            || resources.color_write_enabled != draw.color_write_enabled
            || resources.render_mode != draw.render_mode
        {
            *resources = create_draw_rect_resources(
                &device,
                format,
                sample_count,
                variant,
                stencil_class,
                draw.color_write_enabled,
                draw.render_mode,
            );
        }
        (
            resources.pipeline.clone(),
            resources.bind_group_layout.clone(),
            resources.vertex_buffer.clone(),
            resources.index_buffer.clone(),
            resources.index_count,
        )
    };
    let bind_group = if let Some(bind_group) = pass_def.prepared_bind_group.clone() {
        bind_group
    } else {
        let fallback_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("DrawRect Params Buffer Fallback"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DrawRect Bind Group Fallback"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: fallback_uniform_buffer.as_entire_binding(),
            }],
        })
    };
    let scissor_rect_physical = draw.scissor_rect.and_then(|scissor_rect| {
        logical_scissor_to_target_physical(
            ctx.viewport(),
            scissor_rect,
            target_origin,
            (target_w, target_h),
        )
    });
    ctx.set_pipeline(&pipeline);
    ctx.set_vertex_buffer(0, vertex_buffer.slice(..));
    ctx.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    let dynamic_offset = if pass_def.prepared_bind_group.is_some() {
        pass_def.prepared_dynamic_offset
    } else {
        0
    };
    ctx.set_bind_group(0, &bind_group, &[dynamic_offset]);
    if let Some(stencil_reference) = stencil_reference {
        ctx.set_stencil_reference(stencil_reference as u32);
    }
    if let Some([x, y, width, height]) = scissor_rect_physical {
        ctx.set_scissor_rect(x, y, width, height);
    } else {
        ctx.set_scissor_rect(0, 0, target_w, target_h);
    }
    ctx.draw_indexed(0..index_count, 0, 0..1);
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct QuadVertex {
    uv: [f32; 2],
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct DebugVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct RectParams {
    // [min_x, min_y, max_x, max_y] in physical pixels
    outer_rect: [f32; 4],
    inner_rect: [f32; 4],
    // corner order: TL, TR, BR, BL
    outer_rx: [f32; 4],
    outer_ry: [f32; 4],
    inner_rx: [f32; 4],
    inner_ry: [f32; 4],
    // [left, top, right, bottom]
    border_widths: [f32; 4],
    // flags.x: has_inner (0/1), flags.yzw reserved
    flags: [f32; 4],
    // linear-space, straight alpha (premultiply in shader)
    fill_color: [f32; 4],
    border_left: [f32; 4],
    border_top: [f32; 4],
    border_right: [f32; 4],
    border_bottom: [f32; 4],
    // [w, h, inv_w, inv_h]
    screen_size: [f32; 4],
    depth: f32,
    // Keep host-side uniform size at 256 bytes to match WGSL uniform layout.
    _pad2: [f32; 7],
}

pub(crate) struct DrawRectResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    pipeline_format: wgpu::TextureFormat,
    pipeline_sample_count: u32,
    variant: RectShaderVariant,
    stencil_class: RectStencilClass,
    color_write_enabled: bool,
    render_mode: RectRenderMode,
}

fn create_draw_rect_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
    variant: RectShaderVariant,
    stencil_class: RectStencilClass,
    color_write_enabled: bool,
    render_mode: RectRenderMode,
) -> DrawRectResources {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(match variant {
            RectShaderVariant::Alpha => "DrawRect Alpha Shader",
            RectShaderVariant::Opaque => "DrawRect Opaque Shader",
        }),
        source: wgpu::ShaderSource::Wgsl(
            match (variant, render_mode) {
                (RectShaderVariant::Alpha, RectRenderMode::Combined) => {
                    include_str!("../../shader/rect_alpha.wgsl")
                }
                (RectShaderVariant::Opaque, RectRenderMode::Combined) => {
                    include_str!("../../shader/rect_opaque.wgsl")
                }
                (_, RectRenderMode::FillOnly) => include_str!("../../shader/rect_fill.wgsl"),
                (_, RectRenderMode::BorderOnly) => include_str!("../../shader/rect_border.wgsl"),
            }
            .into(),
        ),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("DrawRect Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: Some(NonZeroU64::new(RECT_UNIFORM_SLOT_SIZE).unwrap()),
            },
            count: None,
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("DrawRect Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("DrawRect Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: match variant {
                    RectShaderVariant::Alpha => Some(wgpu::BlendState {
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
                    RectShaderVariant::Opaque => Some(wgpu::BlendState {
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
                },
                write_mask: if color_write_enabled {
                    wgpu::ColorWrites::ALL
                } else {
                    wgpu::ColorWrites::empty()
                },
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: Some(match (variant, stencil_class) {
            (RectShaderVariant::Alpha, RectStencilClass::None) => wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            },
            (RectShaderVariant::Opaque, RectStencilClass::None) => wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            },
            (_, RectStencilClass::Test) => wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: matches!(variant, RectShaderVariant::Opaque),
                depth_compare: if matches!(variant, RectShaderVariant::Opaque) {
                    wgpu::CompareFunction::LessEqual
                } else {
                    wgpu::CompareFunction::Always
                },
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
                    write_mask: 0xFF,
                },
                bias: wgpu::DepthBiasState::default(),
            },
            (_, RectStencilClass::Increment) => wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState {
                    front: wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::Equal,
                        fail_op: wgpu::StencilOperation::Keep,
                        depth_fail_op: wgpu::StencilOperation::Keep,
                        pass_op: wgpu::StencilOperation::IncrementClamp,
                    },
                    back: wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::Equal,
                        fail_op: wgpu::StencilOperation::Keep,
                        depth_fail_op: wgpu::StencilOperation::Keep,
                        pass_op: wgpu::StencilOperation::IncrementClamp,
                    },
                    read_mask: 0xFF,
                    write_mask: 0xFF,
                },
                bias: wgpu::DepthBiasState::default(),
            },
            (_, RectStencilClass::Decrement) => wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState {
                    front: wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::Equal,
                        fail_op: wgpu::StencilOperation::Keep,
                        depth_fail_op: wgpu::StencilOperation::Keep,
                        pass_op: wgpu::StencilOperation::DecrementClamp,
                    },
                    back: wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::Equal,
                        fail_op: wgpu::StencilOperation::Keep,
                        depth_fail_op: wgpu::StencilOperation::Keep,
                        pass_op: wgpu::StencilOperation::DecrementClamp,
                    },
                    read_mask: 0xFF,
                    write_mask: 0xFF,
                },
                bias: wgpu::DepthBiasState::default(),
            },
        }),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    });

    let quad_vertices = [
        QuadVertex { uv: [0.0, 0.0] },
        QuadVertex { uv: [1.0, 0.0] },
        QuadVertex { uv: [1.0, 1.0] },
        QuadVertex { uv: [0.0, 1.0] },
    ];
    let quad_indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("DrawRect Quad Vertex Buffer"),
        contents: bytemuck::cast_slice(&quad_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("DrawRect Quad Index Buffer"),
        contents: bytemuck::cast_slice(&quad_indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    DrawRectResources {
        pipeline,
        bind_group_layout,
        vertex_buffer,
        index_buffer,
        index_count: quad_indices.len() as u32,
        pipeline_format: format,
        pipeline_sample_count: sample_count,
        variant,
        stencil_class,
        color_write_enabled,
        render_mode,
    }
}

fn build_rect_debug_overlay_geometry(
    params: RectParams,
    global_origin: [f32; 2],
    screen_w: f32,
    screen_h: f32,
    edge_color: [f32; 4],
    point_color: [f32; 4],
) -> (Vec<DebugVertex>, Vec<u32>) {
    let mut out_vertices = Vec::new();
    let mut out_indices = Vec::new();
    let [left, top, right, bottom] = params.outer_rect;
    let left = left + global_origin[0];
    let top = top + global_origin[1];
    let right = right + global_origin[0];
    let bottom = bottom + global_origin[1];
    if right <= left || bottom <= top {
        return (out_vertices, out_indices);
    }

    let corners = [[left, top], [right, top], [right, bottom], [left, bottom]];
    let mut edges = HashSet::new();
    for (u, v) in [(0_u32, 1_u32), (1, 2), (2, 3), (3, 0)] {
        edges.insert((u, v));
    }

    for (u, v) in edges {
        append_debug_line_quad(
            &mut out_vertices,
            &mut out_indices,
            corners[u as usize],
            corners[v as usize],
            1.5,
            edge_color,
            screen_w,
            screen_h,
        );
    }

    for corner in corners {
        append_debug_point_quad(
            &mut out_vertices,
            &mut out_indices,
            corner,
            4.0,
            point_color,
            screen_w,
            screen_h,
        );
    }

    (out_vertices, out_indices)
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
    let offset = [nx * hw, ny * hw];
    let quad = [
        [p0[0] + offset[0], p0[1] + offset[1]],
        [p0[0] - offset[0], p0[1] - offset[1]],
        [p1[0] - offset[0], p1[1] - offset[1]],
        [p1[0] + offset[0], p1[1] + offset[1]],
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
    for point in quad {
        vertices.push(DebugVertex {
            position: pixel_to_ndc(point[0], point[1], screen_w, screen_h),
            color,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn pixel_to_ndc(x: f32, y: f32, screen_w: f32, screen_h: f32) -> [f32; 2] {
    let nx = x / screen_w.max(1.0);
    let ny = y / screen_h.max(1.0);
    [nx * 2.0 - 1.0, 1.0 - ny * 2.0]
}

struct DrawRectResourcesCache<T> {
    entries: std::collections::HashMap<u64, T>,
}

impl<T> DrawRectResourcesCache<T> {
    fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    fn get_or_insert_with<F: FnOnce() -> T>(&mut self, key: u64, create: F) -> &mut T {
        self.entries.entry(key).or_insert_with(create)
    }

    fn begin_frame(&mut self) {
        let _ = &self.entries;
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

fn draw_rect_resources_cache() -> &'static Mutex<DrawRectResourcesCache<DrawRectResources>> {
    static CACHE: OnceLock<Mutex<DrawRectResourcesCache<DrawRectResources>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(DrawRectResourcesCache::new()))
}

pub fn begin_draw_rect_resources_frame() {
    let cache = draw_rect_resources_cache();
    let mut cache = cache.lock().unwrap();
    cache.begin_frame();
}

pub fn clear_draw_rect_resources_cache() {
    let cache = draw_rect_resources_cache();
    let mut cache = cache.lock().unwrap();
    cache.clear();
}

type CornerRadii = [[f32; 2]; 4]; // TL, TR, BR, BL

fn build_rect_params(
    position: [f32; 2],
    size: [f32; 2],
    border_widths_lr_tb: [f32; 4], // [left,right,top,bottom]
    mut outer_radii: CornerRadii,
    mut fill_color: [f32; 4],
    border_side_colors_lr_tb: [[f32; 4]; 4], // [left,right,top,bottom]
    opacity: f32,
    depth: f32,
    screen_w: f32,
    screen_h: f32,
) -> RectParams {
    let width = size[0].max(0.0);
    let height = size[1].max(0.0);

    let outer_min = [position[0], position[1]];
    let outer_max = [position[0] + width, position[1] + height];

    let max_bw = width.min(height) * 0.5;
    let b_left = border_widths_lr_tb[0].clamp(0.0, max_bw);
    let b_right = border_widths_lr_tb[1].clamp(0.0, max_bw);
    let b_top = border_widths_lr_tb[2].clamp(0.0, max_bw);
    let b_bottom = border_widths_lr_tb[3].clamp(0.0, max_bw);

    normalize_corner_radii_css_xy(&mut outer_radii, width, height);

    let inner_min = [outer_min[0] + b_left, outer_min[1] + b_top];
    let inner_max = [outer_max[0] - b_right, outer_max[1] - b_bottom];
    let inner_w = (inner_max[0] - inner_min[0]).max(0.0);
    let inner_h = (inner_max[1] - inner_min[1]).max(0.0);

    let mut inner_radii = [
        [
            (outer_radii[0][0] - b_left).max(0.0),
            (outer_radii[0][1] - b_top).max(0.0),
        ],
        [
            (outer_radii[1][0] - b_right).max(0.0),
            (outer_radii[1][1] - b_top).max(0.0),
        ],
        [
            (outer_radii[2][0] - b_right).max(0.0),
            (outer_radii[2][1] - b_bottom).max(0.0),
        ],
        [
            (outer_radii[3][0] - b_left).max(0.0),
            (outer_radii[3][1] - b_bottom).max(0.0),
        ],
    ];

    let has_inner = inner_w > 0.0 && inner_h > 0.0;
    if has_inner {
        normalize_corner_radii_css_xy(&mut inner_radii, inner_w, inner_h);
    } else {
        inner_radii = [[0.0, 0.0]; 4];
    }

    let opacity = opacity.clamp(0.0, 1.0);
    fill_color[3] *= opacity;

    let mut border_left = border_side_colors_lr_tb[0];
    let mut border_right = border_side_colors_lr_tb[1];
    let mut border_top = border_side_colors_lr_tb[2];
    let mut border_bottom = border_side_colors_lr_tb[3];
    border_left[3] *= opacity;
    border_right[3] *= opacity;
    border_top[3] *= opacity;
    border_bottom[3] *= opacity;

    RectParams {
        outer_rect: [outer_min[0], outer_min[1], outer_max[0], outer_max[1]],
        inner_rect: [inner_min[0], inner_min[1], inner_max[0], inner_max[1]],
        outer_rx: [
            outer_radii[0][0],
            outer_radii[1][0],
            outer_radii[2][0],
            outer_radii[3][0],
        ],
        outer_ry: [
            outer_radii[0][1],
            outer_radii[1][1],
            outer_radii[2][1],
            outer_radii[3][1],
        ],
        inner_rx: [
            inner_radii[0][0],
            inner_radii[1][0],
            inner_radii[2][0],
            inner_radii[3][0],
        ],
        inner_ry: [
            inner_radii[0][1],
            inner_radii[1][1],
            inner_radii[2][1],
            inner_radii[3][1],
        ],
        border_widths: [b_left, b_top, b_right, b_bottom],
        flags: [if has_inner { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        fill_color,
        border_left,
        border_top,
        border_right,
        border_bottom,
        screen_size: [
            screen_w,
            screen_h,
            1.0 / screen_w.max(1.0),
            1.0 / screen_h.max(1.0),
        ],
        depth,
        _pad2: [0.0; 7],
    }
}

fn normalize_corner_radii_css_xy(radii: &mut CornerRadii, width: f32, height: f32) {
    let w = width.max(0.0);
    let h = height.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        *radii = [[0.0, 0.0]; 4];
        return;
    }

    for r in radii.iter_mut() {
        r[0] = r[0].max(0.0);
        r[1] = r[1].max(0.0);
    }

    let sum_top_x = radii[0][0] + radii[1][0];
    let sum_bottom_x = radii[3][0] + radii[2][0];
    let sum_left_y = radii[0][1] + radii[3][1];
    let sum_right_y = radii[1][1] + radii[2][1];

    let sx = [
        if sum_top_x > 0.0 { w / sum_top_x } else { 1.0 },
        if sum_bottom_x > 0.0 {
            w / sum_bottom_x
        } else {
            1.0
        },
    ]
    .into_iter()
    .fold(1.0_f32, f32::min)
    .min(1.0);

    let sy = [
        if sum_left_y > 0.0 {
            h / sum_left_y
        } else {
            1.0
        },
        if sum_right_y > 0.0 {
            h / sum_right_y
        } else {
            1.0
        },
    ]
    .into_iter()
    .fold(1.0_f32, f32::min)
    .min(1.0);

    for r in radii.iter_mut() {
        r[0] *= sx;
        r[1] *= sy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_smaller_than_border_clamps_inner_radii_and_inner_rect_safely() {
        let params = build_rect_params(
            [10.0, 20.0],
            [20.0, 16.0],
            [12.0, 11.0, 13.0, 10.0], // left, right, top, bottom
            [[6.0, 5.0], [4.0, 4.0], [7.0, 6.0], [5.0, 3.0]],
            [0.2, 0.3, 0.4, 1.0],
            [
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                [1.0, 1.0, 0.0, 1.0],
            ],
            1.0,
            0.0,
            800.0,
            600.0,
        );

        assert_eq!(
            params.flags[0], 0.0,
            "inner rect should be disabled when collapsed"
        );
        for &v in params.inner_rx.iter().chain(params.inner_ry.iter()) {
            assert!(v >= 0.0);
            assert_eq!(v, 0.0);
        }
        assert!(
            params.inner_rect[2] <= params.inner_rect[0]
                || params.inner_rect[3] <= params.inner_rect[1]
        );
    }

    #[test]
    fn css_radius_normalization_scales_xy_to_avoid_overlap() {
        // width=100, height=60.
        // x sums: top=140, bottom=140 => sx = 100/140 = 0.7142857...
        // y sums: left=90, right=90 => sy = 60/90 = 0.6666666...
        let params = build_rect_params(
            [0.0, 0.0],
            [100.0, 60.0],
            [4.0, 4.0, 4.0, 4.0],
            [[70.0, 45.0], [70.0, 45.0], [70.0, 45.0], [70.0, 45.0]],
            [0.1, 0.2, 0.3, 1.0],
            [[0.4, 0.4, 0.4, 1.0]; 4],
            1.0,
            0.0,
            1000.0,
            800.0,
        );

        let sx = 100.0 / 140.0;
        let sy = 60.0 / 90.0;
        let expected_rx = 70.0 * sx;
        let expected_ry = 45.0 * sy;

        for i in 0..4 {
            assert!((params.outer_rx[i] - expected_rx).abs() < 1e-4);
            assert!((params.outer_ry[i] - expected_ry).abs() < 1e-4);
        }

        // Ensure adjacent sums are clamped to bounds after normalization.
        let top_sum = params.outer_rx[0] + params.outer_rx[1];
        let bottom_sum = params.outer_rx[3] + params.outer_rx[2];
        let left_sum = params.outer_ry[0] + params.outer_ry[3];
        let right_sum = params.outer_ry[1] + params.outer_ry[2];
        assert!(top_sum <= 100.0 + 1e-4);
        assert!(bottom_sum <= 100.0 + 1e-4);
        assert!(left_sum <= 60.0 + 1e-4);
        assert!(right_sum <= 60.0 + 1e-4);
    }

    #[test]
    fn opaque_rect_depth_is_derived_from_build_time_order() {
        let base = DrawRectPass::new(
            RectPassParams::default(),
            DrawRectInput::default(),
            DrawRectOutput::default(),
        );
        let mut first = OpaqueRectPass::from_draw_rect_pass(base);
        let mut later = OpaqueRectPass::from_draw_rect_pass(DrawRectPass::new(
            RectPassParams::default(),
            DrawRectInput::default(),
            DrawRectOutput::default(),
        ));

        first.set_depth_order(0);
        later.set_depth_order(1);

        assert!(first.inner.params.depth > later.inner.params.depth);
        assert!(first.inner.params.depth <= 1.0);
        assert!(later.inner.params.depth >= 0.0);
    }

    #[test]
    fn opaque_rect_inherits_parent_stencil_clip() {
        let mut pass = OpaqueRectPass::from_draw_rect_pass(DrawRectPass::new(
            RectPassParams::default(),
            DrawRectInput {
                pass_context: RenderPassContext {
                    stencil_clip_id: Some(3),
                    ..Default::default()
                },
                ..Default::default()
            },
            DrawRectOutput::default(),
        ));

        pass.inner.inherit_stencil_clip_if_needed();

        assert_eq!(
            pass.inner.stencil_mode,
            RectStencilMode::Test { clip_id: 3 }
        );
    }
}
