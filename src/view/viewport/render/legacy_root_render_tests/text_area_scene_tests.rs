use super::*;

#[test]
fn retained_auto_scroll_text_area_subtree_selects_typed_property_scene() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_scroll_text_area_scene();
    assert_eq!(properties.scrolls.len(), 1);
    assert_eq!(properties.clips.len(), 2);
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let wrapper_node = arena.get(wrapper).unwrap();
    let wrapper_element = wrapper_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    assert_eq!(
        wrapper_element.children(),
        &[text_area],
        "fixture wrapper component children must mirror the arena"
    );
    let wrapper_offset = wrapper_element
        .exact_retained_scroll_content_wrapper_recording_offset([0.0, 20.0])
        .expect("fixture wrapper must satisfy the sibling oracle");
    let text_area_node = arena.get(text_area).unwrap();
    let text_area_element = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .unwrap();
    assert!(
        text_area_element.exact_retained_property_scroll_glyph_subtree(
            text_area,
            &arena,
            wrapper_offset,
        ),
        "fixture TextArea must satisfy the glyph-only oracle at {wrapper_offset:?}"
    );
    let root_node = arena.get(roots[0]).unwrap();
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    assert!(
        root_element
            .exact_retained_scroll_text_area_subtree_admission(roots[0], &arena, 1.0)
            .is_some(),
        "fixture must satisfy the typed component admission"
    );
    let outer_clip = crate::view::compositor::property_tree::ClipNodeId {
        owner: roots[0],
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let outer_scroll = crate::view::compositor::property_tree::ScrollNodeId(roots[0]);
    let text_clip = crate::view::compositor::property_tree::ClipNodeId {
        owner: text_area,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(outer_clip),
        scroll: Some(outer_scroll),
        ..Default::default()
    };
    let text_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(text_clip),
        scroll: Some(outer_scroll),
        ..Default::default()
    };
    let root_state = properties.node_state_for(roots[0]).unwrap();
    assert_eq!(root_state.paint, Default::default());
    assert_eq!(root_state.descendants, outer_state);
    let wrapper_state = properties.node_state_for(wrapper).unwrap();
    assert_eq!(wrapper_state.paint, outer_state);
    assert_eq!(wrapper_state.descendants, outer_state);
    let text_area_state = properties.node_state_for(text_area).unwrap();
    assert_eq!(text_area_state.paint, outer_state);
    assert_eq!(text_area_state.descendants, text_state);
    for child in arena.children_of(text_area) {
        let child_state = properties.node_state_for(child).unwrap();
        assert_eq!(child_state.paint, text_state);
        assert_eq!(child_state.descendants, text_state);
    }

    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let (scene, trace) = match decision {
        AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (scene, trace),
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "exact S->C->TextArea glyph subtree rejected: {:?}; root={:?} wrapper={:?} text={:?} children={:?}",
            trace.rejections,
            roots[0],
            wrapper,
            text_area,
            arena.children_of(text_area),
        ),
        _ => panic!("exact S->C->TextArea glyph subtree selected wrong authority"),
    };
    assert!(matches!(
        trace.rejections.as_slice(),
        [AutoAuthorityRejection::PropertyScrollPlan { .. }]
    ));
    assert_eq!(scene.boundary_count(), 1);
    assert!(scene.is_canonical());

    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    )
    .expect("typed TextArea scene must prepare as one atomic forest");
    let stamps = prepared.scroll_content_stamps_for_test();
    let [stamp] = stamps.as_slice() else {
        panic!("C1 prepare must seal exactly one resident stamp")
    };
    let [local_clip] = stamp.clip_nodes.as_slice() else {
        panic!("C1 resident must retain exactly one local TextArea clip")
    };
    assert_eq!(local_clip.owner, text_area);
    assert!(local_clip.parent.is_none());
    assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(stamp));

    let mut parent_tamper = stamp.clone();
    parent_tamper.clip_nodes[0].parent = Some(outer_clip);
    assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&parent_tamper));
    let mut owner_tamper = stamp.clone();
    owner_tamper.clip_nodes[0].owner = roots[0];
    assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&owner_tamper));
    let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
    let (_state, trace) = outcome.into_parts();
    assert_eq!(trace.root_count, 1);
    assert_eq!(trace.scroll_group_count, 1);
    assert_eq!(
        trace.backing,
        crate::view::paint::ScrollSceneBackingKind::Single
    );
    assert_eq!(trace.tile_count, 1);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
}

