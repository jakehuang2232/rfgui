use super::*;
use crate::view::render_pass::draw_rect_pass::RectStencilModeTestSnapshot;

#[test]
fn depth_four_execution_declares_one_pair_and_one_raster_per_surface() {
    let prepared = prepared_depth_four_surface_frame();
    assert_eq!(prepared.raster_plan().nodes().len(), 4);
    let mut viewport = Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("depth-four owner");
    let mut graph = FrameGraph::new();
    let (_, actions) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        owner,
        prepared,
        &mut graph,
        execution_context(),
    )
    .expect("depth-four emission");

    assert_eq!(actions.len(), 4);
    assert_eq!(graph.declared_persistent_texture_keys().count(), 8);
    assert_eq!(graph.test_graphics_passes::<ClearPass>().len(), 4);
    assert_eq!(graph.test_graphics_passes::<CompositeLayerPass>().len(), 4);
}

#[test]
fn sealed_shadow_prefix_and_empty_suffix_drive_emission_without_rederivation() {
    let mut visible_viewport = Viewport::new();
    let visible_owner = visible_viewport
        .begin_retained_surface_frame_stage()
        .expect("visible owner");
    let mut visible_graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut visible_viewport,
        visible_owner,
        prepared_self_clip_shadow_surface_frame(false),
        &mut visible_graph,
        execution_context(),
    )
    .expect("visible suffix emission");

    let mut empty_viewport = Viewport::new();
    let empty_owner = empty_viewport
        .begin_retained_surface_frame_stage()
        .expect("empty owner");
    let mut empty_graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut empty_viewport,
        empty_owner,
        prepared_self_clip_shadow_surface_frame(true),
        &mut empty_graph,
        execution_context(),
    )
    .expect("empty suffix emission");

    assert!(visible_graph.pass_descriptors().len() > empty_graph.pass_descriptors().len());
    assert!(
        !empty_graph.pass_descriptors().is_empty(),
        "the incoming-scissor shadow prefix must still emit"
    );

    let expected_scissor = [7, 11, 53, 47];
    let mut scissor_viewport = Viewport::new();
    let scissor_owner = scissor_viewport
        .begin_retained_surface_frame_stage()
        .expect("scissor owner");
    let mut scissor_graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut scissor_viewport,
        scissor_owner,
        prepared_whole_chunk_clip_surface_frame(ArtifactSurfaceResolvedClip::Scissor(
            GraphicsPassScissor::Logical(expected_scissor),
        )),
        &mut scissor_graph,
        execution_context(),
    )
    .expect("whole-chunk scissor emission");
    let visible_rects = scissor_graph
        .test_rect_pass_snapshots()
        .into_iter()
        .filter(|pass| pass.color_write_enabled)
        .collect::<Vec<_>>();
    assert!(!visible_rects.is_empty(), "scissor fixture must emit paint");
    assert!(
        visible_rects
            .iter()
            .all(|pass| pass.effective_scissor_rect == Some(expected_scissor)),
        "WholeChunk(Scissor) must apply its sealed scissor to every emitted paint op"
    );

    let mut whole_empty_viewport = Viewport::new();
    let whole_empty_owner = whole_empty_viewport
        .begin_retained_surface_frame_stage()
        .expect("whole-empty owner");
    let mut whole_empty_graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut whole_empty_viewport,
        whole_empty_owner,
        prepared_whole_chunk_clip_surface_frame(ArtifactSurfaceResolvedClip::Empty),
        &mut whole_empty_graph,
        execution_context(),
    )
    .expect("whole-chunk empty emission");
    assert_eq!(
        whole_empty_graph
            .test_rect_pass_snapshots()
            .into_iter()
            .filter(|pass| pass.color_write_enabled)
            .count(),
        0,
        "WholeChunk(Empty) must emit zero paint ops"
    );
}

#[test]
fn sealed_terminal_clip_replaces_a_disjoint_incoming_scissor() {
    let sealed_scissor = [30, 8, 20, 16];
    let mut ctx = execution_context();
    ctx.replace_scissor_rect(Some([0, 0, 16, 600]));

    let mut viewport = Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("terminal-clip owner");
    let mut graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        owner,
        prepared_whole_chunk_clip_surface_frame(ArtifactSurfaceResolvedClip::Scissor(
            GraphicsPassScissor::Logical(sealed_scissor),
        )),
        &mut graph,
        ctx,
    )
    .expect("terminal-clip emission");

    let visible_rects = graph
        .test_rect_pass_snapshots()
        .into_iter()
        .filter(|pass| pass.color_write_enabled)
        .collect::<Vec<_>>();
    assert!(
        !visible_rects.is_empty(),
        "terminal-clip fixture must emit paint"
    );
    assert!(
        visible_rects
            .iter()
            .all(|pass| pass.effective_scissor_rect == Some(sealed_scissor)),
        "the sealed terminal clip already contains incoming-clip polarity and must replace, not re-intersect, the active scissor"
    );
}

#[test]
fn child_mask_preflight_rejects_overflow_then_emits_sealed_push_and_pop() {
    let mut overflow_ctx = execution_context();
    for _ in 0..u8::MAX {
        assert!(overflow_ctx.push_clip_id().is_some());
    }
    let mut overflow_viewport = Viewport::new();
    let overflow_owner = overflow_viewport
        .begin_retained_surface_frame_stage()
        .expect("overflow owner");
    let mut overflow_graph = FrameGraph::new();
    let before = overflow_graph.build_state_snapshot_for_test();
    let Err(error) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut overflow_viewport,
        overflow_owner,
        prepared_child_mask_surface_frame(),
        &mut overflow_graph,
        overflow_ctx,
    ) else {
        panic!("overflowing child-mask target must reject")
    };
    assert!(matches!(
        error,
        ArtifactSurfaceExecutionError::ChildMaskDepthOverflow {
            incoming_depth: u8::MAX,
            max_mask_depth: 1,
            ..
        }
    ));
    assert_eq!(execution_error_name(error), "child-mask-depth-overflow");
    assert_eq!(overflow_graph.build_state_snapshot_for_test(), before);

    let mut viewport = Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("mask owner");
    let mut graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        owner,
        prepared_child_mask_surface_frame(),
        &mut graph,
        execution_context(),
    )
    .expect("child-mask emission");
    let snapshots = graph.test_rect_pass_snapshots();
    assert!(snapshots.iter().any(|pass| matches!(
        pass.stencil_mode,
        RectStencilModeTestSnapshot::Increment { .. }
    )));
    assert!(snapshots.iter().any(|pass| matches!(
        pass.stencil_mode,
        RectStencilModeTestSnapshot::Decrement { .. }
    )));
}
