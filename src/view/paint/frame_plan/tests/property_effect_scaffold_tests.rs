use super::*;

#[test]
fn property_effect_scaffold_rejects_unproven_multi_boundary_interleave() {
    let (arena, root, child, _, mut properties, mut generations) =
        planning_only_nested_effect_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 0.0, 0.0,
        ))));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let error = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect_err("unproven effect/transform interleave must fail closed");
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
    )));
}

#[test]
fn property_effect_scaffold_admits_only_proven_transform_direct_effect_mapping() {
    let (arena, root, _, child, _, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    let plan = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("existing Transform -> direct Effect coordinate contract is proven");
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .and_then(|seal| seal.effect_scaffold.as_ref())
        .expect("effect scaffold");
    assert_eq!(scaffold.surfaces.len(), 2);
    let PropertyEffectSurfaceKind::Transform {
        nested_effect_dependencies,
        ..
    } = &scaffold.surfaces[0].kind
    else {
        panic!("transform parent")
    };
    assert_eq!(nested_effect_dependencies.len(), 1);
    assert_eq!(
        nested_effect_dependencies[0].child_effect,
        EffectNodeId(child)
    );
    assert_eq!(
        nested_effect_dependencies[0].child_opacity_bits,
        0.5_f32.to_bits()
    );
    let PropertyEffectSurfaceKind::Isolation(isolation) = &scaffold.surfaces[1].kind else {
        panic!("effect child")
    };
    assert!(matches!(
        isolation.composite.basis,
        PropertyIsolationCompositeBasis::ParentTransform { transform, .. }
            if transform == TransformNodeId(root)
    ));
    assert_eq!(isolation.parent_opaque_cursor_delta, 0);
}

#[test]
fn property_effect_scaffold_rejects_mixed_wrapper_multiroot_and_non_affine_shapes() {
    let (mut arena, root, _, _, _, _, mut properties, mut generations) =
        exact_transform_child_isolation_fixture();
    let wrapper = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xea_3001, 0.0, 0.0, 160.0, 120.0)),
    );
    arena.set_parent(root, Some(wrapper));
    arena.set_children(wrapper, vec![root]);
    properties.sync(&arena, &[wrapper]);
    generations.sync(&arena, &[wrapper], &properties);
    let error = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[wrapper],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect_err("non-root transform wrapper is outside proven mixed shape");
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
    )));

    let (mut arena, root, _, _, _, _, mut properties, mut generations) =
        exact_transform_child_isolation_fixture();
    let side_root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xea_3002, 140.0, 0.0, 8.0, 8.0)),
    );
    let roots = [root, side_root];
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    let error = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect_err("mixed property scaffold is single-root only");
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
    )));

    let (arena, root, _, _, _, _, mut properties, generations) =
        exact_transform_child_isolation_fixture();
    properties
        .transforms
        .get_mut(&TransformNodeId(root))
        .expect("transform")
        .viewport_matrix = glam::Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.25, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    let error = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect_err("finite perspective matrix is not the proven affine contract");
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
    )));
}

