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

pub(crate) fn note_witness_check() {
    WITNESS_CHECKS.with(|n| n.set(n.get() + 1));
}
pub(crate) fn witness_checks() -> usize {
    WITNESS_CHECKS.with(Cell::get)
}
