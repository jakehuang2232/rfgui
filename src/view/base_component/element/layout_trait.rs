fn record_refreshed_layout_gate_child_candidates(
    children: &[crate::view::node_arena::NodeKey],
    arena: &crate::view::node_arena::NodeArena,
    mask: DirtyFlags,
    phase: LayoutGateCandidatePhase,
) -> bool {
    // The gate reads the cached subtree dirty aggregates refreshed once
    // per pass at the roots (viewport render.rs / test_support). A full
    // per-child subtree refresh here made every traversal O(n × depth):
    // each node was re-aggregated once per ancestor. Mid-pass dirty
    // changes only clear bits in this pass's mask or add bits outside it
    // (PAINT/RUNTIME), so a stale cache is conservatively dirty — it can
    // trigger a redundant place, never skip a required one.
    record_layout_gate_child_candidates(children, arena, mask, phase)
}

fn measure_inline_ifc_root_child(
    child_key: crate::view::node_arena::NodeKey,
    constraints: LayoutConstraints,
    arena: &mut crate::view::node_arena::NodeArena,
) {
    arena.with_element_taken(child_key, |child, arena| {
        if child.as_any().is::<Text>() {
            child.clear_local_dirty_flags(DirtyPassMask::LAYOUT);
            return;
        }

        child.measure(constraints, arena);
    });
}

fn measure_inline_ifc_root_children(
    children: &[crate::view::node_arena::NodeKey],
    absolute_mask: &[bool],
    constraints: LayoutConstraints,
    arena: &mut crate::view::node_arena::NodeArena,
) {
    for (child_index, child_key) in children.iter().copied().enumerate() {
        if absolute_mask.get(child_index).copied().unwrap_or(false) {
            arena.with_element_taken(child_key, |child, arena| {
                child.measure(constraints, arena);
            });
            continue;
        }

        measure_inline_ifc_root_child(child_key, constraints, arena);
    }
}

impl Layoutable for Element {
    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
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

        // Read the pre-computed per-frame cache instead of walking the
        // entire subtree per child. The cache is refreshed by
        // `NodeArena::refresh_subtree_dirty_cache` at the top of each
        // layout pass (see `viewport/render.rs::run_layout_pass`).
        let child_layout_dirty = record_refreshed_layout_gate_child_candidates(
            &self.children,
            arena,
            DirtyPassMask::LAYOUT,
            LayoutGateCandidatePhase::Measure,
        );
        if !self.layout_dirty && !child_layout_dirty && self.last_layout_proposal == Some(proposal)
        {
            return;
        }

        with_layout_place_profile(|p| {
            if self.layout_dirty {
                p.measure_ran_self_dirty += 1;
            } else if child_layout_dirty {
                p.measure_ran_child_dirty += 1;
            } else {
                p.measure_ran_proposal_changed += 1;
                if let Some(old) = self.last_layout_proposal {
                    if old.width != proposal.width || old.height != proposal.height {
                        p.proposal_changed_size += 1;
                    }
                    if old.viewport_width != proposal.viewport_width
                        || old.viewport_height != proposal.viewport_height
                    {
                        p.proposal_changed_viewport += 1;
                    }
                    if old.percent_base_width != proposal.percent_base_width
                        || old.percent_base_height != proposal.percent_base_height
                    {
                        p.proposal_changed_percent_base += 1;
                    }
                } else {
                    p.proposal_changed_first += 1;
                }
            }
        });

        self.measure_self(proposal);
        self.apply_size_constraints(proposal, false);

        // We should always measure children because they might be Auto or use Percent units
        // that depend on our inner size.
        let is_axis_layout = matches!(
            self.computed_style.layout,
            Layout::Flex { .. } | Layout::Flow { .. }
        );
        if is_axis_layout {
            self.measure_flex_children(proposal, arena);
        } else {
            let insets = resolve_layout_insets(
                &self.computed_style.border_widths,
                &self.computed_style.padding,
                proposal.percent_base_width,
                proposal.percent_base_height,
                proposal.viewport_width,
                proposal.viewport_height,
            );

            let sizes = self.resolve_layout_sizes(proposal);
            let layout_w = sizes.target.width;
            let layout_h = sizes.target.height;
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
            let inner_w = (measure_w - insets.horizontal()).max(0.0);
            let inner_h = (measure_h - insets.vertical()).max(0.0);

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

            let child_constraints = LayoutConstraints {
                max_width: child_available_width,
                max_height: child_available_height,
                viewport_width: proposal.viewport_width,
                viewport_height: proposal.viewport_height,
                percent_base_width: child_percent_base_width,
                percent_base_height: child_percent_base_height,
            };
            if self.computed_style.layout == Layout::Inline && !self.inline_ifc_owned_by_root {
                let absolute_mask = self.compute_children_absolute_mask(arena);
                measure_inline_ifc_root_children(
                    &self.children,
                    &absolute_mask,
                    child_constraints,
                    arena,
                );
            } else {
                let child_keys: Vec<crate::view::node_arena::NodeKey> = self.children.clone();
                for child_key in child_keys {
                    arena.with_element_taken(child_key, |child, arena| {
                        child.measure(child_constraints, arena);
                    });
                }
            }

            if self.computed_style.width == SizeValue::Auto
                || self.computed_style.height == SizeValue::Auto
            {
                let mask = self.compute_children_absolute_mask(arena);
                self.update_size_from_measured_children(arena, &mask);
            }

            // Inline IFC root: the shaped line stack, not the per-child
            // union, is the auto size of this box.
            if self.computed_style.layout == Layout::Inline && !self.inline_ifc_owned_by_root {
                if let Some((content_w, content_h)) =
                    self.measure_inline_ifc_root_content_size(
                        arena,
                        inner_w,
                        proposal.viewport_width,
                        proposal.viewport_height,
                    )
                    && (content_w > 0.0 || content_h > 0.0)
                {
                    self.layout_state.content_size = Size {
                        width: content_w,
                        height: content_h,
                    };
                    if self.computed_style.width == SizeValue::Auto {
                        self.core.set_width(content_w + insets.horizontal());
                    }
                    if self.computed_style.height == SizeValue::Auto {
                        self.core.set_height(content_h + insets.vertical());
                    }
                }
            }
        }
        self.apply_size_constraints(proposal, true);

