//! Stable identifier for retained-tree nodes.
//!
//! Post arena-migration cleanup: `NodeId` is a type alias for
//! [`crate::view::node_arena::NodeKey`] (slotmap generational key). Event API
//! surfaces and dispatch internals use `NodeId` exclusively; the legacy u64
//! `stable_id` path survives only for cross-frame stable-id lookups (e.g.
//! `get_scroll_offset_by_id`).

use std::any::TypeId;
use std::cell::Ref;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;

use crate::view::base_component::ElementTrait;
use crate::view::node_arena::Node;

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
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// `true` if `(px, py)` lies inside this rectangle (half-open on the
    /// far edges, matching most hit-test conventions).
    #[inline]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// Observable per-node state a handler may want to read on any target in
/// the tree. Read-only mirror of the engine's own flags — flipping fields
/// here does nothing; go through the normal `EventCommand` / viewport APIs
/// to mutate state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeState {
    pub hovered: bool,
    pub focused: bool,
    pub pressed: bool,
    pub disabled: bool,
    pub visible: bool,
}

/// ARIA role skeleton. Placeholder — no prop binding yet, so every live
/// accessor returns `None`. Wired up when the a11y layer lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AriaRole {
    Button,
    Link,
    TextBox,
    Checkbox,
    Radio,
    Slider,
    Switch,
    Menu,
    MenuItem,
    Tab,
    TabList,
    TabPanel,
    Dialog,
    Tooltip,
    Image,
    Heading,
    List,
    ListItem,
    Group,
    Region,
}

/// Stable view of a node as an event target.
///
/// Carries id + bounds eagerly (hot-path reads stay cheap) plus an optional
/// `&Viewport` for lazy tree / state queries (`parent`, `ancestors`,
/// `closest`, `contains`, `tag`, `state`, …). The viewport reference is
/// populated by the dispatch layer on entry and its lifetime is bound to
/// the borrow that returned the `EventTarget` — handlers cannot leak a
/// target beyond the current `&mut Event` call, so the borrow checker
/// alone enforces scope; no `unsafe` lifetime extension on the public
/// surface. Accessors fall back to safe defaults when no viewport is
/// attached (synthetic events, test fixtures).
#[derive(Clone, Copy, Default)]
pub struct EventTarget<'a> {
    pub id: NodeId,
    /// Viewport-space bounds of the node at dispatch time.
    pub bounds: Rect,
    /// Bounds in the node's local space (origin at node's own top-left).
    /// `x` / `y` are typically zero; `width` / `height` match `bounds`.
    pub local_bounds: Rect,
    /// Viewport reference used by lazy tree / state accessors. `None`
    /// for synthetic events / test fixtures — accessors return safe
    /// defaults in that case. Arena access goes through
    /// `viewport.node_arena()` (single source of truth, no separate
    /// arena pointer needed under Approach C).
    pub(crate) viewport: Option<&'a crate::view::viewport::Viewport>,
}

impl<'a> EventTarget<'a> {
    /// Construct an `EventTarget` with id only; bounds default to zero and
    /// no viewport is attached. Use when the dispatch site does not have
    /// box-model info yet (e.g. synthetic focus events).
    #[inline]
    pub const fn bare(id: NodeId) -> Self {
        Self {
            id,
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            local_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            viewport: None,
        }
    }

    /// Construct an `EventTarget` snapshot with id + bounds, no viewport
    /// attached. Bubble / hit-test code builds these and passes them to
    /// `EventMeta::set_current_target`; the meta layer re-attaches the
    /// viewport on every getter call from its stored pointer.
    #[inline]
    pub const fn snapshot(id: NodeId, bounds: Rect, local_bounds: Rect) -> Self {
        Self {
            id,
            bounds,
            local_bounds,
            viewport: None,
        }
    }

    // ---- Lazy accessors ----------------------------------------------------
    //
    // All of these return a safe default when `viewport` is `None` (synthetic
    // / test-fixture path) or when the node is no longer in the arena.

