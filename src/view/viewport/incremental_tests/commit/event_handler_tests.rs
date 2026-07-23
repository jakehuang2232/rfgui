use super::*;

/// M4 #4: event-handler changes are now committable incrementally via
/// `Element::clear_rsx_event_handler` + the shared
/// `try_assign_event_handler_prop` dispatcher. The NodeKey must
/// survive the handler swap (was previously force-rebuilt under M3).
#[test]
fn incremental_commit_applies_event_handler_change_preserves_node_key() {
    use crate::ui::PointerDownHandlerProp;
    let handler_a = PointerDownHandlerProp::new(|_| {});
    let handler_b = PointerDownHandlerProp::new(|_| {});

    let first = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler_a}
        />
    };
    let second = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler_b}
        />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("event-handler change must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "on_pointer_down replacement must preserve NodeKey via the M4 setter path",
    );

    use crate::view::base_component::Element as ElementHost;
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert_eq!(
        el.rsx_event_handler_count("on_pointer_down"),
        1,
        "replace semantics: clear + assign must leave exactly one handler, not stack",
    );
}

/// Removing an `on_*` prop between renders emits a reconciler
/// `removed: [..]` entry. M4 #4 routes that through
/// `Element::clear_rsx_event_handler`, so the handler Vec drops to
/// zero and NodeKey still survives.
#[test]
fn incremental_commit_removes_event_handler_prop_clears_handler_list() {
    use crate::ui::PointerDownHandlerProp;
    use crate::view::base_component::Element as ElementHost;

    let handler = PointerDownHandlerProp::new(|_| {});
    let with_handler = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler}
        />
    };
    let without_handler = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0) }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_handler).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).unwrap();
        let el = node.element.as_any().downcast_ref::<ElementHost>().unwrap();
        assert_eq!(el.rsx_event_handler_count("on_pointer_down"), 1);
    }

    viewport
        .render_rsx(&without_handler)
        .expect("handler-removal render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "removed on_pointer_down must commit through clear and keep NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).unwrap();
    let el = node.element.as_any().downcast_ref::<ElementHost>().unwrap();
    assert_eq!(
        el.rsx_event_handler_count("on_pointer_down"),
        0,
        "removed handler prop must clear the handler list",
    );
}
