use super::*;

#[test]
fn fixed_inline_root_with_text_uses_the_owning_ifc_artifact_path() {
    let (arena, roots, root, text) = prepared_fixed_owning_inline_text_root();
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
            (text, PaintChunkRole::TextGlyphs),
        ]
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn standalone_inline_element_image_and_svg_do_not_require_an_owning_ifc_install() {
    fn assert_eligible(arena: &NodeArena, root: NodeKey, _label: &str) {
        let roots = [root];
        let (properties, generations) = sync_identity(arena, &roots);
        let _ = whole_frame_artifact(arena, &roots, &properties, &generations);
    }

    let mut element_arena = new_test_arena();
    let mut element = Element::new_with_id(0x7d10, 0.0, 0.0, 24.0, 18.0);
    let mut element_style = Style::new();
    element_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    element_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
    element_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    element_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(59, 130, 246)),
    );
    element.apply_style(element_style);
    let element_root = commit_element(&mut element_arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut element_arena, element_root, measure, place);
    element_arena
        .get_mut(element_root)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyFlags::ALL);
    element_arena.clear_arena_dirty_subtree(element_root, DirtyFlags::ALL);
    assert_eligible(&element_arena, element_root, "Element");

    let mut image_arena = new_test_arena();
    let mut image = Image::new_with_id(
        0x7d11,
        ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255, 255, 255, 255]),
        },
    );
    let mut image_style = Style::new();
    image_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    image_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
    image_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    image.apply_style(image_style);
    let image_root = commit_element(&mut image_arena, Box::new(image));
    measure_and_place(&mut image_arena, image_root, measure, place);
    image_arena
        .get_mut(image_root)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyFlags::ALL);
    image_arena.clear_arena_dirty_subtree(image_root, DirtyFlags::ALL);
    assert_eligible(&image_arena, image_root, "Image");

    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='18'><rect width='24' height='18' fill='#22c55e'/></svg>";
    let mut svg_arena = new_test_arena();
    let mut svg = Svg::new_with_id(0x7d12, SvgSource::Content(SVG.into()));
    let mut svg_style = Style::new();
    svg_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    svg_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
    svg_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    svg.apply_style(svg_style);
    let svg_root = commit_element(&mut svg_arena, Box::new(svg));
    measure_and_place(&mut svg_arena, svg_root, measure, place);
    {
        let mut node = svg_arena.get_mut(svg_root).unwrap();
        let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.prepare_content_paint_for_test(SVG, (24.0, 18.0), 1.0)
            .unwrap();
        svg.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    svg_arena.set_children(svg_root, Vec::new());
    svg_arena.clear_arena_dirty_subtree(svg_root, DirtyFlags::ALL);
    assert_eligible(&svg_arena, svg_root, "Svg");
}

#[test]
fn owning_inline_root_with_text_records_dom_order() {
    let (arena, roots, root, text) = prepared_owning_inline_text_root();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(artifact.chunks.len(), 2);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (text, PaintChunkRole::TextGlyphs),
        ]
    );
}

#[test]
fn owning_inline_root_with_decorated_span_records_dom_dfs() {
    let (arena, roots, root, span, text, fragment_count) =
        prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
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
            (span, PaintChunkRole::SelfDecoration),
            (text, PaintChunkRole::TextGlyphs),
        ],
        "coverage DOM DFS, not IFC plan order, is paint order"
    );
    assert_eq!(artifact.chunks[1].op_range.len(), fragment_count);
    assert!(matches!(
        artifact.chunks[1].payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(_, _)
    ));
}
