use super::*;

#[test]
fn native_scroll_forest_raster_identity_invalidates_only_the_ancestor_chain() {
    fn identities(
        plan: &FramePaintPlan,
    ) -> Vec<NativeScrollForestContentRasterProgramIdentity> {
        let forest = plan.native_scroll_forest_planning_scaffold().unwrap();
        forest
            .programs
            .iter()
            .zip(&forest.boundaries)
            .map(|(program, boundary)| {
                program.content_raster_identity(boundary.admission.content_root)
            })
            .collect()
    }
    let (arena, roots, mut properties, mut generations) = native_scroll_forest_plan_fixture();
    let initial = plan_native_scroll_forest_scaffold_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        TransformSurfacePlanContext::default(),
    )
    .unwrap();
    let baseline = identities(&initial);
    let leaf_boundary = initial
        .native_scroll_forest_planning_scaffold()
        .unwrap()
        .boundaries[2]
        .clone();
    let mut leaf = leaf_boundary.admission.content_root;
    while let Some(child) = arena.children_of(leaf).first().copied() {
        leaf = child;
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
        .set_background_color_value(Color::rgb(231, 76, 60));
    for root in &roots {
        arena.refresh_subtree_dirty_cache(*root);
    }
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    let paint_changed = identities(
        &plan_native_scroll_forest_scaffold_with_context(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::default(),
        )
        .unwrap(),
    );
    assert_ne!(paint_changed[2], baseline[2]);
    assert_ne!(paint_changed[1], baseline[1]);
    assert_ne!(paint_changed[0], baseline[0]);
    assert_eq!(paint_changed[3], baseline[3]);
    assert_eq!(paint_changed[4], baseline[4]);
    assert_eq!(paint_changed[5], baseline[5]);

    let child_content = baseline[2].clone();
    let mut parent_content = baseline[1].clone();
    let child_edge = parent_content
        .child_dependencies
        .iter_mut()
        .find(|dependency| dependency.child == NativeScrollBoundaryId(2))
        .unwrap();
    child_edge.scroll.offset.x += 1.0;
    child_edge.offset_bits[0] = child_edge.scroll.offset.x.to_bits();
    assert_eq!(child_content, baseline[2], "child C remains reusable");
    assert_ne!(parent_content, baseline[1], "parent C sees child composite");
    let mut ancestor_content = baseline[0].clone();
    ancestor_content.child_dependencies[0].child_raster_identity =
        Box::new(parent_content.clone());
    assert_ne!(ancestor_content, baseline[0], "ancestor chain propagates");
    assert_eq!(
        baseline[3],
        identities(&initial)[3],
        "sibling C is isolated"
    );
}