#[test]
fn property_effect_scaffold_places_local_and_ancestor_clips_exactly() {
    let (arena, root, _child, _, mut properties, mut generations) =
        planning_only_nested_effect_fixture();
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::SelfClip,
    };
    properties.clips.insert(
        clip_id,
        crate::view::compositor::property_tree::ClipNode {
            owner: root,
            parent: None,
            geometry: ClipGeometry::LogicalScissor([2, 3, 20, 18]),
            behavior: ClipBehavior::Replace,
            generation: 41,
        },
    );
    let state_keys = properties.states.keys().copied().collect::<Vec<_>>();
    for key in state_keys {
        let state = properties.states.get_mut(&key).expect("state");
        state.paint.clip = Some(clip_id);
        state.descendants.clip = Some(clip_id);
    }
    generations.sync(&arena, &[root], &properties);
    let plan = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("exact clip placement scaffold");
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .and_then(|seal| seal.effect_scaffold.as_ref())
        .expect("effect scaffold");
    let PropertyEffectSurfaceKind::Isolation(root_surface) = &scaffold.surfaces[0].kind else {
        panic!("root isolation")
    };
    let PropertyEffectSurfaceKind::Isolation(child_surface) = &scaffold.surfaces[1].kind else {
        panic!("child isolation")
    };
    assert_eq!(root_surface.local_raster_clips[0].id, clip_id);
    assert!(root_surface.ancestor_composite_clips.is_empty());
    assert!(child_surface.local_raster_clips.is_empty());
    assert_eq!(child_surface.ancestor_composite_clips[0].id, clip_id);
    assert_eq!(
        child_surface.composite.resolved_scissor,
        Some([2, 3, 20, 18])
    );

    let production = plan_property_effect_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("exact clip split must materialize");
    let PaintPlanStep::RetainedSurface(production_root) = &production.steps[0] else {
        panic!("root effect surface")
    };
    let root_local_clip = production_root
        .raster_steps
        .iter()
        .find_map(|step| match step {
            PaintPlanStep::ArtifactSpan(span) => span.artifact.clip_nodes.first(),
            PaintPlanStep::RetainedSurface(_) => None,
        })
        .expect("root raster keeps its local clip");
    assert_eq!(root_local_clip.id, clip_id);
    assert_eq!(root_local_clip.parent, None);
    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &production,
        &mut graph,
        ctx,
    )
    .expect("clip-bearing effect scene preflight and emit");
    let composite_scissors = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
        .into_iter()
        .map(|pass| pass.test_snapshot().effective_scissor_rect)
        .collect::<Vec<_>>();
    assert_eq!(
        composite_scissors,
        vec![Some([2, 3, 20, 18]), Some([2, 3, 20, 18]), None],
        "inherited clip belongs on descendant composite edges, not root raster composite"
    );

    let mut clip_drift = plan.clone();
    let scaffold = clip_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("effect scaffold");
    let PropertyEffectSurfaceKind::Isolation(root_surface) = &mut scaffold.surfaces[0].kind
    else {
        panic!("root isolation")
    };
    root_surface.local_raster_clips[0].generation += 1;
    assert!(!property_scene_plan_is_sealed(&clip_drift));

    let mut inherited_clip_erasure = plan.clone();
    let scaffold = inherited_clip_erasure
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("effect scaffold");
    for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
        let PropertyEffectSurfaceKind::Isolation(child_surface) = &mut surfaces[1].kind else {
            panic!("child isolation")
        };
        child_surface.ancestor_composite_clips.clear();
        child_surface.composite.resolved_scissor = None;
    }
    assert!(!property_scene_plan_is_sealed(&inherited_clip_erasure));

    let mut local_parent_drift = plan.clone();
    let scaffold = local_parent_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("effect scaffold");
    for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
        let PropertyEffectSurfaceKind::Isolation(root_surface) = &mut surfaces[0].kind else {
            panic!("root isolation")
        };
        root_surface.local_raster_clips[0].parent = Some(clip_id);
        root_surface.raster_identity.local_raster_clips[0].parent = Some(clip_id);
    }
    assert!(!property_scene_plan_is_sealed(&local_parent_drift));

    let mut forest_terminal_drift = plan.clone();
    let scaffold = forest_terminal_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("effect scaffold");
    for forest in [&mut scaffold.clip_forest, &mut scaffold.planned_clip_forest] {
        forest.nodes[0].parent = Some(forest.nodes[0].id);
    }
    assert!(!property_scene_plan_is_sealed(&forest_terminal_drift));

    let mut role_behavior_drift = plan.clone();
    let scaffold = role_behavior_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("effect scaffold");
    for forest in [&mut scaffold.clip_forest, &mut scaffold.planned_clip_forest] {
        forest.nodes[0].behavior = ClipBehavior::Intersect;
    }
    for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
        let PropertyEffectSurfaceKind::Isolation(root_surface) = &mut surfaces[0].kind else {
            panic!("root isolation")
        };
        root_surface.local_raster_clips[0].behavior = ClipBehavior::Intersect;
        root_surface.raster_identity.local_raster_clips[0].behavior = ClipBehavior::Intersect;
    }
    assert!(!property_scene_plan_is_sealed(&role_behavior_drift));
}

