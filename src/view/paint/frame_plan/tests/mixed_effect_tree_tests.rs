use super::*;

#[test]
fn production_mixed_effect_tree_emits_frozen_child_geometry_and_atomic_stamps() {
    let (arena, root, _, child, _, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    let plan = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .expect("exact mixed plan");
    let root_surface = only_surface(&plan);
    let [_, PaintPlanStep::RetainedSurface(child_surface), _] = root_surface.raster_steps()
    else {
        panic!("mixed fixture keeps one typed child boundary")
    };
    let SurfaceKind::NestedIsolation(nested) = child_surface.kind() else {
        panic!("typed nested isolation")
    };
    let bounds = nested.geometry.source_bounds;

    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    let mut viewport = Viewport::new();
    let outcome = super::super::super::build_retained_effect_tree_from_pool(
        &mut viewport,
        &plan,
        &mut graph,
        ctx,
    )
    .expect("production mixed executor");
    let (state, traces) = outcome.into_parts();
    assert_eq!(state.opaque_rect_order_for_test(), 3);
    assert_eq!(traces.len(), 2);
    assert_eq!(traces[0].boundary_root, root);
    assert_eq!(traces[1].boundary_root, child);
    assert!(
        traces.iter().all(|trace| {
            trace.action == super::super::super::RetainedSurfaceCompileAction::Reraster
        })
    );

    let child_key =
        crate::view::base_component::isolation_layer_stable_key(child_surface.stable_id());
    let child_desc = graph
        .declared_persistent_textures()
        .find_map(|(key, desc)| (key == child_key).then_some(desc))
        .expect("child persistent color descriptor");
    let expected_child_desc = crate::view::base_component::texture_desc_for_logical_bounds(
        bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    assert_eq!(
        [child_desc.width(), child_desc.height()],
        [expected_child_desc.width(), expected_child_desc.height()],
        "child target descriptor is derived only from frozen bounds and scale"
    );
    assert_eq!(child_desc.origin(), expected_child_desc.origin());
    assert_ne!(
        child_desc.origin(),
        (0, 0),
        "positive fractional fixture must preserve a nonzero child-local target origin"
    );
    assert_eq!(
        traces[1].descriptor_size,
        [expected_child_desc.width(), expected_child_desc.height()]
    );

    let composites = graph.test_graphics_passes::<
        crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
    >();
    let [composite] = composites.as_slice() else {
        panic!("one child-local CompositeLayer")
    };
    assert_eq!(
        composite.test_params().rect_pos.map(f32::to_bits),
        [bounds.x.to_bits(), bounds.y.to_bits(),]
    );
    assert_eq!(
        composite.test_params().rect_size.map(f32::to_bits),
        [bounds.width.to_bits(), bounds.height.to_bits(),]
    );
    assert_eq!(composite.test_params().corner_radii, [0.0; 4]);
    assert_eq!(
        composite.test_params().opacity.to_bits(),
        nested.effect.opacity.to_bits()
    );
    assert_eq!(composite.test_params().scissor_rect, None);

    let final_composites =
        graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    let [final_composite] = final_composites.as_slice() else {
        panic!("root CSS transform is emitted exactly once")
    };
    let root_geometry = root_surface.transform_plan_for_test().geometry;
    let final_snapshot = final_composite.test_snapshot();
    assert_eq!(
        final_snapshot.quad_position_bits,
        Some(
            root_geometry
                .quad_positions
                .map(|point| point.map(f32::to_bits))
        )
    );
    assert_eq!(
        final_snapshot.uv_bounds_bits,
        Some(root_geometry.uv_bounds.map(f32::to_bits))
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, Some(2))
    );
    viewport.finish_retained_surface_transaction(false);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );
}

