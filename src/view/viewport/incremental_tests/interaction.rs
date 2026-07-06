#![allow(unused_imports)]

use super::super::Viewport;
use super::common::*;
use crate::style::{
    ClipMode, Color, Cursor, Layout, Length, Padding, ParsedValue, Position, PropertyId,
    ScrollDirection, Style, Transition, TransitionProperty, Transitions, VerticalAlign,
};
use crate::ui::{
    Binding, DragEffect, RsxNode, RsxTagDescriptor, global_state, on_drag_over, on_drop, rsx,
};
use crate::view::Element as HostElement;

#[test]
fn drag_drop_retargets_after_drag_over_rerender() {
    let dropped = global_state(|| Vec::<String>::new());
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);

    viewport
        .render_rsx(&drag_drop_rerender_tree(false, dropped.binding()))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 200.0, 120.0);
    let old_target = viewport
        .scene
        .ui_root_keys
        .iter()
        .rev()
        .find_map(|&root_key| {
            crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, 20.0, 45.0)
        })
        .expect("initial target should hit-test");

    viewport.input_state.drag_state = Some(super::super::DragState {
        source_id: old_target,
        data: crate::ui::DataTransfer::default(),
        effect_allowed: DragEffect::Move,
        last_over_target: Some(old_target),
        last_drop_effect: Some(DragEffect::Move),
    });
    viewport.set_pointer_position_viewport(20.0, 45.0);

    viewport
        .render_rsx(&drag_drop_rerender_tree(true, dropped.binding()))
        .expect("drag-over indicator render");
    run_layout_for_test(&mut viewport, 200.0, 120.0);

    viewport.dispatch_pointer_up_event(crate::view::viewport::PointerButton::Left);

    assert_eq!(dropped.get(), vec!["target".to_string()]);
}

#[test]
fn cursor_resolves_pointer_from_hovered_text_child_ancestor() {
    let mut viewport = Viewport::new();
    viewport.set_size(160, 40);
    viewport
        .render_rsx(&pointer_cursor_with_text_child_tree())
        .expect("render pointer cursor tree");
    run_layout_for_test(&mut viewport, 160.0, 40.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let text_key =
        find_text_node(&viewport.scene.node_arena, root_key, "Hover target").expect("text child");
    let text_snapshot = viewport
        .scene
        .node_arena
        .get(text_key)
        .expect("text child")
        .element
        .box_model_snapshot();
    let target = crate::view::base_component::hit_test(
        &viewport.scene.node_arena,
        root_key,
        text_snapshot.x + text_snapshot.width * 0.5,
        text_snapshot.y + text_snapshot.height * 0.5,
    );

    assert_eq!(target, Some(text_key), "hit-test should land on text child");
    viewport.input_state.hovered_node_id = target;

    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "cursor should inherit from the hovered text child's pointer ancestor",
    );
}

#[test]
fn pointer_move_cursor_respects_root_stacking_over_anchor_parent_resize_handle() {
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);
    viewport
        .render_rsx(&overlapping_root_with_anchor_parent_resize_handle_tree())
        .expect("render overlapping root tree");
    run_layout_for_test(&mut viewport, 200.0, 120.0);

    let lower_root = viewport.scene.ui_root_keys[0];
    let handle_key = viewport.scene.node_arena.children_of(lower_root)[0];
    let higher_root = viewport.scene.ui_root_keys[1];
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            101.0,
            20.0,
        ),
        Some((1, higher_root)),
        "root children follow sibling stacking; an earlier root's escape descendant is not a top layer",
    );

    viewport.set_pointer_position_viewport(101.0, 20.0);
    assert!(
        viewport.dispatch_pointer_move_event(),
        "pointer move should update hover at the resize handle",
    );
    assert_eq!(
        viewport.input_state.hovered_node_id,
        Some(higher_root),
        "production pointer-move path should hover the later root body, not an earlier root descendant",
    );
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Default,
        "escape clipping does not promote an earlier root descendant above a later root",
    );

    let mut popup_stack = crate::view::popup_stack::PopupStack::new();
    let handle_id = viewport
        .scene
        .node_arena
        .get(handle_key)
        .expect("handle node exists")
        .element
        .stable_id();
    popup_stack.register(handle_id);
    assert_eq!(
        crate::view::base_component::hit_test_stacked(
            &viewport.scene.node_arena,
            &popup_stack,
            101.0,
            20.0,
        ),
        Some((lower_root, handle_key)),
        "PopupStack is the explicit top-layer interaction path",
    );
}

