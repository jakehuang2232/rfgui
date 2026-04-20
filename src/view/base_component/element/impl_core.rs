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

    fn inner_rect_for_frame_size(&self, frame_width: f32, frame_height: f32) -> Rect {
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

    fn transition_inner_rect(&self) -> Rect {
        let (frame_width, frame_height) = self.current_layout_transition_size();
        self.inner_rect_for_frame_size(frame_width, frame_height)
    }

    fn has_inner_render_area(&self) -> bool {
        let inner = self.transition_inner_rect();
        inner.width > 0.0 && inner.height > 0.0
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
        let (frame_width, frame_height) = self.current_clip_layout_size();
        self.inner_rect_for_frame_size(frame_width, frame_height)
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
        arena: &crate::view::node_arena::NodeArena,
    ) -> bool {
        if self.is_fragmentable_inline_element() && self.inline_paint_fragments.len() > 1 {
            return false;
        }
        if self.children.is_empty()
            || !self.has_inner_render_area()
        {
            return false;
        }
        // Force clip: (has children AND has border_radius) OR any child has active animator.
        // Covers glyph/AA bleed past rounded inner rrect, and keeps moving children inside.
        if inner_radii.has_any_rounding() {
            return true;
        }
        if self.children.iter().any(|child_key| {
            arena
                .get(*child_key)
                .map(|n| n.element.has_active_animator())
                .unwrap_or(false)
        }) {
            return true;
        }
        let (max_scroll_x, max_scroll_y) = self.max_scroll();
        if max_scroll_x > 0.0 || max_scroll_y > 0.0 {
            return true;
        }
        let inner = self.inner_clip_rect();
        for (idx, child_key) in self.children.iter().enumerate() {
            if overflow_child_indices.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let Some(child_node) = arena.get(*child_key) else {
                continue;
            };
            if child_node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .is_some_and(Element::should_append_to_root_viewport_render)
            {
                continue;
            }
            let snapshot = child_node.element.box_model_snapshot();
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

    fn current_clip_layout_size(&self) -> (f32, f32) {
        let has_active_layout_transition = self.layout_transition_override_width.is_some()
            || self.layout_transition_override_height.is_some();
        if has_active_layout_transition {
            self.current_layout_transition_size()
        } else {
            self.current_layout_target_size()
        }
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
            transform: Transform::default(),
            transform_origin: TransformOrigin::center(),
            resolved_transform: None,
            resolved_inverse_transform: None,
            foreground_color: Color::rgb(0, 0, 0),
            opacity: 1.0,
            scroll_direction: ScrollDirection::None,
            scroll_offset: Position { x: 0.0, y: 0.0 },
            content_size: Size {
                width: 0.0,
                height: 0.0,
            },
            pending_inline_measure_context: None,
            last_inline_measure_context: None,
            inline_paint_fragments: Vec::new(),
            scrollbar_drag: None,
            last_scrollbar_interaction: None,
            scrollbar_shadow_blur_radius: 3.0,
            transition_requests: None,
            last_started_animator: None,
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
            event_handlers: None,
            layout_dirty: true,
            dirty_flags: DirtyFlags::ALL,
            last_layout_placement: None,
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

    pub(crate) fn reconcile_transition_runtime_state(
        &mut self,
        active_channels: Option<&FxHashSet<ChannelId>>,
    ) -> bool {
        let has_channel = |channel| active_channels.is_some_and(|channels| channels.contains(&channel));
        let mut needs_layout = false;
        let mut needs_place = false;

        if !has_channel(CHANNEL_VISUAL_X)
            && (!approx_eq(self.layout_transition_visual_offset_x, 0.0)
                || self.layout_transition_target_x.is_some())
        {
            self.layout_transition_visual_offset_x = 0.0;
            self.layout_transition_target_x = None;
            needs_place = true;
        }
        if !has_channel(CHANNEL_VISUAL_Y)
            && (!approx_eq(self.layout_transition_visual_offset_y, 0.0)
                || self.layout_transition_target_y.is_some())
        {
            self.layout_transition_visual_offset_y = 0.0;
            self.layout_transition_target_y = None;
            needs_place = true;
        }
        if !has_channel(CHANNEL_LAYOUT_WIDTH)
            && (self.layout_transition_override_width.is_some()
                || self.layout_transition_target_width.is_some())
        {
            self.layout_transition_override_width = None;
            self.layout_transition_target_width = None;
            needs_layout = true;
        }
        if !has_channel(CHANNEL_LAYOUT_HEIGHT)
            && (self.layout_transition_override_height.is_some()
                || self.layout_transition_target_height.is_some())
        {
            self.layout_transition_override_height = None;
            self.layout_transition_target_height = None;
            needs_layout = true;
        }

        if needs_layout {
            self.mark_layout_dirty();
        } else if needs_place {
            self.mark_place_dirty();
        }

        needs_layout || needs_place
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.core.set_position(x, y);
        self.mark_place_dirty();
    }

    pub fn set_anchor_name(&mut self, name: Option<AnchorName>) {
        self.anchor_name = name;
        self.mark_place_dirty();
    }

    pub fn set_x(&mut self, x: f32) {
        self.core.set_x(x);
        self.mark_place_dirty();
    }

    pub fn set_y(&mut self, y: f32) {
        self.core.set_y(y);
        self.mark_place_dirty();
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.core.set_size(width, height);
        self.mark_layout_dirty();
    }

    pub fn set_scrollbar_shadow_blur_radius(&mut self, radius: f32) {
        self.scrollbar_shadow_blur_radius = radius.max(0.0);
    }

    pub fn set_width(&mut self, width: f32) {
        self.core.set_width(width);
        self.mark_layout_dirty();
    }

    pub fn set_height(&mut self, height: f32) {
        self.core.set_height(height);
        self.mark_layout_dirty();
    }

    pub fn mark_layout_dirty(&mut self) {
        self.layout_dirty = true;
        self.mark_local_dirty(DirtyFlags::ALL);
    }

    pub(crate) fn mark_place_dirty(&mut self) {
        self.mark_local_dirty(DirtyFlags::PLACE.union(DirtyFlags::BOX_MODEL).union(DirtyFlags::HIT_TEST).union(DirtyFlags::PAINT));
    }

    pub(crate) fn mark_paint_dirty(&mut self) {
        self.mark_local_dirty(DirtyFlags::PAINT);
    }

    pub(crate) fn mark_local_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.union(flags);
    }

    pub fn set_background_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.background_color = Box::new(color);
        self.mark_paint_dirty();
    }

    pub fn set_background_color_value(&mut self, color: Color) {
        self.background_color = Box::new(color);
        self.mark_paint_dirty();
    }

    pub fn set_foreground_color(&mut self, color: Color) {
        self.foreground_color = color;
        self.mark_paint_dirty();
    }

    pub fn set_box_shadows(&mut self, box_shadows: Vec<BoxShadow>) {
        self.box_shadows = box_shadows;
        self.mark_paint_dirty();
    }

    pub fn set_transform_value(&mut self, transform: Transform) {
        self.transform = transform;
        self.update_resolved_transform();
        self.mark_place_dirty();
    }

    pub fn set_transform_progress_value(&mut self, from: Transform, to: Transform, progress: f32) {
        self.transform = interpolate_transform_with_reference_box(
            &from,
            &to,
            progress,
            glam::Vec2::new(
                self.core.layout_size.width.max(0.0),
                self.core.layout_size.height.max(0.0),
            ),
        );
        self.update_resolved_transform();
        self.mark_place_dirty();
    }

    pub fn set_transform_origin_value(&mut self, transform_origin: TransformOrigin) {
        self.transform_origin = transform_origin;
        self.update_resolved_transform();
        self.mark_place_dirty();
    }

    pub fn set_transform_origin_progress_value(
        &mut self,
        from: TransformOrigin,
        to: TransformOrigin,
        progress: f32,
    ) {
        self.transform_origin = crate::interpolate_transform_origin_with_reference_box(
            from,
            to,
            progress,
            glam::Vec2::new(
                self.core.layout_size.width.max(0.0),
                self.core.layout_size.height.max(0.0),
            ),
        );
        self.update_resolved_transform();
        self.mark_place_dirty();
    }

    pub fn set_layout_transition_x(&mut self, value: f32) {
        self.layout_transition_visual_offset_x = value;
        self.mark_place_dirty();
    }

    pub fn set_layout_transition_y(&mut self, value: f32) {
        self.layout_transition_visual_offset_y = value;
        self.mark_place_dirty();
    }

    pub fn set_layout_transition_width(&mut self, value: f32) {
        let value = round_layout_value(value.max(0.0));
        self.layout_transition_override_width = Some(value);
        self.core.layout_size.width = value;
        self.mark_layout_dirty();
    }

    pub fn set_layout_transition_height(&mut self, value: f32) {
        let value = round_layout_value(value.max(0.0));
        self.layout_transition_override_height = Some(value);
        self.core.layout_size.height = value;
        self.mark_layout_dirty();
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
        self.core.layout_position = round_layout_position(layout_x, layout_y);
        self.layout_flow_position = round_layout_position(flow_x, flow_y);
        self.core.layout_size = round_layout_size(layout_width, layout_height);
        self.update_resolved_transform();
        self.last_parent_layout_x = parent_layout_x;
        self.last_parent_layout_y = parent_layout_y;
        self.has_layout_snapshot = true;
    }

    pub(crate) fn can_seed_layout_transition_snapshot(&self) -> bool {
        self.has_layout_snapshot && self.last_layout_placement.is_some()
    }

    pub fn set_border_top_color(&mut self, color: Color) {
        self.border_colors.top = Box::new(color);
        self.mark_paint_dirty();
    }

    pub fn set_border_right_color(&mut self, color: Color) {
        self.border_colors.right = Box::new(color);
        self.mark_paint_dirty();
    }

    pub fn set_border_bottom_color(&mut self, color: Color) {
        self.border_colors.bottom = Box::new(color);
        self.mark_paint_dirty();
    }

    pub fn set_border_left_color(&mut self, color: Color) {
        self.border_colors.left = Box::new(color);
        self.mark_paint_dirty();
    }

    pub fn set_border_radius(&mut self, radius: f32) {
        let radius = radius.max(0.0);
        self.border_radii = CornerRadii::uniform(radius);
        self.border_radius = radius;
        self.mark_paint_dirty();
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
        self.mark_paint_dirty();
    }

    pub fn set_padding(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding = EdgeInsets {
            left: value,
            right: value,
            top: value,
            bottom: value,
        };
        self.mark_layout_dirty();
    }

    pub fn set_padding_x(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding.left = value;
        self.padding.right = value;
        self.mark_layout_dirty();
    }

    pub fn set_padding_y(&mut self, value: f32) {
        let value = value.max(0.0);
        self.padding.top = value;
        self.padding.bottom = value;
        self.mark_layout_dirty();
    }

    pub fn set_padding_left(&mut self, value: f32) {
        self.padding.left = value.max(0.0);
        self.mark_layout_dirty();
    }

    pub fn set_padding_right(&mut self, value: f32) {
        self.padding.right = value.max(0.0);
        self.mark_layout_dirty();
    }

    pub fn set_padding_top(&mut self, value: f32) {
        self.padding.top = value.max(0.0);
        self.mark_layout_dirty();
    }

    pub fn set_padding_bottom(&mut self, value: f32) {
        self.padding.bottom = value.max(0.0);
        self.mark_layout_dirty();
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
        let base = std::mem::take(&mut self.parsed_style);
        self.parsed_style = base + style;
        self.recompute_style();
    }

    pub fn set_intrinsic_size_as_percent_base(&mut self, enabled: bool) {
        self.intrinsic_size_is_percent_base = enabled;
    }

    fn recompute_style(&mut self) {
        let previous_snapshot = self.has_style_snapshot.then(|| self.capture_style_snapshot());
        let old_computed = self.computed_style.clone();
        let merged_hover;
        let effective_style = if self.is_hovered {
            if let Some(hover_style) = self.parsed_style.hover() {
                merged_hover = self.parsed_style.clone() + hover_style.clone();
                &merged_hover
            } else {
                &self.parsed_style
            }
        } else {
            &self.parsed_style
        };
        self.computed_style = compute_style(effective_style, None);
        if let Some(previous_snapshot) = previous_snapshot.as_ref() {
            self.collect_style_transition_requests(&previous_snapshot);
        }
        self.sync_props_from_computed_style();
        if let Some(previous_snapshot) = previous_snapshot.as_ref() {
            self.preserve_transform_transition_baseline(previous_snapshot);
        }
        self.sync_animator_requests();
        self.has_style_snapshot = true;
        if !old_computed.layout_eq(&self.computed_style) {
            self.mark_layout_dirty();
        } else if old_computed != self.computed_style {
            self.mark_place_dirty();
        }
    }

    fn sync_animator_requests(&mut self) {
        let next_animator = self.computed_style.animator.clone();
        if self.last_started_animator == next_animator {
            return;
        }
        let animator = next_animator
            .clone()
            .unwrap_or_else(|| crate::Animator::from_vec(Vec::new()));
        self.transition_requests.get_or_insert_with(Default::default).animation
            .push(crate::transition::AnimationRequest {
                target: self.core.id,
                animator,
            });
        self.last_started_animator = next_animator;
    }

    fn preserve_transform_transition_baseline(&mut self, previous: &ElementStyleSnapshot) {
        let preserve_transform = self
            .transition_requests
            .as_ref()
            .is_some_and(|r| r.style.iter().any(|req| req.field == StyleField::Transform));
        let preserve_transform_origin = self
            .transition_requests
            .as_ref()
            .is_some_and(|r| r.style.iter().any(|req| req.field == StyleField::TransformOrigin));

        if preserve_transform {
            self.transform = previous.transform.clone();
        }
        if preserve_transform_origin {
            self.transform_origin = previous.transform_origin;
        }
        if preserve_transform || preserve_transform_origin {
            self.update_resolved_transform();
        }
    }

    fn collect_style_transition_requests(&mut self, previous: &ElementStyleSnapshot) {
        let changed_fields = previous.diff(&self.computed_style);
        for transition in self.computed_style.transition.as_slice() {
            let runtime = RuntimeStyleTransition {
                duration_ms: transition.duration_ms,
                delay_ms: transition.delay_ms,
                timing: map_transition_timing(transition.timing),
            };
            match transition.property {
                TransitionProperty::All => {
                    for field in &changed_fields {
                        self.transition_requests.get_or_insert_with(Default::default).style.push(StyleTrackRequest {
                            target: self.core.id,
                            field: *field,
                            from: previous.value_for(*field),
                            to: previous.current_value_for(&self.computed_style, *field),
                            transition: runtime,
                        });
                    }
                }
                TransitionProperty::Opacity => {
                    if changed_fields.contains(&StyleField::Opacity) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Opacity,
                                from: previous.value_for(StyleField::Opacity),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::Opacity,
                                ),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BorderRadius => {
                    if changed_fields.contains(&StyleField::BorderRadius) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRadius,
                                from: previous.value_for(StyleField::BorderRadius),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::BorderRadius,
                                ),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BackgroundColor => {
                    if changed_fields.contains(&StyleField::BackgroundColor) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BackgroundColor,
                                from: previous.value_for(StyleField::BackgroundColor),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::BackgroundColor,
                                ),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::Color => {
                    if changed_fields.contains(&StyleField::Color) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Color,
                                from: previous.value_for(StyleField::Color),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::Color,
                                ),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BoxShadow => {
                    if changed_fields.contains(&StyleField::BoxShadow) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BoxShadow,
                                from: previous.value_for(StyleField::BoxShadow),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::BoxShadow,
                                ),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::Transform => {
                    if changed_fields.contains(&StyleField::Transform) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::Transform,
                                from: previous.value_for(StyleField::Transform),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::Transform,
                                ),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::TransformOrigin => {
                    if changed_fields.contains(&StyleField::TransformOrigin) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::TransformOrigin,
                                from: previous.value_for(StyleField::TransformOrigin),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::TransformOrigin,
                                ),
                                transition: runtime,
                            });
                    }
                }
                TransitionProperty::BorderColor => {
                    if changed_fields.contains(&StyleField::BorderTopColor) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderTopColor,
                                from: previous.value_for(StyleField::BorderTopColor),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::BorderTopColor,
                                ),
                                transition: runtime,
                            });
                    }
                    if changed_fields.contains(&StyleField::BorderRightColor) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderRightColor,
                                from: previous.value_for(StyleField::BorderRightColor),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::BorderRightColor,
                                ),
                                transition: runtime,
                            });
                    }
                    if changed_fields.contains(&StyleField::BorderBottomColor) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderBottomColor,
                                from: previous.value_for(StyleField::BorderBottomColor),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::BorderBottomColor,
                                ),
                                transition: runtime,
                            });
                    }
                    if changed_fields.contains(&StyleField::BorderLeftColor) {
                        self.transition_requests.get_or_insert_with(Default::default).style
                            .push(StyleTrackRequest {
                                target: self.core.id,
                                field: StyleField::BorderLeftColor,
                                from: previous.value_for(StyleField::BorderLeftColor),
                                to: previous.current_value_for(
                                    &self.computed_style,
                                    StyleField::BorderLeftColor,
                                ),
                                transition: runtime,
                            });
                    }
                }
                _ => {}
            }
        }
    }
}