#[test]
fn mixed_effect_tree_cpu_oracle_applies_group_opacity_once_after_source_over() {
    fn premultiplied(fill: [f32; 4], opacity: f32) -> [f32; 4] {
        let alpha = fill[3] * opacity;
        [fill[0] * alpha, fill[1] * alpha, fill[2] * alpha, alpha]
    }
    fn source_over(dst: [f32; 4], src: [f32; 4]) -> [f32; 4] {
        let remainder = 1.0 - src[3];
        [
            src[0] + dst[0] * remainder,
            src[1] + dst[1] * remainder,
            src[2] + dst[2] * remainder,
            src[3] + dst[3] * remainder,
        ]
    }
    fn scaled(color: [f32; 4], opacity: f32) -> [f32; 4] {
        color.map(|channel| channel * opacity)
    }

    let (arena, root, _, _, _, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    let plan = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .unwrap();
    let [_, PaintPlanStep::RetainedSurface(child), _] = only_surface(&plan).raster_steps()
    else {
        panic!("mixed child")
    };
    let SurfaceKind::NestedIsolation(isolation) = child.kind() else {
        panic!("nested isolation")
    };
    let artifact = only_span(child).artifact();
    let rects = artifact
        .ops
        .iter()
        .filter_map(|op| match op {
            PaintOp::DrawRect(op) => Some(&op.params),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [bottom, top] = rects.as_slice() else {
        panic!("fixture child artifact owns two overlapping rects")
    };
    let overlap_width = (bottom.position[0] + bottom.size[0])
        .min(top.position[0] + top.size[0])
        - bottom.position[0].max(top.position[0]);
    let overlap_height = (bottom.position[1] + bottom.size[1])
        .min(top.position[1] + top.size[1])
        - bottom.position[1].max(top.position[1]);
    assert!(overlap_width > 0.0 && overlap_height > 0.0);
    assert_eq!(bottom.opacity.to_bits(), 1.0_f32.to_bits());
    assert_eq!(top.opacity.to_bits(), 1.0_f32.to_bits());

    let group = source_over(
        premultiplied(bottom.fill_color, bottom.opacity),
        premultiplied(top.fill_color, top.opacity),
    );
    let correct = scaled(group, isolation.effect.opacity);
    let manual_top_once = premultiplied(top.fill_color, isolation.effect.opacity);
    assert!(
        correct
            .iter()
            .zip(manual_top_once)
            .all(|(actual, expected)| (actual - expected).abs() <= f32::EPSILON),
        "opaque top rect resolves the group, then root opacity scales premultiplied RGBA once"
    );

    let incorrectly_baked_then_grouped = scaled(
        source_over(
            premultiplied(bottom.fill_color, bottom.opacity * isolation.effect.opacity),
            premultiplied(top.fill_color, top.opacity * isolation.effect.opacity),
        ),
        isolation.effect.opacity,
    );
    assert!(
        correct
            .iter()
            .zip(incorrectly_baked_then_grouped)
            .any(|(actual, wrong)| (actual - wrong).abs() > 0.000_001),
        "per-op baking plus CompositeLayer would double-apply and is not the oracle"
    );
}

#[test]
fn mixed_effect_tree_stamp_excludes_child_opacity_but_parent_dependency_tracks_it() {
    fn child_dependency(
        stamp: &super::super::super::RetainedSurfaceRasterStamp,
    ) -> &super::super::super::NestedSurfaceRasterDependency {
        let super::super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) =
            &stamp.ordered_steps[1]
        else {
            panic!("middle step is nested isolation dependency")
        };
        dependency
    }

    let (arena, root, _, child, _, _, mut properties, mut generations) =
        exact_transform_child_isolation_fixture();
    let baseline = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .unwrap();
    let ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let baseline_stamp = super::super::super::prepare_forced_retained_surface_stamp_for_test(
        &baseline,
        &FrameGraph::new(),
        &ctx,
    )
    .unwrap();

    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let changed = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .unwrap();
    let changed_stamp = super::super::super::prepare_forced_retained_surface_stamp_for_test(
        &changed,
        &FrameGraph::new(),
        &ctx,
    )
    .unwrap();
    let baseline_dependency = child_dependency(&baseline_stamp);
    let changed_dependency = child_dependency(&changed_stamp);
    assert_eq!(
        baseline_dependency.child_stamp, changed_dependency.child_stamp,
        "child isolation raster identity excludes its own group opacity"
    );
    assert_ne!(baseline_stamp, changed_stamp);
    let (
        super::super::super::RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
            source_bounds_bits: baseline_bounds,
            opacity_bits: baseline_opacity,
        },
        super::super::super::RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
            source_bounds_bits: changed_bounds,
            opacity_bits: changed_opacity,
        },
    ) = (
        &baseline_dependency.child_composite_geometry,
        &changed_dependency.child_composite_geometry,
    )
    else {
        panic!("mixed parent dependency uses the dedicated nested-isolation stamp")
    };
    assert_eq!(baseline_bounds, changed_bounds);
    assert_eq!(*baseline_opacity, 0.5_f32.to_bits());
    assert_eq!(*changed_opacity, 0.25_f32.to_bits());

    let mut tampered_parent = baseline_stamp.clone();
    let super::super::super::RetainedSurfaceRasterStepStamp::NestedSurface(tampered_dependency) =
        &mut tampered_parent.ordered_steps[1]
    else {
        panic!("mixed dependency")
    };
    tampered_dependency.child_stamp.identity.role =
        super::super::super::RetainedSurfaceRasterRole::RootIsolation;
    let tampered_child = tampered_dependency.child_stamp.as_ref().clone();
    assert!(!super::super::super::retained_surface_raster_stamp_is_canonical(
        &tampered_child
    ));
    assert!(!super::super::super::retained_surface_raster_stamp_is_canonical(
        &tampered_parent
    ));
    let mut viewport = Viewport::new();
    assert!(!viewport.stage_retained_surface_full_set([tampered_parent, tampered_child,]));
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );

    let mut tampered_geometry_parent = baseline_stamp.clone();
    let super::super::super::RetainedSurfaceRasterStepStamp::NestedSurface(
        tampered_geometry_dependency,
    ) = &mut tampered_geometry_parent.ordered_steps[1]
    else {
        panic!("mixed dependency")
    };
    let super::super::super::RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
        source_bounds_bits,
        ..
    } = &mut tampered_geometry_dependency.child_composite_geometry
    else {
        panic!("nested-isolation geometry")
    };
    source_bounds_bits[0] = (f32::from_bits(source_bounds_bits[0]) + 1.0).to_bits();
    let tampered_geometry_child = tampered_geometry_dependency.child_stamp.as_ref().clone();
    let mut viewport = Viewport::new();
    assert!(
        !viewport.stage_retained_surface_full_set([
            tampered_geometry_parent,
            tampered_geometry_child,
        ])
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );

    let mut duplicate_parent = baseline_stamp.clone();
    let mut duplicate_dependency = duplicate_parent.ordered_steps[1].clone();
    let super::super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) =
        &mut duplicate_dependency
    else {
        panic!("mixed dependency")
    };
    let duplicate_child = dependency.child_stamp.as_ref().clone();
    dependency.step_index = duplicate_parent.ordered_steps.len();
    dependency.parent_opaque_order_before = duplicate_parent.opaque_order_span.end;
    dependency.parent_opaque_order_after = duplicate_parent.opaque_order_span.end;
    duplicate_parent.ordered_steps.push(duplicate_dependency);
    assert!(
        super::super::super::retained_surface_raster_stamp_is_canonical(&duplicate_parent),
        "duplicate dependency remains a canonical parent stamp in isolation"
    );
    let mut viewport = Viewport::new();
    assert!(
        !viewport.stage_retained_surface_full_set([duplicate_parent, duplicate_child]),
        "one child member referenced twice cannot form a canonical full-set tree"
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );
}

