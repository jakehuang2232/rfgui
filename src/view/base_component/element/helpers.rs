impl Default for Element {
    fn default() -> Self {
        // Use a large default root size so rsx root without explicit size is still visible.
        Self::new(0.0, 0.0, 10_000.0, 10_000.0)
    }
}

fn expand_corner_radii_for_spread(
    base_radii: CornerRadii,
    spread: f32,
    width: f32,
    height: f32,
) -> CornerRadii {
    normalize_corner_radii(
        CornerRadii {
            top_left: (base_radii.top_left + spread).max(0.0),
            top_right: (base_radii.top_right + spread).max(0.0),
            bottom_right: (base_radii.bottom_right + spread).max(0.0),
            bottom_left: (base_radii.bottom_left + spread).max(0.0),
        },
        width + spread * 2.0,
        height + spread * 2.0,
    )
}

fn normalize_corner_radii(mut radii: CornerRadii, width: f32, height: f32) -> CornerRadii {
    radii.top_left = radii.top_left.max(0.0);
    radii.top_right = radii.top_right.max(0.0);
    radii.bottom_right = radii.bottom_right.max(0.0);
    radii.bottom_left = radii.bottom_left.max(0.0);
    let w = width.max(0.0);
    let h = height.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return CornerRadii::zero();
    }

    let top = radii.top_left + radii.top_right;
    let bottom = radii.bottom_left + radii.bottom_right;
    let left = radii.top_left + radii.bottom_left;
    let right = radii.top_right + radii.bottom_right;

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
        radii.top_left *= scale;
        radii.top_right *= scale;
        radii.bottom_right *= scale;
        radii.bottom_left *= scale;
    }

    radii
}

fn rect_to_scissor_rect(rect: Rect) -> Option<[u32; 4]> {
    let left = rect.x.floor().max(0.0) as i64;
    let top = rect.y.floor().max(0.0) as i64;
    let right = (rect.x + rect.width).ceil().max(0.0) as i64;
    let bottom = (rect.y + rect.height).ceil().max(0.0) as i64;
    if right <= left || bottom <= top {
        return None;
    }
    Some([
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ])
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

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}

#[allow(clippy::too_many_arguments)]
fn apply_collision(
    collision: Collision,
    boundary: Rect,
    x: &mut f32,
    y: &mut f32,
    width: f32,
    height: f32,
    anchor: AnchorSnapshot,
    left: Option<f32>,
    right: Option<f32>,
    top: Option<f32>,
    bottom: Option<f32>,
) {
    if collision == Collision::None {
        return;
    }

    if matches!(collision, Collision::Flip | Collision::FlipFit) {
        if (*x < boundary.x || *x + width > boundary.x + boundary.width)
            && left.is_some()
            && right.is_none()
        {
            let l = left.unwrap_or(0.0);
            *x = anchor.x + anchor.width - l - width;
        } else if (*x < boundary.x || *x + width > boundary.x + boundary.width)
            && right.is_some()
            && left.is_none()
        {
            let r = right.unwrap_or(0.0);
            *x = anchor.x + r;
        }

        if (*y < boundary.y || *y + height > boundary.y + boundary.height)
            && top.is_some()
            && bottom.is_none()
        {
            let t = top.unwrap_or(0.0);
            *y = anchor.y + anchor.height - t - height;
        } else if (*y < boundary.y || *y + height > boundary.y + boundary.height)
            && bottom.is_some()
            && top.is_none()
        {
            let b = bottom.unwrap_or(0.0);
            *y = anchor.y + b;
        }
    }

    if matches!(collision, Collision::Fit | Collision::FlipFit) {
        let max_x = (boundary.x + boundary.width - width).max(boundary.x);
        let max_y = (boundary.y + boundary.height - height).max(boundary.y);
        *x = (*x).clamp(boundary.x, max_x);
        *y = (*y).clamp(boundary.y, max_y);
    }
}

