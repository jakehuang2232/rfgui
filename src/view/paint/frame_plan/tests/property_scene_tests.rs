use super::*;

#[test]
fn property_scene_plans_multi_root_three_level_and_sibling_transform_forest() {
    let fixture = general_property_scene_fixture();
    let plan = plan_transform_property_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("general planning-only transform scene");
    assert!(property_scene_plan_is_sealed(&plan));
    let seal = plan
        .property_scene_seal
        .as_ref()
        .expect("general scene seal");
    assert_eq!(seal.surface_count, 5);
    let id_for = |owner| {
        seal.surfaces
            .values()
            .find(|contract| contract.id.owner == owner)
            .expect("surface contract")
            .id
    };
    let outer = id_for(fixture.outer);
    let inner_a = id_for(fixture.inner_a);
    let deep = id_for(fixture.deep);
    let inner_b = id_for(fixture.inner_b);
    let second_root = id_for(fixture.second_root);
    assert_eq!(
        [
            outer.ordinal,
            inner_a.ordinal,
            deep.ordinal,
            inner_b.ordinal,
            second_root.ordinal,
        ],
        [0, 1, 2, 3, 4]
    );
    assert_eq!(seal.surfaces[&outer].parent, None);
    assert_eq!(seal.surfaces[&inner_a].parent, Some(outer));
    assert_eq!(seal.surfaces[&deep].parent, Some(inner_a));
    assert_eq!(seal.surfaces[&inner_b].parent, Some(outer));
    assert_eq!(seal.surfaces[&second_root].parent, None);

    let top_level = plan
        .steps()
        .iter()
        .filter_map(|step| match step {
            PaintPlanStep::RetainedSurface(surface) => Some(surface.boundary_root()),
            PaintPlanStep::ArtifactSpan(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(top_level, vec![fixture.outer, fixture.second_root]);
    let roots = plan.property_scene_roots.as_ref().unwrap();
    assert_eq!(
        roots[0].top_level_step_span.start,
        roots[0].top_level_step_span.end
    );
    assert_eq!(
        roots[3].top_level_step_span.start,
        roots[3].top_level_step_span.end
    );
}

#[test]
fn property_scene_seal_rejects_topology_identity_reference_and_witness_drift() {
    let fixture = general_property_scene_fixture();
    let base = plan_transform_property_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("sealed general property scene");

    let mut ordinal = base.clone();
    let seal = ordinal.property_scene_seal.as_mut().unwrap();
    let old_id = seal
        .surfaces
        .keys()
        .copied()
        .find(|id| id.owner == fixture.inner_a)
        .unwrap();
    let mut contract = seal.surfaces.remove(&old_id).unwrap();
    contract.id.ordinal += 7;
    seal.surfaces.insert(contract.id, contract);
    assert!(!property_scene_plan_is_sealed(&ordinal));

    let mut parent = base.clone();
    parent
        .property_scene_seal
        .as_mut()
        .unwrap()
        .surfaces
        .values_mut()
        .find(|contract| contract.id.owner == fixture.inner_a)
        .unwrap()
        .transform
        .parent = None;
    assert!(!property_scene_plan_is_sealed(&parent));

    let mut matrix = base.clone();
    matrix
        .property_scene_seal
        .as_mut()
        .unwrap()
        .surfaces
        .values_mut()
        .find(|contract| contract.id.owner == fixture.inner_a)
        .unwrap()
        .transform
        .viewport_matrix = glam::Mat4::from_translation(glam::Vec3::new(99.0, 1.0, 0.0));
    assert!(!property_scene_plan_is_sealed(&matrix));

    let mut generation = base.clone();
    generation
        .property_scene_seal
        .as_mut()
        .unwrap()
        .surfaces
        .values_mut()
        .find(|contract| contract.id.owner == fixture.inner_a)
        .unwrap()
        .transform
        .generation += 1;
    assert!(!property_scene_plan_is_sealed(&generation));

    let mut identity = base.clone();
    let outer_identity = {
        let outer = property_surface_mut(&mut identity.steps, fixture.outer).unwrap();
        (outer.stable_id, outer.persistent_color_key)
    };
    let inner = property_surface_mut(&mut identity.steps, fixture.inner_a).unwrap();
    inner.stable_id = outer_identity.0;
    inner.persistent_color_key = outer_identity.1;
    assert!(!property_scene_plan_is_sealed(&identity));

    let mut zero_stable = base.clone();
    property_surface_mut(&mut zero_stable.steps, fixture.inner_a)
        .unwrap()
        .stable_id = 0;
    assert!(!property_scene_plan_is_sealed(&zero_stable));

    let mut zero_contract_stable = base.clone();
    zero_contract_stable
        .property_scene_seal
        .as_mut()
        .unwrap()
        .surfaces
        .values_mut()
        .find(|contract| contract.id.owner == fixture.inner_a)
        .unwrap()
        .stable_id = 0;
    assert!(!property_scene_plan_is_sealed(&zero_contract_stable));

    let mut alternate_stable = base.clone();
    property_surface_mut(&mut alternate_stable.steps, fixture.inner_a)
        .unwrap()
        .stable_id = 0xf0_f0_f0;
    assert!(!property_scene_plan_is_sealed(&alternate_stable));

    let mut alternate_contract_stable = base.clone();
    alternate_contract_stable
        .property_scene_seal
        .as_mut()
        .unwrap()
        .surfaces
        .values_mut()
        .find(|contract| contract.id.owner == fixture.inner_a)
        .unwrap()
        .stable_id = 0xf0_f0_f0;
    assert!(!property_scene_plan_is_sealed(&alternate_contract_stable));

    let mut arbitrary_key = base.clone();
    property_surface_mut(&mut arbitrary_key.steps, fixture.inner_a)
        .unwrap()
        .persistent_color_key =
        crate::view::frame_graph::PersistentTextureKey::Generic(0xdead_beef);
    assert!(!property_scene_plan_is_sealed(&arbitrary_key));

    let mut missing = base.clone();
    missing.steps.retain(|step| {
        !matches!(step, PaintPlanStep::RetainedSurface(surface) if surface.boundary_root() == fixture.second_root)
    });
    assert!(!property_scene_plan_is_sealed(&missing));

    let mut duplicate = base.clone();
    let repeated = duplicate
        .steps
        .iter()
        .find(|step| {
            matches!(step, PaintPlanStep::RetainedSurface(surface) if surface.boundary_root() == fixture.second_root)
        })
        .cloned()
        .unwrap();
    duplicate.steps.push(repeated);
    assert!(!property_scene_plan_is_sealed(&duplicate));

    let mut artifact_witness = base.clone();
    artifact_witness
        .property_scene_seal
        .as_mut()
        .unwrap()
        .surfaces
        .values_mut()
        .find(|contract| !contract.artifact_validation.is_empty())
        .unwrap()
        .artifact_validation
        .pop();
    assert!(!property_scene_plan_is_sealed(&artifact_witness));

    let mut scissor = plan_transform_property_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0; 2], Some([3, 4, 80, 60])),
    )
    .expect("outer scissor is frozen by the scene seal");
    scissor
        .property_scene_seal
        .as_mut()
        .unwrap()
        .outer_scissor_rect = None;
    assert!(!property_scene_plan_is_sealed(&scissor));
}

