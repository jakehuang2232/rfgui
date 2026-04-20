impl Element {
    fn note_scrollbar_interaction(&mut self) {
        self.last_scrollbar_interaction = Some(Instant::now());
    }

    fn max_scroll(&self) -> (f32, f32) {
        (
            (self.content_size.width - self.layout_inner_size.width).max(0.0),
            (self.content_size.height - self.layout_inner_size.height).max(0.0),
        )
    }

    fn local_inner_origin(&self) -> (f32, f32) {
        (
            self.layout_inner_position.x - self.core.layout_position.x,
            self.layout_inner_position.y - self.core.layout_position.y,
        )
    }

    fn scrollbar_visibility_alpha(&self) -> f32 {
        const HOLD: Duration = Duration::from_millis(900);
        const FADE: Duration = Duration::from_millis(350);
        if self.scrollbar_drag.is_some() {
            return 1.0;
        }
        let (max_x, max_y) = self.max_scroll();
        if max_x <= 0.0 && max_y <= 0.0 {
            return 0.0;
        }
        if self.is_hovered {
            return 1.0;
        }
        let Some(last) = self.last_scrollbar_interaction else {
            return 0.0;
        };
        let elapsed = last.elapsed();
        if elapsed <= HOLD {
            return 1.0;
        }
        let fade_elapsed = elapsed - HOLD;
        if fade_elapsed >= FADE {
            return 0.0;
        }
        1.0 - (fade_elapsed.as_secs_f32() / FADE.as_secs_f32())
    }

    fn scrollbar_geometry(&self, inner_x: f32, inner_y: f32) -> ScrollbarGeometry {
        const THICKNESS: f32 = 6.0;
        const MARGIN: f32 = 3.0;
        const MIN_THUMB: f32 = 24.0;

        let mut geometry = ScrollbarGeometry::default();
        let (max_scroll_x, max_scroll_y) = self.max_scroll();
        let can_scroll_x = matches!(
            self.scroll_direction,
            ScrollDirection::Horizontal | ScrollDirection::Both
        ) && max_scroll_x > 0.0;
        let can_scroll_y = matches!(
            self.scroll_direction,
            ScrollDirection::Vertical | ScrollDirection::Both
        ) && max_scroll_y > 0.0;

        let reserve_v = if can_scroll_y {
            THICKNESS + MARGIN
        } else {
            0.0
        };
        let reserve_h = if can_scroll_x {
            THICKNESS + MARGIN
        } else {
            0.0
        };

        if can_scroll_y {
            let track_x = inner_x + self.layout_inner_size.width - THICKNESS - MARGIN;
            let track_y = inner_y + MARGIN;
            let track_h = (self.layout_inner_size.height - MARGIN * 2.0 - reserve_h).max(0.0);
            if track_h > 0.0 {
                let track = Rect {
                    x: track_x,
                    y: track_y,
                    width: THICKNESS,
                    height: track_h,
                };
                let ratio = (self.layout_inner_size.height / self.content_size.height.max(1.0))
                    .clamp(0.0, 1.0);
                let thumb_h = (track_h * ratio).clamp(MIN_THUMB.min(track_h), track_h);
                let travel = (track_h - thumb_h).max(0.0);
                let thumb_offset = if max_scroll_y > 0.0 {
                    (self.scroll_offset.y / max_scroll_y).clamp(0.0, 1.0) * travel
                } else {
                    0.0
                };
                geometry.vertical_track = Some(track);
                geometry.vertical_thumb = Some(Rect {
                    x: track.x,
                    y: track.y + thumb_offset,
                    width: track.width,
                    height: thumb_h,
                });
            }
        }

        if can_scroll_x {
            let track_x = inner_x + MARGIN;
            let track_y = inner_y + self.layout_inner_size.height - THICKNESS - MARGIN;
            let track_w = (self.layout_inner_size.width - MARGIN * 2.0 - reserve_v).max(0.0);
            if track_w > 0.0 {
                let track = Rect {
                    x: track_x,
                    y: track_y,
                    width: track_w,
                    height: THICKNESS,
                };
                let ratio = (self.layout_inner_size.width / self.content_size.width.max(1.0))
                    .clamp(0.0, 1.0);
                let thumb_w = (track_w * ratio).clamp(MIN_THUMB.min(track_w), track_w);
                let travel = (track_w - thumb_w).max(0.0);
                let thumb_offset = if max_scroll_x > 0.0 {
                    (self.scroll_offset.x / max_scroll_x).clamp(0.0, 1.0) * travel
                } else {
                    0.0
                };
                geometry.horizontal_track = Some(track);
                geometry.horizontal_thumb = Some(Rect {
                    x: track.x + thumb_offset,
                    y: track.y,
                    width: thumb_w,
                    height: track.height,
                });
            }
        }

        geometry
    }

    fn update_scroll_from_drag(
        &mut self,
        axis: ScrollbarAxis,
        mouse_local_x: f32,
        mouse_local_y: f32,
        grab_offset: f32,
    ) -> bool {
        let Some(next_scroll) =
            self.scroll_value_from_drag(axis, mouse_local_x, mouse_local_y, grab_offset)
        else {
            return false;
        };
        let current_scroll = match axis {
            ScrollbarAxis::Vertical => self.scroll_offset.y,
            ScrollbarAxis::Horizontal => self.scroll_offset.x,
        };
        let changed = !approx_eq(next_scroll, current_scroll);
        match axis {
            ScrollbarAxis::Vertical => self.scroll_offset.y = next_scroll,
            ScrollbarAxis::Horizontal => self.scroll_offset.x = next_scroll,
        }
        if changed {
            self.mark_place_dirty();
        }
        changed
    }

    fn scroll_value_from_drag(
        &self,
        axis: ScrollbarAxis,
        mouse_local_x: f32,
        mouse_local_y: f32,
        grab_offset: f32,
    ) -> Option<f32> {
        let (inner_x, inner_y) = self.local_inner_origin();
        let geometry = self.scrollbar_geometry(inner_x, inner_y);
        let (track, thumb) = match axis {
            ScrollbarAxis::Vertical => (geometry.vertical_track, geometry.vertical_thumb),
            ScrollbarAxis::Horizontal => (geometry.horizontal_track, geometry.horizontal_thumb),
        };
        let (Some(track), Some(thumb)) = (track, thumb) else {
            return None;
        };
        let (mouse_axis, track_axis, track_len, thumb_len, max_scroll) = match axis {
            ScrollbarAxis::Vertical => (
                mouse_local_y,
                track.y,
                track.height,
                thumb.height,
                self.max_scroll().1,
            ),
            ScrollbarAxis::Horizontal => (
                mouse_local_x,
                track.x,
                track.width,
                thumb.width,
                self.max_scroll().0,
            ),
        };
        if track_len <= 0.0 || max_scroll <= 0.0 {
            return None;
        }
        let travel = (track_len - thumb_len).max(0.0);
        if travel <= 0.0 {
            return None;
        }
        let thumb_start = (mouse_axis - grab_offset).clamp(track_axis, track_axis + travel);
        let ratio = ((thumb_start - track_axis) / travel).clamp(0.0, 1.0);
        Some(ratio * max_scroll)
    }

    fn is_scrollbar_hit(&self, local_x: f32, local_y: f32) -> bool {
        if self.scrollbar_visibility_alpha() <= 0.0 {
            return false;
        }
        let (inner_x, inner_y) = self.local_inner_origin();
        let geometry = self.scrollbar_geometry(inner_x, inner_y);
        geometry
            .vertical_track
            .is_some_and(|track| track.contains(local_x, local_y))
            || geometry
                .vertical_thumb
                .is_some_and(|thumb| thumb.contains(local_x, local_y))
            || geometry
                .horizontal_track
                .is_some_and(|track| track.contains(local_x, local_y))
            || geometry
                .horizontal_thumb
                .is_some_and(|thumb| thumb.contains(local_x, local_y))
    }

    fn handle_scrollbar_pointer_down(
        &mut self,
        event: &PointerDownEvent,
        control: &mut ViewportControl<'_>,
    ) -> bool {
        if event.pointer.button != Some(UiPointerButton::Left) {
            return false;
        }
        if !self.is_scrollbar_hit(event.pointer.local_x, event.pointer.local_y) {
            return false;
        }
        let local_x = event.pointer.local_x;
        let local_y = event.pointer.local_y;
        let (inner_x, inner_y) = self.local_inner_origin();
        let geometry = self.scrollbar_geometry(inner_x, inner_y);

        if let Some(thumb) = geometry.vertical_thumb {
            if thumb.contains(local_x, local_y) {
                control.cancel_scroll_track(self.core.id, ScrollAxis::Y);
                self.scrollbar_drag = Some(ScrollbarDragState {
                    axis: ScrollbarAxis::Vertical,
                    grab_offset: local_y - thumb.y,
                    reanchor_on_first_move: false,
                });
                control.set_pointer_capture(self.core.id);
                self.note_scrollbar_interaction();
                return true;
            }
        }
        if let Some(track) = geometry.vertical_track {
            if track.contains(local_x, local_y) {
                let grab = geometry
                    .vertical_thumb
                    .map(|thumb| thumb.height * 0.5)
                    .unwrap_or(0.0);
                if let Some(to) =
                    self.scroll_value_from_drag(ScrollbarAxis::Vertical, local_x, local_y, grab)
                {
                    let from = self.scroll_offset.y;
                    let _ = control.start_scroll_track(self.core.id, ScrollAxis::Y, from, to);
                }
                self.scrollbar_drag = Some(ScrollbarDragState {
                    axis: ScrollbarAxis::Vertical,
                    grab_offset: grab,
                    reanchor_on_first_move: true,
                });
                control.set_pointer_capture(self.core.id);
                self.note_scrollbar_interaction();
                return true;
            }
        }

        if let Some(thumb) = geometry.horizontal_thumb {
            if thumb.contains(local_x, local_y) {
                control.cancel_scroll_track(self.core.id, ScrollAxis::X);
                self.scrollbar_drag = Some(ScrollbarDragState {
                    axis: ScrollbarAxis::Horizontal,
                    grab_offset: local_x - thumb.x,
                    reanchor_on_first_move: false,
                });
                control.set_pointer_capture(self.core.id);
                self.note_scrollbar_interaction();
                return true;
            }
        }
        if let Some(track) = geometry.horizontal_track {
            if track.contains(local_x, local_y) {
                let grab = geometry
                    .horizontal_thumb
                    .map(|thumb| thumb.width * 0.5)
                    .unwrap_or(0.0);
                if let Some(to) =
                    self.scroll_value_from_drag(ScrollbarAxis::Horizontal, local_x, local_y, grab)
                {
                    let from = self.scroll_offset.x;
                    let _ = control.start_scroll_track(self.core.id, ScrollAxis::X, from, to);
                }
                self.scrollbar_drag = Some(ScrollbarDragState {
                    axis: ScrollbarAxis::Horizontal,
                    grab_offset: grab,
                    reanchor_on_first_move: true,
                });
                control.set_pointer_capture(self.core.id);
                self.note_scrollbar_interaction();
                return true;
            }
        }
        false
    }

    fn handle_scrollbar_pointer_move(
        &mut self,
        event: &PointerMoveEvent,
        control: &mut ViewportControl<'_>,
    ) -> bool {
        if let Some(drag) = self.scrollbar_drag {
            let mut drag = drag;
            match drag.axis {
                ScrollbarAxis::Vertical => control.cancel_scroll_track(self.core.id, ScrollAxis::Y),
                ScrollbarAxis::Horizontal => {
                    control.cancel_scroll_track(self.core.id, ScrollAxis::X)
                }
            }
            if drag.reanchor_on_first_move {
                let (inner_x, inner_y) = self.local_inner_origin();
                let geometry = self.scrollbar_geometry(inner_x, inner_y);
                drag.grab_offset = match drag.axis {
                    ScrollbarAxis::Vertical => geometry
                        .vertical_thumb
                        .map(|thumb| (event.pointer.local_y - thumb.y).clamp(0.0, thumb.height))
                        .unwrap_or(drag.grab_offset),
                    ScrollbarAxis::Horizontal => geometry
                        .horizontal_thumb
                        .map(|thumb| (event.pointer.local_x - thumb.x).clamp(0.0, thumb.width))
                        .unwrap_or(drag.grab_offset),
                };
                drag.reanchor_on_first_move = false;
                self.scrollbar_drag = Some(drag);
            }
            let changed = self.update_scroll_from_drag(
                drag.axis,
                event.pointer.local_x,
                event.pointer.local_y,
                drag.grab_offset,
            );
            if changed {
                self.note_scrollbar_interaction();
                control.request_redraw();
            }
            return true;
        }
        if self.scrollbar_visibility_alpha() <= 0.0 {
            return false;
        }
        let (inner_x, inner_y) = self.local_inner_origin();
        let geometry = self.scrollbar_geometry(inner_x, inner_y);
        let local_x = event.pointer.local_x;
        let local_y = event.pointer.local_y;
        if geometry
            .vertical_thumb
            .is_some_and(|thumb| thumb.contains(local_x, local_y))
            || geometry
                .horizontal_thumb
                .is_some_and(|thumb| thumb.contains(local_x, local_y))
        {
            self.note_scrollbar_interaction();
            control.request_redraw();
        }
        false
    }

    fn handle_scrollbar_pointer_up(
        &mut self,
        event: &PointerUpEvent,
        control: &mut ViewportControl<'_>,
    ) -> bool {
        if event.pointer.button != Some(UiPointerButton::Left) {
            return false;
        }
        if self.scrollbar_drag.take().is_some() {
            control.release_pointer_capture(self.core.id);
            self.note_scrollbar_interaction();
            return true;
        }
        if self.is_scrollbar_hit(event.pointer.local_x, event.pointer.local_y) {
            self.note_scrollbar_interaction();
            return true;
        }
        false
    }

    fn render_scrollbar_shadow(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
        rect: Rect,
        border_radius: f32,
        color: [f32; 4],
    ) -> BuildState {
        let mesh = ShadowMesh::rounded_rect(
            rect.x,
            rect.y,
            rect.width.max(0.0),
            rect.height.max(0.0),
            border_radius.max(0.0),
        );
        let params = ShadowParams {
            offset_x: 1.0,
            offset_y: 1.0,
            blur_radius: self.scrollbar_shadow_blur_radius.max(0.0),
            color,
            opacity: 1.0,
            spread: 0.0,
            clip_to_geometry: true,
        };

        let next_state = self.push_shadow_pass(
            mesh,
            params,
            graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
        );
        ctx.set_state(next_state);
        ctx.into_state()
    }

    fn render_scrollbars(&mut self, graph: &mut FrameGraph, mut ctx: UiBuildContext) -> BuildState {
        let alpha = self.scrollbar_visibility_alpha();
        if alpha <= 0.0 {
            return ctx.into_state();
        }
        const TRACK_SHADOW_ALPHA: f32 = 0.5;
        const THUMB_SHADOW_ALPHA: f32 = 0.5;
        let geometry =
            self.scrollbar_geometry(self.layout_inner_position.x, self.layout_inner_position.y);
        let track_alpha = (0.35 * alpha).clamp(0.0, 1.0);
        let thumb_alpha = (0.58 * alpha).clamp(0.0, 1.0);
        let track_shadow_alpha = (TRACK_SHADOW_ALPHA * alpha).clamp(0.0, 1.0);
        let thumb_shadow_alpha = (THUMB_SHADOW_ALPHA * alpha).clamp(0.0, 1.0);
        let track_shadow_color = [0.0, 0.0, 0.0, track_shadow_alpha];
        let thumb_shadow_color = [0.0, 0.0, 0.0, thumb_shadow_alpha];
        let track_color = [0.95, 0.95, 0.95, track_alpha];
        let thumb_color = [0.95, 0.95, 0.95, thumb_alpha];
        if let Some(track) = geometry.vertical_track {
            let shadow_state = self.render_scrollbar_shadow(
                graph,
                UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
                track,
                (track.width * 0.5).max(0.0),
                track_shadow_color,
            );
            ctx.set_state(shadow_state);

            let mut pass = DrawRectPass::new(
                RectPassParams {
                    position: [track.x, track.y],
                    size: [track.width, track.height],
                    fill_color: track_color,
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_border_width(0.0);
            pass.set_border_radius((track.width * 0.5).max(0.0));
            self.push_rect_pass_auto(graph, &mut ctx, pass);
        }
        if let Some(track) = geometry.horizontal_track {
            let shadow_state = self.render_scrollbar_shadow(
                graph,
                UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
                track,
                (track.height * 0.5).max(0.0),
                track_shadow_color,
            );
            ctx.set_state(shadow_state);

            let mut pass = DrawRectPass::new(
                RectPassParams {
                    position: [track.x, track.y],
                    size: [track.width, track.height],
                    fill_color: track_color,
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_border_width(0.0);
            pass.set_border_radius((track.height * 0.5).max(0.0));
            self.push_rect_pass_auto(graph, &mut ctx, pass);
        }
        if let Some(thumb) = geometry.vertical_thumb {
            let shadow_state = self.render_scrollbar_shadow(
                graph,
                UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
                thumb,
                (thumb.width * 0.5).max(0.0),
                thumb_shadow_color,
            );
            ctx.set_state(shadow_state);

            let mut pass = DrawRectPass::new(
                RectPassParams {
                    position: [thumb.x, thumb.y],
                    size: [thumb.width, thumb.height],
                    fill_color: thumb_color,
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_border_width(0.0);
            pass.set_border_radius((thumb.width * 0.5).max(0.0));
            self.push_rect_pass_auto(graph, &mut ctx, pass);
        }
        if let Some(thumb) = geometry.horizontal_thumb {
            let shadow_state = self.render_scrollbar_shadow(
                graph,
                UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
                thumb,
                (thumb.height * 0.5).max(0.0),
                thumb_shadow_color,
            );
            ctx.set_state(shadow_state);

            let mut pass = DrawRectPass::new(
                RectPassParams {
                    position: [thumb.x, thumb.y],
                    size: [thumb.width, thumb.height],
                    fill_color: thumb_color,
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_border_width(0.0);
            pass.set_border_radius((thumb.height * 0.5).max(0.0));
            self.push_rect_pass_auto(graph, &mut ctx, pass);
        }
        ctx.into_state()
    }

    pub fn on_pointer_down<F>(&mut self, handler: F)
    where
        F: FnMut(&mut PointerDownEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).pointer_down.push(Box::new(handler));
    }

    pub fn on_pointer_up<F>(&mut self, handler: F)
    where
        F: FnMut(&mut PointerUpEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).pointer_up.push(Box::new(handler));
    }

    pub fn on_pointer_move<F>(&mut self, handler: F)
    where
        F: FnMut(&mut PointerMoveEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).pointer_move.push(Box::new(handler));
    }

    pub fn on_pointer_enter<F>(&mut self, handler: F)
    where
        F: FnMut(&mut PointerEnterEvent) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).pointer_enter.push(Box::new(handler));
    }

    pub fn on_pointer_leave<F>(&mut self, handler: F)
    where
        F: FnMut(&mut PointerLeaveEvent) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).pointer_leave.push(Box::new(handler));
    }

    pub fn on_click<F>(&mut self, handler: F)
    where
        F: FnMut(&mut ClickEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).click.push(Box::new(handler));
    }

    pub fn on_key_down<F>(&mut self, handler: F)
    where
        F: FnMut(&mut KeyDownEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).key_down.push(Box::new(handler));
    }

    pub fn on_key_up<F>(&mut self, handler: F)
    where
        F: FnMut(&mut KeyUpEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).key_up.push(Box::new(handler));
    }

    pub fn on_focus<F>(&mut self, handler: F)
    where
        F: FnMut(&mut FocusEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).focus.push(Box::new(handler));
    }

    pub fn on_blur<F>(&mut self, handler: F)
    where
        F: FnMut(&mut BlurEvent, &mut ViewportControl<'_>) + 'static,
    {
        self.event_handlers.get_or_insert_with(Default::default).blur.push(Box::new(handler));
    }

    pub fn id(&self) -> u64 {
        self.core.id
    }

    pub(crate) fn child_layout_origin(&self) -> (f32, f32) {
        (
            self.layout_flow_inner_position.x - self.scroll_offset.x,
            self.layout_flow_inner_position.y - self.scroll_offset.y,
        )
    }

    pub(crate) fn layout_flow_origin(&self) -> (f32, f32) {
        (self.layout_flow_position.x, self.layout_flow_position.y)
    }

    /// Append a pre-inserted arena node as child. Caller must already have
    /// inserted the child into the arena and set its `Node.parent` to this
    /// element's own key.
    ///
    /// Approach-C migration: old API took `Box<dyn ElementTrait>` and
    /// pushed into a `Vec<Box<_>>` owned by this element. That ownership
    /// now lives in [`crate::view::node_arena::NodeArena`]; callers hand
    /// keys instead.
    pub fn add_child(&mut self, arena: &crate::view::node_arena::NodeArena, child: crate::view::node_arena::NodeKey) {
        if let Some(child_node) = arena.get(child) {
            if let Some(element) = child_node.element.as_any().downcast_ref::<Element>() {
                self.has_absolute_descendant_for_hit_test |= element
                    .is_absolute_positioned_for_hit_test()
                    || element.has_absolute_descendant_for_hit_test;
            }
        }
        self.children.push(child);
        self.mark_layout_dirty();
    }

    /// Replace the child-key list wholesale. Returns the previous keys so
    /// the caller can remove the corresponding nodes from the arena.
    pub(crate) fn replace_children(
        &mut self,
        arena: &crate::view::node_arena::NodeArena,
        children: Vec<crate::view::node_arena::NodeKey>,
    ) -> Vec<crate::view::node_arena::NodeKey> {
        self.has_absolute_descendant_for_hit_test = false;
        for child in &children {
            if let Some(child_node) = arena.get(*child) {
                if let Some(element) = child_node.element.as_any().downcast_ref::<Element>() {
                    self.has_absolute_descendant_for_hit_test |= element
                        .is_absolute_positioned_for_hit_test()
                        || element.has_absolute_descendant_for_hit_test;
                }
            }
        }
        self.mark_layout_dirty();
        std::mem::replace(&mut self.children, children)
    }

    pub(crate) fn has_absolute_descendant_for_hit_test(&self) -> bool {
        self.has_absolute_descendant_for_hit_test
    }

    pub(crate) fn is_absolute_positioned_for_hit_test(&self) -> bool {
        self.computed_style.position.mode() == PositionMode::Absolute
    }

    pub(crate) fn clip_mode_for_hit_test(&self) -> ClipMode {
        self.computed_style.position.clip_mode()
    }

    pub(crate) fn has_anchor_name_for_hit_test(&self) -> bool {
        self.computed_style.position.anchor_name().is_some()
    }

    pub(crate) fn should_append_to_root_viewport_render(&self) -> bool {
        self.computed_style.position.mode() == PositionMode::Absolute
            && self.computed_style.position.clip_mode() == ClipMode::Viewport
    }

    /// Recurses through `Node.children` keys in the arena. Caller must
    /// pass a live arena reference since children now live there rather
    /// than nested under this element.
    fn collect_root_viewport_deferred_descendants(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        ctx: &mut UiBuildContext,
    ) {
        for child_key in &self.children {
            let Some(child_node) = arena.get(*child_key) else { continue };
            let Some(element) = child_node.element.as_any().downcast_ref::<Element>() else {
                continue;
            };
            if element.should_append_to_root_viewport_render() {
                ctx.append_to_defer(element.id());
            }
            element.collect_root_viewport_deferred_descendants(arena, ctx);
        }
    }
}
