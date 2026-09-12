//! Low-level retained host elements and traversal helpers used to build custom elements.

use std::sync::atomic::{AtomicU64, Ordering};

use rustc_hash::FxHashSet;

mod core;
mod element;
mod hit_test;
mod image;
mod resource_slot;
mod style_consumer;
mod svg;
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) use svg::prepare_svg_fixture_for_test;
mod text;
pub(crate) mod text_area;

pub(crate) use core::*;
pub use element::*;
pub(crate) use hit_test::hit_test_pointer_target;
pub use hit_test::{hit_test, hit_test_roots, hit_test_stacked};
pub use image::*;
pub(crate) use style_consumer::ComputedStyleConsumer;
pub use svg::*;
pub use text::*;
pub use text_area::{TextArea, TextAreaImeContext, TextAreaRenderProjection, TextAreaRenderString};

fn next_ui_node_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn round_layout_value(value: f32) -> f32 {
    if value.is_finite() {
        value.round()
    } else {
        value
    }
}

/// Paint offset inherited by an owner's children after the owner applies the
/// engine's layout-position snap. Both live traversal and arena-independent
/// artifact placement use this derivation so they cannot disagree on host
/// placement at fractional coordinates.
pub(crate) fn paint_offset_after_owner_snap(
    owner_viewport_position: [f32; 2],
    parent_paint_offset: [f32; 2],
) -> Option<[f32; 2]> {
    if owner_viewport_position
        .into_iter()
        .chain(parent_paint_offset)
        .any(|value| !value.is_finite())
    {
        return None;
    }
    let paint = [
        owner_viewport_position[0] + parent_paint_offset[0],
        owner_viewport_position[1] + parent_paint_offset[1],
    ];
    let next = [
        parent_paint_offset[0] + round_layout_value(paint[0]) - paint[0],
        parent_paint_offset[1] + round_layout_value(paint[1]) - paint[1],
    ];
    next.into_iter().all(f32::is_finite).then_some(next)
}

pub(crate) fn build_node_by_id(
    node: &mut dyn ElementTrait,
    node_id: u64,
    graph: &mut crate::view::frame_graph::FrameGraph,
    arena: &mut crate::view::node_arena::NodeArena,
    ctx: &mut UiBuildContext,
) -> bool {
    if node.stable_id() == node_id {
        let next_state = node.build(
            graph,
            arena,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
        );
        ctx.set_state(next_state);
        return true;
    }
    // Recurse into arena-resident children. The current `node` is already
    // out of the arena (taken by our caller via `with_element_taken`), so
    // we clone the child-key list and reborrow the arena per child.
    let child_keys: Vec<crate::view::node_arena::NodeKey> = node
        .as_any()
        .downcast_ref::<Element>()
        .map(|el| el.children().to_vec())
        .unwrap_or_default();
    for child_key in child_keys {
        let found = arena
            .with_element_taken(child_key, |child, arena| {
                build_node_by_id(child.as_mut(), node_id, graph, arena, ctx)
            })
            .unwrap_or(false);
        if found {
            return true;
        }
    }
    false
}

pub(crate) fn build_node_by_key(
    node_key: crate::view::node_arena::NodeKey,
    stable_id: u64,
    graph: &mut crate::view::frame_graph::FrameGraph,
    arena: &mut crate::view::node_arena::NodeArena,
    ctx: &mut UiBuildContext,
) -> bool {
    arena
        .with_element_taken(node_key, |node, arena| {
            build_node_by_id(node.as_mut(), stable_id, graph, arena, ctx)
        })
        .unwrap_or(false)
}

pub fn get_ime_cursor_rect_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    stable_id: u64,
) -> Option<(f32, f32, f32, f32)> {
    let node = arena.get(root_key)?;
    if node.element.stable_id() == stable_id {
        return node.element.ime_cursor_rect();
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children {
        if let Some(rect) = get_ime_cursor_rect_by_id(arena, child_key, stable_id) {
            return Some(rect);
        }
    }
    None
}

pub fn get_cursor_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    stable_id: u64,
) -> Option<crate::style::Cursor> {
    let node = arena.get(root_key)?;
    if node.element.stable_id() == stable_id {
        return Some(node.element.cursor());
    }
    let children: Vec<_> = node.children.clone();
    drop(node);
    for child_key in children {
        if let Some(cursor) = get_cursor_by_id(arena, child_key, stable_id) {
            return Some(cursor);
        }
    }
    None
}

