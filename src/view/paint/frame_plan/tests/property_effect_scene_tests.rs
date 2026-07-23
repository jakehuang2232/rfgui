use super::*;

#[test]
fn property_effect_scaffold_freezes_nested_chain_and_opacity_zero_structure() {
    let (arena, root, child, grandchild, properties, generations) =
        planning_only_nested_effect_fixture();
    let plan = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("pure nested opacity forest must produce a planning seal");
    assert!(plan.property_effect_scaffold_is_sealed_for_test());
    assert!(plan.property_scene_transaction_witness().is_none());
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .and_then(|seal| seal.effect_scaffold.as_ref())
        .expect("effect scaffold");
    assert_eq!(
        scaffold
            .surfaces
            .iter()
            .map(|surface| surface.boundary.owner())
            .collect::<Vec<_>>(),
        vec![root, child, grandchild]
    );
    let PropertyEffectSurfaceKind::Isolation(child_surface) = &scaffold.surfaces[1].kind else {
        panic!("child effect surface")
    };
    assert_eq!(child_surface.composite.opacity_bits, 0.0_f32.to_bits());
    assert_eq!(child_surface.effect_chain.live_leaf_to_root.len(), 2);
    assert_eq!(child_surface.effect_chain.detached_ancestors.len(), 1);
    assert_eq!(child_surface.effect_chain.isolated_leaf.parent, None);
    assert!(!child_surface.raster_identity.content.is_empty());
    assert_eq!(child_surface.parent_opaque_cursor_delta, 0);
    let PropertyEffectSurfaceKind::Isolation(parent_surface) = &scaffold.surfaces[0].kind
    else {
        panic!("parent effect surface")
    };
    assert_eq!(parent_surface.nested_dependencies.len(), 1);
    assert_eq!(
        parent_surface.nested_dependencies[0].child_opacity_bits,
        0.0_f32.to_bits()
    );
}

#[test]
fn property_effect_scene_materializes_pure_nested_opacity_forest() {
    let (arena, root, child, grandchild, properties, generations) =
        planning_only_nested_effect_fixture();
    let plan = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("canonical nested opacity forest must materialize");
    assert!(property_scene_plan_is_sealed(&plan));
    let witness = plan
        .property_scene_transaction_witness()
        .expect("production effect scene transaction witness");
    assert_eq!(witness.roots.len(), 1);
    assert_eq!(witness.surfaces.len(), 3);
    assert_eq!(
        witness
            .surfaces
            .iter()
            .map(|surface| (surface.boundary_root, surface.parent_surface))
            .collect::<Vec<_>>(),
        vec![(root, None), (child, Some(root)), (grandchild, Some(child))]
    );
    assert!(
        witness.surfaces.iter().all(|surface| matches!(
            surface.kind,
            PropertySceneTransactionSurfaceKind::Effect(_)
        ))
    );
    assert_eq!(plan.steps.len(), 1);
    let PaintPlanStep::RetainedSurface(root_surface) = &plan.steps[0] else {
        panic!("effect root must be a top-level retained surface")
    };
    assert!(matches!(root_surface.kind, SurfaceKind::NestedIsolation(_)));

    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    let before = graph.build_state_snapshot_for_test();
    let prepared_stamps = super::super::super::prepare_retained_property_scene_stamps_for_test(
        &viewport, &plan, &graph, &ctx,
    )
    .expect("effect stamps are fully prepared before graph mutation");
    assert_eq!(prepared_stamps.len(), 3);
    assert!(prepared_stamps.iter().all(|stamp| {
        stamp.identity.role == super::super::super::RetainedSurfaceRasterRole::PropertyEffect
            && stamp.property_effect.is_some()
    }));
    let root_dependency = prepared_stamps[0]
        .ordered_steps
        .iter()
        .find_map(|step| match step {
            super::super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                Some(dependency)
            }
            super::super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(_) => None,
            super::super::super::RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => None,
        })
        .expect("root stamp embeds the direct child composite dependency");
    let super::super::super::RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
        opacity_bits,
        effect_generation,
        basis,
        resolved_scissor,
        ancestor_composite_clips,
        ..
    } = &root_dependency.child_composite_geometry
    else {
        panic!("dedicated property-effect composite dependency")
    };
    assert_eq!(*opacity_bits, 0.0_f32.to_bits());
    assert_eq!(
        *effect_generation,
        properties.effects[&EffectNodeId(child)].generation
    );
    assert_eq!(
        *basis,
        super::super::super::compiler::PropertyEffectCompositeBasisStamp::ParentEffect(EffectNodeId(
            root
        ))
    );
    assert_eq!(*resolved_scissor, None);
    assert!(ancestor_composite_clips.is_empty());
    assert_eq!(graph.build_state_snapshot_for_test(), before);
    let outcome = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("pure nested effect scene must preflight and emit");
    let (state, trace) = outcome.into_parts();
    assert_eq!(trace.root_count, 1);
    assert_eq!(trace.surface_count, 3);
    assert_eq!(trace.reraster_count, 3);
    assert_eq!(
        state.opaque_rect_order_for_test(),
        0,
        "nested opacity composites must not leak their raster-local opaque cursors"
    );
    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
    assert_eq!(composites.len(), 3);
    assert_eq!(
        composites
            .iter()
            .map(|pass| pass.test_snapshot().opacity_bits)
            .collect::<Vec<_>>(),
        vec![0.75_f32.to_bits(), 0.0_f32.to_bits(), 0.5_f32.to_bits()],
        "each effect opacity is applied once, on its own child-to-parent composite edge"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(3)),
        "the arbitrary-depth forest stages one exact atomic transaction"
    );
    assert_ne!(graph.build_state_snapshot_for_test(), before);
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (3, None)
    );
}