#[test]
fn retained_auto_focused_atomic_projection_text_area_selects_property_scene() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) =
        prepared_focused_atomic_projection_scroll_text_area_scene();
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let root_node = arena.get(roots[0]).unwrap();
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    assert!(
        root_element
            .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                roots[0], &arena, 1.0,
            )
            .is_some(),
        "focused projection fixture must satisfy C3b admission",
    );

    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let (scene, trace) = match decision {
        AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (scene, trace),
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "focused atomic projection TextArea rejected PropertyScrollScene: {:?}",
            trace.rejections
        ),
        _ => panic!("focused atomic projection TextArea selected wrong authority"),
    };
    assert!(matches!(
        trace.rejections.as_slice(),
        [AutoAuthorityRejection::PropertyScrollPlan { .. }]
    ));
    assert_eq!(scene.boundary_count(), 1);
    assert!(scene.is_canonical());

    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    )
    .expect("focused atomic projection scene must prepare through native RetainedAuto path");
    assert_eq!(
        prepared.graph_build_state_snapshot_for_test(),
        graph_before,
        "prepare must remain graph-inert until emit",
    );
    let stamps = prepared.scroll_content_stamps_for_test();
    let [stamp] = stamps.as_slice() else {
        panic!("focused C3b prepare must seal exactly one resident stamp")
    };
    let [local_clip] = stamp.clip_nodes.as_slice() else {
        panic!("focused C3b resident must retain exactly one local TextArea clip")
    };
    assert_eq!(local_clip.owner, text_area);
    assert!(local_clip.parent.is_none());
    assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(stamp));

    let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
    let (state, trace) = outcome.into_parts();
    assert_eq!(state.opaque_rect_order(), 1);
    assert_eq!(
        trace.backing,
        crate::view::paint::ScrollSceneBackingKind::Single
    );
    assert_eq!(trace.tile_count, 1);
    assert_eq!(trace.reraster_count, 1);
    let pass_names = graph
        .pass_descriptors()
        .iter()
        .map(|pass| pass.name)
        .collect::<Vec<_>>();
    let composite = pass_names
        .iter()
        .position(|name| name.ends_with("TextureCompositePass"))
        .expect("resident atomic projection content must composite");
    let caret = pass_names
        .iter()
        .position(|name| name.ends_with("OpaqueRectPass"))
        .expect("visible focused atomic caret must emit dynamically");
    assert!(composite < caret, "caret must follow resident composite");
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
}

