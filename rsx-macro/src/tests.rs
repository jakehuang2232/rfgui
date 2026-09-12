use super::{Child, MultipleNodes, ObjectValueExpr, PropValueExpr, expand_component, expand_node};
use quote::ToTokens;

#[test]
fn close_tag_may_omit_generics_on_open() {
    // React-parity: `<Provider::<T>>…</Provider>` must parse. Close tag
    // path key compares idents only; PhantomData fallback uses open
    // tag when close has no args.
    syn::parse_str::<MultipleNodes>(r#"<Provider::<Ctx> value={v}><Child/></Provider>"#)
        .expect("bare close tag should match generic open tag");
}

#[test]
fn close_tag_ident_mismatch_still_rejected() {
    // Stripping generics must not loosen ident comparison.
    let result =
        syn::parse_str::<MultipleNodes>(r#"<Provider::<Ctx> value={v}><Child/></Consumer>"#);
    assert!(result.is_err(), "close ident mismatch should still fail");
}

#[test]
fn recovers_incomplete_prop_before_self_closing_tag_end() {
    let parsed = syn::parse_str::<MultipleNodes>(
        r#"<NumberField binding={bindings.number} min=0.0 max=100.0 s />"#,
    )
    .expect("rsx should recover incomplete prop");

    let node = match &parsed.nodes[0] {
        super::Child::Element(node) => node,
        _ => panic!("expected element node"),
    };
    let prop = node.props.last().expect("missing recovered prop");
    assert_eq!(prop.key.to_string(), "s");
    assert!(matches!(prop.value, PropValueExpr::Missing));
}

#[test]
fn recovers_incomplete_prop_before_next_prop() {
    let parsed = syn::parse_str::<MultipleNodes>(r#"<Element s other="x" />"#)
        .expect("rsx should recover incomplete prop");

    let node = match &parsed.nodes[0] {
        super::Child::Element(node) => node,
        _ => panic!("expected element node"),
    };
    assert_eq!(node.props.len(), 2);
    assert_eq!(node.props[0].key.to_string(), "s");
    assert!(matches!(node.props[0].value, PropValueExpr::Missing));
    assert_eq!(node.props[1].key.to_string(), "other");
}

#[test]
fn parses_prop_macro_invocation() {
    let parsed =
        syn::parse_str::<MultipleNodes>(r#"<Element style!{ width: Length::px(10.0) } />"#)
            .expect("rsx should parse prop macro");

    let node = match &parsed.nodes[0] {
        super::Child::Element(node) => node,
        _ => panic!("expected element node"),
    };
    let prop = node.props.first().expect("missing macro prop");
    assert_eq!(prop.key.to_string(), "style");
    let PropValueExpr::Macro(tokens) = &prop.value else {
        panic!("expected macro prop value");
    };
    assert_eq!(
        tokens.to_string(),
        "style ! { width : Length :: px (10.0) }"
    );
}

#[test]
fn recovers_incomplete_style_key_before_style_object_end() {
    let parsed = syn::parse_str::<MultipleNodes>(
        r##"<Element style={{ background: Color::hex("#000"), backg }} />"##,
    )
    .expect("rsx should recover incomplete style key");

    let node = match &parsed.nodes[0] {
        super::Child::Element(node) => node,
        _ => panic!("expected element node"),
    };
    let style_prop = node
        .props
        .iter()
        .find(|prop| prop.key == "style")
        .expect("missing style prop");
    let PropValueExpr::Object(entries) = &style_prop.value else {
        panic!("expected object");
    };
    let entry = entries.last().expect("missing recovered style entry");
    assert_eq!(entry.key.to_string(), "backg");
    assert!(matches!(entry.value, ObjectValueExpr::Missing));
}

#[test]
fn recovers_incomplete_style_key_before_comma() {
    let parsed = syn::parse_str::<MultipleNodes>(
        r##"<Element style={{ backg, color: Color::hex("#fff") }} />"##,
    )
    .expect("rsx should recover incomplete style key");

    let node = match &parsed.nodes[0] {
        super::Child::Element(node) => node,
        _ => panic!("expected element node"),
    };
    let style_prop = node
        .props
        .iter()
        .find(|prop| prop.key == "style")
        .expect("missing style prop");
    let PropValueExpr::Object(entries) = &style_prop.value else {
        panic!("expected object");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key.to_string(), "backg");
    assert!(matches!(entries[0].value, ObjectValueExpr::Missing));
    assert_eq!(entries[1].key.to_string(), "color");
}

#[test]
fn recovers_missing_tag_end_and_keeps_following_sibling() {
    let parsed = syn::parse_str::<MultipleNodes>(r#"<Element foo={bar}</Element><Label />"#)
        .expect("rsx should recover missing `>` before closing tag");

    assert_eq!(parsed.nodes.len(), 2);

    let first = match &parsed.nodes[0] {
        super::Child::Element(node) => node,
        _ => panic!("expected first node to be element"),
    };
    let expanded = expand_node(&parsed.nodes[0]).to_string();
    assert_eq!(first.props.len(), 1);
    assert!(expanded.contains("expected `>` to finish the start tag"));

    match &parsed.nodes[1] {
        super::Child::Element(node) => {
            assert_eq!(node.tag.to_token_stream().to_string(), "Label");
        }
        _ => panic!("expected second node to be element"),
    }
}

#[test]
fn keeps_parsing_after_invalid_prop_expression() {
    let parsed = syn::parse_str::<MultipleNodes>(
        r#"<Element on_pointer_down={resize_bottom_down />}</Element><Label />"#,
    )
    .expect("rsx should recover invalid prop expression");

    assert_eq!(parsed.nodes.len(), 2);

    let first = match &parsed.nodes[0] {
        super::Child::Element(node) => node,
        _ => panic!("expected first node to be element"),
    };
    assert!(matches!(first.props[0].value, PropValueExpr::Invalid));

    let expanded = expand_node(&parsed.nodes[0]).to_string();
    assert!(expanded.contains("invalid Rust expression for prop `on_pointer_down` inside `{...}`"));
    assert!(!expanded.contains("invalid prop value"));

    match &parsed.nodes[1] {
        super::Child::Element(node) => {
            assert_eq!(node.tag.to_token_stream().to_string(), "Label");
        }
        _ => panic!("expected second node to be element"),
    }
}

#[test]
fn style_object_prop_expands_via_default_inner_option() {
    let parsed = syn::parse_str::<MultipleNodes>(
        r##"<Text style={{ color: Color::hex("#fff") }}>{"A"}</Text>"##,
    )
    .expect("rsx should parse text style");

    let expanded = expand_node(&parsed.nodes[0]).to_string();
    assert!(expanded.contains("Text"));
    assert!(expanded.contains("__rsx_default_inner_option"));
}

#[test]
fn nested_object_prop_expands_via_default_inner_option() {
    let parsed = syn::parse_str::<MultipleNodes>(
        r##"<Window window_slots={{ root_style: { background: Color::hex("#fff") } }} />"##,
    )
    .expect("rsx should parse nested object prop");

    let expanded = expand_node(&parsed.nodes[0]).to_string();
    // Both outer `window_slots` and inner `root_style` expand through
    // the shared `__rsx_default_inner_option` helper; the helper is
    // called once per nesting level.
    assert_eq!(expanded.matches("__rsx_default_inner_option").count(), 2);
}

#[test]
fn rsx_expansion_uses_create_element_path() {
    let parsed = syn::parse_str::<MultipleNodes>(r#"<Element />"#).expect("rsx should parse");

    let expanded = expand_node(&parsed.nodes[0]).to_string();
    assert!(expanded.contains("__rsx_create_element"));
}

#[test]
fn raw_text_preserves_punctuation_adjacency_and_explicit_spaces() {
    let parsed =
        syn::parse_str::<MultipleNodes>(r#"<Text>vertical-align: A - B /users/path</Text>"#)
            .expect("raw text should parse");
    let Child::Element(text) = &parsed.nodes[0] else {
        panic!("expected Text element");
    };
    let [Child::TextRaw(content)] = text.children.as_slice() else {
        panic!("expected one raw text child");
    };
    assert_eq!(content, "vertical-align: A - B /users/path");
}

#[test]
fn raw_text_collapses_html_like_whitespace() {
    let parsed = syn::parse_str::<MultipleNodes>("<Text>Hello     world\n        from RSX</Text>")
        .expect("raw text should parse");
    let Child::Element(text) = &parsed.nodes[0] else {
        panic!("expected Text element");
    };
    let [Child::TextRaw(content)] = text.children.as_slice() else {
        panic!("expected one raw text child");
    };
    assert_eq!(content, "Hello world from RSX");
}

#[test]
fn raw_text_preserves_collapsed_space_at_element_boundaries() {
    let parsed =
        syn::parse_str::<MultipleNodes>(r#"<Element>Hello <Strong>world</Strong>!</Element>"#)
            .expect("mixed children should parse");
    let Child::Element(root) = &parsed.nodes[0] else {
        panic!("expected root element");
    };
    assert_eq!(root.children.len(), 3);
    assert!(matches!(&root.children[0], Child::TextRaw(text) if text == "Hello "));
    assert!(matches!(&root.children[1], Child::Element(_)));
    assert!(matches!(&root.children[2], Child::TextRaw(text) if text == "!"));
}

#[test]
fn whitespace_between_elements_becomes_one_text_node() {
    let parsed = syn::parse_str::<MultipleNodes>(r#"<Element><A />   <B /></Element>"#)
        .expect("element siblings should parse");
    let Child::Element(root) = &parsed.nodes[0] else {
        panic!("expected root element");
    };
    assert_eq!(root.children.len(), 3);
    assert!(matches!(&root.children[0], Child::Element(_)));
    assert!(matches!(&root.children[1], Child::TextRaw(text) if text == " "));
    assert!(matches!(&root.children[2], Child::Element(_)));
}

#[test]
fn formatting_newlines_between_elements_do_not_create_text_nodes() {
    let parsed = syn::parse_str::<MultipleNodes>("<Element>\n    <A />\n    <B />\n</Element>")
        .expect("formatted element siblings should parse");
    let Child::Element(root) = &parsed.nodes[0] else {
        panic!("expected root element");
    };
    assert_eq!(root.children.len(), 2);
    assert!(
        root.children
            .iter()
            .all(|child| matches!(child, Child::Element(_)))
    );
}

#[test]
fn rejects_duplicate_props_including_key() {
    let err = syn::parse_str::<MultipleNodes>(r#"<Element key=1 key=2 />"#)
        .err()
        .expect("duplicate props must be rejected");
    assert!(err.to_string().contains("duplicate prop `key`"));
}

#[test]
fn component_preserves_supported_attributes_and_parameter_patterns() {
    let component = syn::parse_str(
        r#"
            #[doc = "A mutable component"]
            #[deprecated(note = "use another component")]
            fn Mutable(mut value: i32) -> RsxNode { value += 1; RsxNode::text(value.to_string()) }
        "#,
    )
    .expect("component should parse");

    let expanded = expand_component(component).to_string();
    assert!(expanded.contains("deprecated"));
    assert!(expanded.contains("doc = \"A mutable component\""));
    assert!(expanded.contains("mut value : i32"));
}

#[test]
fn component_rejects_unsupported_function_qualifiers() {
    let component = syn::parse_str("async fn AsyncComponent() -> RsxNode { todo!() }")
        .expect("component should parse");

    let expanded = expand_component(component).to_string();
    assert!(expanded.contains("requires a synchronous, safe Rust function"));
}

#[test]
fn child_policy_expansion_is_named_and_preallocates_static_children() {
    let parsed =
        syn::parse_str::<MultipleNodes>(r#"<Image><Element /></Image>"#).expect("rsx should parse");

    let expanded = expand_node(&parsed.nodes[0]).to_string();
    assert!(expanded.contains("does not accept children"));
    assert!(expanded.contains("with_capacity (1usize)"));
}
