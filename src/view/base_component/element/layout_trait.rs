impl Layoutable for Element {
    fn measure(&mut self, constraints: LayoutConstraints) {
        self.layout_assigned_width = None;
        self.layout_assigned_height = None;
        let context = constraints.context();
        let proposal = LayoutProposal {
            width: context.width,
            height: context.height,
            viewport_width: context.viewport_width,
            viewport_height: context.viewport_height,
            percent_base_width: context.percent_base_width,
            percent_base_height: context.percent_base_height,
        };

        if !self.layout_dirty && self.last_layout_proposal == Some(proposal) {
            return;
        }

        self.measure_self(proposal);
        self.apply_size_constraints(proposal, false);

        // We should always measure children because they might be Auto or use Percent units
        // that depend on our inner size.
        let is_axis_layout = matches!(
            self.computed_style.layout,
            Layout::Inline | Layout::Flex { .. } | Layout::Flow { .. }
        );
        if is_axis_layout {
            self.measure_flex_children(proposal);
        } else {
            let bw_l = resolve_px_or_zero(
                self.computed_style.border_widths.left,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let bw_r = resolve_px_or_zero(
                self.computed_style.border_widths.right,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let bw_t = resolve_px_or_zero(
                self.computed_style.border_widths.top,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let bw_b = resolve_px_or_zero(
                self.computed_style.border_widths.bottom,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );

            let p_l = resolve_px_or_zero(
                self.computed_style.padding.left,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let p_r = resolve_px_or_zero(
                self.computed_style.padding.right,
                proposal.percent_base_width,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let p_t = resolve_px_or_zero(
                self.computed_style.padding.top,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );
            let p_b = resolve_px_or_zero(
                self.computed_style.padding.bottom,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );

            let (layout_w, layout_h) = self.current_layout_transition_size();
            let measure_w = if self.computed_style.width == SizeValue::Auto
                && proposal.percent_base_width.is_some()
            {
                proposal.width.max(0.0)
            } else {
                layout_w
            };
            let measure_h = if self.computed_style.height == SizeValue::Auto
                && self.height_is_known(proposal)
            {
                proposal.height.max(0.0)
            } else {
                layout_h
            };
            let inner_w = (measure_w - bw_l - bw_r - p_l - p_r).max(0.0);
            let inner_h = (measure_h - bw_t - bw_b - p_t - p_b).max(0.0);

            let (child_available_width, child_available_height) = match self.scroll_direction {
                ScrollDirection::None => (inner_w, inner_h),
                ScrollDirection::Vertical => (inner_w, 1_000_000.0),
                ScrollDirection::Horizontal => (1_000_000.0, inner_h),
                ScrollDirection::Both => (1_000_000.0, 1_000_000.0),
            };

            let child_percent_base_width = if self.width_is_known(proposal) {
                Some(inner_w)
            } else {
                None
            };
            let child_percent_base_height = if self.height_is_known(proposal) {
                Some(inner_h)
            } else {
                None
            };

            for child in &mut self.children {
                child.measure(LayoutConstraints {
                    max_width: child_available_width,
                    max_height: child_available_height,
                    viewport_width: proposal.viewport_width,
                    viewport_height: proposal.viewport_height,
                    percent_base_width: child_percent_base_width,
                    percent_base_height: child_percent_base_height,
                });
            }

            if self.computed_style.width == SizeValue::Auto
                || self.computed_style.height == SizeValue::Auto
            {
                self.update_size_from_measured_children();
            }
        }
        self.apply_size_constraints(proposal, true);

        self.last_layout_proposal = Some(proposal);
        self.layout_dirty = false;
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::LAYOUT);
    }

    fn place(&mut self, placement: LayoutPlacement) {
        let child_dirty_flags = self
            .children
            .iter()
            .fold(DirtyFlags::NONE, |flags, child| {
                flags.union(crate::view::base_component::subtree_dirty_flags(child.as_ref()))
            });
        let runtime_dirty = self.dirty_flags.union(child_dirty_flags);
        if !runtime_dirty.intersects(
            DirtyFlags::PLACE.union(DirtyFlags::BOX_MODEL).union(DirtyFlags::HIT_TEST),
        ) && self.last_layout_placement == Some(placement)
        {
            return;
        }

        self.begin_place_scope(placement);
        LAYOUT_PLACE_PROFILE.with(|profile| {
            profile.borrow_mut().node_count += 1;
        });
        let context = placement.context();
        let proposal = LayoutProposal {
            width: context.width,
            height: context.height,
            viewport_width: context.viewport_width,
            viewport_height: context.viewport_height,
            percent_base_width: context.percent_base_width,
            percent_base_height: context.percent_base_height,
        };
        self.resolve_lengths_from_parent_inner(proposal);
        let place_self_started_at = Instant::now();
        self.place_self(
            proposal,
            placement.parent_x,
            placement.parent_y,
            placement.visual_offset_x,
            placement.visual_offset_y,
        );
        let place_self_elapsed_ms = place_self_started_at.elapsed().as_secs_f64() * 1000.0;
        LAYOUT_PLACE_PROFILE.with(|profile| {
            profile.borrow_mut().place_self_ms += place_self_elapsed_ms;
        });
        self.register_anchor_snapshot();
        self.resolve_corner_radii_from_self_box(proposal);
        let max_bw = (self
            .core
            .layout_size
            .width
            .min(self.core.layout_size.height))
            * 0.5;
        let border_left = self.border_widths.left.clamp(0.0, max_bw);
        let border_right = self.border_widths.right.clamp(0.0, max_bw);
        let border_top = self.border_widths.top.clamp(0.0, max_bw);
        let border_bottom = self.border_widths.bottom.clamp(0.0, max_bw);
        let inset_left = border_left + self.padding.left.max(0.0);
        let inset_right = border_right + self.padding.right.max(0.0);
        let inset_top = border_top + self.padding.top.max(0.0);
        let inset_bottom = border_bottom + self.padding.bottom.max(0.0);
        self.layout_flow_inner_position = Position {
            x: self.layout_flow_position.x + inset_left,
            y: self.layout_flow_position.y + inset_top,
        };
        self.layout_inner_position = Position {
            x: self.core.layout_position.x + inset_left,
            y: self.core.layout_position.y + inset_top,
        };
        self.layout_inner_size = Size {
            width: (self.core.layout_size.width - inset_left - inset_right).max(0.0),
            height: (self.core.layout_size.height - inset_top - inset_bottom).max(0.0),
        };

        let is_axis_layout = matches!(
            self.computed_style.layout,
            Layout::Inline | Layout::Flex { .. } | Layout::Flow { .. }
        );
        let child_layout_inner_size = if is_axis_layout {
            let (target_w, target_h) = self.current_layout_target_size();
            Size {
                width: (target_w - inset_left - inset_right).max(0.0),
                height: (target_h - inset_top - inset_bottom).max(0.0),
            }
        } else {
            self.layout_inner_size
        };

        let child_percent_base_width = if self.width_is_known(proposal) {
            Some(child_layout_inner_size.width.max(0.0))
        } else {
            None
        };
        let child_percent_base_height = if self.height_is_known(proposal) {
            Some(child_layout_inner_size.height.max(0.0))
        } else {
            None
        };
        let (child_available_width, child_available_height) = self
            .child_layout_limits_for_inner_size(
                child_layout_inner_size.width,
                child_layout_inner_size.height,
            );
        let place_children_started_at = Instant::now();
        self.place_children(
            proposal.viewport_width,
            proposal.viewport_height,
            child_percent_base_width,
            child_percent_base_height,
            child_available_width,
            child_available_height,
            child_layout_inner_size.width,
            child_layout_inner_size.height,
        );
        let place_children_elapsed_ms =
            place_children_started_at.elapsed().as_secs_f64() * 1000.0;
        LAYOUT_PLACE_PROFILE.with(|profile| {
            profile.borrow_mut().place_children_ms += place_children_elapsed_ms;
        });
        self.end_place_scope();
        self.last_layout_placement = Some(placement);
        self.dirty_flags = self
            .dirty_flags
            .without(DirtyFlags::PLACE.union(DirtyFlags::BOX_MODEL).union(DirtyFlags::HIT_TEST));
    }

    fn measured_size(&self) -> (f32, f32) {
        self.current_layout_transition_size()
    }

    fn set_layout_width(&mut self, width: f32) {
        let width = width.max(0.0);
        if self.layout_assigned_width != Some(width) {
            self.layout_assigned_width = Some(width);
            self.mark_place_dirty();
        }
    }

    fn set_layout_height(&mut self, height: f32) {
        let height = height.max(0.0);
        if self.layout_assigned_height != Some(height) {
            self.layout_assigned_height = Some(height);
            self.mark_place_dirty();
        }
    }

    fn allows_cross_stretch(&self, is_row: bool) -> bool {
        if is_row {
            self.computed_style.height == SizeValue::Auto
        } else {
            self.computed_style.width == SizeValue::Auto
        }
    }

    fn cross_alignment_size(&self, is_row: bool, stretched_cross: Option<f32>) -> f32 {
        let current_cross = if is_row {
            self.core.layout_size.height.max(0.0)
        } else {
            self.core.layout_size.width.max(0.0)
        };
        let target_cross = if is_row {
            self.layout_assigned_height
        } else {
            self.layout_assigned_width
        };
        let has_cross_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            if is_row {
                matches!(t.property, TransitionProperty::All | TransitionProperty::Height)
            } else {
                matches!(t.property, TransitionProperty::All | TransitionProperty::Width)
            }
        });
        let transition_active = if is_row {
            self.layout_transition_override_height.is_some()
        } else {
            self.layout_transition_override_width.is_some()
        };
        let transition_will_start = has_cross_transition
            && target_cross.is_some_and(|target| !approx_eq(target.max(0.0), current_cross));
        if transition_active || transition_will_start {
            current_cross
        } else {
            stretched_cross.unwrap_or(current_cross)
        }
    }

    fn flex_grow(&self) -> f32 {
        self.computed_style.flex_grow
    }

    fn flex_shrink(&self) -> f32 {
        self.computed_style.flex_shrink
    }

    fn flex_basis(&self) -> SizeValue {
        self.computed_style.flex_basis
    }

    fn flex_main_size(&self, is_row: bool) -> SizeValue {
        if is_row {
            self.computed_style.width
        } else {
            self.computed_style.height
        }
    }

    fn flex_has_explicit_min_main_size(&self, is_row: bool) -> bool {
        let property = if is_row {
            crate::style::PropertyId::MinWidth
        } else {
            crate::style::PropertyId::MinHeight
        };
        self.parsed_style.get(property).is_some()
    }

    fn flex_auto_min_main_size(&self, is_row: bool) -> Option<f32> {
        if self.flex_has_explicit_min_main_size(is_row)
            || self.flex_main_size(is_row) != SizeValue::Auto
        {
            return None;
        }
        let (measured_w, measured_h) = self.measured_size();
        Some(if is_row { measured_w } else { measured_h }.max(0.0))
    }

    fn flex_min_main_size(&self, is_row: bool) -> SizeValue {
        if is_row {
            self.computed_style.min_width
        } else {
            self.computed_style.min_height
        }
    }

    fn flex_max_main_size(&self, is_row: bool) -> SizeValue {
        if is_row {
            self.computed_style.max_width
        } else {
            self.computed_style.max_height
        }
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (self.core.position.x, self.core.position.y)
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        if (self.core.position.x - x).abs() > f32::EPSILON
            || (self.core.position.y - y).abs() > f32::EPSILON
        {
            self.core.set_position(x, y);
            self.mark_place_dirty();
        }
    }

    fn measure_inline(&mut self, context: InlineMeasureContext) {
        if self.is_fragmentable_inline_element()
        {
            self.pending_inline_measure_context = Some(context);
            self.measure(LayoutConstraints {
                max_width: context.full_available_width.max(0.0),
                max_height: 1_000_000.0,
                viewport_width: context.viewport_width,
                viewport_height: context.viewport_height,
                percent_base_width: context.percent_base_width,
                percent_base_height: context.percent_base_height,
            });
            self.pending_inline_measure_context = None;
        } else {
            self.measure(LayoutConstraints {
                max_width: context.first_available_width.max(0.0),
                max_height: 1_000_000.0,
                viewport_width: context.viewport_width,
                viewport_height: context.viewport_height,
                percent_base_width: context.percent_base_width,
                percent_base_height: context.percent_base_height,
            });
        }
    }

    fn get_inline_nodes_size(&self) -> Vec<InlineNodeSize> {
        if self.is_fragmentable_inline_element()
        {
            let mut nodes = Vec::new();
            for (idx, child) in self.children.iter().enumerate() {
                if self.child_is_absolute(idx) {
                    continue;
                }
                nodes.extend(child.get_inline_nodes_size());
            }
            let left_inset = (self.border_widths.left + self.padding.left).max(0.0);
            let right_inset = (self.border_widths.right + self.padding.right).max(0.0);
            if let Some(first) = nodes.first_mut() {
                first.width += left_inset;
            }
            if let Some(last) = nodes.last_mut() {
                last.width += right_inset;
            }
            return nodes;
        }
        let (width, height) = self.measured_size();
        vec![InlineNodeSize { width, height }]
    }

    fn place_inline(&mut self, placement: InlinePlacement) {
        if self.is_fragmentable_inline_element()
        {
            let left_inset = (self.border_widths.left + self.padding.left).max(0.0);
            let right_inset = (self.border_widths.right + self.padding.right).max(0.0);
            let top_inset = (self.border_widths.top + self.padding.top).max(0.0);
            let bottom_inset = (self.border_widths.bottom + self.padding.bottom).max(0.0);
            let mut current = 0_usize;
            let mut total_nodes = 0_usize;
            let mut target: Option<(usize, usize, f32, f32)> = None;
            for (child_idx, child) in self.children.iter().enumerate() {
                if self.child_is_absolute(child_idx) {
                    continue;
                }
                let nodes = child.get_inline_nodes_size();
                total_nodes += nodes.len();
                if target.is_none() && placement.node_index < current + nodes.len() {
                    let local_index = placement.node_index - current;
                    let node = nodes[local_index];
                    target = Some((child_idx, local_index, node.width, node.height));
                }
                current += nodes.len();
            }

            if placement.node_index == 0 {
                self.inline_paint_fragments.clear();
                self.layout_flow_position = Position {
                    x: placement.x,
                    y: placement.y,
                };
                self.core.layout_position = Position {
                    x: placement.x,
                    y: placement.y,
                };
                self.core.layout_size = Size {
                    width: 0.0,
                    height: 0.0,
                };
                self.layout_inner_position = self.core.layout_position;
                self.layout_flow_inner_position = self.layout_flow_position;
                self.layout_inner_size = self.core.layout_size;
                self.core.should_render = false;
            }

            let Some((child_idx, local_index, width, height)) = target else {
                return;
            };
            let is_first_fragment = placement.node_index == 0;
            let is_last_fragment = placement.node_index + 1 == total_nodes;
            self.children[child_idx].place_inline(InlinePlacement {
                x: placement.x + if is_first_fragment { left_inset } else { 0.0 },
                y: placement.y,
                node_index: local_index,
                ..placement
            });

            let left = placement.x;
            let top = placement.y - top_inset;
            let outer_width = width.max(0.0)
                + if is_first_fragment { left_inset } else { 0.0 }
                + if is_last_fragment { right_inset } else { 0.0 };
            let outer_height = height.max(0.0) + top_inset + bottom_inset;
            let right = placement.x + outer_width;
            let bottom = top + outer_height;
            let should_extend_existing = self
                .inline_paint_fragments
                .last()
                .is_some_and(|fragment| (fragment.y - top).abs() < 0.5);
            if should_extend_existing {
                if let Some(fragment) = self.inline_paint_fragments.last_mut() {
                    let fragment_right = fragment.x + fragment.width;
                    let fragment_bottom = fragment.y + fragment.height;
                    fragment.x = fragment.x.min(left);
                    fragment.y = fragment.y.min(top);
                    fragment.width = fragment_right.max(right) - fragment.x;
                    fragment.height = fragment_bottom.max(bottom) - fragment.y;
                }
            } else {
                self.inline_paint_fragments.push(Rect {
                    x: left,
                    y: top,
                    width: (right - left).max(0.0),
                    height: (bottom - top).max(0.0),
                });
            }
            if self.core.should_render {
                let current_right = self.core.layout_position.x + self.core.layout_size.width;
                let current_bottom = self.core.layout_position.y + self.core.layout_size.height;
                self.core.layout_position.x = self.core.layout_position.x.min(left);
                self.core.layout_position.y = self.core.layout_position.y.min(top);
                self.layout_flow_position = self.core.layout_position;
                self.core.layout_size.width = current_right.max(right) - self.core.layout_position.x;
                self.core.layout_size.height =
                    current_bottom.max(bottom) - self.core.layout_position.y;
            } else {
                self.core.layout_position = Position { x: left, y: top };
                self.layout_flow_position = self.core.layout_position;
                self.core.layout_size = Size {
                    width: (right - left).max(0.0),
                    height: (bottom - top).max(0.0),
                };
            }
            self.layout_inner_position = self.core.layout_position;
            self.layout_flow_inner_position = self.layout_flow_position;
            self.layout_inner_size = self.core.layout_size;
            self.content_size = self.core.layout_size;
            self.core.should_render =
                self.core.layout_size.width > 0.0 && self.core.layout_size.height > 0.0;
            self.dirty_flags = self
                .dirty_flags
                .without(DirtyFlags::PLACE.union(DirtyFlags::BOX_MODEL).union(DirtyFlags::HIT_TEST));
        } else {
            self.set_layout_offset(placement.offset_x, placement.offset_y);
            self.place(LayoutPlacement {
                parent_x: placement.parent_x,
                parent_y: placement.parent_y,
                visual_offset_x: placement.visual_offset_x,
                visual_offset_y: placement.visual_offset_y,
                available_width: placement.available_width,
                available_height: placement.available_height,
                viewport_width: placement.viewport_width,
                viewport_height: placement.viewport_height,
                percent_base_width: placement.percent_base_width,
                percent_base_height: placement.percent_base_height,
            });
        }
    }
}
