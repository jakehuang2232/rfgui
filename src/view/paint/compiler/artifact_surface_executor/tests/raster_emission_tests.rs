use super::*;
use crate::view::render_pass::draw_rect_pass::RectStencilModeTestSnapshot;

#[test]
fn depth_four_execution_declares_one_retained_color_and_one_raster_per_surface() {
    let prepared = prepared_depth_four_surface_frame();
    assert_eq!(prepared.raster_plan().nodes().len(), 4);
    let mut viewport = Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("depth-four owner");
    let mut graph = FrameGraph::new();
    let (_, actions) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(owner),
        prepared,
        &mut graph,
        execution_context(),
    );

    assert_eq!(actions.len(), 4);
    assert_eq!(graph.declared_persistent_texture_keys().count(), 4);
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
        Some(visible_owner),
        prepared_self_clip_shadow_surface_frame(false),
        &mut visible_graph,
        execution_context(),
    );

    let mut empty_viewport = Viewport::new();
    let empty_owner = empty_viewport
        .begin_retained_surface_frame_stage()
        .expect("empty owner");
    let mut empty_graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut empty_viewport,
        Some(empty_owner),
        prepared_self_clip_shadow_surface_frame(true),
        &mut empty_graph,
        execution_context(),
    );

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
        Some(scissor_owner),
        prepared_whole_chunk_clip_surface_frame(ArtifactSurfaceResolvedClip::Scissor(
            GraphicsPassScissor::Logical(expected_scissor),
        )),
        &mut scissor_graph,
        execution_context(),
    );
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
        Some(whole_empty_owner),
        prepared_whole_chunk_clip_surface_frame(ArtifactSurfaceResolvedClip::Empty),
        &mut whole_empty_graph,
        execution_context(),
    );
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
        Some(owner),
        prepared_whole_chunk_clip_surface_frame(ArtifactSurfaceResolvedClip::Scissor(
            GraphicsPassScissor::Logical(sealed_scissor),
        )),
        &mut graph,
        ctx,
    );

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
fn child_mask_past_a_full_stencil_isolates_then_emits_sealed_push_and_pop() {
    // Arriving with the stencil full, the mask opens a scratch layer whose
    // stencil starts empty, and composites it back under the outer masks.
    let mut full_ctx = execution_context();
    for _ in 0..u8::MAX {
        assert!(full_ctx.push_clip_id().is_some());
    }
    let mut full_viewport = Viewport::new();
    let full_owner = full_viewport
        .begin_retained_surface_frame_stage()
        .expect("full-stencil owner");
    let mut full_graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut full_viewport,
        Some(full_owner),
        prepared_child_mask_surface_frame(),
        &mut full_graph,
        full_ctx,
    );
    let full_snapshots = full_graph.test_rect_pass_snapshots();
    assert!(
        full_snapshots
            .iter()
            .any(|pass| pass.stencil_mode == RectStencilModeTestSnapshot::Increment { clip_id: 0 })
    );
    assert!(
        full_snapshots
            .iter()
            .any(|pass| pass.stencil_mode == RectStencilModeTestSnapshot::Decrement { clip_id: 1 })
    );

    let mut viewport = Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("mask owner");
    let mut graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(owner),
        prepared_child_mask_surface_frame(),
        &mut graph,
        execution_context(),
    );
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
