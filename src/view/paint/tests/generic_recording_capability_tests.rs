use super::*;
use crate::view::compositor::property_tree::ScrollNodeId;
use crate::view::test_support::get_element_mut;

#[test]
fn generic_recording_deferred_phase_and_scrollbar_overlay_keep_canonical_order() {
    for deferred in [false, true] {
        let mut arena = new_test_arena();
        let mut host = Element::new_with_id(0xc1_4300, 0.0, 0.0, 40.0, 32.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(40.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
        if !deferred {
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
        }
        host.apply_style(style);
        let root = commit_element(&mut arena, Box::new(host));
        let make_child = |id, late| {
            let mut child = Element::new_with_id(id, 0.0, 0.0, 20.0, 120.0);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
            style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
            style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::rgb(255, 0, 0)),
            );
            if late {
                style.insert(
                    PropertyId::Position,
                    ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
                );
            }
            child.apply_style(style);
            child
        };
        // Put the deferred child first in arena order: the recorder must still
        // place it after ordinary paint, matching the deferred viewport phase.
        let late =
            deferred.then(|| commit_child(&mut arena, root, Box::new(make_child(0xc1_4301, true))));
        let normal = commit_child(&mut arena, root, Box::new(make_child(0xc1_4302, false)));
        let mut viewport = crate::view::viewport::Viewport::new();
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut viewport,
            &mut arena,
            root,
            [320.0, 240.0],
        );
        if !deferred {
            let mut host = get_element_mut::<Element>(&arena, root);
            host.set_sampled_scrollbar_alpha_for_test(1.0);
        }
        let (properties, generations) = sync_identity(&arena, &[root]);
        if !deferred {
            let snapshot = properties.scroll_snapshot_for(ScrollNodeId(root)).unwrap();
            let context = PaintRecordingContext {
                recording_owner: Some(root),
                recording_owner_stable_id: Some(0xc1_4300),
                surface_dag: true,
                surface_dag_scroll: Some(snapshot.id),
                surface_dag_scroll_snapshot: Some(snapshot),
                ..Default::default()
            };
            assert_eq!(
                context.recorded_scroll_host_snapshot_for_root(0xc1_4300),
                Some(snapshot)
            );
            assert!(
                context
                    .baked_scroll_host_snapshot_for_root(0xc1_4300)
                    .is_none()
            );
            assert!(
                context
                    .recorded_scroll_host_snapshot_for_root(0xc1_4301)
                    .is_none()
            );
            for invalid in [
                PaintRecordingContext {
                    surface_dag: false,
                    ..context
                },
                PaintRecordingContext {
                    recording_owner: Some(normal),
                    ..context
                },
                PaintRecordingContext {
                    surface_dag_scroll: Some(ScrollNodeId(normal)),
                    ..context
                },
                PaintRecordingContext {
                    surface_dag_scroll_snapshot: None,
                    ..context
                },
                PaintRecordingContext {
                    surface_dag_scroll_snapshot: Some(
                        crate::view::compositor::property_tree::ScrollNodeSnapshot {
                            owner: normal,
                            ..snapshot
                        },
                    ),
                    ..context
                },
                PaintRecordingContext {
                    surface_dag_scroll_snapshot: Some(
                        crate::view::compositor::property_tree::ScrollNodeSnapshot {
                            id: ScrollNodeId(normal),
                            ..snapshot
                        },
                    ),
                    ..context
                },
            ] {
                assert!(
                    invalid
                        .recorded_scroll_host_snapshot_for_root(0xc1_4300)
                        .is_none(),
                    "foreign or incomplete generic authority must reject"
                );
            }
        }
        let outcome = record_surface_dag_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_or_else(|error| panic!("deferred={deferred}: {error:?}"));
        let FrameArtifactRecordOutcome::Artifact { artifact, .. } = outcome else {
            panic!("fallback")
        };
        if let Some(late) = late {
            assert_eq!(
                artifact
                    .chunks
                    .iter()
                    .filter(|c| c.owner != root)
                    .map(|c| c.owner)
                    .collect::<Vec<_>>(),
                vec![normal, late]
            );
        } else {
            let last = artifact.chunks.last().unwrap();
            assert_eq!(
                (last.owner, last.id.role, last.id.phase),
                (
                    root,
                    PaintChunkRole::ScrollbarOverlay,
                    PaintNodePhase::AfterChildren
                )
            );
            assert!(matches!(
                &artifact.ops[last.op_range.start],
                PaintOp::PreparedScrollbarOverlay(_)
            ));
            assert_ne!(
                last.properties.clip,
                artifact
                    .chunks
                    .iter()
                    .find(|c| c.owner == normal)
                    .unwrap()
                    .properties
                    .clip,
                "overlay must return to host scope, outside contents clip"
            );
        }
        drop(arena);
        let context = ArtifactSurfaceRasterContext::new(
            1.0,
            wgpu::TextureFormat::Rgba8Unorm,
            [0.0, 0.0],
            None,
            8192,
            128 * 1024 * 1024,
        )
        .unwrap();
        let plan = prepare_artifact_surface_raster_plan(artifact, context).unwrap();
        seal_prepared_artifact_surface_frame(plan).unwrap();
    }
}

