use crate::viewport::debug::append_overlay_label_geometry;
use super::{
    MouseButton, PendingClick, Viewport,
    build_reuse_overlay_geometry, is_valid_click_candidate,
};
use crate::transition::CHANNEL_STYLE_BOX_SHADOW;
use crate::ui::{Binding, RsxNode, UiDirtyState};
use crate::view::Element as HostElement;
use crate::view::base_component::BoxModelSnapshot;
use crate::view::base_component::{
    Element, LayoutConstraints, LayoutPlacement, Layoutable, get_scroll_offset_by_id,
    set_scroll_offset_by_id,
};
use crate::{
    Length, ParsedValue, PropertyId, ScrollDirection, Style, Transform, Transition,
    TransitionProperty, Transitions, Translate,
};

fn place_root(root: &mut Element, width: f32, height: f32) {
    root.measure(LayoutConstraints {
        max_width: width,
        max_height: height,
        viewport_width: width,
        percent_base_width: Some(width),
        percent_base_height: Some(height),
        viewport_height: height,
    });
    root.place(LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: width,
        available_height: height,
        viewport_width: width,
        percent_base_width: Some(width),
        percent_base_height: Some(height),
        viewport_height: height,
    });
}

fn element_by_id_mut(
    root: &mut dyn crate::view::base_component::ElementTrait,
    node_id: u64,
) -> Option<&mut Element> {
    if root.id() == node_id {
        return root.as_any_mut().downcast_mut::<Element>();
    }
    if let Some(children) = root.children_mut() {
        for child in children.iter_mut() {
            if let Some(found) = element_by_id_mut(child.as_mut(), node_id) {
                return Some(found);
            }
        }
    }
    None
}

#[test]
fn click_requires_same_button_and_target() {
    let pending = PendingClick {
        button: MouseButton::Left,
        target_id: 42,
        viewport_x: 10.0,
        viewport_y: 10.0,
    };

    assert!(is_valid_click_candidate(
        pending,
        MouseButton::Left,
        Some(42),
        12.0,
        12.0
    ));
    assert!(!is_valid_click_candidate(
        pending,
        MouseButton::Right,
        Some(42),
        12.0,
        12.0
    ));
    assert!(!is_valid_click_candidate(
        pending,
        MouseButton::Left,
        Some(99),
        12.0,
        12.0
    ));
}

#[test]
fn click_rejects_large_pointer_travel() {
    let pending = PendingClick {
        button: MouseButton::Left,
        target_id: 7,
        viewport_x: 10.0,
        viewport_y: 10.0,
    };

    assert!(is_valid_click_candidate(
        pending,
        MouseButton::Left,
        Some(7),
        14.0,
        13.0
    ));
    assert!(!is_valid_click_candidate(
        pending,
        MouseButton::Left,
        Some(7),
        16.0,
        10.0
    ));
}

#[test]
fn reuse_overlay_geometry_adds_node_id_label_when_requested() {
    let snapshot = BoxModelSnapshot {
        node_id: 42,
        parent_id: None,
        x: 10.0,
        y: 12.0,
        width: 50.0,
        height: 20.0,
        border_radius: 0.0,
        should_render: true,
    };

    let (plain_vertices, plain_indices) =
        build_reuse_overlay_geometry(&snapshot, 1.0, 200.0, 200.0, [1.0, 0.0, 0.0, 1.0], None);
    let (label_vertices, label_indices) = build_reuse_overlay_geometry(
        &snapshot,
        1.0,
        200.0,
        200.0,
        [1.0, 0.0, 0.0, 1.0],
        Some("42"),
    );

    assert!(label_vertices.len() > plain_vertices.len());
    assert!(label_indices.len() > plain_indices.len());
}

#[test]
fn overlay_label_geometry_generates_background_and_digits() {
    let snapshot = BoxModelSnapshot {
        node_id: 7,
        parent_id: None,
        x: 0.0,
        y: 0.0,
        width: 20.0,
        height: 20.0,
        border_radius: 0.0,
        should_render: true,
    };
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    append_overlay_label_geometry(
        &mut vertices,
        &mut indices,
        &snapshot,
        "7",
        [0.0, 1.0, 0.0, 1.0],
        1.0,
        100.0,
        100.0,
    );

    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());
}

#[test]
fn reuse_overlay_geometry_scales_snapshot_coordinates_for_hidpi() {
    let snapshot = BoxModelSnapshot {
        node_id: 42,
        parent_id: None,
        x: 10.0,
        y: 20.0,
        width: 30.0,
        height: 40.0,
        border_radius: 0.0,
        should_render: true,
    };

    let (vertices, indices) =
        build_reuse_overlay_geometry(&snapshot, 2.0, 200.0, 200.0, [1.0, 0.0, 0.0, 1.0], None);

    assert!(!vertices.is_empty());
    assert!(!indices.is_empty());

    let expected_left = -0.8;
    let expected_top = 0.6;
    let min_x = vertices
        .iter()
        .map(|vertex| vertex.position[0])
        .fold(f32::INFINITY, f32::min);
    let max_y = vertices
        .iter()
        .map(|vertex| vertex.position[1])
        .fold(f32::NEG_INFINITY, f32::max);

    assert!((min_x - expected_left).abs() < 0.05);
    assert!((max_y - expected_top).abs() < 0.05);
}

