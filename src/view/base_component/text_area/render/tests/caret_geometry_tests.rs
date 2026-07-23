use super::*;

#[test]
fn projection_fallback_caret_start_uses_projection_left_edge() {
    let (arena, root) = projection_fixture(2, false);
    let snap = projection_snapshot(&arena, root);
    let (x, y, height) = caret_position(&arena, root);

    assert!((x - snap.x).abs() < 0.5, "x={x}, snap.x={}", snap.x);
    assert!((height - snap.height).abs() < 0.01, "height={height}");
    assert!((y - snap.y).abs() < 0.5, "y={y}, snap.y={}", snap.y);
}

#[test]
fn projection_fallback_caret_interpolates_inside_projection() {
    let (arena, root) = projection_fixture(3, false);
    let snap = projection_snapshot(&arena, root);
    let (x, _, height) = caret_position(&arena, root);
    let expected_x = snap.x + snap.width / 3.0;

    assert!((x - expected_x).abs() < 0.5, "x={x}, expected={expected_x}");
    assert!((height - snap.height).abs() < 0.01, "height={height}");
}

#[test]
fn projection_caret_prefers_inner_text_glyphs_when_descendant_exists() {
    // Interior caret positions align with the chip's rendered text
    // (real glyph coordinates), not the root box's char-fraction —
    // the fraction drifts off the visible characters (e.g. a caret
    // floating over the wrong letter inside {{USER_ID}}).
    let (arena, root) = projection_fixture(4, true);
    let snap = projection_snapshot(&arena, root);
    let (x, y, height) = caret_position(&arena, root);
    let expected = arena
        .with_element_taken_ref(root, |el, arena| {
            let text_area = el
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            let key = text_area.children[1];
            glyph_caret_in_projection(arena, key, 2, text_area.cursor_affinity)
        })
        .flatten()
        .expect("inner glyph caret");

    assert!(
        (x - expected.0).abs() < 0.5,
        "x={x}, expected={}",
        expected.0
    );
    assert!(
        (y - expected.1).abs() < 0.5,
        "y={y}, expected={}",
        expected.1
    );
    assert!(
        x > snap.x && x < snap.x + snap.width,
        "caret must sit inside the chip: x={x}, chip=({}, {})",
        snap.x,
        snap.x + snap.width
    );
    assert!(
        (height - expected.2).abs() < 0.01,
        "height={height}, expected={}",
        expected.2
    );
    let projection_snap = projection_snapshot(&arena, root);
    assert!(
        x >= projection_snap.x - 0.5 && x <= projection_snap.x + projection_snap.width + 0.5,
        "caret x should be inside projection bounds: x={x}, projection=({}, {})",
        projection_snap.x,
        projection_snap.width
    );
    assert!(
        y >= projection_snap.y - 0.5 && y <= projection_snap.y + projection_snap.height + 0.5,
        "caret y should be inside projection bounds: y={y}, projection=({}, {})",
        projection_snap.y,
        projection_snap.height
    );
}

#[test]
fn hard_newline_caret_honours_affinity() {
    use crate::view::base_component::text_area::caret_map::CaretAffinity;

    fn fixture(affinity: CaretAffinity) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = "line1\nline2".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.is_focused = true;
        text_area.cursor_char = "line1\n".chars().count();
        text_area.cursor_affinity = affinity;

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
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 300.0,
                available_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        (arena, root)
    }

    let (up_arena, up_root) = fixture(CaretAffinity::Upstream);
    let (_, up_y, _) = caret_position(&up_arena, up_root);
    let (down_arena, down_root) = fixture(CaretAffinity::Downstream);
    let (_, down_y, _) = caret_position(&down_arena, down_root);

    assert!(
        up_y < down_y,
        "Upstream should render before the newline on the upper line; \
         Downstream should render after it on the lower line (up={up_y}, down={down_y})",
    );
}