#[test]
fn property_effect_transaction_rejects_forged_terminal_shape_basis_and_scissor() {
    let (arena, root, child, grandchild, properties, generations) =
        planning_only_nested_effect_fixture();
    let plan = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("canonical effect transaction fixture");
    let viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    let stamps = super::super::super::prepare_retained_property_scene_stamps_for_test(
        &viewport, &plan, &graph, &ctx,
    )
    .expect("canonical prepared stamps");
    let witness = plan
        .property_scene_transaction_witness()
        .expect("canonical effect witness");
    let step_count = plan.steps.len();
    let aggregate = witness.aggregate_opaque_order_span.clone();
    assert!(
        super::super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
            witness.clone(),
            &stamps,
            step_count,
            aggregate.clone(),
        )
        .is_some()
    );

    let mut aggregate_drift = witness.clone();
    aggregate_drift.aggregate_opaque_order_span.end += 1;
    assert!(
        super::super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
            aggregate_drift,
            &stamps,
            step_count,
            aggregate.clone(),
        )
        .is_none(),
        "the transaction must recompute and bind the actual opaque terminal"
    );

    let mut root_coverage_drift = witness.clone();
    root_coverage_drift.roots[0].top_level_step_span.end += 1;
    assert!(
        super::super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
            root_coverage_drift,
            &stamps,
            step_count,
            aggregate.clone(),
        )
        .is_none(),
        "root spans must cover exactly the prepared plan step count"
    );

    let mut transform_transform_effect = witness.clone();
    for (surface, owner) in transform_transform_effect
        .surfaces
        .iter_mut()
        .take(2)
        .zip([root, child])
    {
        surface.kind = PropertySceneTransactionSurfaceKind::Transform(TransformNodeId(owner));
        surface.transform_viewport_matrix_bits =
            Some(glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits));
        surface.effect_composite = None;
    }
    assert_eq!(
        transform_transform_effect.surfaces[2].boundary_root,
        grandchild
    );
    assert!(
        super::super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
            transform_transform_effect,
            &stamps,
            step_count,
            aggregate.clone(),
        )
        .is_none(),
        "effect transaction admission must reject Transform -> Transform -> Effect"
    );

    let mut basis_drift = stamps.clone();
    let dependency = basis_drift[0]
        .ordered_steps
        .iter_mut()
        .find_map(|step| match step {
            super::super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                Some(dependency)
            }
            super::super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(_) => None,
            super::super::super::RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => None,
        })
        .expect("root effect embeds its child");
    let super::super::super::RetainedSurfaceCompositeGeometryStamp::PropertyEffect { basis, .. } =
        &mut dependency.child_composite_geometry
    else {
        panic!("effect dependency geometry")
    };
    *basis = super::super::super::compiler::PropertyEffectCompositeBasisStamp::ParentEffect(
        EffectNodeId(grandchild),
    );
    assert!(
        super::super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
            witness.clone(),
            &basis_drift,
            step_count,
            aggregate.clone(),
        )
        .is_none(),
        "child composite basis must match the actual parent witness"
    );

    let mut scissor_drift = stamps.clone();
    let dependency = scissor_drift[0]
        .ordered_steps
        .iter_mut()
        .find_map(|step| match step {
            super::super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                Some(dependency)
            }
            super::super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(_) => None,
            super::super::super::RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | super::super::super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => None,
        })
        .expect("root effect embeds its child");
    let super::super::super::RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
        resolved_scissor,
        ..
    } = &mut dependency.child_composite_geometry
    else {
        panic!("effect dependency geometry")
    };
    *resolved_scissor = Some([0, 0, 1, 1]);
    assert!(
        super::super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
            witness,
            &scissor_drift,
            step_count,
            aggregate,
        )
        .is_none(),
        "resolved scissor must match the child surface witness exactly"
    );
}

