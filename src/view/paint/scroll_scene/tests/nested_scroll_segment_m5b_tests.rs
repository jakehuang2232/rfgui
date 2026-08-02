use super::*;
use crate::view::paint::RETAINED_CHILD_MASK_SLOT;
use crate::view::paint::TransformSurfacePlanContext;

fn compile_m5b_scene(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    scale_factor: f32,
) -> ValidatedNestedScrollSegmentScene {
    plan_and_validate_nested_scroll_segment_scene(
        arena,
        &[root],
        properties,
        generations,
        scale_factor,
        [0.0; 2],
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("M5b exact nested-scroll segment scene")
}

#[test]
fn nested_scroll_m5b_ready_image_and_svg_match_exact_persistent_leaf_roles() {
    for (kind, expected_role) in [
        (NestedMediaLeafKind::Image, PaintChunkRole::ImageContent),
        (NestedMediaLeafKind::Svg, PaintChunkRole::SvgContent),
    ] {
        let (arena, root, _inner, _leaf, properties, generations) =
            nested_scroll_media_fixture(kind);
        let scene = compile_m5b_scene(&arena, root, &properties, &generations, 1.0);
        let role = scene
            .program
            .iter()
            .find_map(|step| match step {
                NestedScrollSegmentProgramStep::LeafRaster { artifact, .. } => {
                    artifact.chunks.first().map(|chunk| chunk.id.role)
                }
                _ => None,
            })
            .expect("exact ready media segment owns one leaf raster chunk");
        assert_eq!(role, expected_role, "ready {kind:?} payload role");
    }
}

fn install_fractional_nested_geometry(
    arena: &NodeArena,
    root: NodeKey,
    inner: NodeKey,
    leaf: NodeKey,
) {
    let origin = [10.0, 20.0];
    let outer_offset_y = 0.5;
    let inner_offset_y = 0.25;
    for (key, target) in [
        (root, origin),
        (inner, [origin[0], origin[1] - outer_offset_y]),
    ] {
        let mut element = crate::view::test_support::get_element_mut::<Element>(arena, key);
        let bounds = element.box_model_snapshot();
        element.translate_in_place(target[0] - bounds.x, target[1] - bounds.y);
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let target = [origin[0], origin[1] - outer_offset_y - inner_offset_y];
        let mut node = arena.get_mut(leaf).expect("nested Text leaf exists");
        let bounds = node.element.box_model_snapshot();
        node.element
            .translate_in_place(target[0] - bounds.x, target[1] - bounds.y);
        node.element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    for (key, offset_y) in [(root, outer_offset_y), (inner, inner_offset_y)] {
        let mut element = crate::view::test_support::get_element_mut::<Element>(arena, key);
        element.set_scroll_offset((0.0, offset_y));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
}

fn tamper_overlay_and_resign(
    scene: &mut ValidatedNestedScrollSegmentScene,
    boundary_ordinal: u32,
    mutate: impl FnOnce(&mut PaintArtifact),
) {
    let boundary = scene.transaction.seal.ordered_boundaries[boundary_ordinal as usize];
    let artifact = scene
        .program
        .iter_mut()
        .find_map(|step| match step {
            NestedScrollSegmentProgramStep::OverlayAfter {
                boundary: candidate,
                artifact,
            } if *candidate == boundary => Some(artifact),
            _ => None,
        })
        .expect("exact overlay step exists");
    mutate(artifact);
    let identity = PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)
        .expect("tampered artifact remains structurally serializable");
    let RetainedPropertyScrollGenericAuthority::NestedScrollSegmentCompiler(contract) =
        &mut scene.transaction.generic_authority
    else {
        panic!("nested segment compiler authority")
    };
    for witness in [&mut contract.compiled, &mut contract.planned] {
        let step = witness
            .steps
            .iter_mut()
            .find(|step| {
                step.phase == NestedScrollSegmentPhase::OverlayAfter && step.boundary == boundary
            })
            .expect("exact overlay contract step exists");
        step.artifact = identity.clone();
    }
}

fn resign_direct_leaf(scene: &mut ValidatedNestedScrollSegmentScene) {
    let leaf_authority = scene.leaf_authority.clone();
    let RetainedPropertyScrollGenericAuthority::NestedScrollSegmentCompiler(contract) =
        &mut scene.transaction.generic_authority
    else {
        panic!("nested segment compiler authority")
    };
    contract.compiled.leaf_authority = leaf_authority;
    contract.planned = contract.compiled.clone();
}

#[test]
fn nested_scroll_m5b_leaf_authority_is_mutually_exclusive_and_direct_text_is_zero_resident() {
    let (rect_arena, rect_root, _, _, rect_properties, rect_generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let rect = compile_m5b_scene(
        &rect_arena,
        rect_root,
        &rect_properties,
        &rect_generations,
        1.0,
    );
    assert!(matches!(
        rect.leaf_authority,
        NestedScrollSegmentLeafAuthority::Persistent(_)
    ));
    assert_eq!(rect.transaction.scroll_groups.len(), 1);
    assert_eq!(rect.transaction.seal.scroll_bindings.len(), 1);

    let (text_arena, text_root, _, _, text_properties, text_generations) =
        nested_scroll_text_fixture();
    let text = compile_m5b_scene(
        &text_arena,
        text_root,
        &text_properties,
        &text_generations,
        1.0,
    );
    assert!(matches!(
        text.leaf_authority,
        NestedScrollSegmentLeafAuthority::DirectText(_)
    ));
    assert!(text.transaction.is_exact_zero_resident_nested_text());
    assert!(text.transaction.generic_full_set.is_empty());
    assert!(text.transaction.scroll_groups.is_empty());
    assert!(text.transaction.seal.generic_bindings.is_empty());
    assert!(text.transaction.seal.scroll_bindings.is_empty());
}

#[test]
fn nested_scroll_m5b_direct_text_is_not_gated_by_persistent_backing_budget() {
    let (arena, root, _, _, properties, generations) = nested_scroll_text_fixture();
    let scene = plan_and_validate_nested_scroll_segment_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        ScrollSceneSingleTextureBudget::new(1, 1).expect("non-zero zero-residency budget"),
    )
    .expect("DirectText does not allocate or budget a persistent backing");
    assert!(matches!(
        scene.leaf_authority,
        NestedScrollSegmentLeafAuthority::DirectText(_)
    ));
    assert!(scene.transaction.is_exact_zero_resident_nested_text());
}

#[test]
fn nested_scroll_m5b_direct_text_typed_witness_tamper_fails_closed() {
    let (arena, root, _, _, properties, generations) = nested_scroll_text_fixture();
    let scene = compile_m5b_scene(&arena, root, &properties, &generations, 2.0);
    assert!(scene.is_canonical());

    let mut origin_drift = scene.clone();
    let NestedScrollSegmentLeafAuthority::DirectText(direct) = &mut origin_drift.leaf_authority
    else {
        panic!("Text scene owns direct authority")
    };
    direct.paint.actual_origin_bits[1] =
        (f32::from_bits(direct.paint.actual_origin_bits[1]) + 1.0).to_bits();
    resign_direct_leaf(&mut origin_drift);
    assert!(!origin_drift.is_canonical());

    let mut mask_drift = scene.clone();
    let NestedScrollSegmentLeafAuthority::DirectText(direct) = &mut mask_drift.leaf_authority
    else {
        panic!("Text scene owns direct authority")
    };
    direct.mask_depth += 1;
    resign_direct_leaf(&mut mask_drift);
    assert!(!mask_drift.is_canonical());

    let mut emitted_drift = scene.clone();
    let NestedScrollSegmentLeafAuthority::DirectText(direct) = &mut emitted_drift.leaf_authority
    else {
        panic!("Text scene owns direct authority")
    };
    direct.emitted_artifact.chunks[0].bounds_bits[0] =
        (f32::from_bits(direct.emitted_artifact.chunks[0].bounds_bits[0]) + 1.0).to_bits();
    resign_direct_leaf(&mut emitted_drift);
    assert!(!emitted_drift.is_canonical());

    let mut scissor_drift = scene.clone();
    let NestedScrollSegmentLeafAuthority::DirectText(direct) = &mut scissor_drift.leaf_authority
    else {
        panic!("Text scene owns direct authority")
    };
    direct.final_scissor[0] += 1;
    resign_direct_leaf(&mut scissor_drift);
    assert!(!scissor_drift.is_canonical());

    let mut chain_drift = scene.clone();
    let NestedScrollSegmentLeafAuthority::DirectText(direct) = &mut chain_drift.leaf_authority
    else {
        panic!("Text scene owns direct authority")
    };
    direct.boundary_chain.swap(0, 1);
    resign_direct_leaf(&mut chain_drift);
    assert!(!chain_drift.is_canonical());

    let mut context_drift = scene;
    let NestedScrollSegmentLeafAuthority::DirectText(direct) = &mut context_drift.leaf_authority
    else {
        panic!("Text scene owns direct authority")
    };
    direct.context = TransformSurfacePlanContext::new([1.0, 0.0], None);
    resign_direct_leaf(&mut context_drift);
    assert!(!context_drift.is_canonical());
}

#[test]
fn nested_scroll_m5b_direct_text_dpr_and_format_drift_fail_before_graph_or_pool_mutation() {
    let (arena, root, _, _, properties, generations) = nested_scroll_text_fixture();
    let scene = compile_m5b_scene(&arena, root, &properties, &generations, 2.0);

    for mutate in [
        |direct: &mut NestedScrollSegmentDirectTextContract| {
            direct.scale_factor_bits = 1.0_f32.to_bits();
        },
        |direct: &mut NestedScrollSegmentDirectTextContract| {
            direct.target_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        },
    ] {
        let mut drift = scene.clone();
        let NestedScrollSegmentLeafAuthority::DirectText(direct) = &mut drift.leaf_authority else {
            panic!("Text scene owns direct authority")
        };
        mutate(direct);
        resign_direct_leaf(&mut drift);
        assert!(drift.is_canonical());

        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let error = match prepare_nested_scroll_segment_scene_from_pool(
            &mut viewport,
            drift,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
            [0.0, 0.0, 0.0, 1.0],
            owner,
        ) {
            Ok(_) => panic!("DPR or format drift must fail before graph mutation"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            RetainedPropertyScrollScenePrepareError::ContextMismatch
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
    }
}

#[test]
fn nested_scroll_m5b_direct_text_stages_zero_transaction_and_clears_old_active_set() {
    let (rect_arena, rect_root, _, _, rect_properties, rect_generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let rect = compile_m5b_scene(
        &rect_arena,
        rect_root,
        &rect_properties,
        &rect_generations,
        1.0,
    );
    let mut viewport = Viewport::new();
    let rect_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut rect_graph = FrameGraph::new();
    let rect_prepared = prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        rect,
        &mut rect_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        rect_owner,
    )
    .unwrap();
    emit_prepared_nested_scroll_segment_scene(rect_prepared);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(rect_owner), true));
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 1);

    let (text_arena, text_root, _, _, text_properties, text_generations) =
        nested_scroll_text_fixture();
    let text = compile_m5b_scene(
        &text_arena,
        text_root,
        &text_properties,
        &text_generations,
        1.0,
    );
    let text_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut text_graph = FrameGraph::new();
    let prepared = prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        text,
        &mut text_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        text_owner,
    )
    .unwrap();
    assert!(prepared.actions.is_empty());
    emit_prepared_nested_scroll_segment_scene(prepared);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (1, Some(0))
    );
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 1);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(text_owner), true));
    assert_eq!(viewport.property_scroll_active_resident_count_for_test(), 0);
}

