use super::*;

fn zero_surface_element(id: u64, x: f32, color: Color) -> Element {
    let mut element = Element::new_with_id(id, x, 20.0, 80.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn prepared_zero_surface_three_chunk_frame() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut arena = new_test_arena();
    let first = commit_element(
        &mut arena,
        Box::new(zero_surface_element(
            0xc3_a001,
            10.0,
            Color::rgb(230, 20, 30),
        )),
    );
    let mut child_element = zero_surface_element(0xc3_a002, 40.0, Color::rgb(20, 210, 40));
    child_element.set_position(0.0, 0.0);
    let child = commit_child(&mut arena, first, Box::new(child_element));
    let second = commit_element(
        &mut arena,
        Box::new(zero_surface_element(
            0xc3_a003,
            110.0,
            Color::rgb(30, 40, 220),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, first, measure, place);
    measure_and_place(&mut arena, second, measure, place);
    (arena, vec![first, second], child)
}

fn recorded_zero_surface_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
) -> crate::view::paint::PaintArtifact {
    let (properties, generations) = synced_paint_state(arena, roots);
    crate::view::paint::record_surface_dag_frame_artifact(arena, roots, &properties, &generations)
}

fn recorded_zero_surface_child_mask_candidate() -> RecordedArtifactCandidate {
    use crate::style::BorderRadius;

    let mut root_element = zero_surface_element(0xc3_a010, 0.0, Color::rgb(40, 80, 160));
    let mut rounded = Style::new();
    rounded.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    root_element.apply_style(rounded);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    commit_child(
        &mut arena,
        root,
        Box::new(zero_surface_element(
            0xc3_a011,
            12.0,
            Color::rgb(20, 180, 40),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let artifact = recorded_zero_surface_artifact(&arena, &[root]);
    let (chunk_count, op_count) = (artifact.chunks.len(), artifact.ops.len());
    assert_eq!(
        artifact
            .chunks
            .iter()
            .filter(|chunk| chunk.id.slot == crate::view::paint::RETAINED_CHILD_MASK_SLOT)
            .count(),
        2,
    );
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let plan = crate::view::paint::prepare_artifact_surface_raster_plan(
        artifact,
        artifact_surface_raster_context(
            &ctx,
            wgpu::Limits::default().max_texture_dimension_2d,
            ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES,
        ),
    )
    .expect("zero-surface child-mask artifact must prepare");
    assert!(plan.nodes().is_empty());
    let frame = crate::view::paint::seal_prepared_artifact_surface_frame(plan)
        .expect("zero-surface child-mask resident set must seal");
    RecordedArtifactCandidate {
        payload: RecordedArtifactPayload::ArtifactSurface(frame),
        chunk_count,
        op_count,
    }
}

#[test]
fn stage_c_zero_surface_prepare_seals_exact_multi_root_artifact_order() {
    let (arena, roots, child) = prepared_zero_surface_three_chunk_frame();
    let artifact = recorded_zero_surface_artifact(&arena, &roots);
    assert_eq!(
        (artifact.chunks.len(), artifact.ops.len()),
        (3, 3),
        "chunks={:?}",
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role, chunk.id.phase, chunk.id.slot))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>(),
        vec![roots[0], child, roots[1]],
    );
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.op_range.clone())
            .collect::<Vec<_>>(),
        vec![0..1, 1..2, 2..3],
    );
    assert_eq!(
        crate::view::paint::artifact_cursors(&artifact)
            .expect("complete chunk/op traversal owns a terminal cursor")
            .len(),
        3,
    );

    let context = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let plan = crate::view::paint::prepare_artifact_surface_raster_plan(
        artifact,
        artifact_surface_raster_context(
            &context,
            wgpu::Limits::default().max_texture_dimension_2d,
            ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES,
        ),
    )
    .expect("current-target zero-surface artifact must seal through the generic raster plan");
    assert_eq!(plan.roots().len(), 2);
    assert!(plan.nodes().is_empty());
}

#[test]
fn stage_c_zero_surface_child_mask_compiles_at_254_and_isolates_past_a_full_stencil() {
    let mut accepted_graph = FrameGraph::new();
    let mut accepted_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let accepted_target = accepted_ctx.allocate_target(&mut accepted_graph);
    accepted_ctx.set_current_target(accepted_target);
    for expected in 1..=254_u8 {
        assert_eq!(accepted_ctx.push_clip_id(), Some(expected));
    }
    let mut accepted_viewport = Viewport::new();
    let accepted_owner = accepted_viewport
        .begin_retained_surface_frame_stage()
        .expect("accepted frame owns one resident transaction");
    crate::view::paint::take_artifact_compile_count();
    compile_auto_artifact_frame(
        &mut accepted_viewport,
        Some(accepted_owner),
        &mut accepted_graph,
        recorded_zero_surface_child_mask_candidate(),
        &accepted_ctx,
    );
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);
    assert_eq!(
        accepted_viewport.pending_artifact_surface_resident_keys_for_test(),
        Some(Vec::new())
    );
    assert!(
        accepted_viewport
            .finish_retained_surface_transaction_for_frame(Some(accepted_owner), true,)
    );
    assert!(
        accepted_viewport
            .pending_artifact_surface_resident_keys_for_test()
            .is_none()
    );

    // Arriving with the stencil full, the mask isolates into a scratch layer
    // rather than rejecting the frame.
    let mut full_graph = FrameGraph::new();
    let mut full_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let full_target = full_ctx.allocate_target(&mut full_graph);
    full_ctx.set_current_target(full_target);
    for expected in 1..=u8::MAX {
        assert_eq!(full_ctx.push_clip_id(), Some(expected));
    }
    let mut full_viewport = Viewport::new();
    let full_owner = full_viewport
        .begin_retained_surface_frame_stage()
        .expect("full-stencil frame owns one resident transaction");
    compile_auto_artifact_frame(
        &mut full_viewport,
        Some(full_owner),
        &mut full_graph,
        recorded_zero_surface_child_mask_candidate(),
        &full_ctx,
    );
    assert!(full_viewport.finish_retained_surface_transaction_for_frame(Some(full_owner), true));
}

