//! Stable identifier for retained-tree nodes.
//!
//! Post arena-migration cleanup: `NodeId` is a type alias for
//! [`crate::view::node_arena::NodeKey`] (slotmap generational key). Event API
//! surfaces and dispatch internals use `NodeId` exclusively; the legacy u64
//! `stable_id` path survives only for cross-frame stable-id lookups (e.g.
//! `get_scroll_offset_by_id`).

/// Opaque id assigned by the viewport to each element in the retained tree.
///
/// Alias to [`crate::view::node_arena::NodeKey`]. `Copy + Hash + Eq`; the null
/// key (`NodeKey::default()`) marks "no target" / sentinel slots.
pub type NodeId = crate::view::node_arena::NodeKey;

/// Axis-aligned rectangle in viewport-space (logical pixels).
///
/// Dedicated event-layer geometry type so the retained-UI event API does
/// not leak the renderer's `BoxModelSnapshot` into handler code. Pure
/// data — methods go on [`EventTarget`] instead.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    #[inline]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// `true` if `(px, py)` lies inside this rectangle (half-open on the
    /// far edges, matching most hit-test conventions).
    #[inline]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// Stable view of a node as an event target.
///
/// Phase 1 of the event-target refactor: carries id + bounds only. Later
/// phases attach a viewport reference so handlers can walk parents, read
/// component kind / role / state, and look up `data::<T>()` payloads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EventTarget {
    pub id: NodeId,
    /// Viewport-space bounds of the node at dispatch time.
    pub bounds: Rect,
    /// Bounds in the node's local space (origin at node's own top-left).
    /// `x` / `y` are typically zero; `width` / `height` match `bounds`.
    pub local_bounds: Rect,
}

impl EventTarget {
    /// Construct an `EventTarget` with id only; bounds default to zero.
    /// Use when the dispatch site does not have box-model info yet
    /// (e.g. synthetic focus events).
    #[inline]
    pub const fn bare(id: NodeId) -> Self {
        Self {
            id,
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }
}
