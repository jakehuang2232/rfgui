use super::*;

#[test]
fn compile_captures_graphics_pass_descriptor() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    graph.add_graphics_pass(WritePass { output: texture });

    graph.compile().expect("compile should succeed");

    let descriptors = graph.pass_descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].kind, PassKind::Graphics);
    let PassDetails::Graphics(graphics) = &descriptors[0].details else {
        panic!("expected graphics pass details");
    };
    assert_eq!(graphics.color_attachments.len(), 1);
    assert_eq!(
        graphics.color_attachments[0].load_op,
        AttachmentLoadOp::Clear
    );
}

#[test]
fn strict_test_snapshot_rejects_unknown_live_pass_payload() {
    let mut graph = FrameGraph::new();
    graph.add_graphics_pass(SurfacePass);

    let error = graph
        .test_compile_snapshot()
        .expect_err("unknown live pass payload must fail closed");
    let FrameGraphError::Validation(message) = error else {
        panic!("unexpected strict snapshot error: {error:?}");
    };
    assert!(message.contains("strict test snapshot has no payload adapter"));
    assert!(message.contains("SurfacePass"));
}

#[test]
fn graphics_execution_witness_fails_closed_and_stops_group_progress() {
    let mut viewport = Viewport::new();
    let texture_allocations = FxHashMap::default();
    let texture_keys = FxHashMap::default();
    let buffer_allocations = FxHashMap::default();
    let mut record = RecordContext::new(
        &mut viewport,
        &[],
        &[],
        &texture_allocations,
        &texture_keys,
        &buffer_allocations,
    );
    {
        let mut graphics = GraphicsRecordContext::new(&mut record);
        graphics.mark_execution_failed();
        assert!(!graphics_group_can_continue(graphics.execution_failed()));
    }
    assert!(matches!(
        execution_witness_result(record.execution_failed, "graphics group", "fixture"),
        Err(FrameGraphError::Execution(message))
            if message.contains("missing required resource")
    ));

    let mut visited = 0;
    for _ in 0..2 {
        visited += 1;
        if !graphics_group_can_continue(record.execution_failed) {
            break;
        }
    }
    assert_eq!(visited, 1, "a failed group must not execute its next pass");
}

#[test]
fn compile_allows_opaque_rect_to_modify_same_render_target() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    graph.add_graphics_pass(OpaqueRectPass::from_draw_rect_pass(DrawRectPass::new(
        RectPassParams::default(),
        DrawRectInput {
            render_target: RenderTargetIn::with_handle(
                texture
                    .handle()
                    .expect("declared texture should have handle"),
            ),
            ..Default::default()
        },
        DrawRectOutput {
            render_target: RenderTargetOut::with_handle(
                texture
                    .handle()
                    .expect("declared texture should have handle"),
            ),
        },
    )));

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    let timeline = compiled
        .resource_timelines
        .iter()
        .find(|timeline| timeline.resource == ResourceHandle::Texture(texture.handle().unwrap()))
        .expect("resource timeline should exist");
    assert_eq!(timeline.transitions.len(), 2);
    assert_eq!(
        timeline.transitions[1].after,
        ResourceState::Texture(TextureResourceState::ColorAttachment)
    );
}

#[test]
fn compile_groups_inline_graphics_passes_from_descriptor_compatibility() {
    let mut graph = FrameGraph::new();
    let texture = graph.declare_texture::<()>(test_texture_desc());
    let writer = graph.add_graphics_pass(WritePass {
        output: texture.clone(),
    });
    let inline_a = graph.add_graphics_pass(InlineLoadPass {
        target: texture.clone(),
    });
    let inline_b = graph.add_graphics_pass(InlineLoadPass {
        target: texture.clone(),
    });
    let present = graph.add_graphics_pass(make_present_pass(&texture));

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(
        compiled.execution_plan.ordered_passes,
        vec![writer.0, inline_a.0, inline_b.0, present.0]
    );
    assert!(matches!(
        &compiled.execution_plan.steps[..],
        [
            CompiledExecuteStep::GraphicsPass { pass_index },
            CompiledExecuteStep::GraphicsPassGroup(RenderPassGroup { pass_indices, .. }),
            CompiledExecuteStep::GraphicsPass { pass_index: present_index },
        ] if *pass_index == writer.0
            && *present_index == present.0
            && pass_indices == &vec![inline_a.0, inline_b.0]
    ));
}

#[test]
fn compile_prefers_longer_graphics_run_over_lower_index_compute() {
    let mut graph = FrameGraph::new();
    let compute = graph.add_compute_pass(ComputeStubPass);
    let surface_a = graph.add_graphics_pass(MergeableSurfacePass);
    let surface_b = graph.add_graphics_pass(MergeableSurfacePass);

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(
        compiled.execution_plan.ordered_passes,
        vec![surface_a.0, surface_b.0, compute.0]
    );
    assert!(matches!(
        &compiled.execution_plan.steps[..],
        [
            CompiledExecuteStep::GraphicsPassGroup(RenderPassGroup { pass_indices, .. }),
            CompiledExecuteStep::ComputePass { pass_index },
        ] if pass_indices == &vec![surface_a.0, surface_b.0] && *pass_index == compute.0
    ));
}

#[test]
fn compile_finishes_parallel_prep_before_final_target_batch() {
    let mut graph = FrameGraph::new();
    let prep_tex_a = graph.declare_texture::<()>(test_texture_desc());
    let prep_tex_b = graph.declare_texture::<()>(test_texture_desc());
    let prep_a = graph.add_graphics_pass(MergeablePrepPass {
        output: prep_tex_a.clone(),
    });
    let prep_b = graph.add_graphics_pass(MergeablePrepPass {
        output: prep_tex_b.clone(),
    });
    let final_a = graph.add_graphics_pass(MergeableFinalReadPass {
        input: InSlot::with_handle(prep_tex_a.handle().expect("texture a handle")),
    });
    let final_b = graph.add_graphics_pass(MergeableFinalReadPass {
        input: InSlot::with_handle(prep_tex_b.handle().expect("texture b handle")),
    });

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(
        compiled.execution_plan.ordered_passes,
        vec![prep_a.0, prep_b.0, final_a.0, final_b.0]
    );
    assert!(matches!(
        &compiled.execution_plan.steps[..],
        [
            CompiledExecuteStep::GraphicsPass { pass_index: prep_first },
            CompiledExecuteStep::GraphicsPass { pass_index: prep_second },
            CompiledExecuteStep::GraphicsPassGroup(RenderPassGroup { pass_indices, .. }),
        ] if *prep_first == prep_a.0
            && *prep_second == prep_b.0
            && pass_indices == &vec![final_a.0, final_b.0]
    ));
}

#[test]
fn compile_emits_execution_step_shapes_for_compute_and_transfer() {
    let mut graph = FrameGraph::new();
    let compute = graph.add_compute_pass(ComputeStubPass);
    let transfer = graph.add_transfer_pass(TransferStubPass);

    graph.compile().expect("compile should succeed");

    let compiled = graph.compiled_graph().expect("compiled graph should exist");
    assert_eq!(
        compiled.execution_plan.ordered_passes,
        vec![compute.0, transfer.0]
    );
    assert_eq!(
        compiled.execution_plan.steps,
        vec![
            CompiledExecuteStep::ComputePass {
                pass_index: compute.0
            },
            CompiledExecuteStep::TransferPass {
                pass_index: transfer.0
            },
        ]
    );
}
