use super::*;

#[test]
fn svg_delegates_retained_paint_properties_to_its_element() {
    let mut svg = Svg::new_with_id(0xa0ef, simple_svg());
    let mut style = Style::new();
    style.set_border(crate::style::Border::uniform(
        crate::style::Length::px(1.0),
        &Color::hex("#ffffff"),
    ));
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    svg.element.apply_style(style);
    svg.element.set_opacity(0.6);
    svg.element.set_border_radius(4.0);
    svg.element
        .set_box_shadows(vec![BoxShadow::new().offset(1.0)]);

    let properties = svg.retained_paint_properties();
    assert_eq!(properties, svg.element.retained_paint_properties());
    assert_eq!(properties.opacity.to_bits(), 0.6_f32.to_bits());
    assert!(properties.has_rounded_clip);
    assert!(properties.has_box_shadow);
    assert!(properties.has_border);
    assert!(properties.is_scroll_container);
}

#[test]
fn svg_wrapper_forwards_scrollbar_post_layout_lifecycle() {
    let mut svg = Svg::new_with_id(0xa0f0, simple_svg());
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    svg.element.apply_style(style);
    svg.element.layout_state.content_size = Size {
        width: 120.0,
        height: 300.0,
    };

    let now = crate::time::Instant::now();
    assert!(svg.set_hovered(true));
    assert!(svg.wants_animation_frame());
    assert!(
        svg.tick_post_layout_animation_frame(now)
            .contains(DirtyFlags::PAINT)
    );
    assert!(!svg.wants_animation_frame());

    assert!(svg.set_hovered(false));
    assert!(svg.wants_animation_frame());
    assert!(
        svg.tick_post_layout_animation_frame(now)
            .contains(DirtyFlags::PAINT)
    );
    assert!(svg.wants_animation_frame());
    assert!(
        svg.tick_post_layout_animation_frame(now + crate::time::Duration::from_millis(1_250),)
            .contains(DirtyFlags::PAINT)
    );
    assert!(!svg.wants_animation_frame());
}

#[test]
fn retained_paint_signature_covers_source_fit_sampling_and_raster_generation() {
    use std::hash::Hasher;

    let mut svg = Svg::new_with_id(1, simple_svg());
    assert!(svg.retained_paint_signature_is_complete());
    let initial = svg.retained_paint_signature();

    svg.set_fit(crate::view::ImageFit::Cover);
    let fit = svg.retained_paint_signature();
    assert_ne!(fit, initial);

    svg.set_sampling(crate::view::ImageSampling::Nearest);
    let sampling = svg.retained_paint_signature();
    assert_ne!(sampling, fit);

    svg.set_source(SvgSource::Content(
        r##"<svg width="40" height="20" xmlns="http://www.w3.org/2000/svg"><rect width="40" height="20" fill="#00ff00"/></svg>"##
            .to_string(),
    ));
    assert_ne!(svg.retained_paint_signature(), sampling);

    let pixels = std::sync::Arc::<[u8]>::from(vec![255; 16]);
    let first = ImageSnapshot::Ready(ReadyImage {
        sampled_texture_id: SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(1)),
        width: 2,
        height: 2,
        pixels: pixels.clone(),
        generation: 20,
    });
    let second = ImageSnapshot::Ready(ReadyImage {
        sampled_texture_id: SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(1)),
        width: 2,
        height: 2,
        pixels,
        generation: 21,
    });
    let mut first_hasher = std::collections::hash_map::DefaultHasher::new();
    super::super::hash_svg_raster_state(Some(7), Some((2, 2)), Some(&first), &mut first_hasher);
    let mut second_hasher = std::collections::hash_map::DefaultHasher::new();
    super::super::hash_svg_raster_state(Some(7), Some((2, 2)), Some(&second), &mut second_hasher);
    assert_ne!(first_hasher.finish(), second_hasher.finish());
}

#[test]
fn computed_style_consumer_syncs_svg_element_render_state() {
    let mut svg = Svg::new_with_id(2, simple_svg());
    let mut computed = ComputedStyle::default();
    computed.background_color = Color::rgb(30, 40, 50);
    computed.border_colors = EdgeInsets {
        top: Color::rgb(210, 0, 0),
        right: Color::rgb(0, 210, 0),
        bottom: Color::rgb(0, 0, 210),
        left: Color::rgb(210, 210, 0),
    };
    computed.opacity = 0.45;

    ComputedStyleConsumer::apply_computed_style(&mut svg, computed, None);

    let render_state = svg.element.debug_render_state();
    assert_eq!(render_state.background_rgba, [30, 40, 50, 255]);
    assert_eq!(render_state.border_top_rgba, [210, 0, 0, 255]);
    assert_eq!(render_state.border_right_rgba, [0, 210, 0, 255]);
    assert_eq!(render_state.border_bottom_rgba, [0, 0, 210, 255]);
    assert_eq!(render_state.border_left_rgba, [210, 210, 0, 255]);
    assert!((render_state.opacity - 0.45).abs() < 0.001);
}

#[test]
fn setters_mark_dirty_only_when_render_identity_changes() {
    let mut svg = Svg::new_with_id(7, simple_svg());
    svg.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
    svg.set_fit(crate::view::ImageFit::Contain);
    svg.set_sampling(crate::view::ImageSampling::Linear);
    svg.set_source(simple_svg());
    assert!(svg.local_dirty_flags().is_empty());

    svg.set_fit(crate::view::ImageFit::Cover);
    assert_eq!(
        svg.local_dirty_flags(),
        crate::view::base_component::DirtyFlags::PAINT
    );
    svg.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
    svg.set_sampling(crate::view::ImageSampling::Nearest);
    assert_eq!(
        svg.local_dirty_flags(),
        crate::view::base_component::DirtyFlags::PAINT
    );
    svg.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
    svg.set_source(SvgSource::Content(
        r##"<svg width="1" height="1" xmlns="http://www.w3.org/2000/svg"/>"##.into(),
    ));
    assert_eq!(
        svg.local_dirty_flags(),
        crate::view::base_component::DirtyFlags::ALL
    );
}
