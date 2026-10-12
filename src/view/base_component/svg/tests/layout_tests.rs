use super::*;

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
