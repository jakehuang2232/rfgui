//! Clipboard wiring: viewport-level Cmd/Ctrl+C/X/V detection and the
//! corresponding `dispatch_copy_event` / `dispatch_cut_event` /
//! `dispatch_paste_event` round-trips through `TextArea`.

#![cfg(test)]

use super::Viewport;
use crate::platform::{Key, Modifiers};
use crate::ui::{KeyEventData, KeyLocation, RsxNode, RsxTagDescriptor};
use crate::view::base_component::{LayoutConstraints, LayoutPlacement, TextArea};
use crate::view::tags::TextArea as TextAreaTag;

fn modifiers_for(shift: bool, ctrl: bool, alt: bool, meta: bool) -> Modifiers {
    let mut mods = Modifiers::empty();
    if shift {
        mods |= Modifiers::SHIFT;
    }
    if ctrl {
        mods |= Modifiers::CTRL;
    }
    if alt {
        mods |= Modifiers::ALT;
    }
    if meta {
        mods |= Modifiers::META;
    }
    mods
}

fn key_data(key: Key, ctrl: bool, meta: bool) -> KeyEventData {
    KeyEventData {
        key,
        characters: None,
        modifiers: modifiers_for(false, ctrl, false, meta),
        repeat: false,
        is_composing: false,
        location: KeyLocation::Standard,
        timestamp: crate::time::Instant::now(),
    }
}

fn text_area_tree(content: &str) -> RsxNode {
    RsxNode::tagged("TextArea", RsxTagDescriptor::for_tag::<TextAreaTag>())
        .with_prop("content", content.to_string())
}

fn run_layout(viewport: &mut Viewport, w: f32, h: f32) {
    let constraints = LayoutConstraints {
        max_width: w,
        max_height: h,
        viewport_width: w,
        viewport_height: h,
        percent_base_width: Some(w),
        percent_base_height: Some(h),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: w,
        available_height: h,
        viewport_width: w,
        viewport_height: h,
        percent_base_width: Some(w),
        percent_base_height: Some(h),
    };
    let mut arena = std::mem::take(&mut viewport.scene.node_arena);
    let root_keys = viewport.scene.ui_root_keys.clone();
    for &root in &root_keys {
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
    }
    viewport.scene.node_arena = arena;
}

fn build_viewport(content: &str) -> (Viewport, crate::view::node_arena::NodeKey) {
    let mut viewport = Viewport::new();
    viewport.set_size(400, 200);
    viewport
        .render_rsx(&text_area_tree(content))
        .expect("render TextArea");
    run_layout(&mut viewport, 400.0, 200.0);
    let root = viewport.scene.ui_root_keys[0];
    viewport.set_focused_node_id(Some(root));
    (viewport, root)
}

fn set_selection(viewport: &mut Viewport, root: crate::view::node_arena::NodeKey, start: usize, end: usize) {
    let mut arena = std::mem::take(&mut viewport.scene.node_arena);
    arena.with_element_taken(root, |el, _| {
        let ta = el
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("root is TextArea");
        ta.selection_anchor_char = Some(start);
        ta.selection_focus_char = Some(end);
        ta.cursor_char = end;
    });
    viewport.scene.node_arena = arena;
}

fn read_content(viewport: &mut Viewport, root: crate::view::node_arena::NodeKey) -> String {
    let mut arena = std::mem::take(&mut viewport.scene.node_arena);
    let mut content = String::new();
    arena.with_element_taken(root, |el, _| {
        let ta = el
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("root is TextArea");
        content = ta.content.clone();
    });
    viewport.scene.node_arena = arena;
    content
}

#[test]
fn cmd_c_with_selection_queues_clipboard_write() {
    let (mut viewport, root) = build_viewport("hello world");
    set_selection(&mut viewport, root, 0, 5);

    let handled = viewport.dispatch_key_down_event(key_data(Key::KeyC, false, true));
    assert!(handled, "Cmd+C should be marked handled");

    assert_eq!(
        viewport.pending_platform_requests.clipboard_write.as_deref(),
        Some("hello"),
        "clipboard_write should hold the selection"
    );
}

#[test]
fn cmd_c_without_selection_writes_nothing() {
    let (mut viewport, _root) = build_viewport("hello world");

    viewport.dispatch_key_down_event(key_data(Key::KeyC, false, true));

    assert!(
        viewport.pending_platform_requests.clipboard_write.is_none(),
        "no selection => no clipboard write"
    );
}

#[test]
fn cmd_x_writes_clipboard_and_deletes_selection() {
    let (mut viewport, root) = build_viewport("hello world");
    set_selection(&mut viewport, root, 0, 5);

    viewport.dispatch_key_down_event(key_data(Key::KeyX, false, true));

    assert_eq!(
        viewport.pending_platform_requests.clipboard_write.as_deref(),
        Some("hello"),
    );
    assert_eq!(read_content(&mut viewport, root), " world");
}

#[test]
fn cmd_x_on_read_only_copies_but_does_not_delete() {
    let (mut viewport, root) = build_viewport("hello world");
    {
        let mut arena = std::mem::take(&mut viewport.scene.node_arena);
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .read_only = true;
        });
        viewport.scene.node_arena = arena;
    }
    set_selection(&mut viewport, root, 0, 5);

    viewport.dispatch_key_down_event(key_data(Key::KeyX, false, true));

    assert_eq!(
        viewport.pending_platform_requests.clipboard_write.as_deref(),
        Some("hello"),
    );
    assert_eq!(read_content(&mut viewport, root), "hello world");
}

#[test]
fn cmd_v_sets_request_paste_flag() {
    let (mut viewport, _root) = build_viewport("");

    viewport.dispatch_key_down_event(key_data(Key::KeyV, false, true));

    assert!(
        viewport.pending_platform_requests.request_paste,
        "Cmd+V should ask the runner for a paste",
    );
}

#[test]
fn paste_event_inserts_clipboard_text_into_text_area() {
    let (mut viewport, root) = build_viewport("ab");
    set_selection(&mut viewport, root, 2, 2);

    let handled = viewport.dispatch_paste_event("XY".to_string());
    assert!(handled);
    assert_eq!(read_content(&mut viewport, root), "abXY");
}

#[test]
fn paste_event_replaces_selection() {
    let (mut viewport, root) = build_viewport("hello world");
    set_selection(&mut viewport, root, 0, 5);

    viewport.dispatch_paste_event("HEY".to_string());
    assert_eq!(read_content(&mut viewport, root), "HEY world");
}

#[test]
fn paste_event_on_read_only_does_not_modify_content() {
    let (mut viewport, root) = build_viewport("locked");
    {
        let mut arena = std::mem::take(&mut viewport.scene.node_arena);
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .read_only = true;
        });
        viewport.scene.node_arena = arena;
    }

    viewport.dispatch_paste_event("XYZ".to_string());
    assert_eq!(read_content(&mut viewport, root), "locked");
}

#[test]
fn ctrl_shift_c_does_not_trigger_copy() {
    let (mut viewport, root) = build_viewport("hello world");
    set_selection(&mut viewport, root, 0, 5);

    let mut data = key_data(Key::KeyC, true, false);
    data.modifiers = modifiers_for(true, true, false, false);
    viewport.dispatch_key_down_event(data);

    assert!(
        viewport.pending_platform_requests.clipboard_write.is_none(),
        "Ctrl+Shift+C must not trigger Copy",
    );
}
