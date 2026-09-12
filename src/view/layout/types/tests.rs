use super::*;

#[test]
fn layout_state_new_seeds_visual_box_and_zero_content_size() {
    let state = LayoutState::new(10.0, 20.0, 100.0, 50.0);
    assert!((state.layout_position.x - 10.0).abs() < 1e-6);
    assert!((state.layout_position.y - 20.0).abs() < 1e-6);
    assert!((state.layout_size.width - 100.0).abs() < 1e-6);
    assert!((state.layout_size.height - 50.0).abs() < 1e-6);
    assert!((state.layout_inner_position.x - 10.0).abs() < 1e-6);
    assert!((state.layout_flow_position.x - 10.0).abs() < 1e-6);
    // content_size starts at zero (children-driven), distinct from layout_size.
    assert!((state.content_size.width).abs() < 1e-6);
    assert!((state.content_size.height).abs() < 1e-6);
    assert!(state.should_render);
}

#[test]
fn layout_state_new_clamps_negative_dimensions_to_zero() {
    let state = LayoutState::new(0.0, 0.0, -50.0, -10.0);
    assert!((state.layout_size.width).abs() < 1e-6);
    assert!((state.layout_size.height).abs() < 1e-6);
}