pub(crate) fn main_axis_start_and_gap(
    main_limit: f32,
    occupied_main: f32,
    base_gap: f32,
    item_count: usize,
    justify: JustifyContent,
) -> (f32, f32) {
    let free = (main_limit - occupied_main).max(0.0);
    match justify {
        JustifyContent::Start => (0.0, base_gap),
        JustifyContent::Center => (free * 0.5, base_gap),
        JustifyContent::End => (free, base_gap),
        JustifyContent::SpaceBetween => {
            if item_count > 1 {
                (0.0, base_gap + free / ((item_count - 1) as f32))
            } else {
                (0.0, 0.0)
            }
        }
        JustifyContent::SpaceAround => {
            if item_count > 0 {
                let space = free / (item_count as f32);
                (space * 0.5, base_gap + space)
            } else {
                (0.0, base_gap)
            }
        }
        JustifyContent::SpaceEvenly => {
            if item_count > 0 {
                let space = free / ((item_count + 1) as f32);
                (space, base_gap + space)
            } else {
                (0.0, base_gap)
            }
        }
    }
}

pub(crate) fn cross_start_offset(limit: f32, occupied: f32, align: Align) -> f32 {
    let free = (limit - occupied).max(0.0);
    match align {
        Align::Start => 0.0,
        Align::Center => free * 0.5,
        Align::End => free,
    }
}

pub(crate) fn cross_item_offset(line_cross: f32, item_cross: f32, align: Align) -> f32 {
    let free = (line_cross - item_cross).max(0.0);
    match align {
        Align::Start => 0.0,
        Align::Center => free * 0.5,
        Align::End => free,
    }
}

fn trace_layout_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("RFGUI_TRACE_LAYOUT").is_ok())
}

fn trace_promoted_build_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("RFGUI_TRACE_PROMOTED_BUILD").is_ok())
}

pub(crate) fn trace_promoted_build(
    phase: &str,
    node_id: u64,
    parent_id: Option<u64>,
    extra: impl AsRef<str>,
) {
    #[cfg(test)]
    {
        TEST_PROMOTED_BUILD_COUNTS.with(|counts| {
            let mut counts = counts.borrow_mut();
            let key = (node_id, phase.to_string());
            *counts.entry(key).or_insert(0) += 1;
        });
    }
    if !trace_promoted_build_enabled() {
        return;
    }
    eprintln!(
        "[promoted-build] phase={phase} node={node_id} parent={parent_id:?} {}",
        extra.as_ref()
    );
}

#[cfg(test)]
thread_local! {
    static TEST_PROMOTED_BUILD_COUNTS: RefCell<FxHashMap<(u64, String), usize>> =
        RefCell::new(FxHashMap::default());
}

