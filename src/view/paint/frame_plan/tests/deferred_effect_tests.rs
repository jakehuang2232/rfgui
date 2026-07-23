use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn deferred_image_and_svg_effects_record_once_late_and_compile() {
    for host in ["image", "svg"] {
        for state in ["ready", "loading", "error"] {
            for opacity in [0.0_f32, 0.5] {
                let (arena, root, child, properties, generations) =
                    native_nested_effect_fixture(host, state, opacity, false, true);
                assert!(
                    arena
                        .get(child)
                        .unwrap()
                        .element
                        .is_deferred_to_root_viewport_render()
                );
                let plan = plan_property_effect_scene_with_context(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    TransformSurfacePlanContext::default(),
                )
                .unwrap_or_else(|error| {
                    panic!("deferred {host}/{state}/{opacity} fallback: {error:?}")
                });
                let witness = plan.property_scene_transaction_witness().unwrap();
                assert_eq!(witness.surfaces.len(), 1);
                assert_eq!(witness.surfaces[0].boundary_root, child);
                assert!(matches!(
                    witness.surfaces[0].kind,
                    PropertySceneTransactionSurfaceKind::Effect(_)
                ));

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
                        panic!("deferred {host}/{state}/{opacity} emit: {error:?}")
                    });
                assert_eq!(outcome.into_parts().1.surface_count, 1);
                let compile = graph.test_compile_snapshot();
                assert!(compile.is_ok(), "{host}/{state}: {compile:?}");
                let composites = graph
                    .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
                assert_eq!(composites.len(), 1);
                assert_eq!(
                    composites[0].test_snapshot().opacity_bits,
                    opacity.to_bits()
                );
                if state == "ready" {
                    let media_draws = graph
                        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>(
                        )
                        .iter()
                        .filter(|pass| pass.test_snapshot().sampled_source.is_some())
                        .count();
                    assert_eq!(media_draws, 1, "normal+late must not duplicate {host}");
                }
            }
        }
    }
}

