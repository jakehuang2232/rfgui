use super::*;

fn styled_element(id: u64, x: f32, y: f32, width: f32, height: f32) -> Element {
    let mut element = Element::new_with_id(id, x, y, width, height);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(180, 60, 20)),
    );
    element.apply_style(style);
    element
}

fn placed_transform_tree(
    nested: bool,
) -> (
    crate::view::node_arena::NodeArena,
    crate::view::node_arena::NodeKey,
) {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(styled_element(0xc3_f100, 0.25, 0.25, 48.0, 32.0)),
    );
    let transform = commit_child(
        &mut arena,
        root,
        Box::new(styled_element(0xc3_f101, 4.25, 1.5, 28.0, 20.0)),
    );
    let leaf = if nested {
        commit_child(
            &mut arena,
            transform,
            Box::new(styled_element(0xc3_f102, 5.0, 1.75, 12.0, 8.0)),
        )
    } else {
        transform
    };
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.25,
            parent_y: 0.25,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, transform)
        .set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(6.0, 0.0, 0.0))));
    if nested {
        crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
            .set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
                0.0, 3.0, 0.0,
            ))));
    }
    (arena, root)
}

fn transform_graph(
    nested: bool,
    host_offset: [f32; 2],
) -> (
    Vec<crate::view::render_pass::draw_rect_pass::RectPassTestSnapshot>,
    Vec<crate::view::render_pass::texture_composite_pass::TextureCompositePassTestSnapshot>,
) {
    let (mut arena, root) = placed_transform_tree(nested);
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.set_paint_offset(host_offset);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .expect("transform fixture root exists");
    let rects = graph.test_rect_pass_snapshots();
    let composites = graph
        .test_graphics_passes::<
            crate::view::render_pass::texture_composite_pass::TextureCompositePass,
        >()
        .into_iter()
        .map(|pass| pass.test_snapshot())
        .collect();
    (rects, composites)
}

fn detached_rects(
    rects: &[crate::view::render_pass::draw_rect_pass::RectPassTestSnapshot],
    composites: &[crate::view::render_pass::texture_composite_pass::TextureCompositePassTestSnapshot],
) -> Vec<crate::view::render_pass::draw_rect_pass::RectPassTestSnapshot> {
    let detached_targets = composites
        .iter()
        .filter_map(|composite| composite.source_handle)
        .collect::<Vec<_>>();
    rects
        .iter()
        .filter(|rect| {
            rect.output_target
                .is_some_and(|target| detached_targets.contains(&target))
        })
        .cloned()
        .collect()
}

fn assert_root_composite_translated_by_independent_host_snap(
    zero: &crate::view::render_pass::texture_composite_pass::TextureCompositePassTestSnapshot,
    offset: &crate::view::render_pass::texture_composite_pass::TextureCompositePassTestSnapshot,
) {
    let translated = |bits: u32, delta: f32| (f32::from_bits(bits) + delta).to_bits();
    assert_eq!(offset.bounds_bits[2..], zero.bounds_bits[2..]);
    assert_eq!(offset.uv_bounds_bits, zero.uv_bounds_bits);
    let zero_quad = zero.quad_position_bits.expect("transform composite quad");
    let offset_quad = offset.quad_position_bits.expect("transform composite quad");
    for (zero_point, offset_point) in zero_quad.into_iter().zip(offset_quad) {
        // Replaying the fixture's authored root [0.5, 0.5] and Transform
        // [4.75, 2.0] positions gives an independent zero-host versus
        // [3.5, -2.25]-host owner-snap delta of [3, -4].
        assert_eq!(offset_point[0], translated(zero_point[0], 3.0));
        assert_eq!(offset_point[1], translated(zero_point[1], -4.0));
    }
}

#[test]
fn owner_projection_preserves_the_preexisting_active_snap_bit_for_bit() {
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let parent = [3.5, -2.25];
    let owner = [4.75, 1.5];
    ctx.set_paint_offset(parent);
    let expected = crate::view::base_component::paint_offset_after_owner_snap(owner, parent)
        .expect("finite fixture");
    ctx.snap_owner_paint_offset(owner);
    let projection = ctx.owner_paint_offset_projection();
    assert_eq!(
        projection.active.map(f32::to_bits),
        expected.map(f32::to_bits),
        "adding the host-neutral derivation cannot change the preexisting active snap"
    );
}

#[test]
fn scene_root_host_placement_does_not_enter_transform_detached_raster() {
    let (zero_rects, zero_composites) = transform_graph(false, [0.0, 0.0]);
    let (offset_rects, offset_composites) = transform_graph(false, [3.5, -2.25]);
    assert_eq!(
        detached_rects(&zero_rects, &zero_composites),
        detached_rects(&offset_rects, &offset_composites),
        "host placement belongs to the final composite, not detached raster content"
    );
    assert_eq!(zero_composites.len(), 1);
    assert_eq!(offset_composites.len(), 1);
    assert_root_composite_translated_by_independent_host_snap(
        &zero_composites[0],
        &offset_composites[0],
    );
}

#[test]
fn nested_transform_keeps_surface_local_fractional_snap_out_of_host_placement() {
    let (zero_rects, zero_composites) = transform_graph(true, [0.0, 0.0]);
    let (offset_rects, offset_composites) = transform_graph(true, [3.5, -2.25]);
    assert_eq!(zero_composites.len(), 2);
    assert_eq!(offset_composites.len(), 2);
    assert_eq!(
        detached_rects(&zero_rects, &zero_composites),
        detached_rects(&offset_rects, &offset_composites),
        "both detached rasters must retain the zero-host owner snap chain"
    );
    assert_eq!(
        zero_composites[0], offset_composites[0],
        "the inner composite remains surface-local while only the root composite moves"
    );
    assert_root_composite_translated_by_independent_host_snap(
        &zero_composites[1],
        &offset_composites[1],
    );
}
