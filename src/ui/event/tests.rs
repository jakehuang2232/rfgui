use super::*;
use crate::platform::input::Key;

fn new_meta() -> EventMeta {
    EventMeta::new(NodeId::default())
}

#[test]
fn event_meta_default_flags() {
    let meta = new_meta();
    assert_eq!(meta.phase(), EventPhase::None);
    assert_eq!(meta.source(), EventSource::Platform);
    assert!(meta.is_trusted());
    assert!(meta.bubbles());
    assert!(meta.cancelable());
    assert!(!meta.propagation_stopped());
    assert!(!meta.immediate_propagation_stopped());
    assert!(!meta.default_prevented());
    assert!(!meta.handled());
    assert!(meta.event_id() >= 1);
}

#[test]
fn prevent_default_gated_by_cancelable() {
    let mut meta = new_meta();
    meta.prevent_default();
    assert!(meta.default_prevented());

    let mut meta2 = new_meta();
    meta2.set_cancelable(false);
    meta2.prevent_default();
    assert!(
        !meta2.default_prevented(),
        "non-cancelable must ignore prevent_default"
    );
}

#[test]
fn stop_immediate_propagation_implies_stop_propagation() {
    let mut meta = new_meta();
    meta.stop_immediate_propagation();
    assert!(meta.propagation_stopped());
    assert!(meta.immediate_propagation_stopped());
}

#[test]
fn event_ids_are_unique_and_monotonic() {
    let a = new_meta().event_id();
    let b = new_meta().event_id();
    let c = new_meta().event_id();
    assert!(b > a && c > b, "event ids must strictly increase");
}

#[test]
fn is_trusted_reflects_source() {
    let mut meta = new_meta();
    assert!(meta.is_trusted());
    meta.set_source(EventSource::Synthetic);
    assert!(!meta.is_trusted());
    meta.set_source(EventSource::Scripted);
    assert!(!meta.is_trusted());
    meta.set_source(EventSource::Platform);
    assert!(meta.is_trusted());
}

#[test]
fn key_location_from_key_maps_split_and_numpad() {
    assert_eq!(KeyLocation::from_key(Key::ShiftLeft), KeyLocation::Left);
    assert_eq!(KeyLocation::from_key(Key::ShiftRight), KeyLocation::Right);
    assert_eq!(KeyLocation::from_key(Key::ControlLeft), KeyLocation::Left);
    assert_eq!(KeyLocation::from_key(Key::NumberPad7), KeyLocation::Numpad);
    assert_eq!(
        KeyLocation::from_key(Key::NumberPadEnter),
        KeyLocation::Numpad
    );
    assert_eq!(KeyLocation::from_key(Key::KeyA), KeyLocation::Standard);
    assert_eq!(KeyLocation::from_key(Key::Enter), KeyLocation::Standard);
}

#[test]
fn data_transfer_round_trips_text_and_files() {
    use std::path::PathBuf;
    let mut dt = DataTransfer::new();
    dt.set_text("hello");
    dt.add(DragPayload::Files(vec![PathBuf::from("/tmp/a.txt")]));
    assert_eq!(dt.text().as_deref(), Some("hello"));
    assert_eq!(dt.files().unwrap(), vec![PathBuf::from("/tmp/a.txt")]);
    assert_eq!(dt.items().len(), 2);
}

#[test]
fn data_transfer_effect_allowed_defaults_to_none() {
    let mut dt = DataTransfer::new();
    assert_eq!(dt.effect_allowed(), DragEffect::None);
    dt.set_effect_allowed(DragEffect::Copy);
    assert_eq!(dt.effect_allowed(), DragEffect::Copy);
}

#[test]
fn drag_over_accept_sets_effect_and_prevents_default() {
    let meta = new_meta();
    let pointer = PointerEventData {
        viewport_x: 0.0,
        viewport_y: 0.0,
        local_x: 0.0,
        local_y: 0.0,
        button: None,
        buttons: PointerButtons::default(),
        modifiers: Modifiers::default(),
        pointer_id: 0,
        pointer_type: PointerType::Mouse,
        pressure: 0.0,
        timestamp: crate::time::Instant::now(),
    };
    let mut ev = DragOverEvent {
        meta,
        pointer,
        data: DataTransfer::new(),
        drop_effect: None,
    };
    ev.accept(DragEffect::Copy);
    assert_eq!(ev.drop_effect, Some(DragEffect::Copy));
    assert!(ev.meta.default_prevented());
}

#[test]
fn composed_path_empty_by_default() {
    let meta = new_meta();
    assert!(meta.composed_path().is_empty());
}

#[test]
fn related_target_round_trip() {
    let mut meta = new_meta();
    assert!(meta.related_target().is_none());
    let other = EventTarget::bare(NodeId::default());
    meta.set_related_target(Some(other));
    assert!(meta.related_target().is_some());
}

#[test]
fn viewport_commands_push_into_action_queue() {
    let mut meta = new_meta();
    let mut vp = meta.viewport();
    vp.request_redraw();
    vp.write_clipboard("abc");
    vp.window_command(crate::platform::WindowCommand::Minimize);
    vp.request_paste();
    let actions = meta.take_viewport_listener_actions();
    assert_eq!(actions.len(), 4);
    assert!(matches!(actions[0], EventCommand::RequestRedraw));
    assert!(matches!(actions[1], EventCommand::WriteClipboard(_)));
    assert!(matches!(
        actions[2],
        EventCommand::Window(crate::platform::WindowCommand::Minimize)
    ));
    assert!(matches!(actions[3], EventCommand::RequestPaste));
}
