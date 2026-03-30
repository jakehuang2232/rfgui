impl Element {
    const SHOULD_RENDER_OVERSCAN_PX: f32 = 24.0;

    fn has_visible_background(&self) -> bool {
        self.computed_style.background_color.to_rgba_f32()[3] > 0.0
    }

    fn has_visible_border(
        &self,
        width_base: f32,
        height_base: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> bool {
        resolve_px(
            self.computed_style.border_widths.left,
            width_base,
            viewport_width,
            viewport_height,
        ) > 0.0
            || resolve_px(
                self.computed_style.border_widths.right,
                width_base,
                viewport_width,
                viewport_height,
            ) > 0.0
            || resolve_px(
                self.computed_style.border_widths.top,
                height_base,
                viewport_width,
                viewport_height,
            ) > 0.0
            || resolve_px(
                self.computed_style.border_widths.bottom,
                height_base,
                viewport_width,
                viewport_height,
            ) > 0.0
    }

    fn has_visible_self_paint(
        &self,
        width_base: f32,
        height_base: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> bool {
        self.has_visible_background()
            || self.has_visible_border(width_base, height_base, viewport_width, viewport_height)
            || !self.computed_style.box_shadow.is_empty()
    }

    fn transition_inner_rect(&self) -> Rect {
        let (frame_width, frame_height) = self.current_layout_transition_size();
        let max_bw = (frame_width.min(frame_height)) * 0.5;
        let border_left = self.border_widths.left.clamp(0.0, max_bw);
        let border_right = self.border_widths.right.clamp(0.0, max_bw);
        let border_top = self.border_widths.top.clamp(0.0, max_bw);
        let border_bottom = self.border_widths.bottom.clamp(0.0, max_bw);
        let inset_left = border_left + self.padding.left.max(0.0);
        let inset_right = border_right + self.padding.right.max(0.0);
        let inset_top = border_top + self.padding.top.max(0.0);
        let inset_bottom = border_bottom + self.padding.bottom.max(0.0);
        Rect {
            x: self.core.layout_position.x + inset_left,
            y: self.core.layout_position.y + inset_top,
            width: (frame_width - inset_left - inset_right).max(0.0),
            height: (frame_height - inset_top - inset_bottom).max(0.0),
        }
    }

    fn has_inner_render_area(&self) -> bool {
        let inner = self.transition_inner_rect();
        inner.width > 0.0 && inner.height > 0.0
    }

    fn frame_intersects_rect(frame: LayoutFrame, clip: Rect) -> bool {
        frame.width > 0.0
            && frame.height > 0.0
            && frame.x + frame.width > clip.x
            && frame.x < clip.x + clip.width
            && frame.y + frame.height > clip.y
            && frame.y < clip.y + clip.height
    }

    pub(crate) fn absolute_clip_scissor_rect(&self) -> Option<[u32; 4]> {
        if self.computed_style.position.mode() != PositionMode::Absolute {
            return None;
        }
        match self.computed_style.position.clip_mode() {
            ClipMode::Parent => None,
            ClipMode::Viewport => {
                if let Some(rect) = self.absolute_clip_rect {
                    rect_to_scissor_rect(rect)
                } else {
                    let (viewport_w, viewport_h) = self.viewport_size_from_runtime(
                        self.core.layout_size.width,
                        self.core.layout_size.height,
                    );
                    rect_to_scissor_rect(Rect {
                        x: 0.0,
                        y: 0.0,
                        width: viewport_w.max(0.0),
                        height: viewport_h.max(0.0),
                    })
                }
            }
            ClipMode::AnchorParent => self.absolute_clip_rect.and_then(rect_to_scissor_rect),
        }
    }

    fn inner_clip_rect(&self) -> Rect {
        self.transition_inner_rect()
    }

    fn inner_clip_scissor_rect(&self) -> Option<[u32; 4]> {
        rect_to_scissor_rect(self.inner_clip_rect())
    }

    fn inner_clip_radii(&self, outer_radii: CornerRadii) -> CornerRadii {
        let outer_x = self.core.layout_position.x;
        let outer_y = self.core.layout_position.y;
        let outer_w = self.core.layout_size.width.max(0.0);
        let outer_h = self.core.layout_size.height.max(0.0);
        let inner = self.inner_clip_rect();
        let inset_left = (inner.x - outer_x).max(0.0);
        let inset_top = (inner.y - outer_y).max(0.0);
        let inset_right = (outer_x + outer_w - (inner.x + inner.width)).max(0.0);
        let inset_bottom = (outer_y + outer_h - (inner.y + inner.height)).max(0.0);
        normalize_corner_radii(
            CornerRadii {
                top_left: (outer_radii.top_left - inset_left.max(inset_top)).max(0.0),
                top_right: (outer_radii.top_right - inset_right.max(inset_top)).max(0.0),
                bottom_right: (outer_radii.bottom_right - inset_right.max(inset_bottom)).max(0.0),
                bottom_left: (outer_radii.bottom_left - inset_left.max(inset_bottom)).max(0.0),
            },
            inner.width,
            inner.height,
        )
    }

    fn intersects_rect(a: Rect, b: Rect) -> bool {
        let a_right = a.x + a.width;
        let a_bottom = a.y + a.height;
        let b_right = b.x + b.width;
        let b_bottom = b.y + b.height;
        a.x < b_right && b.x < a_right && a.y < b_bottom && b.y < a_bottom
    }

    fn child_touches_inner_corner_clip(
        &self,
        child_rect: Rect,
        inner: Rect,
        inner_radii: CornerRadii,
    ) -> bool {
        let tl = inner_radii.top_left.max(0.0);
        if tl > 0.0
            && Self::intersects_rect(
                child_rect,
                Rect {
                    x: inner.x,
                    y: inner.y,
                    width: tl,
                    height: tl,
                },
            )
        {
            return true;
        }
        let tr = inner_radii.top_right.max(0.0);
        if tr > 0.0
            && Self::intersects_rect(
                child_rect,
                Rect {
                    x: inner.x + inner.width - tr,
                    y: inner.y,
                    width: tr,
                    height: tr,
                },
            )
        {
            return true;
        }
        let br = inner_radii.bottom_right.max(0.0);
        if br > 0.0
            && Self::intersects_rect(
                child_rect,
                Rect {
                    x: inner.x + inner.width - br,
                    y: inner.y + inner.height - br,
                    width: br,
                    height: br,
                },
            )
        {
            return true;
        }
        let bl = inner_radii.bottom_left.max(0.0);
        bl > 0.0
            && Self::intersects_rect(
                child_rect,
                Rect {
                    x: inner.x,
                    y: inner.y + inner.height - bl,
                    width: bl,
                    height: bl,
                },
            )
    }

    fn should_clip_children(
        &self,
        overflow_child_indices: &[bool],
        inner_radii: CornerRadii,
    ) -> bool {
        if self.children.is_empty()
            || !self.has_inner_render_area()
        {
            return false;
        }
        let (max_scroll_x, max_scroll_y) = self.max_scroll();
        if max_scroll_x > 0.0 || max_scroll_y > 0.0 {
            return true;
        }
        let inner = self.inner_clip_rect();
        for (idx, child) in self.children.iter().enumerate() {
            if overflow_child_indices.get(idx).copied().unwrap_or(false) {
                continue;
            }
            if child
                .as_any()
                .downcast_ref::<Element>()
                .is_some_and(Element::should_append_to_root_viewport_render)
            {
                continue;
            }
            let snapshot = child.box_model_snapshot();
            if !snapshot.should_render {
                continue;
            }
            let child_rect = Rect {
                x: snapshot.x,
                y: snapshot.y,
                width: snapshot.width.max(0.0),
                height: snapshot.height.max(0.0),
            };
            let child_right = child_rect.x + child_rect.width;
            let child_bottom = child_rect.y + child_rect.height;
            let inner_right = inner.x + inner.width;
            let inner_bottom = inner.y + inner.height;
            if child_rect.x < inner.x
                || child_rect.y < inner.y
                || child_right > inner_right
                || child_bottom > inner_bottom
            {
                return true;
            }
            if inner_radii.has_any_rounding()
                && self.child_touches_inner_corner_clip(child_rect, inner, inner_radii)
            {
                return true;
            }
        }
        false
    }

    fn begin_child_clip_scope(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        inner_radii: CornerRadii,
    ) -> Option<ChildClipScope> {
        let parent_clip_id = ctx.current_clip_id();
        let Some(child_clip_id) = ctx.push_clip_id() else {
            return None;
        };
        let previous_scissor = ctx.push_scissor_rect(self.inner_clip_scissor_rect());

        let inner = self.inner_clip_rect();
        let mut pass_params = RectPassParams {
            position: [inner.x, inner.y],
            size: [inner.width, inner.height],
            fill_color: [0.0, 0.0, 0.0, 0.0],
            opacity: 1.0,
            ..Default::default()
        };

        pass_params.set_border_width(0.0);
        pass_params.set_border_radii(inner_radii.to_array());

        let mut increment = DrawRectPass::new(
            pass_params,
            DrawRectInput {
                ..Default::default()
            },
            DrawRectOutput {
                ..Default::default()
            },
        );

        increment.set_stencil_increment(parent_clip_id);
        increment.set_color_write_enabled(false);
        self.push_stencil_pass(graph, ctx, increment);

        Some(ChildClipScope {
            previous_scissor,
            parent_clip_id,
            child_clip_id,
        })
    }

    fn end_child_clip_scope(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        scope: Option<ChildClipScope>,
    ) {
        let Some(scope) = scope else {
            return;
        };
        let inner_radii = self.inner_clip_radii(normalize_corner_radii(
            self.border_radii,
            self.core.layout_size.width.max(0.0),
            self.core.layout_size.height.max(0.0),
        ));
        let inner = self.inner_clip_rect();

        let mut decrement = DrawRectPass::new(
            RectPassParams {
                position: [inner.x, inner.y],
                size: [inner.width, inner.height],
                fill_color: [0.0, 0.0, 0.0, 0.0],
                opacity: 1.0,
                ..Default::default()
            },
            DrawRectInput::default(),
            DrawRectOutput::default(),
        );
        decrement.set_border_width(0.0);
        decrement.set_border_radii(inner_radii.to_array());
        decrement.set_stencil_decrement(scope.child_clip_id);
        decrement.set_color_write_enabled(false);
        self.push_stencil_pass(graph, ctx, decrement);

        ctx.pop_clip_id();
        ctx.restore_scissor_rect(scope.previous_scissor);
        debug_assert_eq!(ctx.current_clip_id(), scope.parent_clip_id);
    }

    fn current_layout_transition_size(&self) -> (f32, f32) {
        (
            self.layout_transition_override_width
                .unwrap_or(self.core.size.width)
                .max(0.0),
            self.layout_transition_override_height
                .unwrap_or(self.core.size.height)
                .max(0.0),
        )
    }

    fn current_layout_target_size(&self) -> (f32, f32) {
        (
            self.layout_assigned_width
                .unwrap_or(self.core.size.width)
                .max(0.0),
            self.layout_assigned_height
                .unwrap_or(self.core.size.height)
                .max(0.0),
        )
    }

    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new_with_id(0, x, y, width, height)
    }

    pub fn new_with_id(id: u64, x: f32, y: f32, width: f32, height: f32) -> Self {
        let mut style = Style::new();
        if width != 0.0 {
            style.insert(
                crate::style::PropertyId::Width,
                crate::style::ParsedValue::Length(Length::px(width)),
            );
        }
        if height != 0.0 {
            style.insert(
                crate::style::PropertyId::Height,
                crate::style::ParsedValue::Length(Length::px(height)),
            );
        }

        let mut el = Element {
            core: if id == 0 {
                ElementCore::new(x, y, width, height)
            } else {
                ElementCore::new_with_id(id, x, y, width, height)
            },
            anchor_name: None,
            layout_flow_position: Position { x, y },
            layout_inner_position: Position { x, y },
            layout_flow_inner_position: Position { x, y },
            layout_inner_size: Size {
                width: width.max(0.0),
                height: height.max(0.0),
            },
            intrinsic_size_is_percent_base: true,
            parsed_style: style,
            computed_style: ComputedStyle::default(),
            padding: EdgeInsets {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            },
            background_color: Box::new(Color::hex("#FFFFFF")),
            border_colors: EdgeColors {
                left: Box::new(Color::hex("#000000")),
                right: Box::new(Color::hex("#000000")),
                top: Box::new(Color::hex("#000000")),
                bottom: Box::new(Color::hex("#000000")),
            },
            border_widths: EdgeInsets {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            },
            border_radii: CornerRadii::zero(),
            border_radius: 0.0,
            box_shadows: Vec::new(),
            foreground_color: Color::rgb(0, 0, 0),
            opacity: 1.0,
            scroll_direction: ScrollDirection::None,
            scroll_offset: Position { x: 0.0, y: 0.0 },
            content_size: Size {
                width: 0.0,
                height: 0.0,
            },
            scrollbar_drag: None,
            last_scrollbar_interaction: None,
            scrollbar_shadow_blur_radius: 3.0,
            pending_style_transition_requests: Vec::new(),
            pending_layout_transition_requests: Vec::new(),
            pending_visual_transition_requests: Vec::new(),
            has_style_snapshot: false,
            has_layout_snapshot: false,
            layout_transition_visual_offset_x: 0.0,
            layout_transition_visual_offset_y: 0.0,
            layout_transition_override_width: None,
            layout_transition_override_height: None,
            layout_transition_target_x: None,
            layout_transition_target_y: None,
            layout_transition_target_width: None,
            layout_transition_target_height: None,
            last_parent_layout_x: x,
            last_parent_layout_y: y,
            layout_assigned_width: None,
            layout_assigned_height: None,
            is_hovered: false,
            mouse_down_handlers: Vec::new(),
            mouse_up_handlers: Vec::new(),
            mouse_move_handlers: Vec::new(),
            mouse_enter_handlers: Vec::new(),
            mouse_leave_handlers: Vec::new(),
            click_handlers: Vec::new(),
            key_down_handlers: Vec::new(),
            key_up_handlers: Vec::new(),
            focus_handlers: Vec::new(),
            blur_handlers: Vec::new(),
            layout_dirty: true,
            last_layout_proposal: None,
            flex_info: None,
            has_absolute_descendant_for_hit_test: false,
            absolute_clip_rect: None,
            anchor_parent_clip_rect: None,
            hit_test_clip_rect: None,
            children: Vec::new(),
        };
        el.recompute_style();
        // Initial mount should not animate from constructor defaults to first user style.
        el.has_style_snapshot = false;
        el
    }

    pub(crate) fn active_transition_channels(&self) -> Vec<ChannelId> {
        let mut channels = Vec::new();
        for transition in self.computed_style.transition.as_slice() {
            push_transition_channels(transition.property, &mut channels);
        }
        channels.sort_unstable_by_key(|channel| channel.0);
        channels.dedup();
        channels
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.core.set_position(x, y);
    }

    pub fn set_anchor_name(&mut self, name: Option<AnchorName>) {
        self.anchor_name = name;
    }

    pub fn set_x(&mut self, x: f32) {
        self.core.set_x(x);
    }

    pub fn set_y(&mut self, y: f32) {
        self.core.set_y(y);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.core.set_size(width, height);
        self.layout_dirty = true;
    }

    pub fn set_scrollbar_shadow_blur_radius(&mut self, radius: f32) {
        self.scrollbar_shadow_blur_radius = radius.max(0.0);
    }

    pub fn set_width(&mut self, width: f32) {
        self.core.set_width(width);
        self.layout_dirty = true;
    }

    pub fn set_height(&mut self, height: f32) {
        self.core.set_height(height);
        self.layout_dirty = true;
    }

    pub fn mark_layout_dirty(&mut self) {
        self.layout_dirty = true;
    }

    pub fn set_background_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.background_color = Box::new(color);
    }

    pub fn set_background_color_value(&mut self, color: Color) {
        self.background_color = Box::new(color);
    }

    pub fn set_foreground_color(&mut self, color: Color) {
        self.foreground_color = color;
    }

    pub fn set_layout_transition_x(&mut self, value: f32) {
        self.layout_transition_visual_offset_x = value;
    }

    pub fn set_layout_transition_y(&mut self, value: f32) {
        self.layout_transition_visual_offset_y = value;
    }

    pub fn set_layout_transition_width(&mut self, value: f32) {
        let value = value.max(0.0);
        self.layout_transition_override_width = Some(value);
        self.core.layout_size.width = value;
        self.layout_dirty = true;
    }

    pub fn set_layout_transition_height(&mut self, value: f32) {
        let value = value.max(0.0);
        self.layout_transition_override_height = Some(value);
        self.core.layout_size.height = value;
        self.layout_dirty = true;
    }

    pub fn seed_layout_transition_snapshot(
        &mut self,
        layout_x: f32,
        layout_y: f32,
        flow_x: f32,
        flow_y: f32,
        layout_width: f32,
        layout_height: f32,
        parent_layout_x: f32,
        parent_layout_y: f32,
    ) {
        self.core.layout_position = Position {
            x: layout_x,
            y: layout_y,
        };
        self.layout_flow_position = Position {
            x: flow_x,
            y: flow_y,
        };
        self.core.layout_size = Size {
            width: layout_width.max(0.0),
            height: layout_height.max(0.0),
        };
        self.last_parent_layout_x = parent_layout_x;
        self.last_parent_layout_y = parent_layout_y;
        self.has_layout_snapshot = true;
    }

    pub fn set_border_top_color(&mut self, color: Color) {
        self.border_colors.top = Box::new(color);
    }

    pub fn set_border_right_color(&mut self, color: Color) {
        self.border_colors.right = Box::new(color);
    }

    pub fn set_border_bottom_color(&mut self, color: Color) {
        self.border_colors.bottom = Box::new(color);
    }

    pub fn set_border_left_color(&mut self, color: Color) {
        self.border_colors.left = Box::new(color);
    }

    pub fn set_border_radius(&mut self, radius: f32) {
        let radius = radius.max(0.0);
        self.border_radii = CornerRadii::uniform(radius);
        self.border_radius = radius;
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }

    pub fn set_padding(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding = EdgeInsets {
            left: value,
            right: value,
            top: value,
            bottom: value,
        };
        self.layout_dirty = true;
    }

    pub fn set_padding_x(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding.left = value;
        self.padding.right = value;
        self.layout_dirty = true;
    }

    pub fn set_padding_y(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding.top = value;
        self.padding.bottom = value;
        self.layout_dirty = true;
    }

    pub fn set_padding_left(&mut self, value: f32) {
        self.padding.left = value.max(0.0);
        self.layout_dirty = true;
    }

    pub fn set_padding_right(&mut self, value: f32) {
        self.padding.right = value.max(0.0);
        self.layout_dirty = true;
    }

    pub fn set_padding_top(&mut self, value: f32) {
        self.padding.top = value.max(0.0);
        self.layout_dirty = true;
    }

    pub fn set_padding_bottom(&mut self, value: f32) {
        self.padding.bottom = value.max(0.0);
        self.layout_dirty = true;
    }

    pub(crate) fn width_is_auto(&self) -> bool {
        self.computed_style.width == SizeValue::Auto
    }

    pub(crate) fn height_is_auto(&self) -> bool {
        self.computed_style.height == SizeValue::Auto
    }

    pub(crate) fn inner_content_rect_for_render(&self) -> (f32, f32, f32, f32) {
        (
            self.layout_inner_position.x,
            self.layout_inner_position.y,
            self.layout_inner_size.width.max(0.0),
            self.layout_inner_size.height.max(0.0),
        )
    }

    pub fn apply_style(&mut self, style: Style) {
        self.parsed_style = self.parsed_style.clone() + style;
        self.recompute_style();
    }

    pub fn set_intrinsic_size_as_percent_base(&mut self, enabled: bool) {
        self.intrinsic_size_is_percent_base = enabled;
    }

    fn recompute_style(&mut self) {
        let prev_opacity = self.opacity;
        let prev_border_radius = self.border_radius;
        let prev_background_color = self.background_color.as_ref().to_rgba_u8();
        let prev_foreground_color = self.foreground_color;
        let prev_border_top_color = self.border_colors.top.as_ref().to_rgba_u8();
        let prev_border_right_color = self.border_colors.right.as_ref().to_rgba_u8();
        let prev_border_bottom_color = self.border_colors.bottom.as_ref().to_rgba_u8();
        let prev_border_left_color = self.border_colors.left.as_ref().to_rgba_u8();
        let had_snapshot = self.has_style_snapshot;
        let effective_style = if self.is_hovered {
            match self.parsed_style.hover() {
                Some(hover_style) => self.parsed_style.clone() + hover_style.clone(),
                None => self.parsed_style.clone(),
            }
        } else {
            self.parsed_style.clone()
        };
        self.computed_style = compute_style(&effective_style, None);
        self.sync_props_from_computed_style();
        if had_snapshot {
            self.collect_style_transition_requests(
                prev_opacity,
                prev_border_radius,
                Color::rgba(
                    prev_background_color[0],
                    prev_background_color[1],
                    prev_background_color[2],
                    prev_background_color[3],
                ),
                prev_foreground_color,
                Color::rgba(
                    prev_border_top_color[0],
                    prev_border_top_color[1],
                    prev_border_top_color[2],
                    prev_border_top_color[3],
                ),
                Color::rgba(
                    prev_border_right_color[0],
                    prev_border_right_color[1],
                    prev_border_right_color[2],
                    prev_border_right_color[3],
                ),
                Color::rgba(
                    prev_border_bottom_color[0],
                    prev_border_bottom_color[1],
                    prev_border_bottom_color[2],
                    prev_border_bottom_color[3],
                ),
                Color::rgba(
                    prev_border_left_color[0],
                    prev_border_left_color[1],
                    prev_border_left_color[2],
                    prev_border_left_color[3],
                ),
            );
        }
        self.has_style_snapshot = true;
        self.layout_dirty = true;
    }

    fn collect_style_transition_requests(
        &mut self,
        prev_opacity: f32,
        prev_border_radius: f32,
        prev_background_color: Color,
        prev_foreground_color: Color,
        prev_border_top_color: Color,
        prev_border_right_color: Color,
        prev_border_bottom_color: Color,
        prev_border_left_color: Color,
    ) {
        let next_opacity = self.opacity;
        let next_border_radius = self.border_radius;
        let [bg_r, bg_g, bg_b, bg_a] = self.background_color.as_ref().to_rgba_u8();
        let next_background_color = Color::rgba(bg_r, bg_g, bg_b, bg_a);
        let next_foreground_color = self.foreground_color;
        let [bt_r, bt_g, bt_b, bt_a] = self.border_colors.top.as_ref().to_rgba_u8();
        let [br_r, br_g, br_b, br_a] = self.border_colors.right.as_ref().to_rgba_u8();
        let [bb_r, bb_g, bb_b, bb_a] = self.border_colors.bottom.as_ref().to_rgba_u8();
        let [bl_r, bl_g, bl_b, bl_a] = self.border_colors.left.as_ref().to_rgba_u8();
        let next_border_top_color = Color::rgba(bt_r, bt_g, bt_b, bt_a);
        let next_border_right_color = Color::rgba(br_r, br_g, br_b, br_a);
        let next_border_bottom_color = Color::rgba(bb_r, bb_g, bb_b, bb_a);
        let next_border_left_color = Color::rgba(bl_r, bl_g, bl_b, bl_a);
        for transition in self.computed_style.transition.as_slice() {
            let runtime = RuntimeStyleTransition {
                duration_ms: transition.duration_ms,
                delay_ms: transition.delay_ms,
                timing: map_transition_timing(transition.timing),
            };
            match transition.property {
                TransitionProperty::All => {
                    if !approx_eq(prev_opacity, next_opacity) {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Opacity,
                                from: StyleValue::Scalar(prev_opacity),
                                to: StyleValue::Scalar(next_opacity),
                                transition: runtime,
                            });
                    }
                    if !approx_eq(prev_border_radius, next_border_radius) {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRadius,
                                from: StyleValue::Scalar(prev_border_radius),
                                to: StyleValue::Scalar(next_border_radius),
                                transition: runtime,
                            });
                    }
                    if prev_background_color != next_background_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BackgroundColor,
                                from: StyleValue::Color(prev_background_color),
                                to: StyleValue::Color(next_background_color),
                                transition: runtime,
                            });
                    }
                    if prev_foreground_color != next_foreground_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Color,
                                from: StyleValue::Color(prev_foreground_color),
                                to: StyleValue::Color(next_foreground_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_top_color != next_border_top_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderTopColor,
                                from: StyleValue::Color(prev_border_top_color),
                                to: StyleValue::Color(next_border_top_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_right_color != next_border_right_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRightColor,
                                from: StyleValue::Color(prev_border_right_color),
                                to: StyleValue::Color(next_border_right_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_bottom_color != next_border_bottom_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderBottomColor,
                                from: StyleValue::Color(prev_border_bottom_color),
                                to: StyleValue::Color(next_border_bottom_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_left_color != next_border_left_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderLeftColor,
                                from: StyleValue::Color(prev_border_left_color),
                                to: StyleValue::Color(next_border_left_color),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::Opacity => {
                    if !approx_eq(prev_opacity, next_opacity) {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Opacity,
                                from: StyleValue::Scalar(prev_opacity),
                                to: StyleValue::Scalar(next_opacity),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BorderRadius => {
                    if !approx_eq(prev_border_radius, next_border_radius) {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRadius,
                                from: StyleValue::Scalar(prev_border_radius),
                                to: StyleValue::Scalar(next_border_radius),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BackgroundColor => {
                    if prev_background_color != next_background_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BackgroundColor,
                                from: StyleValue::Color(prev_background_color),
                                to: StyleValue::Color(next_background_color),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::Color => {
                    if prev_foreground_color != next_foreground_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Color,
                                from: StyleValue::Color(prev_foreground_color),
                                to: StyleValue::Color(next_foreground_color),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BorderColor => {
                    if prev_border_top_color != next_border_top_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderTopColor,
                                from: StyleValue::Color(prev_border_top_color),
                                to: StyleValue::Color(next_border_top_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_right_color != next_border_right_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRightColor,
                                from: StyleValue::Color(prev_border_right_color),
                                to: StyleValue::Color(next_border_right_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_bottom_color != next_border_bottom_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderBottomColor,
                                from: StyleValue::Color(prev_border_bottom_color),
                                to: StyleValue::Color(next_border_bottom_color),
                                transition: runtime,
                            });
                    }
                    if prev_border_left_color != next_border_left_color {
                        self.pending_style_transition_requests
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderLeftColor,
                                from: StyleValue::Color(prev_border_left_color),
                                to: StyleValue::Color(next_border_left_color),
                                transition: runtime,
                            });
                    }
                }
                _ => {}
            }
        }
    }
}
