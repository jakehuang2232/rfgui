use super::*;
use crate::style::{Angle, BackfaceVisibility, Rotate};

/// A cube face pivoting about the cube's centre 660px behind the screen,
/// seen through a 2200px perspective.
fn cube_face(turn: f32) -> Mat4 {
    css_perspective_matrix(2200.0)
        * Mat4::from_translation(Vec3::new(0.0, 0.0, -660.0))
        * Mat4::from_rotation_y(turn.to_radians())
        * Mat4::from_translation(Vec3::new(0.0, 0.0, 660.0))
}

#[test]
fn back_facing_follows_the_transformed_normal() {
    assert!(!is_back_facing(Mat4::IDENTITY));
    assert!(!is_back_facing(Mat4::from_rotation_y(89_f32.to_radians())));
    assert!(is_back_facing(Mat4::from_rotation_y(91_f32.to_radians())));
    assert!(is_back_facing(Mat4::from_rotation_x(std::f32::consts::PI)));
    // A 2D mirror keeps its normal.
    assert!(!is_back_facing(Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0))));
    // A singular matrix covers no area.
    assert!(!is_back_facing(Mat4::from_scale(Vec3::new(1.0, 1.0, 0.0))));
}

#[test]
fn perspective_shows_a_side_face_its_back_before_ninety_degrees() {
    // The viewer sits inside the cube's width, so past acos(660 / 2860),
    // about 76.7 degrees, the face is seen from behind.
    for side in [1.0, -1.0] {
        assert!(!is_back_facing(cube_face(70.0 * side)));
        assert!(is_back_facing(cube_face(80.0 * side)));
    }
}

fn face(id: u64, turn: f32, backface: Option<BackfaceVisibility>) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 200.0, 100.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.set_transform(Transform::new([Rotate::y(Angle::deg(turn))]));
    if let Some(backface) = backface {
        style.insert(
            PropertyId::BackfaceVisibility,
            ParsedValue::BackfaceVisibility(backface),
        );
    }
    element.apply_style(style);
    element
}

/// A root with one full-size turned child; returns what a press at the
/// centre lands on.
fn centre_hit(turn: f32, backface: Option<BackfaceVisibility>) -> (bool, bool) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0xbf_0100, 0.0, 0.0, 200.0, 100.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    let child = commit_child(&mut arena, root, Box::new(face(0xbf_0101, turn, backface)));
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 100.0,
            viewport_width: 200.0,
            viewport_height: 100.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(100.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 100.0,
            viewport_width: 200.0,
            viewport_height: 100.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(100.0),
        },
    );
    let culled =
        crate::view::test_support::get_element_mut::<Element>(&arena, child).is_backface_culled();
    let hit = crate::view::base_component::hit_test(&arena, root, 100.0, 50.0);
    (culled, hit == Some(child))
}

#[test]
fn hidden_backface_culls_and_stops_hits_only_while_turned_away() {
    let hidden = Some(BackfaceVisibility::Hidden);
    assert_eq!(centre_hit(180.0, hidden), (true, false));
    assert_eq!(centre_hit(30.0, hidden), (false, true));
    assert_eq!(
        centre_hit(180.0, Some(BackfaceVisibility::Visible)),
        (false, true)
    );
    assert_eq!(centre_hit(180.0, None), (false, true));
}