#[test]
fn hit_test_same_root_escape_descendant_respects_later_sibling_stacking() {
    let mut viewport = Viewport::new();
    viewport.set_size(220, 120);
    viewport
        .render_rsx(&same_root_escape_descendant_under_later_sibling_tree())
        .expect("render same-root stacking tree");
    run_layout_for_test(&mut viewport, 220.0, 120.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let root_children = viewport.scene.node_arena.children_of(root_key);
    let earlier_parent = root_children[0];
    let escape_child = viewport.scene.node_arena.children_of(earlier_parent)[0];
    let later_sibling = root_children[1];

    assert_eq!(
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, 105.0, 15.0,),
        Some(later_sibling),
        "within one root, a later sibling stacks above an earlier sibling's escape descendant",
    );

    let mut popup_stack = crate::view::popup_stack::PopupStack::new();
    let escape_child_id = viewport
        .scene
        .node_arena
        .get(escape_child)
        .expect("escape child exists")
        .element
        .stable_id();
    popup_stack.register(escape_child_id);
    assert_eq!(
        crate::view::base_component::hit_test_stacked(
            &viewport.scene.node_arena,
            &popup_stack,
            105.0,
            15.0,
        ),
        Some((root_key, escape_child)),
        "PopupStack can intentionally promote an escape descendant above normal sibling stacking",
    );
}

#[test]
fn cursor_resolves_from_hovered_node_key_when_stable_ids_collide() {
    let mut lower =
        crate::view::base_component::Element::new_with_id(0xC0DE, 0.0, 0.0, 100.0, 80.0);
    let mut lower_style = Style::new();
    lower_style.insert(PropertyId::Cursor, ParsedValue::Cursor(Cursor::EwResize));
    lower.apply_style(lower_style);

    let higher = crate::view::base_component::Element::new_with_id(0xC0DE, 0.0, 0.0, 100.0, 80.0);

    let mut viewport = Viewport::new();
    let lower_key = viewport
        .scene
        .node_arena
        .insert(crate::view::node_arena::Node::new(Box::new(lower)));
    let higher_key = viewport
        .scene
        .node_arena
        .insert(crate::view::node_arena::Node::new(Box::new(higher)));
    viewport.scene.ui_root_keys = vec![lower_key, higher_key];
    viewport
        .scene
        .node_arena
        .set_roots(viewport.scene.ui_root_keys.clone());
    viewport.input_state.hovered_node_id = Some(lower_key);

    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::EwResize,
        "cursor resolution must use the hovered NodeKey, not a colliding stable id from a later root"
    );
}

#[test]
fn placement_only_position_update_moves_anchor_parent_resize_handle_hit_test() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 140);
    viewport
        .render_rsx(&movable_root_with_anchor_parent_resize_handle_tree(20.0))
        .expect("render initial movable root tree");
    run_layout_for_test(&mut viewport, 240.0, 140.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let handle_key = viewport.scene.node_arena.children_of(root_key)[0];
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            121.0,
            50.0,
        ),
        Some((0, handle_key)),
        "initial right resize handle should be hit-testable"
    );

    viewport
        .render_rsx(&movable_root_with_anchor_parent_resize_handle_tree(80.0))
        .expect("render moved tree through placement-only update");
    run_layout_for_test(&mut viewport, 240.0, 140.0);

    assert_ne!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            121.0,
            50.0,
        ),
        Some((0, handle_key)),
        "old handle position must not remain clickable after placement-only move"
    );
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            181.0,
            50.0,
        ),
        Some((0, handle_key)),
        "new right resize handle position should be hit-testable after placement-only move"
    );

    viewport.set_pointer_position_viewport(181.0, 50.0);
    assert!(
        viewport.dispatch_pointer_move_event(),
        "production pointer move should update hover at the moved resize handle",
    );
    assert_eq!(
        viewport.input_state.hovered_node_id,
        Some(handle_key),
        "hover target should follow the placement-only moved resize handle"
    );
    assert_eq!(viewport.resolve_cursor(), Cursor::EwResize);
}