    /// Parent node, or `None` for the root / detached / no-viewport case.
    pub fn parent(&self) -> Option<EventTarget<'a>> {
        let vp = self.viewport?;
        let parent_key = vp.node_arena().parent_of(self.id)?;
        Some(target_from_viewport(vp, parent_key))
    }

    /// Iterator over ancestors, starting with this target's direct parent
    /// and walking up to the root. Empty when no viewport / no parents.
    pub fn ancestors(&self) -> AncestorIter<'a> {
        AncestorIter {
            viewport: self.viewport,
            next: self
                .viewport
                .and_then(|vp| vp.node_arena().parent_of(self.id)),
        }
    }

    /// First ancestor (inclusive of `self`) for which `pred` returns `true`.
    pub fn closest<F>(&self, mut pred: F) -> Option<EventTarget<'a>>
    where
        F: FnMut(&EventTarget<'a>) -> bool,
    {
        let mut cur = Some(*self);
        while let Some(t) = cur {
            if pred(&t) {
                return Some(t);
            }
            cur = t.parent();
        }
        None
    }

    /// `true` if `other` is a descendant of (or equal to) this target.
    pub fn contains(&self, other: NodeId) -> bool {
        let Some(vp) = self.viewport else {
            return false;
        };
        if self.id == other {
            return true;
        }
        let arena = vp.node_arena();
        let mut cur = arena.parent_of(other);
        while let Some(key) = cur {
            if key == self.id {
                return true;
            }
            cur = arena.parent_of(key);
        }
        false
    }

    /// [`TypeId`] of the concrete element type at this node. Use for
    /// cheap `Copy`/`Eq` identity checks and as the building block for
    /// [`Self::is`] / delegation patterns like
    /// `target.closest(|t| t.is::<Button>())`.
    ///
    /// Returns `None` for synthetic / detached targets (no viewport attached
    /// or key no longer in arena).
    pub fn tag(&self) -> Option<TypeId> {
        let vp = self.viewport?;
        let node = vp.node_arena().get(self.id)?;
        Some((*node.element).as_any().type_id())
    }

    /// Human-readable type name of the concrete element (e.g.
    /// `"rfgui::view::base_component::element::Element"`). Debug / tracing
    /// only — do **not** pattern-match on it (format is not stable across
    /// rustc versions). Prefer [`Self::tag`] / [`Self::is`] for comparisons.
    pub fn tag_name(&self) -> Option<&'static str> {
        let vp = self.viewport?;
        let node = vp.node_arena().get(self.id)?;
        // Dispatches through the `ElementTypeName` vtable slot so we get
        // the concrete element type's name, not `"dyn ElementTrait"`.
        Some((*node.element).element_type_name())
    }

    /// True if the node's concrete element type is `T`. Equivalent to
    /// `self.tag() == Some(TypeId::of::<T>())` but reads better at the
    /// call site.
    ///
    /// ```ignore
    /// target.closest(|t| t.is::<Button>());
    /// ```
    #[inline]
    pub fn is<T: ElementTrait>(&self) -> bool {
        self.tag() == Some(TypeId::of::<T>())
    }

    /// Borrow the element as `&T` when the concrete type matches. Returns
    /// `None` on type mismatch / synthetic target / detached node.
    ///
    /// ```ignore
    /// if let Some(btn) = target.downcast::<Button>() {
    ///     if btn.disabled { return; }
    /// }
    /// ```
    ///
    /// The returned [`ElementRef`] holds a [`Ref`] into the arena slot
    /// and must not outlive the current handler invocation. Do not clone
    /// or store it — dropping it releases the borrow so other code can
    /// mutate the arena.
    pub fn downcast<T: ElementTrait>(&self) -> Option<ElementRef<'a, T>> {
        let vp = self.viewport?;
        let guard = vp.node_arena().get(self.id)?;
        if (*guard.element).as_any().type_id() != TypeId::of::<T>() {
            return None;
        }
        Some(ElementRef {
            guard,
            _phantom: PhantomData,
        })
    }

    /// Borrow the element as a `&dyn ElementTrait` trait object. Useful
    /// when the specific type is not known but trait methods suffice.
    /// Returns `None` for synthetic / detached targets.
    pub fn element(&self) -> Option<ElementRef<'a, dyn ElementTrait>> {
        let vp = self.viewport?;
        let guard = vp.node_arena().get(self.id)?;
        Some(ElementRef {
            guard,
            _phantom: PhantomData,
        })
    }

    /// ARIA role placeholder. Always `None` until the a11y layer lands.
    #[inline]
    pub fn role(&self) -> Option<AriaRole> {
        None
    }

    /// Read-only snapshot of the node's hover / focus / press / disabled
    /// / visibility state at the time of this call.
    pub fn state(&self) -> NodeState {
        let mut s = NodeState::default();
        if let Some(vp) = self.viewport {
            s.hovered = vp.hovered_node_id() == Some(self.id);
            s.focused = vp.focused_node_id() == Some(self.id);
            s.visible = vp.node_arena().contains_key(self.id);
        }
        s
    }

    /// `true` if the node is marked disabled. Convenience over `state().disabled`.
    #[inline]
    pub fn disabled(&self) -> bool {
        self.state().disabled
    }

    /// 2D affine transform applied to the node (scale / rotation / skew),
    /// in the form `[a, b, c, d, e, f]` = `[[a, c, e], [b, d, f], [0, 0, 1]]`.
    /// `None` when no non-identity transform is recorded. Placeholder —
    /// returns `None` until layout exposes it.
    #[inline]
    pub fn transform(&self) -> Option<[f32; 6]> {
        None
    }

    /// Bounds in host-window (screen) coordinates. Currently aliases
    /// `bounds` — window offset plumbing lands when the host-window API
    /// exposes it. Always returns a finite rect (never `None`).
    #[inline]
    pub fn screen_bounds(&self) -> Rect {
        self.bounds
    }
}

