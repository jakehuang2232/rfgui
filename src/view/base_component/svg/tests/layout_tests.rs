use super::*;

#[test]
fn transformed_svg_wrapper_and_untransformed_media_expand_parent_exact_bounds() {
    let mut parent = Element::new_with_id(0xA200, 0.0, 0.0, 10.0, 10.0);
    parent.set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
        100.0, 0.0, 0.0,
    ))));
    let mut svg = Svg::new_with_id(0xA201, simple_svg());
    svg.element = Element::new_with_id(0xA201, 100.0, 2.0, 4.0, 2.0);
    svg.element
        .set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
            -100.0, 0.0, 0.0,
        ))));

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _svg_key = commit_child(&mut arena, parent_key, Box::new(svg));
    let geometry = crate::view::test_support::get_element::<Element>(&arena, parent_key)
        .exact_transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
        .expect("Svg explicitly supplies exact wrapper plus media coverage");
    assert_eq!(
        [
            geometry.source_bounds.x.to_bits(),
            geometry.source_bounds.y.to_bits(),
            geometry.source_bounds.width.to_bits(),
            geometry.source_bounds.height.to_bits(),
        ],
        [
            0.0_f32.to_bits(),
            0.0_f32.to_bits(),
            104.0_f32.to_bits(),
            10.0_f32.to_bits(),
        ]
    );
}

#[test]
fn auto_size_uses_svg_intrinsic_dimensions_when_loaded() {
    let mut svg = Svg::new_with_id(1, simple_svg());
    svg.apply_style(Style::new());
    std::thread::sleep(std::time::Duration::from_millis(10));
    let mut arena = new_test_arena();
    svg.measure(
        LayoutConstraints {
            max_width: 500.0,
            max_height: 500.0,
            viewport_width: 500.0,
            viewport_height: 500.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        &mut arena,
    );
    assert_eq!(svg.measured_size(), (80.0, 40.0));
}
