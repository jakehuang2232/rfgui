use super::*;

#[test]
fn nested_transform_bounds_use_child_c0_quad_and_local_matrix_product() {
    let parent = Element::new_with_id(71_000, 0.25, 0.25, 10.0, 10.0);
    let child = Element::new_with_id(71_001, 12.25, 1.5, 4.0, 2.0);
    let mut arena = crate::view::test_support::new_test_arena();
    let parent_key = crate::view::test_support::commit_element(&mut arena, Box::new(parent));
    let child_key =
        crate::view::test_support::commit_child(&mut arena, parent_key, Box::new(child));

    let parent_matrix = Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0));
    // Exact T(30, 0) * Rz(90deg) oracle. Keeping the quarter turn
    // literal avoids trigonometric epsilon from weakening the bitwise
    // bounds golden below.
    let child_matrix = Mat4::from_cols_array(&[
        0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 30.0, 0.0, 0.0, 1.0,
    ]);
    crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
        .resolved_transform = Some(parent_matrix);
    crate::view::test_support::get_element_mut::<Element>(&arena, child_key)
        .resolved_transform = Some(child_matrix);

    let paint_offset = [0.2, -0.3];
    let parent_geometry = crate::view::test_support::get_element::<Element>(&arena, parent_key)
        .transform_surface_geometry_snapshot(&arena, paint_offset, None)
        .expect("finite nested transform geometry must be canonical");
    let exact_geometry = crate::view::test_support::get_element::<Element>(&arena, parent_key)
        .exact_transform_surface_geometry_snapshot(&arena, paint_offset, None)
        .expect("the built-in nested tree has exact retained coverage");
    assert!(
        parent_geometry.bitwise_eq(exact_geometry),
        "legacy build and retained C0 must share one canonical geometry algorithm"
    );
    assert_eq!(
        [
            parent_geometry.source_bounds.x.to_bits(),
            parent_geometry.source_bounds.y.to_bits(),
            parent_geometry.source_bounds.width.to_bits(),
            parent_geometry.source_bounds.height.to_bits(),
        ],
        [
            0.25_f32.to_bits(),
            0.25_f32.to_bits(),
            28.0_f32.to_bits(),
            15.5_f32.to_bits(),
        ],
        "parent source must union the transformed child C0 quad AABB, including child visual snap"
    );
    assert_eq!(
        parent_geometry
            .quad_positions
            .map(|point| point.map(f32::to_bits)),
        [
            [100.0_f32.to_bits(), 15.5_f32.to_bits()],
            [128.0_f32.to_bits(), 15.5_f32.to_bits()],
            [128.0_f32.to_bits(), 0.0_f32.to_bits()],
            [100.0_f32.to_bits(), 0.0_f32.to_bits()],
        ]
    );

    // Independent absolute-coordinate matrix oracle. The child transform
    // is local and the two texture composites naturally produce P * C.
    // Neither inverse(P) * C nor P * P * C is the legacy model.
    let corner = Vec3::new(12.25, 3.5, 0.0).extend(1.0);
    let expected = parent_matrix * child_matrix * corner;
    assert_eq!(
        expected.to_array().map(f32::to_bits),
        [
            126.5_f32.to_bits(),
            12.25_f32.to_bits(),
            0.0_f32.to_bits(),
            1.0_f32.to_bits(),
        ]
    );
    assert_ne!(
        expected.to_array().map(f32::to_bits),
        (parent_matrix.inverse() * child_matrix * corner)
            .to_array()
            .map(f32::to_bits)
    );
    assert_ne!(
        expected.to_array().map(f32::to_bits),
        (parent_matrix * parent_matrix * child_matrix * corner)
            .to_array()
            .map(f32::to_bits)
    );

    let scale_one = crate::view::base_component::texture_desc_for_logical_bounds(
        parent_geometry.source_bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let scale_two = crate::view::base_component::texture_desc_for_logical_bounds(
        parent_geometry.source_bounds,
        2.0,
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    assert_eq!((scale_one.width(), scale_one.height()), (29, 16));
    assert_eq!((scale_two.width(), scale_two.height()), (57, 32));
}

#[test]
fn untransformed_wrapper_propagates_fractional_snap_to_nested_transform_bounds() {
    let parent = Element::new_with_id(71_010, 0.25, 0.25, 10.0, 10.0);
    let wrapper = Element::new_with_id(71_011, 5.8, 2.8, 2.0, 2.0);
    let child = Element::new_with_id(71_012, 12.6, 1.6, 4.0, 2.0);
    let mut arena = crate::view::test_support::new_test_arena();
    let parent_key = crate::view::test_support::commit_element(&mut arena, Box::new(parent));
    let wrapper_key =
        crate::view::test_support::commit_child(&mut arena, parent_key, Box::new(wrapper));
    let child_key =
        crate::view::test_support::commit_child(&mut arena, wrapper_key, Box::new(child));

    crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
        .resolved_transform = Some(Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0)));
    crate::view::test_support::get_element_mut::<Element>(&arena, child_key)
        .resolved_transform = Some(Mat4::from_translation(Vec3::new(30.0, 0.0, 0.0)));

    let parent_geometry = crate::view::test_support::get_element::<Element>(&arena, parent_key)
        .transform_surface_geometry_snapshot(&arena, [0.2, -0.3], None)
        .expect("wrapper snap propagation must keep nested geometry canonical");
    assert_eq!(
        [
            parent_geometry.source_bounds.x.to_bits(),
            parent_geometry.source_bounds.y.to_bits(),
            parent_geometry.source_bounds.width.to_bits(),
            parent_geometry.source_bounds.height.to_bits(),
        ],
        [
            0.25_f32.to_bits(),
            0.25_f32.to_bits(),
            46.75_f32.to_bits(),
            10.0_f32.to_bits(),
        ],
        "the nested child quad must include the wrapper-adjusted (+0.4, +0.4) visual snap"
    );

    // Parent snap produces (-0.25, -0.25); the untransformed wrapper then
    // advances it to (+0.2, +0.2). That crosses the child's rounding
    // boundary and deliberately differs by one logical pixel from
    // incorrectly forwarding only the parent's offset.
    let child_geometry = crate::view::test_support::get_element::<Element>(&arena, child_key)
        .transform_surface_geometry_snapshot(&arena, [0.2, 0.2], None)
        .expect("nested child geometry");
    assert_eq!(
        child_geometry
            .quad_positions
            .map(|point| point.map(f32::to_bits)),
        [
            [43.0_f32.to_bits(), 4.0_f32.to_bits()],
            [47.0_f32.to_bits(), 4.0_f32.to_bits()],
            [47.0_f32.to_bits(), 2.0_f32.to_bits()],
            [43.0_f32.to_bits(), 2.0_f32.to_bits()],
        ]
    );
    let wrong_parent_only =
        crate::view::test_support::get_element::<Element>(&arena, child_key)
            .transform_surface_geometry_snapshot(&arena, [-0.25, -0.25], None)
            .expect("finite wrong-offset comparison fixture");
    assert_ne!(
        child_geometry
            .quad_positions
            .map(|point| point.map(f32::to_bits)),
        wrong_parent_only
            .quad_positions
            .map(|point| point.map(f32::to_bits))
    );
}