#[test]
fn retained_window_accordion_button_false_to_true_hit_tests_without_scroll() {
    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport
        .render_rsx(&retained_window_accordion_button_tree())
        .expect("render collapsed retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let section_key = viewport.scene.node_arena.children_of(root_key)[1];
    let button_key = viewport.scene.node_arena.children_of(section_key)[0];
    let label_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button label");

    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(section_key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("section element")
                .replace_style(expanded_retained_accordion_section_style());
            cx.invalidate(crate::view::base_component::DirtyFlags::ALL);
        })
        .expect("section exists");

    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        run_layout_for_test(&mut viewport, 460.0, 380.0);
    }

    assert_eq!(
        viewport.scene.node_arena.children_of(section_key)[0],
        button_key,
        "button NodeKey should be retained across false -> true expansion",
    );
    assert!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained") == Some(label_key),
        "label NodeKey should be retained across false -> true expansion",
    );

    let label_snapshot = viewport
        .scene
        .node_arena
        .get(label_key)
        .expect("contained label")
        .element
        .box_model_snapshot();
    let hit_x = label_snapshot.x + label_snapshot.width * 0.5;
    let hit_y = label_snapshot.y + label_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    assert!(
        matches!(target, Some(target) if target == label_key || target == button_key),
        "hit at retained expanded button ({hit_x}, {hit_y}) should target button branch; got {target:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "retained expanded button should resolve pointer cursor before any scroll",
    );
}

#[test]
fn retained_window_accordion_button_rerender_false_to_true_hit_tests_without_scroll() {
    let collapsed = retained_window_accordion_button_tree_with_expanded(false);
    let expanded = retained_window_accordion_button_tree_with_expanded(true);

    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&collapsed)
        .expect("render collapsed retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let section_key = viewport.scene.node_arena.children_of(root_key)[1];
    let button_key = viewport.scene.node_arena.children_of(section_key)[0];
    let label_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button label");

    viewport
        .render_rsx(&expanded)
        .expect("rerender expanded retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        run_layout_for_test(&mut viewport, 460.0, 380.0);
    }

    assert_eq!(
        viewport.scene.ui_root_keys[0], root_key,
        "root NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        viewport.scene.node_arena.children_of(root_key)[1],
        section_key,
        "section NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        viewport.scene.node_arena.children_of(section_key)[0],
        button_key,
        "button NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained"),
        Some(label_key),
        "label NodeKey should be retained across false -> true rerender",
    );

    let label_snapshot = viewport
        .scene
        .node_arena
        .get(label_key)
        .expect("contained label")
        .element
        .box_model_snapshot();
    let hit_x = label_snapshot.x + label_snapshot.width * 0.5;
    let hit_y = label_snapshot.y + label_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    assert!(
        matches!(target, Some(target) if target == label_key || target == button_key),
        "hit at rerendered expanded button ({hit_x}, {hit_y}) should target button branch before any scroll; got {target:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "rerendered expanded button should resolve pointer cursor before any scroll",
    );
}

#[test]
fn component_test_button_section_rerender_false_to_true_hit_tests_without_scroll() {
    let collapsed = retained_component_test_button_section_tree_with_expanded(false);
    let expanded = retained_component_test_button_section_tree_with_expanded(true);

    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&collapsed)
        .expect("render collapsed component-test-like tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let contained_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button text");
    let button_key = viewport
        .scene
        .node_arena
        .parent_of(contained_key)
        .expect("contained text parent button");

    viewport
        .render_rsx(&expanded)
        .expect("rerender expanded component-test-like tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        let profile = run_layout_for_test_with_gate_profile(&mut viewport, 460.0, 380.0);
        eprintln!("profile1 {profile:?}");
    }
    let post_layout = viewport.run_post_layout_transitions(1.0, 2.0);
    if post_layout.relayout_required {
        let profile = run_layout_for_test_with_gate_profile(&mut viewport, 460.0, 380.0);
        eprintln!("profile2 {profile:?}");
    }

    assert_eq!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained"),
        Some(contained_key),
        "contained text NodeKey should be retained across component-test-like expansion",
    );

    let contained_snapshot = viewport
        .scene
        .node_arena
        .get(contained_key)
        .expect("contained button text")
        .element
        .box_model_snapshot();
    let hit_x = contained_snapshot.x + contained_snapshot.width * 0.5;
    let hit_y = contained_snapshot.y + contained_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    {
        let bm_btn = viewport
            .scene
            .node_arena
            .get(button_key)
            .unwrap()
            .element
            .box_model_snapshot();
        let bm_txt = viewport
            .scene
            .node_arena
            .get(contained_key)
            .unwrap()
            .element
            .box_model_snapshot();
        eprintln!("DBG hit=({hit_x},{hit_y}) target={target:?}");
        eprintln!(
            "DBG button {button_key:?} x={} y={} w={} h={} render={}",
            bm_btn.x, bm_btn.y, bm_btn.width, bm_btn.height, bm_btn.should_render
        );
        eprintln!(
            "DBG text   {contained_key:?} x={} y={} w={} h={} render={}",
            bm_txt.x, bm_txt.y, bm_txt.width, bm_txt.height, bm_txt.should_render
        );
    }
    assert!(
        matches!(target, Some(target) if target == contained_key || target == button_key),
        "hit at component-test-like expanded button ({hit_x}, {hit_y}) should target the button branch before any scroll; got {target:?}, button={button_key:?}, text={contained_key:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "component-test-like expanded button should resolve pointer cursor before any scroll",
    );
}

