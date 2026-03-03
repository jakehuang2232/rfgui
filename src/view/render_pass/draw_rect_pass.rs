use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::render_pass::RenderPass;
use crate::view::render_pass::render_target::{
    render_target_bundle, render_target_msaa_view, render_target_size, render_target_view,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use wgpu::util::DeviceExt;

pub struct DrawRectPass {
    position: [f32; 2],
    size: [f32; 2],
    fill_color: [f32; 4],
    border_color: [f32; 4],
    border_side_colors: [[f32; 4]; 4], // [left, right, top, bottom]
    use_border_side_colors: bool,
    border_widths: [f32; 4],     // [left, right, top, bottom]
    border_radii: [[f32; 2]; 4], // [top_left, top_right, bottom_right, bottom_left] each is [rx, ry]
    opacity: f32,
    depth: f32,
    scissor_rect: Option<[u32; 4]>,
    stencil_mode: RectStencilMode,
    color_write_enabled: bool,
    color_target: Option<TextureHandle>,
    render_mode: RectRenderMode,
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
}

#[derive(Default)]
pub struct DrawRectOutput {
    pub render_target: RenderTargetOut,
}

#[derive(Default)]
pub struct OpaqueRectInput {
    pub render_target: RenderTargetIn,
}

#[derive(Default)]
pub struct OpaqueRectOutput {
    pub render_target: RenderTargetOut,
}

pub struct OpaqueRectPass {
    inner: DrawRectPass,
    input: OpaqueRectInput,
    output: OpaqueRectOutput,
    depth_order: u32,
}

impl DrawRectPass {
    pub fn new(position: [f32; 2], size: [f32; 2], color: [f32; 4], opacity: f32) -> Self {
        Self {
            position,
            size,
            fill_color: color,
            border_color: [0.0, 0.0, 0.0, 0.0],
            border_side_colors: [[0.0, 0.0, 0.0, 0.0]; 4],
            use_border_side_colors: false,
            border_widths: [0.0; 4],
            border_radii: [[0.0, 0.0]; 4],
            opacity,
            depth: 0.0,
            scissor_rect: None,
            stencil_mode: RectStencilMode::Disabled,
            color_write_enabled: true,
            color_target: None,
            render_mode: RectRenderMode::Combined,
            input: DrawRectInput::default(),
            output: DrawRectOutput::default(),
        }
    }

    pub fn set_position(&mut self, position: [f32; 2]) {
        self.position = position;
    }

    pub fn set_size(&mut self, size: [f32; 2]) {
        self.size = size;
    }

    pub fn set_color(&mut self, color: [f32; 4]) {
        self.fill_color = color;
    }

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

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }

    pub fn set_depth(&mut self, depth: f32) {
        self.depth = depth.clamp(0.0, 1.0);
    }

    pub fn set_input(&mut self, input: RenderTargetIn) {
        self.input.render_target = input;
    }

    pub fn set_output(&mut self, output: RenderTargetOut) {
        self.output.render_target = output;
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

    pub fn set_color_write_enabled(&mut self, enabled: bool) {
        self.color_write_enabled = enabled;
    }

    pub fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        self.color_target = color_target;
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
        let opacity = self.opacity.clamp(0.0, 1.0);
        if opacity < OPAQUE_THRESHOLD {
            return false;
        }
        if !matches!(self.render_mode, RectRenderMode::BorderOnly)
            && self.fill_color[3].clamp(0.0, 1.0) < OPAQUE_THRESHOLD
        {
            return false;
        }
        let side_colors = if self.use_border_side_colors {
            self.border_side_colors
        } else {
            [self.border_color; 4]
        };
        let side_widths = self.border_widths;
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

    pub fn batch_key(&self) -> DrawRectBatchKey {
        let (stencil_class, _) = stencil_class_and_reference(self.stencil_mode);
        DrawRectBatchKey {
            color_target: self.color_target,
            stencil_class,
            color_write_enabled: self.color_write_enabled,
            render_mode: self.render_mode,
        }
    }

    pub fn snapshot_draw(&self) -> DrawRectDraw {
        DrawRectDraw {
            position: self.position,
            size: self.size,
            fill_color: self.fill_color,
            border_color: self.border_color,
            border_side_colors: self.border_side_colors,
            use_border_side_colors: self.use_border_side_colors,
            border_widths: self.border_widths,
            border_radii: self.border_radii,
            opacity: self.opacity,
            depth: self.depth,
            scissor_rect: self.scissor_rect,
            stencil_mode: self.stencil_mode,
            color_write_enabled: self.color_write_enabled,
            color_target: self.color_target,
            render_mode: self.render_mode,
        }
    }
}

impl OpaqueRectPass {
    pub fn new(position: [f32; 2], size: [f32; 2], color: [f32; 4], opacity: f32) -> Self {
        Self {
            inner: DrawRectPass::new(position, size, color, opacity),
            input: OpaqueRectInput::default(),
            output: OpaqueRectOutput::default(),
            depth_order: 0,
        }
    }

    pub fn from_draw_rect_pass(pass: DrawRectPass) -> Self {
        Self {
            inner: pass,
            input: OpaqueRectInput::default(),
            output: OpaqueRectOutput::default(),
            depth_order: 0,
        }
    }

    pub fn set_output(&mut self, output: RenderTargetOut) {
        self.output.render_target = output;
    }

    pub fn set_input(&mut self, input: RenderTargetIn) {
        self.input.render_target = input;
    }

    pub fn set_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.inner.set_scissor_rect(scissor_rect);
    }

    pub fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        self.inner.set_color_target(color_target);
    }

    pub fn set_depth_order(&mut self, depth_order: u32) {
        self.depth_order = depth_order;
    }

    pub fn normalize_depth(&mut self, total_count: u32) {
        let denom = total_count.max(1) as f32;
        let t = (self.depth_order as f32 + 0.5) / denom;
        self.inner.set_depth((1.0 - t).clamp(0.0, 1.0));
    }

    pub fn batch_key(&self) -> DrawRectBatchKey {
        self.inner.batch_key()
    }

    pub fn snapshot_draw(&self) -> DrawRectDraw {
        self.inner.snapshot_draw()
    }
}

