use super::*;

#[test]
fn auto_size_uses_intrinsic_dimensions_when_loaded() {
    let mut image = Image::new_with_id(1, rgba_source(80, 40));
    image.apply_style(Style::new());
    let mut arena = new_test_arena();
    image.measure(
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
    assert_eq!(image.measured_size(), (80.0, 40.0));
}

#[test]
fn texture_bounds_apply_host_paint_offset_without_changing_size() {
    let element = Element::new(10.25, 20.75, 100.0, 50.0);
    let parent_paint_offset = [0.2, -0.3];
    let bounds = [18.25, 24.5, 80.0, 40.0];

    let adjusted = super::super::paint_adjusted_texture_bounds(&element, parent_paint_offset, bounds);

    let expected_dx = (10.25_f32 + parent_paint_offset[0]).round()
        - (10.25_f32 + parent_paint_offset[0])
        + parent_paint_offset[0];
    let expected_dy = (20.75_f32 + parent_paint_offset[1]).round()
        - (20.75_f32 + parent_paint_offset[1])
        + parent_paint_offset[1];
    assert!((adjusted[0] - (bounds[0] + expected_dx)).abs() < 0.001);
    assert!((adjusted[1] - (bounds[1] + expected_dy)).abs() < 0.001);
    assert_eq!(adjusted[2], bounds[2]);
    assert_eq!(adjusted[3], bounds[3]);
}

#[test]
fn transformed_image_wrapper_and_untransformed_media_expand_parent_surface_in_order() {
    let mut parent = Element::new_with_id(0x9200, 0.0, 0.0, 10.0, 10.0);
    parent.set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
        100.0, 0.0, 0.0,
    ))));
    let mut image = Image::new_with_id(0x9201, rgba_source(4, 2));
    image.element = Element::new_with_id(0x9201, 100.0, 2.0, 4.0, 2.0);
    image
        .element
        .set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
            -100.0, 0.0, 0.0,
        ))));

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _image_key = commit_child(&mut arena, parent_key, Box::new(image));
    let geometry = crate::view::test_support::get_element::<Element>(&arena, parent_key)
        .exact_transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
        .expect("Image explicitly supplies exact wrapper plus media coverage");
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
        ],
        "wrapper moves to x=0..4, but the sampled media still paints at x=100..104"
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(100, 80, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let outer_target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(outer_target);
    arena
        .with_element_taken(parent_key, |element, arena| {
            element.build(&mut graph, arena, ctx)
        })
        .expect("transformed parent containing Image");

    let composites =
        graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(composites.len(), 3);
    let wrapper = composites[0].test_snapshot();
    let media = composites[1].test_snapshot();
    let parent = composites[2].test_snapshot();
    assert!(wrapper.source_handle.is_some());
    assert!(media.source_handle.is_none(), "media is a sampled upload");
    assert_eq!(
        media.bounds_bits,
        [100.0, 2.0, 4.0, 2.0].map(f32::to_bits),
        "the media pass remains untransformed even though the embedded Element wrapper moves"
    );
    assert_eq!(wrapper.output_target, media.output_target);
    assert_eq!(media.output_target, parent.source_handle);
    assert_eq!(parent.output_target, outer_target.handle());
    assert_eq!(
        graph.declared_persistent_textures().count(),
        4,
        "parent and Image wrapper each own one color/depth surface pair"
    );
}

#[test]
fn contain_preserves_aspect_ratio_inside_subpixel_destination() {
    let (draw, uv) =
        super::super::compute_image_mapping(crate::view::ImageFit::Contain, 4.0, 2.0, 0.5, 0.5);
    assert_eq!(
        draw.map(f32::to_bits),
        [0.0, 0.125, 0.5, 0.25].map(f32::to_bits)
    );
    assert_eq!(uv.map(f32::to_bits), [0.0, 0.0, 4.0, 2.0].map(f32::to_bits));
    assert!(draw[0] >= 0.0 && draw[1] >= 0.0);
    assert!(draw[0] + draw[2] <= 0.5);
    assert!(draw[1] + draw[3] <= 0.5);
}

#[test]
fn invalid_image_mapping_is_empty() {
    assert_eq!(
        super::super::compute_image_mapping(crate::view::ImageFit::Fill, 4.0, 2.0, 0.0, 0.5),
        ([0.0; 4], [0.0; 4])
    );
}

#[test]
fn flex_distribution_does_not_feed_back_into_image_basis() {
    let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
    parent.apply_style(parent_style);

    let mut image = Image::new_with_id(2, rgba_source(20, 20));
    let mut image_style = Style::new();
    image_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    image.apply_style(image_style);

    let mut sibling = Element::new(0.0, 0.0, 120.0, 20.0);
    let mut sibling_style = Style::new();
    sibling_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
    sibling_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    sibling_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    sibling.apply_style(sibling_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let image_key = commit_child(&mut arena, parent_key, Box::new(image));
    let sibling_key = commit_child(&mut arena, parent_key, Box::new(sibling));

    let constraints = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };

    arena.with_element_taken(parent_key, |el, arena_ref| {
        el.measure(constraints, arena_ref);
        el.place(placement, arena_ref);
    });

    let image_snapshot = arena.get(image_key).unwrap().element.box_model_snapshot();
    let sibling_snapshot = arena.get(sibling_key).unwrap().element.box_model_snapshot();
    assert_eq!(image_snapshot.width, 14.285714);
    assert_eq!(sibling_snapshot.width, 85.71429);

    arena.with_element_taken(parent_key, |el, arena_ref| {
        if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
            e.mark_layout_dirty();
        }
        el.measure(constraints, arena_ref);
        el.place(placement, arena_ref);
    });
    let image_snapshot = arena.get(image_key).unwrap().element.box_model_snapshot();
    let sibling_snapshot = arena.get(sibling_key).unwrap().element.box_model_snapshot();
    assert_eq!(image_snapshot.width, 14.285714);
    assert_eq!(sibling_snapshot.width, 85.71429);
}
