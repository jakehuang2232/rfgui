use super::*;

#[test]
fn direct_scroll_transform_admission_is_isolated_from_the_b0_oracle() {
    let (plain_arena, plain_root, _, _) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::FrameRootScroll);
    let plain_node = plain_arena.get(plain_root).expect("plain scroll host");
    let plain = plain_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .expect("plain scroll host element");
    assert!(
        plain
            .exact_retained_scroll_host_admission(plain_root, &plain_arena, 1.0)
            .is_some()
    );
    assert!(
        plain
            .exact_retained_scroll_transform_host_admission(plain_root, &plain_arena, 1.0)
            .is_none()
    );

    let (transform_arena, transform_root, _, _) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let transform_node = transform_arena
        .get(transform_root)
        .expect("scroll-transform host");
    let transform = transform_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .expect("scroll-transform host element");
    assert!(
        transform
            .exact_retained_scroll_host_admission(transform_root, &transform_arena, 1.0)
            .is_none()
    );
    let admission = transform
        .exact_retained_scroll_transform_host_admission(transform_root, &transform_arena, 1.0)
        .expect("direct transformed content admission");
    assert_eq!(admission.boundary_root, transform_root);
    assert_eq!(admission.transform_content, transform.children()[0]);
}

#[test]
fn direct_scroll_transform_recorders_seal_host_marker_overlay_and_offset_zero_content() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let root_node = arena.get(root).expect("scroll host");
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .expect("scroll host element");
    let admission = root_element
        .exact_retained_scroll_transform_host_admission(root, &arena, 1.0)
        .expect("direct S->T admission");
    let child = admission.transform_content;
    let scroll = properties
        .scroll_snapshot_for(ScrollNodeId(root))
        .expect("scroll snapshot");
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let clip = properties
        .clip_snapshot_for(Some(clip_id))
        .and_then(|chain| (chain.len() == 1).then(|| chain[0]))
        .expect("contents clip");
    let marker = PlannedBoundary {
        root: child,
        stable_id: admission.transform_content_stable_id,
        kind: PlannedBoundaryKind::Transform(TransformNodeId(child)),
    };
    let host_witness = PaintBakedScrollHostWitness::new(root, child, scroll, clip_id)
        .expect("baked host witness");
    let host_steps = super::super::super::frame_recorder::record_scroll_transform_host_steps_for_plan(
        &arena,
        root,
        &properties,
        &generations,
        host_witness,
        [0.0, 0.0],
        marker,
    )
    .expect("exact H-marker-O host recording");
    assert!(matches!(
        host_steps.as_slice(),
        [
            super::super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(_),
            super::super::super::frame_recorder::RecordedTransformSurfaceStep::Boundary(found),
            super::super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(_),
        ] if *found == marker
    ));

    let content_witness = PaintScrollContentWitness::new(root, child, scroll, clip)
        .expect("scroll-content witness");
    let content_steps =
        super::super::super::frame_recorder::record_scroll_transform_content_steps_for_plan(
            &arena,
            child,
            &properties,
            &generations,
            PaintTransformSurfaceWitness::canonical_root(child),
            content_witness,
        )
        .expect("offset-zero transformed content recording");
    let [super::super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact)] =
        content_steps.as_slice()
    else {
        panic!("one transformed-content artifact")
    };
    assert!(artifact.chunks.iter().all(|chunk| {
        chunk.properties.transform == Some(TransformNodeId(child))
            && chunk.properties.scroll.is_none()
            && chunk.properties.clip.is_none()
    }));
}

#[test]
fn direct_scroll_transform_schedule_seals_only_s_then_direct_translation_content() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let plan = super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0, 0.0],
        None,
    )
    .expect("exact [S, T-content] schedule");
    assert!(plan.is_canonical());

    let dpr2 = super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
        &arena,
        &[root],
        &properties,
        &generations,
        2.0,
        [0.0, 0.0],
        None,
    )
    .expect("exact [S, T-content] schedule supports DPR2");
    assert!(dpr2.is_canonical());

    let (plain_arena, plain_root, plain_properties, plain_generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::FrameRootScroll);
    assert!(
        super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            &plain_arena,
            &[plain_root],
            &plain_properties,
            &plain_generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .is_err()
    );
}