#[test]
fn retained_auto_focused_atomic_projection_preedit_selects_property_scene() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) =
        prepared_focused_atomic_projection_scroll_text_area_scene_with_preedit(Some((
            "中",
            Some((0, "中".len())),
        )));
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let root_node = arena.get(roots[0]).unwrap();
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    assert!(
        root_element
            .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                roots[0], &arena, 1.0,
            )
            .is_some(),
        "focused projection preedit fixture must satisfy C3b admission",
    );

    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let (scene, trace) = match decision {
        AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (scene, trace),
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "focused atomic projection preedit rejected PropertyScrollScene: {:?}",
            trace.rejections
        ),
        _ => panic!("focused atomic projection preedit selected wrong authority"),
    };
    assert!(matches!(
        trace.rejections.as_slice(),
        [AutoAuthorityRejection::PropertyScrollPlan { .. }]
    ));
    assert_eq!(scene.boundary_count(), 1);
    assert!(scene.is_canonical());

    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    )
    .expect(
        "focused atomic projection preedit scene must prepare through native RetainedAuto path",
    );
    let stamps = prepared.scroll_content_stamps_for_test();
    let [stamp] = stamps.as_slice() else {
        panic!("focused projection preedit prepare must seal exactly one resident stamp")
    };
    let [local_clip] = stamp.clip_nodes.as_slice() else {
        panic!(
            "focused projection preedit resident must retain exactly one local TextArea clip"
        )
    };
    assert_eq!(local_clip.owner, text_area);
    assert!(local_clip.parent.is_none());
    assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(stamp));

    let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
    let (state, trace) = outcome.into_parts();
    assert_eq!(state.opaque_rect_order(), 2);
    assert_eq!(
        trace.backing,
        crate::view::paint::ScrollSceneBackingKind::Single
    );
    assert_eq!(trace.tile_count, 1);
    assert_eq!(trace.reraster_count, 1);
    let pass_names = graph
        .pass_descriptors()
        .iter()
        .map(|pass| pass.name)
        .collect::<Vec<_>>();
    let composite = pass_names
        .iter()
        .position(|name| name.ends_with("TextureCompositePass"))
        .expect("resident atomic projection content must composite");
    let post_rects = pass_names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| name.ends_with("OpaqueRectPass").then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(
        post_rects.len(),
        2,
        "preedit underline and caret must be post-composite sidecars"
    );
    assert!(
        post_rects.into_iter().all(|index| composite < index),
        "preedit sidecars must follow resident composite",
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
}

#[test]
fn rejected_frame_root_scroll_candidate_is_observationally_pure_for_text_area_scene() {
    let (arena, roots, properties, generations) = prepared_scroll_text_area_scene();
    let budget = crate::view::paint::ScrollSceneSingleTextureBudget::new(
        wgpu::Limits::default().max_texture_dimension_2d,
        128 * 1024 * 1024,
    )
    .unwrap();
    let sampled_at = crate::time::Instant::now();
    let plan_property = || {
        crate::view::paint::plan_and_validate_property_scroll_scene(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8Unorm,
            budget,
        )
    };
    assert!(plan_property().is_ok(), "baseline PropertyScroll candidate");
    for _ in 0..2 {
        assert_eq!(
            crate::view::paint::plan_and_validate_frame_root_scroll_scene(
                &arena,
                &roots,
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
                wgpu::TextureFormat::Bgra8Unorm,
            )
            .err(),
            Some(crate::view::paint::PropertyScrollScenePlanError::InvalidContract),
        );
    }
    assert!(
        plan_property().is_ok(),
        "repeated rejected candidates must preserve later candidate observation"
    );
}