        self.last_layout_proposal = Some(proposal);
        self.layout_dirty = false;
        self.dirty_flags = self.dirty_flags.without(DirtyPassMask::LAYOUT);
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        // O(1) cache read per child; see refresh_subtree_dirty_cache.
        let placement_dirty_mask = DirtyPassMask::PLACEMENT;
        let child_placement_dirty = record_refreshed_layout_gate_child_candidates(
            &self.children,
            arena,
            placement_dirty_mask,
            LayoutGateCandidatePhase::Placement,
        );
        let inline_ifc_layout_call_site_dirty =
            self.inline_ifc_layout_call_site_dirty_gate(arena, placement);
        if !self.dirty_flags.intersects(placement_dirty_mask)
            && !child_placement_dirty
            && !inline_ifc_layout_call_site_dirty
            && self.last_layout_placement == Some(placement)
            && self.hit_test_clip_matches_current_placement(placement)
            && (self.children.is_empty()
                || rect_approx_eq(
                    self.last_child_hit_test_clip_rect,
                    Some(self.current_child_hit_test_clip_rect()),
                ))
        {
            return;
        }

        self.begin_place_scope(placement);
        with_layout_place_profile(|profile| {
            profile.node_count += 1;
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
        profile_layout_place_time(LayoutPlaceTiming::PlaceSelf, || {
            self.place_self(
                proposal,
                placement.parent_x,
                placement.parent_y,
                placement.visual_offset_x,
                placement.visual_offset_y,
            );
        });
        self.register_anchor_snapshot();
        self.push_ancestor_anchor_scope();
        self.resolve_corner_radii_from_self_box(proposal);
        let max_bw = (self
            .layout_state
            .layout_size
            .width
            .min(self.layout_state.layout_size.height))
            * 0.5;
        let border_left = self.border_widths.left.clamp(0.0, max_bw);
        let border_right = self.border_widths.right.clamp(0.0, max_bw);
        let border_top = self.border_widths.top.clamp(0.0, max_bw);
        let border_bottom = self.border_widths.bottom.clamp(0.0, max_bw);
        let inset_left = border_left + self.padding.left.max(0.0);
        let inset_right = border_right + self.padding.right.max(0.0);
        let inset_top = border_top + self.padding.top.max(0.0);
        let inset_bottom = border_bottom + self.padding.bottom.max(0.0);
        self.layout_state.layout_flow_inner_position = Position {
            x: self.layout_state.layout_flow_position.x + inset_left,
            y: self.layout_state.layout_flow_position.y + inset_top,
        };
        self.layout_state.layout_inner_position = Position {
            x: self.layout_state.layout_position.x + inset_left,
            y: self.layout_state.layout_position.y + inset_top,
        };
        self.layout_state.layout_inner_size = Size {
            width: (self.layout_state.layout_size.width - inset_left - inset_right).max(0.0),
            height: (self.layout_state.layout_size.height - inset_top - inset_bottom).max(0.0),
        };

        let is_axis_layout = matches!(
            self.computed_style.layout,
            Layout::Inline | Layout::Flex { .. } | Layout::Flow { .. }
        );
        let child_layout_inner_size = if is_axis_layout {
            let sizes = self.resolve_layout_sizes(proposal);
            Size {
                width: (sizes.axis_place_constraint.width - inset_left - inset_right).max(0.0),
                height: (sizes.axis_place_constraint.height - inset_top - inset_bottom).max(0.0),
            }
        } else {
            self.layout_state.layout_inner_size
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
        profile_layout_place_time(LayoutPlaceTiming::PlaceChildren, || {
            self.place_children(
                proposal.viewport_width,
                proposal.viewport_height,
                child_percent_base_width,
                child_percent_base_height,
                child_available_width,
                child_available_height,
                child_layout_inner_size.width,
                child_layout_inner_size.height,
                arena,
            );
        });
        profile_layout_place_time(LayoutPlaceTiming::InlineIfcRootInstall, || {
            self.run_inline_ifc_root_after_place(arena, placement, child_layout_inner_size.width);
        });
        self.pop_ancestor_anchor_scope();
        self.end_place_scope();
        self.last_layout_placement = Some(placement);
        self.dirty_flags = self.dirty_flags.without(DirtyPassMask::PLACEMENT);
    }

    fn measured_size(&self) -> (f32, f32) {
        let size = self.current_parent_measure_size();
        (size.width, size.height)
    }

    fn layout_target_size(&self) -> (f32, f32) {
        self.current_layout_target_size()
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

    fn cross_alignment_size(
        &self,
        is_row: bool,
        stretched_cross: Option<f32>,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> f32 {
        let rendered_cross = if is_row {
            self.layout_state.layout_size.height.max(0.0)
        } else {
            self.layout_state.layout_size.width.max(0.0)
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
}