#[test]
fn nested_transform_graph_orders_child_surface_before_parent_composite() {
    let parent = Element::new_with_id(71_100, 0.25, 0.25, 10.0, 10.0);
    let child = Element::new_with_id(71_101, 12.25, 1.5, 4.0, 2.0);
    let mut arena = crate::view::test_support::new_test_arena();
    let parent_key = crate::view::test_support::commit_element(&mut arena, Box::new(parent));
    let child_key =
        crate::view::test_support::commit_child(&mut arena, parent_key, Box::new(child));
    crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
        .resolved_transform = Some(Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0)));
    crate::view::test_support::get_element_mut::<Element>(&arena, child_key)
        .resolved_transform = Some(Mat4::from_cols_array(&[
        0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 30.0, 0.0, 0.0, 1.0,
    ]));

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(100, 80, wgpu::TextureFormat::Bgra8Unorm, 2.0);
    let outer_target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(outer_target);
    arena
        .with_element_taken(parent_key, |element, arena| {
            element.build(&mut graph, arena, ctx)
        })
        .expect("nested transformed build");

    let clear_name = std::any::type_name::<crate::view::frame_graph::ClearPass>();
    let composite_name =
        std::any::type_name::<crate::view::render_pass::TextureCompositePass>();
    let surface_passes = graph
        .pass_descriptors()
        .into_iter()
        .filter_map(|descriptor| {
            (descriptor.name == clear_name)
                .then_some("clear")
                .or_else(|| (descriptor.name == composite_name).then_some("composite"))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        surface_passes,
        ["clear", "clear", "composite", "composite"],
        "parent clear -> child clear -> child-to-parent composite -> parent-to-output composite"
    );

    let clears = graph.test_graphics_passes::<crate::view::frame_graph::ClearPass>();
    let composites =
        graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(clears.len(), 2);
    assert_eq!(composites.len(), 2);
    let parent_clear = clears[0].test_snapshot();
    let child_clear = clears[1].test_snapshot();
    let child_composite = composites[0].test_snapshot();
    let parent_composite = composites[1].test_snapshot();
    assert_eq!(child_composite.source_handle, child_clear.output_target);
    assert_eq!(child_composite.output_target, parent_clear.output_target);
    assert_eq!(parent_composite.source_handle, parent_clear.output_target);
    assert_eq!(parent_composite.output_target, outer_target.handle());

    let declared = graph
        .declared_persistent_textures()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(declared.len(), 4, "two color/depth surface pairs");
    let parent_color = declared
        .get(&crate::view::base_component::transformed_layer_stable_key(
            71_100,
        ))
        .expect("parent transformed color surface");
    let child_color = declared
        .get(&crate::view::base_component::transformed_layer_stable_key(
            71_101,
        ))
        .expect("child transformed color surface");
    assert_eq!((parent_color.width(), parent_color.height()), (57, 32));
    assert_eq!(parent_color.origin(), (0, 0));
    assert_eq!((child_color.width(), child_color.height()), (9, 4));
    assert_eq!(child_color.origin(), (24, 3));
}

