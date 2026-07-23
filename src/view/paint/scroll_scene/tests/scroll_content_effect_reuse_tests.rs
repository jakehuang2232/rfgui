use super::*;

#[test]
fn scroll_content_effect_scroll_only_keeps_all_resident_stamps() {
    for outer_transform in [false, true] {
        let (arena, root, mut properties, mut generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                outer_transform,
                true,
            );
        let baseline = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let baseline = match baseline {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let (_, baseline_frozen) = freeze_scroll_content_effect_transaction(&baseline).unwrap();
        let scroll = baseline.roots[0].boundary.scroll.owner;
        let baseline_offset = baseline.roots[0].boundary.scroll.offset;
        let mut viewport = Viewport::new();
        let baseline_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut baseline_graph = FrameGraph::new();
        let baseline_prepared = prepare_retained_scroll_content_effect_scene_from_pool(
            &mut viewport,
            baseline,
            &mut baseline_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            baseline_owner,
        )
        .unwrap();
        let _ = emit_prepared_retained_scroll_content_effect_scene(baseline_prepared);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(baseline_owner), true,)
        );
        {
            let mut scroll_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
            scroll_element.set_scroll_offset((baseline_offset.x, baseline_offset.y + 7.0));
            // This fixture owns exact preinstalled layout geometry; only
            // the viewport scroll sample changes between frames.
            scroll_element.clear_local_dirty_flags(DirtyPassMask::PLACEMENT);
        }
        let content_root = arena.children_of(scroll)[0];
        let mut pending = vec![content_root];
        while let Some(owner) = pending.pop() {
            pending.extend(arena.children_of(owner));
            let mut element =
                crate::view::test_support::get_element_mut::<Element>(&arena, owner);
            element.layout_state.layout_position.y -= 7.0;
            element.layout_state.layout_inner_position.y -= 7.0;
            element.clear_local_dirty_flags(DirtyPassMask::PLACEMENT);
        }
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let candidate = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let candidate = match candidate {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let (_, candidate_frozen) =
            freeze_scroll_content_effect_transaction(&candidate).unwrap();
        assert_ne!(
            candidate.roots[0].boundary.scroll.offset, baseline_offset,
            "live scroll composite snapshot must advance"
        );
        assert_eq!(
            candidate_frozen[0].effect_stamp,
            baseline_frozen[0].effect_stamp
        );
        assert_eq!(
            candidate_frozen[0].content_stamp,
            baseline_frozen[0].content_stamp
        );
        if outer_transform {
            assert_ne!(
                candidate_frozen[0].outer_stamp, baseline_frozen[0].outer_stamp,
                "T bakes H/C/O and must reraster for a live scroll sample"
            );
        } else {
            assert!(candidate_frozen[0].outer_stamp.is_none());
        }

        let candidate_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut candidate_graph = FrameGraph::new();
        let mut prepared = prepare_retained_scroll_content_effect_scene_from_pool(
            &mut viewport,
            candidate,
            &mut candidate_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            candidate_owner,
        )
        .unwrap();
        prepared.refresh_actions_from_committed_test_pool();
        let frozen = &prepared.roots[0].frozen;
        assert_eq!(
            prepared.actions[&frozen.effect_stamp.identity.resident_key()],
            RetainedSurfaceCompileAction::Reuse
        );
        assert_eq!(
            prepared.actions[&frozen.content_stamp.identity.resident_key()],
            RetainedSurfaceCompileAction::Reuse
        );
        if let Some(outer) = &frozen.outer_stamp {
            assert_eq!(
                prepared.actions[&outer.identity.resident_key()],
                RetainedSurfaceCompileAction::Reraster
            );
        }
        let _ = emit_prepared_retained_scroll_content_effect_scene(prepared);
        assert_eq!(
            candidate_graph.test_graphics_passes::<ClearPass>().len(),
            if outer_transform { 2 } else { 1 }
        );
        assert_eq!(
            candidate_graph
                .test_graphics_passes::<TextureCompositePass>()
                .len(),
            if outer_transform { 2 } else { 1 }
        );
        assert!(
            candidate_graph
                .test_graphics_passes::<CompositeLayerPass>()
                .is_empty()
        );
        assert!(
            viewport
                .finish_retained_surface_transaction_for_frame(Some(candidate_owner), true,)
        );
    }
}

