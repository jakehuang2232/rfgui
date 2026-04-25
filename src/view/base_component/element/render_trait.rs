impl Renderable for Element {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        if trace_layout_enabled() {
            eprintln!(
                "[build ] pos=({:.1},{:.1}) size=({:.1},{:.1}) should_render={}",
                self.layout_state.layout_position.x,
                self.layout_state.layout_position.y,
                self.layout_state.layout_size.width,
                self.layout_state.layout_size.height,
                self.layout_state.should_render
            );
        }
        if !self.layout_state.should_render {
            if self.has_absolute_descendant_for_hit_test {
                self.collect_root_viewport_deferred_descendants(arena, &mut ctx);
            }
            return ctx.into_state();
        }

        let viewport = ctx.viewport();
        let base_state = self.build_base_only(graph, arena, ctx);
        self.compose_promoted_descendants_only(
            graph,
            arena,
            UiBuildContext::from_parts(viewport, base_state),
        )
    }
}