#[test]
fn property_effect_scaffold_rejects_clip_role_behavior_mismatch_at_admission() {
    for (role, behavior) in [
        (ClipNodeRole::SelfClip, ClipBehavior::Intersect),
        (ClipNodeRole::ContentsClip, ClipBehavior::Replace),
    ] {
        let (arena, root, _, _, mut properties, generations) =
            planning_only_nested_effect_fixture();
        let clip_id = ClipNodeId { owner: root, role };
        properties.clips.insert(
            clip_id,
            crate::view::compositor::property_tree::ClipNode {
                owner: root,
                parent: None,
                geometry: ClipGeometry::LogicalScissor([2, 3, 20, 18]),
                behavior,
                generation: 41,
            },
        );
        let state_keys = properties.states.keys().copied().collect::<Vec<_>>();
        for key in state_keys {
            let state = properties.states.get_mut(&key).expect("state");
            if key == root && role == ClipNodeRole::ContentsClip {
                state.paint.clip = None;
            } else {
                state.paint.clip = Some(clip_id);
            }
            state.descendants.clip = Some(clip_id);
        }
        let error = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect_err("clip role and behavior pairing must be exact");
        assert!(
            error
                .reasons
                .contains(&FramePaintPlanRejection::InvalidClipChain(root))
        );
    }
}

#[test]
fn property_effect_scaffold_rejects_stale_live_generation_fingerprint() {
    let (arena, root, _, _, properties, generations) = planning_only_nested_effect_fixture();
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(210, 30, 70)),
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, root).apply_style(style);
    assert!(!generations.matches_live_snapshot(&arena, &[root], &properties));
    let error = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect_err("stale generations cannot mint an artifact-input fingerprint");
    assert_eq!(
        error.reasons,
        vec![FramePaintPlanRejection::InvalidPropertyScene]
    );
}

#[test]
fn property_effect_scaffold_seal_rejects_effect_clip_root_and_dependency_drift() {
    let (arena, root, _, _, properties, generations) = planning_only_nested_effect_fixture();
    let build = || {
        plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("sealed scaffold")
    };
    let mut effect_drift = build();
    let scaffold = effect_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("scaffold");
    let PropertyEffectSurfaceKind::Isolation(effect) = &mut scaffold.surfaces[1].kind else {
        panic!("effect")
    };
    effect.composite.effect_generation += 1;
    assert!(!property_scene_plan_is_sealed(&effect_drift));

    let mut reparent_drift = build();
    let scaffold = reparent_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("scaffold");
    let PropertyEffectSurfaceKind::Isolation(effect) = &mut scaffold.surfaces[1].kind else {
        panic!("effect")
    };
    effect.effect_chain.live_leaf_to_root[0].parent = None;
    assert!(!property_scene_plan_is_sealed(&reparent_drift));

    let mut ancestor_snapshot_drift = build();
    let scaffold = ancestor_snapshot_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("scaffold");
    for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
        let PropertyEffectSurfaceKind::Isolation(effect) = &mut surfaces[2].kind else {
            panic!("grandchild effect")
        };
        effect.effect_chain.live_leaf_to_root[2].opacity = 0.625;
        effect.effect_chain.live_leaf_to_root[2].generation += 1;
        effect.effect_chain.detached_ancestors[1].opacity = 0.625;
        effect.effect_chain.detached_ancestors[1].generation += 1;
    }
    assert!(!property_scene_plan_is_sealed(&ancestor_snapshot_drift));

    let mut child_content_drift = build();
    let scaffold = child_content_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("scaffold");
    for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
        let PropertyEffectSurfaceKind::Isolation(effect) = &mut surfaces[1].kind else {
            panic!("child effect")
        };
        effect.raster_identity.content[0].self_paint_revision += 1;
    }
    assert!(!property_scene_plan_is_sealed(&child_content_drift));

    let mut root_content_topology_drift = build();
    let scaffold = root_content_topology_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("scaffold");
    for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
        let PropertyEffectSurfaceKind::Isolation(effect) = &mut surfaces[0].kind else {
            panic!("root effect")
        };
        assert!(effect.raster_identity.content.len() > 1);
        effect.raster_identity.content[1].parent = None;
    }
    assert!(!property_scene_plan_is_sealed(&root_content_topology_drift));

    let mut dependency_drift = build();
    let scaffold = dependency_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("scaffold");
    let PropertyEffectSurfaceKind::Isolation(effect) = &mut scaffold.surfaces[0].kind else {
        panic!("effect")
    };
    effect.nested_dependencies[0].child_opacity_bits ^= 1;
    assert!(!property_scene_plan_is_sealed(&dependency_drift));

    let mut root_drift = build();
    root_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("scaffold")
        .roots[0]
        .boundary_ordinal_span = 1..3;
    assert!(!property_scene_plan_is_sealed(&root_drift));

    let mut context_drift = build();
    context_drift
        .property_scene_seal
        .as_mut()
        .expect("seal")
        .context = TransformSurfacePlanContext::new([9.0, 0.0], None);
    assert!(!property_scene_plan_is_sealed(&context_drift));

    let mut outer_scissor_drift = build();
    outer_scissor_drift
        .property_scene_seal
        .as_mut()
        .expect("seal")
        .outer_scissor_rect = Some([1, 2, 3, 4]);
    assert!(!property_scene_plan_is_sealed(&outer_scissor_drift));

    let mut resolved_scissor_drift = build();
    let scaffold = resolved_scissor_drift
        .property_scene_seal
        .as_mut()
        .and_then(|seal| seal.effect_scaffold.as_mut())
        .expect("scaffold");
    let PropertyEffectSurfaceKind::Isolation(effect) = &mut scaffold.surfaces[0].kind else {
        panic!("effect")
    };
    effect.composite.resolved_scissor = Some([1, 2, 3, 4]);
    assert!(!property_scene_plan_is_sealed(&resolved_scissor_drift));
}