#[test]
fn stage_c_zero_surface_retained_auto_emits_the_leaf_fill_once() {
    let (arena, roots) = prepared_safe_leaf();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let selection_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let AutoAuthorityDecision::Artifact { candidate, trace } = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &selection_ctx,
        true,
    ) else {
        panic!("property-neutral frame must select artifact authority")
    };
    assert!(trace.rejections.is_empty());
    assert_eq!((candidate.chunk_count, candidate.op_count), (1, 1),);
    let RecordedArtifactPayload::ArtifactSurface(frame) = &candidate.payload;
    assert!(frame.raster_plan().nodes().is_empty());
    assert!(frame.residents().is_empty());

    crate::view::paint::take_artifact_compile_count();
    let mut viewport = Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("safe leaf owns one resident transaction");
    let mut graph = FrameGraph::new();
    let mut compile_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = compile_ctx.allocate_target(&mut graph);
    compile_ctx.set_current_target(target);
    compile_auto_artifact_frame(
        &mut viewport,
        Some(owner),
        &mut graph,
        candidate,
        &compile_ctx,
    );
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        Some(Vec::new())
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    assert!(
        viewport
            .pending_artifact_surface_resident_keys_for_test()
            .is_none()
    );

    let rects = graph.test_rect_pass_snapshots();
    assert_eq!(rects.len(), 1, "the safe leaf paints one fill");
    assert_eq!(
        rects[0].fill_color_bits,
        crate::style::ColorLike::to_rgba_f32(&Color::rgb(230, 20, 30)).map(f32::to_bits)
    );
}

#[test]
fn stage_c_general_surface_takes_root_opacity_through_the_common_executor() {
    let (arena, roots) = prepared_native_text_with_opacity(0.5);
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert_generic_primary(&arena, &roots, &properties, &generations, &ctx);
}
