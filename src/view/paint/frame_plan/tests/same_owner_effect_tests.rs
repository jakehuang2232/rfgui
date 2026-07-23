use super::*;

#[test]
fn same_owner_transform_effect_seals_prepares_emits_and_compiles() {
    let (arena, root, _before, child, ..) = nested_exact_transform_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);

    let plan = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("same-owner Transform -> Effect must materialize");
    let witness = plan
        .property_scene_transaction_witness()
        .expect("same-owner transaction");
    assert_eq!(witness.surfaces.len(), 2);
    assert_eq!(witness.surfaces[0].boundary_root, root);
    assert_eq!(witness.surfaces[1].boundary_root, root);
    assert!(matches!(
        witness.surfaces[0].kind,
        PropertySceneTransactionSurfaceKind::Transform(_)
    ));
    assert!(matches!(
        witness.surfaces[1].kind,
        PropertySceneTransactionSurfaceKind::Effect(_)
    ));

    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
    let outcome = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("same-owner pair must prepare and emit");
    assert_eq!(outcome.into_parts().1.surface_count, 2);
    let compile = graph.test_compile_snapshot();
    assert!(compile.is_ok(), "{compile:?}");
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        1
    );
    let effects = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].test_snapshot().opacity_bits, 0.5_f32.to_bits());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn same_owner_image_and_svg_transform_effect_matrix_stays_retained() {
    for host in ["image", "svg"] {
        for state in ["ready", "loading", "error"] {
            for opacity in [0.0_f32, 0.5] {
                let (arena, root, child, properties, generations) =
                    native_nested_effect_fixture(host, state, opacity, true, false);
                let plan = plan_property_effect_scene_with_context(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    TransformSurfacePlanContext::default(),
                )
                .unwrap_or_else(|error| {
                    panic!("same-owner {host}/{state}/{opacity} fallback: {error:?}")
                });
                let witness = plan
                    .property_scene_transaction_witness()
                    .expect("same-owner native transaction");
                assert_eq!(witness.surfaces.len(), 2, "{host}/{state}");
                assert!(witness.surfaces.iter().all(|surface| {
                    surface.boundary_root == child && surface.stable_id != 0
                }));
                assert!(matches!(
                    witness.surfaces[0].kind,
                    PropertySceneTransactionSurfaceKind::Transform(_)
                ));
                assert!(matches!(
                    witness.surfaces[1].kind,
                    PropertySceneTransactionSurfaceKind::Effect(_)
                ));
                assert_ne!(
                    witness.surfaces[0].persistent_color_key,
                    witness.surfaces[1].persistent_color_key,
                    "typed boundaries require distinct pool resources"
                );

                let mut viewport = Viewport::new();
                let mut graph = FrameGraph::new();
                let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
                let outcome =
                    super::super::super::build_retained_property_scene_with_forced_pool_for_test(
                        &mut viewport,
                        &plan,
                        &mut graph,
                        ctx,
                    )
                    .unwrap_or_else(|error| {
                        panic!("same-owner {host}/{state}/{opacity} emit: {error:?}")
                    });
                assert_eq!(outcome.into_parts().1.surface_count, 2);
                let compile = graph.test_compile_snapshot();
                assert!(compile.is_ok(), "{host}/{state}: {compile:?}");
                assert_eq!(
                    graph
                        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>(
                        )
                        .iter()
                        .filter(|pass| pass.test_snapshot().sampled_source.is_none())
                        .count(),
                    1,
                    "{host}/{state} transform composite"
                );
                let effects = graph
                    .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
                assert_eq!(effects.len(), 1, "{host}/{state} effect composite");
                assert_eq!(
                    effects[0].test_snapshot().opacity_bits,
                    opacity.to_bits(),
                    "{host}/{state} opacity"
                );
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn same_owner_transform_effect_tamper_matrix_generation_bounds_resource_role_and_order_fail_closed()
 {
    let (arena, root, _child, properties, generations) =
        native_nested_effect_fixture("image", "ready", 0.5, true, false);
    let build = || {
        plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("canonical same-owner image")
    };
    fn outer(plan: &mut FramePaintPlan) -> &mut RetainedSurfacePlan {
        plan.steps
            .iter_mut()
            .find_map(|step| match step {
                PaintPlanStep::RetainedSurface(surface)
                    if matches!(surface.kind, SurfaceKind::Transform(_)) =>
                {
                    Some(surface.as_mut())
                }
                _ => None,
            })
            .expect("outer transform")
    }
    fn inner(plan: &mut FramePaintPlan) -> &mut RetainedSurfacePlan {
        outer(plan)
            .raster_steps
            .iter_mut()
            .find_map(|step| match step {
                PaintPlanStep::RetainedSurface(surface)
                    if matches!(surface.kind, SurfaceKind::NestedIsolation(_)) =>
                {
                    Some(surface.as_mut())
                }
                _ => None,
            })
            .expect("inner effect")
    }

    let mut matrix = build();
    let SurfaceKind::Transform(transform) = &mut outer(&mut matrix).kind else {
        unreachable!()
    };
    transform.geometry.viewport_transform.w_axis.x += 1.0;
    assert!(!property_scene_plan_is_sealed(&matrix));

    let mut generation = build();
    let SurfaceKind::NestedIsolation(effect) = &mut inner(&mut generation).kind else {
        unreachable!()
    };
    effect
        .property_scene
        .as_mut()
        .unwrap()
        .composite
        .effect_generation += 1;
    assert!(!property_scene_plan_is_sealed(&generation));

    let mut bounds = build();
    let SurfaceKind::NestedIsolation(effect) = &mut inner(&mut bounds).kind else {
        unreachable!()
    };
    effect.geometry.source_bounds.width += 1.0;
    assert!(!property_scene_plan_is_sealed(&bounds));

    let mut duplicate_key = build();
    let outer_key = outer(&mut duplicate_key).persistent_color_key;
    inner(&mut duplicate_key).persistent_color_key = outer_key;
    assert!(!property_scene_plan_is_sealed(&duplicate_key));

    let mut reversed = build();
    reversed
        .property_scene_seal
        .as_mut()
        .unwrap()
        .effect_scaffold
        .as_mut()
        .unwrap()
        .surfaces
        .swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&reversed));

    let plan = build();
    let witness = plan.property_scene_transaction_witness().unwrap();
    let viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
    let mut stamps = super::super::super::prepare_retained_property_scene_stamps_for_test(
        &viewport, &plan, &graph, &ctx,
    )
    .expect("canonical stamps");
    stamps[1].identity.role = super::super::super::RetainedSurfaceRasterRole::Transform;
    assert!(
        super::super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
            witness,
            &stamps,
            plan.steps.len(),
            plan.property_scene_seal
                .as_ref()
                .unwrap()
                .aggregate_opaque_order_span
                .clone(),
        )
        .is_none(),
        "same-owner effect cannot masquerade as a duplicate transform role"
    );

    let (
        mut resource_arena,
        resource_root,
        resource_child,
        resource_properties,
        resource_generations,
    ) = native_nested_effect_fixture("image", "loading", 0.5, true, false);
    resource_arena
        .get(resource_child)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Image>()
        .unwrap()
        .set_resource_error_for_test();
    resource_arena
        .with_element_taken(resource_child, |element, arena| element.sync_arena(arena))
        .expect("install changed same-owner resource snapshot");
    assert!(
        plan_property_effect_scene_with_context(
            &resource_arena,
            &[resource_root],
            &resource_properties,
            &resource_generations,
            TransformSurfacePlanContext::default(),
        )
        .is_err()
    );

    let (mut topology_arena, topology_root, _child, topology_properties, topology_generations) =
        native_nested_effect_fixture("image", "ready", 0.5, true, false);
    let _ = commit_child(
        &mut topology_arena,
        topology_root,
        Box::new(Element::new_with_id(0xc1_12ff, 0.0, 0.0, 1.0, 1.0)),
    );
    assert!(
        plan_property_effect_scene_with_context(
            &topology_arena,
            &[topology_root],
            &topology_properties,
            &topology_generations,
            TransformSurfacePlanContext::default(),
        )
        .is_err()
    );
}