#[test]
fn mixed_effect_tree_forced_reuse_matrix_keeps_opacity_composite_only() {
    // Opacity-only: child raster U, parent dependency R.
    {
        let (arena, root, _, child, _, _, mut properties, mut generations) =
            exact_transform_child_isolation_fixture();
        let baseline = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap();
        let mut viewport = Viewport::new();
        assert_eq!(
            execute_forced_plan_graph(&mut viewport, &baseline)
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            2
        );
        viewport.finish_retained_surface_transaction(true);

        crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let changed = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap();
        let graph = execute_forced_plan_graph(&mut viewport, &changed);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            1,
            "opacity-only rerasterizes parent dependency, not child raster"
        );
        assert_eq!(graph.test_rect_pass_snapshots().len(), 3);
        let composites = graph.test_graphics_passes::<
            crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
        >();
        assert_eq!(composites.len(), 1);
        assert_eq!(
            composites[0].test_params().opacity.to_bits(),
            0.25_f32.to_bits()
        );
        viewport.finish_retained_surface_transaction(false);
    }

    // Root transform-only: both rasters U, final matrix updates.
    {
        let (arena, root, _, _, _, _, mut properties, mut generations) =
            exact_transform_child_isolation_fixture();
        let baseline = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap();
        let mut viewport = Viewport::new();
        let baseline_graph = execute_forced_plan_graph(&mut viewport, &baseline);
        let baseline_final = baseline_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()[0]
            .test_snapshot();
        viewport.finish_retained_surface_transaction(true);
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(101.0, 0.0, 0.0),
            )));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let changed = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap();
        let graph = execute_forced_plan_graph(&mut viewport, &changed);
        assert!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .is_empty()
        );
        assert!(graph.test_rect_pass_snapshots().is_empty());
        assert!(
            graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .is_empty()
        );
        let finals =
            graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
        assert_eq!(finals.len(), 1);
        assert_ne!(finals[0].test_snapshot(), baseline_final);
        viewport.finish_retained_surface_transaction(false);
    }

    // Child paint: child R and parent dependency R.
    {
        let (arena, root, _, child, _, _, mut properties, mut generations) =
            exact_transform_child_isolation_fixture();
        let baseline = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap();
        let mut viewport = Viewport::new();
        execute_forced_plan_graph(&mut viewport, &baseline);
        viewport.finish_retained_surface_transaction(true);
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_background_color_value(Color::rgb(12, 220, 44));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let changed = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap();
        let graph = execute_forced_plan_graph(&mut viewport, &changed);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            2
        );
        assert_eq!(graph.test_rect_pass_snapshots().len(), 5);
        viewport.finish_retained_surface_transaction(false);
    }

    // Parent-only paint: parent R, child U.
    {
        let (arena, root, _, _, _, _, mut properties, mut generations) =
            exact_transform_child_isolation_fixture();
        let baseline = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap();
        let mut viewport = Viewport::new();
        execute_forced_plan_graph(&mut viewport, &baseline);
        viewport.finish_retained_surface_transaction(true);
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_background_color_value(Color::rgb(220, 12, 44));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let changed = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &properties,
            &generations,
        )
        .unwrap();
        let graph = execute_forced_plan_graph(&mut viewport, &changed);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            1
        );
        assert_eq!(graph.test_rect_pass_snapshots().len(), 3);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .len(),
            1
        );
        viewport.finish_retained_surface_transaction(false);
    }
}

