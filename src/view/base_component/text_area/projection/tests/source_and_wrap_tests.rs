use super::*;

#[test]
fn normalize_rejects_projection_ranges_that_cross_newline() {
    let projections = vec![super::super::TextAreaRenderProjection {
        range: 1..4,
        node: RsxNode::text("a\nb"),
    }];
    let normalized = super::super::normalize_projections("xa\nby", &projections);
    assert!(
        normalized.is_empty(),
        "cross-line projections must not swallow hard newline semantics"
    );
}

#[test]
fn text_area_projection_fixed_width_wrap_keeps_plain_run_slicing() {
    let content = concat!(
        "Fetch a long environment URL from {{API_HOST}} while the line wraps automatically\n",
        "/v1/users/{{USER_ID}}/profiles/preferences/activity/export/sessions"
    );
    let api_host = char_range_of(content, "{{API_HOST}}");
    let user_id = char_range_of(content, "{{USER_ID}}");

    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.multiline = true;
    text_area.auto_wrap = true;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(api_host.clone(), |_text_area_node| {
            projection_chip_node("API_HOST", 88.0)
        });
        render.range(user_id.clone(), |_text_area_node| {
            projection_chip_node("USER_ID", 72.0)
        });
    }));

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 168.0,
            max_height: 360.0,
            viewport_width: 168.0,
            viewport_height: 360.0,
            percent_base_width: Some(168.0),
            percent_base_height: Some(360.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 168.0,
            available_height: 360.0,
            viewport_width: 168.0,
            viewport_height: 360.0,
            percent_base_width: Some(168.0),
            percent_base_height: Some(360.0),
        },
    );

    let children = arena.children_of(root);
    let run_segments = children
        .iter()
        .filter_map(|key| {
            arena
                .with_element_taken_ref(*key, |child, _| {
                    child
                        .as_any()
                        .downcast_ref::<TextAreaTextRun>()
                        .map(|run| (run.text.clone(), run.char_range.clone()))
                })
                .flatten()
        })
        .collect::<Vec<_>>();
    let projection_count = children
        .iter()
        .filter(|key| {
            arena
                .with_element_taken_ref(**key, |child, _| {
                    child.as_any().is::<TextAreaProjectionSegment>()
                })
                .unwrap_or(false)
        })
        .count();

    assert_eq!(projection_count, 2, "expected two atomic projection slots");
    assert_eq!(
        run_segments.len(),
        4,
        "projection slicing should leave four plain TextAreaTextRun segments"
    );

    let run_texts = run_segments
        .iter()
        .map(|(text, _)| text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        run_texts,
        vec![
            "Fetch a long environment URL from ",
            " while the line wraps automatically",
            "/v1/users/",
            "/profiles/preferences/activity/export/sessions",
        ],
        "projection tokens should be removed from plain run slices",
    );

    for (run_text, char_range) in &run_segments {
        assert_eq!(
            run_text.chars().count(),
            char_range.end.saturating_sub(char_range.start),
            "plain run char range should describe the sliced run text"
        );
    }

    let run_contents = run_segments
        .iter()
        .map(|(text, _)| text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        !run_contents.contains("API_HOST") && !run_contents.contains("USER_ID"),
        "plain run slices do not carry projection atomic boxes"
    );
    assert_ne!(
        run_contents, content,
        "plain run slices alone do not describe the visible unified render/layout path"
    );
}

