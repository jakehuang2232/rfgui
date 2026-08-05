use super::*;

#[test]
fn generic_preedit_edge_survives_absent_caret_edge() {
    let (arena, roots, text_area_root, _, _) =
        prepared_projection_text_area_preedit_tree(8, "中", Some((0, "中".len())));
    {
        let mut node = arena.get_mut(text_area_root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.caret_visible = false;
    }
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert!(
        artifact
            .chunks
            .iter()
            .all(|chunk| chunk.id.role != PaintChunkRole::Caret),
        "hidden caret must not create a composite edge",
    );
    let underline_chunks = artifact
        .chunks
        .iter()
        .filter(|chunk| {
            chunk.owner == text_area_root
                && chunk.id.phase == PaintNodePhase::AfterChildren
                && chunk.id.role == PaintChunkRole::TextDecoration
        })
        .collect::<Vec<_>>();
    let [underline_chunk] = underline_chunks.as_slice() else {
        panic!("preedit must create exactly one independent underline edge")
    };
    let underline = PaintCompositeEdge::from_artifact_chunk(&artifact, underline_chunk)
        .expect("generic underline edge must be self-canonical");
    let opaque_delta = paint_composite_edge_opaque_delta(std::slice::from_ref(&underline))
        .expect("generic underline edge must have a sealed opaque delta");

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    emit_paint_composite_edges(&[underline], &mut graph, &mut ctx);

    assert_eq!(ctx.opaque_rect_order(), opaque_delta);
    let transparent = graph
        .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>()
        .len();
    let opaque = graph
        .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::OpaqueRectPass>()
        .len();
    assert_eq!(transparent + opaque, 1);
}
