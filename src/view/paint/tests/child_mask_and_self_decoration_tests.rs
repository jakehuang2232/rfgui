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
            ParsedValue::Position(
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
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    let stats = manifest.stats();
    assert_eq!(stats.total_nodes, 10);
    assert_eq!(stats.legacy_boundaries, 0);
    assert_eq!(stats.validation_errors, 0, "{stats:?}");

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
                _ => false,
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

    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible);
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
fn retained_rectangular_child_mask_records_and_rejects_tampering() {
    let mut root_element = Element::new_with_id(0x6d4e, 0.0, 0.0, 100.0, 80.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(80.0)));
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(40, 80, 160)),
    );
    root_element.apply_style(root_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    let mut child = Element::new_with_id(0x6d4f, 0.0, 0.0, 48.0, 32.0);
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(76.0))
                .top(Length::px(60.0)),
        ),
    );
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(20, 180, 40)),
    );
    child.apply_style(child_style);
    commit_child(&mut arena, root, Box::new(child));

    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let valid = whole_frame_artifact(&arena, &[root], &properties, &generations).0;
    let mask_chunks = valid
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, chunk)| chunk.id.slot == RETAINED_CHILD_MASK_SLOT)
        .map(|(index, chunk)| (index, chunk.id.phase, chunk.op_range.start))
        .collect::<Vec<_>>();
    assert_eq!(mask_chunks.len(), 2);
    assert_eq!(mask_chunks[0].1, PaintNodePhase::BeforeChildren);
    assert_eq!(mask_chunks[1].1, PaintNodePhase::AfterChildren);
    let mask_op = mask_chunks[0].2;
    let PaintOp::DrawRect(mask) = &valid.ops[mask_op] else {
        panic!("rectangular mask begin owns a DrawRect")
    };
    assert!(
        mask.params
            .border_radii
            .iter()
            .flatten()
            .all(|radius| *radius == 0.0)
    );
    compiled_whole_frame_graph(&valid);

    let mut radii_tampered = valid.clone();
    let PaintOp::DrawRect(mask) = &mut radii_tampered.ops[mask_op] else {
        panic!("rectangular mask begin owns a DrawRect")
    };
    mask.params.border_radii[0][0] = 1.0;
    assert_compiler_rejects_before_emit(&radii_tampered, "rectangular child-mask radii tamper");

    let mut phase_tampered = valid;
    phase_tampered.chunks[mask_chunks[1].0].id.phase = PaintNodePhase::BeforeChildren;
    assert_compiler_rejects_before_emit(
        &phase_tampered,
        "rectangular child-mask phase pairing tamper",
    );
}