#[test]
fn invalid_nested_projective_geometry_fails_parent_surface_closed() {
    let parent = Element::new_with_id(71_200, 0.0, 0.0, 10.0, 10.0);
    let child = Element::new_with_id(71_201, 12.0, 1.0, 4.0, 2.0);
    let mut arena = crate::view::test_support::new_test_arena();
    let parent_key = crate::view::test_support::commit_element(&mut arena, Box::new(parent));
    let child_key =
        crate::view::test_support::commit_child(&mut arena, parent_key, Box::new(child));
    crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
        .resolved_transform = Some(Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)));
    crate::view::test_support::get_element_mut::<Element>(&arena, child_key)
        .resolved_transform = Some(Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ]));

    assert!(
        crate::view::test_support::get_element::<Element>(&arena, parent_key)
            .transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
            .is_none(),
        "child projective W=0 must invalidate parent source coverage"
    );

    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(100, 80, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    arena
        .with_element_taken(parent_key, |element, arena| {
            element.build(&mut graph, arena, ctx)
        })
        .expect("invalid nested geometry must fail closed without panicking");
    assert!(graph.pass_descriptors().is_empty());
    assert!(graph.declared_persistent_textures().next().is_none());
}

#[test]
fn legacy_invalid_transform_geometry_emits_no_surface_or_composite_pass() {
    let element = Element::new_with_id(70_000, 0.0, 0.0, 20.0, 10.0);
    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(&mut arena, Box::new(element));
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 80.0,
            viewport_width: 100.0,
            viewport_height: 80.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(80.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 100.0,
            available_height: 80.0,
            viewport_width: 100.0,
            viewport_height: 80.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(80.0),
        },
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, root).resolved_transform =
        Some(Mat4::from_cols_array(&[
            f32::NAN,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ]));

    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(100, 80, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .expect("invalid transform build must fail closed without panicking");

    assert!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .is_empty(),
        "invalid geometry must never reach a texture composite"
    );
    assert!(
        graph.declared_persistent_textures().next().is_none(),
        "invalid geometry must not allocate a retained transform surface"
    );
}

