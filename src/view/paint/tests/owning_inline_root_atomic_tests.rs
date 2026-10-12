use super::*;

#[test]
fn fixed_inline_root_with_atomic_records_standard_chunks() {
    let (arena, roots, root, before, atomic, after) = prepared_owning_inline_root_with_atomic();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (before, PaintChunkRole::TextGlyphs),
            (atomic, PaintChunkRole::SelfDecoration),
            (after, PaintChunkRole::TextGlyphs),
        ],
        "atomic children keep standard coverage chunks in live DOM order"
    );
}

#[test]
fn owning_inline_root_atomic_move_and_paint_refresh_preserve_authority_and_order() {
    fn record(
        arena: &NodeArena,
        roots: &[NodeKey],
    ) -> (PaintArtifact, Vec<(NodeKey, PaintChunkRole)>) {
        let (properties, generations) = sync_identity(arena, roots);
        let artifact = whole_frame_artifact(arena, roots, &properties, &generations);
        let order = artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect();
        (artifact, order)
    }

    let (mut arena, roots, root, _, atomic, _) = prepared_owning_inline_root_with_atomic();
    let (initial, initial_order) = record(&arena, &roots);
    let initial_root_range = initial
        .chunks
        .iter()
        .find(|chunk| chunk.owner == root)
        .unwrap()
        .op_range
        .clone();

    let mut moved = arena
        .get(root)
        .unwrap()
        .element
        .last_placement()
        .expect("root fixture must retain placement");
    moved.parent_x += 13.0;
    moved.parent_y += 7.0;
    arena.with_element_taken(root, |root, arena| root.place(moved, arena));
    let (after_move, move_order) = record(&arena, &roots);
    assert_eq!(move_order, initial_order);

    let mut node = arena.get_mut(atomic).unwrap();
    let atomic_element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
    atomic_element.set_background_color_value(Color::rgb(239, 68, 68));
    drop(node);
    let (after_paint, paint_order) = record(&arena, &roots);
    assert_eq!(paint_order, initial_order);
    assert_eq!(
        after_paint
            .chunks
            .iter()
            .find(|chunk| chunk.owner == root)
            .unwrap()
            .op_range,
        initial_root_range,
        "owning root remains SelfDecoration-only after atomic paint refresh"
    );
    assert_eq!(
        after_paint
            .chunks
            .iter()
            .filter(|chunk| chunk.owner == atomic)
            .count(),
        1,
        "atomic paint remains its own standard chunk"
    );
    assert_eq!(
        compiled_whole_frame_graph(&after_move).pass_descriptors(),
        compiled_whole_frame_graph(&after_paint).pass_descriptors()
    );
}

#[test]
fn mixed_wrapping_inline_root_uses_live_dom_dfs() {
    let (arena, roots, root, before, span, nested_text, atomic, after, fragment_count) =
        prepared_mixed_wrapping_inline_root();
    assert!(fragment_count >= 2, "fixture must exercise a wrapped span");
    assert!(
        atomic.data().as_ffi() < before.data().as_ffi(),
        "fixture must allocate the atomic before its earlier DOM sibling"
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (before, PaintChunkRole::TextGlyphs),
            (span, PaintChunkRole::SelfDecoration),
            (nested_text, PaintChunkRole::TextGlyphs),
            (atomic, PaintChunkRole::SelfDecoration),
            (after, PaintChunkRole::TextGlyphs),
        ],
        "coverage DOM DFS alone owns paint order"
    );
}

#[test]
fn owning_inline_root_with_image_atomic_keeps_image_chunk() {
    let (arena, roots, root, before, image, after) =
        prepared_owning_inline_root_with_image_atomic();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (before, PaintChunkRole::TextGlyphs),
            (image, PaintChunkRole::ImageContent),
            (after, PaintChunkRole::TextGlyphs),
        ]
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn owning_inline_root_with_svg_atomic_keeps_svg_chunk() {
    let (arena, roots, root, before, svg, after) = prepared_owning_inline_root_with_svg_atomic();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (before, PaintChunkRole::TextGlyphs),
            (svg, PaintChunkRole::SvgContent),
            (after, PaintChunkRole::TextGlyphs),
        ]
    );
}

