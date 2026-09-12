use super::*;

#[test]
fn text_area_inline_ifc_projection_fixed_width_wrap_applies_unified_atomic_placement() {
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

    let projection_segments = package
        .source_segments
        .iter()
        .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(projection_segments.len(), 2);

    for segment in projection_segments {
        let atomic_package = package
            .atomic_package_for_child(segment.child_key)
            .expect("projection child should have an atomic package");
        let placement = atomic_package
            .placements
            .first()
            .expect("projection child should have an atomic placement");
        let snapshot = arena
            .with_element_taken_ref(segment.child_key, |child, _| child.box_model_snapshot())
            .expect("projection child snapshot");
        assert!(
            (snapshot.x - placement.rect.x).abs() < 0.01,
            "projection x should be applied from unified atomic placement: snapshot={}, placement={}",
            snapshot.x,
            placement.rect.x
        );
        assert!(
            (snapshot.y - placement.rect.y).abs() < 0.01,
            "projection y should be applied from unified atomic placement: snapshot={}, placement={}",
            snapshot.y,
            placement.rect.y
        );
        assert!(
            (snapshot.width - placement.rect.width).abs() < 0.01,
            "projection width should remain aligned with unified atomic measurement"
        );
        assert!(
            (snapshot.height - placement.rect.height).abs() < 0.01,
            "projection height should remain aligned with unified atomic measurement"
        );
    }
}

#[test]
fn text_area_inline_ifc_projection_overlay_sources_match_unified_atomic_placement() {
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
    text_area.cursor_char = api_host.start + 2;
    text_area.selection_anchor_char = Some(api_host.start + 1);
    text_area.selection_focus_char = Some(api_host.start + 2);
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
    let overlay_sources = package.projection_overlay_sources();
    assert_eq!(overlay_sources.len(), 2);

    let children = arena.children_of(root);
    let projection_child_index = children
        .iter()
        .position(|key| {
            arena
                .with_element_taken_ref(*key, |child, _| {
                    child.as_any().is::<TextAreaProjectionSegment>()
                })
                .unwrap_or(false)
        })
        .expect("first projection child");
    let projection_key = children[projection_child_index];
    let overlay_source = package
        .projection_overlay_source_for_child(projection_key)
        .expect("projection overlay source should come from unified IFC root");
    assert_eq!(
        overlay_source.char_range,
        char_range_of(content, "{{API_HOST}}")
    );
    assert_eq!(
        overlay_source.backing_byte_range.start,
        overlay_source.backing_byte_range.end
    );

    let selection_context = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .projection_selection_context_for_child(
                    projection_child_index,
                    projection_key,
                    arena,
                )
        })
        .expect("root exists")
        .expect("projection selection context should remain available");
    assert_eq!(selection_context.start, 1);
    assert_eq!(selection_context.end, 2);
    assert_eq!(
        overlay_source.char_range.start + selection_context.start
            ..overlay_source.char_range.start + selection_context.end,
        char_range_of(content, "{{API_HOST}}").start + 1
            ..char_range_of(content, "{{API_HOST}}").start + 2,
        "projection selection overlay should map through the same TextArea source range as unified IFC"
    );

    let snapshot = arena
        .with_element_taken_ref(projection_key, |child, _| child.box_model_snapshot())
        .expect("projection child snapshot");
    assert!(
        (snapshot.x - overlay_source.atomic_rect.x).abs() < 0.01
            && (snapshot.y - overlay_source.atomic_rect.y).abs() < 0.01
            && (snapshot.width - overlay_source.atomic_rect.width).abs() < 0.01
            && (snapshot.height - overlay_source.atomic_rect.height).abs() < 0.01,
        "projection overlay source should use the same atomic rect applied to the visible projection placement"
    );
}

