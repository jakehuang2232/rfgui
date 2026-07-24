use super::*;

#[test]
fn scroll_host_planner_freezes_matching_live_and_property_payloads() {
    let (arena, root, child, properties, generations) = fixture();
    let plan = super::super::super::plan_single_root_scroll_host_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
    )
    .expect("exact scroll fixture must plan");
    let [super::super::super::PaintPlanStep::RetainedSurface(surface)] = plan.steps() else {
        panic!("scroll plan must contain one retained surface");
    };
    let super::super::super::SurfaceKind::ScrollHost(scroll_plan) = surface.kind() else {
        panic!("scroll plan must retain the dedicated surface kind");
    };
    assert_eq!(scroll_plan.admission.child, child);
    assert!(
        scroll_plan
            .admission
            .matches_scroll_node(scroll_plan.scroll)
    );

    let graph = crate::view::frame_graph::FrameGraph::new();
    let ctx = crate::view::base_component::UiBuildContext::new(
        100,
        80,
        wgpu::TextureFormat::Rgba8Unorm,
        1.0,
    );
    let stamp =
        super::super::super::prepare_retained_scroll_host_stamp_for_test(&plan, &graph, &ctx)
            .expect("typed scroll plan must prepare before graph mutation");
    assert_eq!(
        stamp.identity.role,
        super::super::super::RetainedSurfaceRasterRole::ScrollHost
    );
    assert_eq!(stamp.scroll_host.unwrap().scroll, scroll_plan.scroll);

    let mut offset_ctx = crate::view::base_component::UiBuildContext::new(
        100,
        80,
        wgpu::TextureFormat::Rgba8Unorm,
        1.0,
    );
    offset_ctx.set_paint_offset([0.25, 0.0]);
    let untouched_graph = crate::view::frame_graph::FrameGraph::new();
    assert!(
        super::super::super::prepare_retained_scroll_host_stamp_for_test(
            &plan,
            &untouched_graph,
            &offset_ctx,
        )
        .is_err(),
        "prepare must independently reject nonzero incoming paint snap"
    );
}

#[test]
fn scroll_host_planner_rejects_live_property_race_and_opaque_scrollbar() {
    let (arena, root, _child, mut properties, generations) = fixture();
    properties
        .scrolls
        .get_mut(&ScrollNodeId(root))
        .unwrap()
        .offset
        .y += 1.0;
    assert!(
        super::super::super::plan_single_root_scroll_host_surface(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
        )
        .is_err()
    );

    let (arena, root, _child, mut properties, generations) = fixture();
    properties
        .scrolls
        .get_mut(&ScrollNodeId(root))
        .unwrap()
        .scrollbar_overlay
        .paint_state = crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow;
    assert!(
        super::super::super::plan_single_root_scroll_host_surface(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
        )
        .is_err()
    );
}

#[test]
fn scroll_host_planner_accepts_frame_sampled_translucent_scrollbar() {
    let (arena, root, _child, properties, generations) = translucent_fixture();
    assert_eq!(
        properties
            .scroll_snapshot_for(ScrollNodeId(root))
            .unwrap()
            .scrollbar_overlay
            .paint_state,
        crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow
    );
    assert!(
        super::super::super::plan_single_root_scroll_host_surface(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
        )
        .is_ok()
    );
}

#[test]
fn scroll_offset_is_an_explicit_raster_stamp_dependency() {
    let (arena, root, _child, properties, generations) = fixture();
    let plan = super::super::super::plan_single_root_scroll_host_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
    )
    .unwrap();
    let graph = crate::view::frame_graph::FrameGraph::new();
    let ctx = crate::view::base_component::UiBuildContext::new(
        100,
        80,
        wgpu::TextureFormat::Rgba8Unorm,
        1.0,
    );
    let baseline =
        super::super::super::prepare_retained_scroll_host_stamp_for_test(&plan, &graph, &ctx)
            .unwrap();
    let mut offset_only = baseline.clone();
    let dependency = offset_only.scroll_host.as_mut().unwrap();
    dependency.scroll.offset.y += 1.0;
    let (track, thumb) = crate::view::base_component::canonical_vertical_scrollbar_geometry(
        dependency.scroll.viewport,
        dependency.scroll.content_size.height,
        dependency.scroll.offset.y,
        false,
    )
    .unwrap();
    dependency.scroll.scrollbar_overlay.vertical_track = Some(track);
    dependency.scroll.scrollbar_overlay.vertical_thumb = Some(thumb);
    assert_ne!(baseline, offset_only);
    assert!(super::super::super::retained_surface_raster_stamp_is_canonical(&offset_only));
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            baseline.clone(),
            &baseline,
        ),
        super::super::super::RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            baseline,
            &offset_only,
        ),
        super::super::super::RetainedSurfaceCompileAction::Reraster
    );
}

#[test]
fn scroll_host_planner_rejects_non_identity_frame_context() {
    let (arena, root, _child, properties, generations) = fixture();
    let plan = |scale, offset, scissor| {
        super::super::super::plan_single_root_scroll_host_surface(
            &arena,
            &[root],
            &properties,
            &generations,
            scale,
            offset,
            scissor,
        )
    };
    let dpr2 = plan(2.0, [0.0; 2], None)
        .expect("device-aligned scroll-host geometry remains exact at DPR2");
    let [super::super::super::PaintPlanStep::RetainedSurface(surface)] = dpr2.steps() else {
        panic!("DPR2 scroll host must keep the single retained-surface descriptor");
    };
    let super::super::super::SurfaceKind::ScrollHost(scroll_plan) = surface.kind() else {
        panic!("DPR2 scroll host must keep the typed scroll descriptor");
    };
    let device_aligned = |value: f32| {
        let device = value * 2.0;
        device.is_finite() && device.fract().to_bits() == 0.0_f32.to_bits()
    };
    assert!(
        [
            scroll_plan.admission.source_bounds.x,
            scroll_plan.admission.source_bounds.y,
            scroll_plan.admission.source_bounds.x + scroll_plan.admission.source_bounds.width,
            scroll_plan.admission.source_bounds.y + scroll_plan.admission.source_bounds.height,
            scroll_plan.scroll.viewport.x,
            scroll_plan.scroll.viewport.y,
            scroll_plan.scroll.viewport.x + scroll_plan.scroll.viewport.width,
            scroll_plan.scroll.viewport.y + scroll_plan.scroll.viewport.height,
        ]
        .into_iter()
        .all(device_aligned)
    );
    assert!(plan(0.0, [0.0; 2], None).is_err());
    assert!(plan(f32::NAN, [0.0; 2], None).is_err());
    assert!(plan(1.0, [1.0, 0.0], None).is_err());
    assert!(plan(1.0, [0.0; 2], Some([0, 0, 100, 80])).is_err());

    let (unaligned_arena, unaligned_root, _, mut properties, mut generations) = fixture();
    unaligned_arena
        .get_mut(unaligned_root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .layout_state
        .layout_position
        .x += 0.25;
    unaligned_arena.refresh_subtree_dirty_cache(unaligned_root);
    properties.sync(&unaligned_arena, &[unaligned_root]);
    generations.sync(&unaligned_arena, &[unaligned_root], &properties);
    assert!(
        super::super::super::plan_single_root_scroll_host_surface(
            &unaligned_arena,
            &[unaligned_root],
            &properties,
            &generations,
            2.0,
            [0.0; 2],
            None,
        )
        .is_err()
    );
}