pub(crate) fn select_all_text_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
) -> bool {
    arena
        .mutate_element_ref_with_invalidation(root_key, |element, cx| {
            if element.stable_id() == node_id {
                if let Some(text_area) = element.as_any_mut().downcast_mut::<TextArea>() {
                    text_area.select_all();
                    cx.invalidate(element.local_dirty_flags());
                    return true;
                }
                return false;
            }
            let children: Vec<_> = element.children().to_vec();
            for child_key in children {
                if select_all_text_by_id(cx.arena(), child_key, node_id) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

pub(crate) fn select_text_range_by_id(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    node_id: u64,
    start: usize,
    end: usize,
) -> bool {
    arena
        .mutate_element_ref_with_invalidation(root_key, |element, cx| {
            if element.stable_id() == node_id {
                if let Some(text_area) = element.as_any_mut().downcast_mut::<TextArea>() {
                    text_area.select_range(start, end);
                    cx.invalidate(element.local_dirty_flags());
                    return true;
                }
                return false;
            }
            let children: Vec<_> = element.children().to_vec();
            for child_key in children {
                if select_text_range_by_id(cx.arena(), child_key, node_id, start, end) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

/// True when `descendant_key` lies in the subtree rooted at `ancestor_key`
/// (walks via `arena.parent_of`). `root_key` is retained for API compatibility
/// and used only to bound the search (ancestor must be reachable from it).
pub fn subtree_contains_node(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    ancestor_key: crate::view::node_arena::NodeKey,
    descendant_key: crate::view::node_arena::NodeKey,
) -> bool {
    if !arena.contains_key(ancestor_key) || !arena.contains_key(descendant_key) {
        return false;
    }
    // Walk up from descendant_key, checking for ancestor_key along the way.
    // Stop if we exit the root_key's subtree.
    let mut cur = Some(descendant_key);
    let mut reached_root = false;
    while let Some(k) = cur {
        if k == ancestor_key {
            return true;
        }
        if k == root_key {
            reached_root = true;
        }
        cur = arena.parent_of(k);
    }
    let _ = reached_root;
    false
}

pub fn has_animation_frame_request(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
) -> bool {
    fn visit(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        seen: &mut FxHashSet<crate::view::node_arena::NodeKey>,
    ) -> bool {
        if !seen.insert(key) {
            return false;
        }
        let Some(node) = arena.get(key) else {
            return false;
        };
        if node.element.wants_animation_frame() {
            return true;
        }
        let children = node.children.clone();
        drop(node);
        for child in children {
            if visit(arena, child, seen) {
                return true;
            }
        }
        false
    }

    visit(arena, root_key, &mut FxHashSet::default())
}

// Exact native defaults have no side effects. Avoid acquiring mutable access
// for those no-op calls: it would invalidate every retained input each frame.
// Unknown/custom implementations always receive both original hooks.
fn native_noop_tick_children(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    post_layout: bool,
) -> Option<Vec<crate::view::node_arena::NodeKey>> {
    let node = arena.get(key)?;
    let host = node.element.as_any();
    let noop = host.is::<Text>() || host.downcast_ref::<Element>()
        .is_some_and(|element| !post_layout || element.post_layout_animation_is_noop());
    noop.then(|| node.element.children().to_vec())
}

/// Advance retained animation state using one viewport-owned time sample.
///
/// The generic hook keeps the viewport independent of concrete components.
/// Element-owned and arena-owned dirty state are updated together through the
/// scoped invalidation path.
pub(crate) fn tick_animation_frames(
    arena: &mut crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    now: crate::time::Instant,
) -> bool {
    fn visit(
        arena: &mut crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        now: crate::time::Instant,
        seen: &mut FxHashSet<crate::view::node_arena::NodeKey>,
    ) -> bool {
        if !seen.insert(key) {
            return false;
        }
        let Some((children, dirty)) = native_noop_tick_children(arena, key, false)
            .map(|children| (children, DirtyFlags::NONE))
            .or_else(|| arena.mutate_element_with_invalidation(key, |element, cx| {
            let children = element.children().to_vec();
            let dirty = element.tick_animation_frame(now);
            if !dirty.is_empty() {
                cx.invalidate(dirty);
            }
            (children, dirty)
        })) else {
            return false;
        };
        let mut changed = !dirty.is_empty();
        for child in children {
            changed |= visit(arena, child, now, seen);
        }
        changed
    }

    let mut seen = FxHashSet::default();
    roots.iter().copied().fold(false, |changed, root| {
        visit(arena, root, now, &mut seen) || changed
    })
}

/// Resolve retained visual state that depends on final layout using the same
/// viewport-owned semantic time sample as the pre-layout animation tick.
pub(crate) fn tick_post_layout_animation_frames(
    arena: &mut crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    now: crate::time::Instant,
) -> bool {
    fn visit(
        arena: &mut crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        now: crate::time::Instant,
        seen: &mut FxHashSet<crate::view::node_arena::NodeKey>,
    ) -> bool {
        if !seen.insert(key) {
            return false;
        }
        let Some((children, dirty)) = native_noop_tick_children(arena, key, true)
            .map(|children| (children, DirtyFlags::NONE))
            .or_else(|| arena.mutate_element_with_invalidation(key, |element, cx| {
            let children = element.children().to_vec();
            let dirty = element.tick_post_layout_animation_frame(now);
            if !dirty.is_empty() {
                cx.invalidate(dirty);
            }
            (children, dirty)
        })) else {
            return false;
        };
        let mut changed = !dirty.is_empty();
        for child in children {
            changed |= visit(arena, child, now, seen);
        }
        changed
    }

    let mut seen = FxHashSet::default();
    roots.iter().copied().fold(false, |changed, root| {
        visit(arena, root, now, &mut seen) || changed
    })
}

/// Forward `EventTarget` methods to an inner field (typically `element`).
///
/// One form: `forward_event_target!(full element)` — forwards every method,
/// including `cursor()`. Used by Image / Svg.
///
/// Earlier `dispatch_only` / `dispatch_pair` arms supported `Text`'s wrapping
/// `Element`; both went away with the M6 NOT-IS-A refactor (Text dropped its
/// inner `Element` and now impls `EventTarget` directly with trait defaults
/// for the dispatch methods).
macro_rules! forward_event_target {
    (full $field:ident) => {
        $crate::view::base_component::forward_event_target!(@dispatch $field);
        $crate::view::base_component::forward_event_target!(@state_and_requests $field);
    };
    (@dispatch $field:ident) => {
        fn dispatch_pointer_down(
            &mut self,
            event: &mut $crate::ui::PointerDownEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_pointer_down(event, control, arena, self_key);
        }
        fn dispatch_pointer_up(
            &mut self,
            event: &mut $crate::ui::PointerUpEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_pointer_up(event, control, arena, self_key);
        }
        fn dispatch_pointer_move(
            &mut self,
            event: &mut $crate::ui::PointerMoveEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_pointer_move(event, control, arena, self_key);
        }
        fn dispatch_click(
            &mut self,
            event: &mut $crate::ui::ClickEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_click(event, control, arena, self_key);
        }
        fn dispatch_context_menu(
            &mut self,
            event: &mut $crate::ui::ContextMenuEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_context_menu(event, control, arena, self_key);
        }
        fn dispatch_wheel(
            &mut self,
            event: &mut $crate::ui::WheelEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_wheel(event, control, arena, self_key);
        }
        fn dispatch_key_down(
            &mut self,
            event: &mut $crate::ui::KeyDownEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_key_down(event, control, arena, self_key);
        }
        fn dispatch_key_up(
            &mut self,
            event: &mut $crate::ui::KeyUpEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_key_up(event, control, arena, self_key);
        }
        fn dispatch_focus(
            &mut self,
            event: &mut $crate::ui::FocusEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_focus(event, control, arena, self_key);
        }
        fn dispatch_blur(
            &mut self,
            event: &mut $crate::ui::BlurEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_blur(event, control, arena, self_key);
        }
        fn dispatch_ime_commit(
            &mut self,
            event: &mut $crate::ui::ImeCommitEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_ime_commit(event, control, arena, self_key);
        }
        fn dispatch_ime_enabled(
            &mut self,
            event: &mut $crate::ui::ImeEnabledEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_ime_enabled(event, control, arena, self_key);
        }
        fn dispatch_ime_disabled(
            &mut self,
            event: &mut $crate::ui::ImeDisabledEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_ime_disabled(event, control, arena, self_key);
        }
        fn dispatch_drag_start(
            &mut self,
            event: &mut $crate::ui::DragStartEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_drag_start(event, control, arena, self_key);
        }
        fn dispatch_drag_over(
            &mut self,
            event: &mut $crate::ui::DragOverEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_drag_over(event, control, arena, self_key);
        }
        fn dispatch_drag_leave(
            &mut self,
            event: &mut $crate::ui::DragLeaveEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_drag_leave(event, control, arena, self_key);
        }
        fn dispatch_drop(
            &mut self,
            event: &mut $crate::ui::DropEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_drop(event, control, arena, self_key);
        }
        fn dispatch_drag_end(
            &mut self,
            event: &mut $crate::ui::DragEndEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_drag_end(event, control, arena, self_key);
        }
        fn dispatch_copy(
            &mut self,
            event: &mut $crate::ui::CopyEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_copy(event, control, arena, self_key);
        }
        fn dispatch_cut(
            &mut self,
            event: &mut $crate::ui::CutEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_cut(event, control, arena, self_key);
        }
        fn dispatch_paste(
            &mut self,
            event: &mut $crate::ui::PasteEvent,
            control: &mut $crate::view::viewport::ViewportControl<'_>,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_paste(event, control, arena, self_key);
        }
    };
    (@state_and_requests $field:ident) => {
        fn dispatch_pointer_enter(
            &mut self,
            event: &mut $crate::ui::PointerEnterEvent,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_pointer_enter(event, arena, self_key);
        }
        fn dispatch_pointer_leave(
            &mut self,
            event: &mut $crate::ui::PointerLeaveEvent,
            arena: &$crate::view::node_arena::NodeArena,
            self_key: $crate::view::node_arena::NodeKey,
        ) {
            self.$field.dispatch_pointer_leave(event, arena, self_key);
        }
        fn cancel_pointer_interaction(&mut self) -> bool {
            self.$field.cancel_pointer_interaction()
        }
        fn set_hovered(&mut self, hovered: bool) -> bool {
            self.$field.set_hovered(hovered)
        }
        fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
            self.$field.scroll_by(dx, dy)
        }
        fn can_scroll_by(&self, dx: f32, dy: f32) -> bool {
            self.$field.can_scroll_by(dx, dy)
        }
        fn get_scroll_offset(&self) -> (f32, f32) {
            self.$field.get_scroll_offset()
        }
        fn set_scroll_offset(&mut self, offset: (f32, f32)) {
            self.$field.set_scroll_offset(offset);
        }
        fn cursor(&self) -> $crate::style::Cursor {
            self.$field.cursor()
        }
        fn wants_animation_frame(&self) -> bool {
            self.$field.wants_animation_frame()
        }
        fn take_style_transition_requests(
            &mut self,
        ) -> Vec<$crate::transition::StyleTrackRequest> {
            self.$field.take_style_transition_requests()
        }
        fn take_layout_transition_requests(
            &mut self,
        ) -> Vec<$crate::transition::LayoutTrackRequest> {
            self.$field.take_layout_transition_requests()
        }
        fn take_visual_transition_requests(
            &mut self,
        ) -> Vec<$crate::transition::VisualTrackRequest> {
            self.$field.take_visual_transition_requests()
        }
    };
}

pub(crate) use forward_event_target;

#[cfg(test)]
mod tests;
