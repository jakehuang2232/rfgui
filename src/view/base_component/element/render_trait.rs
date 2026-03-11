impl Renderable for Element {
    fn build(&mut self, graph: &mut FrameGraph, mut ctx: UiBuildContext) -> BuildState {
        if trace_layout_enabled() {
            eprintln!(
                "[build ] pos=({:.1},{:.1}) size=({:.1},{:.1}) should_render={}",
                self.core.layout_position.x,
                self.core.layout_position.y,
                self.core.layout_size.width,
                self.core.layout_size.height,
                self.core.should_render
            );
        }
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
            false,
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
                let viewport = ctx.viewport();
                let next_state = child.build(graph, ctx);
                ctx = UiBuildContext::from_parts(viewport, next_state);
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
                let viewport = ctx.viewport();
                let next_state = child.build(graph, ctx);
                ctx = UiBuildContext::from_parts(viewport, next_state);
            }
        }
        self.end_child_clip_scope(graph, &mut ctx, child_clip_scope);
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
}
