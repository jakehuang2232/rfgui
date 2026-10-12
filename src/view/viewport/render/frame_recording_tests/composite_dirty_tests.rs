use super::*;

#[test]
fn successful_frame_clears_new_node_initial_composite_dirty() {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(colored_element(30, 10.0, Color::rgb(230, 20, 30))),
    );
    let roots = vec![root];
    assert!(
        arena
            .get(root)
            .expect("root should exist")
            .element
            .local_dirty_flags()
            .contains(DirtyFlags::ALL),
        "a newly committed Element begins with all local work dirty"
    );
    arena.refresh_subtree_dirty_cache(root);
    assert!(
        arena
            .cached_subtree_dirty(root)
            .contains(DirtyFlags::COMPOSITE),
        "arena subtree aggregate must include the new Element's local composite bit"
    );

    let mut properties = PropertyTrees::default();
    let mut generations = PaintGenerationTracker::default();
    observe_compositor_state(&arena, &roots, &mut properties, &mut generations);
    assert!(properties.paint_state_for(root).is_some());
    assert!(generations.snapshot(root).is_some());

    finish_frame_dirty_lifecycle(&mut arena, &roots, true, true);
    assert_consumed_dirty_cleared(&arena, root);
}

#[test]
fn opacity_composite_dirty_is_observed_in_frame_then_cleared_after_execute() {
    let (mut arena, roots) = prepared_safe_leaf();
    let root = roots[0];
    let mut properties = PropertyTrees::default();
    let mut generations = PaintGenerationTracker::default();
    observe_compositor_state(&arena, &roots, &mut properties, &mut generations);
    let initial_composite_revision = generations
        .snapshot(root)
        .expect("initial generation should exist")
        .composite_revision;
    finish_frame_dirty_lifecycle(&mut arena, &roots, true, true);

    set_opacity_with_invalidation(&mut arena, root, 0.5);
    assert_composite_dirty_preserved(&arena, root);
    observe_compositor_state(&arena, &roots, &mut properties, &mut generations);

    let paint_state = properties
        .paint_state_for(root)
        .expect("property state should be observed before build");
    let effect = paint_state
        .effect
        .expect("non-unit opacity should create an effect node");
    assert_eq!(
        properties.effects[&effect].opacity.to_bits(),
        0.5_f32.to_bits()
    );
    assert_ne!(
        generations
            .snapshot(root)
            .expect("updated generation should exist")
            .composite_revision,
        initial_composite_revision,
        "paint generation must consume this frame's effect change before dirty clear"
    );

    finish_frame_dirty_lifecycle(&mut arena, &roots, true, true);
    assert_consumed_dirty_cleared(&arena, root);
}

#[test]
fn compile_or_execute_failure_preserves_composite_dirty() {
    for (compiled, executed) in [(false, false), (true, false)] {
        let (mut arena, roots) = prepared_safe_leaf();
        let root = roots[0];
        let mut properties = PropertyTrees::default();
        let mut generations = PaintGenerationTracker::default();
        observe_compositor_state(&arena, &roots, &mut properties, &mut generations);
        finish_frame_dirty_lifecycle(&mut arena, &roots, true, true);

        set_opacity_with_invalidation(&mut arena, root, 0.5);
        observe_compositor_state(&arena, &roots, &mut properties, &mut generations);
        finish_frame_dirty_lifecycle(&mut arena, &roots, compiled, executed);

        assert_composite_dirty_preserved(&arena, root);
        assert!(
            arena
                .get(root)
                .expect("root should exist")
                .element
                .local_dirty_flags()
                .contains(DirtyFlags::PAINT),
            "failed frame must preserve the coupled paint work"
        );
    }
}
