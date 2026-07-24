use super::*;

#[test]
fn scroll_content_recorder_detaches_child_and_neutralizes_scroll_clip_atomically() {
    let (arena, root, child, properties, generations) = fixture();
    let artifact = record_scroll_content_local_artifact_for_plan(
        &arena,
        &properties,
        &generations,
        content_witness(root, child, &properties),
    )
    .unwrap();

    assert!(!artifact.chunks.is_empty());
    assert!(artifact.chunks.iter().all(|chunk| {
        chunk.owner == child
            && chunk.properties == Default::default()
            && chunk.id.role != super::super::super::PaintChunkRole::ScrollbarOverlay
    }));
    assert!(artifact.clip_nodes.is_empty());
    assert!(artifact.effect_nodes.is_empty());
    assert_eq!(
        artifact.owner_nodes,
        vec![super::super::super::PaintOwnerSnapshot {
            owner: child,
            parent: None,
        }]
    );
}

#[test]
fn scroll_content_recorder_normalizes_the_complete_two_dimensional_offset() {
    fn assert_rect_params_bitwise_eq(
        left: &crate::view::render_pass::draw_rect_pass::RectPassParams,
        right: &crate::view::render_pass::draw_rect_pass::RectPassParams,
    ) {
        assert_eq!(
            left.position.map(f32::to_bits),
            right.position.map(f32::to_bits)
        );
        assert_eq!(left.size.map(f32::to_bits), right.size.map(f32::to_bits));
        assert_eq!(
            left.fill_color.map(f32::to_bits),
            right.fill_color.map(f32::to_bits)
        );
        assert_eq!(left.opacity.to_bits(), right.opacity.to_bits());
        assert_eq!(
            left.border_widths.map(f32::to_bits),
            right.border_widths.map(f32::to_bits)
        );
        assert_eq!(
            left.border_radii.map(|radius| radius.map(f32::to_bits)),
            right.border_radii.map(|radius| radius.map(f32::to_bits))
        );
        assert_eq!(
            left.border_color.map(f32::to_bits),
            right.border_color.map(f32::to_bits)
        );
        assert_eq!(
            left.border_side_colors.map(|color| color.map(f32::to_bits)),
            right
                .border_side_colors
                .map(|color| color.map(f32::to_bits))
        );
        assert_eq!(left.use_border_side_colors, right.use_border_side_colors);
        assert_eq!(left.depth.to_bits(), right.depth.to_bits());
        for (left, right) in [
            (left.gradient.as_ref(), right.gradient.as_ref()),
            (
                left.border_gradient.as_ref(),
                right.border_gradient.as_ref(),
            ),
        ] {
            match (left, right) {
                (None, None) => {}
                (Some(left), Some(right)) => {
                    assert_eq!(left.kind, right.kind);
                    assert_eq!(left.axis.map(f32::to_bits), right.axis.map(f32::to_bits));
                    assert_eq!(left.repeating, right.repeating);
                    assert_eq!(left.stops.len(), right.stops.len());
                    for (left, right) in left.stops.iter().zip(right.stops.iter()) {
                        assert_eq!(left.color.map(f32::to_bits), right.color.map(f32::to_bits));
                        assert_eq!(left.pos.map(f32::to_bits), right.pos.map(f32::to_bits));
                    }
                }
                _ => panic!("normalized rect gradient presence changed"),
            }
        }
    }

    fn assert_ops_bitwise_eq(left: &PaintOp, right: &PaintOp) {
        match (left, right) {
            (PaintOp::DrawRect(left), PaintOp::DrawRect(right)) => {
                assert_eq!(left.mode, right.mode);
                assert_rect_params_bitwise_eq(&left.params, &right.params);
            }
            (PaintOp::PreparedShadow(left), PaintOp::PreparedShadow(right)) => {
                assert_eq!(
                    left.mesh
                        .vertices
                        .iter()
                        .map(|point| point.map(f32::to_bits))
                        .collect::<Vec<_>>(),
                    right
                        .mesh
                        .vertices
                        .iter()
                        .map(|point| point.map(f32::to_bits))
                        .collect::<Vec<_>>()
                );
                assert_eq!(left.mesh.indices, right.mesh.indices);
                assert_eq!(
                    left.params.offset_x.to_bits(),
                    right.params.offset_x.to_bits()
                );
                assert_eq!(
                    left.params.offset_y.to_bits(),
                    right.params.offset_y.to_bits()
                );
                assert_eq!(
                    left.params.blur_radius.to_bits(),
                    right.params.blur_radius.to_bits()
                );
                assert_eq!(
                    left.params.color.map(f32::to_bits),
                    right.params.color.map(f32::to_bits)
                );
                assert_eq!(
                    left.params.opacity.to_bits(),
                    right.params.opacity.to_bits()
                );
                assert_eq!(left.params.spread.to_bits(), right.params.spread.to_bits());
                assert_eq!(left.params.clip_to_geometry, right.params.clip_to_geometry);
            }
            _ => panic!("normalized scroll-content op kind changed"),
        }
    }

    let (zero_arena, zero_root, zero_child, zero_properties, zero_generations) =
        fixture_at_offset([0.0, 0.0]);
    let zero = record_scroll_content_local_artifact_for_plan(
        &zero_arena,
        &zero_properties,
        &zero_generations,
        content_witness(zero_root, zero_child, &zero_properties),
    )
    .unwrap();
    let (moved_arena, moved_root, moved_child, moved_properties, moved_generations) =
        fixture_at_offset([3.5, 47.25]);
    let moved = record_scroll_content_local_artifact_for_plan(
        &moved_arena,
        &moved_properties,
        &moved_generations,
        content_witness(moved_root, moved_child, &moved_properties),
    )
    .unwrap();
    let (
        negative_zero_arena,
        negative_zero_root,
        negative_zero_child,
        negative_zero_properties,
        negative_zero_generations,
    ) = fixture_at_offset([-0.0, -0.0]);
    let negative_zero = record_scroll_content_local_artifact_for_plan(
        &negative_zero_arena,
        &negative_zero_properties,
        &negative_zero_generations,
        content_witness(
            negative_zero_root,
            negative_zero_child,
            &negative_zero_properties,
        ),
    )
    .unwrap();

    for candidate in [&moved, &negative_zero] {
        assert_eq!(zero.chunks.len(), candidate.chunks.len());
        for (zero, candidate) in zero.chunks.iter().zip(&candidate.chunks) {
            assert_eq!(
                [
                    zero.bounds.x,
                    zero.bounds.y,
                    zero.bounds.width,
                    zero.bounds.height
                ]
                .map(f32::to_bits),
                [
                    candidate.bounds.x,
                    candidate.bounds.y,
                    candidate.bounds.width,
                    candidate.bounds.height,
                ]
                .map(f32::to_bits)
            );
            assert_eq!(zero.payload_identity, candidate.payload_identity);
        }
        assert_eq!(zero.ops.len(), candidate.ops.len());
        assert!(!zero.ops.is_empty());
        for (zero, candidate) in zero.ops.iter().zip(&candidate.ops) {
            assert_ops_bitwise_eq(zero, candidate);
        }
    }
}

