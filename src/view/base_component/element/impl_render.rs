impl Element {
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
}
