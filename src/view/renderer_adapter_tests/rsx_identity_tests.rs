use super::*;

#[test]
fn element_debug_type_defaults_empty_and_flows_through_prop_paths() {
    let mut arena = crate::view::test_support::new_test_arena();
    let default_root = commit_rsx_tree(&mut arena, &host_element_node())[0];
    assert!(
        crate::view::test_support::get_element::<BaseElement>(&arena, default_root)
            .debug_type()
            .is_empty()
    );

    // No concrete public flags exist yet. An unnamed retained bit exercises
    // the bitflag transport without prematurely defining the flag surface.
    let marker = DebugType::from_bits_retain(1);
    let marked_tree = rsx! { <HostElement debug_type={marker} /> };
    let marked_root = commit_rsx_tree(&mut arena, &marked_tree)[0];
    assert_eq!(
        crate::view::test_support::get_element::<BaseElement>(&arena, marked_root).debug_type(),
        marker
    );

    let viewport_style = crate::style::Style::new();
    let ctx = crate::view::fiber_work::ApplyContext {
        viewport_style: &viewport_style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let next = DebugType::from_bits_retain(2);
    arena.with_element_taken(marked_root, |element, arena_ref| {
        assert_eq!(
            element.apply_prop(
                arena_ref,
                marked_root,
                &ctx,
                "debug_type",
                next.into_prop_value(),
            ),
            crate::view::fiber_work::PropApplyOutcome::Applied
        );
    });
    assert_eq!(
        crate::view::test_support::get_element::<BaseElement>(&arena, marked_root).debug_type(),
        next
    );

    arena.with_element_taken(marked_root, |element, arena_ref| {
        assert_eq!(
            element.reset_prop(arena_ref, marked_root, &ctx, "debug_type"),
            crate::view::fiber_work::PropApplyOutcome::Applied
        );
    });
    assert!(
        crate::view::test_support::get_element::<BaseElement>(&arena, marked_root)
            .debug_type()
            .is_empty()
    );
}

#[test]
fn rsx_raw_text_uses_html_like_punctuation_and_child_boundary_spacing() {
    let tree = rsx! {
        <HostElement>
            vertical-align: A - B <HostText>middle</HostText> tail
        </HostElement>
    };
    let RsxNode::Element(root) = tree else {
        panic!("expected host element");
    };
    assert_eq!(root.children.len(), 3);
    assert!(matches!(
        &root.children[0],
        RsxNode::Text(text) if text.content == "vertical-align: A - B "
    ));
    assert!(matches!(&root.children[1], RsxNode::Element(_)));
    assert!(matches!(
        &root.children[2],
        RsxNode::Text(text) if text.content == " tail"
    ));
}

#[test]
fn rsx_raw_text_preserves_space_around_expression_boundaries() {
    let who = "world";
    let tree = rsx! { <HostElement>Hello {who}!</HostElement> };
    let RsxNode::Element(root) = tree else {
        panic!("expected host element");
    };
    assert_eq!(root.children.len(), 3);
    assert!(matches!(
        &root.children[0],
        RsxNode::Text(text) if text.content == "Hello "
    ));
    assert!(matches!(
        &root.children[1],
        RsxNode::Text(text) if text.content == "world"
    ));
    assert!(matches!(
        &root.children[2],
        RsxNode::Text(text) if text.content == "!"
    ));
}

#[test]
fn identity_token_uses_type_and_local_key_stably() {
    let identity_a = RsxNodeIdentity::new(
        "Button",
        Some(RsxKey::Local(crate::ui::component_key_token(&"item-a"))),
    );
    let identity_b = RsxNodeIdentity::new(
        "Button",
        Some(RsxKey::Local(crate::ui::component_key_token(&"item-a"))),
    );
    let token_a = identity_token_from_node_identity(&identity_a, 0);
    let token_b = identity_token_from_node_identity(&identity_b, 0);
    assert_eq!(token_a, token_b);
}

#[test]
fn identity_token_distinguishes_local_and_global_key() {
    let local = RsxNodeIdentity::new(
        "Button",
        Some(RsxKey::Local(crate::ui::component_key_token(&"item-a"))),
    );
    let global = RsxNodeIdentity::new("Button", Some(RsxKey::Global(GlobalKey::from("item-a"))));
    assert_ne!(
        identity_token_from_node_identity(&local, 0),
        identity_token_from_node_identity(&global, 0)
    );
}

#[test]
fn rendered_node_id_prefers_tag_descriptor_type_name() {
    struct DescriptorA;
    struct DescriptorB;

    let path = [1_u64, 2_u64];
    let first = RsxNode::tagged("Element", crate::ui::RsxTagDescriptor::of::<DescriptorA>());
    let second = RsxNode::tagged("Element", crate::ui::RsxTagDescriptor::of::<DescriptorB>());

    assert_ne!(
        rendered_node_id(&first, &path, None),
        rendered_node_id(&second, &path, None)
    );
}

#[test]
fn custom_host_builder_reaches_arena_without_adapter_changes() {
    use crate::view::base_component::{ElementTrait, Text};
    use crate::view::renderer_adapter::ElementDescriptor;
    use crate::view::{BuildCtx, HostBuilder, host_builder_node};

    struct MyHost;

    impl HostBuilder for MyHost {
        fn build_descriptor(
            _node: &crate::ui::RsxElementNode,
            _path: &[u64],
            _ctx: &BuildCtx,
        ) -> Result<ElementDescriptor, String> {
            // Distinctive stable id so the assertion catches dispatch.
            Ok(ElementDescriptor::leaf(Box::new(
                Text::from_content_with_id(0xDEAD_BEEF, "custom-host-marker"),
            )))
        }
    }

    let tree = host_builder_node::<MyHost>("MyHost");
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");

    let node = arena.get(root).expect("root committed");
    let text = node
        .element
        .as_any()
        .downcast_ref::<Text>()
        .expect("custom host produced a Text leaf via HostBuilder");
    assert_eq!(text.stable_id(), 0xDEAD_BEEF);
}
