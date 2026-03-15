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

        let viewport = ctx.viewport();
        let base_state = self.build_base_only(graph, ctx);
        self.compose_promoted_descendants_only(graph, UiBuildContext::from_parts(viewport, base_state))
    }
}