#[test]
fn mixed_root_isolation_and_transform_tree_executors_reject_cross_shape_atomically() {
    let (arena, root, _, _, _, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    let mixed = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .unwrap();
    let (arena, transform_root, _, _, _, _, properties, generations) =
        nested_exact_transform_fixture();
    let transform_tree = plan_single_root_transform_surface(
        &arena,
        &[transform_root],
        &properties,
        &generations,
    )
    .unwrap();
    let (arena, isolation_root, properties, generations) = exact_isolation_fixture(0.5);
    let root_isolation = plan_single_root_isolation_surface(
        &arena,
        &[isolation_root],
        &properties,
        &generations,
        160,
        120,
        1.0,
        None,
    )
    .unwrap();
    let mut viewport = Viewport::new();

    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    let graph_before = graph.build_state_snapshot_for_test();
    let transaction_before = viewport.retained_surface_transaction_shape_for_test();
    let error = match super::super::super::build_retained_surface_tree_from_pool(
        &mut viewport,
        &mixed,
        &mut graph,
        ctx,
    ) {
        Ok(_) => panic!("T->T executor cannot accept mixed effect tree"),
        Err(error) => error,
    };
    assert_eq!(error, super::super::super::ForcedTransformSurfaceError::PlanShape);
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        transaction_before
    );

    let mut graph = FrameGraph::new();
    let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
    let graph_before = graph.build_state_snapshot_for_test();
    let error = match super::super::super::build_retained_isolation_surface_from_pool(
        &mut viewport,
        &mixed,
        &mut graph,
        ctx,
    ) {
        Ok(_) => panic!("root isolation executor cannot accept mixed effect tree"),
        Err(error) => error,
    };
    assert_eq!(error, super::super::super::ForcedTransformSurfaceError::PlanShape);
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        transaction_before
    );

    for (label, plan) in [
        ("T->T", &transform_tree),
        ("root isolation", &root_isolation),
    ] {
        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let graph_before = graph.build_state_snapshot_for_test();
        let error = match super::super::super::build_retained_effect_tree_from_pool(
            &mut viewport,
            plan,
            &mut graph,
            ctx,
        ) {
            Ok(_) => panic!("mixed executor cannot accept {label}"),
            Err(error) => error,
        };
        assert_eq!(error, super::super::super::ForcedTransformSurfaceError::PlanShape);
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            transaction_before
        );
    }
}