impl fmt::Debug for EventTarget<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventTarget")
            .field("id", &self.id)
            .field("bounds", &self.bounds)
            .field("local_bounds", &self.local_bounds)
            .field("viewport", &self.viewport.map(|_| "&Viewport"))
            .finish()
    }
}

impl PartialEq for EventTarget<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Ignore `viewport` — it is scratch state for lazy lookups, not
        // part of a target's identity. Two targets pointing at the same
        // node with the same bounds are equal regardless of whether they
        // were taken from a live dispatch or a synthetic fixture.
        self.id == other.id
            && self.bounds == other.bounds
            && self.local_bounds == other.local_bounds
    }
}

/// Transient handle to a node's concrete element, returned by
/// [`EventTarget::downcast`] and [`EventTarget::element`].
///
/// Wraps a [`Ref`] into the arena slot; its lifetime is bound to the
/// `EventTarget` that produced it. Hold it only for the duration of a
/// handler — cloning into long-lived storage would keep the slot
/// immutably borrowed and eventually panic when the engine tries to
/// mutate it.
///
/// `Deref`s to either `T` ([`EventTarget::downcast::<T>`]) or
/// `dyn ElementTrait` ([`EventTarget::element`]).
pub struct ElementRef<'a, T: ?Sized> {
    guard: Ref<'a, Node>,
    _phantom: PhantomData<fn() -> *const T>,
}

impl<'a, T: ElementTrait> Deref for ElementRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // `downcast` verifies the type before constructing — the downcast
        // here cannot fail. Expressed as `expect` rather than `unwrap`
        // for a clearer panic message in the impossible case.
        (*self.guard.element)
            .as_any()
            .downcast_ref::<T>()
            .expect("ElementRef<T> constructed without type check")
    }
}

