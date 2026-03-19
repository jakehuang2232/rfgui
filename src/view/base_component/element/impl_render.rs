impl Element {
    pub(crate) fn inline_promotion_rendering_reason(&self) -> Option<&'static str> {
        if self.children.is_empty()
            || self.layout_inner_size.width <= 0.0
            || self.layout_inner_size.height <= 0.0
        {
            return None;
        }
        let overflow_child_indices: Vec<bool> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx))
            .collect();
        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.core.layout_size.width.max(0.0),
            self.core.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        if self.should_clip_children(&overflow_child_indices, inner_radii) {
            Some(if inner_radii.has_any_rounding() {
                "child-stencil-clip-inline"
            } else {
                "child-scissor-clip-inline"
            })
        } else {
            None
        }
    }

    pub(crate) fn requires_inline_promotion_rendering(&self) -> bool {
        self.inline_promotion_rendering_reason().is_some()
    }

    fn build_base_descendants_only(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
        force_self_opaque: bool,
    ) -> BuildState {
        if !self.core.should_render {
            if self.has_absolute_descendant_for_hit_test {
                self.collect_root_viewport_deferred_descendants(&mut ctx);
            }
            return ctx.into_state();
        }

        let previous_scissor_rect = self
            .absolute_clip_scissor_rect()
            .map(|scissor| ctx.push_scissor_rect(Some(scissor)));

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.core.layout_size.width.max(0.0),
            self.core.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        self.border_radius = outer_radii.max();
        let pipeline_state = self.build_render_pipeline(
            graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            force_self_opaque,
        );
        ctx.set_state(pipeline_state);

        let overflow_child_indices: Vec<bool> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx))
            .collect();

        let child_clip_scope = if self.should_clip_children(&overflow_child_indices, inner_radii) {
            self.begin_child_clip_scope(graph, &mut ctx, inner_radii)
        } else {
            None
        };

        if self.layout_inner_size.width > 0.0 && self.layout_inner_size.height > 0.0 {
            for (idx, child) in self.children.iter_mut().enumerate() {
                if overflow_child_indices.get(idx).copied().unwrap_or(false) {
                    continue;
                }
                if ctx.is_node_promoted(child.id()) {
                    continue;
                }
                if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                    let viewport = ctx.viewport();
                    let next_state = element.build_base_descendants_only(graph, ctx, false);
                    ctx = UiBuildContext::from_parts(viewport, next_state);
                } else {
                    let viewport = ctx.viewport();
                    let next_state = child.build(graph, ctx);
                    ctx = UiBuildContext::from_parts(viewport, next_state);
                }
            }
        }

        for (idx, is_overflow) in overflow_child_indices.into_iter().enumerate() {
            if !is_overflow {
                continue;
            }
            if let Some(child) = self.children.get_mut(idx) {
                if child
                    .as_any()
                    .downcast_ref::<Element>()
                    .is_some_and(Element::should_append_to_root_viewport_render)
                {
                    ctx.append_to_defer(child.id());
                    continue;
                }
                if ctx.is_node_promoted(child.id()) {
                    continue;
                }
                if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                    let viewport = ctx.viewport();
                    let next_state = element.build_base_descendants_only(graph, ctx, false);
                    ctx = UiBuildContext::from_parts(viewport, next_state);
                } else {
                    let viewport = ctx.viewport();
                    let next_state = child.build(graph, ctx);
                    ctx = UiBuildContext::from_parts(viewport, next_state);
                }
            }
        }
        self.end_child_clip_scope(graph, &mut ctx, child_clip_scope);

        if let Some(previous) = previous_scissor_rect {
            ctx.restore_scissor_rect(previous);
        }
        ctx.into_state()
    }

    fn measure_flex_children(&mut self, proposal: LayoutProposal) {
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
            && proposal.percent_base_height.is_some()
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
        let info = self.compute_flex_info(
            inner_w,
            inner_h,
            proposal.viewport_width,
            proposal.viewport_height,
        );
        let is_row = matches!(
            self.computed_style.layout_flow_direction(),
            FlowDirection::Row
        );

        if self.computed_style.width == SizeValue::Auto {
            let auto_width = if is_row {
                info.total_main
            } else {
                info.total_cross
            };
            self.core.set_width(auto_width + bw_l + bw_r + p_l + p_r);
        }
        if self.computed_style.height == SizeValue::Auto {
            let auto_height = if is_row {
                info.total_cross
            } else {
                info.total_main
            };
            self.core.set_height(auto_height + bw_t + bw_b + p_t + p_b);
        }

        self.content_size = Size {
            width: if is_row {
                info.total_main
            } else {
                info.total_cross
            },
            height: if is_row {
                info.total_cross
            } else {
                info.total_main
            },
        };
        self.flex_info = Some(info);
    }

    fn compute_flex_info(
        &self,
        inner_w: f32,
        inner_h: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> FlexLayoutInfo {
        let is_row = matches!(
            self.computed_style.layout_flow_direction(),
            FlowDirection::Row
        );
        let wrap = matches!(self.computed_style.layout_flow_wrap(), FlowWrap::Wrap);
        let main_limit = if is_row { inner_w } else { inner_h };
        let gap_base = if is_row { inner_w } else { inner_h };
        let gap = resolve_px(
            self.computed_style.gap,
            gap_base,
            viewport_width,
            viewport_height,
        );

        let mut child_sizes = vec![(0.0_f32, 0.0_f32); self.children.len()];
        for (idx, child) in self.children.iter().enumerate() {
            if self.child_is_absolute(idx) {
                continue;
            }
            let (w, h) = child.measured_size();
            let main = if is_row { w } else { h };
            let cross = if is_row { h } else { w };
            child_sizes[idx] = (main, cross);
        }

        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut line_main_sum: Vec<f32> = Vec::new();
        let mut line_cross_max: Vec<f32> = Vec::new();
        let mut current = Vec::new();
        let mut current_main = 0.0;
        let mut current_cross = 0.0;

        for (idx, (item_main, item_cross)) in child_sizes.iter().copied().enumerate() {
            if self.child_is_absolute(idx) {
                continue;
            }
            let next_main = if current.is_empty() {
                item_main
            } else {
                current_main + gap + item_main
            };
            if wrap && !current.is_empty() && next_main > main_limit {
                lines.push(current);
                line_main_sum.push(current_main);
                line_cross_max.push(current_cross);
                current = Vec::new();
                current_main = 0.0;
                current_cross = 0.0;
            }
            if current.is_empty() {
                current_main = item_main;
                current_cross = item_cross;
            } else {
                current_main += gap + item_main;
                current_cross = current_cross.max(item_cross);
            }
            current.push(idx);
        }
        if !current.is_empty() {
            lines.push(current);
            line_main_sum.push(current_main);
            line_cross_max.push(current_cross);
        }

        let total_main = line_main_sum.iter().fold(0.0f32, |a, &b| a.max(b));
        let total_cross = line_cross_max.iter().sum::<f32>()
            + gap * (line_cross_max.len().saturating_sub(1) as f32);

        FlexLayoutInfo {
            lines,
            line_main_sum,
            line_cross_max,
            total_main,
            total_cross,
            child_sizes,
        }
    }

    fn build_render_pipeline(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
        force_opaque: bool,
    ) -> BuildState {
        let fill_color = self.background_color.as_ref().to_rgba_f32();
        let opacity = if force_opaque { 1.0 } else { self.opacity };
        let shadow_state = self.render_box_shadows(
            graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            opacity,
        );
        ctx.set_state(shadow_state);

        let max_bw = (self
            .core
            .layout_size
            .width
            .min(self.core.layout_size.height))
            * 0.5;
        let left = self.border_widths.left.clamp(0.0, max_bw);
        let right = self.border_widths.right.clamp(0.0, max_bw);
        let top = self.border_widths.top.clamp(0.0, max_bw);
        let bottom = self.border_widths.bottom.clamp(0.0, max_bw);

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.core.layout_size.width.max(0.0),
            self.core.layout_size.height.max(0.0),
        );
        let mut fill_pass = DrawRectPass::new(
            RectPassParams {
                position: [self.core.layout_position.x, self.core.layout_position.y],
                size: [self.core.layout_size.width, self.core.layout_size.height],
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
            return ctx.into_state();
        }

        let mut border_pass = DrawRectPass::new(
            RectPassParams {
                position: [self.core.layout_position.x, self.core.layout_position.y],
                size: [self.core.layout_size.width, self.core.layout_size.height],
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

    fn render_box_shadows(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
        opacity: f32,
    ) -> BuildState {
        if self.box_shadows.is_empty() {
            return ctx.into_state();
        }
        let layout_x = self.core.layout_position.x;
        let layout_y = self.core.layout_position.y;
        let layout_w = self.core.layout_size.width.max(0.0);
        let layout_h = self.core.layout_size.height.max(0.0);
        if layout_w <= 0.0 || layout_h <= 0.0 {
            return ctx.into_state();
        }
        let outer_radii = normalize_corner_radii(self.border_radii, layout_w, layout_h);
        let shadows = self.box_shadows.clone();
        for shadow in shadows {
            let spread = shadow.spread;
            let shadow_radii =
                expand_corner_radii_for_spread(outer_radii, spread, layout_w, layout_h);
            let mesh = ShadowMesh::rounded_rect_with_radii(
                layout_x - spread,
                layout_y - spread,
                layout_w + spread * 2.0,
                layout_h + spread * 2.0,
                shadow_radii.to_array(),
            );
            let params = ShadowParams {
                offset_x: shadow.offset_x,
                offset_y: shadow.offset_y,
                blur_radius: shadow.blur.max(0.0),
                color: shadow.color.to_rgba_f32(),
                opacity: opacity.clamp(0.0, 1.0),
                spread: 0.0,
                clip_to_geometry: false,
            };
            let next_state = self.push_shadow_pass(
                mesh,
                params,
                graph,
                UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
            );
            ctx.set_state(next_state);
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

    pub(crate) fn build_base_only(
        &mut self,
        graph: &mut FrameGraph,
        ctx: UiBuildContext,
    ) -> BuildState {
        self.build_base_descendants_only(graph, ctx, false)
    }

    pub(crate) fn compose_promoted_descendants_only(
        &mut self,
        graph: &mut FrameGraph,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        let has_deferred_descendants = self.children.iter().any(|child| {
            child.as_any()
                .downcast_ref::<Element>()
                .is_some_and(Element::should_append_to_root_viewport_render)
        });
        let has_promoted_descendants = self.has_composited_promoted_descendants(&ctx);

        let previous_scissor_rect = self
            .absolute_clip_scissor_rect()
            .map(|scissor| ctx.push_scissor_rect(Some(scissor)));

        if has_promoted_descendants || has_deferred_descendants {
            let overflow_child_indices: Vec<bool> = (0..self.children.len())
                .map(|idx| self.child_renders_outside_inner_clip(idx))
                .collect();
            let outer_radii = normalize_corner_radii(
                self.border_radii,
                self.core.layout_size.width.max(0.0),
                self.core.layout_size.height.max(0.0),
            );
            let inner_radii = self.inner_clip_radii(outer_radii);
            let should_clip_promoted_descendants =
                self.should_clip_children(&overflow_child_indices, inner_radii);
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

            if self.layout_inner_size.width > 0.0 && self.layout_inner_size.height > 0.0 {
                for (idx, child) in self.children.iter_mut().enumerate() {
                    if overflow_child_indices.get(idx).copied().unwrap_or(false) {
                        continue;
                    }
                    if ctx.is_node_promoted(child.id()) {
                        Self::build_promoted_child(
                            graph,
                            &mut ctx,
                            child,
                            mask_target,
                        );
                        continue;
                    }
                    if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                        let viewport = ctx.viewport();
                        let next_state = if element.requires_inline_promotion_rendering() {
                            element.build(graph, ctx)
                        } else {
                            element.compose_promoted_descendants_only(graph, ctx)
                        };
                        ctx = UiBuildContext::from_parts(viewport, next_state);
                    }
                }
            }

            for (idx, is_overflow) in overflow_child_indices.into_iter().enumerate() {
                if !is_overflow {
                    continue;
                }
                if let Some(child) = self.children.get_mut(idx) {
                    if child
                        .as_any()
                        .downcast_ref::<Element>()
                        .is_some_and(Element::should_append_to_root_viewport_render)
                    {
                        ctx.append_to_defer(child.id());
                        continue;
                    }
                    if ctx.is_node_promoted(child.id()) {
                        Self::build_promoted_child(
                            graph,
                            &mut ctx,
                            child,
                            mask_target,
                        );
                        continue;
                    }
                    if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                        let viewport = ctx.viewport();
                        let next_state = if element.requires_inline_promotion_rendering() {
                            element.build(graph, ctx)
                        } else {
                            element.compose_promoted_descendants_only(graph, ctx)
                        };
                        ctx = UiBuildContext::from_parts(viewport, next_state);
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
            ctx.graphics_pass_context(),
            ctx.state.scissor_rect,
        );
    }

    fn composite_layer_target_into_current(
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        layer_target: RenderTargetOut,
        composite_bounds: crate::view::base_component::PromotionCompositeBounds,
        opacity: f32,
        pass_context: crate::view::render_pass::render_target::GraphicsPassContext,
        scissor_rect: Option<[u32; 4]>,
    ) {
        let parent_target = ctx.current_target().unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
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
                pass_context,
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
        let mut mask_ctx = UiBuildContext::from_parts(
            ctx.viewport(),
            BuildState::for_layer_subtree_with_ancestor_clip(ctx.ancestor_clip_context()),
        );
        let mask_target = mask_ctx.allocate_persistent_target_with_key(
            graph,
            crate::view::base_component::promoted_clip_mask_stable_key(self.id()),
            self.promotion_composite_bounds(),
        );
        mask_ctx.set_current_target(mask_target);
        let mut pass = DrawRectPass::new(
            RectPassParams {
                position: [self.layout_inner_position.x, self.layout_inner_position.y],
                size: [self.layout_inner_size.width, self.layout_inner_size.height],
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
        let composite_bounds = child.promotion_composite_bounds();
        let pass = crate::view::render_pass::TextureCompositePass::new(
            crate::view::render_pass::TextureCompositeParams {
                bounds: [
                    composite_bounds.x,
                    composite_bounds.y,
                    composite_bounds.width,
                    composite_bounds.height,
                ],
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

    fn has_composited_promoted_descendants(&self, ctx: &UiBuildContext) -> bool {
        let Some(children) = self.children() else {
            return false;
        };
        for child in children {
            if child
                .as_any()
                .downcast_ref::<Element>()
                .is_some_and(Element::should_append_to_root_viewport_render)
            {
                continue;
            }
            if ctx.is_node_promoted(child.id()) {
                return true;
            }
            if let Some(element) = child.as_any().downcast_ref::<Element>() {
                if element.has_composited_promoted_descendants(ctx) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn build_promoted_layer(
        &mut self,
        graph: &mut FrameGraph,
        ctx: UiBuildContext,
        requested_update_kind: crate::view::promotion::PromotedLayerUpdateKind,
        can_reuse_base: bool,
        context: crate::view::viewport::DebugReusePathContext,
    ) -> BuildState {
        let viewport = ctx.viewport();
        let mut ctx = ctx;
        if can_reuse_base {
            self.collect_root_viewport_deferred_descendants(&mut ctx);
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
            self.build_base_only(graph, ctx)
        };

        let probe_ctx = UiBuildContext::from_parts(viewport.clone(), base_state.clone());
        let has_composited_descendants = self.has_composited_promoted_descendants(&probe_ctx);
        let requested_composition_update_kind = probe_ctx
            .promoted_composition_update_kind(self.id())
            .unwrap_or(crate::view::promotion::PromotedLayerUpdateKind::Reraster);
        let can_reuse_final = can_reuse_base
            && matches!(
                requested_composition_update_kind,
                crate::view::promotion::PromotedLayerUpdateKind::Reuse
            );
        crate::view::viewport::record_debug_reuse_path(
            crate::view::viewport::DebugReusePathRecord {
                node_id: self.id(),
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
            crate::view::base_component::promoted_final_layer_stable_key(self.id()),
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
        self.compose_promoted_descendants_only(graph, compose_ctx)
    }

    fn build_promoted_child(
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        child: &mut Box<dyn ElementTrait>,
        mask_target: Option<RenderTargetOut>,
    ) {
        let child_id = child.id();
        let requested_update = ctx
            .promoted_update_kind(child_id)
            .unwrap_or(crate::view::promotion::PromotedLayerUpdateKind::Reraster);
        let has_ancestor_scissor = ctx.scissor_rect().is_some();
        let has_ancestor_stencil = ctx.current_clip_id() != 0;
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
            let next_state = child.build(graph, UiBuildContext::from_parts(viewport, ctx.state_clone()));
            ctx.set_state(next_state);
            let _ = mask_target;
            return;
        }
        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
            if let Some(reason) = element.inline_promotion_rendering_reason() {
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
                let next_state =
                    element.build(graph, UiBuildContext::from_parts(viewport, ctx.state_clone()));
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
            child.id(),
            child.promotion_composite_bounds(),
        );
        child_ctx.set_current_target(layer_target);
        let child_state = if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
            element.build_promoted_layer(
                graph,
                child_ctx,
                update_kind,
                can_reuse,
                crate::view::viewport::DebugReusePathContext::Child,
            )
        } else if can_reuse {
            crate::view::viewport::record_debug_reuse_path(
                crate::view::viewport::DebugReusePathRecord {
                    node_id: child.id(),
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
                    node_id: child.id(),
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
            child.build(graph, child_ctx)
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