#[test]
fn generic_recording_transparent_resource_slots_preserve_only_active_children() {
    for svg in [false, true] {
        for error in [false, true] {
            for child_present in [false, true] {
                let mut arena = new_test_arena();
                let mut style = Style::new();
                style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
                style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
                style.insert(
                    PropertyId::BackgroundColor,
                    ParsedValue::color_like(Color::rgba(0, 0, 0, 0)),
                );
                let owner = if svg {
                    let source = SvgSource::Content(format!(
                        "<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- c14 {error} {child_present} --></svg>"
                    ));
                    let key = crate::view::svg_resource::prime_svg_document_ready_for_test(
                        &source, 20.0, 32.0,
                    );
                    if error {
                        crate::view::svg_resource::set_svg_document_error_for_test(key);
                    } else {
                        crate::view::svg_resource::set_svg_document_loading_for_test(key);
                    }
                    let mut host = Svg::new_with_id(0xc1_4200, source);
                    host.apply_style(style);
                    commit_element(&mut arena, Box::new(host))
                } else {
                    let source = ImageSource::Rgba {
                        width: 1,
                        height: 1,
                        pixels: Arc::from([255, 0, 0, 255]),
                    };
                    let handle = crate::view::image_resource::acquire_image_resource(&source);
                    if error {
                        crate::view::image_resource::set_image_error_for_test(
                            handle.asset_id(),
                            "c14",
                        );
                    } else {
                        crate::view::image_resource::set_image_loading_for_test(handle.asset_id());
                    }
                    let mut host = Image::new_with_id(0xc1_4200, source);
                    host.apply_style(style);
                    commit_element(&mut arena, Box::new(host))
                };
                let child = child_present.then(|| {
                    let mut element = Element::new_with_id(0xc1_4201, 0.0, 0.0, 8.0, 8.0);
                    element.set_background_color_value(Color::rgb(255, 0, 0));
                    let child = arena.insert(Node::with_parent(Box::new(element), Some(owner)));
                    arena.with_element_taken(owner, |el, _| {
                        if svg {
                            let host = el.as_any_mut().downcast_mut::<Svg>().unwrap();
                            if error {
                                host.attach_error_slot_cold(vec![child]);
                            } else {
                                host.attach_loading_slot_cold(vec![child]);
                            }
                        } else {
                            let host = el.as_any_mut().downcast_mut::<Image>().unwrap();
                            if error {
                                host.attach_error_slot_cold(vec![child]);
                            } else {
                                host.attach_loading_slot_cold(vec![child]);
                            }
                        }
                    });
                    child
                });
                let mut viewport = crate::view::viewport::Viewport::new();
                crate::view::viewport::layout_artifact_style_scene_for_test(
                    &mut viewport,
                    &mut arena,
                    owner,
                    [64.0, 64.0],
                );
                let (properties, generations) = sync_identity(&arena, &[owner]);
                let outcome = record_surface_dag_frame_artifact(
                    &arena,
                    &[owner],
                    &properties,
                    &generations,
                    RendererMode::ForcedForTests,
                )
                .unwrap_or_else(|error| panic!("svg={svg} child={child_present}: {error:?}"));
                let FrameArtifactRecordOutcome::Artifact { artifact, .. } = outcome else {
                    panic!("fallback")
                };
                // The recorder preserves a canonical empty owner chunk; it
                // must neither invent paint nor skip the active descendants.
                for chunk in artifact.chunks.iter().filter(|chunk| chunk.owner == owner) {
                    assert!(
                        chunk.op_range.is_empty(),
                        "transparent wrapper must have no commands: {:?}",
                        artifact.ops
                    );
                }
                assert_eq!(
                    artifact
                        .chunks
                        .iter()
                        .filter(|chunk| chunk.owner != owner)
                        .map(|chunk| chunk.owner)
                        .collect::<Vec<_>>(),
                    child.into_iter().collect::<Vec<_>>()
                );
            }
        }
    }
}

