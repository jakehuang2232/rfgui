use super::*;
use std::cell::Cell;

thread_local! {
    static WITNESS_CHECKS: Cell<usize> = const { Cell::new(0) };
    static GEOMETRY_REBUILDS: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn note_geometry_rebuild() {
    GEOMETRY_REBUILDS.with(|count| count.set(count.get() + 1));
}

fn assert_preflight_does_not_rebuild_geometry(with_atomic: bool) {
    let before_layout = GEOMETRY_REBUILDS.with(Cell::get);
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7e10, 0.0, 0.0, 240.0, 400.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(400.0)));
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    let text = commit_child(
        &mut arena,
        root,
        Box::new(Text::from_content(&"預檢 hello e\u{301} שלום ".repeat(128))),
    );
    let mut owners = vec![root, text];
    if with_atomic {
        let atomic = commit_child(
            &mut arena,
            root,
            Box::new(Element::new_with_id(0x7e11, 0.0, 0.0, 24.0, 18.0)),
        );
        owners.push(atomic);
    }
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 240.0,
            max_height: 400.0,
            viewport_width: 240.0,
            viewport_height: 400.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(400.0),
        },
        LayoutPlacement {
            available_width: 240.0,
            available_height: 400.0,
            viewport_width: 240.0,
            viewport_height: 400.0,
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(400.0),
        },
    );
    for owner in owners {
        arena
            .get_mut(owner)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    let after_layout = GEOMETRY_REBUILDS.with(Cell::get);
    assert!(
        after_layout > before_layout,
        "the fixture must build real IFC geometry during layout"
    );
    let node = arena.get(root).unwrap();
    let element = node.element.as_any().downcast_ref::<Element>().unwrap();
    for _ in 0..3 {
        assert_eq!(element.owning_inline_ifc_root_paint_witness(&arena), Ok(()));
    }
    // Count work rather than wall time: this must also fail on a fast machine
    // if preflight starts rebuilding the unused caret/glyph geometry again.
    assert_eq!(GEOMETRY_REBUILDS.with(Cell::get), after_layout);
}

#[test]
fn owning_inline_text_preflight_does_not_rebuild_layout_geometry() {
    assert_preflight_does_not_rebuild_geometry(false);
}

#[test]
fn owning_inline_atomic_preflight_does_not_rebuild_text_geometry() {
    assert_preflight_does_not_rebuild_geometry(true);
}