#[test]
fn direct_scroll_transform_geometry_freezes_offset_zero_raster_and_one_xy_projection() {
    let (arena, root, _, _) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let child = arena.children_of(root)[0];
    {
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root_element.layout_state.layout_position.x = 10.0;
        root_element.layout_state.layout_position.y = 20.0;
        root_element.layout_state.content_size.width = 240.0;
        root_element.set_scroll_offset((3.5, 47.25));
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut child_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, child);
        child_element.layout_state.layout_position.x = 6.5;
        child_element.layout_state.layout_position.y = -27.25;
        child_element.layout_state.layout_size.width = 240.0;
        child_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    let observation = {
        let node = arena.get(root).unwrap();
        node.element.scroll_geometry_observation(root, &arena)
    };
    let crate::view::base_component::ScrollGeometryObservation::Exact(observation) =
        observation
    else {
        panic!("{observation:?}")
    };
    assert_eq!(
        observation.offset.map(f32::to_bits),
        [3.5, 47.25].map(f32::to_bits)
    );
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "{:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let scaffold = super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0, 0.0],
        None,
    )
    .expect("nonzero-origin full-2D S->T scaffold");
    assert_eq!(scaffold.overlay_op_count_for_test(), 0);
    let geometry = super::super::super::scroll_scene::plan_direct_scroll_transform_geometry(
        &arena,
        scaffold,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        super::super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
            .unwrap(),
    )
    .expect("single direct transformed-content backing");
    assert!(geometry.is_canonical());
    assert_eq!(
        [
            geometry.raster_bounds().x,
            geometry.raster_bounds().y,
            geometry.raster_bounds().width,
            geometry.raster_bounds().height,
        ]
        .map(f32::to_bits),
        [10.0, 20.0, 240.0, 240.0].map(f32::to_bits),
    );
    let params = geometry.composite_params();
    assert_eq!(
        params.bounds.map(f32::to_bits),
        [9.5, -27.25, 240.0, 240.0].map(f32::to_bits),
    );
    assert_ne!(
        params.bounds.map(f32::to_bits),
        [13.0, 20.0, 240.0, 240.0].map(f32::to_bits),
        "scroll projection must not be omitted",
    );
    assert_ne!(
        params.bounds.map(f32::to_bits),
        [6.0, -74.5, 240.0, 240.0].map(f32::to_bits),
        "scroll projection must not be applied twice",
    );
    assert_eq!(
        params
            .quad_positions
            .expect("direct transformed quad")
            .map(|point| point.map(f32::to_bits)),
        [
            [9.5, 212.75],
            [249.5, 212.75],
            [249.5, -27.25],
            [9.5, -27.25],
        ]
        .map(|point| point.map(f32::to_bits)),
    );
    assert_eq!(
        params.uv_bounds.expect("offset-zero UV").map(f32::to_bits),
        [10.0, 20.0, 240.0, 240.0].map(f32::to_bits),
    );
    assert_eq!(params.scissor_rect, Some([10, 20, 120, 90]));
}

#[test]
fn direct_scroll_transform_geometry_rejects_rotation_and_tiling_fallback() {
    let (arena, root, _, _) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let scaffold = super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0, 0.0],
        None,
    )
    .expect("direct translation scaffold");
    assert!(
        super::super::super::scroll_scene::plan_direct_scroll_transform_geometry(
            &arena,
            scaffold,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            super::super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(64, u64::MAX)
                .unwrap(),
        )
        .is_err()
    );

    let child = arena.children_of(root)[0];
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_rotation_z(0.25)));
    let mut rotated_properties = PropertyTrees::default();
    rotated_properties.sync(&arena, &[root]);
    let mut rotated_generations = PaintGenerationTracker::default();
    rotated_generations.sync(&arena, &[root], &rotated_properties);
    assert!(
        super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            &arena,
            &[root],
            &rotated_properties,
            &rotated_generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .is_err()
    );
}

#[test]
fn direct_scroll_transform_frozen_artifact_geometry_and_backing_tamper_fail_closed() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let scaffold = super::super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0, 0.0],
        None,
    )
    .expect("sealed direct S->T scaffold");
    let mut artifact_tamper = scaffold.clone();
    artifact_tamper.tamper_content_artifact_bounds_for_test();
    assert!(!artifact_tamper.is_canonical());
    assert!(super::super::super::scroll_scene::plan_direct_scroll_transform_geometry(
        &arena,
        artifact_tamper,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        super::super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(
            u32::MAX,
            u64::MAX,
        )
        .unwrap(),
    )
    .is_err());
    let mut host_tamper = scaffold.clone();
    host_tamper.tamper_host_artifact_bounds_for_test();
    assert!(!host_tamper.is_canonical());

    let geometry = super::super::super::scroll_scene::plan_direct_scroll_transform_geometry(
        &arena,
        scaffold.clone(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        super::super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
            .unwrap(),
    )
    .expect("sealed direct geometry");
    let mut geometry_tamper = geometry.clone();
    geometry_tamper.tamper_geometry_seal_for_test();
    assert!(!geometry_tamper.is_canonical());
    let mut backing_tamper = geometry;
    backing_tamper.tamper_backing_seal_for_test();
    assert!(!backing_tamper.is_canonical());

    let child = arena.children_of(root)[0];
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            4.0, 0.0, 0.0,
        ))));
    assert!(matches!(
        super::super::super::scroll_scene::plan_direct_scroll_transform_geometry(
            &arena,
            scaffold,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            super::super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(
                u32::MAX,
                u64::MAX,
            )
            .unwrap(),
        ),
        Err(super::super::super::scroll_scene::PropertyScrollScenePlanError::LiveSnapshotDrift)
    ));
}