impl RenderTargetPass for DrawRectPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        DrawRectPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        DrawRectPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.scissor_rect = intersect_scissor_rects(self.scissor_rect, scissor_rect);
    }

    fn apply_stencil_clip(&mut self, clip_id: Option<u8>) {
        if !matches!(self.stencil_mode, RectStencilMode::Disabled) {
            return;
        }
        if let Some(clip_id) = clip_id {
            self.set_stencil_test(clip_id);
        }
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        DrawRectPass::set_color_target(self, color_target);
    }
}

impl RenderTargetPass for OpaqueRectPass {
    fn set_input(&mut self, input: RenderTargetIn) {
        OpaqueRectPass::set_input(self, input);
    }

    fn set_output(&mut self, output: RenderTargetOut) {
        OpaqueRectPass::set_output(self, output);
    }

    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.inner.scissor_rect = intersect_scissor_rects(self.inner.scissor_rect, scissor_rect);
    }

    fn apply_stencil_clip(&mut self, clip_id: Option<u8>) {
        self.inner.apply_stencil_clip(clip_id);
    }

    fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        OpaqueRectPass::set_color_target(self, color_target);
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
const RECT_UNIFORM_SLOT_SIZE: u64 = 256;
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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DrawRectBatchKey {
    color_target: Option<TextureHandle>,
    stencil_class: RectStencilClass,
    color_write_enabled: bool,
    render_mode: RectRenderMode,
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

impl RenderPass for DrawRectPass {
    type Input = DrawRectInput;
    type Output = DrawRectOutput;

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
        if let Some(handle) = self.input.render_target.handle() {
            let source: OutSlot<TextureResource, RenderTargetTag> = OutSlot::with_handle(handle);
            builder.read_texture(&mut self.input.render_target, &source);
        }
        if self.output.render_target.handle().is_some() {
            builder.write_texture(&mut self.output.render_target);
        }
    }

    fn execute(&mut self, ctx: &mut PassContext<'_, '_>) {
        execute_draw_rect_pass(self, ctx, RectShaderVariant::Alpha);
    }

    fn batchable(&self) -> bool {
        true
    }

    fn get_batch_key(&self) -> Option<u64> {
        let mut hasher = DefaultHasher::new();
        self.batch_key().hash(&mut hasher);
        Some(hasher.finish())
    }
}