#[test]
fn owning_inline_root_paint_refresh_preserves_auto_wrap_geometry() {
    fn fragment_geometry(arena: &NodeArena, span: NodeKey) -> Vec<[u32; 4]> {
        arena
            .get(span)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .inline_fragment_rects()
            .iter()
            .map(|rect| {
                [
                    rect.x.to_bits(),
                    rect.y.to_bits(),
                    rect.width.to_bits(),
                    rect.height.to_bits(),
                ]
            })
            .collect()
    }

    fn root_geometry(arena: &NodeArena, root: NodeKey) -> ([u32; 2], [u32; 2]) {
        let node = arena.get(root).unwrap();
        let root = node.element.as_any().downcast_ref::<Element>().unwrap();
        let measured = root.measured_size();
        (
            [measured.0.to_bits(), measured.1.to_bits()],
            [
                root.inline_ifc_root_build_width_for_test()
                    .expect("fixture must retain an IFC install")
                    .to_bits(),
                root.inline_ifc_root_applied_width_for_test()
                    .expect("fixture must retain an IFC install")
                    .to_bits(),
            ],
        )
    }

    let (mut arena, roots, root, span, _, _) =
        prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
    let before = fragment_geometry(&arena, span);
    let root_before = root_geometry(&arena, root);
    assert!(before.len() >= 2, "fixture must wrap before paint damage");

    arena
        .get_mut(span)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_background_color_value(Color::rgb(22, 163, 74));
    let (measure, place) = wrapping_inline_span_constraints();
    measure_and_place(&mut arena, root, measure, place);

    assert_eq!(
        fragment_geometry(&arena, span),
        before,
        "paint-only same-constraints refresh must retain the original shaping width"
    );
    assert_eq!(
        root_geometry(&arena, root),
        root_before,
        "paint-only same-constraints refresh must preserve root size and build authority"
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    let refreshed = artifact
        .ops
        .iter()
        .find_map(|op| match op {
            PaintOp::PreparedInlineIfcDecoration(fragment) => Some(fragment),
            _ => None,
        })
        .expect("refreshed span must emit inline decoration ops");
    assert_eq!(
        refreshed.fill.fill_color.map(f32::to_bits),
        Color::rgb(22, 163, 74).to_rgba_f32().map(f32::to_bits)
    );
}

#[test]
fn owning_inline_root_move_and_paint_preserve_dual_width_authority() {
    fn fragments(arena: &NodeArena, span: NodeKey) -> Vec<[f32; 4]> {
        arena
            .get(span)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .inline_fragment_rects()
            .iter()
            .map(|rect| [rect.x, rect.y, rect.width, rect.height])
            .collect()
    }
    fn widths(arena: &NodeArena, root: NodeKey) -> [u32; 2] {
        let node = arena.get(root).unwrap();
        let root = node.element.as_any().downcast_ref::<Element>().unwrap();
        [
            root.inline_ifc_root_build_width_for_test()
                .unwrap()
                .to_bits(),
            root.inline_ifc_root_applied_width_for_test()
                .unwrap()
                .to_bits(),
        ]
    }

    let (mut arena, roots, root, span, _, _) =
        prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
    let before_fragments = fragments(&arena, span);
    let before_widths = widths(&arena, root);
    arena
        .get_mut(span)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_background_color_value(Color::rgb(22, 163, 74));

    let (measure, mut place) = wrapping_inline_span_constraints();
    place.parent_x = 7.0;
    place.parent_y = 11.0;
    measure_and_place(&mut arena, root, measure, place);

    assert_eq!(widths(&arena, root), before_widths);
    assert_eq!(
        fragments(&arena, span),
        before_fragments
            .iter()
            .map(|rect| [rect[0] + 7.0, rect[1] + 11.0, rect[2], rect[3]])
            .collect::<Vec<_>>()
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    let refreshed = artifact
        .ops
        .iter()
        .find_map(|op| match op {
            PaintOp::PreparedInlineIfcDecoration(fragment) => Some(fragment),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        refreshed.fill.fill_color.map(f32::to_bits),
        Color::rgb(22, 163, 74).to_rgba_f32().map(f32::to_bits)
    );
}

#[test]
fn owning_inline_root_assigned_width_change_reshapes_current_install() {
    fn state(arena: &NodeArena, root: NodeKey, text: NodeKey) -> ([u32; 2], [u32; 4]) {
        let node = arena.get(root).unwrap();
        let root = node.element.as_any().downcast_ref::<Element>().unwrap();
        let widths = [
            root.inline_ifc_root_build_width_for_test()
                .unwrap()
                .to_bits(),
            root.inline_ifc_root_applied_width_for_test()
                .unwrap()
                .to_bits(),
        ];
        let bounds = arena
            .get(text)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Text>()
            .unwrap()
            .inline_ifc_owned_paint_geometry_for_test()
            .unwrap()
            .0;
        (
            widths,
            [
                bounds.x.to_bits(),
                bounds.y.to_bits(),
                bounds.width.to_bits(),
                bounds.height.to_bits(),
            ],
        )
    }

    let (mut arena, _roots, root, text) = prepared_percent_owning_inline_text_root();
    let before = state(&arena, root, text);
    let (_, mut place) = constraints();
    place.available_width = 240.0;
    place.percent_base_width = Some(480.0);
    arena.with_element_taken(root, |element, _arena| {
        element.set_layout_width(240.0);
    });
    arena.refresh_subtree_dirty_cache(root);
    arena.with_element_taken(root, |element, arena| {
        element.place(place, arena);
    });
    let after = state(&arena, root, text);

    assert_ne!(after.0[1], before.0[1], "assigned width must change");
    assert_eq!(
        after.0[0], after.0[1],
        "a real assigned-width change establishes a new build authority"
    );
    assert_ne!(
        after.1, before.1,
        "the assigned width change must reshape text geometry"
    );
}