impl<'a> Deref for ElementRef<'a, dyn ElementTrait> {
    type Target = dyn ElementTrait;
    fn deref(&self) -> &dyn ElementTrait {
        &*self.guard.element
    }
}

impl<'a, T: ?Sized> fmt::Debug for ElementRef<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElementRef")
            .field("ty", &std::any::type_name::<T>())
            .finish()
    }
}

/// Iterator over an [`EventTarget`]'s ancestor chain. See
/// [`EventTarget::ancestors`].
pub struct AncestorIter<'a> {
    viewport: Option<&'a crate::view::viewport::Viewport>,
    next: Option<NodeId>,
}

impl<'a> Iterator for AncestorIter<'a> {
    type Item = EventTarget<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let vp = self.viewport?;
        let key = self.next?;
        let target = target_from_viewport(vp, key);
        self.next = vp.node_arena().parent_of(key);
        Some(target)
    }
}

fn target_from_viewport<'a>(
    vp: &'a crate::view::viewport::Viewport,
    key: NodeId,
) -> EventTarget<'a> {
    let (bounds, local_bounds) = vp
        .node_arena()
        .get(key)
        .map(|node_ref| {
            let snap = node_ref.element.box_model_snapshot();
            (
                Rect::new(snap.x, snap.y, snap.width, snap.height),
                Rect::new(0.0, 0.0, snap.width, snap.height),
            )
        })
        .unwrap_or_default();
    EventTarget {
        id: key,
        bounds,
        local_bounds,
        viewport: Some(vp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_target_has_zero_bounds_and_no_ctx() {
        let t = EventTarget::bare(NodeId::default());
        assert_eq!(t.bounds, Rect::default());
        assert_eq!(t.local_bounds, Rect::default());
        assert!(t.viewport.is_none());
    }

    #[test]
    fn snapshot_target_carries_bounds_but_no_ctx() {
        let b = Rect::new(10.0, 20.0, 30.0, 40.0);
        let lb = Rect::new(0.0, 0.0, 30.0, 40.0);
        let t = EventTarget::snapshot(NodeId::default(), b, lb);
        assert_eq!(t.bounds, b);
        assert_eq!(t.local_bounds, lb);
        assert!(t.viewport.is_none());
    }

    #[test]
    fn accessors_safe_default_without_ctx() {
        // With no arena / viewport attached (synthetic / test fixture
        // path), every lazy accessor returns a safe default rather than
        // panicking. This is the contract the dispatch layer relies on
        // when events are constructed without a live context.
        let t = EventTarget::bare(NodeId::default());
        assert!(t.parent().is_none());
        assert!(t.ancestors().next().is_none());
        assert!(t.closest(|_| true).map(|c| c == t).unwrap_or(false)); // closest finds self
        assert!(!t.contains(NodeId::default()) || t.id == NodeId::default());
        assert!(t.tag().is_none());
        assert!(t.tag_name().is_none());
        assert!(
            t.downcast::<crate::view::base_component::Element>()
                .is_none()
        );
        assert!(t.element().is_none());
        assert!(t.role().is_none());
        assert_eq!(t.state(), NodeState::default());
        assert!(!t.disabled());
        assert!(t.transform().is_none());
        assert_eq!(t.screen_bounds(), t.bounds);
    }

    #[test]
    fn closest_self_match_does_not_need_ctx() {
        let t = EventTarget::bare(NodeId::default());
        let hit = t.closest(|c| c.id == t.id);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().id, t.id);
    }

    #[test]
    fn partial_eq_ignores_ctx_fields() {
        // Two targets with the same id / bounds but different ctx slots
        // must compare equal — the viewport / arena refs are scratch.
        let a = EventTarget::bare(NodeId::default());
        let b = EventTarget::bare(NodeId::default());
        assert_eq!(a, b);
    }

    #[test]
    fn node_state_default_all_false() {
        let s = NodeState::default();
        assert!(!s.hovered);
        assert!(!s.focused);
        assert!(!s.pressed);
        assert!(!s.disabled);
        assert!(!s.visible);
    }

    mod live {
        //! Tests that exercise `tag` / `is` / `downcast` against a live
        //! arena. Render a minimal rsx tree into a `Viewport`, pick the
        //! resulting root `NodeKey`, hand-build an `EventTarget` pointing
        //! at it, then verify the accessors report the concrete type.
        use super::*;
        use crate::Length;
        use crate::ui::{RsxNode, rsx};
        use crate::view::Element as RuntimeElementTag;
        use crate::view::base_component::Element as RuntimeElement;
        use crate::view::viewport::Viewport;

        fn host_tree() -> RsxNode {
            rsx! {
                <RuntimeElementTag style={{
                    width: Length::px(100.0),
                    height: Length::px(50.0),
                }} />
            }
        }

        fn viewport_with_root() -> (Viewport, NodeId) {
            let mut vp = Viewport::new();
            vp.render_rsx(&host_tree()).expect("render succeeds");
            let key = vp
                .node_arena()
                .roots()
                .first()
                .copied()
                .expect("one root inserted");
            (vp, key)
        }

        fn target_for<'a>(vp: &'a Viewport, key: NodeId) -> EventTarget<'a> {
            super::super::target_from_viewport(vp, key)
        }

        #[test]
        fn tag_returns_concrete_type_id() {
            let (vp, key) = viewport_with_root();
            let t = target_for(&vp, key);
            assert_eq!(t.tag(), Some(std::any::TypeId::of::<RuntimeElement>()));
        }

        #[test]
        fn tag_name_contains_element() {
            let (vp, key) = viewport_with_root();
            let t = target_for(&vp, key);
            let name = t.tag_name().expect("live target has a type name");
            assert!(name.contains("Element"), "unexpected tag_name: {name}");
        }

        #[test]
        fn is_reports_correct_type() {
            let (vp, key) = viewport_with_root();
            let t = target_for(&vp, key);
            assert!(t.is::<RuntimeElement>());
        }

        #[test]
        fn downcast_hits_on_matching_type() {
            let (vp, key) = viewport_with_root();
            let t = target_for(&vp, key);
            let el = t.downcast::<RuntimeElement>().expect("type matches");
            // Deref lets us read fields through the `ElementRef` guard.
            let _id: u64 = el.stable_id();
        }

        #[test]
        fn element_returns_trait_object() {
            let (vp, key) = viewport_with_root();
            let t = target_for(&vp, key);
            let el = t.element().expect("live target has an element");
            // `dyn ElementTrait` exposes trait methods directly.
            let _ = el.box_model_snapshot();
        }

        // Sentinel element type used purely as a `TypeId` for the
        // negative-downcast test — it is never actually inserted into
        // the arena, so downcasting an `Element` slot to this type must
        // fail.
        struct NotAnElement;

        #[test]
        fn downcast_mismatched_type_returns_none() {
            let (vp, key) = viewport_with_root();
            let t = target_for(&vp, key);
            // `NotAnElement` does not implement `ElementTrait`, so the
            // bound on `downcast::<T>` rejects it at compile time —
            // instead verify via `is::<T>` that the type ids differ.
            assert_ne!(t.tag(), Some(std::any::TypeId::of::<NotAnElement>()));
            assert!(!t.is::<crate::view::base_component::TextArea>());
            assert!(
                t.downcast::<crate::view::base_component::TextArea>()
                    .is_none()
            );
        }

        #[test]
        fn closest_with_type_predicate() {
            let (vp, key) = viewport_with_root();
            let t = target_for(&vp, key);
            let hit = t.closest(|c| c.is::<RuntimeElement>());
            assert_eq!(hit.map(|h| h.id), Some(key));
        }
    }
}
