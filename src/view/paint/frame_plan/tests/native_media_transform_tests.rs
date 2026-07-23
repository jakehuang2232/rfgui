use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn direct_image_and_svg_transform_roots_build_sealed_property_surfaces_and_emit() {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='12'><rect width='16' height='12' fill='#22c55e'/></svg>";

    for (host_index, is_svg) in [false, true].into_iter().enumerate() {
        for (state_index, state) in ["ready", "loading", "error"].into_iter().enumerate() {
            let stable_id = 0xc1_0f00 + (host_index * 0x10 + state_index) as u64;
            let mut style = Style::new();
            style.insert(
                PropertyId::Width,
                ParsedValue::Length(crate::style::Length::px(16.0)),
            );
            style.insert(
                PropertyId::Height,
                ParsedValue::Length(crate::style::Length::px(12.0)),
            );
            style.insert(
                PropertyId::Transition,
                ParsedValue::Transition(Transitions::single(Transition::new(
                    TransitionProperty::Width,
                    200,
                ))),
            );
            style.set_transform(Transform::new([Rotate::z(Angle::deg(11.0))]));
            let host: Box<dyn ElementTrait> = if is_svg {
                let source = if state == "ready" {
                    SvgSource::Content(SVG.into())
                } else {
                    SvgSource::Path(format!("direct-transform-{state}-{stable_id}.svg").into())
                };
                let mut svg = Svg::new_with_id(stable_id, source);
                svg.apply_style(style);
                match state {
                    "loading" => svg.set_document_loading_for_transform_test(),
                    "error" => svg.set_document_error_for_transform_test(),
                    _ => {}
                }
                Box::new(svg)
            } else {
                let source = if state == "ready" {
                    ImageSource::Rgba {
                        width: 2,
                        height: 2,
                        pixels: Arc::from([
                            255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255,
                            255,
                        ]),
                    }
                } else {
                    ImageSource::Path(
                        format!("direct-transform-{state}-{stable_id}.png").into(),
                    )
                };
                let mut image = Image::new_with_id(stable_id, source);
                image.apply_style(style);
                match state {
                    "loading" => image.set_resource_loading_for_test(),
                    "error" => image.set_resource_error_for_test(),
                    _ => {}
                }
                Box::new(image)
            };
            let mut arena = new_test_arena();
            let root = commit_element(&mut arena, host);
            arena
                .with_element_taken(root, |element, arena| element.sync_arena(arena))
                .expect("freeze direct media resource state");
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
                    parent_x: 3.25,
                    parent_y: 4.5,
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
            {
                let mut node = arena.get_mut(root).expect("direct media transition root");
                if is_svg {
                    node.element
                        .as_any_mut()
                        .downcast_mut::<Svg>()
                        .expect("Svg host")
                        .set_layout_transition_width_for_test(18.0);
                } else {
                    node.element
                        .as_any_mut()
                        .downcast_mut::<Image>()
                        .expect("Image host")
                        .set_layout_transition_width_for_test(18.0);
                }
            }
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
                    parent_x: 3.25,
                    parent_y: 4.5,
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
            assert!(
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .retained_sampled_layout_transition_snapshot()
                    .is_some(),
                "host={is_svg} state={state} must install one exact sampled transition"
            );
            if is_svg && state == "ready" {
                arena
                    .get_mut(root)
                    .expect("svg root")
                    .element
                    .as_any_mut()
                    .downcast_mut::<Svg>()
                    .expect("Svg host")
                    .prepare_content_paint_for_test(SVG, (16.0, 12.0), 1.0)
                    .expect("prepare exact SVG paint");
            }
            let mut properties = PropertyTrees::default();
            properties.sync(&arena, &[root]);
            assert!(properties.validation_errors.is_empty());
            let mut generations = PaintGenerationTracker::default();
            generations.sync(&arena, &[root], &properties);

            let narrow =
                plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
                    .expect("direct native media root must satisfy narrow transform authority");
            let [PaintPlanStep::RetainedSurface(narrow_surface)] = narrow.steps.as_slice()
            else {
                panic!("narrow direct media plan must own one transform surface")
            };
            assert_eq!(narrow_surface.boundary_root, root);
            assert_eq!(narrow_surface.transform(), TransformNodeId(root));

            let plan = plan_transform_property_scene_with_context(
                &arena,
                &[root],
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
            .expect("direct native media transform root must be retained");
            assert!(property_scene_plan_is_sealed(&plan));
            let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
                panic!("direct media root must produce exactly one transform surface")
            };
            assert_eq!(surface.boundary_root, root);
            assert_eq!(surface.transform(), TransformNodeId(root));
            let span = only_span(surface);
            assert!(
                super::super::super::compiler::validate_transform_property_surface_artifact_for_plan(
                    &span.artifact,
                    root,
                    TransformNodeId(root),
                )
                .is_some()
            );
            let expected_role = if state == "ready" {
                if is_svg {
                    super::super::super::PaintChunkRole::SvgContent
                } else {
                    super::super::super::PaintChunkRole::ImageContent
                }
            } else {
                super::super::super::PaintChunkRole::SelfDecoration
            };
            assert!(
                span.artifact
                    .chunks
                    .iter()
                    .any(|chunk| chunk.id.role == expected_role),
                "host={is_svg} state={state}"
            );

            let mut viewport = Viewport::new();
            let mut graph = FrameGraph::new();
            let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
            let prepared = super::super::super::prepare_retained_property_scene_from_pool(
                &viewport, &plan, &graph, &ctx,
            )
            .expect("direct native media property-scene preflight");
            let _ = super::super::super::emit_prepared_retained_property_scene(
                &mut viewport,
                prepared,
                &mut graph,
                ctx,
            );
            assert!(!graph.pass_descriptors().is_empty());
            assert!(graph.test_compile_snapshot().is_ok());
        }
    }
}