#[test]
fn property_effect_scene_mismatch_rejects_before_graph_pool_or_pending_mutation() {
    let (arena, root, _, _, properties, generations) = planning_only_nested_effect_fixture();
    let plan = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("canonical property effect scene");

    let mut viewport = Viewport::new();
    let mut baseline_graph = FrameGraph::new();
    let baseline_ctx = parent_context_without_clear(&mut baseline_graph, 160, 120, 1.0);
    super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &plan,
        &mut baseline_graph,
        baseline_ctx,
    )
    .expect("baseline transaction");
    viewport.finish_retained_surface_transaction(true);

    let mut mismatch = plan.clone();
    let PaintPlanStep::RetainedSurface(root_surface) = &mut mismatch.steps[0] else {
        panic!("effect root surface")
    };
    let SurfaceKind::NestedIsolation(root_effect) = &mut root_surface.kind else {
        panic!("effect root kind")
    };
    root_effect
        .property_scene
        .as_mut()
        .expect("property contract")
        .composite
        .effect_generation += 1;

    let graph = FrameGraph::new();
    let ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let error = match super::super::super::prepare_retained_property_scene_from_pool(
        &viewport, &mismatch, &graph, &ctx,
    ) {
        Ok(_) => panic!("drifted effect contract cannot mint a pre-clear token"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        super::super::super::ForcedTransformSurfaceError::PlanShape
            | super::super::super::ForcedTransformSurfaceError::GeometryContract
    ));
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before,
        "failed preparation preserves both resident pool and pending transaction"
    );
}

#[test]
fn property_effect_scene_child_opacity_reuses_child_and_rerasterizes_direct_parent() {
    let (arena, root, child, grandchild, mut properties, mut generations) =
        planning_only_nested_effect_fixture();
    let build_plan = |properties: &PropertyTrees, generations: &PaintGenerationTracker| {
        plan_property_effect_scene_with_context(
            &arena,
            &[root],
            properties,
            generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("canonical effect scene")
    };
    let baseline = build_plan(&properties, &generations);
    let mut viewport = Viewport::new();
    let mut baseline_graph = FrameGraph::new();
    let baseline_ctx = parent_context_without_clear(&mut baseline_graph, 160, 120, 1.0);
    super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &baseline,
        &mut baseline_graph,
        baseline_ctx,
    )
    .expect("baseline effect transaction");
    viewport.finish_retained_surface_transaction(true);

    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let changed = build_plan(&properties, &generations);
    let mut changed_graph = FrameGraph::new();
    let changed_ctx = parent_context_without_clear(&mut changed_graph, 160, 120, 1.0);
    let changed = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &changed,
        &mut changed_graph,
        changed_ctx,
    )
    .expect("opacity-only effect transaction");
    let (_, trace) = changed.into_parts();
    let actions = trace
        .surfaces
        .iter()
        .map(|surface| (surface.boundary_root, surface.action))
        .collect::<FxHashMap<_, _>>();
    assert_eq!(
        actions[&child],
        super::super::super::RetainedSurfaceCompileAction::Reuse,
        "own opacity/effect generation are excluded from the child's own raster stamp"
    );
    assert_eq!(
        actions[&grandchild],
        super::super::super::RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        actions[&root],
        super::super::super::RetainedSurfaceCompileAction::Reraster,
        "direct parent dependency includes child opacity and effect generation"
    );
    assert_eq!(trace.reraster_count, 1);
    assert_eq!(trace.reuse_count, 2);
    viewport.finish_retained_surface_transaction(true);

    crate::view::test_support::get_element_mut::<Element>(&arena, grandchild).set_opacity(0.6);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let transitive = build_plan(&properties, &generations);
    let mut transitive_graph = FrameGraph::new();
    let transitive_ctx = parent_context_without_clear(&mut transitive_graph, 160, 120, 1.0);
    let transitive = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &transitive,
        &mut transitive_graph,
        transitive_ctx,
    )
    .expect("grandchild opacity-only transaction");
    let (_, trace) = transitive.into_parts();
    let actions = trace
        .surfaces
        .iter()
        .map(|surface| (surface.boundary_root, surface.action))
        .collect::<FxHashMap<_, _>>();
    assert_eq!(
        actions[&grandchild],
        super::super::super::RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        actions[&child],
        super::super::super::RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        actions[&root],
        super::super::super::RetainedSurfaceCompileAction::Reraster,
        "the child's changed full stamp propagates into its own parent dependency"
    );
    assert_eq!(trace.reraster_count, 2);
    assert_eq!(trace.reuse_count, 1);
}

