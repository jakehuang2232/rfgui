use super::*;

#[test]
fn active_scroll_with_a_removed_surface_snapshot_fails_closed_at_recording() {
    let mut arena = new_test_arena();
    let mut root_element = Element::new_with_id(0xe2_a330, 0.0, 0.0, 100.0, 80.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(80.0)));
    root_element.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root_element));
    let mut child = Element::new_with_id(0xe2_a331, 0.0, 0.0, 20.0, 160.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(160.0)));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(255, 0, 0)),
    );
    child.apply_style(style);
    commit_child(&mut arena, root, Box::new(child));
    let mut viewport = Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        [320.0, 240.0],
    );
    let roots = [root];
    let (mut properties, generations) = synced_paint_state(&arena, &roots);
    assert_eq!(properties.scrolls.len(), 1);
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true),
        AutoAuthorityDecision::Artifact { .. }
    ));
    // A complete active observation is not interchangeable with an inactive
    // declaration. Simulate loss of its frozen snapshot, retaining the scene.
    properties.scrolls.clear();
    assert!(
        arena
            .get(root)
            .unwrap()
            .element
            .retained_paint_properties()
            .is_scroll_container
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
        !trace.rejections.is_empty(),
        "missing active snapshot must remain a typed rejection"
    );
}