#[cfg(test)]
pub(crate) fn reset_test_promoted_build_counts() {
    TEST_PROMOTED_BUILD_COUNTS.with(|counts| counts.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn test_promoted_build_count(node_id: u64, phase: &str) -> usize {
    TEST_PROMOTED_BUILD_COUNTS.with(|counts| {
        counts
            .borrow()
            .get(&(node_id, phase.to_string()))
            .copied()
            .unwrap_or(0)
    })
}

pub(crate) fn resolve_px(length: Length, base: f32, viewport_width: f32, viewport_height: f32) -> f32 {
    length
        .resolve_with_base(Some(base), viewport_width, viewport_height)
        .unwrap_or(0.0)
        .max(0.0)
}

pub(crate) fn resolve_px_with_base(
    length: Length,
    base: Option<f32>,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    length
        .resolve_with_base(base, viewport_width, viewport_height)
        .map(|v| v.max(0.0))
}

fn resolve_signed_px_with_base(
    length: Length,
    base: Option<f32>,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    length.resolve_with_base(base, viewport_width, viewport_height)
}

fn resolve_px_or_zero(
    length: Length,
    base: Option<f32>,
    viewport_width: f32,
    viewport_height: f32,
) -> f32 {
    resolve_px_with_base(length, base, viewport_width, viewport_height).unwrap_or(0.0)
}

/// Resolved px insets (border + padding) per side. Used by axis-layout
/// shells to thread `inner_w` / `inner_h` / inset pairs into the layout
/// free functions without repeating the 8 resolve calls per call site.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedLayoutInsets {
    pub border_l: f32,
    pub border_r: f32,
    pub border_t: f32,
    pub border_b: f32,
    pub padding_l: f32,
    pub padding_r: f32,
    pub padding_t: f32,
    pub padding_b: f32,
}

impl ResolvedLayoutInsets {
    /// Sum of horizontal insets (left + right border + padding).
    #[inline]
    pub fn horizontal(&self) -> f32 {
        self.border_l + self.border_r + self.padding_l + self.padding_r
    }

    /// Sum of vertical insets (top + bottom border + padding).
    #[inline]
    pub fn vertical(&self) -> f32 {
        self.border_t + self.border_b + self.padding_t + self.padding_b
    }
}

/// Resolve the 4 border-width and 4 padding values against the given
/// proposal. Centralizes the otherwise-repeated `resolve_px_or_zero`
/// block at axis-layout shell entry points.
pub(crate) fn resolve_layout_insets(
    border_widths: &crate::style::EdgeInsets<Length>,
    padding: &crate::style::EdgeInsets<Length>,
    percent_base_width: Option<f32>,
    percent_base_height: Option<f32>,
    viewport_width: f32,
    viewport_height: f32,
) -> ResolvedLayoutInsets {
    let bw_l = resolve_px_or_zero(
        border_widths.left,
        percent_base_width,
        viewport_width,
        viewport_height,
    );
    let bw_r = resolve_px_or_zero(
        border_widths.right,
        percent_base_width,
        viewport_width,
        viewport_height,
    );
    let bw_t = resolve_px_or_zero(
        border_widths.top,
        percent_base_height,
        viewport_width,
        viewport_height,
    );
    let bw_b = resolve_px_or_zero(
        border_widths.bottom,
        percent_base_height,
        viewport_width,
        viewport_height,
    );
    let p_l = resolve_px_or_zero(
        padding.left,
        percent_base_width,
        viewport_width,
        viewport_height,
    );
    let p_r = resolve_px_or_zero(
        padding.right,
        percent_base_width,
        viewport_width,
        viewport_height,
    );
    let p_t = resolve_px_or_zero(
        padding.top,
        percent_base_height,
        viewport_width,
        viewport_height,
    );
    let p_b = resolve_px_or_zero(
        padding.bottom,
        percent_base_height,
        viewport_width,
        viewport_height,
    );
    ResolvedLayoutInsets {
        border_l: bw_l,
        border_r: bw_r,
        border_t: bw_t,
        border_b: bw_b,
        padding_l: p_l,
        padding_r: p_r,
        padding_t: p_t,
        padding_b: p_b,
    }
}

fn map_transition_timing(timing: TransitionTiming) -> TimeFunction {
    match timing {
        TransitionTiming::Linear => TimeFunction::Linear,
        TransitionTiming::EaseIn => TimeFunction::EaseIn,
        TransitionTiming::EaseOut => TimeFunction::EaseOut,
        TransitionTiming::EaseInOut => TimeFunction::EaseInOut,
    }
}

fn push_transition_channels(property: TransitionProperty, out: &mut Vec<ChannelId>) {
    match property {
        TransitionProperty::All => {
            out.extend([
                CHANNEL_VISUAL_X,
                CHANNEL_VISUAL_Y,
                CHANNEL_LAYOUT_WIDTH,
                CHANNEL_LAYOUT_HEIGHT,
                CHANNEL_STYLE_OPACITY,
                CHANNEL_STYLE_BORDER_RADIUS,
                CHANNEL_STYLE_BACKGROUND_COLOR,
                CHANNEL_STYLE_COLOR,
                CHANNEL_STYLE_BORDER_TOP_COLOR,
                CHANNEL_STYLE_BORDER_RIGHT_COLOR,
                CHANNEL_STYLE_BORDER_BOTTOM_COLOR,
                CHANNEL_STYLE_BORDER_LEFT_COLOR,
                CHANNEL_STYLE_BOX_SHADOW,
                CHANNEL_STYLE_TRANSFORM,
                CHANNEL_STYLE_TRANSFORM_ORIGIN,
            ]);
        }
        TransitionProperty::Position => {
            out.extend([CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y]);
        }
        TransitionProperty::PositionX | TransitionProperty::X => out.push(CHANNEL_VISUAL_X),
        TransitionProperty::PositionY | TransitionProperty::Y => out.push(CHANNEL_VISUAL_Y),
        TransitionProperty::Width => out.push(CHANNEL_LAYOUT_WIDTH),
        TransitionProperty::Height => out.push(CHANNEL_LAYOUT_HEIGHT),
        TransitionProperty::Opacity => out.push(CHANNEL_STYLE_OPACITY),
        TransitionProperty::BorderRadius => out.push(CHANNEL_STYLE_BORDER_RADIUS),
        TransitionProperty::BackgroundColor => out.push(CHANNEL_STYLE_BACKGROUND_COLOR),
        TransitionProperty::Color => out.push(CHANNEL_STYLE_COLOR),
        TransitionProperty::BoxShadow => out.push(CHANNEL_STYLE_BOX_SHADOW),
        TransitionProperty::Transform => out.push(CHANNEL_STYLE_TRANSFORM),
        TransitionProperty::TransformOrigin => out.push(CHANNEL_STYLE_TRANSFORM_ORIGIN),
        TransitionProperty::BorderColor => out.extend([
            CHANNEL_STYLE_BORDER_TOP_COLOR,
            CHANNEL_STYLE_BORDER_RIGHT_COLOR,
            CHANNEL_STYLE_BORDER_BOTTOM_COLOR,
            CHANNEL_STYLE_BORDER_LEFT_COLOR,
        ]),
        TransitionProperty::Gap | TransitionProperty::Padding | TransitionProperty::BorderWidth => {
        }
    }
}

fn resolve_position2d_component(
    length: Length,
    base: f32,
) -> f32 {
    length.resolve_with_base(Some(base), 0.0, 0.0).unwrap_or(base * 0.5)
}

fn linear_endpoints_for_side(side: crate::style::SideOrCorner, width: f32, height: f32) -> ([f32; 2], [f32; 2]) {
    use crate::style::SideOrCorner::*;
    let cx = width * 0.5;
    let cy = height * 0.5;
    match side {
        Top => ([cx, height], [cx, 0.0]),
        Bottom => ([cx, 0.0], [cx, height]),
        Left => ([width, cy], [0.0, cy]),
        Right => ([0.0, cy], [width, cy]),
        TopLeft => ([width, height], [0.0, 0.0]),
        TopRight => ([0.0, height], [width, 0.0]),
        BottomLeft => ([width, 0.0], [0.0, height]),
        BottomRight => ([0.0, 0.0], [width, height]),
    }
}

fn linear_endpoints_for_angle(angle_rad: f32, width: f32, height: f32) -> ([f32; 2], [f32; 2]) {
    // CSS angle: 0deg = to top, positive = clockwise.
    let cx = width * 0.5;
    let cy = height * 0.5;
    let sin_a = angle_rad.sin();
    let cos_a = angle_rad.cos();
    // Direction vector (CSS up = -y, clockwise from up).
    let dir_x = sin_a;
    let dir_y = -cos_a;
    // Gradient line length = |w*sin| + |h*cos|.
    let line_len = (width * sin_a).abs() + (height * cos_a).abs();
    let half = line_len * 0.5;
    let p1 = [cx + dir_x * half, cy + dir_y * half];
    let p0 = [cx - dir_x * half, cy - dir_y * half];
    (p0, p1)
}

fn radial_radii(
    shape: crate::style::RadialShape,
    size: crate::style::RadialSize,
    cx: f32,
    cy: f32,
    width: f32,
    height: f32,
) -> (f32, f32) {
    use crate::style::{RadialShape, RadialSize};
    let (rx, ry) = match size {
        RadialSize::ClosestSide => (cx.min(width - cx).max(0.0), cy.min(height - cy).max(0.0)),
        RadialSize::FarthestSide => (cx.max(width - cx).max(0.0), cy.max(height - cy).max(0.0)),
        RadialSize::ClosestCorner => {
            let dx = cx.min(width - cx).max(0.0);
            let dy = cy.min(height - cy).max(0.0);
            let d = (dx * dx + dy * dy).sqrt();
            (d, d)
        }
        RadialSize::FarthestCorner => {
            let dx = cx.max(width - cx).max(0.0);
            let dy = cy.max(height - cy).max(0.0);
            let d = (dx * dx + dy * dy).sqrt();
            (d, d)
        }
        RadialSize::Explicit { rx, ry } => (
            rx.resolve_with_base(Some(width), 0.0, 0.0).unwrap_or(0.0).max(0.0),
            ry.resolve_with_base(Some(height), 0.0, 0.0).unwrap_or(0.0).max(0.0),
        ),
    };
    match shape {
        RadialShape::Circle => {
            let r = rx.max(ry);
            (r, r)
        }
        RadialShape::Ellipse => (rx.max(1e-3), ry.max(1e-3)),
    }
}

fn resolve_gradient_paint(
    gradient: &crate::style::Gradient,
    width: f32,
    height: f32,
) -> crate::render_pass::draw_rect_pass::GradientPaint {
    use crate::render_pass::draw_rect_pass::{GradientKindGpu, GradientPaint, GradientStopGpu};
    use crate::style::{ColorLike, Gradient, GradientLine, Length as L};

    let mut paint = GradientPaint::default();
    let stops_in = gradient.stops();
    let n = stops_in.len();

    let axis_len = match gradient {
        Gradient::Linear { line, .. } => {
            let (p0, p1) = match line {
                GradientLine::Angle(a) => {
                    linear_endpoints_for_angle(a.to_radians(), width, height)
                }
                GradientLine::ToSide(side) => linear_endpoints_for_side(*side, width, height),
            };
            let dx = p1[0] - p0[0];
            let dy = p1[1] - p0[1];
            (dx * dx + dy * dy).sqrt().max(1.0)
        }
        _ => width.max(height).max(1.0),
    };

    let mut positions: Vec<Option<f32>> = stops_in
        .iter()
        .take(n)
        .map(|s| match s.position {
            Some(L::Percent(p)) => Some(p / 100.0),
            Some(L::Px(px)) => Some(px / axis_len),
            Some(L::Zero) => Some(0.0),
            Some(other) => other
                .resolve_with_base(Some(axis_len), 0.0, 0.0)
                .map(|v| v / axis_len),
            None => None,
        })
        .collect();

    if n > 0 {
        if positions[0].is_none() {
            positions[0] = Some(0.0);
        }
        if positions[n - 1].is_none() {
            positions[n - 1] = Some(1.0);
        }
        let mut i = 1;
        while i < n - 1 {
            if positions[i].is_none() {
                let start_i = i - 1;
                let start_v = positions[start_i].unwrap_or(0.0);
                let mut end_i = i;
                while end_i < n && positions[end_i].is_none() {
                    end_i += 1;
                }
                let end_v = positions[end_i].unwrap_or(1.0);
                let gap = (end_i - start_i) as f32;
                for k in (start_i + 1)..end_i {
                    let t = (k - start_i) as f32 / gap;
                    positions[k] = Some(start_v + (end_v - start_v) * t);
                }
                i = end_i;
            } else {
                i += 1;
            }
        }
        for k in 1..n {
            let prev = positions[k - 1].unwrap_or(0.0);
            let cur = positions[k].unwrap_or(prev);
            if cur < prev {
                positions[k] = Some(prev);
            }
        }
    }

    let stops_vec: Vec<GradientStopGpu> = stops_in
        .iter()
        .take(n)
        .enumerate()
        .map(|(k, stop)| {
            let color = stop.color.to_rgba_f32();
            let p = positions[k].unwrap_or(0.0);
            GradientStopGpu {
                color,
                pos: [p, 0.0, 0.0, 0.0],
            }
        })
        .collect();
    paint.stops = std::sync::Arc::from(stops_vec);
    paint.repeating = gradient.is_repeating();

    match gradient {
        Gradient::Linear { line, .. } => {
            paint.kind = GradientKindGpu::Linear;
            let (p0, p1) = match line {
                GradientLine::Angle(a) => {
                    linear_endpoints_for_angle(a.to_radians(), width, height)
                }
                GradientLine::ToSide(side) => linear_endpoints_for_side(*side, width, height),
            };
            paint.axis = [p0[0], p0[1], p1[0], p1[1]];
        }
        Gradient::Radial {
            shape,
            size,
            position,
            ..
        } => {
            paint.kind = GradientKindGpu::Radial;
            let cx = resolve_position2d_component(position.x, width);
            let cy = resolve_position2d_component(position.y, height);
            let (rx, ry) = radial_radii(*shape, *size, cx, cy, width, height);
            paint.axis = [cx, cy, rx, ry];
        }
        Gradient::Conic { from, position, .. } => {
            paint.kind = GradientKindGpu::Conic;
            let cx = resolve_position2d_component(position.x, width);
            let cy = resolve_position2d_component(position.y, height);
            paint.axis = [cx, cy, from.to_radians(), 0.0];
        }
    }
    paint
}
