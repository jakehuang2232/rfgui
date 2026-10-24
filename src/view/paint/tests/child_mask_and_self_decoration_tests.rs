use super::*;

#[test]
fn retained_child_mask_keeps_window_like_overflow_handles_outside_mask_scope() {
    let mut root_element = leaf_element(0x6d30, Color::rgb(40, 80, 160), 1.0, false);
    let mut rounded = Style::new();
    rounded.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    root_element.apply_style(rounded);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    let content = commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(0x6d31, Color::rgb(20, 180, 40), 1.0, false)),
    );
    let mut handles = Vec::new();
    for index in 0..8 {
        let mut handle = leaf_element(0x6d40 + index, Color::rgb(220, 80, 30), 1.0, false);
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::position(
                Position::absolute()
                    .left(Length::px(index as f32 * 5.0))
                    .top(Length::px(index as f32 * 3.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        handle.apply_style(style);
        handles.push(commit_child(&mut arena, root, Box::new(handle)));
    }

    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    // Layout derives `should_paint`, so make the first resize handle
    // event-only after layout. It must still retain canonical coverage.
    arena
        .get_mut(handles[0])
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_should_paint_for_test(false);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let manifest = record_coverage_manifest(
        &arena,
        &[root],
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    let stats = manifest.stats();
    assert_eq!(stats.total_nodes, 10);

    let mask_end_index = manifest
        .items
        .iter()
        .position(|item| {
            matches!(
                item,
                PaintCoverageItem::ArtifactChunk { chunk, .. }
                    if chunk.owner == root
                        && chunk.id.slot == RETAINED_CHILD_MASK_SLOT
                        && chunk.id.phase == PaintNodePhase::AfterChildren
            )
        })
        .expect("rounded host records a mask end");
    let content_index = manifest
        .items
        .iter()
        .position(|item| {
            matches!(item, PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == content)
        })
        .expect("in-scope content records");
    assert!(content_index < mask_end_index);
    for handle in &handles {
        let handle_index = manifest
            .items
            .iter()
            .position(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => chunk.owner == *handle,
                PaintCoverageItem::TransparentNode { owner, .. }
                | PaintCoverageItem::CulledSubtree { owner, .. } => *owner == *handle,
            })
            .unwrap_or_else(|| panic!("overflow handle {handle:?} must remain covered"));
        assert!(handle_index > mask_end_index);
    }
    assert!(manifest.items.iter().any(|item| matches!(
        item,
        PaintCoverageItem::ArtifactChunk {
            chunk,
            ops: Some(ops),
            ..
        } if chunk.owner == handles[0] && ops.is_empty()
    )));

    let artifact = whole_frame_artifact(&arena, &[root], &properties, &generations);
    let artifact_mask_end = artifact
        .chunks
        .iter()
        .position(|chunk| {
            chunk.owner == root
                && chunk.id.slot == RETAINED_CHILD_MASK_SLOT
                && chunk.id.phase == PaintNodePhase::AfterChildren
        })
        .unwrap();
    for handle in handles.iter().skip(1) {
        assert!(
            artifact
                .chunks
                .iter()
                .position(|chunk| chunk.owner == *handle)
                .unwrap()
                > artifact_mask_end
        );
    }
}

#[test]
fn css_opacity_zero_records_canonical_empty_self_decoration_and_shadow_identity() {
    let mut element = Element::new_with_id(0x6d28, 10.25, 20.75, 80.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(40, 80, 160)),
    );
    style.set_box_shadow(two_outer_shadows());
    style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
    element.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let artifact = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(artifact.ops.is_empty());
    assert!(matches!(
        &artifact.chunks[0].payload_identity,
        PaintPayloadIdentity::PreparedShadows(identities, decoration)
            if identities.is_empty() && decoration.is_empty()
    ));
    let _ = take_artifact_compile_count();
    let _ = compiled_whole_frame_graph(&artifact);
    assert_eq!(take_artifact_compile_count(), 1);
}