#[test]
fn scroll_content_effect_invalidation_matrix_is_dependency_exact() {
    for mutation in ["effect-opacity", "effect-paint", "wrapper-paint"] {
        let (arena, root, mut properties, mut generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                true, true,
            );
        let baseline = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let baseline = match baseline {
            ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let (_, baseline_frozen) = freeze_scroll_content_effect_transaction(&baseline).unwrap();
        let mut viewport = Viewport::new();
        let baseline_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut baseline_graph = FrameGraph::new();
        let prepared = prepare_retained_scroll_content_effect_scene_from_pool(
            &mut viewport,
            baseline,
            &mut baseline_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            baseline_owner,
        )
        .unwrap();
        let _ = emit_prepared_retained_scroll_content_effect_scene(prepared);
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(baseline_owner), true,)
        );

        let effect = arena.find_by_stable_id(0xb4_3020).unwrap();
        match mutation {
            "effect-opacity" => {
                crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                    .set_opacity(0.5);
            }
            "effect-paint" => {
                crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                    .set_background_color_value(Color::rgb(96, 32, 64));
            }
            "wrapper-paint" => {
                let wrapper = arena.find_by_stable_id(0xb4_3011).unwrap();
                crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
                    .set_background_color_value(Color::rgb(36, 12, 24));
            }
            _ => unreachable!(),
        }
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let candidate = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let candidate = match candidate {
            ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let (_, candidate_frozen) =
            freeze_scroll_content_effect_transaction(&candidate).unwrap();
        let effect_reraster = mutation == "effect-paint";
        assert_eq!(
            candidate_frozen[0].effect_stamp != baseline_frozen[0].effect_stamp,
            effect_reraster,
            "{mutation}: exact E invalidation"
        );
        assert_ne!(
            candidate_frozen[0].content_stamp, baseline_frozen[0].content_stamp,
            "{mutation}: C owns the E composite or wrapper payload"
        );
        assert_ne!(
            candidate_frozen[0].outer_stamp, baseline_frozen[0].outer_stamp,
            "{mutation}: T owns the C composite"
        );

        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let mut prepared = prepare_retained_scroll_content_effect_scene_from_pool(
            &mut viewport,
            candidate,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            owner,
        )
        .unwrap();
        prepared.refresh_actions_from_committed_test_pool();
        let frozen = &prepared.roots[0].frozen;
        assert_eq!(
            prepared.actions[&frozen.effect_stamp.identity.resident_key()],
            if effect_reraster {
                RetainedSurfaceCompileAction::Reraster
            } else {
                RetainedSurfaceCompileAction::Reuse
            },
            "{mutation}: E action"
        );
        assert_eq!(
            prepared.actions[&frozen.content_stamp.identity.resident_key()],
            RetainedSurfaceCompileAction::Reraster,
            "{mutation}: C action"
        );
        assert_eq!(
            prepared.actions[&frozen.outer_stamp.as_ref().unwrap().identity.resident_key()],
            RetainedSurfaceCompileAction::Reraster,
            "{mutation}: T action"
        );
        let _ = emit_prepared_retained_scroll_content_effect_scene(prepared);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }
}

#[test]
fn scroll_content_effect_native_text_image_svg_cover_dpr_and_wrapper_shapes() {
    for (kind, state) in [
        ("text", "ready"),
        ("image", "ready"),
        ("image", "loading"),
        ("image", "error"),
        ("svg", "ready"),
        ("svg", "loading"),
        ("svg", "error"),
    ] {
        for outer_transform in [false, true] {
            for neutral_wrapper in [false, true] {
                let (arena, root, properties, generations) =
                    scroll_content_effect_native_leaf_fixture(
                        kind,
                        state,
                        outer_transform,
                        neutral_wrapper,
                    );
                for scale_factor in [1.0_f32, 2.0] {
                    let scene = PropertyBoundaryDagCompiler::plan_and_validate(
                        &arena,
                        &[root],
                        &properties,
                        &generations,
                        scale_factor,
                        [0.0; 2],
                        None,
                        crate::time::Instant::now(),
                        wgpu::TextureFormat::Bgra8UnormSrgb,
                        generous_budget(),
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "{kind}/{state} outer={outer_transform} wrapper={neutral_wrapper} DPR={scale_factor}: {error:?}"
                        )
                    });
                    let scene = match (scene, outer_transform) {
                        (ValidatedPropertyBoundaryDagScene::ScrollEffect(scene), false)
                        | (
                            ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene),
                            true,
                        ) => scene,
                        _ => panic!("wrong Phase3 grammar"),
                    };
                    let (transaction, frozen) =
                        freeze_scroll_content_effect_transaction(&scene).unwrap();
                    assert!(transaction.is_canonical());
                    assert_eq!(frozen.len(), 1);
                    assert_eq!(
                        frozen[0].effect_stamp.target.scale_factor_bits,
                        scale_factor.to_bits()
                    );
                    assert_eq!(
                        frozen[0].content_stamp.target.scale_factor_bits,
                        scale_factor.to_bits()
                    );
                    assert_eq!(frozen[0].outer_stamp.is_some(), outer_transform);
                }
            }
        }
    }
}

#[test]
fn effect_transform_scroll_graph_token_rejects_effect_identity_and_generation_drift() {
    let (arena, root, properties, generations) = effect_transform_scroll_fixture();
    let scene = validated_effect_transform_scroll_fixture_scene(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
        1.0,
    );

    let mut effect_id = scene.clone();
    effect_id.roots[0].insertion.outer_receiver.id =
        crate::view::compositor::property_tree::EffectNodeId(
            effect_id.roots[0].insertion.inner.receiver.owner,
        );
    assert!(!effect_id.is_canonical());

    let mut effect_generation = scene.clone();
    effect_generation.roots[0]
        .insertion
        .outer_receiver
        .generation = 0;
    assert!(!effect_generation.is_canonical());

    let mut transform_owner = scene;
    transform_owner.roots[0].insertion.inner.receiver.owner = root;
    assert!(!transform_owner.is_canonical());
}