#[test]
fn property_scene_preserves_ordered_root_spans_and_rejects_root_witness_drift() {
    let mut fixture = general_property_scene_fixture();
    let painted_root = |id: u64, x: f32, color| {
        let mut element = Element::new_with_id(id, x, 30.0, 9.0, 7.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    };
    let painted_a = commit_element(
        &mut fixture.arena,
        Box::new(painted_root(0xd1_1001, 180.0, Color::rgb(20, 60, 100))),
    );
    let painted_b = commit_element(
        &mut fixture.arena,
        Box::new(painted_root(0xd1_1002, 195.0, Color::rgb(100, 60, 20))),
    );
    let constraints = LayoutConstraints {
        max_width: 220.0,
        max_height: 140.0,
        viewport_width: 220.0,
        viewport_height: 140.0,
        percent_base_width: Some(220.0),
        percent_base_height: Some(140.0),
    };
    for (root, x) in [(painted_a, 180.0), (painted_b, 195.0)] {
        measure_and_place(
            &mut fixture.arena,
            root,
            constraints,
            LayoutPlacement {
                parent_x: x,
                parent_y: 30.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 140.0,
                viewport_width: 220.0,
                viewport_height: 140.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(140.0),
            },
        );
    }
    let transparent = fixture.roots[0];
    let trailing_transparent = fixture.roots[3];
    crate::view::test_support::get_element_mut::<Element>(&fixture.arena, transparent)
        .set_should_paint_for_test(false);
    crate::view::test_support::get_element_mut::<Element>(&fixture.arena, trailing_transparent)
        .set_should_paint_for_test(false);
    fixture.roots = vec![
        painted_a,
        painted_b,
        transparent,
        fixture.outer,
        fixture.second_root,
        trailing_transparent,
    ];
    fixture.properties = PropertyTrees::default();
    fixture.properties.sync(&fixture.arena, &fixture.roots);
    fixture.generations = PaintGenerationTracker::default();
    fixture
        .generations
        .sync(&fixture.arena, &fixture.roots, &fixture.properties);
    let base = plan_transform_property_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("mixed roots retain their exact input order");
    assert!(property_scene_plan_is_sealed(&base));
    let roots = base.property_scene_roots.as_ref().unwrap();
    assert_eq!(
        roots.iter().map(|root| root.root).collect::<Vec<_>>(),
        fixture.roots
    );
    assert!(roots[0].top_level_step_span.start < roots[0].top_level_step_span.end);
    assert!(roots[1].top_level_step_span.start < roots[1].top_level_step_span.end);
    assert_eq!(
        roots[0].top_level_step_span.end,
        roots[1].top_level_step_span.start
    );
    assert_eq!(
        roots[2].top_level_step_span.start, roots[2].top_level_step_span.end,
        "transparent root keeps an explicit empty insertion span"
    );
    assert!(roots[3].top_level_step_span.start < roots[3].top_level_step_span.end);
    assert!(roots[4].top_level_step_span.start < roots[4].top_level_step_span.end);
    assert_eq!(
        roots[5].top_level_step_span.start,
        roots[5].top_level_step_span.end
    );

    let duplicate_input = plan_transform_property_scene_with_context(
        &fixture.arena,
        &[painted_a, painted_a],
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("duplicate roots fail before materialization");
    assert!(
        duplicate_input
            .reasons
            .contains(&FramePaintPlanRejection::DuplicateRoot(painted_a))
    );

    let mut reordered_input = fixture.roots.clone();
    reordered_input.reverse();
    let reordered = plan_transform_property_scene_with_context(
        &fixture.arena,
        &reordered_input,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("a new input order gets a newly sealed exact order");
    assert_eq!(
        reordered
            .property_scene_roots
            .as_ref()
            .unwrap()
            .iter()
            .map(|root| root.root)
            .collect::<Vec<_>>(),
        reordered_input
    );

    let mut duplicate = base.clone();
    duplicate.property_scene_roots.as_mut().unwrap()[1].root = painted_a;
    assert!(!property_scene_plan_is_sealed(&duplicate));

    let mut reorder = base.clone();
    reorder.property_scene_roots.as_mut().unwrap().swap(0, 1);
    assert!(!property_scene_plan_is_sealed(&reorder));

    let mut stable = base.clone();
    stable.property_scene_roots.as_mut().unwrap()[0].stable_id += 1;
    assert!(!property_scene_plan_is_sealed(&stable));

    let mut owner = base.clone();
    owner.property_scene_roots.as_mut().unwrap()[0].owner.parent = Some(painted_b);
    assert!(!property_scene_plan_is_sealed(&owner));

    let mut span = base.clone();
    span.property_scene_roots.as_mut().unwrap()[0]
        .top_level_step_span
        .end += 1;
    assert!(!property_scene_plan_is_sealed(&span));
}

#[test]
fn property_scene_accepts_retained_baseline_baseline_and_rejects_effect_scroll_deferred_or_legacy_boundaries()
 {
    let fixture = general_property_scene_fixture();
    let baseline = plan_transform_property_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("retained-compatible retained property scene");
    assert!(property_scene_plan_is_sealed(&baseline));

    for property in ["effect", "scroll"] {
        let property_fixture = general_property_scene_fixture();
        let mut properties = property_fixture.properties;
        let state = properties
            .states
            .get_mut(&property_fixture.inner_a)
            .unwrap();
        match property {
            "effect" => state.paint.effect = Some(EffectNodeId(property_fixture.inner_a)),
            "scroll" => state.paint.scroll = Some(ScrollNodeId(property_fixture.inner_a)),
            _ => unreachable!(),
        }
        let error = plan_transform_property_scene_with_context(
            &property_fixture.arena,
            &property_fixture.roots,
            &properties,
            &property_fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect_err("unsupported property authority must fail closed");
        assert!(
            !error.reasons.is_empty()
                && error
                    .reasons
                    .iter()
                    .all(|reason| matches!(reason, FramePaintPlanRejection::Coverage(_))),
            "{property}: {:?}",
            error.reasons
        );
    }

    let deferred_fixture = general_property_scene_fixture();
    let mut deferred_style = Style::new();
    deferred_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            crate::style::Position::absolute()
                .left(crate::style::Length::px(0.0))
                .clip(crate::style::ClipMode::Viewport),
        ),
    );
    crate::view::test_support::get_element_mut::<Element>(
        &deferred_fixture.arena,
        deferred_fixture.inner_a,
    )
    .apply_style(deferred_style);
    let deferred = plan_transform_property_scene_with_context(
        &deferred_fixture.arena,
        &deferred_fixture.roots,
        &deferred_fixture.properties,
        &deferred_fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("deferred ordering cannot enter a retained property scene");
    assert!(
        deferred
            .reasons
            .contains(&FramePaintPlanRejection::DeferredBoundary(
                deferred_fixture.inner_a
            ))
    );

    let mut legacy_fixture = general_property_scene_fixture();
    let neutral_root = legacy_fixture.roots[0];
    commit_child(
        &mut legacy_fixture.arena,
        neutral_root,
        Box::new(Element::new_with_id(0xd1_00ff, 0.0, 0.0, 1.0, 1.0)),
    );
    measure_and_place(
        &mut legacy_fixture.arena,
        neutral_root,
        LayoutConstraints {
            max_width: 220.0,
            max_height: 140.0,
            viewport_width: 220.0,
            viewport_height: 140.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(140.0),
        },
        LayoutPlacement {
            parent_x: 130.0,
            parent_y: 10.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 220.0,
            available_height: 140.0,
            viewport_width: 220.0,
            viewport_height: 140.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(140.0),
        },
    );
    legacy_fixture.properties = PropertyTrees::default();
    legacy_fixture
        .properties
        .sync(&legacy_fixture.arena, &legacy_fixture.roots);
    legacy_fixture.generations = PaintGenerationTracker::default();
    legacy_fixture.generations.sync(
        &legacy_fixture.arena,
        &legacy_fixture.roots,
        &legacy_fixture.properties,
    );
    let mut rounded = Style::new();
    rounded.insert(
        PropertyId::BorderRadius,
        ParsedValue::Length(crate::style::Length::px(2.0)),
    );
    crate::view::test_support::get_element_mut::<Element>(&legacy_fixture.arena, neutral_root)
        .apply_style(rounded);
    let legacy = plan_transform_property_scene_with_context(
        &legacy_fixture.arena,
        &legacy_fixture.roots,
        &legacy_fixture.properties,
        &legacy_fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("unowned child clip remains legacy");
    assert!(
        legacy.reasons.iter().any(|reason| matches!(
            reason,
            FramePaintPlanRejection::Coverage(FrameArtifactFallbackReason::LegacyBoundary(_))
        )),
        "{:?}",
        legacy.reasons
    );
}

#[test]
fn property_scene_freezes_exact_local_and_ancestor_rect_clips() {
    let mut fixture = general_property_scene_fixture();
    let clip_id = ClipNodeId {
        owner: fixture.outer,
        role: ClipNodeRole::SelfClip,
    };
    fixture.properties.clips.insert(
        clip_id,
        crate::view::compositor::property_tree::ClipNode {
            owner: fixture.outer,
            parent: None,
            geometry: ClipGeometry::LogicalScissor([7, 9, 31, 19]),
            behavior: ClipBehavior::Replace,
            generation: 1,
        },
    );
    let subtree = fixture
        .properties
        .states
        .keys()
        .copied()
        .filter(|&key| {
            let mut cursor = Some(key);
            while let Some(owner) = cursor {
                if owner == fixture.outer {
                    return true;
                }
                cursor = fixture.arena.parent_of(owner);
            }
            false
        })
        .collect::<Vec<_>>();
    for key in subtree {
        let state = fixture.properties.states.get_mut(&key).unwrap();
        state.paint.clip = Some(clip_id);
        state.descendants.clip = Some(clip_id);
    }
    fixture.generations = PaintGenerationTracker::default();
    fixture
        .generations
        .sync(&fixture.arena, &fixture.roots, &fixture.properties);
    let base = plan_transform_property_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("exact logical rect clips are admitted");
    assert!(property_scene_plan_is_sealed(&base));
    let seal = base.property_scene_seal.as_ref().unwrap();
    let inner = seal
        .surfaces
        .values()
        .find(|contract| contract.id.owner == fixture.inner_a)
        .unwrap();
    assert_eq!(inner.ancestor_composite_clips.len(), 1);
    assert_eq!(inner.resolved_composite_scissor, Some([7, 9, 31, 19]));

    let mut ancestor = base.clone();
    ancestor
        .property_scene_seal
        .as_mut()
        .unwrap()
        .surfaces
        .values_mut()
        .find(|contract| contract.id.owner == fixture.inner_a)
        .unwrap()
        .ancestor_composite_clips[0]
        .logical_scissor = [8, 9, 31, 19];
    assert!(!property_scene_plan_is_sealed(&ancestor));

    let mut local = base.clone();
    let outer = property_surface_mut(&mut local.steps, fixture.outer).unwrap();
    let snapshot = outer
        .raster_steps
        .iter_mut()
        .find_map(|step| match step {
            PaintPlanStep::ArtifactSpan(span) => span.artifact.clip_nodes.first_mut(),
            PaintPlanStep::RetainedSurface(_) => None,
        })
        .expect("outer artifact owns the exact local clip");
    snapshot.logical_scissor[0] += 1;
    assert!(!property_scene_plan_is_sealed(&local));
}

#[test]
fn property_scene_executor_emits_arbitrary_depth_forest_and_stages_one_transaction() {
    let fixture = general_property_scene_fixture();
    let plan = plan_transform_property_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("sealed general property scene");
    let terminal = plan
        .property_scene_seal
        .as_ref()
        .unwrap()
        .aggregate_opaque_order_span
        .end;
    let mut graph = FrameGraph::new();
    let (ctx, _) = parent_context_with_clear(&mut graph, 220, 140, 1.0);
    let mut viewport = Viewport::new();
    let prepared =
        super::super::super::prepare_retained_property_scene_from_pool(&viewport, &plan, &graph, &ctx)
            .expect("multi-root arbitrary-depth property-scene preflight");
    let outcome = super::super::super::emit_prepared_retained_property_scene(
        &mut viewport,
        prepared,
        &mut graph,
        ctx,
    );
    let (state, trace) = outcome.into_parts();
    assert_eq!(trace.root_count, fixture.roots.len());
    assert_eq!(trace.surface_count, 5);
    assert_eq!(trace.reraster_count, 5);
    assert_eq!(trace.reuse_count, 0);
    assert_eq!(state.opaque_rect_order_for_test(), terminal);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        5,
        "every transform edge is applied exactly once during the initial raster"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(5))
    );
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (5, None)
    );

    let mut reuse_graph = FrameGraph::new();
    let (reuse_ctx, _) = parent_context_with_clear(&mut reuse_graph, 220, 140, 1.0);
    let reuse = super::super::super::build_retained_property_scene_with_forced_pool_for_test(
        &mut viewport,
        &plan,
        &mut reuse_graph,
        reuse_ctx,
    )
    .expect("identical scene reuses every compatible resident pair");
    let (_, reuse_trace) = reuse.into_parts();
    assert_eq!(reuse_trace.reraster_count, 0);
    assert_eq!(reuse_trace.reuse_count, 5);
    assert_eq!(
        reuse_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        2,
        "a reused forest composites only its two already-rasterized top-level surfaces"
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn property_scene_context_mismatch_rejects_before_graph_or_pool_mutation() {
    let fixture = general_property_scene_fixture();
    let plan = plan_transform_property_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::default(),
    )
    .unwrap();
    let graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(220, 140, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.push_scissor_rect(Some([1, 2, 30, 40]));
    let graph_before = graph.build_state_snapshot_for_test();
    let viewport = Viewport::new();
    let transaction_before = viewport.retained_surface_transaction_shape_for_test();
    let error = match super::super::super::prepare_retained_property_scene_from_pool(
        &viewport, &plan, &graph, &ctx,
    ) {
        Ok(_) => panic!("mismatched live context cannot prepare the frozen scene"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        super::super::super::ForcedTransformSurfaceError::ContextMismatch
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        transaction_before
    );
}