#[test]
fn retained_auto_scroll_text_area_normalized_identity_reuses_outer_scroll_only() {
    let select = |stage: &str,
                  arena: &NodeArena,
                  roots: &[NodeKey],
                  properties: &PropertyTrees,
                  generations: &PaintGenerationTracker| {
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        match select_retained_auto_authority(arena, roots, properties, generations, &ctx, true)
        {
            AutoAuthorityDecision::PropertyScrollScene { scene, .. } => scene,
            AutoAuthorityDecision::Legacy { trace } => panic!(
                "normalized C1 {stage} fixture must select PropertyScene: {:?}",
                trace.rejections,
            ),
            _ => panic!("normalized C1 fixture selected the wrong retained authority"),
        }
    };
    let prepare_emit =
        |viewport: &mut Viewport, scene: crate::view::paint::ValidatedPropertyScrollScene| {
            let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut graph = FrameGraph::new();
            let mut prepared =
                crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
                    viewport,
                    scene,
                    &mut graph,
                    UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                    [0.0, 0.0, 0.0, 1.0],
                    frame_owner,
                )
                .unwrap();
            prepared.refresh_actions_from_committed_test_pool();
            let stamps = prepared.scroll_content_stamps_for_test();
            let [stamp] = stamps.as_slice() else {
                panic!("single C1 boundary must prepare one stamp")
            };
            let stamp = stamp.clone();
            let outcome =
                crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
            let (_state, trace) = outcome.into_parts();
            assert!(
                viewport
                    .finish_retained_surface_transaction_for_frame(Some(frame_owner), true,)
            );
            (stamp, trace)
        };

    let content =
        "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode";
    let (mut arena, roots, mut properties, mut generations) =
        prepared_scroll_text_area_scene_with(20.0, 9.0, content);
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let text_area_clip = crate::view::compositor::property_tree::ClipNodeId {
        owner: text_area,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let baseline_live_clip_generation = properties
        .clip_snapshot_for(Some(text_area_clip))
        .expect("baseline TextArea live clip chain")[0]
        .generation;
    let baseline_raw_self_paint_revision = generations
        .snapshot(text_area)
        .expect("baseline TextArea paint generation")
        .self_paint_revision;
    let mut viewport = Viewport::new();
    let (baseline_stamp, baseline) = prepare_emit(
        &mut viewport,
        select("baseline", &arena, &roots, &properties, &generations),
    );
    assert_eq!((baseline.reraster_count, baseline.reuse_count), (1, 0));

    update_prepared_scroll_text_area_scene(
        &mut arena,
        &roots,
        &mut properties,
        &mut generations,
        30.0,
        9.0,
    );
    let outer_live_clip_generation = properties
        .clip_snapshot_for(Some(text_area_clip))
        .expect("outer-scroll TextArea live clip chain")[0]
        .generation;
    let outer_raw_self_paint_revision = generations
        .snapshot(text_area)
        .expect("outer-scroll TextArea paint generation")
        .self_paint_revision;
    assert_ne!(
        outer_live_clip_generation, baseline_live_clip_generation,
        "same-arena outer scroll must advance the raw live TextArea clip generation"
    );
    assert_ne!(
        outer_raw_self_paint_revision, baseline_raw_self_paint_revision,
        "same-arena outer scroll must advance the raw TextArea self-paint revision"
    );
    let (outer_scroll_stamp, outer_scroll) = prepare_emit(
        &mut viewport,
        select("outer-scroll", &arena, &roots, &properties, &generations),
    );
    assert!(
        outer_scroll_stamp == baseline_stamp,
        "outer-scroll-only motion must preserve the normalized detached-content stamp"
    );
    assert_eq!(
        (outer_scroll.reraster_count, outer_scroll.reuse_count),
        (0, 1)
    );

    update_prepared_scroll_text_area_scene(
        &mut arena,
        &roots,
        &mut properties,
        &mut generations,
        30.0,
        10.0,
    );
    let (local_scroll_stamp, local_scroll) = prepare_emit(
        &mut viewport,
        select("local-scroll", &arena, &roots, &properties, &generations),
    );
    assert!(
        local_scroll_stamp != outer_scroll_stamp,
        "local TextArea scroll must change the detached-content stamp"
    );
    assert_eq!(
        (local_scroll.reraster_count, local_scroll.reuse_count),
        (1, 0)
    );

    let (content_arena, content_roots, content_properties, content_generations) =
        prepared_scroll_text_area_scene_with(
            30.0,
            10.0,
            "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode; changed payload must reraster while preserving the same generated owner",
        );
    let (content_stamp, changed_content) = prepare_emit(
        &mut viewport,
        select(
            "content",
            &content_arena,
            &content_roots,
            &content_properties,
            &content_generations,
        ),
    );
    assert!(
        content_stamp != local_scroll_stamp,
        "TextArea payload changes must change the detached-content stamp"
    );
    assert_eq!(
        (changed_content.reraster_count, changed_content.reuse_count),
        (1, 0)
    );
}