/// A moved inline root installs its text and span geometry again at the new
/// content origin. The paint witness re-derives the same geometry, so it must
/// accept every origin, including when the first line sits above the content
/// top. A mismatch fails a retained frame back to the legacy renderer.
#[test]
fn moved_inline_root_witness_accepts_every_fractional_origin() {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7e20, 0.0, 0.0, 160.0, 0.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
    style.insert(PropertyId::Height, ParsedValue::Auto);
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    // A line height below the font's own height puts the first line above
    // the content top.
    let tight_text = |content: &str| {
        let mut text = Text::from_content(content);
        text.set_font_size(14.0);
        text.set_line_height(0.8);
        Box::new(text)
    };
    let text = commit_child(&mut arena, root, tight_text("Third Party Licenses "));
    let mut span = Element::new_with_id(0x7e21, 0.0, 0.0, 0.0, 0.0);
    let mut span_style = Style::new();
    span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    span_style.insert(PropertyId::Width, ParsedValue::Auto);
    span_style.insert(PropertyId::Height, ParsedValue::Auto);
    span.apply_style(span_style);
    let span = commit_child(&mut arena, root, Box::new(span));
    let span_text = commit_child(
        &mut arena,
        span,
        tight_text("Apache License 2.0 moxcms pxfm"),
    );
    let constraints = LayoutConstraints {
        max_width: 160.0,
        max_height: 400.0,
        viewport_width: 800.0,
        viewport_height: 800.0,
        percent_base_width: Some(160.0),
        percent_base_height: Some(400.0),
    };
    // Origins across 512, where content and text rects fall in different
    // f32 binades and differently ordered sums round apart.
    for step in 0..400_u16 {
        let parent_y = 500.0 + f32::from(step) * 0.0731;
        measure_and_place(
            &mut arena,
            root,
            constraints,
            LayoutPlacement {
                available_width: 160.0,
                available_height: 400.0,
                viewport_width: 800.0,
                viewport_height: 800.0,
                parent_x: 37.0,
                parent_y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(400.0),
            },
        );
        for owner in [root, text, span, span_text] {
            arena
                .get_mut(owner)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        let node = arena.get(root).unwrap();
        let element = node.element.as_any().downcast_ref::<Element>().unwrap();
        let top_offset = element
            .inline_ifc_layout_call_site
            .current
            .as_ref()
            .expect("the root installed its inline formatting context")
            .content_top_offset;
        assert!(
            top_offset < 0.0,
            "the fixture's first line must sit above the content top"
        );
        assert_eq!(
            element.owning_inline_ifc_root_paint_witness(&arena),
            Ok(()),
            "root origin y {parent_y}"
        );
    }
}

pub(crate) fn note_witness_check() {
    WITNESS_CHECKS.with(|n| n.set(n.get() + 1));
}
pub(crate) fn witness_checks() -> usize {
    WITNESS_CHECKS.with(Cell::get)
}

/// A column moved by its parent shifts its inline formatting context roots
/// through the translation fast path: each root installs its text again at
/// the moved origin instead of running a full place, its paint witness
/// accepts the result, and the text lands where a fresh place puts it.
#[test]
fn moved_column_translates_inline_roots_without_placing_them() {
    let build = || {
        let mut arena = new_test_arena();
        let mut column = Element::new_with_id(0x7e40, 0.0, 0.0, 200.0, 0.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().column().into()),
        );
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        style.insert(PropertyId::Height, ParsedValue::Auto);
        column.apply_style(style);
        let column = commit_element(&mut arena, Box::new(column));
        let mut nodes = vec![column];
        let mut roots = Vec::new();
        let mut texts = Vec::new();
        for (index, content) in ["Third Party Licenses", "Apache License 2.0"]
            .into_iter()
            .enumerate()
        {
            let mut root = Element::new_with_id(0x7e41 + index as u64, 0.0, 0.0, 0.0, 0.0);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
            style.insert(PropertyId::Width, ParsedValue::Auto);
            style.insert(PropertyId::Height, ParsedValue::Auto);
            root.apply_style(style);
            let root = commit_child(&mut arena, column, Box::new(root));
            let text = commit_child(&mut arena, root, Box::new(Text::from_content(content)));
            nodes.extend([root, text]);
            roots.push(root);
            texts.push(text);
        }
        (arena, column, nodes, roots, texts)
    };
    let place_at = |arena: &mut NodeArena, column, parent_y: f32| {
        measure_and_place(
            arena,
            column,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 400.0,
                viewport_width: 800.0,
                viewport_height: 800.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(400.0),
            },
            LayoutPlacement {
                available_width: 200.0,
                available_height: 400.0,
                viewport_width: 800.0,
                viewport_height: 800.0,
                parent_x: 11.0,
                parent_y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(400.0),
            },
        );
    };
    let text_position = |arena: &NodeArena, text| {
        let node = arena.get(text).unwrap();
        let text = node.element.as_any().downcast_ref::<Text>().unwrap();
        [
            text.layout_state.layout_position.x,
            text.layout_state.layout_position.y,
        ]
    };

    let (mut arena, column, nodes, roots, texts) = build();
    place_at(&mut arena, column, 0.0);
    for &node in &nodes {
        arena
            .get_mut(node)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(column, DirtyFlags::ALL);
    crate::view::base_component::reset_layout_place_profile();
    crate::view::base_component::set_layout_place_profile_enabled(true);
    place_at(&mut arena, column, 37.25);
    crate::view::base_component::set_layout_place_profile_enabled(false);
    let profile = crate::view::base_component::take_layout_place_profile();
    assert_eq!(
        (
            profile.translated_subtree_roots,
            profile.child_place_calls,
            profile.inline_ifc_root_install_reuse_calls,
        ),
        (2, 0, 2),
        "both inline roots follow the move by translation"
    );

    let (mut fresh, fresh_column, _, _, fresh_texts) = build();
    place_at(&mut fresh, fresh_column, 37.25);
    for (&root, (&text, &fresh_text)) in roots.iter().zip(texts.iter().zip(&fresh_texts)) {
        let node = arena.get(root).unwrap();
        let element = node.element.as_any().downcast_ref::<Element>().unwrap();
        assert_eq!(element.owning_inline_ifc_root_paint_witness(&arena), Ok(()));
        drop(node);
        let [x, y] = text_position(&arena, text);
        let [fresh_x, fresh_y] = text_position(&fresh, fresh_text);
        assert!(
            (x - fresh_x).abs() <= 1e-3 && (y - fresh_y).abs() <= 1e-3,
            "translated text at ({x}, {y}), freshly placed at ({fresh_x}, {fresh_y})"
        );
    }
}
