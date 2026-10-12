use super::*;

#[test]
fn viewport_scissor_preserves_explicit_empty() {
    assert_eq!(
        rect_to_logical_scissor_rect(Rect {
            x: 12.0,
            y: 18.0,
            width: 0.0,
            height: 20.0,
        }),
        [12, 18, 0, 20]
    );
    assert_eq!(
        rect_to_logical_scissor_rect(Rect {
            x: -5.0,
            y: -7.0,
            width: -10.0,
            height: -20.0,
        }),
        [0, 0, 0, 0]
    );

    let mut text_area = TextArea::new();
    text_area.layout_state.layout_position =
        crate::view::base_component::Position { x: 12.0, y: 18.0 };
    text_area.viewport_size = crate::view::base_component::Size {
        width: 0.0,
        height: 20.0,
    };
    assert_eq!(text_area.viewport_logical_scissor_rect(), [12, 18, 0, 20]);
    assert_eq!(
        ElementTrait::contents_clip_geometry(&text_area)
            .and_then(|clip| clip.placed_at(glam::Vec2::new(12.0, 18.0))),
        Some(crate::view::compositor::property_tree::ClipGeometry::Viewport([12, 18, 0, 20])),
        "the typed clip must keep an explicit empty",
    );
}