#[test]
fn projection_text_area_caret_stays_at_insertion_point_across_frames() {
    // Chip ranges in the fixture: {{API_HOST}}=69..81, {{USER_ID}}=91..102.
    for cursor in [10_usize, 68, 69, 81, 82, 90, 91, 102, 103, 130] {
        projection_caret_probe_at(cursor);
    }
}

fn projection_caret_probe_at(cursor_char: usize) {
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea as TextAreaHost;

    let content = global_state(|| {
        String::from(
            "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line",
        )
    });
    // global_state persists across probe invocations in one process;
    // reset so every cursor position starts from the pristine fixture.
    content.binding().set(String::from(
        "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line",
    ));
    let projection_renderer = crate::ui::on_text_area_render(move |render| {
        let chars: Vec<char> = render.content().chars().collect();
        let mut ranges = Vec::new();
        let mut index = 0_usize;
        while index + 1 < chars.len() {
            if chars[index] == '{' && chars[index + 1] == '{' {
                let start = index;
                let mut cursor = index + 2;
                while cursor + 1 < chars.len() {
                    if chars[cursor] == '}' && chars[cursor + 1] == '}' {
                        ranges.push((start, cursor + 2));
                        index = cursor + 2;
                        break;
                    }
                    cursor += 1;
                }
                if cursor + 1 >= chars.len() {
                    break;
                }
                continue;
            }
            index += 1;
        }
        for (start, end) in ranges {
            let slice: String = chars[start..end].iter().collect();
            render.range(start..end, move |_node| {
                let slice = slice.clone();
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop(
                    "style",
                    crate::view::ElementStylePropSchema {
                        font_size: Some(crate::style::FontSize::Px(24.0)),
                        ..Default::default()
                    },
                )
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text(slice)),
                )
            });
        }
    });

    let tree = {
        let binding = content.binding();
        let renderer = projection_renderer.clone();
        move || {
            let binding = binding.clone();
            let renderer = renderer.clone();
            rsx! {
                <HostTextArea
                    binding={binding}
                    auto_wrap={true}
                    multiline={true}
                    on_render={renderer}
                />
            }
        }
    };

    let mut viewport = Viewport::new();
    viewport.set_size(360, 200);
    viewport.render_rsx(&tree()).expect("render TextArea");
    run_layout_for_test(&mut viewport, 360.0, 200.0);
    viewport.refresh_frame_box_models();
    let root_key = viewport.scene.ui_root_keys[0];

    viewport.set_focused_node_id(Some(root_key));
    viewport
        .scene
        .node_arena
        .with_element_taken(root_key, |el, _| {
            let text_area = el
                .as_any_mut()
                .downcast_mut::<TextAreaHost>()
                .expect("TextArea root");
            text_area.cursor_char = cursor_char;
        });

    let caret_before = viewport
        .scene
        .node_arena
        .with_element_taken_ref(root_key, |el, arena| {
            el.as_any()
                .downcast_ref::<TextAreaHost>()
                .unwrap()
                .caret_screen_position(arena)
        })
        .expect("root")
        .expect("caret before typing");

    // Sequence-type three characters, one full frame each (rsx rebuild
    // with the binding round-trip + projection re-render, then layout).
    let mut previous = caret_before;
    for step in 0..3 {
        assert!(viewport.dispatch_text_input_event("X".to_string()));
        viewport
            .render_rsx(&tree())
            .expect("re-render after typing");
        run_layout_for_test(&mut viewport, 360.0, 200.0);
        viewport.refresh_frame_box_models();

        let caret_after = viewport
            .scene
            .node_arena
            .with_element_taken_ref(root_key, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextAreaHost>()
                    .unwrap()
                    .caret_screen_position(arena)
            })
            .expect("root")
            .expect("caret after typing");

        // Oracle: a from-scratch layout of the post-edit content with the
        // same cursor gives the ground-truth caret. Incremental frames
        // must land on the same position (wrap reflow included).
        let expected = {
            let post_content = content.binding().get();
            let cursor_now = viewport
                .scene
                .node_arena
                .with_element_taken_ref(root_key, |el, _| {
                    el.as_any()
                        .downcast_ref::<TextAreaHost>()
                        .unwrap()
                        .cursor_char
                })
                .expect("root");
            let fresh_state = global_state(|| post_content.clone());
            fresh_state.binding().set(post_content.clone());
            let fresh_renderer = projection_renderer.clone();
            let fresh_tree = {
                let binding = fresh_state.binding();
                rsx! {
                    <HostTextArea
                        binding={binding}
                        auto_wrap={true}
                        multiline={true}
                        on_render={fresh_renderer}
                    />
                }
            };
            let mut fresh = Viewport::new();
            fresh.set_size(360, 200);
            fresh.render_rsx(&fresh_tree).expect("render oracle");
            run_layout_for_test(&mut fresh, 360.0, 200.0);
            fresh.refresh_frame_box_models();
            let fresh_key = fresh.scene.ui_root_keys[0];
            fresh
                .scene
                .node_arena
                .with_element_taken(fresh_key, |el, _| {
                    el.as_any_mut()
                        .downcast_mut::<TextAreaHost>()
                        .unwrap()
                        .cursor_char = cursor_now;
                });
            fresh
                .scene
                .node_arena
                .with_element_taken_ref(fresh_key, |el, arena| {
                    el.as_any()
                        .downcast_ref::<TextAreaHost>()
                        .unwrap()
                        .caret_screen_position(arena)
                })
                .expect("oracle root")
                .expect("oracle caret")
        };
        let dx = (caret_after.0 - expected.0).abs();
        let dy = (caret_after.1 - expected.1).abs();
        assert!(
            dx < 1.0 && dy < 1.0,
            "cursor {cursor_char} step {step}: incremental caret must match a fresh layout: incremental={caret_after:?} fresh={expected:?}"
        );
        previous = caret_after;
    }

    // IME preedit at the same spot: the caret must track the preedit
    // cursor, not jump to another position. Skip positions at/inside a
    // projection chip — there the preedit is hosted by the projection and
    // anchors to the chip instead of the committed-text line.
    let cursor_now = viewport
        .scene
        .node_arena
        .with_element_taken_ref(root_key, |el, _| {
            el.as_any()
                .downcast_ref::<TextAreaHost>()
                .unwrap()
                .cursor_char
        })
        .expect("root");
    let near_chip = {
        let text = content.binding().get();
        let chars: Vec<char> = text.chars().collect();
        let probe = |index: usize| -> bool {
            index + 1 < chars.len()
                && ((chars[index] == '{' && chars[index + 1] == '{')
                    || (chars[index] == '}' && chars[index + 1] == '}'))
        };
        (cursor_now.saturating_sub(13)..(cursor_now + 2).min(chars.len())).any(probe)
    };
    if near_chip {
        return;
    }
    assert!(viewport.dispatch_ime_preedit_event("zh".to_string(), Some((2, 2))));
    viewport
        .render_rsx(&tree())
        .expect("re-render after preedit");
    run_layout_for_test(&mut viewport, 360.0, 200.0);
    viewport.refresh_frame_box_models();
    let caret_preedit = viewport
        .scene
        .node_arena
        .with_element_taken_ref(root_key, |el, arena| {
            el.as_any()
                .downcast_ref::<TextAreaHost>()
                .unwrap()
                .caret_screen_position(arena)
        })
        .expect("root")
        .expect("caret during preedit");
    let dy = (caret_preedit.1 - previous.1).abs();
    let dx = caret_preedit.0 - previous.0;
    assert!(
        dy < 9.0 && dx > 0.5 && dx < 60.0,
        "cursor {cursor_char}: preedit caret must sit after the preedit text: before={previous:?} preedit={caret_preedit:?}"
    );
}
