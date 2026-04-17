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

        let child_layout_dirty = self.children.iter().any(|child| {
            crate::view::base_component::subtree_dirty_flags(child.as_ref())
                .intersects(DirtyFlags::LAYOUT)
        });
        let inline_measure_context_changed = self.is_fragmentable_inline_element()
            && self.pending_inline_measure_context != self.last_inline_measure_context;

        if !self.layout_dirty
            && !child_layout_dirty
            && !inline_measure_context_changed
            && self.last_layout_proposal == Some(proposal)
        {
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
        self.last_inline_measure_context = self.pending_inline_measure_context;
        self.layout_dirty = false;
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::LAYOUT);
    }

    fn place(&mut self, placement: LayoutPlacement) {
        let child_dirty_flags = self.children.iter().fold(DirtyFlags::NONE, |flags, child| {
            flags.union(crate::view::base_component::subtree_dirty_flags(
                child.as_ref(),
            ))
        });
        let runtime_dirty = self.dirty_flags.union(child_dirty_flags);
        if !runtime_dirty.intersects(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
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
        let place_children_elapsed_ms = place_children_started_at.elapsed().as_secs_f64() * 1000.0;
        LAYOUT_PLACE_PROFILE.with(|profile| {
            profile.borrow_mut().place_children_ms += place_children_elapsed_ms;
        });
        self.end_place_scope();
        self.last_layout_placement = Some(placement);
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
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

    fn cross_alignment_size(&self, is_row: bool, stretched_cross: Option<f32>) -> f32 {
        let rendered_cross = if is_row {
            self.core.layout_size.height.max(0.0)
        } else {
            self.core.layout_size.width.max(0.0)
        };
        let (measured_w, measured_h) = self.measured_size();
        let measured_cross = if is_row { measured_h } else { measured_w }.max(0.0);
        let target_cross = if is_row {
            self.layout_assigned_height
        } else {
            self.layout_assigned_width
        };
        let has_cross_transition = self.computed_style.transition.as_slice().iter().any(|t| {
            if is_row {
                matches!(
                    t.property,
                    TransitionProperty::All | TransitionProperty::Height
                )
            } else {
                matches!(
                    t.property,
                    TransitionProperty::All | TransitionProperty::Width
                )
            }
        });
        let transition_active = if is_row {
            self.layout_transition_override_height.is_some()
        } else {
            self.layout_transition_override_width.is_some()
        };
        let transition_will_start = has_cross_transition
            && target_cross.is_some_and(|target| !approx_eq(target.max(0.0), rendered_cross));
        if transition_active || transition_will_start {
            rendered_cross
        } else {
            stretched_cross.unwrap_or(target_cross.unwrap_or(measured_cross))
        }
    }

    fn flex_props(&self) -> crate::view::base_component::FlexProps {
        let (measured_w, measured_h) = self.measured_size();
        crate::view::base_component::FlexProps {
            grow: self.computed_style.flex_grow,
            shrink: self.computed_style.flex_shrink,
            basis: self.computed_style.flex_basis,
            width: self.computed_style.width,
            height: self.computed_style.height,
            min_width: self.computed_style.min_width,
            min_height: self.computed_style.min_height,
            max_width: self.computed_style.max_width,
            max_height: self.computed_style.max_height,
            has_explicit_min_width: self
                .parsed_style
                .get(crate::style::PropertyId::MinWidth)
                .is_some(),
            has_explicit_min_height: self
                .parsed_style
                .get(crate::style::PropertyId::MinHeight)
                .is_some(),
            allows_cross_stretch_when_row: self.computed_style.height == SizeValue::Auto,
            allows_cross_stretch_when_col: self.computed_style.width == SizeValue::Auto,
            intrinsic_width: Some(measured_w),
            intrinsic_height: Some(measured_h),
            intrinsic_feeds_auto_min: true,
            intrinsic_feeds_auto_base: false,
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
        if self.is_fragmentable_inline_element() {
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
        if self.is_fragmentable_inline_element() {
            let left_inset = (self.border_widths.left + self.padding.left).max(0.0);
            let right_inset = (self.border_widths.right + self.padding.right).max(0.0);
            let inline_child_count = self
                .children
                .iter()
                .enumerate()
                .filter(|(idx, _)| !self.child_is_absolute(*idx))
                .count();
            if inline_child_count > 1 {
                if let Some(info) = self.flex_info.as_ref() {
                    return info
                        .lines
                        .iter()
                        .enumerate()
                        .map(|(line_idx, _)| InlineNodeSize {
                            width: info.line_main_sum[line_idx].max(0.0)
                                + if line_idx == 0 { left_inset } else { 0.0 }
                                + if line_idx + 1 == info.lines.len() {
                                    right_inset
                                } else {
                                    0.0
                                },
                            // Match CSS inline formatting: vertical padding/border paints
                            // outside the line box and must not increase line height.
                            height: info.line_cross_max[line_idx].max(0.0),
                        })
                        .collect();
                }
                return Vec::new();
            }
            let mut nodes = Vec::new();
            for (idx, child) in self.children.iter().enumerate() {
                if self.child_is_absolute(idx) {
                    continue;
                }
                nodes.extend(child.get_inline_nodes_size());
            }
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
        if self.is_fragmentable_inline_element() {
            let left_inset = (self.border_widths.left + self.padding.left).max(0.0);
            let right_inset = (self.border_widths.right + self.padding.right).max(0.0);
            let top_inset = (self.border_widths.top + self.padding.top).max(0.0);
            let bottom_inset = (self.border_widths.bottom + self.padding.bottom).max(0.0);

            if placement.node_index == 0 {
                self.inline_paint_fragments.clear();
                self.layout_flow_position = Position {
                    x: placement.x,
                    y: placement.y,
                };
                self.core.layout_position = self.layout_flow_position;
                self.core.layout_size = Size {
                    width: 0.0,
                    height: 0.0,
                };
                self.layout_inner_position = self.core.layout_position;
                self.layout_flow_inner_position = self.layout_flow_position;
                self.layout_inner_size = self.core.layout_size;
                self.core.should_render = false;
            }

            let inline_child_count = self
                .children
                .iter()
                .enumerate()
                .filter(|(idx, _)| !self.child_is_absolute(*idx))
                .count();
            let is_row = matches!(
                self.computed_style.layout_axis_direction(),
                FlowDirection::Row
            );
            let align = self.computed_style.layout_axis_align();

            let (line_width, line_height, total_nodes) = if inline_child_count > 1 {
                let Some(info) = self.flex_info.as_ref() else {
                    return;
                };
                let total_nodes = info.lines.len();
                let Some(line) = info.lines.get(placement.node_index) else {
                    return;
                };
                let line_width = info
                    .line_main_sum
                    .get(placement.node_index)
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0);
                let line_height = info
                    .line_cross_max
                    .get(placement.node_index)
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0);
                let mut main_cursor = 0.0_f32;
                let mut prev_child_index: Option<usize> = None;
                let gap = resolve_px(
                    self.computed_style.gap,
                    line_width.max(0.0),
                    placement.viewport_width,
                    placement.viewport_height,
                );

                for item in line {
                    if prev_child_index != Some(item.child_index) && prev_child_index.is_some() {
                        main_cursor += gap;
                    }
                    let align_offset =
                        cross_item_offset(line_height, item.cross.max(0.0), align);
                    let content_origin_x = placement.x
                        + if placement.node_index == 0 {
                            left_inset
                        } else {
                            0.0
                        };
                    let (x, y) = if is_row {
                        (
                            content_origin_x + main_cursor + item.main_offset,
                            placement.y + align_offset + item.cross_offset,
                        )
                    } else {
                        (
                            placement.x + align_offset + item.cross_offset,
                            placement.y + main_cursor + item.main_offset,
                        )
                    };
                    self.children[item.child_index].place_inline(InlinePlacement {
                        x,
                        y,
                        node_index: item.node_index,
                        ..placement
                    });
                    main_cursor += item.main.max(0.0);
                    prev_child_index = Some(item.child_index);
                }

                (line_width, line_height, total_nodes)
            } else {
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
                let Some((child_idx, local_index, width, height)) = target else {
                    return;
                };
                self.children[child_idx].place_inline(InlinePlacement {
                    x: placement.x
                        + if placement.node_index == 0 {
                            left_inset
                        } else {
                            0.0
                        },
                    y: placement.y,
                    node_index: local_index,
                    ..placement
                });
                (width.max(0.0), height.max(0.0), total_nodes)
            };

            let is_first_fragment = placement.node_index == 0;
            let is_last_fragment = placement.node_index + 1 == total_nodes;
            let left = placement.x;
            let top = placement.y - top_inset;
            let outer_width = line_width
                + if is_first_fragment { left_inset } else { 0.0 }
                + if is_last_fragment { right_inset } else { 0.0 };
            let outer_height = line_height + top_inset + bottom_inset;
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
                self.core.layout_size.width =
                    current_right.max(right) - self.core.layout_position.x;
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
            self.dirty_flags = self.dirty_flags.without(
                DirtyFlags::PLACE
                    .union(DirtyFlags::BOX_MODEL)
                    .union(DirtyFlags::HIT_TEST),
            );
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
