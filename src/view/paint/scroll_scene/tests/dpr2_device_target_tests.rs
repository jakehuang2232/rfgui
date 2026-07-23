use super::*;

#[test]
fn direct_scroll_transform_dpr2_prepares_emits_and_rejects_device_contract_drift() {
    let (arena, root, properties, generations) = direct_scroll_transform_dpr_fixture();
    assert!(
        arena
            .get(root)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .exact_retained_scroll_transform_host_admission(root, &arena, 2.0)
            .is_some(),
        "DPR2 direct S->T Element admission"
    );
    let make = || {
        let scaffold = plan_direct_scroll_transform_scene_scaffold(
            &arena,
            &[root],
            &properties,
            &generations,
            2.0,
            [0.0; 2],
            None,
        )
        .expect("DPR2 direct S->T scaffold");
        let geometry = plan_direct_scroll_transform_geometry(
            &arena,
            scaffold,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .expect("DPR2 direct S->T geometry");
        compile_direct_scroll_transform_transaction(geometry)
            .expect("DPR2 direct S->T transaction")
    };
    let transaction = make();
    assert!(transaction.is_canonical());
    assert_dpr2_target(transaction.stamp_for_test(), [120, 240]);

    let mut descriptor = transaction.clone();
    descriptor.tamper_synchronized_descriptor_for_test();
    assert!(!descriptor.is_canonical());
    let mut origin = transaction.clone();
    origin.tamper_synchronized_descriptor_origin_for_test();
    assert!(!origin.is_canonical());
    let mut scale = transaction.clone();
    scale.tamper_synchronized_scale_for_test();
    assert!(!scale.is_canonical());

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut paint_origin =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0);
    paint_origin.set_paint_offset([0.5, 0.0]);
    let mut scissor = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0);
    scissor.replace_scissor_rect(Some([0, 0, 240, 180]));
    for ctx in [
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        paint_origin,
        scissor,
    ] {
        let mut graph = FrameGraph::new();
        assert!(matches!(
            prepare_direct_scroll_transform_scene_from_pool(
                &mut viewport,
                transaction.clone(),
                &mut graph,
                ctx,
                [0.0; 4],
                owner,
            ),
            Err(RetainedPropertyScrollScenePrepareError::ContextMismatch)
        ));
        assert!(graph.pass_descriptors().is_empty());
    }
    let mut graph = FrameGraph::new();
    let prepared = prepare_direct_scroll_transform_scene_from_pool(
        &mut viewport,
        transaction,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
        [0.0; 4],
        owner,
    )
    .expect("DPR2 direct S->T prepare");
    let terminal = prepared.parent_terminal_for_test();
    let outcome = emit_prepared_direct_scroll_transform_scene(prepared);
    assert_eq!(outcome.state.opaque_rect_order(), terminal);
    assert_eq!(outcome.trace.reraster_count, 1);
    let composites = graph.test_graphics_passes::<TextureCompositePass>();
    assert_eq!(composites.len(), 1);
    assert_eq!(
        composites[0].test_snapshot().explicit_scissor_rect,
        Some([0, 0, 120, 90]),
        "DPR2 direct S->T freezes the logical scissor consumed by physical conversion"
    );
}

#[test]
fn transform_scroll_dpr2_plan_prepare_emit_freezes_device_targets() {
    let (arena, root, _, _, properties, generations) =
        transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(7.0, 5.0, 0.0)));
    let scene = plan_and_validate_transform_scroll_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        2.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("DPR2 T->S scene");
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_transform_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
        [0.0; 4],
        owner,
    )
    .expect("DPR2 T->S prepare");
    assert_dpr2_target(&prepared.roots[0].receiver_stamp, [120, 90]);
    assert_dpr2_target(
        &prepared.roots[0].boundary.group.ordered_stamps()[0],
        [120, 240],
    );
    let terminal = prepared.roots[0].receiver_opaque_terminal;
    let outcome = emit_prepared_retained_transform_scroll_scene(prepared);
    assert_eq!(outcome.state.opaque_rect_order(), terminal);
    assert_eq!(outcome.trace.reraster_count, 2);
}

#[test]
fn effect_scroll_dpr2_plan_prepare_emit_freezes_device_targets() {
    let (arena, root, _, _, mut properties, mut generations) =
        transform_scroll_fixture(glam::Mat4::IDENTITY);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.625);
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let scene = plan_and_validate_effect_scroll_scene_checkpoint(
        &arena,
        &[root],
        &properties,
        &generations,
        2.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("DPR2 E->S scene");
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_effect_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
        [0.0; 4],
        owner,
    )
    .expect("DPR2 E->S prepare");
    assert_dpr2_target(&prepared.roots[0].receiver_stamp, [120, 90]);
    assert_dpr2_target(
        &prepared.roots[0].boundary.group.ordered_stamps()[0],
        [120, 240],
    );
    let terminal = prepared.roots[0].receiver_opaque_terminal;
    let outcome = emit_prepared_retained_effect_scroll_scene(prepared);
    assert_eq!(outcome.state.opaque_rect_order(), terminal);
    assert_eq!(outcome.trace.reraster_count, 2);
}

#[test]
fn transform_effect_scroll_dpr2_plan_prepare_emit_freezes_device_targets() {
    let (arena, root, properties, generations) = transform_effect_scroll_fixture();
    let scene = plan_and_validate_transform_effect_scroll_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        2.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("DPR2 T->E->S scene");
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_transform_effect_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
        [0.0; 4],
        owner,
    )
    .expect("DPR2 T->E->S prepare");
    assert_dpr2_target(&prepared.roots[0].outer_stamp, [120, 90]);
    assert_dpr2_target(&prepared.roots[0].inner.receiver_stamp, [120, 90]);
    assert_dpr2_target(
        &prepared.roots[0].inner.boundary.group.ordered_stamps()[0],
        [120, 240],
    );
    let terminal = prepared.roots[0].outer_opaque_terminal;
    let outcome = emit_prepared_retained_transform_effect_scroll_scene(prepared);
    assert_eq!(outcome.state.opaque_rect_order(), terminal);
    assert_eq!(outcome.trace.reraster_count, 3);
}

#[test]
fn effect_transform_scroll_dpr2_plan_prepare_emit_freezes_device_targets() {
    let (arena, root, properties, generations) = effect_transform_scroll_fixture();
    let scene = validated_effect_transform_scroll_fixture_scene(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
        2.0,
    );
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_effect_transform_scroll_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
        [0.0; 4],
        owner,
    )
    .expect("DPR2 E->T->S prepare");
    let outer_logical = prepared.roots[0]
        .outer_stamp
        .target
        .source_bounds_bits
        .map(f32::from_bits);
    assert_dpr2_target(
        &prepared.roots[0].outer_stamp,
        [outer_logical[2] as u32, outer_logical[3] as u32],
    );
    assert_dpr2_target(&prepared.roots[0].inner.receiver_stamp, [120, 90]);
    assert_dpr2_target(
        &prepared.roots[0].inner.boundary.group.ordered_stamps()[0],
        [120, 240],
    );
    let outcome = emit_prepared_retained_effect_transform_scroll_scene(prepared);
    assert_eq!(outcome.trace.reraster_count, 3);
}