#[test]
fn nested_scroll_m5b_fractional_ancestor_and_inner_offsets_pass_exact_segment_seals() {
    let (arena, root, inner, leaf, mut properties, mut generations) = nested_scroll_text_fixture();
    install_fractional_nested_geometry(&arena, root, inner, leaf);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    assert!(properties.validation_errors.is_empty());

    let scene = compile_m5b_scene(&arena, root, &properties, &generations, 2.0);
    let RetainedPropertyScrollGenericAuthority::NestedScrollSegmentCompiler(contract) =
        &scene.transaction.generic_authority
    else {
        panic!("nested segment compiler authority")
    };
    let inner_boundary = &contract.compiled.boundaries[1];
    let inner_overlay = contract
        .compiled
        .steps
        .iter()
        .find(|step| {
            step.phase == NestedScrollSegmentPhase::OverlayAfter
                && step.boundary == inner_boundary.boundary
        })
        .expect("inner overlay contract step");
    assert_ne!(
        inner_overlay.artifact.chunks.first().unwrap().bounds_bits,
        inner_boundary.source_bounds_bits,
        "overlay head is the normalized mask-end chunk"
    );
    assert_eq!(
        inner_overlay.artifact.chunks.last().unwrap().bounds_bits,
        inner_boundary.source_bounds_bits,
        "overlay tail is the compiler-sealed self chunk"
    );
    let NestedScrollSegmentLeafAuthority::DirectText(direct) = &contract.compiled.leaf_authority
    else {
        panic!("fractional standalone Text must select the direct leaf authority")
    };
    assert!(direct.paint.is_canonical());
    assert!(scene.transaction.is_exact_zero_resident_nested_text());

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .expect("fractional exact snapshots remain executable without weakening a seal");
    assert!(matches!(
        prepared.leaf,
        PreparedNestedScrollSegmentLeaf::DirectText { .. }
    ));
    assert!(prepared.actions.is_empty());
    let outcome = emit_prepared_nested_scroll_segment_scene(prepared);
    assert_eq!(
        (outcome.trace.reraster_count, outcome.trace.reuse_count),
        (0, 0)
    );
    assert!(graph.declared_persistent_texture_keys().next().is_none());
    assert!(
        graph
            .test_graphics_passes::<TextureCompositePass>()
            .is_empty()
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>()
            .len(),
        1
    );
    assert!(!viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

#[test]
fn nested_scroll_m5b_overlay_self_bounds_drift_fails_the_segment_contract() {
    let (arena, root, _inner, _leaf, properties, generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let mut scene = compile_m5b_scene(&arena, root, &properties, &generations, 1.0);
    tamper_overlay_and_resign(&mut scene, 1, |artifact| {
        artifact.chunks.last_mut().unwrap().bounds.y += 1.0;
    });
    assert!(!scene.is_canonical());
}

#[test]
fn nested_scroll_m5b_missing_overlay_mask_head_fails_the_joint_compiler_token() {
    let (arena, root, _inner, _leaf, properties, generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let mut scene = compile_m5b_scene(&arena, root, &properties, &generations, 1.0);
    tamper_overlay_and_resign(&mut scene, 1, |artifact| {
        let mask = artifact.chunks.remove(0);
        assert_eq!(mask.id.slot, RETAINED_CHILD_MASK_SLOT);
        assert_eq!(mask.op_range, 0..1);
        artifact.ops.remove(0);
        for chunk in &mut artifact.chunks {
            chunk.op_range = chunk.op_range.start - 1..chunk.op_range.end - 1;
        }
    });
    assert!(
        scene.is_canonical(),
        "the phase seal deliberately delegates the cross-H/O mask pair to the joint compiler token"
    );

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let error = match prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    ) {
        Ok(_) => panic!("missing overlay mask-end must not mint the joint compiler token"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        RetainedPropertyScrollScenePrepareError::BoundaryDrift
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
}

#[test]
fn nested_scroll_m5b_leaf_paint_generation_change_rerasterizes_only_the_leaf() {
    let (arena, root, _inner, leaf, mut properties, mut generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    let baseline = compile_m5b_scene(&arena, root, &properties, &generations, 1.0);
    let baseline_stamp = baseline.persistent_leaf_stamp_for_test().clone();
    let mut viewport = Viewport::new();
    let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut cold_graph = FrameGraph::new();
    let cold = prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        baseline,
        &mut cold_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        cold_owner,
    )
    .unwrap();
    emit_prepared_nested_scroll_segment_scene(cold);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

    crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
        .set_background_color_value(Color::rgb(192, 32, 64));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let changed = compile_m5b_scene(&arena, root, &properties, &generations, 1.0);
    assert_ne!(changed.persistent_leaf_stamp_for_test(), &baseline_stamp);

    let changed_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut changed_graph = FrameGraph::new();
    let mut prepared = prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        changed,
        &mut changed_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        changed_owner,
    )
    .unwrap();
    prepared.refresh_action_from_committed_test_pool();
    let prepared_stamp = prepared.persistent_leaf_parts_for_test().0;
    assert_eq!(
        prepared.actions[&prepared_stamp.identity.resident_key()],
        RetainedSurfaceCompileAction::Reraster
    );
    let outcome = emit_prepared_nested_scroll_segment_scene(prepared);
    assert_eq!(
        (outcome.trace.reraster_count, outcome.trace.reuse_count),
        (1, 0)
    );
    assert_eq!(changed_graph.test_graphics_passes::<ClearPass>().len(), 2);
    assert_eq!(
        changed_graph
            .test_graphics_passes::<TextureCompositePass>()
            .len(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(changed_owner), true));
}

#[test]
fn nested_scroll_m5b_reverse_overlay_order_paints_outer_scrollbar_last() {
    let (arena, root, inner, _leaf, mut properties, mut generations) =
        crate::view::paint::frame_plan::tests::nested_scroll_plan_fixture();
    for key in [root, inner] {
        crate::view::test_support::get_element_mut::<Element>(&arena, key)
            .set_sampled_scrollbar_alpha_for_test(1.0);
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let scene = compile_m5b_scene(&arena, root, &properties, &generations, 1.0);
    let boundaries = &scene
        .plan
        .property_scroll_planning_scaffold()
        .unwrap()
        .boundaries;
    let expected = boundaries
        .iter()
        .rev()
        .flat_map(|boundary| {
            let overlay = boundary.scroll.scrollbar_overlay;
            [
                overlay.vertical_track.unwrap(),
                overlay.vertical_thumb.unwrap(),
            ]
        })
        .map(|rect| [rect.x, rect.y, rect.width, rect.height].map(f32::to_bits))
        .collect::<Vec<_>>();

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_nested_scroll_segment_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .unwrap();
    emit_prepared_nested_scroll_segment_scene(prepared);

    let track_color = [0.95, 0.95, 0.95, 0.35].map(f32::to_bits);
    let thumb_color = [0.95, 0.95, 0.95, 0.58].map(f32::to_bits);
    let painted = graph
        .test_rect_pass_snapshots()
        .into_iter()
        .filter(|rect| rect.fill_color_bits == track_color || rect.fill_color_bits == thumb_color)
        .map(|rect| {
            [
                rect.position_bits[0],
                rect.position_bits[1],
                rect.size_bits[0],
                rect.size_bits[1],
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        painted, expected,
        "inner overlay must paint before outer overlay"
    );
    assert_eq!(
        &painted[painted.len() - 2..],
        &expected[expected.len() - 2..]
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}