#[test]
fn native_scroll_forest_joint_pool_transaction_is_cold_warm_and_postorder_atomic() {
    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let plan = plan_native_scroll_forest_scaffold_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        TransformSurfacePlanContext::default(),
    )
    .unwrap();
    let mut viewport = Viewport::new();
    let cold = super::super::super::scroll_scene::prepare_native_scroll_forest_transaction_from_pool(
        &viewport,
        &plan,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .expect("cold native forest transaction");
    assert!(cold.transaction_is_canonical_for_test());
    assert_eq!(cold.stamps_for_test().len(), 6);
    assert_eq!(cold.actions_for_test().len(), 6);
    let program_shapes = cold.program_shapes_for_test();
    assert_eq!(program_shapes.len(), 6);
    assert_eq!(program_shapes[1][2], 2);
    let cold_trace = cold.emission_trace_for_test();
    let position = |needle| cold_trace.iter().position(|step| *step == needle).unwrap();
    use super::super::super::scroll_scene::NativeScrollForestEmissionTraceStep as Trace;
    assert!(
        position(Trace::Host(NativeScrollBoundaryId(1)))
            < position(Trace::BeginContentRaster(NativeScrollBoundaryId(1)))
    );
    assert!(
        position(Trace::BeginContentRaster(NativeScrollBoundaryId(1)))
            < position(Trace::Host(NativeScrollBoundaryId(2)))
    );
    assert!(
        position(Trace::Overlay(NativeScrollBoundaryId(2)))
            < position(Trace::Host(NativeScrollBoundaryId(3)))
    );
    assert!(
        position(Trace::Overlay(NativeScrollBoundaryId(3)))
            < position(Trace::CompositeContent(NativeScrollBoundaryId(1)))
    );
    assert!(
        position(Trace::CompositeContent(NativeScrollBoundaryId(1)))
            < position(Trace::Overlay(NativeScrollBoundaryId(1)))
    );
    let empty_graph = FrameGraph::new().build_state_snapshot_for_test();
    let mut cold_graph = FrameGraph::new();
    let mut cold_ctx = UiBuildContext::new(700, 700, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let caller_root_target = cold_ctx.allocate_target(&mut cold_graph);
    cold_ctx.set_current_target(caller_root_target);
    cold_graph.add_graphics_pass(crate::view::render_pass::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: cold_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: caller_root_target,
        },
    ));
    assert!(
        cold.actions_for_test()
            .values()
            .all(|action| *action == RetainedSurfaceCompileAction::Reraster)
    );
    let keys = cold
        .stamps_for_test()
        .iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<Vec<_>>();
    let color_keys = cold
        .stamps_for_test()
        .iter()
        .map(|stamp| stamp.identity.color_key)
        .collect::<Vec<_>>();
    let cold_state = super::super::super::scroll_scene::emit_prepared_native_scroll_forest_transaction(
        &mut viewport,
        &mut cold_graph,
        cold_ctx,
        cold,
    );
    assert_ne!(cold_graph.build_state_snapshot_for_test(), empty_graph);
    assert_eq!(
        cold_graph.declared_persistent_texture_keys().count(),
        12,
        "cold forest declares every boundary C color/depth pair"
    );
    let clears = cold_graph.test_graphics_passes::<crate::view::render_pass::ClearPass>();
    assert_eq!(clears.len(), 7);
    assert_eq!(
        clears
            .iter()
            .filter(|clear| {
                clear.test_snapshot().output_target != caller_root_target.handle()
            })
            .count(),
        6,
        "cold forest clears every boundary C exactly once; caller owns the frame clear"
    );
    assert_eq!(
        cold_graph
            .test_graphics_passes::<
                crate::view::render_pass::texture_composite_pass::TextureCompositePass,
            >()
            .len(),
        6,
        "cold forest composites three child C targets and three root C targets"
    );
    let root_target = cold_state
        .current_target()
        .expect("native forest emits into one frame target");
    assert_eq!(root_target.handle(), caller_root_target.handle());
    let present = cold_graph.add_graphics_pass(
        crate::view::render_pass::present_surface_pass::PresentSurfacePass::new(
            crate::view::render_pass::present_surface_pass::PresentSurfaceParams,
            crate::view::render_pass::present_surface_pass::PresentSurfaceInput {
                source: crate::view::render_pass::draw_rect_pass::RenderTargetIn::with_handle(
                    root_target
                        .handle()
                        .expect("native forest frame target has a handle"),
                ),
            },
            crate::view::render_pass::present_surface_pass::PresentSurfaceOutput,
        ),
    );
    cold_graph
        .add_pass_sink(
            present,
            crate::view::frame_graph::ExternalSinkKind::SurfacePresent,
        )
        .unwrap();
    let cold_snapshot = cold_graph.test_compile_snapshot().unwrap();
    let payloads = cold_snapshot.pass_payloads();
    let composites = payloads
        .iter()
        .enumerate()
        .filter_map(|(index, payload)| match payload {
            FramePassTestPayload::TextureComposite(pass) => Some((index, pass)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(composites.len(), 6);
    for &(composite_index, composite) in &composites {
        let active_clip = composite
            .pass_context
            .stencil_clip_id
            .expect("every native boundary C composite is inside its H/O mask");
        let increment_clip = active_clip - 1;
        assert!(
            payloads[..composite_index].iter().rev().any(|payload| {
                matches!(
                    payload,
                    FramePassTestPayload::DrawRect(rect)
                        if rect.output_target == composite.output_target
                            && matches!(
                                rect.stencil_mode,
                                crate::view::render_pass::draw_rect_pass::RectStencilModeTestSnapshot::Increment {
                                    clip_id
                                } if clip_id == increment_clip
                            )
                )
            }),
            "same-target Increment precedes each native C composite"
        );
        assert!(
            payloads[composite_index + 1..].iter().any(|payload| {
                matches!(
                    payload,
                    FramePassTestPayload::DrawRect(rect)
                        if rect.output_target == composite.output_target
                            && matches!(
                                rect.stencil_mode,
                                crate::view::render_pass::draw_rect_pass::RectStencilModeTestSnapshot::Decrement {
                                    clip_id
                                } if clip_id == active_clip
                            )
                )
            }),
            "same-target Decrement follows each native C composite"
        );
    }
    let rounded_parent_composite = composites
        .iter()
        .find(|(_, candidate)| {
            candidate.source_handle.is_some()
                && composites
                    .iter()
                    .filter(|(_, child)| child.output_target == candidate.source_handle)
                    .count()
                    == 2
        })
        .expect("rounded parent C owns two child C composites");
    let rounded_content_target = rounded_parent_composite
        .1
        .source_handle
        .expect("rounded parent composite samples its persistent C");
    let nested_composites = payloads
        .iter()
        .enumerate()
        .filter_map(|(index, payload)| match payload {
            FramePassTestPayload::TextureComposite(pass)
                if pass.output_target == Some(rounded_content_target) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(nested_composites.len(), 2);
    let between = payloads
        .iter()
        .enumerate()
        .position(|(_, payload)| {
            matches!(
                payload,
                FramePassTestPayload::DrawRect(rect)
                    if rect.output_target == Some(rounded_content_target)
                        && rect.size_bits
                            == [20.0_f32.to_bits(), 20.0_f32.to_bits()]
                        && rect.color_write_enabled
            )
        })
        .expect("between-siblings artifact stays in rounded parent C");
    assert!(
        nested_composites[0] < between && between < nested_composites[1],
        "parent C preserves child C, between artifact, sibling C order"
    );
    viewport.finish_retained_surface_transaction(true);

    let warm = super::super::super::scroll_scene::prepare_native_scroll_forest_transaction_with_forced_pool_for_test(
        &viewport,
        &plan,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .expect("warm native forest transaction");
    assert_eq!(
        keys.iter()
            .map(|key| warm.actions_for_test()[key])
            .collect::<Vec<_>>(),
        vec![RetainedSurfaceCompileAction::Reuse; 6]
    );
    let mut warm_graph = FrameGraph::new();
    let warm_ctx = UiBuildContext::new(700, 700, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let _warm_state =
        super::super::super::scroll_scene::emit_prepared_native_scroll_forest_transaction(
            &mut viewport,
            &mut warm_graph,
            warm_ctx,
            warm,
        );
    assert_eq!(
        warm_graph.declared_persistent_texture_keys().count(),
        6,
        "warm roots reuse their baked descendants and declare only root C pairs"
    );
    assert_eq!(
        warm_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        0,
        "warm reuse emits no C clear"
    );
    assert_eq!(
        warm_graph
            .test_graphics_passes::<
                crate::view::render_pass::texture_composite_pass::TextureCompositePass,
            >()
            .len(),
        3,
        "warm reuse composites only the three root C targets"
    );
    viewport.finish_retained_surface_transaction(true);

    let mut tampered = plan.clone();
    let scaffold = tampered
        .property_scene_seal
        .as_mut()
        .unwrap()
        .native_scroll_forest_scaffold
        .as_mut()
        .unwrap();
    scaffold.programs[1].child_dependencies[0].composite_scissor[0] += 1;
    scaffold.planned_programs = scaffold.programs.clone();
    assert!(
        super::super::super::scroll_scene::prepare_native_scroll_forest_transaction_from_pool(
            &viewport,
            &tampered,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )
        .is_err()
    );
    let after_rejection = super::super::super::scroll_scene::prepare_native_scroll_forest_transaction_with_forced_pool_for_test(
        &viewport,
        &plan,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .unwrap();
    assert!(
        after_rejection
            .actions_for_test()
            .values()
            .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
    );

    viewport.forget_retained_surface_pair_witness_for_test(color_keys[2]);
    let mixed = super::super::super::scroll_scene::prepare_native_scroll_forest_transaction_with_forced_pool_for_test(
        &viewport,
        &plan,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )
    .expect("mixed native forest transaction");
    let actions = mixed.actions_for_test();
    assert_eq!(actions[&keys[2]], RetainedSurfaceCompileAction::Reraster);
    assert_eq!(actions[&keys[1]], RetainedSurfaceCompileAction::Reraster);
    assert_eq!(actions[&keys[0]], RetainedSurfaceCompileAction::Reraster);
    assert_eq!(actions[&keys[3]], RetainedSurfaceCompileAction::Reuse);
    assert_eq!(actions[&keys[4]], RetainedSurfaceCompileAction::Reuse);
    assert_eq!(actions[&keys[5]], RetainedSurfaceCompileAction::Reuse);
}

#[test]
fn native_scroll_forest_fresh_offset_reuses_child_c_and_rerasterizes_only_ancestors() {
    fn root_cleared_context(graph: &mut FrameGraph) -> UiBuildContext {
        let mut ctx = UiBuildContext::new(700, 700, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        graph.add_graphics_pass(crate::view::render_pass::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target,
            },
        ));
        ctx
    }

    let plan = |offset_x| {
        let (arena, roots, properties, generations) =
            native_scroll_forest_plan_fixture_with_s2_offset(offset_x);
        plan_native_scroll_forest_scaffold_with_context(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::default(),
        )
        .unwrap()
    };
    let baseline_plan = plan(30.0);
    let moved_plan = plan(31.0);
    let mut viewport = Viewport::new();
    let baseline =
        super::super::super::scroll_scene::prepare_native_scroll_forest_transaction_from_pool(
            &viewport,
            &baseline_plan,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )
        .unwrap();
    let baseline_stamps = baseline.stamps_for_test().to_vec();
    let keys = baseline_stamps
        .iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<Vec<_>>();
    let mut baseline_graph = FrameGraph::new();
    let baseline_ctx = root_cleared_context(&mut baseline_graph);
    let _baseline_state =
        super::super::super::scroll_scene::emit_prepared_native_scroll_forest_transaction(
            &mut viewport,
            &mut baseline_graph,
            baseline_ctx,
            baseline,
        );
    viewport.finish_retained_surface_transaction(true);

    let moved =
        super::super::super::scroll_scene::prepare_native_scroll_forest_transaction_with_forced_pool_for_test(
            &viewport,
            &moved_plan,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )
        .unwrap();
    let moved_stamps = moved.stamps_for_test();
    assert_eq!(
        moved_stamps[2], baseline_stamps[2],
        "fresh legal offset keeps the changed boundary's offset-zero C identity reusable"
    );
    assert_ne!(
        moved_stamps[1], baseline_stamps[1],
        "parent C seals the changed child composite edge"
    );
    assert_ne!(
        moved_stamps[0], baseline_stamps[0],
        "ancestor C recursively seals the changed descendant edge"
    );
    for index in [3, 4, 5] {
        assert_eq!(
            moved_stamps[index], baseline_stamps[index],
            "unrelated sibling/root C identity stays stable"
        );
    }
    let actions = moved.actions_for_test();
    assert_eq!(actions[&keys[0]], RetainedSurfaceCompileAction::Reraster);
    assert_eq!(actions[&keys[1]], RetainedSurfaceCompileAction::Reraster);
    assert_eq!(actions[&keys[2]], RetainedSurfaceCompileAction::Reuse);
    assert_eq!(actions[&keys[3]], RetainedSurfaceCompileAction::Reuse);
    assert_eq!(actions[&keys[4]], RetainedSurfaceCompileAction::Reuse);
    assert_eq!(actions[&keys[5]], RetainedSurfaceCompileAction::Reuse);

    let mut moved_graph = FrameGraph::new();
    let moved_ctx = root_cleared_context(&mut moved_graph);
    let caller_target = moved_ctx.current_target().unwrap().handle();
    let _moved_state =
        super::super::super::scroll_scene::emit_prepared_native_scroll_forest_transaction(
            &mut viewport,
            &mut moved_graph,
            moved_ctx,
            moved,
        );
    assert_eq!(moved_graph.declared_persistent_texture_keys().count(), 12);
    assert_eq!(
        moved_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .iter()
            .filter(|clear| clear.test_snapshot().output_target != caller_target)
            .count(),
        2,
        "only the two changed ancestor C targets rerasterize"
    );
    assert_eq!(
        moved_graph
            .test_graphics_passes::<
                crate::view::render_pass::texture_composite_pass::TextureCompositePass,
            >()
            .len(),
        6,
        "rerasterized ancestor path consumes and composites reusable child C targets"
    );
    viewport.finish_retained_surface_transaction(true);
}