#[test]
fn property_effect_scaffold_preserves_mixed_root_and_boundary_dfs_order() {
    let (mut arena, first_root, _, _, _, _) = planning_only_nested_effect_fixture();
    let mut second = Element::new_with_id(0xea_2001, 120.0, 4.0, 12.0, 9.0);
    second.set_opacity(0.25);
    let second_root = commit_element(&mut arena, Box::new(second));
    let neutral_root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xea_2002, 138.0, 4.0, 8.0, 8.0)),
    );
    let constraints = LayoutConstraints {
        max_width: 160.0,
        max_height: 120.0,
        viewport_width: 160.0,
        viewport_height: 120.0,
        percent_base_width: Some(160.0),
        percent_base_height: Some(120.0),
    };
    for (root, x) in [(second_root, 120.0), (neutral_root, 138.0)] {
        measure_and_place(
            &mut arena,
            root,
            constraints,
            LayoutPlacement {
                parent_x: x,
                parent_y: 4.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
        );
    }
    let roots = [second_root, neutral_root, first_root];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let plan = plan_property_effect_scene_scaffold_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("multi-root effect scaffold");
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .and_then(|seal| seal.effect_scaffold.as_ref())
        .expect("effect scaffold");
    assert_eq!(
        scaffold
            .roots
            .iter()
            .map(|root| root.root)
            .collect::<Vec<_>>(),
        roots
    );
    assert_eq!(scaffold.roots[0].boundary_ordinal_span, 0..1);
    assert_eq!(scaffold.roots[1].boundary_ordinal_span, 1..1);
    assert_eq!(scaffold.roots[2].boundary_ordinal_span, 1..4);
    assert_eq!(scaffold.surfaces[0].boundary.owner(), second_root);
    assert_eq!(scaffold.surfaces[1].boundary.owner(), first_root);

    let production = plan_property_effect_scene_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("multi-root effect forest must materialize");
    let witness = production
        .property_scene_transaction_witness()
        .expect("multi-root effect transaction");
    assert_eq!(witness.roots.len(), 3);
    assert_eq!(witness.surfaces.len(), 4);
    assert!(witness.roots[1].top_level_step_span.is_empty());
    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    let outcome = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &production,
        &mut graph,
        ctx,
    )
    .expect("multi-root effect forest preflight and emit");
    let (_, trace) = outcome.into_parts();
    assert_eq!(trace.root_count, 3);
    assert_eq!(trace.surface_count, 4);
}