#[test]
fn sampled_element_layout_transition_seals_transform_and_effect_property_boundaries() {
    for property in ["transform", "effect"] {
        let mut element = Element::new_with_id(
            if property == "transform" {
                0xc1_0f18
            } else {
                0xc1_0f19
            },
            0.0,
            0.0,
            24.0,
            18.0,
        );
        let mut style = Style::new();
        style.insert(
            PropertyId::Width,
            ParsedValue::Length(crate::style::Length::px(24.0)),
        );
        style.insert(
            PropertyId::Height,
            ParsedValue::Length(crate::style::Length::px(18.0)),
        );
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(30, 90, 180)),
        );
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Width,
                200,
            ))),
        );
        if property == "transform" {
            style.set_transform(Transform::new([Rotate::z(Angle::deg(8.0))]));
        } else {
            style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
        }
        element.apply_style(style);

        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let constraints = LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        };
        let placement = LayoutPlacement {
            parent_x: 2.0,
            parent_y: 3.0,
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
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_layout_transition_width(28.0);
        measure_and_place(&mut arena, root, constraints, placement);
        assert!(
            arena
                .get(root)
                .unwrap()
                .element
                .retained_sampled_layout_transition_snapshot()
                .is_some(),
            "{property} transition sample must be installed"
        );

        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let plan = if property == "transform" {
            plan_transform_property_scene_with_context(
                &arena,
                &[root],
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
        } else {
            plan_property_effect_scene_with_context(
                &arena,
                &[root],
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
        }
        .unwrap_or_else(|error| panic!("sampled Element {property}: {error:?}"));
        assert!(property_scene_plan_is_sealed(&plan));

        let mut graph = FrameGraph::new();
        let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
        let mut viewport = Viewport::new();
        let prepared = super::super::super::prepare_retained_property_scene_from_pool(
            &viewport, &plan, &graph, &ctx,
        )
        .unwrap_or_else(|error| panic!("sampled Element {property} preflight: {error:?}"));
        let _ = super::super::super::emit_prepared_retained_property_scene(
            &mut viewport,
            prepared,
            &mut graph,
            ctx,
        );
        assert!(graph.test_compile_snapshot().is_ok());

        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_layout_transition_width(30.0);
        measure_and_place(&mut arena, root, constraints, placement);
        assert!(
            arena
                .get(root)
                .unwrap()
                .element
                .retained_sampled_layout_transition_snapshot()
                .is_some(),
            "the changed {property} sample is installed but not generation-synced"
        );
        let stale = if property == "transform" {
            plan_transform_property_scene_with_context(
                &arena,
                &[root],
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
        } else {
            plan_property_effect_scene_with_context(
                &arena,
                &[root],
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
        };
        assert!(
            stale.is_err(),
            "an installed but revision-stale {property} sample must fail closed"
        );
    }
}

#[test]
fn malformed_sampled_element_transition_remains_fail_closed() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_layout_transition_x(f32::NAN);
    arena
        .get_mut(root)
        .unwrap()
        .element
        .clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
    arena.clear_arena_dirty_subtree(root, crate::view::base_component::DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    assert!(
        arena
            .get(root)
            .unwrap()
            .element
            .retained_sampled_layout_transition_snapshot()
            .is_none()
    );
    let error = plan_transform_property_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("non-finite sampled transition must remain fail-closed");
    assert!(
        error
            .reasons
            .contains(&FramePaintPlanRejection::LayoutTransition(root))
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn direct_native_media_transform_geometry_and_identity_drift_fail_closed() {
    let fixture = |width: f32| {
        let mut image = Image::new_with_id(
            0xc1_0f20,
            ImageSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::from([255_u8, 255, 255, 255]),
            },
        );
        let mut style = Style::new();
        style.insert(
            PropertyId::Width,
            ParsedValue::Length(crate::style::Length::px(width)),
        );
        style.insert(
            PropertyId::Height,
            ParsedValue::Length(crate::style::Length::px(12.0)),
        );
        style.set_transform(Transform::new([Rotate::z(Angle::deg(7.0))]));
        image.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(image));
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
                parent_x: 2.0,
                parent_y: 3.0,
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
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, properties, generations)
    };

    let (arena, root, properties, generations) = fixture(0.0);
    let error = plan_transform_property_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("zero-area native media transform must remain fail-closed");
    assert!(
        error
            .reasons
            .contains(&FramePaintPlanRejection::InvalidSurfaceGeometry(root))
    );

    let (arena, root, mut properties, generations) = fixture(16.0);
    properties
        .transforms
        .get_mut(&TransformNodeId(root))
        .expect("transform snapshot")
        .viewport_matrix *= glam::Mat4::from_translation(glam::vec3(1.0, 0.0, 0.0));
    let error = plan_transform_property_scene_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect_err("property/live transform identity drift must remain fail-closed");
    assert!(
        error
            .reasons
            .contains(&FramePaintPlanRejection::InvalidRootTransform(root))
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn nested_direct_image_and_svg_transform_boundaries_stay_native_retained() {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='12'><rect width='16' height='12' fill='#38bdf8'/></svg>";

    for (index, is_svg) in [false, true].into_iter().enumerate() {
        let mut parent =
            Element::new_with_id(0xc1_0f40 + index as u64 * 2, 0.0, 0.0, 48.0, 32.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        parent_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(12, 24, 48)),
        );
        parent.apply_style(parent_style);

        let child_id = 0xc1_0f41 + index as u64 * 2;
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(crate::style::Length::px(16.0)),
        );
        child_style.insert(
            PropertyId::Height,
            ParsedValue::Length(crate::style::Length::px(12.0)),
        );
        child_style.set_transform(Transform::new([Rotate::z(Angle::deg(9.0))]));
        let child: Box<dyn ElementTrait> = if is_svg {
            let mut svg = Svg::new_with_id(child_id, SvgSource::Content(SVG.into()));
            svg.apply_style(child_style);
            Box::new(svg)
        } else {
            let mut image = Image::new_with_id(
                child_id,
                ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([40_u8, 160, 240, 255]),
                },
            );
            image.apply_style(child_style);
            Box::new(image)
        };

        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(parent));
        let child = commit_child(&mut arena, root, child);
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
                parent_x: 2.0,
                parent_y: 3.0,
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
        if is_svg {
            arena
                .get_mut(child)
                .expect("svg child")
                .element
                .as_any_mut()
                .downcast_mut::<Svg>()
                .expect("Svg host")
                .prepare_content_paint_for_test(SVG, (16.0, 12.0), 1.0)
                .expect("prepare exact nested SVG paint");
        }
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        assert!(properties.validation_errors.is_empty());
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);

        let plan = plan_transform_property_scene_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("nested direct native media transform must be retained");
        assert!(property_scene_plan_is_sealed(&plan));
        let seal = plan
            .property_scene_seal
            .as_ref()
            .expect("property scene seal");
        assert_eq!(seal.surface_count, 1);
        assert!(seal.surfaces.values().any(|surface| {
            surface.id.owner == child
                && surface.stable_id == arena.get(child).unwrap().element.stable_id()
        }));

        let mut graph = FrameGraph::new();
        let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
        let mut viewport = Viewport::new();
        let prepared = super::super::super::prepare_retained_property_scene_from_pool(
            &viewport, &plan, &graph, &ctx,
        )
        .expect("nested native media property-scene preflight");
        let _ = super::super::super::emit_prepared_retained_property_scene(
            &mut viewport,
            prepared,
            &mut graph,
            ctx,
        );
        assert!(graph.test_compile_snapshot().is_ok());
    }
}
