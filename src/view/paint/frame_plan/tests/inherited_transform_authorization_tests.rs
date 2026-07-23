use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn image_and_svg_inherited_transform_are_authorized_only_by_surface_recording() {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='12'><rect width='16' height='12' fill='#22c55e'/></svg>";
    let mut root = Element::new_with_id(0xc1_1000, 0.0, 0.0, 48.0, 32.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.set_transform(Transform::new([Rotate::z(Angle::deg(8.0))]));
    root.apply_style(root_style);

    let mut image = Image::new_with_id(
        0xc1_1001,
        ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255_u8, 255, 255, 255]),
        },
    );
    let mut media_style = Style::new();
    media_style.insert(
        PropertyId::Width,
        ParsedValue::Length(crate::style::Length::px(16.0)),
    );
    media_style.insert(
        PropertyId::Height,
        ParsedValue::Length(crate::style::Length::px(12.0)),
    );
    image.apply_style(media_style.clone());
    let mut svg = Svg::new_with_id(0xc1_1002, SvgSource::Content(SVG.into()));
    svg.apply_style(media_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root));
    let image = commit_child(&mut arena, root, Box::new(image));
    let svg = commit_child(&mut arena, root, Box::new(svg));
    let constraints = LayoutConstraints {
        max_width: 160.0,
        max_height: 120.0,
        viewport_width: 160.0,
        viewport_height: 120.0,
        percent_base_width: Some(160.0),
        percent_base_height: Some(120.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 160.0,
        available_height: 120.0,
        viewport_width: 160.0,
        viewport_height: 120.0,
        percent_base_width: Some(160.0),
        percent_base_height: Some(120.0),
    };
    measure_and_place(&mut arena, root, constraints, placement);
    arena
        .get_mut(svg)
        .expect("svg")
        .element
        .as_any_mut()
        .downcast_mut::<Svg>()
        .expect("Svg host")
        .prepare_content_paint_for_test(SVG, (16.0, 12.0), 1.0)
        .expect("prepare exact SVG paint");

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let transform = TransformNodeId(root);
    for owner in [image, svg] {
        let property = properties.paint_state_for(owner).expect("property state");
        let local = generations
            .local_generations_for(owner)
            .expect("paint generations");
        let revision = super::super::super::PaintContentRevision {
            self_paint_revision: local.self_paint_revision,
            composite_revision: local.composite_revision,
            topology_revision: local.topology_revision,
        };
        let node = arena.get(owner).expect("media child");
        assert!(
            node.element
                .record_shadow_paint_metadata(
                    owner,
                    property,
                    revision,
                    &arena,
                    super::super::super::PaintRecordingContext::default(),
                )
                .is_none(),
            "ordinary metadata path must reject inherited transform"
        );
        let context = super::super::super::PaintRecordingContext {
            recording_owner: Some(owner),
            recording_owner_stable_id: Some(node.element.stable_id()),
            transform_surface: Some(
                PaintTransformSurfaceWitness::canonical_root(root).for_target(owner),
            ),
            ..Default::default()
        };
        assert!(context.authorizes_transform_surface_owner(Some(transform)));
        assert!(
            node.element
                .record_shadow_paint_metadata(owner, property, revision, &arena, context,)
                .is_some(),
            "surface-scoped canonical owner witness must admit inherited transform"
        );
    }

    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("surface planner must own Image/SVG inherited transform");
    let PaintPlanStep::RetainedSurface(surface) = &plan.steps[0] else {
        panic!("one retained surface")
    };
    let roles = only_span(surface)
        .artifact
        .chunks
        .iter()
        .map(|chunk| chunk.id.role)
        .collect::<Vec<_>>();
    assert!(roles.contains(&super::super::super::PaintChunkRole::ImageContent));
    assert!(roles.contains(&super::super::super::PaintChunkRole::SvgContent));

    let mut graph = FrameGraph::new();
    assert!(
        super::super::super::try_compile_artifact(
            &only_span(surface).artifact,
            &mut graph,
            UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        )
        .is_err()
    );
    assert!(graph.pass_descriptors().is_empty());

    let mut legacy_graph = FrameGraph::new();
    let (legacy_ctx, legacy_parent) =
        parent_context_with_clear(&mut legacy_graph, 160, 120, 1.0);
    arena
        .with_element_taken(root, |element, arena| {
            element.build(&mut legacy_graph, arena, legacy_ctx)
        })
        .expect("legacy Image/SVG transformed build");
    legacy_graph
        .add_texture_sink(
            &legacy_parent,
            crate::view::frame_graph::ExternalSinkKind::DebugCapture,
        )
        .expect("legacy Image/SVG sink");

    let mut forced_graph = FrameGraph::new();
    let (forced_ctx, forced_parent) =
        parent_context_with_clear(&mut forced_graph, 160, 120, 1.0);
    let mut viewport = Viewport::new();
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &plan,
        &mut forced_graph,
        forced_ctx,
    )
    .expect("forced Image/SVG transformed build");
    forced_graph
        .add_texture_sink(
            &forced_parent,
            crate::view::frame_graph::ExternalSinkKind::DebugCapture,
        )
        .expect("forced Image/SVG sink");
    assert_eq!(
        forced_graph
            .test_compile_snapshot()
            .expect("strict forced Image/SVG snapshot"),
        legacy_graph
            .test_compile_snapshot()
            .expect("strict legacy Image/SVG snapshot")
    );

    arena
        .get_mut(image)
        .expect("image")
        .element
        .as_any_mut()
        .downcast_mut::<Image>()
        .expect("Image host")
        .set_layout_transition_width_for_test(17.0);
    arena
        .get_mut(svg)
        .expect("svg")
        .element
        .as_any_mut()
        .downcast_mut::<Svg>()
        .expect("Svg host")
        .set_layout_transition_width_for_test(17.0);
    let error = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect_err("wrapper-forwarded runtime layout state must fail closed");
    assert!(
        error
            .reasons
            .contains(&FramePaintPlanRejection::LayoutTransition(image))
    );
    assert!(
        error
            .reasons
            .contains(&FramePaintPlanRejection::LayoutTransition(svg))
    );
}