#[test]
fn wheel_uses_only_topmost_hit_target_ancestry() {
    let mut background = Element::new(0.0, 0.0, 100.0, 100.0);
    let background_id = background.id();
    let mut background_style = Style::new();
    background_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    background.apply_style(background_style);
    background.add_child(Box::new(Element::new(0.0, 0.0, 100.0, 300.0)));
    place_root(&mut background, 100.0, 100.0);

    let mut foreground = Element::new(0.0, 0.0, 100.0, 100.0);
    foreground.add_child(Box::new(Element::new(0.0, 0.0, 100.0, 100.0)));
    place_root(&mut foreground, 100.0, 100.0);

    let mut viewport = Viewport::new();
    viewport.ui_roots.push(Box::new(background));
    viewport.ui_roots.push(Box::new(foreground));
    viewport.set_mouse_position_viewport(50.0, 50.0);

    assert_eq!(
        Viewport::find_scroll_handler_at_pointer(&viewport.ui_roots, 50.0, 50.0, 0.0, 24.0),
        None
    );
    assert!(!viewport.dispatch_mouse_wheel_event(0.0, 24.0));
    assert_eq!(
        get_scroll_offset_by_id(viewport.ui_roots[0].as_ref(), background_id),
        Some((0.0, 0.0))
    );
}

#[test]
fn wheel_bubbles_to_ancestor_when_child_is_at_scroll_limit() {
    let mut root = Element::new(0.0, 0.0, 100.0, 100.0);
    let root_id = root.id();
    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(root_style);

    let mut child = Element::new(0.0, 0.0, 100.0, 300.0);
    let child_id = child.id();
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    child.apply_style(child_style);
    child.add_child(Box::new(Element::new(0.0, 0.0, 100.0, 500.0)));

    root.add_child(Box::new(child));
    place_root(&mut root, 100.0, 100.0);

    assert_eq!(
        set_scroll_offset_by_id(&mut root, child_id, (0.0, 200.0)),
        true
    );
    assert_eq!(get_scroll_offset_by_id(&root, child_id), Some((0.0, 200.0)));
    assert_eq!(get_scroll_offset_by_id(&root, root_id), Some((0.0, 0.0)));

    let mut viewport = Viewport::new();
    viewport.ui_roots.push(Box::new(root));
    viewport.set_mouse_position_viewport(50.0, 50.0);

    assert_eq!(
        Viewport::find_scroll_handler_at_pointer(&viewport.ui_roots, 50.0, 50.0, 0.0, 24.0),
        Some((0, root_id))
    );
    assert!(viewport.dispatch_mouse_wheel_event(0.0, 24.0));
    assert_eq!(
        get_scroll_offset_by_id(viewport.ui_roots[0].as_ref(), child_id),
        Some((0.0, 200.0))
    );
    assert_eq!(
        get_scroll_offset_by_id(viewport.ui_roots[0].as_ref(), root_id),
        Some((0.0, 0.0))
    );
}

#[test]
fn hover_transform_transition_updates_live_element_in_viewport_flow() {
    let mut root = Element::new(0.0, 0.0, 240.0, 240.0);
    let mut child = Element::new(24.0, 24.0, 120.0, 80.0);
    let child_id = child.id();

    let mut style = Style::new();
    style.set_transform(Transform::default());
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::from(vec![Transition::new(
            TransitionProperty::Transform,
            1000,
        )])),
    );
    let mut hover = Style::new();
    hover.set_transform(Transform::new([Translate::x(Length::px(40.0))]));
    style.set_hover(hover);
    child.apply_style(style);
    root.add_child(Box::new(child));
    place_root(&mut root, 240.0, 240.0);

    let mut viewport = Viewport::new();
    viewport.ui_roots.push(Box::new(root));

    let hover_changed = Viewport::sync_hover_visual_only(
        &mut viewport.ui_roots,
        &mut viewport.input_state.hovered_node_id,
        Some(child_id),
    );
    assert!(hover_changed);

    let mut roots = std::mem::take(&mut viewport.ui_roots);
    let result = viewport.run_post_layout_transitions(&mut roots, 0.5, 0.5);
    assert!(result.redraw_changed);

    let child = element_by_id_mut(roots[0].as_mut(), child_id).expect("child should exist");
    assert_ne!(child.debug_transform(), &Transform::default());
    assert_ne!(
        child.debug_transform(),
        &Transform::new([Translate::x(Length::px(40.0))])
    );
    viewport.ui_roots = roots;
}

fn redraw_only_transform_root(toggle: &Binding<bool>) -> RsxNode {
    let translated = toggle.get();
    crate::ui::rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(80.0),
            transform: if translated {
                Transform::new([Translate::x(Length::px(48.0))])
            } else {
                Transform::default()
            },
        }} />
    }
}

#[test]
fn redraw_only_transform_sync_updates_live_tree_without_rebuild() {
    let toggle = Binding::new_with_dirty_state(false, UiDirtyState::REDRAW);
    let first = redraw_only_transform_root(&toggle);
    let second = {
        toggle.set(true);
        redraw_only_transform_root(&toggle)
    };

    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&first)
        .expect("initial render should succeed");
    let original_id = viewport.ui_roots[0].id();

    viewport
        .render_rsx(&second)
        .expect("redraw-only transform render should succeed");

    assert_eq!(viewport.ui_roots[0].id(), original_id);
    let element = element_by_id_mut(viewport.ui_roots[0].as_mut(), original_id)
        .expect("root element should remain live");
    assert_eq!(
        element.debug_transform(),
        &Transform::new([Translate::x(Length::px(48.0))])
    );
}

#[test]
fn viewport_registers_box_shadow_transition_channel() {
    let viewport = Viewport::new();
    assert!(
        viewport
            .transition_channels
            .contains(&CHANNEL_STYLE_BOX_SHADOW)
    );
}
