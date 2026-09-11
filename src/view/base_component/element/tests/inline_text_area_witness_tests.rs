use super::*;
use crate::view::base_component::TextArea;

#[test]
fn inline_text_area_requires_current_measurement_alignment_and_placement() {
    let mut arena = new_test_arena();
    let mut element = Element::new_with_id(0x7e20, 0.0, 0.0, 240.0, 80.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    element.apply_style(style);
    let root = commit_element(&mut arena, Box::new(element));
    let mut editor = TextArea::new();
    editor.set_text("25".into());
    assert!(editor.inline_atomic_measurement_snapshot().is_none());
    assert!(editor.last_placement().is_none());
    let editor_key = commit_child(&mut arena, root, Box::new(editor));
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 240.0,
            max_height: 80.0,
            viewport_width: 640.0,
            viewport_height: 480.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(80.0),
        },
        LayoutPlacement {
            parent_x: 11.0,
            parent_y: 7.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 240.0,
            available_height: 80.0,
            viewport_width: 640.0,
            viewport_height: 480.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(80.0),
        },
    );
    fn clear(arena: &mut NodeArena, key: NodeKey) {
        for child in arena.children_of(key).to_vec() {
            clear(arena, child);
        }
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    clear(&mut arena, root);
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    let valid = |arena: &NodeArena| {
        arena
            .get(root)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .owning_inline_ifc_root_paint_witness(arena)
            .is_ok()
    };
    assert!(
        valid(&arena),
        "production layout must authorize the editor atomic host"
    );
    // Change only the recorded evidence, without dirty injection: the same
    // validator must reject stale inputs rather than certifying by host type.
    for drift in 0..3 {
        let mut node = arena.get_mut(editor_key).unwrap();
        let editor = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        let original = (
            editor.last_measure_constraints,
            editor.last_atomic_placement,
            editor.vertical_align,
        );
        match drift {
            0 => editor.last_measure_constraints.as_mut().unwrap().max_width += 1.0,
            1 => editor.last_atomic_placement.as_mut().unwrap().parent_x += 1.0,
            _ => editor.vertical_align = VerticalAlign::Top,
        }
        drop(node);
        assert!(
            !valid(&arena),
            "atomic evidence drift {drift} must be rejected"
        );
        let mut node = arena.get_mut(editor_key).unwrap();
        let editor = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        (
            editor.last_measure_constraints,
            editor.last_atomic_placement,
            editor.vertical_align,
        ) = original;
        drop(node);
        assert!(valid(&arena));
    }
}
