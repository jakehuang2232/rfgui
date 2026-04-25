impl Element {
    pub(crate) fn inline_promotion_rendering_reason(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> Option<&'static str> {
        if self.children.is_empty()
            || self.layout_state.layout_inner_size.width <= 0.0
            || self.layout_state.layout_inner_size.height <= 0.0
        {
            return None;
        }
        let overflow_child_indices: Vec<bool> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
            .collect();
        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        if self.should_clip_children(&overflow_child_indices, inner_radii, arena) {
            Some(if inner_radii.has_any_rounding() {
                "child-stencil-clip-inline"
            } else {
                "child-scissor-clip-inline"
            })
        } else {
            None
        }
    }

    fn build_base_descendants_only(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
        force_self_opaque: bool,
    ) -> BuildState {
        self.build_base_descendants_only_inner(graph, arena, ctx, force_self_opaque, true)
    }

    fn build_base_descendants_only_inner(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
        force_self_opaque: bool,
        allow_transform: bool,
    ) -> BuildState {
        let accumulated_render_transform = self
            .resolved_transform
            .map(|transform| match ctx.current_render_transform() {
                Some(parent) => parent * transform,
                None => transform,
            });
        ctx.set_current_render_transform(accumulated_render_transform);
        trace_promoted_build(
            "base",
            self.stable_id(),
            self.box_model_snapshot().parent_id,
            format!(
                "promoted={} force_opaque={} children={} target={:?}",
                ctx.is_node_promoted(self.stable_id()),
                force_self_opaque,
                self.children.len(),
                ctx.current_target().and_then(|target| target.handle())
            ),
        );
        if !self.layout_state.should_render {
            if self.has_absolute_descendant_for_hit_test {
                self.collect_root_viewport_deferred_descendants(arena, &mut ctx);
            }
            return ctx.into_state();
        }

        if allow_transform && self.resolved_transform.is_some() {
            return self.build_transformed_subtree(graph, arena, ctx, force_self_opaque);
        }

        let previous_scissor_rect = self
            .absolute_clip_scissor_rect()
            .map(|scissor| ctx.push_scissor_rect(Some(scissor)));

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        self.border_radius = outer_radii.max();
        let pipeline_state = self.build_render_pipeline(
            graph,
            arena,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            force_self_opaque,
        );
        ctx.set_state(pipeline_state);

        let overflow_child_indices: Vec<bool> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
            .collect();

        let should_clip_children =
            self.should_clip_children(&overflow_child_indices, inner_radii, arena);
        let child_clip_scope = if should_clip_children {
            self.begin_child_clip_scope(graph, &mut ctx, inner_radii)
        } else {
            None
        };
        let should_render_children = !should_clip_children || child_clip_scope.is_some();

        let child_keys: Vec<crate::view::node_arena::NodeKey> = self.children.clone();
        if should_render_children && self.has_visible_inner_render_area(&ctx) {
            for (idx, child_key) in child_keys.iter().copied().enumerate() {
                if overflow_child_indices.get(idx).copied().unwrap_or(false) {
                    continue;
                }
                let viewport = ctx.viewport();
                let taken_state = ctx.state_clone();
                let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
                let next_ctx = arena
                    .with_element_taken(child_key, |child, arena| {
                        let ctx_local = ctx_in;
                        if ctx_local.is_node_promoted(child.stable_id()) {
                            return ctx_local;
                        }
                        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                            let vp = ctx_local.viewport();
                            let next_state = element
                                .build_base_descendants_only(graph, arena, ctx_local, false);
                            UiBuildContext::from_parts(vp, next_state)
                        } else {
                            let vp = ctx_local.viewport();
                            let next_state = child.build(graph, arena, ctx_local);
                            UiBuildContext::from_parts(vp, next_state)
                        }
                    });
                if let Some(c) = next_ctx {
                    ctx = c;
                }
            }
        }

        if should_render_children && self.has_visible_inner_render_area(&ctx) {
            for (idx, is_overflow) in overflow_child_indices.into_iter().enumerate() {
                if !is_overflow {
                    continue;
                }
                let Some(child_key) = child_keys.get(idx).copied() else {
                    continue;
                };
                let viewport = ctx.viewport();
                let taken_state = ctx.state_clone();
                let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
                let next_ctx = arena
                    .with_element_taken(child_key, |child, arena| {
                        let mut ctx_local = ctx_in;
                        if child
                            .as_any()
                            .downcast_ref::<Element>()
                            .is_some_and(Element::should_append_to_root_viewport_render)
                        {
                            ctx_local.append_to_defer(child.stable_id());
                            return ctx_local;
                        }
                        if ctx_local.is_node_promoted(child.stable_id()) {
                            return ctx_local;
                        }
                        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                            let vp = ctx_local.viewport();
                            let next_state = element
                                .build_base_descendants_only(graph, arena, ctx_local, false);
                            UiBuildContext::from_parts(vp, next_state)
                        } else {
                            let vp = ctx_local.viewport();
                            let next_state = child.build(graph, arena, ctx_local);
                            UiBuildContext::from_parts(vp, next_state)
                        }
                    });
                if let Some(c) = next_ctx {
                    ctx = c;
                }
            }
        }
        self.end_child_clip_scope(graph, &mut ctx, child_clip_scope);

        if let Some(previous) = previous_scissor_rect {
            ctx.restore_scissor_rect(previous);
        }
        ctx.into_state()
    }

    fn measure_flex_children(
        &mut self,
        proposal: LayoutProposal,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let insets = resolve_layout_insets(
            &self.computed_style.border_widths,
            &self.computed_style.padding,
            proposal.percent_base_width,
            proposal.percent_base_height,
            proposal.viewport_width,
            proposal.viewport_height,
        );

        let (layout_w, layout_h) = self.current_layout_target_size();
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

        let inline_wrap = matches!(self.computed_style.layout_flow_wrap(), FlowWrap::Wrap);
        let inline_gap = resolve_px(
            self.computed_style.gap,
            inner_w,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let inline_horizontal_insets = insets.horizontal().max(0.0);
        let inline_first_available_width =
            self.pending_inline_measure_context.map(|context| {
                (context.first_available_width - inline_horizontal_insets)
                    .max(0.0)
                    .min(inner_w)
            });
        let absolute_mask = self.compute_children_absolute_mask(arena);
        let is_row = matches!(
            self.computed_style.layout_axis_direction(),
            FlowDirection::Row
        );
        let is_real_flex = matches!(self.computed_style.layout, Layout::Flex { .. });
        let solver_wrap =
            !is_real_flex && matches!(self.computed_style.layout_flow_wrap(), FlowWrap::Wrap);
        let main_limit = if is_row { inner_w } else { inner_h };
        let solver_gap = resolve_px(
            self.computed_style.gap,
            if is_row { inner_w } else { inner_h },
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let outputs = crate::view::layout::measure::measure_axis(
            crate::view::layout::measure::MeasureAxisInputs {
                layout: self.computed_style.layout,
                children: &self.children,
                absolute_mask: &absolute_mask,
                is_row,
                is_real_flex,
                solver_wrap,
                solver_gap,
                main_limit,
                inner_width: inner_w,
                child_available_width,
                child_available_height,
                child_percent_base_width,
                child_percent_base_height,
                viewport_width: proposal.viewport_width,
                viewport_height: proposal.viewport_height,
                inline_wrap,
                inline_gap,
                inline_first_available_width,
            },
            arena,
        );

        if self.computed_style.width == SizeValue::Auto {
            let auto_width = if is_row {
                outputs.flex_info.total_main
            } else {
                outputs.flex_info.total_cross
            };
            self.core.set_width(auto_width + insets.horizontal());
        }
        if self.computed_style.height == SizeValue::Auto {
            let auto_height = if is_row {
                outputs.flex_info.total_cross
            } else {
                outputs.flex_info.total_main
            };
            self.core.set_height(auto_height + insets.vertical());
        }

        self.layout_state.content_size = outputs.content_size;
        self.flex_info = Some(outputs.flex_info);
    }

    fn build_render_pipeline(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
        force_opaque: bool,
    ) -> BuildState {
        if !self.core.should_paint {
            return ctx.into_state();
        }
        let fill_color = self.background_color.as_ref().to_rgba_f32();
        let opacity = if force_opaque { 1.0 } else { self.opacity };
        let gradient_paint = self.computed_style.background_image.as_ref().map(|g| {
            resolve_gradient_paint(
                g,
                self.layout_state.layout_size.width.max(0.0),
                self.layout_state.layout_size.height.max(0.0),
            )
        });
        let border_gradient_paint = self.computed_style.border_image.as_ref().map(|g| {
            resolve_gradient_paint(
                g,
                self.layout_state.layout_size.width.max(0.0),
                self.layout_state.layout_size.height.max(0.0),
            )
        });
        let shadow_state = self.render_box_shadows(
            graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            opacity,
        );
        ctx.set_state(shadow_state);

        if self.is_fragmentable_inline_element() && !self.inline_paint_fragments.is_empty() {
            return self.build_inline_fragment_render_pipeline(graph, ctx, fill_color, opacity);
        }

        let max_bw = (self
            .layout_state
            .layout_size
            .width
            .min(self.layout_state.layout_size.height))
            * 0.5;
        let left = self.border_widths.left.clamp(0.0, max_bw);
        let right = self.border_widths.right.clamp(0.0, max_bw);
        let top = self.border_widths.top.clamp(0.0, max_bw);
        let bottom = self.border_widths.bottom.clamp(0.0, max_bw);

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let mut fill_pass = DrawRectPass::new(
            RectPassParams {
                position: [self.layout_state.layout_position.x, self.layout_state.layout_position.y],
                size: [self.layout_state.layout_size.width, self.layout_state.layout_size.height],
                fill_color,
                opacity,
                gradient: gradient_paint,
                ..Default::default()
            },
            DrawRectInput::default(),
            DrawRectOutput::default(),
        );
        fill_pass.set_render_mode(RectRenderMode::FillOnly);
        fill_pass.set_border_widths(left, right, top, bottom);
        fill_pass.set_border_radii(outer_radii.to_array());
        self.push_rect_pass_auto(graph, &mut ctx, fill_pass);

        if left <= 0.0 && right <= 0.0 && top <= 0.0 && bottom <= 0.0 {
            return ctx.into_state();
        }

        let mut border_pass = DrawRectPass::new(
            RectPassParams {
                position: [self.layout_state.layout_position.x, self.layout_state.layout_position.y],
                size: [self.layout_state.layout_size.width, self.layout_state.layout_size.height],
                fill_color: [0.0, 0.0, 0.0, 0.0],
                opacity,
                border_gradient: border_gradient_paint,
                ..Default::default()
            },
            DrawRectInput::default(),
            DrawRectOutput::default(),
        );
        border_pass.set_render_mode(RectRenderMode::BorderOnly);
        border_pass.set_border_side_colors(
            self.border_colors.left.as_ref().to_rgba_f32(),
            self.border_colors.right.as_ref().to_rgba_f32(),
            self.border_colors.top.as_ref().to_rgba_f32(),
            self.border_colors.bottom.as_ref().to_rgba_f32(),
        );
        border_pass.set_border_widths(left, right, top, bottom);
        border_pass.set_border_radii(outer_radii.to_array());
        self.push_rect_pass_auto(graph, &mut ctx, border_pass);
        ctx.into_state()
    }

    fn build_inline_fragment_render_pipeline(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
        fill_color: [f32; 4],
        opacity: f32,
    ) -> BuildState {
        for fragment in self.inline_paint_fragments.clone() {
            let max_bw = (fragment.width.min(fragment.height)) * 0.5;
            let left = self.border_widths.left.clamp(0.0, max_bw);
            let right = self.border_widths.right.clamp(0.0, max_bw);
            let top = self.border_widths.top.clamp(0.0, max_bw);
            let bottom = self.border_widths.bottom.clamp(0.0, max_bw);
            let outer_radii = normalize_corner_radii(
                self.border_radii,
                fragment.width.max(0.0),
                fragment.height.max(0.0),
            );

            let mut fill_pass = DrawRectPass::new(
                RectPassParams {
                    position: [fragment.x, fragment.y],
                    size: [fragment.width, fragment.height],
                    fill_color,
                    opacity,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            fill_pass.set_render_mode(RectRenderMode::FillOnly);
            fill_pass.set_border_widths(left, right, top, bottom);
            fill_pass.set_border_radii(outer_radii.to_array());
            self.push_rect_pass_auto(graph, &mut ctx, fill_pass);

            if left <= 0.0 && right <= 0.0 && top <= 0.0 && bottom <= 0.0 {
                continue;
            }

            let mut border_pass = DrawRectPass::new(
                RectPassParams {
                    position: [fragment.x, fragment.y],
                    size: [fragment.width, fragment.height],
                    fill_color: [0.0, 0.0, 0.0, 0.0],
                    opacity,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            border_pass.set_render_mode(RectRenderMode::BorderOnly);
            border_pass.set_border_side_colors(
                self.border_colors.left.as_ref().to_rgba_f32(),
                self.border_colors.right.as_ref().to_rgba_f32(),
                self.border_colors.top.as_ref().to_rgba_f32(),
                self.border_colors.bottom.as_ref().to_rgba_f32(),
            );
            border_pass.set_border_widths(left, right, top, bottom);
            border_pass.set_border_radii(outer_radii.to_array());
            self.push_rect_pass_auto(graph, &mut ctx, border_pass);
        }
        ctx.into_state()
    }

    fn push_pass<P: GraphicsPass + DrawRectIoPass + 'static>(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        mut pass: P,
    ) {
        let input = self.ensure_current_render_target(graph, ctx);
        if let Some(handle) = input.handle() {
            pass.draw_rect_input_mut().render_target = RenderTargetIn::with_handle(handle);
        }
        pass.draw_rect_input_mut().pass_context = ctx.graphics_pass_context();
        pass.set_scissor_rect(ctx.scissor_rect());
        pass.draw_rect_output_mut().render_target = input;
        graph.add_graphics_pass(pass);
        ctx.set_current_target(input);
    }

    fn push_stencil_pass<P: GraphicsPass + DrawRectIoPass + 'static>(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        mut pass: P,
    ) {
        let input = self.ensure_current_render_target(graph, ctx);
        if let Some(handle) = input.handle() {
            pass.draw_rect_input_mut().render_target = RenderTargetIn::with_handle(handle);
        }
        pass.draw_rect_input_mut().pass_context = ctx.graphics_pass_context();
        pass.set_scissor_rect(ctx.scissor_rect());
        pass.draw_rect_output_mut().render_target = input;
        graph.add_graphics_pass(pass);
        ctx.set_current_target(input);
    }

    fn push_rect_pass_auto(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        pass: DrawRectPass,
    ) {
        if pass.is_opaque_candidate() {
            let mut opaque: OpaqueRectPass = pass.into_opaque();
            opaque.set_depth_order(ctx.next_opaque_rect_order());
            self.push_pass(graph, ctx, opaque);
            return;
        }
        self.push_pass(graph, ctx, pass);
    }

    fn sync_props_from_computed_style(&mut self) {
        self.background_color = Box::new(self.computed_style.background_color);
        self.foreground_color = self.computed_style.color;
        self.box_shadows = self.computed_style.box_shadow.clone();
        self.transform = self.computed_style.transform.clone();
        self.transform_origin = self.computed_style.transform_origin;
        self.border_colors.left = Box::new(self.computed_style.border_colors.left);
        self.border_colors.right = Box::new(self.computed_style.border_colors.right);
        self.border_colors.top = Box::new(self.computed_style.border_colors.top);
        self.border_colors.bottom = Box::new(self.computed_style.border_colors.bottom);
        self.border_widths.left = resolve_px(
            self.computed_style.border_widths.left,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.border_widths.right = resolve_px(
            self.computed_style.border_widths.right,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.border_widths.top = resolve_px(
            self.computed_style.border_widths.top,
            self.core.size.height,
            0.0,
            0.0,
        );
        self.border_widths.bottom = resolve_px(
            self.computed_style.border_widths.bottom,
            self.core.size.height,
            0.0,
            0.0,
        );
        let radius_base = self.core.size.width.min(self.core.size.height).max(0.0);
        self.border_radii = CornerRadii {
            top_left: resolve_px(
                self.computed_style.border_radii.top_left,
                radius_base,
                0.0,
                0.0,
            ),
            top_right: resolve_px(
                self.computed_style.border_radii.top_right,
                radius_base,
                0.0,
                0.0,
            ),
            bottom_right: resolve_px(
                self.computed_style.border_radii.bottom_right,
                radius_base,
                0.0,
                0.0,
            ),
            bottom_left: resolve_px(
                self.computed_style.border_radii.bottom_left,
                radius_base,
                0.0,
                0.0,
            ),
        };
        self.border_radius = self.border_radii.max();
        self.opacity = self.computed_style.opacity.clamp(0.0, 1.0);
        self.update_resolved_transform();
        self.scroll_direction = self.computed_style.scroll_direction;
        self.padding.left = resolve_px(
            self.computed_style.padding.left,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.padding.right = resolve_px(
            self.computed_style.padding.right,
            self.core.size.width,
            0.0,
            0.0,
        );
        self.padding.top = resolve_px(
            self.computed_style.padding.top,
            self.core.size.height,
            0.0,
            0.0,
        );
        self.padding.bottom = resolve_px(
            self.computed_style.padding.bottom,
            self.core.size.height,
            0.0,
            0.0,
        );
    }

    fn update_resolved_transform(&mut self) {
        self.resolved_transform = self.compute_transform_matrix();
        self.resolved_inverse_transform = self.resolved_transform.and_then(|matrix| {
            let det = matrix.determinant();
            if det.abs() <= 0.000_001 {
                None
            } else {
                Some(matrix.inverse())
            }
        });
    }

    fn compute_transform_matrix(&self) -> Option<Mat4> {
        if self.transform.as_slice().is_empty() {
            return None;
        }
        let size = self.layout_state.layout_size;
        let origin = Vec3::new(
            resolve_signed_px_with_base(
                self.transform_origin.x(),
                Some(size.width.max(0.0)),
                0.0,
                0.0,
            )
            .unwrap_or(0.0),
            resolve_signed_px_with_base(
                self.transform_origin.y(),
                Some(size.height.max(0.0)),
                0.0,
                0.0,
            )
            .unwrap_or(0.0),
            self.transform_origin.z(),
        );
        let mut transform = Mat4::IDENTITY;
        for entry in self.transform.as_slice() {
            let next = match entry.kind() {
                TransformKind::Translate { x, y, z } => Mat4::from_translation(Vec3::new(
                    resolve_signed_px_with_base(x, Some(size.width.max(0.0)), 0.0, 0.0)
                        .unwrap_or(0.0),
                    resolve_signed_px_with_base(y, Some(size.height.max(0.0)), 0.0, 0.0)
                        .unwrap_or(0.0),
                    z,
                )),
                TransformKind::Scale { x, y, z } => Mat4::from_scale(Vec3::new(x, y, z)),
                TransformKind::Rotate { x, y, z } => {
                    Mat4::from_rotation_x(x.to_radians())
                        * Mat4::from_rotation_y(y.to_radians())
                        * Mat4::from_rotation_z(z.to_radians())
                }
                TransformKind::Perspective { depth } => css_perspective_matrix(depth.max(0.0001)),
                TransformKind::Matrix { matrix } => Mat4::from_cols_array(&matrix),
            };
            transform *= next;
        }
        let origin_world = Vec3::new(
            self.layout_state.layout_position.x + origin.x,
            self.layout_state.layout_position.y + origin.y,
            origin.z,
        );
        Some(
            Mat4::from_translation(origin_world)
                * transform
                * Mat4::from_translation(-origin_world),
        )
    }

    fn render_box_shadows(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
        opacity: f32,
    ) -> BuildState {
        if self.box_shadows.is_empty() {
            return ctx.into_state();
        }
        let fragment_rects = if self.is_fragmentable_inline_element() && !self.inline_paint_fragments.is_empty()
        {
            self.inline_paint_fragments.clone()
        } else {
            vec![Rect {
                x: self.layout_state.layout_position.x,
                y: self.layout_state.layout_position.y,
                width: self.layout_state.layout_size.width.max(0.0),
                height: self.layout_state.layout_size.height.max(0.0),
            }]
        };
        let shadows = self.box_shadows.clone();
        for fragment in fragment_rects {
            if fragment.width <= 0.0 || fragment.height <= 0.0 {
                continue;
            }
            let outer_radii =
                normalize_corner_radii(self.border_radii, fragment.width, fragment.height);
            for shadow in shadows.iter().cloned() {
                let spread = shadow.spread;
                let shadow_radii = expand_corner_radii_for_spread(
                    outer_radii,
                    spread,
                    fragment.width,
                    fragment.height,
                );
                let mesh = ShadowMesh::rounded_rect_with_radii(
                    fragment.x - spread,
                    fragment.y - spread,
                    fragment.width + spread * 2.0,
                    fragment.height + spread * 2.0,
                    shadow_radii.to_array(),
                );
                let params = ShadowParams {
                    offset_x: shadow.offset_x,
                    offset_y: shadow.offset_y,
                    blur_radius: shadow.blur.max(0.0),
                    color: shadow.color.to_rgba_f32(),
                    opacity: opacity.clamp(0.0, 1.0),
                    spread: 0.0,
                    clip_to_geometry: shadow.inset,
                };
                let next_state = self.push_shadow_pass(
                    mesh,
                    params,
                    graph,
                    UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
                );
                ctx.set_state(next_state);
            }
        }
        ctx.into_state()
    }

    fn push_shadow_pass(
        &mut self,
        mesh: ShadowMesh,
        params: ShadowParams,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        self.ensure_current_render_target(graph, &mut ctx);
        let output = ctx
            .current_target()
            .unwrap_or_else(|| ctx.allocate_target(graph));
        ctx.set_current_target(output);
        let built = build_shadow_module(
            graph,
            ShadowModuleSpec {
                mesh,
                params,
                viewport_width: ctx.viewport.target_width,
                viewport_height: ctx.viewport.target_height,
                scale_factor: ctx.viewport.scale_factor,
                pass_context: ctx.graphics_pass_context(),
                output,
            },
        );
        if built {
            ctx.set_current_target(output);
        }
        ctx.into_state()
    }

    fn ensure_current_render_target(
        &self,
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
    ) -> RenderTargetOut {
        if let Some(target) = ctx.current_target() {
            return target;
        }
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    }

    fn transformed_quad_positions(&self, bounds: crate::view::base_component::PromotionCompositeBounds) -> [[f32; 2]; 4] {
        let corners = [
            Vec3::new(bounds.x, bounds.y + bounds.height, 0.0),
            Vec3::new(bounds.x + bounds.width, bounds.y + bounds.height, 0.0),
            Vec3::new(bounds.x + bounds.width, bounds.y, 0.0),
            Vec3::new(bounds.x, bounds.y, 0.0),
        ];
        let matrix = self.resolved_transform.unwrap_or(Mat4::IDENTITY);
        corners.map(|corner| {
            let transformed = matrix * corner.extend(1.0);
            let w = if transformed.w.abs() <= 0.000_001 {
                1.0
            } else {
                transformed.w
            };
            [transformed.x / w, transformed.y / w]
        })
    }

    fn build_transformed_subtree(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
        force_self_opaque: bool,
    ) -> BuildState {
        let source_bounds = self.transform_subtree_raster_bounds(arena);
        let mut layer_ctx = UiBuildContext::from_parts(
            ctx.viewport(),
            BuildState::for_layer_subtree_with_ancestor_clip(AncestorClipContext::default()),
        );
        layer_ctx.set_current_render_transform(ctx.current_render_transform());
        let layer_target = layer_ctx.allocate_persistent_target_with_key(
            graph,
            crate::view::base_component::transformed_layer_stable_key(self.stable_id()),
            source_bounds,
        );
        layer_ctx.set_current_target(layer_target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: layer_ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: layer_target,
            },
        ));
        let layer_state =
            self.build_base_descendants_only_inner(graph, arena, layer_ctx, force_self_opaque, false);
        ctx.state.merge_child_side_effects(&layer_state);

        let parent_target = ctx.current_target().unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
        ctx.set_current_target(parent_target);
        graph.add_graphics_pass(crate::view::render_pass::TextureCompositePass::new(
            crate::view::render_pass::TextureCompositeParams {
                bounds: [
                    source_bounds.x,
                    source_bounds.y,
                    source_bounds.width,
                    source_bounds.height,
                ],
                quad_positions: Some(self.transformed_quad_positions(source_bounds)),
                uv_bounds: Some([
                    source_bounds.x,
                    source_bounds.y,
                    source_bounds.width,
                    source_bounds.height,
                ]),
                mask_uv_bounds: None,
                use_mask: false,
                source_is_premultiplied: true,
                opacity: 1.0,
                scissor_rect: ctx.state.scissor_rect,
            },
            crate::view::render_pass::TextureCompositeInput {
                source: crate::view::render_pass::TextureCompositeSourceIn::with_handle(
                    layer_target.handle().expect("transformed layer target should exist"),
                ),
                sampled_source_key: None,
                sampled_source_size: None,
                sampled_source_upload: None,
                sampled_upload_state_key: None,
                sampled_upload_generation: None,
                sampled_source_sampling: None,
                mask: Default::default(),
                pass_context: ctx.graphics_pass_context(),
            },
            crate::view::render_pass::TextureCompositeOutput {
                render_target: parent_target,
            },
        ));
        ctx.set_current_target(parent_target);
        ctx.into_state()
    }

    pub(crate) fn build_base_only(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        self.build_base_descendants_only(graph, arena, ctx, false)
    }

    pub(crate) fn compose_promoted_descendants_only(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        trace_promoted_build(
            "compose-descendants",
            self.stable_id(),
            self.box_model_snapshot().parent_id,
            format!(
                "promoted={} children={} target={:?}",
                ctx.is_node_promoted(self.stable_id()),
                self.children.len(),
                ctx.current_target().and_then(|target| target.handle())
            ),
        );
        let has_deferred_descendants = self.children.iter().any(|child_key| {
            arena
                .get(*child_key)
                .map(|node| {
                    node.element
                        .as_any()
                        .downcast_ref::<Element>()
                        .is_some_and(Element::should_append_to_root_viewport_render)
                })
                .unwrap_or(false)
        });
        let has_promoted_descendants = self.has_composited_promoted_descendants(arena, &ctx);

        let previous_scissor_rect = self
            .absolute_clip_scissor_rect()
            .map(|scissor| ctx.push_scissor_rect(Some(scissor)));

        if has_promoted_descendants || has_deferred_descendants {
            let overflow_child_indices: Vec<bool> = (0..self.children.len())
                .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
                .collect();
            let outer_radii = normalize_corner_radii(
                self.border_radii,
                self.layout_state.layout_size.width.max(0.0),
                self.layout_state.layout_size.height.max(0.0),
            );
            let inner_radii = self.inner_clip_radii(outer_radii);
            let should_clip_promoted_descendants =
                self.should_clip_children(&overflow_child_indices, inner_radii, arena);
            let use_mask_clip = should_clip_promoted_descendants && inner_radii.has_any_rounding();
            let previous_inner_scissor = if use_mask_clip {
                Some(ctx.push_scissor_rect(self.inner_clip_scissor_rect()))
            } else {
                None
            };
            let mask_target = if use_mask_clip {
                Some(self.render_promoted_child_clip_mask(graph, &ctx, inner_radii))
            } else {
                None
            };
            let child_clip_scope = if should_clip_promoted_descendants {
                self.begin_child_clip_scope(graph, &mut ctx, inner_radii)
            } else {
                None
            };
            let should_render_children =
                !should_clip_promoted_descendants || child_clip_scope.is_some();

            let child_keys: Vec<crate::view::node_arena::NodeKey> = self.children.clone();
            if should_render_children && self.has_visible_inner_render_area(&ctx) {
                for (idx, child_key) in child_keys.iter().copied().enumerate() {
                    if overflow_child_indices.get(idx).copied().unwrap_or(false) {
                        continue;
                    }
                    let child_id = arena
                        .get(child_key)
                        .map(|n| n.element.stable_id())
                        .unwrap_or(0);
                    if ctx.is_node_promoted(child_id) {
                        Self::build_promoted_child(
                            graph,
                            arena,
                            &mut ctx,
                            child_key,
                            mask_target,
                        );
                        continue;
                    }
                    let viewport = ctx.viewport();
                    let taken_state = ctx.state_clone();
                    let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
                    let next_ctx = arena.with_element_taken(child_key, |child, arena| {
                        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                            let vp = ctx_in.viewport();
                            let next_state =
                                element.compose_promoted_descendants_only(graph, arena, ctx_in);
                            UiBuildContext::from_parts(vp, next_state)
                        } else {
                            ctx_in
                        }
                    });
                    if let Some(c) = next_ctx {
                        ctx = c;
                    }
                }
            }

            if should_render_children && self.has_visible_inner_render_area(&ctx) {
                for (idx, is_overflow) in overflow_child_indices.into_iter().enumerate() {
                    if !is_overflow {
                        continue;
                    }
                    let Some(child_key) = child_keys.get(idx).copied() else {
                        continue;
                    };
                    let (child_id, is_defer) = arena
                        .get(child_key)
                        .map(|n| {
                            (
                                n.element.stable_id(),
                                n.element
                                    .as_any()
                                    .downcast_ref::<Element>()
                                    .is_some_and(Element::should_append_to_root_viewport_render),
                            )
                        })
                        .unwrap_or((0, false));
                    if is_defer {
                        ctx.append_to_defer(child_id);
                        continue;
                    }
                    if ctx.is_node_promoted(child_id) {
                        Self::build_promoted_child(
                            graph,
                            arena,
                            &mut ctx,
                            child_key,
                            mask_target,
                        );
                        continue;
                    }
                    let viewport = ctx.viewport();
                    let taken_state = ctx.state_clone();
                    let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
                    let next_ctx = arena.with_element_taken(child_key, |child, arena| {
                        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                            let vp = ctx_in.viewport();
                            let next_state =
                                element.compose_promoted_descendants_only(graph, arena, ctx_in);
                            UiBuildContext::from_parts(vp, next_state)
                        } else {
                            ctx_in
                        }
                    });
                    if let Some(c) = next_ctx {
                        ctx = c;
                    }
                }
            }

            self.end_child_clip_scope(graph, &mut ctx, child_clip_scope);
            if let Some(previous) = previous_inner_scissor {
                ctx.restore_scissor_rect(previous);
            }
        }
        let scrollbar_state = self.render_scrollbars(
            graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
        );
        ctx.set_state(scrollbar_state);

        if let Some(previous) = previous_scissor_rect {
            ctx.restore_scissor_rect(previous);
        }

        ctx.into_state()
    }

    fn composite_promoted_child_target(
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        child: &dyn ElementTrait,
        layer_target: RenderTargetOut,
    ) {
        let composite_bounds = child.promotion_composite_bounds();
        let opacity = if child.as_any().downcast_ref::<Element>().is_some() {
            1.0
        } else {
            child.promotion_node_info().opacity.clamp(0.0, 1.0)
        };
        Self::composite_layer_target_into_current(
            graph,
            ctx,
            layer_target,
            composite_bounds,
            opacity,
            ctx.state.scissor_rect,
        );
    }

    fn composite_layer_target_into_current(
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        layer_target: RenderTargetOut,
        composite_bounds: crate::view::base_component::PromotionCompositeBounds,
        opacity: f32,
        scissor_rect: Option<[u32; 4]>,
    ) {
        let parent_target = ctx.current_target().unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
        ctx.set_current_target(parent_target);
        let pass = crate::view::render_pass::composite_layer_pass::CompositeLayerPass::new(
            crate::view::render_pass::composite_layer_pass::CompositeLayerParams {
                rect_pos: [composite_bounds.x, composite_bounds.y],
                rect_size: [composite_bounds.width, composite_bounds.height],
                corner_radii: composite_bounds.corner_radii,
                opacity,
                scissor_rect,
                clear_target: false,
            },
            crate::view::render_pass::composite_layer_pass::CompositeLayerInput {
                layer: crate::view::render_pass::composite_layer_pass::LayerIn::with_handle(
                    layer_target.handle().expect("promoted layer target should exist"),
                ),
                pass_context: ctx.graphics_pass_context(),
            },
            crate::view::render_pass::composite_layer_pass::CompositeLayerOutput {
                render_target: parent_target,
            },
        );
        graph.add_graphics_pass(pass);
        ctx.set_current_target(parent_target);
    }

    fn render_promoted_child_clip_mask(
        &mut self,
        graph: &mut FrameGraph,
        ctx: &UiBuildContext,
        inner_radii: CornerRadii,
    ) -> RenderTargetOut {
        let inner = self.inner_clip_rect();
        let mut mask_ctx = UiBuildContext::from_parts(
            ctx.viewport(),
            BuildState::for_layer_subtree_with_ancestor_clip(ctx.ancestor_clip_context()),
        );
        let mask_target = mask_ctx.allocate_persistent_target_with_key(
            graph,
            crate::view::base_component::promoted_clip_mask_stable_key(self.stable_id()),
            self.promotion_composite_bounds(),
        );
        mask_ctx.set_current_target(mask_target);
        let mut pass = DrawRectPass::new(
            RectPassParams {
                position: [inner.x, inner.y],
                size: [inner.width, inner.height],
                fill_color: [1.0, 1.0, 1.0, 1.0],
                opacity: 1.0,
                ..Default::default()
            },
            DrawRectInput::default(),
            DrawRectOutput::default(),
        );
        pass.set_render_mode(RectRenderMode::FillOnly);
        pass.set_border_width(0.0);
        pass.set_border_radii(inner_radii.to_array());
        pass.set_clear_target(true);
        let mut mask_ctx = mask_ctx;
        self.push_pass(graph, &mut mask_ctx, pass);
        mask_target
    }

    fn composite_promoted_child_target_with_mask(
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        child: &dyn ElementTrait,
        layer_target: RenderTargetOut,
        mask_target: RenderTargetOut,
    ) {
        let parent_target = ctx.current_target().unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
        ctx.set_current_target(parent_target);
        let composite_bounds = child.promotion_composite_bounds();
        let pass = crate::view::render_pass::TextureCompositePass::new(
            crate::view::render_pass::TextureCompositeParams {
                bounds: [
                    composite_bounds.x,
                    composite_bounds.y,
                    composite_bounds.width,
                    composite_bounds.height,
                ],
                quad_positions: None,
                uv_bounds: Some([
                    composite_bounds.x,
                    composite_bounds.y,
                    composite_bounds.width,
                    composite_bounds.height,
                ]),
                mask_uv_bounds: Some([
                    composite_bounds.x,
                    composite_bounds.y,
                    composite_bounds.width,
                    composite_bounds.height,
                ]),
                use_mask: true,
                source_is_premultiplied: true,
                opacity: if child.as_any().downcast_ref::<Element>().is_some() {
                    1.0
                } else {
                    child.promotion_node_info().opacity.clamp(0.0, 1.0)
                },
                scissor_rect: ctx.state.scissor_rect,
            },
            crate::view::render_pass::TextureCompositeInput {
                source: crate::view::render_pass::TextureCompositeSourceIn::with_handle(
                    layer_target.handle().expect("promoted layer target should exist"),
                ),
                sampled_source_key: None,
                sampled_source_size: None,
                sampled_source_upload: None,
                sampled_upload_state_key: None,
                sampled_upload_generation: None,
                sampled_source_sampling: None,
                mask: crate::view::render_pass::TextureCompositeMaskIn::with_handle(
                    mask_target.handle().expect("promoted clip mask target should exist"),
                ),
                pass_context: ctx.graphics_pass_context(),
            },
            crate::view::render_pass::TextureCompositeOutput {
                render_target: parent_target,
            },
        );
        graph.add_graphics_pass(pass);
        ctx.set_current_target(parent_target);
    }

    fn has_composited_promoted_descendants(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        ctx: &UiBuildContext,
    ) -> bool {
        for child_key in &self.children {
            let Some(node) = arena.get(*child_key) else {
                continue;
            };
            let child = node.element.as_ref();
            if child
                .as_any()
                .downcast_ref::<Element>()
                .is_some_and(Element::should_append_to_root_viewport_render)
            {
                continue;
            }
            if ctx.is_node_promoted(child.stable_id()) {
                return true;
            }
            if let Some(element) = child.as_any().downcast_ref::<Element>() {
                if element.has_composited_promoted_descendants(arena, ctx) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn build_promoted_layer(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
        requested_update_kind: crate::view::promotion::PromotedLayerUpdateKind,
        can_reuse_base: bool,
        context: crate::view::viewport::DebugReusePathContext,
    ) -> BuildState {
        trace_promoted_build(
            "promoted-layer",
            self.stable_id(),
            self.box_model_snapshot().parent_id,
            format!(
                "context={context:?} requested={requested_update_kind:?} can_reuse_base={} target={:?}",
                can_reuse_base,
                ctx.current_target().and_then(|target| target.handle())
            ),
        );
        let viewport = ctx.viewport();
        let mut ctx = ctx;
        if can_reuse_base {
            self.collect_root_viewport_deferred_descendants(arena, &mut ctx);
        }
        let base_target = ctx.current_target().expect("promoted layer target should exist");
        let base_state = if can_reuse_base {
            ctx.into_state()
        } else {
            graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: base_target,
                },
            ));
            self.build_base_only(graph, arena, ctx)
        };

        let probe_ctx = UiBuildContext::from_parts(viewport.clone(), base_state.clone());
        let has_composited_descendants = self.has_composited_promoted_descendants(arena, &probe_ctx);
        let requested_composition_update_kind = probe_ctx
            .promoted_composition_update_kind(self.stable_id())
            .unwrap_or(crate::view::promotion::PromotedLayerUpdateKind::Reraster);
        let can_reuse_final = can_reuse_base
            && matches!(
                requested_composition_update_kind,
                crate::view::promotion::PromotedLayerUpdateKind::Reuse
            );
        crate::view::viewport::record_debug_reuse_path(
            crate::view::viewport::DebugReusePathRecord {
                node_id: self.stable_id(),
                context,
                requested: requested_update_kind,
                can_reuse: if has_composited_descendants {
                    can_reuse_final
                } else {
                    can_reuse_base
                },
                actual: if has_composited_descendants {
                    if can_reuse_final {
                        crate::view::promotion::PromotedLayerUpdateKind::Reuse
                    } else {
                        crate::view::promotion::PromotedLayerUpdateKind::Reraster
                    }
                } else if can_reuse_base {
                    crate::view::promotion::PromotedLayerUpdateKind::Reuse
                } else {
                    crate::view::promotion::PromotedLayerUpdateKind::Reraster
                },
                reason: if matches!(
                    requested_update_kind,
                    crate::view::promotion::PromotedLayerUpdateKind::Reuse
                ) && !can_reuse_base
                {
                    Some("reuse-blocked")
                } else if has_composited_descendants
                    && can_reuse_base
                    && matches!(
                        requested_composition_update_kind,
                        crate::view::promotion::PromotedLayerUpdateKind::Reraster
                    )
                {
                    Some("composition-reraster")
                } else {
                    None
                },
                clip_rect: self.absolute_clip_scissor_rect(),
            },
        );
        if !has_composited_descendants {
            if can_reuse_base {
                return base_state;
            }
            return self.render_scrollbars(
                graph,
                UiBuildContext::from_parts(viewport, base_state),
            );
        }

        let mut compose_ctx = UiBuildContext::from_parts(viewport, base_state);
        let final_target = compose_ctx.allocate_persistent_target_with_key(
            graph,
            crate::view::base_component::promoted_final_layer_stable_key(self.stable_id()),
            self.promotion_composite_bounds(),
        );
        if can_reuse_final {
            let mut reused_state = compose_ctx.into_state();
            reused_state.target = Some(final_target);
            return reused_state;
        }
        compose_ctx.set_current_target(final_target);
        let compose_pass_context = compose_ctx.graphics_pass_context();
        let parent_target = compose_ctx
            .current_target()
            .expect("promoted final target should exist");
        graph.add_graphics_pass(
            crate::view::render_pass::composite_layer_pass::CompositeLayerPass::new(
                crate::view::render_pass::composite_layer_pass::CompositeLayerParams {
                    rect_pos: [
                        self.promotion_composite_bounds().x,
                        self.promotion_composite_bounds().y,
                    ],
                    rect_size: [
                        self.promotion_composite_bounds().width,
                        self.promotion_composite_bounds().height,
                    ],
                    corner_radii: self.promotion_composite_bounds().corner_radii,
                    opacity: 1.0,
                    scissor_rect: None,
                    clear_target: true,
                },
                crate::view::render_pass::composite_layer_pass::CompositeLayerInput {
                    layer: crate::view::render_pass::composite_layer_pass::LayerIn::with_handle(
                        base_target.handle().expect("promoted base target should exist"),
                    ),
                    pass_context: compose_pass_context,
                },
                crate::view::render_pass::composite_layer_pass::CompositeLayerOutput {
                    render_target: parent_target,
                },
            ),
        );
        self.compose_promoted_descendants_only(graph, arena, compose_ctx)
    }

    fn build_promoted_child(
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: &mut UiBuildContext,
        child_key: crate::view::node_arena::NodeKey,
        mask_target: Option<RenderTargetOut>,
    ) {
        arena.with_element_taken(child_key, |child, arena| {
            Self::build_promoted_child_inner(graph, arena, ctx, child, mask_target);
        });
    }

    fn build_promoted_child_inner(
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: &mut UiBuildContext,
        child: &mut Box<dyn ElementTrait>,
        mask_target: Option<RenderTargetOut>,
    ) {
        let child_id = child.stable_id();
        trace_promoted_build(
            "promoted-child",
            child_id,
            child.box_model_snapshot().parent_id,
            format!(
                "mask={} ancestor_scissor={} ancestor_stencil={} requested={:?}",
                mask_target.is_some(),
                ctx.scissor_rect().is_some(),
                ctx.current_clip_id() != 0,
                ctx.promoted_update_kind(child_id)
                    .unwrap_or(crate::view::promotion::PromotedLayerUpdateKind::Reraster)
            ),
        );
        let requested_update = ctx
            .promoted_update_kind(child_id)
            .unwrap_or(crate::view::promotion::PromotedLayerUpdateKind::Reraster);
        let has_ancestor_scissor = ctx.scissor_rect().is_some();
        let has_ancestor_stencil = ctx.current_clip_id() != 0;
        if has_ancestor_stencil {
            crate::view::viewport::record_debug_reuse_path(
                crate::view::viewport::DebugReusePathRecord {
                    node_id: child_id,
                    context: crate::view::viewport::DebugReusePathContext::Child,
                    requested: requested_update,
                    can_reuse: false,
                    actual: crate::view::promotion::PromotedLayerUpdateKind::Reraster,
                    reason: Some("ancestor-stencil-inline"),
                    clip_rect: None,
                },
            );
            let viewport = ctx.viewport();
            let next_state = child.build(
                graph,
                arena,
                UiBuildContext::from_parts(viewport, ctx.state_clone()),
            );
            ctx.set_state(next_state);
            let _ = mask_target;
            return;
        }
        if has_ancestor_scissor && !has_ancestor_stencil {
            crate::view::viewport::record_debug_reuse_path(
                crate::view::viewport::DebugReusePathRecord {
                    node_id: child_id,
                    context: crate::view::viewport::DebugReusePathContext::Child,
                    requested: requested_update,
                    can_reuse: false,
                    actual: crate::view::promotion::PromotedLayerUpdateKind::Reraster,
                    reason: Some("ancestor-scissor-inline"),
                    clip_rect: None,
                },
            );
            let viewport = ctx.viewport();
            let next_state = child.build(
                graph,
                arena,
                UiBuildContext::from_parts(viewport, ctx.state_clone()),
            );
            ctx.set_state(next_state);
            let _ = mask_target;
            return;
        }
        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
            if let Some(reason) = element.inline_promotion_rendering_reason(arena) {
                if reason == "child-scissor-clip-inline"
                    || reason == "child-stencil-clip-inline"
                {
                    // Child clip geometry is tracked in promotion signatures; do not block
                    // promoted child reuse solely because the container clips its children.
                } else {
                crate::view::viewport::record_debug_reuse_path(
                    crate::view::viewport::DebugReusePathRecord {
                        node_id: child_id,
                        context: crate::view::viewport::DebugReusePathContext::Child,
                        requested: requested_update,
                        can_reuse: false,
                        actual: crate::view::promotion::PromotedLayerUpdateKind::Reraster,
                        reason: Some(reason),
                        clip_rect: element.absolute_clip_scissor_rect(),
                    },
                );
                let viewport = ctx.viewport();
                let next_state = element.build(
                    graph,
                    arena,
                    UiBuildContext::from_parts(viewport, ctx.state_clone()),
                );
                ctx.set_state(next_state);
                let _ = mask_target;
                return;
                }
            }
        }

        let update_kind = requested_update;
        let reuse_result = crate::view::base_component::can_reuse_promoted_subtree(
            child.as_ref(),
            ctx,
            arena,
        );
        let can_reuse = matches!(
            update_kind,
            crate::view::promotion::PromotedLayerUpdateKind::Reuse
        ) && reuse_result;
        let mut child_ctx = UiBuildContext::from_parts(
            ctx.viewport(),
            BuildState::for_layer_subtree_with_ancestor_clip(ctx.ancestor_clip_context()),
        );
        let layer_target = child_ctx.allocate_promoted_layer_target(
            graph,
            child.stable_id(),
            child.promotion_composite_bounds(),
        );
        child_ctx.set_current_target(layer_target);
        let child_state = if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
            element.build_promoted_layer(
                graph,
                arena,
                child_ctx,
                update_kind,
                can_reuse,
                crate::view::viewport::DebugReusePathContext::Child,
            )
        } else if can_reuse {
            crate::view::viewport::record_debug_reuse_path(
                crate::view::viewport::DebugReusePathRecord {
                    node_id: child.stable_id(),
                    context: crate::view::viewport::DebugReusePathContext::Child,
                    requested: update_kind,
                    can_reuse,
                    actual: crate::view::promotion::PromotedLayerUpdateKind::Reuse,
                    reason: None,
                    clip_rect: None,
                },
            );
            child_ctx.into_state()
        } else {
            crate::view::viewport::record_debug_reuse_path(
                crate::view::viewport::DebugReusePathRecord {
                    node_id: child.stable_id(),
                    context: crate::view::viewport::DebugReusePathContext::Child,
                    requested: update_kind,
                    can_reuse,
                    actual: crate::view::promotion::PromotedLayerUpdateKind::Reraster,
                    reason: if matches!(
                        update_kind,
                        crate::view::promotion::PromotedLayerUpdateKind::Reuse
                    ) {
                        Some("reuse-blocked")
                    } else {
                        None
                    },
                    clip_rect: None,
                },
            );
            graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: child_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: layer_target,
                },
            ));
            child.build(graph, arena, child_ctx)
        };
        ctx.merge_child_state_side_effects(&child_state);
        let layer_target = child_state.target.unwrap_or(layer_target);
        if let Some(mask_target) = mask_target {
            Self::composite_promoted_child_target_with_mask(
                graph,
                ctx,
                child.as_ref(),
                layer_target,
                mask_target,
            );
        } else {
            Self::composite_promoted_child_target(graph, ctx, child.as_ref(), layer_target);
        }
    }
}