#[test]
fn property_effect_scene_hidden_content_reraster_then_nonzero_reuses_child() {
    let (arena, root, child, grandchild, mut properties, mut generations) =
        planning_only_nested_effect_fixture();
    let build_plan = |properties: &PropertyTrees, generations: &PaintGenerationTracker| {
        plan_property_effect_scene_with_context(
            &arena,
            &[root],
            properties,
            generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("canonical hidden effect scene")
    };
    let baseline = build_plan(&properties, &generations);
    let mut viewport = Viewport::new();
    let mut baseline_graph = FrameGraph::new();
    let baseline_ctx = parent_context_without_clear(&mut baseline_graph, 160, 120, 1.0);
    super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &baseline,
        &mut baseline_graph,
        baseline_ctx,
    )
    .expect("hidden baseline");
    viewport.finish_retained_surface_transaction(true);

    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(12, 180, 90)),
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, child).apply_style(style);
    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.0);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let hidden_changed = build_plan(&properties, &generations);
    let mut hidden_graph = FrameGraph::new();
    let hidden_ctx = parent_context_without_clear(&mut hidden_graph, 160, 120, 1.0);
    let hidden = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &hidden_changed,
        &mut hidden_graph,
        hidden_ctx,
    )
    .expect("hidden content change must remain structurally paintable");
    let (_, hidden_trace) = hidden.into_parts();
    let hidden_actions = hidden_trace
        .surfaces
        .iter()
        .map(|surface| (surface.boundary_root, surface.action))
        .collect::<FxHashMap<_, _>>();
    assert_eq!(
        hidden_actions[&child],
        super::super::super::RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        hidden_actions[&root],
        super::super::super::RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        hidden_actions[&grandchild],
        super::super::super::RetainedSurfaceCompileAction::Reuse
    );
    viewport.finish_retained_surface_transaction(true);

    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.4);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let revealed = build_plan(&properties, &generations);
    let mut revealed_graph = FrameGraph::new();
    let revealed_ctx = parent_context_without_clear(&mut revealed_graph, 160, 120, 1.0);
    let revealed = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &revealed,
        &mut revealed_graph,
        revealed_ctx,
    )
    .expect("revealing updated hidden content");
    let (_, revealed_trace) = revealed.into_parts();
    let revealed_actions = revealed_trace
        .surfaces
        .iter()
        .map(|surface| (surface.boundary_root, surface.action))
        .collect::<FxHashMap<_, _>>();
    assert_eq!(
        revealed_actions[&child],
        super::super::super::RetainedSurfaceCompileAction::Reuse,
        "0->nonzero changes only composite authority after hidden content was rerastered"
    );
    assert_eq!(
        revealed_actions[&root],
        super::super::super::RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        revealed_actions[&grandchild],
        super::super::super::RetainedSurfaceCompileAction::Reuse
    );
}

#[test]
fn property_effect_scene_executes_only_proven_mixed_transform_effect_shape() {
    let (arena, root, _, child, _, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    let plan = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("proven mixed transform/effect scene");
    let witness = plan
        .property_scene_transaction_witness()
        .expect("mixed transaction witness");
    assert!(matches!(
        witness.surfaces.as_slice(),
        [
            PropertySceneTransactionSurfaceWitness {
                kind: PropertySceneTransactionSurfaceKind::Transform(_),
                parent_surface: None,
                ..
            },
            PropertySceneTransactionSurfaceWitness {
                kind: PropertySceneTransactionSurfaceKind::Effect(_),
                parent_surface: Some(parent),
                boundary_root,
                ..
            }
        ] if *parent == root && *boundary_root == child
    ));

    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    let outcome = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("mixed scene preflight and infallible emit");
    let (state, trace) = outcome.into_parts();
    assert_eq!(trace.surface_count, 2);
    assert_eq!(trace.reraster_count, 2);
    assert_eq!(
        state.opaque_rect_order_for_test(),
        plan.property_scene_seal
            .as_ref()
            .expect("seal")
            .aggregate_opaque_order_span
            .end
    );
    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
    assert_eq!(composites.len(), 1);
    assert_eq!(
        composites[0].test_snapshot().opacity_bits,
        0.5_f32.to_bits(),
        "the child effect applies opacity exactly once"
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        1,
        "the transform root composites exactly once"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(2))
    );
}
