use super::*;

#[test]
fn enqueue_then_drain_preserves_order() {
    let _ = drain_viewport_actions();
    let h = use_viewport();
    h.set_debug_trace_fps(true);
    h.set_debug_geometry_overlay(false);
    h.request_redraw();
    let actions = drain_viewport_actions();
    assert_eq!(
        actions,
        vec![
            ViewportAction::SetDebugTraceFps(true),
            ViewportAction::SetDebugGeometryOverlay(false),
            ViewportAction::RequestRedraw,
        ]
    );
}

#[test]
fn drain_empties_queue() {
    let _ = drain_viewport_actions();
    use_viewport().set_debug_trace_render_time(true);
    let first = drain_viewport_actions();
    assert_eq!(first.len(), 1);
    let second = drain_viewport_actions();
    assert!(second.is_empty());
}

#[test]
fn retained_auto_debug_setters_enqueue_explicit_actions() {
    let _ = drain_viewport_actions();
    let h = use_viewport();
    h.set_debug_retained_auto_overlay(true);
    h.set_debug_retained_auto_authority(false);
    h.set_debug_retained_auto_reuse_actions(true);
    h.set_debug_retained_auto_fallback_reasons(false);
    assert_eq!(
        drain_viewport_actions(),
        vec![
            ViewportAction::SetDebugRetainedAutoOverlay(true),
            ViewportAction::SetDebugRetainedAutoAuthority(false),
            ViewportAction::SetDebugRetainedAutoReuseActions(true),
            ViewportAction::SetDebugRetainedAutoFallbackReasons(false),
        ]
    );
}
