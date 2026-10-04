use crate::view::frame_graph::slot::{InSlot, OutSlot};
#[cfg(test)]
use crate::view::frame_graph::texture_resource::TextureHandle;
use crate::view::frame_graph::texture_resource::TextureResource;
use crate::view::frame_graph::{
    GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy, PrepareContext,
};
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, render_target_origin, render_target_sample_count,
    resolve_graphics_pass_scissor_to_target_physical, resolve_texture_ref,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use rustc_hash::FxHashSet;
use std::num::NonZeroU64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GradientKindGpu {
    #[default]
    Linear,
    Radial,
    Conic,
}

/// Packed gradient stop layout matching the WGSL storage buffer element.
/// 32 bytes stride: `color: vec4<f32>`, `pos: vec4<f32>` (.x is position).
#[derive(Default, Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct GradientStopGpu {
    pub color: [f32; 4],
    pub pos: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct GradientPaint {
    pub kind: GradientKindGpu,
    /// Linear: [p0.x, p0.y, p1.x, p1.y] in local paint-box pixels (pre-scale).
    /// Radial: [cx, cy, rx, ry].
    /// Conic:  [cx, cy, from_angle_rad, 0].
    pub axis: [f32; 4],
    pub repeating: bool,
    pub stops: std::sync::Arc<[GradientStopGpu]>,
}