#[test]
fn retained_child_mask_store_rejects_geometry_and_phase_tampering() {
    let mut root_element = leaf_element(0x6d50, Color::rgb(40, 80, 160), 1.0, false);
    let mut rounded = Style::new();
    rounded.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    root_element.apply_style(rounded);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(0x6d51, Color::rgb(20, 180, 40), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let valid = whole_frame_artifact(&arena, &[root], &properties, &generations).0;
    let mask_begin = valid
        .chunks
        .iter()
        .position(|chunk| {
            chunk.id.slot == RETAINED_CHILD_MASK_SLOT
                && chunk.id.phase == PaintNodePhase::BeforeChildren
        })
        .unwrap();
    let mask_end = valid
        .chunks
        .iter()
        .position(|chunk| {
            chunk.id.slot == RETAINED_CHILD_MASK_SLOT
                && chunk.id.phase == PaintNodePhase::AfterChildren
        })
        .unwrap();

    let mut position_tampered = valid.clone();
    let position_op = position_tampered.chunks[mask_begin].op_range.start;
    let PaintOp::DrawRect(mask) = &mut position_tampered.ops[position_op] else {
        panic!("mask begin owns a DrawRect")
    };
    mask.params.position[0] += 1.0;
    assert_compiler_rejects_before_emit(&position_tampered, "child-mask position tamper");

    let mut radii_tampered = valid.clone();
    let radii_op = radii_tampered.chunks[mask_begin].op_range.start;
    let PaintOp::DrawRect(mask) = &mut radii_tampered.ops[radii_op] else {
        panic!("mask begin owns a DrawRect")
    };
    mask.params.border_radii[0][0] += 1.0;
    assert_compiler_rejects_before_emit(&radii_tampered, "child-mask radii tamper");

    let mut phase_tampered = valid;
    phase_tampered.chunks[mask_end].id.phase = PaintNodePhase::BeforeChildren;
    assert_compiler_rejects_before_emit(&phase_tampered, "child-mask phase pairing tamper");
}

#[test]
fn self_decoration_grammar_accepts_empty_but_rejects_shadow_only_and_border_only() {
    let (arena, root, properties, generations) =
        prepared_shadow_leaf(0x6d26, 1.0, two_outer_shadows(), true);
    let (valid, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);

    let mut empty = valid.clone();
    empty.ops.clear();
    empty.chunks[0].op_range = 0..0;
    empty.chunks[0].payload_identity =
        PaintPayloadIdentity::prepared_shadows(std::iter::empty());
    let _ = take_artifact_compile_count();
    let _ = compiled_whole_frame_graph(&empty);
    assert_eq!(take_artifact_compile_count(), 1);

    let mut empty_with_stale_identity = empty.clone();
    empty_with_stale_identity.chunks[0].payload_identity =
        valid.chunks[0].payload_identity.clone();
    let _ = take_artifact_compile_count();
    let _ = compiled_whole_frame_graph(&empty_with_stale_identity);
    assert_eq!(take_artifact_compile_count(), 0);

    let mut shadow_only = valid.clone();
    shadow_only
        .ops
        .retain(|op| matches!(op, PaintOp::PreparedShadow(_)));
    shadow_only.chunks[0].op_range = 0..shadow_only.ops.len();
    let _ = take_artifact_compile_count();
    let _ = compiled_whole_frame_graph(&shadow_only);
    assert_eq!(take_artifact_compile_count(), 0);

    let mut border_only = valid;
    let border = border_only
        .ops
        .iter()
        .find(|op| {
            matches!(op, PaintOp::DrawRect(rect)
                if rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly)
        })
        .cloned()
        .unwrap();
    border_only.ops = vec![border];
    border_only.chunks[0].op_range = 0..1;
    let _ = take_artifact_compile_count();
    let _ = compiled_whole_frame_graph(&border_only);
    assert_eq!(take_artifact_compile_count(), 0);
}

#[test]
fn shadow_metadata_identity_detects_order_and_param_drift_before_full_authority() {
    let (arena, root, properties, generations) =
        prepared_shadow_leaf(0x6d27, 1.0, two_outer_shadows(), false);
    let metadata = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let PaintCoverageItem::ArtifactChunk { chunk, .. } = &metadata.items[0] else {
        panic!("shadow metadata must be an artifact chunk")
    };
    let PaintPayloadIdentity::PreparedShadows(identities, _) = &chunk.payload_identity else {
        panic!("shadow metadata must own ordered structural identities")
    };
    assert_eq!(identities.len(), 2);

    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_box_shadows(vec![
            BoxShadow::new()
                .color(Color::rgb(20, 40, 220))
                .offset_x(-7.0)
                .offset_y(4.5),
            BoxShadow::new()
                .color(Color::rgb(220, 30, 20))
                .offset_x(1.5)
                .offset_y(-2.25),
        ]);
    let full = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(!frame_recorder::canonical_manifest_matches(
        &metadata, &full
    ));
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
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible);
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

#[test]
fn css_opacity_zero_does_not_bypass_remaining_metadata_capability_blockers() {
    fn element(id: u64, position: Option<Position>, transform: bool) -> Element {
        let mut element = Element::new_with_id(id, 10.25, 20.75, 80.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(40, 80, 160)),
        );
        style.set_box_shadow(two_outer_shadows());
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
        if let Some(position) = position {
            style.insert(PropertyId::Position, ParsedValue::Position(position));
        }
        if transform {
            style.set_transform(Transform::new([Translate::x(Length::px(3.0))]));
        }
        element.apply_style(style);
        element
    }

    let cases = [(
        "transform",
        element(0x6d29, None, true),
        LegacyPaintReason::Transform,
    )];
    for (case, element, expected) in cases {
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let _ = take_full_artifact_record_count();
        let error = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(
            error
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(expected)),
            "{case}: {error:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0, "{case}");
    }

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element(0x6d2c, None, false)));
    commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(0x6d2d, Color::rgb(20, 180, 40), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let _ = take_full_artifact_record_count();
    let error = record_property_neutral_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert!(
        error
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineRoot
            )),
        "children: {error:?}"
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}
