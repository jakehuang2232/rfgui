//! Stable identifier for retained-tree nodes.
//!
//! Transitional state during the Approach-C arena migration: `NodeId` is
//! still a `u64` wrapper used by the event API (Step 1 of the event
//! refactor), while [`crate::view::node_arena::NodeKey`] is the real
//! slotmap-generational arena key. The two get merged in the cleanup
//! phase once every dispatch path has been ported onto the arena.

use std::fmt;

/// Opaque id assigned by the viewport to each element in the retained
/// tree.
///
/// Transitional u64 wrapper — the target form is an alias for
/// [`crate::view::node_arena::NodeKey`]. Kept separate for now so the
/// event API can continue to compile while element-layer code is
/// progressively ported onto the arena.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct NodeId(pub u64);

impl NodeId {
    /// Raw wire value. Prefer `NodeId` in signatures; use this only when
    /// interoperating with legacy `u64`-typed internals.
    #[inline]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", self.0)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

impl From<u64> for NodeId {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<NodeId> for u64 {
    #[inline]
    fn from(value: NodeId) -> Self {
        value.0
    }
}

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
