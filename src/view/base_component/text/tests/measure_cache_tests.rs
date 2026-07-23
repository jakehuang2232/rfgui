use super::*;

#[test]
fn shared_measure_cache_separates_wrap_and_nowrap_layouts() {
    let content = "shared cache wraps this sentence across several lines";
    let width = Some(82.0);
    let font_size = 14.0;
    let line_height = 1.25;
    let font_weight = 400;
    let align = InlineIfcAlignment::Left;
    let fonts: Vec<String> = Vec::new();

    let (_, wrap_height_first) = measure_text_size(
        content,
        width,
        true,
        font_size,
        line_height,
        font_weight,
        align,
        &fonts,
    );
    let (_, nowrap_height_second) = measure_text_size(
        content,
        width,
        false,
        font_size,
        line_height,
        font_weight,
        align,
        &fonts,
    );
    assert!(
        wrap_height_first > nowrap_height_second + 1.0,
        "nowrap measurement must not reuse the prior wrapped cache entry"
    );

    let content = "shared cache nowrap first still wraps later";
    let (_, nowrap_height_first) = measure_text_size(
        content,
        width,
        false,
        font_size,
        line_height,
        font_weight,
        align,
        &fonts,
    );
    let (_, wrap_height_second) = measure_text_size(
        content,
        width,
        true,
        font_size,
        line_height,
        font_weight,
        align,
        &fonts,
    );
    assert!(
        wrap_height_second > nowrap_height_first + 1.0,
        "wrapped measurement must not reuse the prior nowrap cache entry"
    );
}

#[test]
fn per_text_layout_cache_only_retains_recent_widths() {
    let mut text = Text::from_content("bounded per-node layout cache");
    for width in 20..40 {
        let _ = text.relayout_from_base(Some(width as f32), true);
    }

    assert_eq!(text.layout_cache.len(), 4);
}

#[test]
fn text_measure_clears_layout_dirty() {
    let mut a = arena();
    let mut text = Text::from_content("measured text");
    text.measure(
        LayoutConstraints {
            max_width: 200.0,
            max_height: 120.0,
            viewport_width: 200.0,
            viewport_height: 120.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(120.0),
        },
        &mut a,
    );

    assert!(!text.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
}

#[test]
fn clean_text_measure_with_same_constraints_skips_relayout() {
    let mut a = arena();
    let mut text = Text::from_content("cached text");
    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 120.0,
        viewport_width: 200.0,
        viewport_height: 120.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(120.0),
    };
    text.measure(constraints, &mut a);

    crate::view::base_component::reset_text_measure_profile();
    crate::view::base_component::set_text_measure_profile_enabled(true);
    text.measure(constraints, &mut a);
    crate::view::base_component::set_text_measure_profile_enabled(false);
    let profile = crate::view::base_component::take_text_measure_profile();

    assert_eq!(profile.relayout_from_base_calls, 0);
    assert_eq!(profile.measure_text_layout_calls, 0);
}
