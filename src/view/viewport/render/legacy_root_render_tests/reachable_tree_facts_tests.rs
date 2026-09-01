use super::*;

#[test]
fn reachable_tree_facts_mark_the_typed_text_area_paint_family() {
    let (arena, roots, _, _) = prepared_scroll_text_area_scene();
    let facts = super::super::retained_auto_reachable_tree_facts(&arena, &roots);

    assert!(facts.has_scroll_container);
    assert!(facts.has_text_area_paint_family);
}

#[test]
fn reachable_tree_facts_do_not_misclassify_inline_ifc_owned_text_as_text_area() {
    let (arena, root, _, _) = crate::view::paint::nested_scroll_unready_text_fixture_for_test(
        crate::view::paint::NestedTextFallbackKind::InlineIfcOwned,
    );
    let facts = super::super::retained_auto_reachable_tree_facts(&arena, &[root]);

    assert!(facts.has_scroll_container);
    assert!(!facts.has_text_area_paint_family);
}

#[test]
fn authored_scroll_without_a_surface_snapshot_fails_closed_at_recording() {
    let mut arena = new_test_arena();
    let mut root_element = Element::new_with_id(0xe2_a330, 0.0, 0.0, 100.0, 80.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_element.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root_element));
    commit_child(
        &mut arena,
        root,
        Box::new(Element::new_with_id(0xe2_a331, 0.0, 0.0, 20.0, 20.0)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let roots = [root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert!(properties.scrolls.is_empty());
    assert!(
        super::super::retained_auto_reachable_tree_facts(&arena, &roots).has_scroll_container
    );
    let decision = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        true,
    );
    let AutoAuthorityDecision::Legacy { trace } = decision else {
        panic!("an authored scroll container without a surface snapshot must fail closed")
    };

    assert!(
        trace.rejections.iter().any(|rejection| matches!(
            rejection,
            AutoAuthorityRejection::Artifact { eligibility }
                if eligibility.reasons.as_slice()
                    == [crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                        crate::view::paint::LegacyPaintReason::ScrollContainer,
                    )]
        )),
        "authored scroll fallback trace: {:?}",
        trace.rejections
    );
    assert!(
        !trace
            .rejections
            .iter()
            .any(|rejection| matches!(rejection, AutoAuthorityRejection::ArtifactPrepare { .. })),
        "recording rejection must prevent construction of an empty raster plan"
    );
}
