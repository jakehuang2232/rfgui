use super::*;

#[test]
fn text_area_v2_content_spawns_a_text_run_and_shapes() {
    let tree = host_text_area_node().with_prop("content", "hello world");
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let (w, h, is_run) = measured_run_size(&arena, root);
    assert!(is_run, "TextArea's first child must be a TextAreaTextRun");
    assert!(w > 0.0, "Run must have shaped width, got {w}");
    assert!(h > 0.0, "Run must have shaped height, got {h}");

    // TextArea itself wraps the run and reports the same content extent.
    let ta_snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    assert!(ta_snapshot.width >= w - 0.5);
    assert!(ta_snapshot.height >= h - 0.5);
}

#[test]
fn text_area_v2_cursor_style_cascades_to_generated_run() {
    let style = ElementStylePropSchema {
        cursor: Some(Cursor::Pointer),
        ..empty_element_style()
    };
    let tree = host_text_area_node()
        .with_prop("content", "hello world")
        .with_prop("style", style);
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let root_stable_id = arena.get(root).unwrap().element.stable_id();
    let root_cursor = get_cursor_by_id(&arena, root, root_stable_id).expect("root cursor exists");
    assert_eq!(root_cursor, Cursor::Pointer);

    let run = *arena
        .children_of(root)
        .first()
        .expect("TextArea should spawn a generated run");
    let run_stable_id = arena.get(run).unwrap().element.stable_id();
    let run_cursor = get_cursor_by_id(&arena, root, run_stable_id).expect("run cursor exists");
    assert_eq!(run_cursor, Cursor::Pointer);
}

#[test]
fn text_area_v2_cursor_style_cascades_to_projection_text() {
    let style = ElementStylePropSchema {
        cursor: Some(Cursor::Text),
        ..empty_element_style()
    };
    let tree = host_text_area_node()
        .with_prop("content", "aa/v1/users/bb")
        .with_prop("style", style)
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(2..12, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("/v1/users/")))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let projection = arena.children_of(root)[1];
    let mut stack = arena.children_of(projection);
    let mut projection_text = None;
    while let Some(key) = stack.pop() {
        if arena
            .get(key)
            .is_some_and(|node| node.element.as_any().is::<Text>())
        {
            projection_text = Some(key);
            break;
        }
        stack.extend(arena.children_of(key));
    }
    let projection_text = projection_text.expect("projection should contain Text");
    let stable_id = arena.get(projection_text).unwrap().element.stable_id();
    let cursor = get_cursor_by_id(&arena, root, stable_id).expect("cursor exists");
    assert_eq!(cursor, Cursor::Text);
}

#[test]
fn text_area_v2_plain_run_between_projections_hit_tests_as_text_cursor() {
    let tree = host_text_area_node()
        .with_prop("content", "{{API_HOST}}/v1/users/{{USER_ID}}/activity")
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(0..12, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("{{API_HOST}}")))
                    });
                    render.range(22..33, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("{{USER_ID}}")))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let children = arena.children_of(root);
    assert_eq!(children.len(), 4);
    let middle_run = children[1];
    assert!(
        arena
            .get(middle_run)
            .is_some_and(|node| node.element.as_any().is::<TextAreaTextRun>()),
        "expected /v1/users/ to be a generated TextAreaTextRun",
    );
    let snap = arena.get(middle_run).unwrap().element.box_model_snapshot();
    let target = hit_test(
        &arena,
        root,
        snap.x + snap.width * 0.5,
        snap.y + snap.height * 0.5,
    )
    .expect("hit-test should find the middle plain run");
    let stable_id = arena.get(target).unwrap().element.stable_id();
    let cursor = get_cursor_by_id(&arena, root, stable_id).expect("cursor exists");
    assert_eq!(cursor, Cursor::Text);
}

#[test]
fn text_area_v2_projection_applies_on_first_measure() {
    let tree = host_text_area_node()
        .with_prop("content", "abXYZcd")
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(2..5, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("XYZ")))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let children = arena.children_of(root);
    assert_eq!(
        children.len(),
        3,
        "first measure should rebuild into Run / projection / Run",
    );
    assert!(
        !arena
            .get(children[1])
            .unwrap()
            .element
            .as_any()
            .is::<crate::view::base_component::text_area::TextAreaTextRun>(),
        "middle child should be projection output, not the original plain Run",
    );
    assert!(
        subtree_has_text_descendant(&arena, children[1]),
        "projection subtree should contain the projected Text on the first frame",
    );
}

#[test]
fn text_area_v2_empty_content_with_placeholder_spawns_placeholder_run() {
    let tree = host_text_area_node().with_prop("placeholder", "type here");
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let (_, _, is_run) = measured_run_size(&arena, root);
    assert!(is_run, "Placeholder fallback must spawn a Run");
    let run_key = *arena.children_of(root).first().unwrap();
    let is_placeholder = arena
        .get(run_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
        .unwrap()
        .is_placeholder;
    assert!(
        is_placeholder,
        "placeholder Run must carry is_placeholder=true"
    );
}

#[test]
fn text_area_incremental_max_length_normalizes_existing_content() {
    let tree = host_text_area_node().with_prop("content", "abcdef");
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let viewport_style = crate::style::Style::new();
    let ctx = crate::view::fiber_work::ApplyContext {
        viewport_style: &viewport_style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };

    arena.with_element_taken(root, |element, arena_ref| {
        {
            let text_area = element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root");
            text_area.cursor_char = 6;
            text_area.selection_anchor_char = Some(1);
            text_area.selection_focus_char = Some(6);
            text_area.ime_preedit = "pending".to_string();
            text_area.ime_preedit_cursor = Some((7, 7));
        }
        element.apply_prop(
            arena_ref,
            root,
            &ctx,
            "max_length",
            crate::ui::PropValue::I64(3),
        );
    });

    let root_node = arena.get(root).expect("TextArea root");
    let text_area = root_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("TextArea root");
    assert_eq!(text_area.content, "abc");
    assert_eq!(text_area.cursor_char, 3);
    assert_eq!(text_area.selection_anchor_char, None);
    assert_eq!(text_area.selection_focus_char, None);
    assert!(text_area.ime_preedit.is_empty());
    assert_eq!(text_area.ime_preedit_cursor, None);
}

#[test]
fn text_area_v2_no_content_no_placeholder_has_no_children() {
    let tree = host_text_area_node();
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    assert!(arena.children_of(root).is_empty());
}

// ---------------------------------------------------------------------------
// Phase 6b regression: a downstream-style custom host that implements
// `HostBuilder` (no `RsxTag` boilerplate) gets dispatched through
// `host_builder_node` without any change to renderer_adapter.
// ---------------------------------------------------------------------------