#[test]
fn text_area_inline_ifc_projection_fixed_width_wrap_builds_unified_root_source() {
    let content = concat!(
        "Fetch a long environment URL from {{API_HOST}} while the line wraps automatically\n",
        "/v1/users/{{USER_ID}}/profiles/preferences/activity/export/sessions"
    );
    let api_host = char_range_of(content, "{{API_HOST}}");
    let user_id = char_range_of(content, "{{USER_ID}}");

    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.multiline = true;
    text_area.auto_wrap = true;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(api_host.clone(), |_text_area_node| {
            projection_chip_node("API_HOST", 88.0)
        });
        render.range(user_id.clone(), |_text_area_node| {
            projection_chip_node("USER_ID", 72.0)
        });
    }));

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 168.0,
            max_height: 360.0,
            viewport_width: 168.0,
            viewport_height: 360.0,
            percent_base_width: Some(168.0),
            percent_base_height: Some(360.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 168.0,
            available_height: 360.0,
            viewport_width: 168.0,
            viewport_height: 360.0,
            percent_base_width: Some(168.0),
            percent_base_height: Some(360.0),
        },
    );

    let package = arena
        .with_element_taken_ref(root, |el, _| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .unified_inline_ifc_root_package(&arena)
        })
        .flatten()
        .expect("TextArea should expose a unified IFC root package");

    assert_eq!(package.width_constraint, Some(168.0));
    assert!(package.allow_wrap);
    assert_eq!(package.text_run_count(), 4);
    assert_eq!(package.projection_segment_count(), 2);
    assert_eq!(
        package.source_segments.len(),
        arena.children_of(root).len(),
        "each TextArea child source should map to one IFC root item"
    );

    let source_kinds = package
        .source_segments
        .iter()
        .map(|segment| segment.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        source_kinds,
        vec![
            TextAreaUnifiedIfcSourceKind::TextRun,
            TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox,
            TextAreaUnifiedIfcSourceKind::TextRun,
            TextAreaUnifiedIfcSourceKind::LineBreak,
            TextAreaUnifiedIfcSourceKind::TextRun,
            TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox,
            TextAreaUnifiedIfcSourceKind::TextRun,
        ],
        "unified root source should preserve TextArea text/projection/newline sequence"
    );

    let projection_ranges = package
        .source_segments
        .iter()
        .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
        .map(|segment| segment.char_range.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        projection_ranges,
        vec![
            char_range_of(content, "{{API_HOST}}"),
            char_range_of(content, "{{USER_ID}}"),
        ],
        "projection atomic boxes must keep original TextArea char/source ranges"
    );

    assert_eq!(
        package.ifc.backing_text(),
        concat!(
            "Fetch a long environment URL from  while the line wraps automatically\n",
            "/v1/users//profiles/preferences/activity/export/sessions"
        ),
        "projection tokens should be represented by atomic boxes, not glyph text"
    );
    assert_eq!(package.atomic_sources.len(), 2);
    for source in &package.atomic_sources {
        let atomic_package = package.ifc.atomic_box_placement_package(*source);
        assert_eq!(atomic_package.source, *source);
        assert_eq!(
            atomic_package.placements.len(),
            1,
            "each TextArea projection source should produce one atomic placement"
        );
        let placement = &atomic_package.placements[0];
        assert_eq!(placement.source, *source);
        assert!(
            placement.measurement.measured_size.width > 0.0
                && placement.measurement.measured_size.height > 0.0,
            "projection atomic measurement should come from the laid-out projection segment"
        );
    }
}

#[test]
fn text_area_projection_tall_child_keeps_unified_ifc_geometry_height() {
    let content = "before {{TALL}} after";
    let tall_range = char_range_of(content, "{{TALL}}");
    let tall_height = 240.0;

    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.multiline = true;
    text_area.auto_wrap = true;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(tall_range.clone(), move |_text_area_node| {
            tall_projection_block_node(96.0, tall_height)
        });
    }));

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 180.0,
            max_height: 2_000.0,
            viewport_width: 180.0,
            viewport_height: 2_000.0,
            percent_base_width: Some(180.0),
            percent_base_height: Some(2_000.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 180.0,
            available_height: 2_000.0,
            viewport_width: 180.0,
            viewport_height: 2_000.0,
            percent_base_width: Some(180.0),
            percent_base_height: Some(2_000.0),
        },
    );

    let package = arena
        .with_element_taken_ref(root, |el, _| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .unified_inline_ifc_root_package(&arena)
        })
        .flatten()
        .expect("TextArea should expose a unified IFC root package");
    let projection_segment = package
        .source_segments
        .iter()
        .find(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
        .cloned()
        .expect("projection source segment");
    let atomic_package = package
        .atomic_package_for_child(projection_segment.child_key)
        .expect("projection child should have an atomic package");
    let placement = atomic_package
        .placements
        .first()
        .expect("projection child should have an atomic placement");
    let projection_snapshot = arena
        .with_element_taken_ref(projection_segment.child_key, |child, _| {
            child.box_model_snapshot()
        })
        .expect("projection child snapshot");
    let content_size = package.content_size();

    assert!(
        placement.measurement.measured_size.height >= tall_height - 0.01,
        "atomic measurement should keep the tall projection height: measured={}",
        placement.measurement.measured_size.height
    );
    assert!(
        placement.rect.height >= tall_height - 0.01,
        "atomic placement rect should keep the tall projection height: rect={}",
        placement.rect.height
    );
    assert!(
        projection_snapshot.height >= tall_height - 0.01,
        "projection snapshot should keep the tall child height: snapshot={projection_snapshot:?}"
    );
    assert!(
        content_size.height >= tall_height - 0.01,
        "TextArea unified IFC content size should include tall projection height: content_size={content_size:?}"
    );
}