#[test]
fn text_area_inline_ifc_projection_atomic_placement_honors_vertical_align() {
    fn projection_y_for(vertical_align: crate::style::VerticalAlign) -> (f32, f32) {
        let content = "aaa{{BIG}}bbb";
        let projection_range = char_range_of(content, "{{BIG}}");
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.vertical_align = vertical_align;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(projection_range.clone(), |_text_area_node| {
                projection_chip_node("BIG", 88.0)
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
                max_width: 240.0,
                max_height: 120.0,
                viewport_width: 240.0,
                viewport_height: 120.0,
                percent_base_width: Some(240.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 240.0,
                available_height: 120.0,
                viewport_width: 240.0,
                viewport_height: 120.0,
                percent_base_width: Some(240.0),
                percent_base_height: Some(120.0),
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
        let projection_child = package
            .source_segments
            .iter()
            .find(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
            .expect("projection source")
            .child_key;
        let placement_y = package
            .atomic_package_for_child(projection_child)
            .expect("projection atomic package")
            .placements
            .first()
            .expect("projection atomic placement")
            .rect
            .y;
        let snapshot_y = arena
            .with_element_taken_ref(projection_child, |child, _| child.box_model_snapshot().y)
            .expect("projection snapshot");
        let glyph_y = package
            .text_pass_staging_input([0.0, 0.0], 1.0, 0, 1.0)
            .glyphs
            .first()
            .expect("plain glyph")
            .final_paint_pos[1];
        assert!(
            (placement_y - snapshot_y).abs() < 0.01,
            "visible projection placement should stay sourced from unified atomic placement"
        );
        (placement_y, glyph_y)
    }

    let (top_projection_y, top_glyph_y) = projection_y_for(crate::style::VerticalAlign::Top);
    let (bottom_projection_y, bottom_glyph_y) =
        projection_y_for(crate::style::VerticalAlign::Bottom);

    assert!(
        (bottom_projection_y - top_projection_y).abs() < 0.01,
        "the tallest projection box can stay pinned while shorter glyphs move, top={top_projection_y}, bottom={bottom_projection_y}",
    );
    assert!(
        bottom_glyph_y > top_glyph_y + 1.0,
        "TextArea unified glyph render must move when vertical_align changes, top={top_glyph_y}, bottom={bottom_glyph_y}",
    );
}

#[test]
fn text_area_inline_ifc_auto_width_projection_keeps_following_text_after_atomic_box() {
    let content = concat!(
        "First line with a long value that can wrap when auto wrap is enabled.",
        "{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\n",
        "Tail line"
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
            auto_projection_chip_node("{{API_HOST}}")
        });
        render.range(user_id.clone(), |_text_area_node| {
            auto_projection_chip_node("{{USER_ID}}")
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
            max_width: 360.0,
            max_height: 176.0,
            viewport_width: 360.0,
            viewport_height: 176.0,
            percent_base_width: Some(360.0),
            percent_base_height: Some(176.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 360.0,
            available_height: 176.0,
            viewport_width: 360.0,
            viewport_height: 176.0,
            percent_base_width: Some(360.0),
            percent_base_height: Some(176.0),
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
    let user_segment = package
        .source_segments
        .iter()
        .find(|segment| {
            segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                && segment.char_range == char_range_of(content, "{{USER_ID}}")
        })
        .expect("USER_ID projection segment");
    let following_run = package
        .source_segments
        .iter()
        .find(|segment| {
            segment.kind == TextAreaUnifiedIfcSourceKind::TextRun
                && segment.char_range.start == char_range_of(content, "{{USER_ID}}").end
        })
        .expect("text run after USER_ID");
    let atomic = package
        .atomic_package_for_child(user_segment.child_key)
        .expect("USER_ID atomic package")
        .placements
        .first()
        .expect("USER_ID atomic placement")
        .clone();
    let snapshot = package.ifc.text_layout_snapshot_ref();
    let following_glyph = snapshot
        .lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .find(|glyph| glyph.source == following_run.source)
        .expect("first glyph after USER_ID");

    if following_glyph.y >= atomic.rect.y
        && following_glyph.y < atomic.rect.y + atomic.rect.height.max(1.0)
    {
        assert!(
            following_glyph.x >= atomic.rect.x + atomic.rect.width - 0.5,
            "text after USER_ID must not be painted under the projection chip: glyph_x={}, atomic=({}, {}, {}, {})",
            following_glyph.x,
            atomic.rect.x,
            atomic.rect.y,
            atomic.rect.width,
            atomic.rect.height,
        );
    }
}