#[test]
fn transformed_build_declares_exact_color_depth_descriptor_pair_at_scale_two() {
    let mut element = Element::new_with_id(70_010, 3.25, 2.5, 4.0, 2.0);
    let mut style = crate::style::Style::new();
    style.insert(
        crate::style::PropertyId::BackgroundColor,
        crate::style::ParsedValue::color_like(crate::style::Color::hex("#336699")),
    );
    style.set_transform(crate::style::Transform::new([crate::style::Translate::x(
        crate::style::Length::px(1.0),
    )]));
    element.apply_style(style);
    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(&mut arena, Box::new(element));
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 40.0,
            max_height: 30.0,
            viewport_width: 40.0,
            viewport_height: 30.0,
            percent_base_width: Some(40.0),
            percent_base_height: Some(30.0),
        },
        LayoutPlacement {
            parent_x: 3.25,
            parent_y: 2.5,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 40.0,
            available_height: 30.0,
            viewport_width: 40.0,
            viewport_height: 30.0,
            percent_base_width: Some(40.0),
            percent_base_height: Some(30.0),
        },
    );
    let geometry = crate::view::test_support::get_element::<Element>(&arena, root)
        .transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
        .expect("positive transformed fixture");
    assert_eq!(
        [
            geometry.source_bounds.x.to_bits(),
            geometry.source_bounds.y.to_bits(),
            geometry.source_bounds.width.to_bits(),
            geometry.source_bounds.height.to_bits(),
        ],
        [
            6.5_f32.to_bits(),
            5.0_f32.to_bits(),
            4.0_f32.to_bits(),
            2.0_f32.to_bits()
        ],
        "source bounds are a hard-coded legacy oracle, not descriptor-helper output"
    );

    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(40, 30, wgpu::TextureFormat::Bgra8Unorm, 2.0);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .expect("transformed build");

    let color_key = crate::view::frame_graph::PersistentTextureKey::retained(
        crate::view::frame_graph::RetainedTextureRole::TransformedColor,
        70_010,
    );
    let depth_key = crate::view::frame_graph::PersistentTextureKey::retained(
        crate::view::frame_graph::RetainedTextureRole::TransformedDepthStencil,
        70_010,
    );
    assert_eq!(color_key.depth_stencil(), Some(depth_key));
    let declared = graph
        .declared_persistent_textures()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(declared.len(), 2);
    let color = declared
        .get(&color_key)
        .expect("transformed color role/key");
    let depth = declared
        .get(&depth_key)
        .expect("transformed depth role/key");

    assert_eq!((color.width(), color.height()), (8, 4));
    assert_eq!(color.origin(), (13, 10));
    assert_eq!(color.format(), wgpu::TextureFormat::Bgra8Unorm);
    assert_eq!(color.dimension(), wgpu::TextureDimension::D2);
    assert_eq!(color.sample_count(), 1);
    assert_eq!(
        color.usage(),
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
    );

    assert_eq!((depth.width(), depth.height()), (8, 4));
    assert_eq!(depth.origin(), (0, 0));
    assert_eq!(depth.format(), wgpu::TextureFormat::Depth24PlusStencil8);
    assert_eq!(depth.dimension(), wgpu::TextureDimension::D2);
    assert_eq!(depth.sample_count(), 1);
    assert_eq!(depth.usage(), wgpu::TextureUsages::RENDER_ATTACHMENT);
    assert_eq!(color.width(), depth.width());
    assert_eq!(color.height(), depth.height());
    assert_eq!(color.dimension(), depth.dimension());
    assert_eq!(color.sample_count(), depth.sample_count());
}