#[test]
fn retained_auto_scroll_text_area_selection_noncanonical_states_fail_closed() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let viewport = Viewport::new();
    let pool_before = viewport.compositor.retained_surfaces.clone();
    for (selection, label) in [
        ((Some(3), Some(3)), "collapsed"),
        ((Some(3), None), "missing focus"),
        ((None, Some(3)), "missing anchor"),
        ((Some(0), Some(usize::MAX)), "out-of-range endpoint"),
    ] {
        let (arena, roots, mut properties, mut generations) = prepared_scroll_text_area_scene();
        update_prepared_scroll_text_area_selection(
            &arena,
            &roots,
            &mut properties,
            &mut generations,
            selection,
            None,
        );
        let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        ) else {
            panic!("noncanonical C2a {label} selection must stay Legacy")
        };
        assert!(matches!(
            trace.rejections.first(),
            Some(AutoAuthorityRejection::PropertyScrollPlan { .. })
        ));
    }
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(viewport.compositor.retained_surfaces, pool_before);
}

#[test]
fn retained_auto_scroll_text_area_forest_rejects_nonexact_owner_sets_and_stable_ids() {
    let plan = |arena: &NodeArena,
                roots: &[NodeKey],
                properties: &PropertyTrees,
                generations: &PaintGenerationTracker| {
        crate::view::paint::plan_and_validate_property_scroll_scene(
            arena,
            roots,
            properties,
            generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8Unorm,
            crate::view::paint::ScrollSceneSingleTextureBudget::new(4096, 128 * 1024 * 1024)
                .unwrap(),
        )
    };

    let (mut arena, roots, mut properties, generations) = prepared_scroll_text_area_scene();
    let extra = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_aff0, 0.0, 0.0, 1.0, 1.0,
    ))));
    let extra_state = properties
        .states
        .get(&roots[0])
        .expect("root property state")
        .clone();
    properties.states.insert(extra, extra_state);
    assert!(
        plan(&arena, &roots, &properties, &generations).is_err(),
        "an extra unreachable property-state key must fail closed"
    );

    let (arena, roots, mut properties, generations) = prepared_scroll_text_area_scene();
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let generated = arena.children_of(text_area)[0];
    properties.states.remove(&generated);
    assert!(
        plan(&arena, &roots, &properties, &generations).is_err(),
        "a missing generated-child property-state key must fail closed"
    );

    let (arena, roots, _, _) = prepared_scroll_text_area_scene();
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let wrapper_stable_id = arena.get(wrapper).expect("wrapper").element.stable_id();
    arena
        .get_mut(text_area)
        .expect("TextArea")
        .element
        .as_any_mut()
        .downcast_mut::<TextArea>()
        .expect("TextArea type")
        .node_id = wrapper_stable_id;
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert!(
        plan(&arena, &roots, &properties, &generations).is_err(),
        "TextArea/wrapper stable-id collision must fail closed"
    );

    let (arena, roots, _, _) = prepared_scroll_text_area_scene();
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let generated = arena.children_of(text_area)[0];
    let text_area_stable_id = arena.get(text_area).expect("TextArea").element.stable_id();
    let mut generated_node = arena.get_mut(generated).expect("generated TextArea child");
    if let Some(run) = generated_node
        .element
        .as_any_mut()
        .downcast_mut::<crate::view::base_component::text_area::TextAreaTextRun>(
    ) {
        run.node_id = text_area_stable_id;
    } else if let Some(line_break) = generated_node
        .element
        .as_any_mut()
        .downcast_mut::<crate::view::base_component::text_area::TextAreaLineBreak>(
    ) {
        line_break.node_id = text_area_stable_id;
    } else {
        panic!("C1 fixture generated child must be run/line-break")
    }
    drop(generated_node);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert!(
        plan(&arena, &roots, &properties, &generations).is_err(),
        "TextArea/generated-child stable-id collision must fail closed"
    );
}