impl Default for GradientPaint {
    fn default() -> Self {
        Self {
            kind: GradientKindGpu::Linear,
            axis: [0.0; 4],
            repeating: false,
            stops: std::sync::Arc::from(Vec::<GradientStopGpu>::new()),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct RectPassParams {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub fill_color: [f32; 4],
    pub opacity: f32,
    pub border_widths: [f32; 4],
    pub border_radii: [[f32; 2]; 4],
    pub border_color: [f32; 4],
    pub border_side_colors: [[f32; 4]; 4],
    pub use_border_side_colors: bool,
    pub depth: f32,
    pub gradient: Option<GradientPaint>,
    pub border_gradient: Option<GradientPaint>,
}

impl RectPassParams {
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
}

pub struct DrawRectPass {
    params: RectPassParams,
    scissor_rect: Option<[u32; 4]>,
    stencil_mode: RectStencilMode,
    color_write_enabled: bool,
    clear_target: bool,
    render_mode: RectRenderMode,
    /// Recording state resolved by `prepare`; `None` until prepare succeeds.
    prepared: Option<PreparedRect>,
    input: DrawRectInput,
    output: DrawRectOutput,
}

/// Outcome of preparing one rectangle for the current frame.
enum PreparedRect {
    /// Degenerate geometry: nothing is drawn.
    Empty,
    Draw(PreparedRectDraw),
}

/// Everything recording needs, resolved once during prepare: the pipeline,
/// the layout used to bind this frame's instance storage, the dynamic state,
/// and this rectangle's index into the frame's instance array.
struct PreparedRectDraw {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    layout_key: u64,
    uses_gradient_stops: bool,
    stencil_reference: Option<u32>,
    /// Target-physical scissor `[x, y, width, height]`.
    scissor: [u32; 4],
    instance: u32,
}

#[cfg(test)]
impl DrawRectPass {
    pub(crate) fn test_snapshot(&self) -> RectPassTestSnapshot {
        RectPassTestSnapshot::from_pass(self, false, None)
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GradientStopTestSnapshot {
    pub(crate) color_bits: [u32; 4],
    pub(crate) position_bits: [u32; 4],
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GradientPaintTestSnapshot {
    pub(crate) kind: GradientKindGpu,
    pub(crate) axis_bits: [u32; 4],
    pub(crate) repeating: bool,
    pub(crate) stops: Vec<GradientStopTestSnapshot>,
}

#[cfg(test)]
impl From<&GradientPaint> for GradientPaintTestSnapshot {
    fn from(paint: &GradientPaint) -> Self {
        Self {
            kind: paint.kind,
            axis_bits: paint.axis.map(f32::to_bits),
            repeating: paint.repeating,
            stops: paint
                .stops
                .iter()
                .map(|stop| GradientStopTestSnapshot {
                    color_bits: stop.color.map(f32::to_bits),
                    position_bits: stop.pos.map(f32::to_bits),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RectStencilModeTestSnapshot {
    Disabled,
    Test { clip_id: u8 },
    Increment { clip_id: u8 },
    Decrement { clip_id: u8 },
}

#[cfg(test)]
impl From<RectStencilMode> for RectStencilModeTestSnapshot {
    fn from(mode: RectStencilMode) -> Self {
        match mode {
            RectStencilMode::Disabled => Self::Disabled,
            RectStencilMode::Test { clip_id } => Self::Test { clip_id },
            RectStencilMode::Increment { clip_id } => Self::Increment { clip_id },
            RectStencilMode::Decrement { clip_id } => Self::Decrement { clip_id },
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RectPassTestSnapshot {
    pub(crate) opaque: bool,
    pub(crate) opaque_depth_order: Option<u32>,
    pub(crate) position_bits: [u32; 2],
    pub(crate) size_bits: [u32; 2],
    pub(crate) fill_color_bits: [u32; 4],
    pub(crate) opacity_bits: u32,
    pub(crate) border_width_bits: [u32; 4],
    pub(crate) border_radius_bits: [[u32; 2]; 4],
    pub(crate) border_color_bits: [u32; 4],
    pub(crate) border_side_color_bits: [[u32; 4]; 4],
    pub(crate) use_border_side_colors: bool,
    pub(crate) depth_bits: u32,
    pub(crate) gradient: Option<GradientPaintTestSnapshot>,
    pub(crate) border_gradient: Option<GradientPaintTestSnapshot>,
    pub(crate) mode: RectRenderMode,
    pub(crate) explicit_scissor_rect: Option<[u32; 4]>,
    pub(crate) effective_scissor_rect: Option<[u32; 4]>,
    pub(crate) stencil_mode: RectStencilModeTestSnapshot,
    pub(crate) color_write_enabled: bool,
    pub(crate) clear_target: bool,
    pub(crate) input_target: Option<TextureHandle>,
    pub(crate) output_target: Option<TextureHandle>,
    pub(crate) pass_context: RenderPassContext,
}

#[cfg(test)]
impl RectPassTestSnapshot {
    fn from_pass(pass: &DrawRectPass, opaque: bool, opaque_depth_order: Option<u32>) -> Self {
        Self {
            opaque,
            opaque_depth_order,
            position_bits: pass.params.position.map(f32::to_bits),
            size_bits: pass.params.size.map(f32::to_bits),
            fill_color_bits: pass.params.fill_color.map(f32::to_bits),
            opacity_bits: pass.params.opacity.to_bits(),
            border_width_bits: pass.params.border_widths.map(f32::to_bits),
            border_radius_bits: pass
                .params
                .border_radii
                .map(|radius| radius.map(f32::to_bits)),
            border_color_bits: pass.params.border_color.map(f32::to_bits),
            border_side_color_bits: pass
                .params
                .border_side_colors
                .map(|color| color.map(f32::to_bits)),
            use_border_side_colors: pass.params.use_border_side_colors,
            depth_bits: pass.params.depth.to_bits(),
            gradient: pass.params.gradient.as_ref().map(Into::into),
            border_gradient: pass.params.border_gradient.as_ref().map(Into::into),
            mode: pass.render_mode,
            explicit_scissor_rect: pass.scissor_rect,
            effective_scissor_rect: intersect_scissor_rects(
                pass.input.pass_context.logical_scissor_rect(),
                pass.scissor_rect,
            ),
            stencil_mode: pass.stencil_mode.into(),
            color_write_enabled: pass.color_write_enabled,
            clear_target: pass.clear_target,
            input_target: pass.input.render_target.handle(),
            output_target: pass.output.render_target.handle(),
            pass_context: pass.input.pass_context,
        }
    }
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
            prepared: None,
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

    pub fn set_render_mode(&mut self, mode: RectRenderMode) {
        self.render_mode = mode;
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

    /// Resolves this rectangle for the frame: appends its instance (and any
    /// gradient stops) to the viewport's per-frame arrays and records the
    /// pipeline, scissor and stencil reference used when recording.
    fn prepare_instance(&mut self, ctx: &mut PrepareContext<'_, '_>, variant: RectShaderVariant) {
        self.prepared = None;
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
        // Degenerate rects draw nothing and stage no instance.
        let outer_max = [
            scaled_position[0] + scaled_size[0].max(0.0),
            scaled_position[1] + scaled_size[1].max(0.0),
        ];
        if outer_max[0] <= scaled_position[0] || outer_max[1] <= scaled_position[1] {
            self.prepared = Some(PreparedRect::Empty);
            return;
        }
        // Without a device no pipeline exists; recording reports the failure.
        let Some(device) = ctx.viewport.device().cloned() else {
            return;
        };
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
        let gradient_upload = self.params.gradient.as_ref().and_then(|g| {
            push_gradient_paint_stops(ctx.viewport, g, self.params.opacity).map(|stops_start| {
                GradientUploadInfo {
                    kind: g.kind,
                    repeating: g.repeating,
                    stop_count: g.stops.len() as u32,
                    stops_start_index: stops_start,
                    axis_scaled: scaled_gradient_axis(g, scale, scaled_position),
                }
            })
        });
        let border_gradient_upload = self.params.border_gradient.as_ref().and_then(|g| {
            push_gradient_paint_stops(ctx.viewport, g, self.params.opacity).map(|stops_start| {
                GradientUploadInfo {
                    kind: g.kind,
                    repeating: g.repeating,
                    stop_count: g.stops.len() as u32,
                    stops_start_index: stops_start,
                    axis_scaled: scaled_gradient_axis(g, scale, scaled_position),
                }
            })
        });
        let instance = build_rect_instance(
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
            gradient_upload.as_ref(),
            border_gradient_upload.as_ref(),
        );
        if ctx.viewport.debug_options().geometry_overlay {
            let (overlay_w, overlay_h) = ctx.viewport.surface_size();
            let (debug_vertices, debug_indices) = build_rect_debug_overlay_geometry(
                instance,
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

        let format = ctx.viewport.offscreen_format();
        let sample_count = self
            .output
            .render_target
            .handle()
            .and_then(|handle| render_target_sample_count(ctx, handle))
            .unwrap_or_else(|| ctx.viewport.msaa_sample_count());
        let (stencil_class, stencil_reference) = stencil_class_and_reference(self.stencil_mode);
        let shape = RectShaderShape::detect(
            self.render_mode,
            self.params.fill_color,
            self.params.border_widths,
            self.params.border_color,
            self.params.border_side_colors,
            self.params.use_border_side_colors,
            self.params.border_radii,
            self.params.gradient.is_some(),
            self.params.border_gradient.is_some(),
        );
        let layout_key = rect_resource_cache_key(
            variant,
            stencil_class,
            self.color_write_enabled,
            self.render_mode,
            shape,
        );
        let (pipeline, bind_group_layout) = with_draw_rect_resources_cache(|cache| {
            let resources = cache.get_or_insert_scoped_with(
                ctx.viewport.render_resource_scope_id(),
                layout_key,
                || {
                    create_draw_rect_resources(
                        &device,
                        format,
                        sample_count,
                        variant,
                        stencil_class,
                        self.color_write_enabled,
                        self.render_mode,
                        shape,
                    )
                },
            );
            if resources.pipeline_format != format
                || resources.pipeline_sample_count != sample_count
                || resources.variant != variant
                || resources.stencil_class != stencil_class
                || resources.color_write_enabled != self.color_write_enabled
                || resources.render_mode != self.render_mode
                || resources.shape != shape
            {
                *resources = create_draw_rect_resources(
                    &device,
                    format,
                    sample_count,
                    variant,
                    stencil_class,
                    self.color_write_enabled,
                    self.render_mode,
                    shape,
                );
            }
            (
                resources.pipeline.clone(),
                resources.bind_group_layout.clone(),
            )
        });
        let scissor = resolve_graphics_pass_scissor_to_target_physical(
            ctx.viewport,
            self.input.pass_context.scissor_rect,
            self.scissor_rect,
            target_origin,
            (target_w, target_h),
        )
        .unwrap_or([0, 0, target_w, target_h]);
        let instance = ctx.viewport.push_rect_instance(instance);
        self.prepared = Some(PreparedRect::Draw(PreparedRectDraw {
            pipeline,
            bind_group_layout,
            layout_key,
            uses_gradient_stops: shape.has_gradient || shape.has_border_gradient,
            stencil_reference: stencil_reference.map(u32::from),
            scissor,
            instance,
        }));
    }

    /// Records the prepared instance. A rectangle that was never prepared
    /// fails the frame.
    fn record_prepared(&self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        let draw = match &self.prepared {
            Some(PreparedRect::Draw(draw)) => draw,
            Some(PreparedRect::Empty) => return,
            None => {
                ctx.mark_execution_failed();
                return;
            }
        };
        // Resolved at record time: a later flush in the same frame may have
        // replaced the instance or gradient buffer since this rect prepared.
        let Some(bind_group) = ctx.viewport().rect_bind_group(
            draw.layout_key,
            &draw.bind_group_layout,
            draw.uses_gradient_stops,
        ) else {
            ctx.mark_execution_failed();
            return;
        };
        ctx.set_pipeline(&draw.pipeline);
        ctx.set_bind_group(0, &bind_group, &[]);
        if let Some(reference) = draw.stencil_reference {
            ctx.set_stencil_reference(reference);
        }
        let [x, y, width, height] = draw.scissor;
        ctx.set_scissor_rect(x, y, width, height);
        // The shader emits the six quad corners procedurally per instance.
        ctx.draw(0..6, draw.instance..draw.instance + 1);
    }
}

impl OpaqueRectPass {
    #[cfg(test)]
    pub(crate) fn test_snapshot(&self) -> RectPassTestSnapshot {
        RectPassTestSnapshot::from_pass(&self.inner, true, Some(self.depth_order))
    }

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

#[cfg(test)]
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
pub(crate) const RECT_INSTANCE_STRIDE: u64 = std::mem::size_of::<RectInstance>() as u64;
pub(crate) const RECT_INSTANCE_BUFFER_INITIAL_CAPACITY: u64 = 1024 * RECT_INSTANCE_STRIDE;
pub(crate) const GRADIENT_STOP_STRIDE: u64 = 32;
pub(crate) const GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY: u64 = 256 * GRADIENT_STOP_STRIDE;
// The WGSL storage array stride for 18 `vec4<f32>` fields.
const _: () = assert!(RECT_INSTANCE_STRIDE == 18 * 16);
const _: () = assert!(GRADIENT_STOP_STRIDE == std::mem::size_of::<GradientStopGpu>() as u64);

#[derive(Clone, Copy)]
pub struct RenderTargetTag;
pub type RenderTargetIn = InSlot<TextureResource, RenderTargetTag>;
pub type RenderTargetOut = OutSlot<TextureResource, RenderTargetTag>;

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
        self.prepare_instance(ctx, RectShaderVariant::Alpha);
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        self.record_prepared(ctx);
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
        self.inner.prepare_instance(ctx, RectShaderVariant::Opaque);
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        self.inner.record_prepared(ctx);
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
    shape: RectShaderShape,
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
    let border_id = match shape.border {
        super::rect_shader::RectBorderKind::None => 0_u64,
        super::rect_shader::RectBorderKind::Uniform => 1_u64,
        super::rect_shader::RectBorderKind::PerSide => 2_u64,
    };
    let fill_id = if shape.has_fill { 1_u64 } else { 0_u64 };
    let round_id = if shape.rounded { 1_u64 } else { 0_u64 };
    let gradient_id = if shape.has_gradient { 1_u64 } else { 0_u64 };
    let border_gradient_id = if shape.has_border_gradient {
        1_u64
    } else {
        0_u64
    };
    RECT_RESOURCES_BASE
        + variant_id * 1_000_000
        + border_gradient_id * 400_000
        + gradient_id * 200_000
        + stencil_id * 10_000
        + color_id * 1_000
        + mode_id * 100
        + border_id * 16
        + fill_id * 4
        + round_id
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct RectShaderShape {
    pub has_fill: bool,
    pub border: super::rect_shader::RectBorderKind,
    pub rounded: bool,
    pub has_gradient: bool,
    pub has_border_gradient: bool,
}

impl RectShaderShape {
    pub(crate) fn detect(
        render_mode: RectRenderMode,
        fill_color: [f32; 4],
        border_widths: [f32; 4],
        border_color: [f32; 4],
        border_side_colors: [[f32; 4]; 4],
        use_border_side_colors: bool,
        border_radii: [[f32; 2]; 4],
        has_gradient: bool,
        has_border_gradient: bool,
    ) -> Self {
        use super::rect_shader::RectBorderKind;
        let has_any_border_width = border_widths.iter().any(|&w| w > 0.0);
        let effective_sides = if use_border_side_colors {
            border_side_colors
        } else {
            [border_color; 4]
        };
        let border_alpha_zero = effective_sides.iter().all(|c| c[3] <= 0.0);
        let border = if !has_any_border_width || border_alpha_zero {
            RectBorderKind::None
        } else {
            let c0 = effective_sides[0];
            let uniform = effective_sides.iter().all(|c| {
                (c[0] - c0[0]).abs() < 1e-6
                    && (c[1] - c0[1]).abs() < 1e-6
                    && (c[2] - c0[2]).abs() < 1e-6
                    && (c[3] - c0[3]).abs() < 1e-6
            });
            if uniform {
                RectBorderKind::Uniform
            } else {
                RectBorderKind::PerSide
            }
        };
        let rounded = border_radii.iter().any(|r| r[0] > 0.0 || r[1] > 0.0);
        let has_fill = fill_color[3] > 0.0 || has_gradient;
        // Narrow per render_mode so unused axes don't bloat pipeline cache.
        match render_mode {
            RectRenderMode::FillOnly => Self {
                has_fill: true,
                border: RectBorderKind::None,
                rounded,
                has_gradient,
                has_border_gradient: false,
            },
            RectRenderMode::BorderOnly => Self {
                has_fill: false,
                border: if matches!(border, RectBorderKind::None) {
                    RectBorderKind::Uniform
                } else {
                    border
                },
                rounded,
                has_gradient: false,
                has_border_gradient,
            },
            RectRenderMode::Combined => Self {
                has_fill,
                border,
                rounded,
                has_gradient,
                has_border_gradient,
            },
        }
    }
}

fn stencil_class_and_reference(stencil_mode: RectStencilMode) -> (RectStencilClass, Option<u8>) {
    match stencil_mode {
        RectStencilMode::Disabled => (RectStencilClass::None, None),
        RectStencilMode::Test { clip_id } => (RectStencilClass::Test, Some(clip_id)),
        RectStencilMode::Increment { clip_id } => (RectStencilClass::Increment, Some(clip_id)),
        RectStencilMode::Decrement { clip_id } => (RectStencilClass::Decrement, Some(clip_id)),
    }
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct DebugVertex {
    position: [f32; 2],
    color: [f32; 4],
}

/// One rectangle in the per-frame instance storage buffer. The layout matches
/// `RectInstance` in rect.wgsl (an array element, 16-byte aligned fields).
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub(crate) struct RectInstance {
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
    // flags.x: has_inner (0/1), flags.y: depth, flags.zw reserved.
    flags: [f32; 4],
    // linear-space, straight alpha (premultiply in shader)
    fill_color: [f32; 4],
    border_left: [f32; 4],
    border_top: [f32; 4],
    border_right: [f32; 4],
    border_bottom: [f32; 4],
    // [w, h, inv_w, inv_h]
    screen_size: [f32; 4],
    // x: gradient_kind (0=none, 1=linear, 2=radial, 3=conic)
    // y: stop_count, z: repeating (0/1), w: stops_start_index (as f32)
    gradient_info: [f32; 4],
    // Linear: [p0.x, p0.y, p1.x, p1.y] in physical pixels.
    // Radial: [cx, cy, rx, ry].
    // Conic:  [cx, cy, from_angle_rad, 0].
    gradient_axis: [f32; 4],
    // Same layout as gradient_info / gradient_axis, but for border paint.
    border_gradient_info: [f32; 4],
    border_gradient_axis: [f32; 4],
}

pub(crate) struct DrawRectResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_format: wgpu::TextureFormat,
    pipeline_sample_count: u32,
    variant: RectShaderVariant,
    stencil_class: RectStencilClass,
    color_write_enabled: bool,
    render_mode: RectRenderMode,
    shape: RectShaderShape,
}

fn create_draw_rect_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
    variant: RectShaderVariant,
    stencil_class: RectStencilClass,
    color_write_enabled: bool,
    render_mode: RectRenderMode,
    shape: RectShaderShape,
) -> DrawRectResources {
    use super::rect_shader::{RectShaderKey, build_rect_shader};
    let shader = build_rect_shader(
        device,
        RectShaderKey {
            has_fill: shape.has_fill,
            border: shape.border,
            rounded: shape.rounded,
            opaque: matches!(variant, RectShaderVariant::Opaque),
            pass: render_mode,
            has_gradient: shape.has_gradient,
            has_border_gradient: shape.has_border_gradient,
        },
    );

    // Binding 0 is the frame's instance array, indexed by `instance_index`.
    // A solid shader never reads gradient stops, so it omits binding 1 rather
    // than binding an unused dummy. The shape is also part of the
    // resource/layout cache key, so gradient variants stay distinct.
    let mut entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(RECT_INSTANCE_STRIDE),
        },
        count: None,
    }];
    if shape.has_gradient || shape.has_border_gradient {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("DrawRect Bind Group Layout"),
        entries: &entries,
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("DrawRect Pipeline Layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("DrawRect Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
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
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            },
            (RectShaderVariant::Opaque, RectStencilClass::None) => wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            },
            (_, RectStencilClass::Test) => wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(matches!(variant, RectShaderVariant::Opaque)),
                depth_compare: if matches!(variant, RectShaderVariant::Opaque) {
                    Some(wgpu::CompareFunction::LessEqual)
                } else {
                    Some(wgpu::CompareFunction::Always)
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
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
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
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
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

    DrawRectResources {
        pipeline,
        bind_group_layout,
        pipeline_format: format,
        pipeline_sample_count: sample_count,
        variant,
        stencil_class,
        color_write_enabled,
        render_mode,
        shape,
    }
}

fn build_rect_debug_overlay_geometry(
    params: RectInstance,
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
    let mut edges = FxHashSet::default();
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

crate::static_resource_cache! {
    fn with_draw_rect_resources_cache -> ResourceCache<DrawRectResources>
        = stats("draw_rect_pipeline")
}

pub fn begin_draw_rect_resources_frame() {
    with_draw_rect_resources_cache(|_cache| {});
}

pub fn clear_draw_rect_resources_cache() {
    with_draw_rect_resources_cache(|cache| {
        cache.clear();
    });
}

pub(super) fn release_scope(scope: u64) {
    with_draw_rect_resources_cache(|cache| cache.clear_scope(scope));
}

type CornerRadii = [[f32; 2]; 4]; // TL, TR, BR, BL

fn build_rect_instance(
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
    gradient: Option<&GradientUploadInfo>,
    border_gradient: Option<&GradientUploadInfo>,
) -> RectInstance {
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

    fn gradient_uniform(g: Option<&GradientUploadInfo>) -> ([f32; 4], [f32; 4]) {
        let mut info = [0.0_f32; 4];
        let mut axis = [0.0_f32; 4];
        if let Some(g) = g {
            info[0] = match g.kind {
                GradientKindGpu::Linear => 1.0,
                GradientKindGpu::Radial => 2.0,
                GradientKindGpu::Conic => 3.0,
            };
            info[1] = g.stop_count as f32;
            info[2] = if g.repeating { 1.0 } else { 0.0 };
            info[3] = g.stops_start_index as f32;
            axis = g.axis_scaled;
        }
        (info, axis)
    }
    let (gradient_info, gradient_axis) = gradient_uniform(gradient);
    let (border_gradient_info, border_gradient_axis) = gradient_uniform(border_gradient);

    RectInstance {
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
        flags: [if has_inner { 1.0 } else { 0.0 }, depth, 0.0, 0.0],
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
        gradient_info,
        gradient_axis,
        border_gradient_info,
        border_gradient_axis,
    }
}

/// Gradient paint axis resolved to physical pixels plus the frame's gradient
/// stop array start index for this draw's stops.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GradientUploadInfo {
    pub kind: GradientKindGpu,
    pub repeating: bool,
    pub stop_count: u32,
    pub stops_start_index: u32,
    pub axis_scaled: [f32; 4],
}

fn push_gradient_paint_stops(
    viewport: &mut crate::view::viewport::Viewport,
    paint: &GradientPaint,
    opacity: f32,
) -> Option<u32> {
    let opacity = opacity.clamp(0.0, 1.0);
    viewport.push_gradient_stops(paint.stops.iter().map(|stop| {
        let mut stop = *stop;
        stop.color[3] *= opacity;
        stop
    }))
}

fn scaled_gradient_axis(paint: &GradientPaint, scale: f32, origin: [f32; 2]) -> [f32; 4] {
    let a = paint.axis;
    match paint.kind {
        GradientKindGpu::Linear => [
            origin[0] + a[0] * scale,
            origin[1] + a[1] * scale,
            origin[0] + a[2] * scale,
            origin[1] + a[3] * scale,
        ],
        GradientKindGpu::Radial => [
            origin[0] + a[0] * scale,
            origin[1] + a[1] * scale,
            a[2] * scale,
            a[3] * scale,
        ],
        GradientKindGpu::Conic => [
            origin[0] + a[0] * scale,
            origin[1] + a[1] * scale,
            a[2],
            0.0,
        ],
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
mod tests;
