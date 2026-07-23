use super::*;

#[test]
fn image_delegates_retained_paint_properties_to_its_element() {
    let mut image = Image::new_with_id(0x90ef, rgba_source(1, 1));
    let mut style = Style::new();
    style.set_border(crate::style::Border::uniform(
        Length::px(1.0),
        &Color::hex("#ffffff"),
    ));
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    image.element.apply_style(style);
    image.element.set_opacity(0.4);
    image.element.set_border_radius(3.0);
    image
        .element
        .set_box_shadows(vec![BoxShadow::new().offset(1.0)]);

    let properties = image.retained_paint_properties();
    assert_eq!(properties, image.element.retained_paint_properties());
    assert_eq!(properties.opacity.to_bits(), 0.4_f32.to_bits());
    assert!(properties.has_rounded_clip);
    assert!(properties.has_box_shadow);
    assert!(properties.has_border);
    assert!(properties.is_scroll_container);
}

#[test]
fn image_wrapper_forwards_scrollbar_post_layout_lifecycle() {
    let mut image = Image::new_with_id(0x90f0, rgba_source(1, 1));
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    image.element.apply_style(style);
    image.element.layout_state.content_size = Size {
        width: 120.0,
        height: 300.0,
    };

    let now = crate::time::Instant::now();
    assert!(image.set_hovered(true));
    assert!(image.wants_animation_frame());
    assert!(
        image
            .tick_post_layout_animation_frame(now)
            .contains(DirtyFlags::PAINT)
    );
    assert!(!image.wants_animation_frame());

    assert!(image.set_hovered(false));
    assert!(image.wants_animation_frame());
    assert!(
        image
            .tick_post_layout_animation_frame(now)
            .contains(DirtyFlags::PAINT)
    );
    assert!(image.wants_animation_frame());
    assert!(
        image
            .tick_post_layout_animation_frame(now + crate::time::Duration::from_millis(1_250),)
            .contains(DirtyFlags::PAINT)
    );
    assert!(!image.wants_animation_frame());
}

#[test]
fn retained_paint_signature_covers_source_fit_sampling_and_resource_generation() {
    use std::hash::Hasher;

    let mut image = Image::new_with_id(1, rgba_source(8, 4));
    assert!(image.retained_paint_signature_is_complete());
    let initial = image.retained_paint_signature();

    image.set_fit(crate::view::ImageFit::Cover);
    let fit = image.retained_paint_signature();
    assert_ne!(fit, initial);

    image.set_sampling(crate::view::ImageSampling::Nearest);
    let sampling = image.retained_paint_signature();
    assert_ne!(sampling, fit);

    image.set_source(rgba_source(9, 4));
    assert_ne!(image.retained_paint_signature(), sampling);

    let pixels = std::sync::Arc::<[u8]>::from(vec![255; 16]);
    let first = ImageSnapshot::Ready(ReadyImage {
        sampled_texture_id: SampledTextureId::Image(ImageAssetId::for_test(1)),
        width: 2,
        height: 2,
        pixels: pixels.clone(),
        generation: 10,
    });
    let second = ImageSnapshot::Ready(ReadyImage {
        sampled_texture_id: SampledTextureId::Image(ImageAssetId::for_test(1)),
        width: 2,
        height: 2,
        pixels,
        generation: 11,
    });
    let mut first_hasher = std::collections::hash_map::DefaultHasher::new();
    super::super::hash_image_snapshot(Some(&first), &mut first_hasher);
    let mut second_hasher = std::collections::hash_map::DefaultHasher::new();
    super::super::hash_image_snapshot(Some(&second), &mut second_hasher);
    assert_ne!(first_hasher.finish(), second_hasher.finish());
}

#[test]
fn image_setters_mark_only_the_required_dirty_scope_and_same_source_is_noop() {
    let pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([255_u8; 4]);
    let source = ImageSource::Rgba {
        width: 1,
        height: 1,
        pixels: pixels.clone(),
    };
    let mut image = Image::new_with_id(30, source.clone());
    image.clear_local_dirty_flags(DirtyFlags::ALL);

    image.set_fit(crate::view::ImageFit::Contain);
    image.set_sampling(crate::view::ImageSampling::Linear);
    image.set_source(source);
    assert!(image.local_dirty_flags().is_empty());

    image.set_fit(crate::view::ImageFit::Cover);
    assert_eq!(image.local_dirty_flags(), DirtyFlags::PAINT);
    image.clear_local_dirty_flags(DirtyFlags::ALL);
    image.set_sampling(crate::view::ImageSampling::Nearest);
    assert_eq!(image.local_dirty_flags(), DirtyFlags::PAINT);
    image.clear_local_dirty_flags(DirtyFlags::ALL);

    image.set_source(ImageSource::Rgba {
        width: 1,
        height: 1,
        pixels: std::sync::Arc::from([255_u8; 4]),
    });
    assert_eq!(image.local_dirty_flags(), DirtyFlags::ALL);
    assert!(image.frozen_snapshot.is_none());
}

#[test]
fn computed_style_consumer_syncs_image_element_render_state() {
    let mut image = Image::new_with_id(2, rgba_source(80, 40));
    let mut computed = ComputedStyle::default();
    computed.background_color = Color::rgb(20, 30, 40);
    computed.border_colors = EdgeInsets {
        top: Color::rgb(200, 0, 0),
        right: Color::rgb(0, 200, 0),
        bottom: Color::rgb(0, 0, 200),
        left: Color::rgb(200, 200, 0),
    };
    computed.opacity = 0.4;

    ComputedStyleConsumer::apply_computed_style(&mut image, computed, None);

    let render_state = image.element.debug_render_state();
    assert_eq!(render_state.background_rgba, [20, 30, 40, 255]);
    assert_eq!(render_state.border_top_rgba, [200, 0, 0, 255]);
    assert_eq!(render_state.border_right_rgba, [0, 200, 0, 255]);
    assert_eq!(render_state.border_bottom_rgba, [0, 0, 200, 255]);
    assert_eq!(render_state.border_left_rgba, [200, 200, 0, 255]);
    assert!((render_state.opacity - 0.4).abs() < 0.001);
}