/// TextArea caret geometry now comes from the unified root IFC. A
/// projected badge contributes root-level atomic caret stops, so
/// affinity is resolved by the root line boundary rather than by
/// probing the projection's descendant Text.
#[test]
fn projection_badge_wrap_caret_affinity() {
    use crate::view::base_component::text_area::caret_map::CaretAffinity;
    // Mirror textarea_test: `{{API_HOST}}/v1/users/{{USER_ID}}/activity/...`
    // — single paragraph, badge projection in the middle, narrow
    // wrap forces the second badge to split.
    let user_token = "{{USER_ID_WITH_A_VERY_LONG_PROJECTION_BADGE_THAT_MUST_WRAP}}";
    let content = format!("{{{{API_HOST}}}}/v1/users/{user_token}/activity/with/path");
    let usr_start = content.find(user_token).unwrap();
    let usr_end = usr_start + user_token.len();

    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.cursor_char = 0;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        // Two badge ranges.
        let host = "{{API_HOST}}";
        let host_start = render.content().find(host).unwrap();
        let host_end = host_start + host.len();
        for (start, end) in [(host_start, host_end), (usr_start, usr_end)] {
            let slice: String = render
                .content()
                .chars()
                .skip(start)
                .take(end - start)
                .collect();
            render.range(start..end, move |_node| {
                let style = ElementStylePropSchema {
                    width: Some(crate::style::Length::px(120.0)),
                    padding: Some(
                        crate::style::Padding::uniform(crate::style::Length::px(0.0))
                            .x(crate::style::Length::px(8.0)),
                    ),
                    ..Default::default()
                };
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop("style", style)
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text(slice.clone())),
                )
            });
        }
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
            max_width: 320.0,
            max_height: 300.0,
            viewport_width: 320.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 300.0,
            viewport_width: 320.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );

    let probe_chars = [usr_start, (usr_start + usr_end) / 2, usr_end];
    arena
        .with_element_taken_ref(root, |el, arena| {
            let ta = el.as_any().downcast_ref::<TextArea>().unwrap();
            let map = super::super::super::caret_map::CaretNavigationMap::build(ta, arena);
            for char_index in probe_chars {
                assert!(
                    map.caret_stop_for_char(char_index, CaretAffinity::Downstream)
                        .is_some(),
                    "root map should expose projection atomic caret stop for char {char_index}"
                );
            }
        })
        .expect("TextArea root");

    // Interior carets render from the badge's inner text glyphs;
    // the badge bounds still contain them and affinity must not
    // produce a caret outside the badge.
    let badge_snap = arena
        .with_element_taken_ref(root, |el, arena| {
            let ta = el.as_any().downcast_ref::<TextArea>().unwrap();
            let key = ta
                .children
                .iter()
                .copied()
                .filter(|&key| {
                    arena
                        .with_element_taken_ref(key, |child, _| {
                            !child.as_any().is::<TextAreaTextRun>()
                                && !child.as_any().is::<TextAreaLineBreak>()
                        })
                        .unwrap_or(false)
                })
                .nth(1)
                .expect("user badge child");
            arena.with_element_taken_ref(key, |child, _| child.box_model_snapshot())
        })
        .flatten()
        .expect("badge snapshot");
    for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
        let cursor = (usr_start + usr_end) / 2;
        arena.with_element_taken(root, |el, _| {
            let ta = el
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root");
            ta.cursor_char = cursor;
            ta.cursor_affinity = affinity;
        });
        let (cx, cy, height) = caret_position(&arena, root);
        assert!(
            cx >= badge_snap.x - 0.5
                && cx <= badge_snap.x + badge_snap.width + 0.5
                && cy >= badge_snap.y - 0.5
                && cy + height <= badge_snap.y + badge_snap.height + 0.5,
            "interior caret must stay inside the wrapped badge for {affinity:?}: caret=({cx},{cy},{height}) badge=({},{},{},{})",
            badge_snap.x,
            badge_snap.y,
            badge_snap.width,
            badge_snap.height,
        );
    }
}

#[test]
fn projection_caret_inside_wrapped_text_honours_affinity() {
    use crate::view::base_component::text_area::caret_map::CaretAffinity;
    let mut text_area = TextArea::new();
    text_area.content = "ab/activity/with/a/very/long/pathcd".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.cursor_char = 0;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..34, |_text_area_node| {
            let style = ElementStylePropSchema {
                width: Some(Length::px(120.0)),
                height: Some(Length::px(80.0)),
                ..Default::default()
            };
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop("style", style)
            .with_child(
                RsxNode::tagged(
                    "Text",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                )
                .with_child(RsxNode::text("/activity/with/a/very/long/path")),
            )
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
            max_width: 300.0,
            max_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 10.0,
            parent_y: 20.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );

    let proj_key = projection_key(&arena, root);
    let projection_rect = snapshot(&arena, proj_key);
    let cursor = 2 + "/activity/with/a/very/long/path".chars().count() / 2;
    let mut positions = Vec::new();
    for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
        arena.with_element_taken(root, |el, _| {
            let ta = el
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root");
            ta.cursor_char = cursor;
            ta.cursor_affinity = affinity;
        });
        positions.push((affinity, caret_position(&arena, root)));
    }

    for (affinity, (x, y, height)) in positions {
        assert!(
            projection_rect.x <= x && x <= projection_rect.x + projection_rect.width,
            "{affinity:?} root caret x should stay inside projection atomic box: x={x}, rect={projection_rect:?}"
        );
        assert!(
            projection_rect.y <= y && y <= projection_rect.y + projection_rect.height,
            "{affinity:?} root caret y should stay inside projection atomic box: y={y}, rect={projection_rect:?}"
        );
        assert!(
            height > 0.0,
            "{affinity:?} root caret should expose visible height"
        );
    }
}

#[test]
fn projection_preedit_caret_follows_preedit_cursor() {
    let (arena_start, root_start) = projection_fixture_with_preedit_cursor(Some((0, 0)));
    let (arena_end, root_end) = projection_fixture_with_preedit_cursor(Some((3, 3)));

    let (start_x, start_y, _) = caret_position(&arena_start, root_start);
    let (end_x, end_y, _) = caret_position(&arena_end, root_end);

    assert!(
        end_x > start_x + 0.5,
        "preedit caret should move right when IME cursor moves to the end: start_x={start_x}, end_x={end_x}"
    );
    assert!(
        (end_y - start_y).abs() < 0.5,
        "same-line preedit caret should keep y stable: start_y={start_y}, end_y={end_y}"
    );
}
