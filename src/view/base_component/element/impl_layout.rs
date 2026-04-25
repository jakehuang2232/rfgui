impl Element {
    const LAYOUT_TRANSITION_FINISH_EPSILON: f32 = 0.05;

    fn measure_self(&mut self, proposal: LayoutProposal) {
        if let SizeValue::Length(width) = self.computed_style.width {
            if let Some(resolved) = resolve_px_with_base(
                width,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            ) {
                self.core.set_width(resolved);
            }
        }
        if let SizeValue::Length(height) = self.computed_style.height {
            if let Some(resolved) = resolve_px_with_base(
                height,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            ) {
                self.core.set_height(resolved);
            }
        }
    }

    fn resolve_size_constraint(
        value: SizeValue,
        percent_base: Option<f32>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let SizeValue::Length(length) = value else {
            return None;
        };
        resolve_px_with_base(length, percent_base, viewport_width, viewport_height)
            .map(|v| v.max(0.0))
    }

    fn apply_size_constraints(&mut self, proposal: LayoutProposal, include_auto: bool) {
        let min_width = Self::resolve_size_constraint(
            self.computed_style.min_width,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        )
        .unwrap_or(0.0);
        let min_height = Self::resolve_size_constraint(
            self.computed_style.min_height,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        )
        .unwrap_or(0.0);

        let mut max_width = Self::resolve_size_constraint(
            self.computed_style.max_width,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let mut max_height = Self::resolve_size_constraint(
            self.computed_style.max_height,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        if let Some(value) = max_width {
            max_width = Some(value.max(min_width));
        }
        if let Some(value) = max_height {
            max_height = Some(value.max(min_height));
        }

        if include_auto || self.computed_style.width != SizeValue::Auto {
            let mut width = self.core.size.width.max(0.0).max(min_width);
            if let Some(max_width) = max_width {
                width = width.min(max_width);
            }
            self.core.set_width(width);
        }

        if include_auto || self.computed_style.height != SizeValue::Auto {
            let mut height = self.core.size.height.max(0.0).max(min_height);
            if let Some(max_height) = max_height {
                height = height.min(max_height);
            }
            self.core.set_height(height);
        }
    }

    fn width_is_known(&self, proposal: LayoutProposal) -> bool {
        match self.computed_style.width {
            SizeValue::Length(length) if length.needs_percent_base() => {
                proposal.percent_base_width.is_some()
            }
            SizeValue::Length(Length::Vw(_)) => true,
            SizeValue::Length(Length::Vh(_)) => true,
            SizeValue::Length(_) => true,
            SizeValue::Auto => proposal.percent_base_width.is_some(),
        }
    }

    fn height_is_known(&self, proposal: LayoutProposal) -> bool {
        match self.computed_style.height {
            SizeValue::Length(length) if length.needs_percent_base() => {
                proposal.percent_base_height.is_some()
            }
            SizeValue::Length(Length::Vw(_)) => true,
            SizeValue::Length(Length::Vh(_)) => true,
            SizeValue::Length(_) => true,
            SizeValue::Auto => {
                self.layout_assigned_height.is_some()
                    || (self.intrinsic_size_is_percent_base
                        && proposal.percent_base_height.is_some()
                        && self.core.size.height > 0.0)
            }
        }
    }

    fn resolve_lengths_from_parent_inner(&mut self, proposal: LayoutProposal) {
        self.border_widths.left = resolve_px_or_zero(
            self.computed_style.border_widths.left,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        self.border_widths.right = resolve_px_or_zero(
            self.computed_style.border_widths.right,
            proposal.percent_base_width,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        self.border_widths.top = resolve_px_or_zero(
            self.computed_style.border_widths.top,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        self.border_widths.bottom = resolve_px_or_zero(
            self.computed_style.border_widths.bottom,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        if self
            .parsed_style
            .get(crate::style::PropertyId::PaddingLeft)
            .is_some()
        {
            self.padding.left = resolve_px_or_zero(
                self.computed_style.padding.left,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
        }
        if self
            .parsed_style
            .get(crate::style::PropertyId::PaddingRight)
            .is_some()
        {
            self.padding.right = resolve_px_or_zero(
                self.computed_style.padding.right,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
        }
        if self
            .parsed_style
            .get(crate::style::PropertyId::PaddingTop)
            .is_some()
        {
            self.padding.top = resolve_px_or_zero(
                self.computed_style.padding.top,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );
        }
        if self
            .parsed_style
            .get(crate::style::PropertyId::PaddingBottom)
            .is_some()
        {
            self.padding.bottom = resolve_px_or_zero(
                self.computed_style.padding.bottom,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );
        }
    }

    fn resolve_corner_radii_from_self_box(&mut self, proposal: LayoutProposal) {
        let radius_base = self
            .layout_state
            .layout_size
            .width
            .min(self.layout_state.layout_size.height)
            .max(0.0);
        self.border_radii = CornerRadii {
            top_left: resolve_px(
                self.computed_style.border_radii.top_left,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            top_right: resolve_px(
                self.computed_style.border_radii.top_right,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            bottom_right: resolve_px(
                self.computed_style.border_radii.bottom_right,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            bottom_left: resolve_px(
                self.computed_style.border_radii.bottom_left,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
        };
        self.border_radius = self.border_radii.max();
    }

    pub(crate) fn set_border_radius_transition_sample(&mut self, radius: f32) {
        let proposal = self.last_layout_proposal.unwrap_or(LayoutProposal {
            width: self.layout_state.layout_size.width.max(0.0),
            height: self.layout_state.layout_size.height.max(0.0),
            viewport_width: 0.0,
            viewport_height: 0.0,
            percent_base_width: None,
            percent_base_height: None,
        });
        let radius_base = self
            .layout_state
            .layout_size
            .width
            .min(self.layout_state.layout_size.height)
            .max(0.0);
        let target_radii = CornerRadii {
            top_left: resolve_px(
                self.computed_style.border_radii.top_left,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            top_right: resolve_px(
                self.computed_style.border_radii.top_right,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            bottom_right: resolve_px(
                self.computed_style.border_radii.bottom_right,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
            bottom_left: resolve_px(
                self.computed_style.border_radii.bottom_left,
                radius_base,
                proposal.viewport_width,
                proposal.viewport_height,
            ),
        };
        let target_max = target_radii.max();
        if target_max <= f32::EPSILON {
            self.border_radii = CornerRadii::uniform(0.0);
            self.border_radius = 0.0;
            return;
        }

        let scale = radius.max(0.0) / target_max;
        self.border_radii = CornerRadii {
            top_left: target_radii.top_left * scale,
            top_right: target_radii.top_right * scale,
            bottom_right: target_radii.bottom_right * scale,
            bottom_left: target_radii.bottom_left * scale,
        };
        self.border_radius = self.border_radii.max();
    }

    fn update_content_size_from_children(
        &mut self,
        arena: &crate::view::node_arena::NodeArena,
        absolute_mask: &[bool],
    ) {
        if self.children.is_empty() {
            self.layout_state.content_size = Size {
                width: 0.0,
                height: 0.0,
            };
            return;
        }
        let mut max_x = 0.0_f32;
        let mut max_y = 0.0_f32;
        for (idx, child_key) in self.children.iter().copied().enumerate() {
            if absolute_mask.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let Some(child_node) = arena.get(child_key) else {
                continue;
            };
            let snapshot = child_node.element.box_model_snapshot();
            let (child_flow_x, child_flow_y) = child_node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .map(|el| (el.layout_state.layout_flow_position.x, el.layout_state.layout_flow_position.y))
                .unwrap_or((snapshot.x, snapshot.y));
            let rel_x = child_flow_x - self.layout_state.layout_flow_inner_position.x + self.scroll_offset.x;
            let rel_y = child_flow_y - self.layout_state.layout_flow_inner_position.y + self.scroll_offset.y;
            max_x = max_x.max(rel_x + snapshot.width.max(0.0));
            max_y = max_y.max(rel_y + snapshot.height.max(0.0));
        }
        self.layout_state.content_size = Size {
            width: max_x.max(0.0),
            height: max_y.max(0.0),
        };
    }

    fn clamp_scroll_offset(&mut self) {
        let max_x = (self.layout_state.content_size.width - self.layout_state.layout_inner_size.width).max(0.0);
        let max_y = (self.layout_state.content_size.height - self.layout_state.layout_inner_size.height).max(0.0);
        self.scroll_offset.x = self.scroll_offset.x.clamp(0.0, max_x);
        self.scroll_offset.y = self.scroll_offset.y.clamp(0.0, max_y);
    }

    fn update_size_from_measured_children(
        &mut self,
        arena: &crate::view::node_arena::NodeArena,
        absolute_mask: &[bool],
    ) {
        let has_in_flow_children = absolute_mask.iter().any(|is_abs| !*is_abs)
            && absolute_mask.len() == self.children.len();

        let mut max_w = 0.0_f32;
        let mut max_h = 0.0_f32;
        if has_in_flow_children {
            for (idx, child_key) in self.children.iter().copied().enumerate() {
                if absolute_mask.get(idx).copied().unwrap_or(false) {
                    continue;
                }
                let Some(child_node) = arena.get(child_key) else {
                    continue;
                };
                let (w, h) = child_node.element.measured_size();
                max_w = max_w.max(w);
                max_h = max_h.max(h);
            }
        }

        let proposal = self.last_layout_proposal.unwrap_or(LayoutProposal {
            width: 10_000.0,
            height: 10_000.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
            percent_base_width: None,
            percent_base_height: None,
        });

        let insets = resolve_layout_insets(
            &self.computed_style.border_widths,
            &self.computed_style.padding,
            proposal.percent_base_width,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        if self.computed_style.width == SizeValue::Auto {
            self.core.set_width(max_w + insets.horizontal());
        }
        if self.computed_style.height == SizeValue::Auto {
            self.core.set_height(max_h + insets.vertical());
        }
    }

    fn place_self(
        &mut self,
        proposal: LayoutProposal,
        parent_x: f32,
        parent_y: f32,
        parent_visual_offset_x: f32,
        parent_visual_offset_y: f32,
    ) {
        let fallback_parent_rect = Rect {
            x: parent_x + parent_visual_offset_x,
            y: parent_y + parent_visual_offset_y,
            width: proposal.width.max(0.0),
            height: proposal.height.max(0.0),
        };
        let parent_clip_rect = self
            .current_parent_hit_test_clip_rect()
            .unwrap_or(fallback_parent_rect);
        self.anchor_parent_clip_rect = Some(parent_clip_rect);
        // The current layout pass must always start from the latest assigned size.
        // Active transition targets are historical state used only for retarget detection.
        let mut target_width = self
            .layout_assigned_width
            .unwrap_or(self.core.size.width)
            .max(0.0);
        let mut target_height = self
            .layout_assigned_height
            .unwrap_or(self.core.size.height)
            .max(0.0);
        let mut target_rel_x = self.core.position.x;
        let mut target_rel_y = self.core.position.y;
        let is_absolute = self.computed_style.position.mode() == PositionMode::Absolute;
        let mut absolute_clip_rect: Option<Rect> = None;
        if is_absolute {
            let fallback_anchor = AnchorSnapshot {
                x: parent_x,
                y: parent_y,
                width: proposal.width.max(0.0),
                height: proposal.height.max(0.0),
                parent_clip_rect,
            };
            let anchor = self.resolve_anchor_snapshot(fallback_anchor);
            let left = self.computed_style.position.left_inset().and_then(|v| {
                resolve_signed_px_with_base(
                    v,
                    Some(anchor.width),
                    proposal.viewport_width,
                    proposal.viewport_height,
                )
            });
            let right = self.computed_style.position.right_inset().and_then(|v| {
                resolve_signed_px_with_base(
                    v,
                    Some(anchor.width),
                    proposal.viewport_width,
                    proposal.viewport_height,
                )
            });
            let top = self.computed_style.position.top_inset().and_then(|v| {
                resolve_signed_px_with_base(
                    v,
                    Some(anchor.height),
                    proposal.viewport_width,
                    proposal.viewport_height,
                )
            });
            let bottom = self.computed_style.position.bottom_inset().and_then(|v| {
                resolve_signed_px_with_base(
                    v,
                    Some(anchor.height),
                    proposal.viewport_width,
                    proposal.viewport_height,
                )
            });

            if let (Some(l), Some(r)) = (left, right) {
                target_width = (anchor.width - l - r).max(0.0);
            }
            if let (Some(t), Some(b)) = (top, bottom) {
                target_height = (anchor.height - t - b).max(0.0);
            }

            target_rel_x = if let Some(l) = left {
                (anchor.x - parent_x) + l
            } else if let Some(r) = right {
                (anchor.x - parent_x) + (anchor.width - r - target_width)
            } else {
                anchor.x - parent_x
            };
            target_rel_y = if let Some(t) = top {
                (anchor.y - parent_y) + t
            } else if let Some(b) = bottom {
                (anchor.y - parent_y) + (anchor.height - b - target_height)
            } else {
                anchor.y - parent_y
            };

            let mut abs_x = parent_x + target_rel_x;
            let mut abs_y = parent_y + target_rel_y;
            let boundary = match self.computed_style.position.collision_boundary() {
                CollisionBoundary::Parent => Rect {
                    x: parent_x,
                    y: parent_y,
                    width: proposal.width.max(0.0),
                    height: proposal.height.max(0.0),
                },
                CollisionBoundary::Viewport => {
                    let (vw, vh) = self.viewport_size_from_runtime(proposal.width, proposal.height);
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: vw,
                        height: vh,
                    }
                }
            };
            let clip_mode = self.computed_style.position.clip_mode();
            let has_anchor = self.computed_style.position.anchor_name().is_some();
            absolute_clip_rect = Some(match clip_mode {
                ClipMode::Parent => parent_clip_rect,
                ClipMode::Viewport => {
                    let (vw, vh) = self.viewport_size_from_runtime(proposal.width, proposal.height);
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: vw.max(0.0),
                        height: vh.max(0.0),
                    }
                }
                ClipMode::AnchorParent if has_anchor => anchor.parent_clip_rect,
                ClipMode::AnchorParent => parent_clip_rect,
            });
            apply_collision(
                self.computed_style.position.collision_mode(),
                boundary,
                &mut abs_x,
                &mut abs_y,
                target_width,
                target_height,
                anchor,
                left,
                right,
                top,
                bottom,
            );
            target_rel_x = abs_x - parent_x;
            target_rel_y = abs_y - parent_y;
        }
        let has_x_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            matches!(
                t.property,
                TransitionProperty::Position
                    | TransitionProperty::PositionX
                    | TransitionProperty::X
            )
        });
        let has_y_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            matches!(
                t.property,
                TransitionProperty::Position
                    | TransitionProperty::PositionY
                    | TransitionProperty::Y
            )
        });
        let has_width_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            matches!(
                t.property,
                TransitionProperty::All | TransitionProperty::Width
            )
        });
        let has_height_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            matches!(
                t.property,
                TransitionProperty::All | TransitionProperty::Height
            )
        });
        if !has_x_transition {
            self.layout_transition_visual_offset_x = 0.0;
            self.layout_transition_target_x = None;
        }
        if !has_y_transition {
            self.layout_transition_visual_offset_y = 0.0;
            self.layout_transition_target_y = None;
        }
        if !has_width_transition {
            self.layout_transition_override_width = None;
            self.layout_transition_target_width = None;
        } else if self
            .layout_transition_override_width
            .zip(self.layout_transition_target_width)
            .is_some_and(|(current, target)| approx_eq(current, target))
        {
            self.layout_transition_override_width = None;
            self.layout_transition_target_width = None;
        }
        if !has_height_transition {
            self.layout_transition_override_height = None;
            self.layout_transition_target_height = None;
        } else if self
            .layout_transition_override_height
            .zip(self.layout_transition_target_height)
            .is_some_and(|(current, target)| approx_eq(current, target))
        {
            self.layout_transition_override_height = None;
            self.layout_transition_target_height = None;
        }
        let current_visual_rel_x = (self.layout_state.layout_flow_position.x - self.last_parent_layout_x)
            + self.layout_transition_visual_offset_x;
        let current_visual_rel_y = (self.layout_state.layout_flow_position.y - self.last_parent_layout_y)
            + self.layout_transition_visual_offset_y;
        let prev_target_rel_x = self.layout_state.layout_flow_position.x - self.last_parent_layout_x;
        let prev_target_rel_y = self.layout_state.layout_flow_position.y - self.last_parent_layout_y;
        let current_offset_x = current_visual_rel_x - target_rel_x;
        let current_offset_y = current_visual_rel_y - target_rel_y;
        let prev_width = self.layout_state.layout_size.width.max(0.0);
        let prev_height = self.layout_state.layout_size.height.max(0.0);
        // If visual target changes while track is active, always rebase from current rendered
        // position and restart. This keeps the visual track start anchored to "where it is now".
        if self
            .layout_transition_target_x
            .is_some_and(|active| !approx_eq(active, target_rel_x))
        {
            self.layout_transition_visual_offset_x = current_offset_x;
            self.layout_transition_target_x = None;
        }
        if self
            .layout_transition_target_y
            .is_some_and(|active| !approx_eq(active, target_rel_y))
        {
            self.layout_transition_visual_offset_y = current_offset_y;
            self.layout_transition_target_y = None;
        }
        if self.has_layout_snapshot {
            self.collect_layout_transition_requests(
                current_offset_x,
                current_offset_y,
                prev_target_rel_x,
                prev_target_rel_y,
                prev_width,
                prev_height,
                target_rel_x,
                target_rel_y,
                target_width,
                target_height,
            );
        }
        self.has_layout_snapshot = true;

        let frame_rel_x = target_rel_x;
        let frame_rel_y = target_rel_y;
        let frame_width = self
            .layout_transition_override_width
            .unwrap_or(target_width)
            .max(0.0);
        let frame_height = self
            .layout_transition_override_height
            .unwrap_or(target_height)
            .max(0.0);
        self.layout_state.layout_flow_position = Position {
            x: parent_x + frame_rel_x,
            y: parent_y + frame_rel_y,
        };
        let frame = LayoutFrame {
            x: self.layout_state.layout_flow_position.x
                + parent_visual_offset_x
                + self.layout_transition_visual_offset_x,
            y: self.layout_state.layout_flow_position.y
                + parent_visual_offset_y
                + self.layout_transition_visual_offset_y,
            width: frame_width,
            height: frame_height,
        };
        self.layout_state.layout_position = Position {
            x: frame.x,
            y: frame.y,
        };
        self.layout_state.layout_size = Size {
            width: frame.width,
            height: frame.height,
        };
        self.update_resolved_transform();

        self.absolute_clip_rect = if is_absolute {
            absolute_clip_rect
        } else {
            None
        };
        let inherited_hit_test_clip = self
            .current_parent_hit_test_clip_rect()
            .unwrap_or(parent_clip_rect);
        self.hit_test_clip_rect = Some(if is_absolute {
            match self.computed_style.position.clip_mode() {
                ClipMode::Viewport => absolute_clip_rect.unwrap_or(inherited_hit_test_clip),
                ClipMode::Parent | ClipMode::AnchorParent => absolute_clip_rect
                    .map(|rect| intersect_rect(rect, inherited_hit_test_clip))
                    .unwrap_or(inherited_hit_test_clip),
            }
        } else {
            inherited_hit_test_clip
        });
        let cull_rect = if is_absolute {
            absolute_clip_rect.unwrap_or(parent_clip_rect)
        } else {
            self.current_parent_child_clip_rect().unwrap_or(parent_clip_rect)
        };
        let transformed_frame_bounds = self.transformed_frame_bounding_rect(frame);
        let intersects_parent_clip = transformed_frame_bounds.intersects(cull_rect);
        let intersects_absolute_clip = self
            .absolute_clip_rect
            .is_none_or(|clip| transformed_frame_bounds.intersects(clip));
        let max_bw = (frame.width.min(frame.height)) * 0.5;
        let border_left = self.border_widths.left.clamp(0.0, max_bw);
        let border_right = self.border_widths.right.clamp(0.0, max_bw);
        let border_top = self.border_widths.top.clamp(0.0, max_bw);
        let border_bottom = self.border_widths.bottom.clamp(0.0, max_bw);
        let inset_left = border_left + self.padding.left.max(0.0);
        let inset_right = border_right + self.padding.right.max(0.0);
        let inset_top = border_top + self.padding.top.max(0.0);
        let inset_bottom = border_bottom + self.padding.bottom.max(0.0);
        let inner_width = (frame.width - inset_left - inset_right).max(0.0);
        let inner_height = (frame.height - inset_top - inset_bottom).max(0.0);
        let has_nonzero_inner_area = inner_width > 0.0 && inner_height > 0.0;
        let has_visible_self_paint = self.has_visible_self_paint(
            frame.width.max(0.0),
            frame.height.max(0.0),
            proposal.viewport_width,
            proposal.viewport_height,
        );

        self.layout_state.should_render = frame.width > 0.0
            && frame.height > 0.0
            && intersects_parent_clip
            && intersects_absolute_clip;
        self.core.should_paint = self.layout_state.should_render
            && self.computed_style.opacity > 0.0
            && has_nonzero_inner_area
            && has_visible_self_paint;
        self.last_parent_layout_x = parent_x;
        self.last_parent_layout_y = parent_y;
    }

    fn collect_layout_transition_requests(
        &mut self,
        prev_offset_x: f32,
        prev_offset_y: f32,
        prev_target_rel_x: f32,
        prev_target_rel_y: f32,
        prev_width: f32,
        prev_height: f32,
        target_rel_x: f32,
        target_rel_y: f32,
        target_width: f32,
        target_height: f32,
    ) {
        let current_width = self
            .layout_transition_override_width
            .unwrap_or(prev_width)
            .max(0.0);
        let current_height = self
            .layout_transition_override_height
            .unwrap_or(prev_height)
            .max(0.0);
        let width_is_close_enough =
            (current_width - target_width).abs() < Self::LAYOUT_TRANSITION_FINISH_EPSILON;
        let height_is_close_enough =
            (current_height - target_height).abs() < Self::LAYOUT_TRANSITION_FINISH_EPSILON;
        if width_is_close_enough {
            self.layout_transition_override_width = None;
            self.layout_transition_target_width = None;
        }
        if height_is_close_enough {
            self.layout_transition_override_height = None;
            self.layout_transition_target_height = None;
        }
        for transition in self.computed_style.transition.as_slice() {
            let runtime_layout = RuntimeLayoutTransition {
                duration_ms: transition.duration_ms,
                delay_ms: transition.delay_ms,
                timing: map_transition_timing(transition.timing),
            };
            let runtime_visual = RuntimeVisualTransition {
                duration_ms: transition.duration_ms,
                delay_ms: transition.delay_ms,
                timing: map_transition_timing(transition.timing),
            };
            match transition.property {
                TransitionProperty::All => {
                    let should_start_width = self
                        .layout_transition_target_width
                        .is_none_or(|active| !approx_eq(active, target_width));
                    if should_start_width && !width_is_close_enough {
                        self.transition_requests.get_or_insert_with(Default::default).layout
                            .push(LayoutTrackRequest {
                                target: self.core.id,
                                field: LayoutField::Width,
                                from: current_width,
                                to: target_width,
                                transition: runtime_layout,
                            });
                        self.layout_transition_override_width = Some(current_width);
                        self.layout_transition_target_width = Some(target_width);
                    }
                    let should_start_height = self
                        .layout_transition_target_height
                        .is_none_or(|active| !approx_eq(active, target_height));
                    if should_start_height && !height_is_close_enough {
                        self.transition_requests.get_or_insert_with(Default::default).layout
                            .push(LayoutTrackRequest {
                                target: self.core.id,
                                field: LayoutField::Height,
                                from: current_height,
                                to: target_height,
                                transition: runtime_layout,
                            });
                        self.layout_transition_override_height = Some(current_height);
                        self.layout_transition_target_height = Some(target_height);
                    }
                }
                TransitionProperty::Position => {
                    let should_start_x = self.layout_transition_target_x.is_none();
                    if should_start_x
                        && !approx_eq(prev_offset_x, 0.0)
                        && !approx_eq(prev_target_rel_x, target_rel_x)
                    {
                        self.transition_requests.get_or_insert_with(Default::default).visual
                            .push(VisualTrackRequest {
                                target: self.core.id,
                                field: VisualField::X,
                                from: prev_offset_x,
                                to: 0.0,
                                transition: runtime_visual,
                            });
                        self.layout_transition_visual_offset_x = prev_offset_x;
                        self.layout_transition_target_x = Some(target_rel_x);
                    }
                    let should_start_y = self.layout_transition_target_y.is_none();
                    if should_start_y
                        && !approx_eq(prev_offset_y, 0.0)
                        && !approx_eq(prev_target_rel_y, target_rel_y)
                    {
                        self.transition_requests.get_or_insert_with(Default::default).visual
                            .push(VisualTrackRequest {
                                target: self.core.id,
                                field: VisualField::Y,
                                from: prev_offset_y,
                                to: 0.0,
                                transition: runtime_visual,
                            });
                        self.layout_transition_visual_offset_y = prev_offset_y;
                        self.layout_transition_target_y = Some(target_rel_y);
                    }
                }
                TransitionProperty::Width => {
                    let should_start_width = self
                        .layout_transition_target_width
                        .is_none_or(|active| !approx_eq(active, target_width));
                    if should_start_width && !width_is_close_enough {
                        self.transition_requests.get_or_insert_with(Default::default).layout
                            .push(LayoutTrackRequest {
                                target: self.core.id,
                                field: LayoutField::Width,
                                from: current_width,
                                to: target_width,
                                transition: runtime_layout,
                            });
                        self.layout_transition_override_width = Some(current_width);
                        self.layout_transition_target_width = Some(target_width);
                    }
                }
                TransitionProperty::Height => {
                    let should_start_height = self
                        .layout_transition_target_height
                        .is_none_or(|active| !approx_eq(active, target_height));
                    if should_start_height && !height_is_close_enough {
                        self.transition_requests.get_or_insert_with(Default::default).layout
                            .push(LayoutTrackRequest {
                                target: self.core.id,
                                field: LayoutField::Height,
                                from: current_height,
                                to: target_height,
                                transition: runtime_layout,
                            });
                        self.layout_transition_override_height = Some(current_height);
                        self.layout_transition_target_height = Some(target_height);
                    }
                }
                TransitionProperty::X | TransitionProperty::PositionX => {
                    let should_start_x = self.layout_transition_target_x.is_none();
                    if should_start_x
                        && !approx_eq(prev_offset_x, 0.0)
                        && !approx_eq(prev_target_rel_x, target_rel_x)
                    {
                        self.transition_requests.get_or_insert_with(Default::default).visual
                            .push(VisualTrackRequest {
                                target: self.core.id,
                                field: VisualField::X,
                                from: prev_offset_x,
                                to: 0.0,
                                transition: runtime_visual,
                            });
                        self.layout_transition_visual_offset_x = prev_offset_x;
                        self.layout_transition_target_x = Some(target_rel_x);
                    }
                }
                TransitionProperty::Y | TransitionProperty::PositionY => {
                    let should_start_y = self.layout_transition_target_y.is_none();
                    if should_start_y
                        && !approx_eq(prev_offset_y, 0.0)
                        && !approx_eq(prev_target_rel_y, target_rel_y)
                    {
                        self.transition_requests.get_or_insert_with(Default::default).visual
                            .push(VisualTrackRequest {
                                target: self.core.id,
                                field: VisualField::Y,
                                from: prev_offset_y,
                                to: 0.0,
                                transition: runtime_visual,
                            });
                        self.layout_transition_visual_offset_y = prev_offset_y;
                        self.layout_transition_target_y = Some(target_rel_y);
                    }
                }
                _ => {}
            }
        }
    }

    fn child_layout_limits_for_inner_size(&self, inner_width: f32, inner_height: f32) -> (f32, f32) {
        const SCROLL_EXPANDED_LIMIT: f32 = 1_000_000.0;
        match self.scroll_direction {
            ScrollDirection::None => (inner_width, inner_height),
            ScrollDirection::Vertical => (inner_width, SCROLL_EXPANDED_LIMIT),
            ScrollDirection::Horizontal => (SCROLL_EXPANDED_LIMIT, inner_height),
            ScrollDirection::Both => (SCROLL_EXPANDED_LIMIT, SCROLL_EXPANDED_LIMIT),
        }
    }

    fn begin_place_scope(&self, placement: LayoutPlacement) {
        PLACEMENT_RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            if runtime.depth == 0 {
                runtime.anchors.clear();
                runtime.child_clip_stack.clear();
                runtime.hit_test_clip_stack.clear();
                runtime.viewport_width = placement.viewport_width.max(0.0);
                runtime.viewport_height = placement.viewport_height.max(0.0);
            }
            runtime.depth += 1;
        });
    }

    fn end_place_scope(&self) {
        PLACEMENT_RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            if runtime.depth > 0 {
                runtime.depth -= 1;
            }
            if runtime.depth == 0 {
                runtime.anchors.clear();
                runtime.child_clip_stack.clear();
                runtime.hit_test_clip_stack.clear();
            }
        });
    }

    fn push_child_clip_scope(&self, rect: Rect) {
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime.borrow_mut().child_clip_stack.push(rect);
        });
    }

    fn pop_child_clip_scope(&self) {
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime.borrow_mut().child_clip_stack.pop();
        });
    }

    fn current_parent_child_clip_rect(&self) -> Option<Rect> {
        PLACEMENT_RUNTIME.with(|runtime| runtime.borrow().child_clip_stack.last().copied())
    }

    fn push_hit_test_clip_scope(&self, rect: Rect) {
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime.borrow_mut().hit_test_clip_stack.push(rect);
        });
    }

    fn pop_hit_test_clip_scope(&self) {
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime.borrow_mut().hit_test_clip_stack.pop();
        });
    }

    fn current_parent_hit_test_clip_rect(&self) -> Option<Rect> {
        PLACEMENT_RUNTIME.with(|runtime| runtime.borrow().hit_test_clip_stack.last().copied())
    }

    fn register_anchor_snapshot(&self) {
        let Some(anchor_name) = self.anchor_name.as_ref() else {
            return;
        };
        let parent_clip_rect = self.anchor_parent_clip_rect.unwrap_or(Rect {
            x: self.last_parent_layout_x,
            y: self.last_parent_layout_y,
            width: 0.0,
            height: 0.0,
        });
        let snapshot = AnchorSnapshot {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width.max(0.0),
            height: self.layout_state.layout_size.height.max(0.0),
            parent_clip_rect,
        };
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime
                .borrow_mut()
                .anchors
                .insert(anchor_name.as_str().to_string(), snapshot);
        });
    }

    fn resolve_anchor_snapshot(&self, fallback: AnchorSnapshot) -> AnchorSnapshot {
        let Some(anchor_name) = self.computed_style.position.anchor_name() else {
            return fallback;
        };
        PLACEMENT_RUNTIME.with(|runtime| {
            runtime
                .borrow()
                .anchors
                .get(anchor_name.as_str())
                .copied()
                .unwrap_or(fallback)
        })
    }

    fn viewport_size_from_runtime(&self, fallback_width: f32, fallback_height: f32) -> (f32, f32) {
        PLACEMENT_RUNTIME.with(|runtime| {
            let runtime = runtime.borrow();
            let width = if runtime.viewport_width > 0.0 {
                runtime.viewport_width
            } else {
                fallback_width.max(0.0)
            };
            let height = if runtime.viewport_height > 0.0 {
                runtime.viewport_height
            } else {
                fallback_height.max(0.0)
            };
            (width, height)
        })
    }

    fn child_is_absolute(
        &self,
        index: usize,
        arena: &crate::view::node_arena::NodeArena,
    ) -> bool {
        self.children
            .get(index)
            .and_then(|k| arena.get(*k))
            .and_then(|node| {
                node.element
                    .as_any()
                    .downcast_ref::<Element>()
                    .map(|el| el.computed_style.position.mode() == PositionMode::Absolute)
            })
            .unwrap_or(false)
    }

    /// Build a parallel `Vec<bool>` matching `self.children` indices where
    /// each entry is `child_is_absolute(idx)`. Running this once at the top
    /// of `place_children` (and then re-using the slice across the two
    /// place passes + `update_size_from_measured_children` +
    /// `update_content_size_from_children`) converts 3–5 redundant per-child
    /// downcasts into a single pass. Cheap to call; caller owns the buffer.
    pub(crate) fn compute_children_absolute_mask(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<bool> {
        let mut mask = Vec::with_capacity(self.children.len());
        for child_key in &self.children {
            let is_abs = arena
                .get(*child_key)
                .and_then(|node| {
                    node.element
                        .as_any()
                        .downcast_ref::<Element>()
                        .map(|el| el.computed_style.position.mode() == PositionMode::Absolute)
                })
                .unwrap_or(false);
            mask.push(is_abs);
        }
        mask
    }

    fn child_renders_outside_inner_clip(
        &self,
        index: usize,
        arena: &crate::view::node_arena::NodeArena,
    ) -> bool {
        self.children
            .get(index)
            .and_then(|k| arena.get(*k))
            .and_then(|node| {
                node.element.as_any().downcast_ref::<Element>().map(|el| {
                    if el.computed_style.position.mode() != PositionMode::Absolute {
                        return false;
                    }
                    match el.computed_style.position.clip_mode() {
                        ClipMode::Parent => false,
                        ClipMode::Viewport => true,
                        ClipMode::AnchorParent => {
                            el.computed_style.position.anchor_name().is_some()
                        }
                    }
                })
            })
            .unwrap_or(false)
    }

    fn recompute_absolute_descendant_for_hit_test(
        &mut self,
        arena: &crate::view::node_arena::NodeArena,
    ) {
        self.has_absolute_descendant_for_hit_test = self.children.iter().any(|child_key| {
            arena
                .get(*child_key)
                .and_then(|node| {
                    node.element
                        .as_any()
                        .downcast_ref::<Element>()
                        .map(|el| {
                            el.is_absolute_positioned_for_hit_test()
                                || el.has_absolute_descendant_for_hit_test
                        })
                })
                .unwrap_or(false)
        });
    }

    fn place_children(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        child_percent_base_width: Option<f32>,
        child_percent_base_height: Option<f32>,
        child_available_width: f32,
        child_available_height: f32,
        child_inner_width: f32,
        child_inner_height: f32,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let inherited_hit_test_clip = self.hit_test_clip_rect.unwrap_or(Rect {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width.max(0.0),
            height: self.layout_state.layout_size.height.max(0.0),
        });
        self.push_hit_test_clip_scope(intersect_rect(
            inherited_hit_test_clip,
            self.inner_clip_rect(),
        ));
        let overscan = Self::SHOULD_RENDER_OVERSCAN_PX.max(0.0);
        self.push_child_clip_scope(Rect {
            x: self.layout_state.layout_inner_position.x - overscan,
            y: self.layout_state.layout_inner_position.y - overscan,
            width: (self.layout_state.layout_inner_size.width + overscan * 2.0).max(0.0),
            height: (self.layout_state.layout_inner_size.height + overscan * 2.0).max(0.0),
        });
        let is_axis_layout = matches!(
            self.computed_style.layout,
            Layout::Inline | Layout::Flex { .. } | Layout::Flow { .. }
        );
        if is_axis_layout {
            let place_flex_started_at = Instant::now();
            self.place_flex_children(
                child_inner_width,
                child_inner_height,
                child_available_width,
                child_available_height,
                viewport_width,
                viewport_height,
                child_percent_base_width,
                child_percent_base_height,
                arena,
            );
            let place_flex_elapsed_ms =
                place_flex_started_at.elapsed().as_secs_f64() * 1000.0;
            LAYOUT_PLACE_PROFILE.with(|profile| {
                let mut profile = profile.borrow_mut();
                profile.place_flex_children_ms += place_flex_elapsed_ms;
                match self.computed_style.layout {
                    Layout::Inline => profile.place_layout_inline_ms += place_flex_elapsed_ms,
                    Layout::Flex { .. } => profile.place_layout_flex_ms += place_flex_elapsed_ms,
                    Layout::Flow { .. } => profile.place_layout_flow_ms += place_flex_elapsed_ms,
                    Layout::Grid => {}
                }
            });
        } else {
            let origin_x = self.layout_state.layout_flow_inner_position.x - self.scroll_offset.x;
            let origin_y = self.layout_state.layout_flow_inner_position.y - self.scroll_offset.y;
            let visual_offset_x = self.layout_state.layout_position.x - self.layout_state.layout_flow_position.x;
            let visual_offset_y = self.layout_state.layout_position.y - self.layout_state.layout_flow_position.y;
            let non_axis_children_started_at = Instant::now();
            let child_keys: Vec<crate::view::node_arena::NodeKey> = self.children.clone();
            // Build the is-absolute mask once: each call is arena.get +
            // RefCell::borrow + downcast, and this loop used to do it twice
            // per child (non-abs then abs pass), with more redundant calls
            // from update_content_size_from_children.
            let absolute_mask = self.compute_children_absolute_mask(arena);
            for (idx, child_key) in child_keys.iter().copied().enumerate() {
                if absolute_mask.get(idx).copied().unwrap_or(false) {
                    continue;
                }
                LAYOUT_PLACE_PROFILE.with(|profile| {
                    profile.borrow_mut().child_place_calls += 1;
                });
                arena.with_element_taken(child_key, |child, arena| {
                    child.place(
                        LayoutPlacement {
                            parent_x: origin_x,
                            parent_y: origin_y,
                            visual_offset_x,
                            visual_offset_y,
                            available_width: child_available_width,
                            available_height: child_available_height,
                            viewport_width,
                            viewport_height,
                            percent_base_width: child_percent_base_width,
                            percent_base_height: child_percent_base_height,
                        },
                        arena,
                    );
                });
            }
            let non_axis_children_elapsed_ms =
                non_axis_children_started_at.elapsed().as_secs_f64() * 1000.0;
            LAYOUT_PLACE_PROFILE.with(|profile| {
                profile.borrow_mut().non_axis_child_place_ms += non_axis_children_elapsed_ms;
            });
            let absolute_children_started_at = Instant::now();
            for (idx, child_key) in child_keys.iter().copied().enumerate() {
                if !absolute_mask.get(idx).copied().unwrap_or(false) {
                    continue;
                }
                LAYOUT_PLACE_PROFILE.with(|profile| {
                    let mut profile = profile.borrow_mut();
                    profile.child_place_calls += 1;
                    profile.absolute_child_place_calls += 1;
                });
                arena.with_element_taken(child_key, |child, arena| {
                    child.place(
                        LayoutPlacement {
                            parent_x: origin_x,
                            parent_y: origin_y,
                            visual_offset_x,
                            visual_offset_y,
                            available_width: child_available_width,
                            available_height: child_available_height,
                            viewport_width,
                            viewport_height,
                            percent_base_width: child_percent_base_width,
                            percent_base_height: child_percent_base_height,
                        },
                        arena,
                    );
                });
            }
            let absolute_children_elapsed_ms =
                absolute_children_started_at.elapsed().as_secs_f64() * 1000.0;
            LAYOUT_PLACE_PROFILE.with(|profile| {
                profile.borrow_mut().absolute_child_place_ms += absolute_children_elapsed_ms;
            });
        }
        // Mask is scoped per-branch above (axis layout builds its own);
        // recompute once here so both branches feed the helper without
        // paying for another per-child downcast inside it.
        let absolute_mask_for_content = self.compute_children_absolute_mask(arena);
        let update_content_size_started_at = Instant::now();
        self.update_content_size_from_children(arena, &absolute_mask_for_content);
        let update_content_size_elapsed_ms =
            update_content_size_started_at.elapsed().as_secs_f64() * 1000.0;
        LAYOUT_PLACE_PROFILE.with(|profile| {
            profile.borrow_mut().update_content_size_ms += update_content_size_elapsed_ms;
        });
        let clamp_scroll_started_at = Instant::now();
        self.clamp_scroll_offset();
        let clamp_scroll_elapsed_ms = clamp_scroll_started_at.elapsed().as_secs_f64() * 1000.0;
        LAYOUT_PLACE_PROFILE.with(|profile| {
            profile.borrow_mut().clamp_scroll_ms += clamp_scroll_elapsed_ms;
        });
        let recompute_hit_test_started_at = Instant::now();
        self.recompute_absolute_descendant_for_hit_test(arena);
        let recompute_hit_test_elapsed_ms =
            recompute_hit_test_started_at.elapsed().as_secs_f64() * 1000.0;
        LAYOUT_PLACE_PROFILE.with(|profile| {
            profile.borrow_mut().recompute_hit_test_ms += recompute_hit_test_elapsed_ms;
        });
        self.pop_child_clip_scope();
        self.pop_hit_test_clip_scope();
    }

    fn place_flex_children(
        &mut self,
        child_inner_width: f32,
        child_inner_height: f32,
        child_available_width: f32,
        child_available_height: f32,
        viewport_width: f32,
        viewport_height: f32,
        child_percent_base_width: Option<f32>,
        child_percent_base_height: Option<f32>,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        if self.children.is_empty() {
            return;
        }

        let is_row = matches!(
            self.computed_style.layout_axis_direction(),
            FlowDirection::Row
        );
        let main_limit = if is_row { child_inner_width } else { child_inner_height };
        let cross_limit = if is_row { child_inner_height } else { child_inner_width };
        let gap_base = if is_row { child_inner_width } else { child_inner_height };
        let gap = resolve_px(
            self.computed_style.gap,
            gap_base,
            viewport_width,
            viewport_height,
        );
        let origin_x = self.layout_state.layout_flow_inner_position.x - self.scroll_offset.x;
        let origin_y = self.layout_state.layout_flow_inner_position.y - self.scroll_offset.y;
        let visual_offset_x = self.layout_state.layout_position.x - self.layout_state.layout_flow_position.x;
        let visual_offset_y = self.layout_state.layout_position.y - self.layout_state.layout_flow_position.y;

        let absolute_mask = self.compute_children_absolute_mask(arena);
        let info = if let Some(cached) = self.flex_info.take() {
            cached
        } else {
            let is_real_flex = matches!(self.computed_style.layout, Layout::Flex { .. });
            let solver_wrap = !is_real_flex
                && matches!(self.computed_style.layout_flow_wrap(), FlowWrap::Wrap);
            crate::view::layout::flex_solver::compute_flex_info(
                crate::view::layout::flex_solver::FlexSolverInputs {
                    layout_kind: self.computed_style.layout,
                    children: &self.children,
                    absolute_mask: &absolute_mask,
                    is_row,
                    is_real_flex,
                    wrap: solver_wrap,
                    gap,
                    main_limit,
                    child_available_width,
                    child_available_height,
                    viewport_width,
                    viewport_height,
                    child_percent_base_width,
                    child_percent_base_height,
                },
                arena,
            )
        };

        crate::view::layout::place::place_axis_children(
            crate::view::layout::place::PlaceAxisChildrenInputs {
                layout: self.computed_style.layout,
                children: &self.children,
                flex_info: info,
                is_row,
                gap,
                main_limit,
                cross_limit,
                origin_x,
                origin_y,
                visual_offset_x,
                visual_offset_y,
                child_available_width,
                child_available_height,
                viewport_width,
                viewport_height,
                child_percent_base_width,
                child_percent_base_height,
                align: self.computed_style.layout_axis_align(),
                justify_content: self.computed_style.layout_axis_justify_content(),
                cross_size: self.computed_style.layout_axis_cross_size(),
            },
            arena,
        );

        crate::view::layout::place::place_absolute_children(
            crate::view::layout::place::PlaceAbsoluteChildrenInputs {
                children: &self.children,
                absolute_mask: &absolute_mask,
                origin_x,
                origin_y,
                visual_offset_x,
                visual_offset_y,
                child_available_width,
                child_available_height,
                viewport_width,
                viewport_height,
                child_percent_base_width,
                child_percent_base_height,
            },
            arena,
        );
    }
}