#[test]
fn scroll_content_recorder_rejects_retargeted_edge_and_inline_ifc_leaf() {
    let (mut arena, root, child, properties, generations) = fixture();
    let witness = content_witness(root, child, &properties);
    let other_parent = arena.insert(Node::new(Box::new(Element::new_with_id(
        81_103, 0.0, 0.0, 1.0, 1.0,
    ))));
    arena.set_parent(child, Some(other_parent));
    assert_eq!(arena.get(root).unwrap().element.children(), [child]);
    assert!(
        record_scroll_content_local_artifact_for_plan(&arena, &properties, &generations, witness,)
            .is_err()
    );

    let (arena, root, child, properties, generations) = fixture();
    let witness = content_witness(root, child, &properties);
    let mut inline_style = Style::new();
    inline_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    inline_style.insert(PropertyId::Width, ParsedValue::Auto);
    inline_style.insert(PropertyId::Height, ParsedValue::Auto);
    let mut child_node = arena.get_mut(child).unwrap();
    let child_element = child_node
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap();
    child_element.apply_style(inline_style);
    child_element.layout_state.layout_size = Size {
        width: 0.0,
        height: 0.0,
    };
    drop(child_node);
    assert!(
        record_scroll_content_local_artifact_for_plan(&arena, &properties, &generations, witness,)
            .is_err()
    );
}

#[test]
fn scroll_content_recorder_rejects_half_property_mismatch_wrong_parent_and_nonfinite_offset() {
    let (arena, root, child, mut properties, generations) = fixture();
    let witness = content_witness(root, child, &properties);
    properties.states.get_mut(&child).unwrap().paint.clip = None;
    assert!(
        record_scroll_content_local_artifact_for_plan(&arena, &properties, &generations, witness,)
            .is_err()
    );

    let (mut arena, root, child, properties, generations) = fixture();
    let witness = content_witness(root, child, &properties);
    arena.set_parent(child, None);
    assert!(
        record_scroll_content_local_artifact_for_plan(&arena, &properties, &generations, witness,)
            .is_err()
    );

    let (arena, root, child, properties, _) = fixture();
    let mut scroll = properties.scroll_snapshot_for(ScrollNodeId(root)).unwrap();
    scroll.offset.x = f32::NAN;
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let clip = properties
        .clip_snapshot_for(Some(clip_id))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert!(PaintScrollContentWitness::new(root, child, scroll, clip).is_none());
    drop(arena);
}
