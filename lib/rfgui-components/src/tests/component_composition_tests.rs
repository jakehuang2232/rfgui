use super::*;

#[test]
fn material_symbol_icon_renders_as_typed_element_with_symbol_font() {
    let tree = rsx! { <CloseIcon /> };

    let RsxNode::Element(root) = tree else {
        panic!("icon should render element root");
    };
    assert_eq!(root.tag, "Element");

    let style = shared_element_style(&root).expect("missing icon root style");
    assert_eq!(
        style.font.as_ref().expect("missing icon font").as_slice(),
        &[String::from("Material Symbols Outlined")]
    );
    assert_eq!(
        style.font_size, None,
        "icon should inherit font_size from parent"
    );

    let text_child = root.children.first().expect("missing text child");
    let RsxNode::Element(text_node) = text_child else {
        panic!("icon child should be text element");
    };
    assert!(is_host_tag::<Text>(text_node));
    assert_eq!(text_node.children.len(), 1);
    match &text_node.children[0] {
        RsxNode::Text(content) => assert_eq!(content.content, "close"),
        other => panic!("expected ligature text child, got {other:?}"),
    }
}

#[test]
fn button_label_preserves_whitespace() {
    let tree = rsx! {
        <Button variant={Some(ButtonVariant::Contained)}>
            "Click Me"
        </Button>
    };
    let text = find_first_text(&tree).expect("button should carry text child");
    assert_eq!(text, "Click Me");
}

#[test]
fn window_supports_children_with_optional_size_props() {
    let tree = rsx! {
        <Window
            title="Panel"
            width=420.0
        >
            <Button>"Inside"</Button>
        </Window>
    };
    let RsxNode::Element(root) = tree else {
        panic!("window should render element root");
    };
    assert_eq!(root.tag_descriptor, Some(RsxTagDescriptor::of::<Window>()));
    assert!(!root.children.is_empty());
}

#[test]
fn create_element_supports_multiple_children() {
    let tree = rsx! {
        <Element>
            <Text>"A"</Text>
            <Text>"B"</Text>
        </Element>
    };
    let RsxNode::Element(root) = tree else {
        panic!("create_element should produce an element root");
    };
    assert_eq!(root.tag_descriptor, Some(RsxTagDescriptor::of::<Element>()));
    assert_eq!(root.children.len(), 2);
}

// Provider-as-node lazy boundary: `<Provider<T>>` wraps a lazy child
// component. Under the P2 walker pipeline, the child's render body
// runs when the walker descends INTO the Provider node, so walker
// ancestry already has the value pushed — no snapshotting needed.
// React parity P4: when a conditional branch is NOT selected, the
// component in that branch must not have its render body invoked.
// Pre-P2 every `<Heavy/>` eagerly ran its render inside the rsx!
// expression, so both branches always executed. Post-P2 the
// unselected branch never reaches the walker, so its render body
// never fires. This test locks in the cost-skip behaviour.
#[test]
fn conditional_branch_does_not_render_unselected_component() {
    use rfgui::ui::{component, rsx};
    use std::cell::Cell;

    thread_local! {
        static HEAVY_RENDER_CALLS: Cell<u32> = const { Cell::new(0) };
    }

    #[component]
    fn Heavy() -> RsxNode {
        HEAVY_RENDER_CALLS.with(|c| c.set(c.get() + 1));
        rsx! { <rfgui::view::Text>"heavy"</rfgui::view::Text> }
    }

    #[component]
    fn ConditionalParent(show_heavy: bool) -> RsxNode {
        rsx! {
            <rfgui::view::Element>
                <rfgui::view::Text>"always here"</rfgui::view::Text>
                {if show_heavy {
                    rsx! { <Heavy /> }
                } else {
                    RsxNode::fragment(vec![])
                }}
            </rfgui::view::Element>
        }
    }

    // Baseline
    HEAVY_RENDER_CALLS.with(|c| c.set(0));

    // Branch not taken — Heavy::render must not fire.
    let _ = rsx! { <ConditionalParent show_heavy={false} /> };
    let calls_when_false = HEAVY_RENDER_CALLS.with(|c| c.get());
    assert_eq!(
        calls_when_false, 0,
        "Heavy::render ran even though `show_heavy=false` branch skipped"
    );

    // Branch taken — Heavy::render must fire exactly once.
    let _ = rsx! { <ConditionalParent show_heavy={true} /> };
    let calls_when_true = HEAVY_RENDER_CALLS.with(|c| c.get());
    assert_eq!(
        calls_when_true, 1,
        "Heavy::render should run exactly once when branch selected; got {calls_when_true}"
    );
}

#[test]
fn context_crosses_lazy_component_boundary() {
    use rfgui::ui::{Provider, component, use_context};

    #[derive(Clone)]
    struct CtxValue(&'static str);

    #[component]
    fn CtxConsumer() -> RsxNode {
        let seen = use_context::<CtxValue>().map(|v| v.0).unwrap_or("<none>");
        rsx! { <rfgui::view::Text>{seen}</rfgui::view::Text> }
    }

    #[component]
    fn CtxProvider() -> RsxNode {
        rsx! {
            <Provider::<CtxValue> value={CtxValue("from-provider")}>
                <CtxConsumer />
            </Provider>
        }
    }

    let tree = rsx! { <CtxProvider /> };
    let mut texts = Vec::new();
    collect_text_nodes(&tree, &mut texts);
    assert!(
        texts.iter().any(|t| t == "from-provider"),
        "consumer should read value installed by ancestor Provider node across lazy boundary; got {texts:?}"
    );
}

#[test]
fn window_supports_nested_optional_object_props() {
    let tree = rsx! {
        <Window
            title="Panel"
            window_slots={{
                root_style: {
                    background: rfgui::style::Color::hex("#ffffff"),
                },
                title_bar_style: {
                    height: rfgui::style::Length::px(28.0),
                },
            }}
        >
            <Button>"Inside"</Button>
        </Window>
    };

    let RsxNode::Element(root) = tree else {
        panic!("window should render element root");
    };
    assert_eq!(root.tag_descriptor, Some(RsxTagDescriptor::of::<Window>()));
    assert!(!root.children.is_empty());
}
