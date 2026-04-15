//! `use_viewport` hook and deferred viewport action queue.
//!
//! Lets component event handlers mutate viewport state (primarily debug
//! toggles) without either (a) threading `event.viewport` through every
//! callback closure or (b) routing values through global atomics that the
//! main render loop then re-applies.
//!
//! Usage:
//!
//! ```ignore
//! let viewport = use_viewport();
//! on_click(move |_| viewport.set_debug_trace_render_time(true))
//! ```
//!
//! The handle is zero-sized and cheap to clone. Calls enqueue
//! `ViewportAction`s into a thread-local buffer; the viewport drains and
//! applies the buffer at the top of each render pass.

use crate::Color;
use std::cell::RefCell;

thread_local! {
    static VIEWPORT_ACTIONS: RefCell<Vec<ViewportAction>> = const { RefCell::new(Vec::new()) };
}

/// Queued mutation to be applied to the live `Viewport` on the next
/// render pass. Variants map 1:1 to `ViewportControl` setters so the
/// dispatch site stays mechanical.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportAction {
    SetDebugTraceFps(bool),
    SetDebugTraceRenderTime(bool),
    SetDebugTraceLayoutDetail(bool),
    SetDebugTraceCompileDetail(bool),
    SetDebugTraceExecuteDetail(bool),
    SetDebugTraceReusePath(bool),
    SetDebugGeometryOverlay(bool),
    SetPromotionEnabled(bool),
    SetClearColor(Color),
    RequestRedraw,
}

/// Handle returned by [`use_viewport`]. Methods do not touch the live
/// viewport directly — they push an action onto the thread-local queue
/// which the engine drains before the next render.
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewportHandle;

impl ViewportHandle {
    fn push(action: ViewportAction) {
        VIEWPORT_ACTIONS.with(|q| q.borrow_mut().push(action));
    }

    pub fn set_debug_trace_fps(&self, enabled: bool) {
        Self::push(ViewportAction::SetDebugTraceFps(enabled));
    }

    pub fn set_debug_trace_render_time(&self, enabled: bool) {
        Self::push(ViewportAction::SetDebugTraceRenderTime(enabled));
    }

    pub fn set_debug_trace_layout_detail(&self, enabled: bool) {
        Self::push(ViewportAction::SetDebugTraceLayoutDetail(enabled));
    }

    pub fn set_debug_trace_compile_detail(&self, enabled: bool) {
        Self::push(ViewportAction::SetDebugTraceCompileDetail(enabled));
    }

    pub fn set_debug_trace_execute_detail(&self, enabled: bool) {
        Self::push(ViewportAction::SetDebugTraceExecuteDetail(enabled));
    }

    pub fn set_debug_trace_reuse_path(&self, enabled: bool) {
        Self::push(ViewportAction::SetDebugTraceReusePath(enabled));
    }

    pub fn set_debug_geometry_overlay(&self, enabled: bool) {
        Self::push(ViewportAction::SetDebugGeometryOverlay(enabled));
    }

    pub fn set_promotion_enabled(&self, enabled: bool) {
        Self::push(ViewportAction::SetPromotionEnabled(enabled));
    }

    pub fn set_clear_color(&self, color: Color) {
        Self::push(ViewportAction::SetClearColor(color));
    }

    pub fn request_redraw(&self) {
        Self::push(ViewportAction::RequestRedraw);
    }
}

/// Component-side hook returning a [`ViewportHandle`]. Call inside a
/// `#[component]` render function or any callback that needs to mutate
/// the viewport without owning a reference to it.
pub fn use_viewport() -> ViewportHandle {
    ViewportHandle
}

/// Drain every pending `ViewportAction` from the thread-local queue.
/// Intended solely for the viewport render path — normal user code
/// should go through [`use_viewport`].
#[doc(hidden)]
pub fn drain_viewport_actions() -> Vec<ViewportAction> {
    VIEWPORT_ACTIONS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

#[cfg(test)]
mod tests {
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
}