// C-1.4's executable recording inventory. It calls the same generic recorder
// for every row and compiles only the frozen result, after dropping the arena.
// This is recording/compiler evidence; native pixel evidence is tracked
// separately, never inferred from these fixture names or op counts.
#[test]
fn generic_recording_capability_matrix_closes_and_compiles_native_commands() {
    let mut cases = Vec::new();
    let (arena, roots, _) = prepared_plain_tree();
    cases.push(("fill", arena, roots));
    let (arena, roots) = prepared_gradient_tree();
    cases.push(("gradient", arena, roots));
    let (arena, roots) = prepared_asymmetric_border_tree();
    cases.push(("border", arena, roots));
    let (arena, root, _, _) = prepared_shadow_leaf(0xc1_4100, 1.0, two_outer_shadows(), true);
    cases.push(("shadow", arena, vec![root]));
    let (arena, roots, _) = prepared_text_tree(false);
    cases.push(("text", arena, roots));
    let (arena, roots, ..) = prepared_wrapping_inline_span_tree();
    cases.push(("inline decoration", arena, roots));
    let (arena, roots, ..) = prepared_owning_inline_root_with_image_atomic();
    cases.push(("inline image", arena, roots));
    let (arena, roots, ..) = prepared_owning_inline_root_with_svg_atomic();
    cases.push(("inline SVG", arena, roots));
    let (arena, roots) = prepared_zero_opacity_tree();
    cases.push(("culled and visible roots", arena, roots));
    for (name, arena, roots) in cases {
        let (properties, generations) = sync_identity(&arena, &roots);
        let result = record_surface_dag_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = result
        else {
            panic!("{name} fallback")
        };
        assert!(eligibility.eligible && !artifact.ops.is_empty(), "{name}");
        let present = artifact.ops.iter().any(|op| match name {
            "shadow" => matches!(op, PaintOp::PreparedShadow(_)),
            "text" => matches!(op, PaintOp::PreparedText(_)),
            "inline decoration" => matches!(op, PaintOp::PreparedInlineIfcDecoration(_)),
            "inline image" => matches!(op, PaintOp::PreparedImage(_)),
            "inline SVG" => matches!(op, PaintOp::PreparedSvg(_)),
            _ => matches!(op, PaintOp::DrawRect(_)),
        });
        assert!(present, "{name} must retain its command kind");
        drop(arena);
        drop(properties);
        drop(generations);
        let context = ArtifactSurfaceRasterContext::new(
            1.0,
            wgpu::TextureFormat::Rgba8Unorm,
            [0.0, 0.0],
            None,
            8192,
            128 * 1024 * 1024,
        )
        .unwrap();
        let plan = prepare_artifact_surface_raster_plan(artifact, context)
            .unwrap_or_else(|error| panic!("{name} plan: {error:?}"));
        seal_prepared_artifact_surface_frame(plan)
            .unwrap_or_else(|error| panic!("{name} seal: {error:?}"));
    }
}
