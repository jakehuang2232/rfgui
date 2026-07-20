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
        let Some((children, dirty)) = arena.mutate_element_with_invalidation(key, |element, cx| {
            let children = element.children().to_vec();
            let dirty = element.tick_animation_frame(now);
            if !dirty.is_empty() {
                cx.invalidate(dirty);
            }
            (children, dirty)
        }) else {
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
        let Some((children, dirty)) = arena.mutate_element_with_invalidation(key, |element, cx| {
            let children = element.children().to_vec();
            let dirty = element.tick_post_layout_animation_frame(now);
            if !dirty.is_empty() {
                cx.invalidate(dirty);
            }
            (children, dirty)
        }) else {
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
mod tests {
    use super::{
        hit_test, hit_test_roots, tick_animation_frames, tick_post_layout_animation_frames,
    };
    use crate::style::{Anchor, AnchorName, Color, Layout};
    use crate::style::{
        Angle, ClipMode, Length, ParsedValue, Position, PropertyId, Rotate, ScrollDirection, Style,
        Transform, TransformOrigin, Translate,
    };
    use crate::ui::{
        ClickEvent, EventMeta, Modifiers, NodeId, PointerButton, PointerButtons, PointerEventData,
    };
    use crate::view::base_component::{
        BoxModelSnapshot, BuildState, DirtyFlags, Element, ElementTrait, EventTarget,
        LayoutConstraints, LayoutPlacement, Layoutable, PaintResourcePreparationContext,
        Renderable, UiBuildContext,
    };
    use crate::view::frame_graph::FrameGraph;
    use crate::view::node_arena::{NodeArena, NodeKey};
    use crate::view::test_support::{
        commit_child, commit_element, measure_and_place, new_test_arena,
    };
    use crate::view::viewport::dispatch::dispatch_click_from_hit_test;
    use crate::view::{Viewport, ViewportControl};
    use std::cell::Cell;
    use std::rc::Rc;

    struct AnimationTickProbe {
        id: u64,
        children: Vec<NodeKey>,
        ticks: Rc<Cell<u32>>,
        wants_checks: Rc<Cell<u32>>,
        tick_now: Rc<Cell<Option<crate::time::Instant>>>,
        post_tick_now: Rc<Cell<Option<crate::time::Instant>>>,
        resource_now: Rc<Cell<Option<crate::time::Instant>>>,
    }

    impl Layoutable for AnimationTickProbe {
        fn requires_arena_sync(&self) -> bool {
            true
        }
        fn prepare_paint_resources(&mut self, context: PaintResourcePreparationContext) {
            self.resource_now.set(Some(context.now));
        }
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (1.0, 1.0)
        }
        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl EventTarget for AnimationTickProbe {
        fn wants_animation_frame(&self) -> bool {
            self.wants_checks.set(self.wants_checks.get() + 1);
            false
        }
    }

    impl Renderable for AnimationTickProbe {
        fn build(
            &mut self,
            _graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            ctx: UiBuildContext,
        ) -> BuildState {
            ctx.into_state()
        }
    }

    impl ElementTrait for AnimationTickProbe {
        fn stable_id(&self) -> u64 {
            self.id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                border_radius: 0.0,
                should_render: true,
            }
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn tick_animation_frame(&mut self, now: crate::time::Instant) -> DirtyFlags {
            self.ticks.set(self.ticks.get() + 1);
            self.tick_now.set(Some(now));
            DirtyFlags::NONE
        }

        fn tick_post_layout_animation_frame(&mut self, now: crate::time::Instant) -> DirtyFlags {
            self.post_tick_now.set(Some(now));
            DirtyFlags::NONE
        }

        fn children(&self) -> &[NodeKey] {
            &self.children
        }

        fn sync_children_mirror(&mut self, children: &[NodeKey]) {
            self.children.clear();
            self.children.extend_from_slice(children);
        }
    }

    #[test]
    fn animation_tick_visits_duplicate_and_cyclic_topology_once() {
        let root_ticks = Rc::new(Cell::new(0));
        let child_ticks = Rc::new(Cell::new(0));
        let root_wants_checks = Rc::new(Cell::new(0));
        let child_wants_checks = Rc::new(Cell::new(0));
        let root_tick_now = Rc::new(Cell::new(None));
        let root_post_tick_now = Rc::new(Cell::new(None));
        let root_resource_now = Rc::new(Cell::new(None));
        let child_tick_now = Rc::new(Cell::new(None));
        let child_post_tick_now = Rc::new(Cell::new(None));
        let child_resource_now = Rc::new(Cell::new(None));
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(AnimationTickProbe {
                id: 0xa001,
                children: Vec::new(),
                ticks: root_ticks.clone(),
                wants_checks: root_wants_checks.clone(),
                tick_now: root_tick_now.clone(),
                post_tick_now: root_post_tick_now.clone(),
                resource_now: root_resource_now.clone(),
            }),
        );
        let child = commit_child(
            &mut arena,
            root,
            Box::new(AnimationTickProbe {
                id: 0xa002,
                children: Vec::new(),
                ticks: child_ticks.clone(),
                wants_checks: child_wants_checks.clone(),
                tick_now: child_tick_now.clone(),
                post_tick_now: child_post_tick_now.clone(),
                resource_now: child_resource_now.clone(),
            }),
        );
        arena.set_children(root, vec![child, child]);
        arena.set_children(child, vec![root]);

        let semantic_now = crate::time::Instant::now();
        assert!(!tick_animation_frames(
            &mut arena,
            &[root, root],
            semantic_now
        ));
        assert!(!tick_post_layout_animation_frames(
            &mut arena,
            &[root, root],
            semantic_now,
        ));
        arena.prepare_registered_paint_resources(PaintResourcePreparationContext {
            frame_number: 7,
            device_scale: 1.0,
            now: semantic_now,
        });
        assert_eq!(root_ticks.get(), 1);
        assert_eq!(child_ticks.get(), 1);
        assert_eq!(root_tick_now.get(), Some(semantic_now));
        assert_eq!(root_post_tick_now.get(), Some(semantic_now));
        assert_eq!(root_resource_now.get(), Some(semantic_now));
        assert_eq!(child_tick_now.get(), Some(semantic_now));
        assert_eq!(child_post_tick_now.get(), Some(semantic_now));
        assert_eq!(child_resource_now.get(), Some(semantic_now));
        assert!(!super::has_animation_frame_request(&arena, root));
        assert_eq!(root_wants_checks.get(), 1);
        assert_eq!(child_wants_checks.get(), 1);
    }

    fn constraints(w: f32, h: f32) -> LayoutConstraints {
        LayoutConstraints {
            max_width: w,
            max_height: h,
            viewport_width: w,
            percent_base_width: Some(w),
            percent_base_height: Some(h),
            viewport_height: h,
        }
    }

    fn placement(w: f32, h: f32) -> LayoutPlacement {
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: w,
            available_height: h,
            viewport_width: w,
            percent_base_width: Some(w),
            percent_base_height: Some(h),
            viewport_height: h,
        }
    }

    #[test]
    fn hit_test_allows_absolute_viewport_clip_outside_parent() {
        let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
        root.set_background_color_value(Color::rgb(16, 16, 16));
        let parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(400.0, 300.0),
            placement(400.0, 300.0),
        );

        assert_eq!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_key));
    }

    #[test]
    fn hit_test_maps_points_through_translated_parent_transform() {
        let root = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.set_transform(Transform::new([Translate::x(Length::px(100.0))]));
        parent.apply_style(parent_style);

        let mut child = Element::new(10.0, 10.0, 20.0, 20.0);
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(400.0, 300.0),
            placement(400.0, 300.0),
        );

        assert_eq!(hit_test(&arena, root_key, 115.0, 15.0), Some(child_key));
    }

    #[test]
    fn hit_test_maps_points_through_rotated_parent_transform() {
        let root = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        parent_style.set_transform(Transform::new([Rotate::z(Angle::deg(90.0))]));
        parent_style.set_transform_origin(TransformOrigin::center());
        parent.apply_style(parent_style);

        let mut child = Element::new(70.0, 10.0, 20.0, 20.0);
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(400.0, 300.0),
            placement(400.0, 300.0),
        );

        assert_eq!(hit_test(&arena, root_key, 80.0, 80.0), Some(child_key));
    }

    #[test]
    fn hit_test_allows_absolute_viewport_clip_when_parent_not_rendered() {
        let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
        root.set_anchor_name(Some(AnchorName::new("root_anchor")));
        root.set_background_color_value(Color::rgb(16, 16, 16));
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(500.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::Parent),
            ),
        );
        parent.apply_style(parent_style);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .anchor("root_anchor")
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(400.0, 300.0),
            placement(400.0, 300.0),
        );

        assert_eq!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_key));
    }

    #[test]
    fn hit_test_blocks_absolute_parent_clip_outside_parent() {
        let root = Element::new(0.0, 0.0, 400.0, 300.0);
        let parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Parent),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(400.0, 300.0),
            placement(400.0, 300.0),
        );

        assert_ne!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_key));
    }

    #[test]
    fn hit_test_prefers_scrollbar_over_children() {
        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#101010")),
        );
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);
        let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
        child.set_background_color_value(Color::rgb(255, 0, 0));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let _child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(120.0, 120.0),
            placement(120.0, 120.0),
        );
        arena.with_element_taken(root_key, |el, _a| {
            if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
                let _ = e.set_hovered(true);
            }
        });

        assert_eq!(hit_test(&arena, root_key, 115.0, 60.0), Some(root_key));
    }

    #[test]
    fn overflow_child_hit_bubbles_but_parent_is_not_targetable_outside_clip() {
        let mut root = Element::new(0.0, 0.0, 200.0, 160.0);
        root.set_background_color_value(Color::rgb(16, 16, 16));
        let mut clip_parent = Element::new(0.0, 0.0, 100.0, 80.0);
        clip_parent.set_background_color_value(Color::rgb(32, 32, 32));
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let parent_clicks = Rc::new(Cell::new(0));
        let parent_clicks_binding = parent_clicks.clone();
        parent.on_click(move |_event, _control| {
            parent_clicks_binding.set(parent_clicks_binding.get() + 1);
        });
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(50.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::Parent),
            ),
        );
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let child_clicks = Rc::new(Cell::new(0));
        let child_clicks_binding = child_clicks.clone();
        child.on_click(move |_event, _control| {
            child_clicks_binding.set(child_clicks_binding.get() + 1);
        });
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(60.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let clip_parent_key = commit_child(&mut arena, root_key, Box::new(clip_parent));
        let parent_key = commit_child(&mut arena, clip_parent_key, Box::new(parent));
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(200.0, 160.0),
            placement(200.0, 160.0),
        );

        assert_eq!(hit_test(&arena, root_key, 115.0, 15.0), Some(child_key));
        assert_eq!(hit_test(&arena, root_key, 145.0, 15.0), Some(root_key));

        let mut viewport = Viewport::new();
        let mut control = ViewportControl::new(&mut viewport);
        let mut click_child = ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: 115.0,
                viewport_y: 15.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: Modifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
            click_count: 1,
        };
        assert!(dispatch_click_from_hit_test(
            &mut arena,
            root_key,
            &mut click_child,
            &mut control
        ));
        assert_eq!(child_clicks.get(), 1);
        assert_eq!(parent_clicks.get(), 1);

        let mut click_outside = ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: 145.0,
                viewport_y: 15.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(PointerButton::Left),
                buttons: PointerButtons::default(),
                modifiers: Modifiers::default(),
                pointer_id: 0,
                pointer_type: crate::platform::input::PointerType::Mouse,
                pressure: 0.0,
                timestamp: crate::time::Instant::now(),
            },
            click_count: 1,
        };
        let _ =
            dispatch_click_from_hit_test(&mut arena, root_key, &mut click_outside, &mut control);
        assert_eq!(child_clicks.get(), 1);
        assert_eq!(parent_clicks.get(), 1);
    }

    #[test]
    fn hit_test_roots_respects_later_root_over_anchor_parent_overflow_handle() {
        let mut lower_root = Element::new(0.0, 0.0, 100.0, 80.0);
        lower_root.set_background_color_value(Color::rgb(16, 16, 16));
        let mut handle = Element::new(0.0, 0.0, 4.0, 80.0);
        let mut handle_style = Style::new();
        handle_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        handle_style.insert(
            PropertyId::Cursor,
            ParsedValue::Cursor(crate::style::Cursor::EwResize),
        );
        handle_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .right(Length::px(-2.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        handle.apply_style(handle_style);

        let mut higher_root = Element::new(50.0, 0.0, 100.0, 80.0);
        higher_root.set_background_color_value(Color::rgb(32, 32, 32));

        let mut arena = new_test_arena();
        let lower_key = commit_element(&mut arena, Box::new(lower_root));
        let handle_key = commit_child(&mut arena, lower_key, Box::new(handle));
        let higher_key = commit_element(&mut arena, Box::new(higher_root));

        let root_keys = [lower_key, higher_key];
        for &root_key in &root_keys {
            measure_and_place(
                &mut arena,
                root_key,
                constraints(200.0, 160.0),
                placement(200.0, 160.0),
            );
        }

        assert_eq!(hit_test(&arena, lower_key, 101.0, 20.0), Some(handle_key));
        assert_eq!(
            hit_test_roots(&arena, &root_keys, 101.0, 20.0),
            Some((1, higher_key)),
            "root children follow sibling stacking; an earlier root's overflow handle is not a top layer"
        );
    }

    #[test]
    fn hit_test_window_like_anchor_parent_resize_handles_all_edges() {
        let mut root = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().column().into()),
        );
        root_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(20.0))
                    .top(Length::px(30.0)),
            ),
        );
        root.apply_style(root_style);

        let mut content = Element::new(0.0, 0.0, 100.0, 80.0);
        content.set_background_color_value(Color::rgb(32, 32, 32));

        fn resize_handle(position: Position, cursor: crate::style::Cursor) -> Element {
            let mut handle = Element::new(0.0, 0.0, 0.0, 0.0);
            let mut style = Style::new();
            style.insert(PropertyId::Position, ParsedValue::Position(position));
            style.insert(PropertyId::Cursor, ParsedValue::Cursor(cursor));
            match cursor {
                crate::style::Cursor::EwResize => {
                    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(4.0)));
                }
                crate::style::Cursor::NsResize => {
                    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(4.0)));
                }
                _ => {}
            }
            handle.apply_style(style);
            handle
        }

        let left = resize_handle(
            Position::absolute()
                .left(Length::px(-2.0))
                .top(Length::px(0.0))
                .bottom(Length::px(0.0))
                .clip(ClipMode::AnchorParent),
            crate::style::Cursor::EwResize,
        );
        let right = resize_handle(
            Position::absolute()
                .right(Length::px(-2.0))
                .top(Length::px(0.0))
                .bottom(Length::px(0.0))
                .clip(ClipMode::AnchorParent),
            crate::style::Cursor::EwResize,
        );
        let top = resize_handle(
            Position::absolute()
                .left(Length::px(0.0))
                .right(Length::px(0.0))
                .top(Length::px(-2.0))
                .clip(ClipMode::AnchorParent),
            crate::style::Cursor::NsResize,
        );
        let bottom = resize_handle(
            Position::absolute()
                .left(Length::px(0.0))
                .right(Length::px(0.0))
                .bottom(Length::px(-2.0))
                .clip(ClipMode::AnchorParent),
            crate::style::Cursor::NsResize,
        );

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let _content_key = commit_child(&mut arena, root_key, Box::new(content));
        let left_key = commit_child(&mut arena, root_key, Box::new(left));
        let right_key = commit_child(&mut arena, root_key, Box::new(right));
        let top_key = commit_child(&mut arena, root_key, Box::new(top));
        let bottom_key = commit_child(&mut arena, root_key, Box::new(bottom));

        measure_and_place(
            &mut arena,
            root_key,
            constraints(200.0, 160.0),
            placement(200.0, 160.0),
        );

        let left_snapshot = arena
            .get(left_key)
            .expect("left handle")
            .element
            .box_model_snapshot();
        let right_snapshot = arena
            .get(right_key)
            .expect("right handle")
            .element
            .box_model_snapshot();
        let top_snapshot = arena
            .get(top_key)
            .expect("top handle")
            .element
            .box_model_snapshot();
        let bottom_snapshot = arena
            .get(bottom_key)
            .expect("bottom handle")
            .element
            .box_model_snapshot();
        assert_eq!(
            (left_snapshot.width, left_snapshot.height),
            (4.0, 80.0),
            "left edge snapshot should use the placed frame size"
        );
        assert_eq!(
            (right_snapshot.width, right_snapshot.height),
            (4.0, 80.0),
            "right edge snapshot should use the placed frame size"
        );
        assert_eq!(
            (top_snapshot.width, top_snapshot.height),
            (100.0, 4.0),
            "top edge snapshot should use the placed frame size"
        );
        assert_eq!(
            (bottom_snapshot.width, bottom_snapshot.height),
            (100.0, 4.0),
            "bottom edge snapshot should use the placed frame size"
        );

        assert_eq!(hit_test(&arena, root_key, 19.0, 50.0), Some(left_key));
        assert_eq!(hit_test(&arena, root_key, 121.0, 50.0), Some(right_key));
        assert_eq!(hit_test(&arena, root_key, 50.0, 29.0), Some(top_key));
        assert_eq!(hit_test(&arena, root_key, 50.0, 111.0), Some(bottom_key));
    }

    fn absolute_diagnostic_element(position: Position, cursor: crate::style::Cursor) -> Element {
        let mut element = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut style = Style::new();
        style.insert(PropertyId::Position, ParsedValue::Position(position));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        style.insert(PropertyId::Cursor, ParsedValue::Cursor(cursor));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff00ff")),
        );
        element.apply_style(style);
        element
    }

    fn absolute_diagnostic_roots(
        position: Position,
    ) -> (
        crate::view::node_arena::NodeArena,
        [crate::view::node_arena::NodeKey; 2],
        crate::view::node_arena::NodeKey,
    ) {
        let mut lower_root = Element::new(0.0, 0.0, 80.0, 80.0);
        lower_root.set_background_color_value(Color::rgb(16, 16, 16));
        let popup = absolute_diagnostic_element(position, crate::style::Cursor::Crosshair);

        let mut higher_root = Element::new(90.0, 0.0, 80.0, 80.0);
        higher_root.set_background_color_value(Color::rgb(32, 32, 32));

        let mut arena = new_test_arena();
        let lower_key = commit_element(&mut arena, Box::new(lower_root));
        let popup_key = commit_child(&mut arena, lower_key, Box::new(popup));
        let higher_key = commit_element(&mut arena, Box::new(higher_root));
        let root_keys = [lower_key, higher_key];
        for &root_key in &root_keys {
            measure_and_place(
                &mut arena,
                root_key,
                constraints(220.0, 120.0),
                placement(220.0, 120.0),
            );
        }
        (arena, root_keys, popup_key)
    }

    #[test]
    fn diagnostic_absolute_viewport_clip_without_anchor_respects_later_root_body() {
        let (arena, root_keys, popup_key) = absolute_diagnostic_roots(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Viewport),
        );

        assert_eq!(hit_test(&arena, root_keys[0], 105.0, 15.0), Some(popup_key));
        assert_eq!(
            hit_test_roots(&arena, &root_keys, 105.0, 15.0),
            Some((1, root_keys[1])),
            "clip:Viewport escapes the parent gate but does not cross root sibling stacking"
        );
    }

    #[test]
    fn diagnostic_absolute_anchor_parent_without_anchor_respects_later_root_body() {
        let (arena, root_keys, popup_key) = absolute_diagnostic_roots(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(10.0))
                .clip(ClipMode::AnchorParent),
        );

        assert_eq!(hit_test(&arena, root_keys[0], 105.0, 15.0), Some(popup_key));
        assert_eq!(
            hit_test_roots(&arena, &root_keys, 105.0, 15.0),
            Some((1, root_keys[1])),
            "clip:AnchorParent escapes the parent gate but does not cross root sibling stacking"
        );
    }

    #[test]
    fn diagnostic_absolute_parent_clip_overflow_does_not_escape_parent_hit_region() {
        let (arena, root_keys, popup_key) = absolute_diagnostic_roots(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Parent),
        );

        assert_eq!(
            hit_test(&arena, root_keys[0], 75.0, 15.0),
            Some(root_keys[0])
        );
        assert_eq!(hit_test(&arena, root_keys[0], 105.0, 15.0), None);
        assert_ne!(
            hit_test_roots(&arena, &root_keys, 105.0, 15.0),
            Some((0, popup_key)),
            "ClipMode::Parent absolute overflow is clipped by design"
        );
    }

    #[test]
    fn diagnostic_root_level_absolute_viewport_clip_loses_to_later_root_body() {
        let mut lower_root = absolute_diagnostic_element(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Viewport),
            crate::style::Cursor::Crosshair,
        );
        lower_root.set_anchor_name(Some(AnchorName::new("diagnostic_root_popup")));
        let mut higher_root = Element::new(90.0, 0.0, 80.0, 80.0);
        higher_root.set_background_color_value(Color::rgb(32, 32, 32));

        let mut arena = new_test_arena();
        let popup_root_key = commit_element(&mut arena, Box::new(lower_root));
        let higher_key = commit_element(&mut arena, Box::new(higher_root));
        let root_keys = [popup_root_key, higher_key];
        for &root_key in &root_keys {
            measure_and_place(
                &mut arena,
                root_key,
                constraints(220.0, 120.0),
                placement(220.0, 120.0),
            );
        }

        assert_eq!(
            hit_test(&arena, popup_root_key, 105.0, 15.0),
            Some(popup_root_key)
        );
        assert_eq!(
            hit_test_roots(&arena, &root_keys, 105.0, 15.0),
            Some((1, higher_key)),
            "root-level absolute roots follow root stacking; a later root body wins"
        );
    }

    #[test]
    fn diagnostic_transformed_absolute_viewport_clip_respects_later_root_body() {
        let mut lower_root = Element::new(0.0, 0.0, 80.0, 80.0);
        lower_root.set_background_color_value(Color::rgb(16, 16, 16));
        let mut popup = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut popup_style = Style::new();
        popup_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(0.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        popup_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        popup_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        popup_style.insert(
            PropertyId::Cursor,
            ParsedValue::Cursor(crate::style::Cursor::Crosshair),
        );
        popup_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff00ff")),
        );
        popup_style.set_transform(Transform::new([Translate::x(Length::px(100.0))]));
        popup.apply_style(popup_style);

        let mut higher_root = Element::new(90.0, 0.0, 80.0, 80.0);
        higher_root.set_background_color_value(Color::rgb(32, 32, 32));

        let mut arena = new_test_arena();
        let lower_key = commit_element(&mut arena, Box::new(lower_root));
        let popup_key = commit_child(&mut arena, lower_key, Box::new(popup));
        let higher_key = commit_element(&mut arena, Box::new(higher_root));
        let root_keys = [lower_key, higher_key];
        for &root_key in &root_keys {
            measure_and_place(
                &mut arena,
                root_key,
                constraints(220.0, 120.0),
                placement(220.0, 120.0),
            );
        }
        assert_eq!(hit_test(&arena, lower_key, 105.0, 15.0), Some(popup_key));
        assert_eq!(
            hit_test_roots(&arena, &root_keys, 105.0, 15.0),
            Some((1, higher_key)),
            "transformed escape absolute remains owned by its root stacking context"
        );
    }

    #[test]
    fn diagnostic_named_anchor_anchor_parent_respects_later_root_body() {
        let mut lower_root = Element::new(0.0, 0.0, 80.0, 80.0);
        lower_root.set_background_color_value(Color::rgb(16, 16, 16));
        lower_root.set_anchor_name(Some(AnchorName::new("diagnostic_anchor")));
        let popup = absolute_diagnostic_element(
            Position::absolute()
                .anchor(Anchor::Name(AnchorName::new("diagnostic_anchor")))
                .right(Length::px(-20.0))
                .top(Length::px(10.0))
                .clip(ClipMode::AnchorParent),
            crate::style::Cursor::Crosshair,
        );

        let mut higher_root = Element::new(90.0, 0.0, 80.0, 80.0);
        higher_root.set_background_color_value(Color::rgb(32, 32, 32));

        let mut arena = new_test_arena();
        let lower_key = commit_element(&mut arena, Box::new(lower_root));
        let popup_key = commit_child(&mut arena, lower_key, Box::new(popup));
        let higher_key = commit_element(&mut arena, Box::new(higher_root));
        let root_keys = [lower_key, higher_key];
        for &root_key in &root_keys {
            measure_and_place(
                &mut arena,
                root_key,
                constraints(220.0, 120.0),
                placement(220.0, 120.0),
            );
        }

        assert_eq!(hit_test(&arena, lower_key, 95.0, 15.0), Some(popup_key));
        assert_eq!(
            hit_test_roots(&arena, &root_keys, 95.0, 15.0),
            Some((1, higher_key)),
            "named AnchorParent escape remains owned by its root stacking context"
        );
    }
}
