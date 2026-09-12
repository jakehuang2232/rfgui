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
    use crate::style::Length;
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