#[test]
fn legacy_transform_surface_freezes_raster_then_composite_contract() {
    let mut root = Element::new_with_id(70_001, -10.25, 5.5, 20.0, 10.0);
    let mut root_style = crate::style::Style::new();
    root_style.insert(
        crate::style::PropertyId::Layout,
        crate::style::ParsedValue::Layout(crate::style::Layout::Grid),
    );
    root_style.insert(
        crate::style::PropertyId::BackgroundColor,
        crate::style::ParsedValue::color_like(crate::style::Color::hex("#224466")),
    );
    root_style.set_transform(crate::style::Transform::new([crate::style::Rotate::z(
        crate::style::Angle::deg(90.0),
    )]));
    root_style.set_transform_origin(crate::style::TransformOrigin::center());
    root.apply_style(root_style);

    let mut child = Element::new_with_id(70_002, 0.0, 0.0, 60.0, 30.0);
    let mut child_style = crate::style::Style::new();
    child_style.insert(
        crate::style::PropertyId::BackgroundColor,
        crate::style::ParsedValue::color_like(crate::style::Color::hex("#aa3300")),
    );
    child.apply_style(child_style);

    let mut arena = crate::view::test_support::new_test_arena();
    let root_key = crate::view::test_support::commit_element(&mut arena, Box::new(root));
    let _child_key =
        crate::view::test_support::commit_child(&mut arena, root_key, Box::new(child));
    crate::view::test_support::measure_and_place(
        &mut arena,
        root_key,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 80.0,
            viewport_width: 100.0,
            viewport_height: 80.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(80.0),
        },
        LayoutPlacement {
            parent_x: -10.25,
            parent_y: 5.5,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 100.0,
            available_height: 80.0,
            viewport_width: 100.0,
            viewport_height: 80.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(80.0),
        },
    );

    let paint_offset = [0.2, -0.3];
    let outer_scissor = [3, 4, 50, 60];
    let geometry = {
        let root = crate::view::test_support::get_element::<Element>(&arena, root_key);
        let own_bounds = root.untransformed_paint_bounds();
        let owner_position = [
            root.layout_state.layout_position.x,
            root.layout_state.layout_position.y,
        ];
        let raster_paint_offset = crate::view::base_component::paint_offset_after_owner_snap(
            owner_position,
            [0.0, 0.0],
        )
        .expect("finite zero-host owner placement");
        let composite_paint_offset =
            crate::view::base_component::paint_offset_after_owner_snap(
                owner_position,
                paint_offset,
            )
            .expect("finite active owner placement");
        let geometry = root
            .transform_surface_geometry_snapshot_with_placement(
                &arena,
                raster_paint_offset,
                composite_paint_offset,
                Some(outer_scissor),
            )
            .expect("measured transformed root must expose legacy surface geometry");
        assert!(
            geometry.source_bounds.width > own_bounds.width
                || geometry.source_bounds.height > own_bounds.height,
            "descendant paint must expand the retained source surface"
        );

        let snap = root.box_model_snapshot();
        let center = Vec3::new(snap.x + snap.width * 0.5, snap.y + snap.height * 0.5, 0.0);
        let transformed_center = geometry.viewport_transform * center.extend(1.0);
        assert!((transformed_center.x - center.x).abs() < 0.001);
        assert!((transformed_center.y - center.y).abs() < 0.001);
        geometry
    };

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(100, 80, wgpu::TextureFormat::Bgra8Unorm, 2.0);
    ctx.translate_paint_offset(paint_offset[0], paint_offset[1]);
    ctx.push_scissor_rect(Some(outer_scissor));
    let parent_target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(parent_target);
    let build_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    arena
        .with_element_taken(root_key, |root, arena| {
            root.build(&mut graph, arena, build_ctx)
        })
        .expect("legacy transformed root must build");

    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name)
        .collect::<Vec<_>>();
    assert_eq!(
        pass_names.first().copied(),
        Some(std::any::type_name::<crate::view::frame_graph::ClearPass>()),
        "surface color/depth clear must precede every subtree raster pass"
    );
    assert_eq!(
        pass_names.last().copied(),
        Some(std::any::type_name::<
            crate::view::render_pass::TextureCompositePass,
        >()),
        "transformed texture composite must remain the final surface pass"
    );
    let composite_index = pass_names.len() - 1;
    assert!(
        pass_names[..composite_index].iter().any(|name| {
            *name
                == std::any::type_name::<
                    crate::view::render_pass::draw_rect_pass::DrawRectPass,
                >()
                || *name
                    == std::any::type_name::<
                        crate::view::render_pass::draw_rect_pass::OpaqueRectPass,
                    >()
        }),
        "subtree raster paint must stay between clear and composite"
    );

    let composites =
        graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(composites.len(), 1);
    let composite = composites[0].test_snapshot();
    assert_eq!(
        composite.bounds_bits,
        [
            geometry.visual_bounds.x.to_bits(),
            geometry.visual_bounds.y.to_bits(),
            geometry.visual_bounds.width.to_bits(),
            geometry.visual_bounds.height.to_bits(),
        ]
    );
    assert_eq!(
        composite.quad_position_bits,
        Some(geometry.quad_positions.map(|point| point.map(f32::to_bits)))
    );
    assert_eq!(
        composite.uv_bounds_bits,
        Some([0.0, 11.0, 60.0, 30.0].map(f32::to_bits))
    );
    assert_eq!(composite.explicit_scissor_rect, Some(outer_scissor));
    assert!(composite.source_is_premultiplied);
    assert_eq!(composite.opacity_bits, 1.0_f32.to_bits());

    let transformed_key = crate::view::base_component::transformed_layer_stable_key(70_001);
    let (_, transformed_desc) = graph
        .declared_persistent_textures()
        .find(|(key, _)| *key == transformed_key)
        .expect("legacy transform must declare its persistent color surface");
    assert_eq!(
        [
            geometry.source_bounds.x.to_bits(),
            geometry.source_bounds.y.to_bits(),
            geometry.source_bounds.width.to_bits(),
            geometry.source_bounds.height.to_bits(),
        ],
        [
            (-21.0_f32).to_bits(),
            11.0_f32.to_bits(),
            60.0_f32.to_bits(),
            30.0_f32.to_bits(),
        ],
        "detached source coverage must retain the zero-host owner snap"
    );
    assert_eq!(
        (transformed_desc.width(), transformed_desc.height()),
        (120, 60)
    );
    assert_eq!(transformed_desc.origin(), (0, 22));
    assert_eq!(
        composite.uv_bounds_bits,
        Some([
            0.0_f32.to_bits(),
            11.0_f32.to_bits(),
            60.0_f32.to_bits(),
            30.0_f32.to_bits(),
        ])
    );
    // Independent scale-2 oracle: full logical X coverage is
    // floor(-21.0 * 2)=-42 through ceil(39.0 * 2)=78, i.e. 120 pixels.
    // The raster is rebased by +21 logical px; the receiver quad above
    // retains the original geometry. No negative source texels are lost.
    assert_eq!(78_i32 - (-42_i32), 120);
    assert_eq!(transformed_desc.width(), 120);
}