impl RenderPass for OpaqueRectPass {
    type Input = OpaqueRectInput;
    type Output = OpaqueRectOutput;

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
        if let Some(handle) = self.input.render_target.handle() {
            let source: OutSlot<TextureResource, RenderTargetTag> = OutSlot::with_handle(handle);
            builder.read_texture(&mut self.input.render_target, &source);
        }
        if self.output.render_target.handle().is_some() {
            builder.write_texture(&mut self.output.render_target);
        }
    }

    fn execute(&mut self, ctx: &mut PassContext<'_, '_>) {
        execute_draw_rect_pass(&mut self.inner, ctx, RectShaderVariant::Opaque);
    }

    fn batchable(&self) -> bool {
        true
    }

    fn get_batch_key(&self) -> Option<u64> {
        let mut hasher = DefaultHasher::new();
        self.batch_key().hash(&mut hasher);
        Some(hasher.finish())
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

fn execute_draw_rect_pass(
    pass_def: &mut DrawRectPass,
    ctx: &mut PassContext<'_, '_>,
    variant: RectShaderVariant,
) {
    let draw = pass_def.snapshot_draw();
    execute_draw_rect_batch(&[draw], ctx, variant);
}

pub fn execute_draw_rect_pass_batch(draws: Vec<DrawRectDraw>, ctx: &mut PassContext<'_, '_>) {
    execute_draw_rect_batch(draws.as_slice(), ctx, RectShaderVariant::Alpha);
}

pub fn execute_opaque_rect_pass_batch(draws: Vec<DrawRectDraw>, ctx: &mut PassContext<'_, '_>) {
    execute_draw_rect_batch(draws.as_slice(), ctx, RectShaderVariant::Opaque);
}

fn execute_draw_rect_batch(
    draws: &[DrawRectDraw],
    ctx: &mut PassContext<'_, '_>,
    variant: RectShaderVariant,
) {
    if draws.is_empty() {
        return;
    }

    let first_draw = draws[0];
    let surface_size = ctx.viewport.surface_size();
    let lookup_started_at = Instant::now();
    let (offscreen_view, offscreen_msaa_view, target_w, target_h) = match first_draw.color_target {
        Some(handle) => {
            if let Some(bundle) = render_target_bundle(ctx, handle) {
                (
                    Some(bundle.view),
                    bundle.msaa_view,
                    bundle.size.0,
                    bundle.size.1,
                )
            } else {
                let fallback_view = render_target_view(ctx, handle);
                let fallback_msaa = render_target_msaa_view(ctx, handle);
                let (w, h) = render_target_size(ctx, handle).unwrap_or(surface_size);
                (fallback_view, fallback_msaa, w, h)
            }
        }
        None => (None, None, surface_size.0, surface_size.1),
    };
    ctx.record_detail_timing(
        "execute/draw_rect/resources/target_lookup",
        lookup_started_at.elapsed().as_secs_f64() * 1000.0,
    );

    let device_state_started_at = Instant::now();
    let scale = ctx.viewport.scale_factor();
    let device = match ctx.viewport.device() {
        Some(device) => device.clone(),
        None => return,
    };
    let queue = match ctx.viewport.queue() {
        Some(queue) => queue.clone(),
        None => return,
    };

    let format = ctx.viewport.surface_format();
    let sample_count = ctx.viewport.msaa_sample_count();
    let (stencil_class, _) = stencil_class_and_reference(first_draw.stencil_mode);
    let cache_key = rect_resource_cache_key(
        variant,
        stencil_class,
        first_draw.color_write_enabled,
        first_draw.render_mode,
    );
    ctx.record_detail_timing(
        "execute/draw_rect/resources/device_state",
        device_state_started_at.elapsed().as_secs_f64() * 1000.0,
    );

    let cache_started_at = Instant::now();
    let (
        pipeline,
        bind_group_layout,
        vertex_buffer,
        index_buffer,
        index_count,
        uniform_buffer,
        uniform_bind_group,
        uniform_dynamic_offset,
        uses_depth_stencil,
    ) = {
        let cache = draw_rect_resources_cache();
        let mut cache = cache.lock().unwrap();
        let resources = cache.get_or_insert_with(cache_key, || {
            create_draw_rect_resources(
                &device,
                format,
                sample_count,
                variant,
                stencil_class,
                first_draw.color_write_enabled,
                first_draw.render_mode,
            )
        });
        if resources.pipeline_format != format
            || resources.pipeline_sample_count != sample_count
            || resources.variant != variant
            || resources.stencil_class != stencil_class
            || resources.color_write_enabled != first_draw.color_write_enabled
            || resources.render_mode != first_draw.render_mode
        {
            *resources = create_draw_rect_resources(
                &device,
                format,
                sample_count,
                variant,
                stencil_class,
                first_draw.color_write_enabled,
                first_draw.render_mode,
            );
        }
        let mut ring_offsets = Vec::with_capacity(draws.len());
        for _ in draws {
            ring_offsets.push(resources.reserve_uniform_dynamic_offset());
        }
        (
            resources.pipeline.clone(),
            resources.bind_group_layout.clone(),
            resources.vertex_buffer.clone(),
            resources.index_buffer.clone(),
            resources.index_count,
            resources.uniform_buffer.clone(),
            resources.uniform_bind_group.clone(),
            ring_offsets,
            resources.uses_depth_stencil,
        )
    };
    ctx.record_detail_timing(
        "execute/draw_rect/resources/cache",
        cache_started_at.elapsed().as_secs_f64() * 1000.0,
    );
    ctx.record_detail_timing(
        "execute/draw_rect/resources",
        lookup_started_at.elapsed().as_secs_f64() * 1000.0,
    );

    let mut prepared_draws = Vec::with_capacity(draws.len());
    for draw in draws {
        if draw.color_target != first_draw.color_target
            || draw.color_write_enabled != first_draw.color_write_enabled
            || stencil_class_and_reference(draw.stencil_mode).0 != stencil_class
            || draw.render_mode != first_draw.render_mode
        {
            continue;
        }
        let params_started_at = Instant::now();
        let scaled_position = [draw.position[0] * scale, draw.position[1] * scale];
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
        ctx.record_detail_timing(
            "execute/draw_rect/params",
            params_started_at.elapsed().as_secs_f64() * 1000.0,
        );
        if params.outer_rect[2] <= params.outer_rect[0]
            || params.outer_rect[3] <= params.outer_rect[1]
        {
            continue;
        }
        let (_, stencil_reference) = stencil_class_and_reference(draw.stencil_mode);
        let scissor_rect_physical = draw.scissor_rect.and_then(|scissor_rect| {
            ctx.viewport
                .logical_scissor_to_physical(scissor_rect, (target_w, target_h))
        });
        prepared_draws.push((params, stencil_reference, scissor_rect_physical));
    }
    if prepared_draws.is_empty() {
        return;
    }

    let encode_started_at = Instant::now();
    let mut binding_total_ms = 0.0_f64;
    let mut fallback_total_ms = 0.0_f64;
    let mut ring_hit_count = 0_usize;
    {
        let msaa_enabled = ctx.viewport.msaa_sample_count() > 1;
        let parts = match ctx.viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
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
        let mut pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("DrawRect"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                    resolve_target,
                })],
                depth_stencil_attachment: if uses_depth_stencil {
                    parts.depth_stencil_attachment(wgpu::LoadOp::Load, wgpu::LoadOp::Load)
                } else {
                    None
                },
                ..Default::default()
            });

        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        for (idx, (params, stencil_reference, scissor_rect_physical)) in
            prepared_draws.iter().enumerate()
        {
            let binding_started_at = Instant::now();
            let fallback_started_at = Instant::now();
            let ring_offset = uniform_dynamic_offset[idx];
            let (bind_group, dynamic_offset) = match ring_offset {
                Some(offset) => {
                    queue.write_buffer(&uniform_buffer, offset as u64, bytemuck::bytes_of(params));
                    ring_hit_count = ring_hit_count.saturating_add(1);
                    (uniform_bind_group.clone(), offset)
                }
                None => {
                    let fallback_uniform_buffer =
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("DrawRect Params Buffer Fallback"),
                            contents: bytemuck::bytes_of(params),
                            usage: wgpu::BufferUsages::UNIFORM,
                        });
                    let fallback_bind_group =
                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("DrawRect Bind Group Fallback"),
                            layout: &bind_group_layout,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: fallback_uniform_buffer.as_entire_binding(),
                            }],
                        });
                    fallback_total_ms += fallback_started_at.elapsed().as_secs_f64() * 1000.0;
                    (fallback_bind_group, 0)
                }
            };
            binding_total_ms += binding_started_at.elapsed().as_secs_f64() * 1000.0;
            pass.set_bind_group(0, &bind_group, &[dynamic_offset]);
            if let Some(stencil_reference) = stencil_reference {
                pass.set_stencil_reference(*stencil_reference as u32);
            }
            if let Some([x, y, width, height]) = scissor_rect_physical {
                pass.set_scissor_rect(*x, *y, *width, *height);
            } else {
                pass.set_scissor_rect(0, 0, target_w, target_h);
            }
            pass.draw_indexed(0..index_count, 0, 0..1);
        }
    }
    for _ in 0..ring_hit_count {
        ctx.record_detail_count("execute/draw_rect/binding/ring_hit");
    }
    if fallback_total_ms > 0.0 {
        ctx.record_detail_timing("execute/draw_rect/binding/fallback", fallback_total_ms);
    }
    if binding_total_ms > 0.0 {
        ctx.record_detail_timing("execute/draw_rect/binding", binding_total_ms);
    }
    ctx.record_detail_timing(
        "execute/draw_rect/encode",
        encode_started_at.elapsed().as_secs_f64() * 1000.0,
    );
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct QuadVertex {
    uv: [f32; 2],
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
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    uniform_next_slot: u32,
    uniform_slot_count: u32,
    pipeline_format: wgpu::TextureFormat,
    pipeline_sample_count: u32,
    variant: RectShaderVariant,
    stencil_class: RectStencilClass,
    color_write_enabled: bool,
    render_mode: RectRenderMode,
    uses_depth_stencil: bool,
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
        depth_stencil: match (variant, stencil_class) {
            (RectShaderVariant::Alpha, RectStencilClass::None) => None,
            (RectShaderVariant::Opaque, RectStencilClass::None) => Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            (_, RectStencilClass::Test) => Some(wgpu::DepthStencilState {
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
            }),
            (_, RectStencilClass::Increment) => Some(wgpu::DepthStencilState {
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
            }),
            (_, RectStencilClass::Decrement) => Some(wgpu::DepthStencilState {
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
            }),
        },
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
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("DrawRect Uniform Ring Buffer"),
        size: RECT_UNIFORM_SLOT_SIZE * RECT_UNIFORM_SLOT_COUNT as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("DrawRect Uniform Ring Bind Group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &uniform_buffer,
                offset: 0,
                size: Some(NonZeroU64::new(RECT_UNIFORM_SLOT_SIZE).unwrap()),
            }),
        }],
    });

    DrawRectResources {
        pipeline,
        bind_group_layout,
        vertex_buffer,
        index_buffer,
        index_count: quad_indices.len() as u32,
        uniform_buffer,
        uniform_bind_group,
        uniform_next_slot: 0,
        uniform_slot_count: RECT_UNIFORM_SLOT_COUNT,
        pipeline_format: format,
        pipeline_sample_count: sample_count,
        variant,
        stencil_class,
        color_write_enabled,
        render_mode,
        uses_depth_stencil: !matches!(
            (variant, stencil_class),
            (RectShaderVariant::Alpha, RectStencilClass::None)
        ),
    }
}

impl DrawRectResources {
    fn reserve_uniform_dynamic_offset(&mut self) -> Option<u32> {
        if self.uniform_next_slot >= self.uniform_slot_count {
            return None;
        }
        let slot = self.uniform_next_slot;
        self.uniform_next_slot = self.uniform_next_slot.saturating_add(1);
        Some((slot as u64 * RECT_UNIFORM_SLOT_SIZE) as u32)
    }
}

struct DrawRectResourcesCache {
    entries: std::collections::HashMap<u64, DrawRectResources>,
}

impl DrawRectResourcesCache {
    fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    fn get_or_insert_with<F: FnOnce() -> DrawRectResources>(
        &mut self,
        key: u64,
        create: F,
    ) -> &mut DrawRectResources {
        self.entries.entry(key).or_insert_with(create)
    }

    fn begin_frame(&mut self) {
        for resources in self.entries.values_mut() {
            resources.uniform_next_slot = 0;
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

fn draw_rect_resources_cache() -> &'static Mutex<DrawRectResourcesCache> {
    static CACHE: OnceLock<Mutex<DrawRectResourcesCache>> = OnceLock::new();
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
}
