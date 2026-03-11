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

fn main_axis_start_and_gap(
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

fn cross_start_offset(limit: f32, occupied: f32, align: Align) -> f32 {
    let free = (limit - occupied).max(0.0);
    match align {
        Align::Start => 0.0,
        Align::Center => free * 0.5,
        Align::End => free,
    }
}

fn cross_item_offset(line_cross: f32, item_cross: f32, align: Align) -> f32 {
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

fn resolve_px(length: Length, base: f32, viewport_width: f32, viewport_height: f32) -> f32 {
    length
        .resolve_with_base(Some(base), viewport_width, viewport_height)
        .unwrap_or(0.0)
        .max(0.0)
}

fn resolve_px_with_base(
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