#[test]
fn deferred_element_nested_subtree_effect_records_once_late_and_compiles() {
    for opacity in [0.0_f32, 0.5] {
        let (arena, root, deferred, nested, properties, generations) =
            deferred_element_effect_fixture(opacity);
        let plan = plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .unwrap_or_else(|error| panic!("deferred Element/{opacity}: {error:?}"));
        assert!(plan.steps.iter().all(|step| {
            match step {
                PaintPlanStep::ArtifactSpan(span) => span
                    .artifact
                    .chunks
                    .iter()
                    .all(|chunk| chunk.owner != deferred && chunk.owner != nested),
                PaintPlanStep::RetainedSurface(_) => true,
            }
        }));
        let mut graph = FrameGraph::new();
        let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
        let mut viewport = Viewport::new();
        let stamps = super::super::super::prepare_retained_property_scene_stamps_for_test(
            &viewport, &plan, &graph, &ctx,
        )
        .expect("deferred nested Element stamps");
        assert_eq!(stamps.len(), 1);
        let owners = stamps[0]
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>();
        assert_eq!(
            owners.iter().filter(|owner| **owner == deferred).count(),
            1,
            "deferred owner must be absent from normal phase and present once late"
        );
        assert_eq!(owners.iter().filter(|owner| **owner == nested).count(), 1);

        let outcome = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("deferred nested Element emit");
        assert_eq!(outcome.into_parts().1.surface_count, 1);
        assert!(graph.test_compile_snapshot().is_ok());
        let effects = graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].test_snapshot().opacity_bits, opacity.to_bits());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn deferred_effect_clip_phase_generation_opacity_resource_and_topology_tamper_fail_closed() {
    let build = || native_nested_effect_fixture("image", "ready", 0.5, false, true);

    for tamper in [
        "intersect",
        "clip-generation",
        "effect-generation",
        "opacity",
    ] {
        let (arena, root, child, mut properties, generations) = build();
        match tamper {
            "intersect" => {
                properties
                    .clips
                    .get_mut(&ClipNodeId {
                        owner: child,
                        role: ClipNodeRole::SelfClip,
                    })
                    .unwrap()
                    .behavior = ClipBehavior::Intersect;
            }
            "clip-generation" => {
                properties
                    .clips
                    .get_mut(&ClipNodeId {
                        owner: child,
                        role: ClipNodeRole::SelfClip,
                    })
                    .unwrap()
                    .generation = 0;
            }
            "effect-generation" => {
                properties
                    .effects
                    .get_mut(&EffectNodeId(child))
                    .unwrap()
                    .generation = 0;
            }
            "opacity" => {
                properties
                    .effects
                    .get_mut(&EffectNodeId(child))
                    .unwrap()
                    .opacity = f32::NAN;
            }
            _ => unreachable!(),
        }
        assert!(
            plan_property_effect_scene_with_context(
                &arena,
                &[root],
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
            .is_err(),
            "{tamper} must reject"
        );
    }

    let (arena, root, _child, properties, generations) = build();
    let mut reordered = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .unwrap();
    assert_eq!(reordered.steps.len(), 2);
    reordered.steps.swap(0, 1);
    assert!(
        !property_scene_plan_is_sealed(&reordered),
        "late deferred surface cannot move ahead of normal root paint"
    );

    let (mut arena, root, child, properties, generations) =
        native_nested_effect_fixture("image", "loading", 0.5, false, true);
    arena
        .get(child)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Image>()
        .unwrap()
        .set_resource_error_for_test();
    arena
        .with_element_taken(child, |element, arena| element.sync_arena(arena))
        .unwrap();
    assert!(
        plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .is_err()
    );

    let (mut arena, root, child, properties, generations) = build();
    arena.set_parent(child, None);
    assert!(
        plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .is_err()
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn nested_text_image_and_svg_effect_owners_seal_prepare_and_emit() {
    for (host, states) in [
        ("text", &["ready"][..]),
        ("image", &["ready", "loading", "error"][..]),
        ("svg", &["ready", "loading", "error"][..]),
    ] {
        for state in states {
            for opacity in [0.0_f32, 0.5] {
                let (arena, root, child, properties, generations) =
                    native_nested_effect_fixture(host, state, opacity, false, false);
                assert!(properties.validation_errors.is_empty(), "{host}/{state}");
                let effect = properties
                    .effects
                    .get(&EffectNodeId(child))
                    .expect("native child effect snapshot");
                assert_eq!(
                    effect.opacity.to_bits(),
                    opacity.to_bits(),
                    "{host}/{state}"
                );

                let plan = plan_property_effect_scene_with_context(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    TransformSurfacePlanContext::default(),
                )
                .unwrap_or_else(|error| {
                    panic!("{host}/{state}/{opacity} must remain retained: {error:?}")
                });
                assert!(property_scene_plan_is_sealed(&plan), "{host}/{state}");
                let witness = plan
                    .property_scene_transaction_witness()
                    .expect("native nested effect transaction witness");
                assert!(witness.surfaces.iter().any(|surface| {
                    surface.boundary_root == child
                        && matches!(
                            surface.kind,
                            PropertySceneTransactionSurfaceKind::Effect(_)
                        )
                }));

                let mut viewport = Viewport::new();
                let mut graph = FrameGraph::new();
                let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
                let prepared = super::super::super::prepare_retained_property_scene_from_pool(
                    &viewport, &plan, &graph, &ctx,
                )
                .unwrap_or_else(|error| {
                    panic!("{host}/{state}/{opacity} preflight failed: {error:?}")
                });
                let _ = super::super::super::emit_prepared_retained_property_scene(
                    &mut viewport,
                    prepared,
                    &mut graph,
                    ctx,
                );
                let compile = graph.test_compile_snapshot();
                assert!(compile.is_ok(), "{host}/{state}: {compile:?}");
                let composites = graph
                    .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
                assert_eq!(composites.len(), 1, "{host}/{state}/{opacity}");
                assert_eq!(
                    composites[0].test_snapshot().opacity_bits,
                    opacity.to_bits(),
                    "{host}/{state} opacity must be compiled exactly once"
                );
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn transform_parent_accepts_direct_native_effect_owner() {
    for host in ["text", "image", "svg"] {
        let (arena, root, child, _properties, _generations) =
            native_nested_effect_fixture(host, "ready", 0.5, false, false);
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::vec3(
                2.0, 1.0, 0.0,
            ))));
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);

        let plan = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap_or_else(|error| panic!("Transform -> {host} effect: {error:?}"));
        let root_surface = only_surface(&plan);
        assert!(root_surface.raster_steps.iter().any(|step| matches!(
            step,
            PaintPlanStep::RetainedSurface(surface)
                if surface.boundary_root == child
                    && matches!(surface.kind, SurfaceKind::NestedIsolation(_))
        )));

        let mut viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let outcome = super::super::super::build_retained_effect_tree_from_pool(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .unwrap_or_else(|error| panic!("Transform -> {host} emit: {error:?}"));
        assert_eq!(outcome.into_parts().1.len(), 2);
        let composites = graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
        assert_eq!(composites.len(), 1, "Transform -> {host}");
        assert_eq!(
            composites[0].test_snapshot().opacity_bits,
            0.5_f32.to_bits()
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_nested_effect_geometry_identity_resource_and_topology_drift_fail_closed() {
    let (arena, root, _child, properties, generations) =
        native_nested_effect_fixture("image", "ready", 0.5, false, false);
    let plan = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("canonical native nested effect");

    fn first_effect(plan: &mut FramePaintPlan) -> &mut RetainedSurfacePlan {
        plan.steps
            .iter_mut()
            .find_map(|step| match step {
                PaintPlanStep::RetainedSurface(surface)
                    if matches!(surface.kind, SurfaceKind::NestedIsolation(_)) =>
                {
                    Some(surface.as_mut())
                }
                PaintPlanStep::ArtifactSpan(_) | PaintPlanStep::RetainedSurface(_) => None,
            })
            .expect("native effect surface")
    }

    let mut geometry_drift = plan.clone();
    let SurfaceKind::NestedIsolation(effect) = &mut first_effect(&mut geometry_drift).kind
    else {
        panic!("nested isolation")
    };
    effect.geometry.source_bounds.width += 1.0;
    assert!(
        !property_scene_plan_is_sealed(&geometry_drift),
        "live geometry cannot diverge from the trait-sealed source bounds"
    );

    let mut identity_drift = plan.clone();
    first_effect(&mut identity_drift).stable_id ^= 1;
    assert!(
        !property_scene_plan_is_sealed(&identity_drift),
        "effect surface owner identity is part of the seal"
    );

    let mut generation_drift = plan.clone();
    let SurfaceKind::NestedIsolation(effect) = &mut first_effect(&mut generation_drift).kind
    else {
        panic!("nested isolation")
    };
    effect
        .property_scene
        .as_mut()
        .expect("effect property contract")
        .composite
        .effect_generation += 1;
    let viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
    assert!(
        super::super::super::prepare_retained_property_scene_from_pool(
            &viewport,
            &generation_drift,
            &graph,
            &ctx,
        )
        .is_err(),
        "effect generation drift must reject before graph mutation"
    );

    let (mut arena, root, child, properties, generations) =
        native_nested_effect_fixture("image", "loading", 0.5, false, false);
    arena
        .get(child)
        .expect("image child")
        .element
        .as_any()
        .downcast_ref::<Image>()
        .expect("Image host")
        .set_resource_error_for_test();
    arena
        .with_element_taken(child, |element, arena| element.sync_arena(arena))
        .expect("install changed resource snapshot");
    assert!(
        plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .is_err(),
        "stale loading/error resource identity must not reuse a retained seal"
    );

    let (mut arena, root, child, properties, generations) =
        native_nested_effect_fixture("svg", "loading", 0.5, false, false);
    arena.set_parent(child, None);
    let error = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("detached native effect owner must fail closed");
    assert!(
        error.reasons.iter().any(|reason| matches!(
            reason,
            FramePaintPlanRejection::TopologyMismatch(owner) if *owner == child
        )) || error
            .reasons
            .contains(&FramePaintPlanRejection::InvalidPropertyScene)
    );
}
