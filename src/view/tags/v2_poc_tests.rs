use super::*;
#[allow(unused_imports)]
use crate::ui::RsxTag;
use crate::ui::{__rsx_default_inner_option, create_element};

// Test A: explicit type annotations — baseline.
#[test]
fn v2_element_build_explicit() {
    let node = create_element::<Element>(
        {
            let mut init: ElementPropSchema = Default::default();
            init.style = Some({
                let mut s: ElementStylePropSchema = Default::default();
                s.background_color = Some(Box::new(crate::style::Color::hex("#000000")));
                s
            });
            init
        },
        Vec::new(),
        None,
    );
    match node {
        RsxNode::Element(_) => {}
        _ => panic!("expected element node"),
    }
}

// Test B: phantom-guided inference, no type name appears.
#[test]
fn v2_element_build_inferred() {
    let node = create_element::<Element>(
        {
            let mut init: ElementPropSchema = Default::default();
            init.style = Some({
                let mut s = __rsx_default_inner_option(&init.style);
                s.background_color = Some(Box::new(crate::style::Color::hex("#000000")));
                s
            });
            init
        },
        Vec::new(),
        None,
    );
    match node {
        RsxNode::Element(_) => {}
        _ => panic!("expected element node"),
    }
}

// Test C: nested hover — phantom fallback all the way.
#[test]
fn v2_element_build_inferred_nested_hover() {
    let node = create_element::<Element>(
        {
            let mut init: ElementPropSchema = Default::default();
            init.style = Some({
                let mut s = __rsx_default_inner_option(&init.style);
                s.background_color = Some(Box::new(crate::style::Color::hex("#111111")));
                s.hover = Some({
                    let mut h = __rsx_default_inner_option(&s.hover);
                    h.background_color = Some(Box::new(crate::style::Color::hex("#222222")));
                    h
                });
                s
            });
            init
        },
        Vec::new(),
        None,
    );
    match node {
        RsxNode::Element(_) => {}
        _ => panic!("expected element node"),
    }
}

// Verified manually: writing `s.not_a_real_field = ...` above produces
// `E0609: no field 'not_a_real_field' on type ElementStylePropSchema`.
// Compile-time field checks survive the phantom-fallback inference.

// ---------- rsx! macro end-to-end tests ----------
use crate::style::Length;
use crate::ui::rsx;

#[test]
fn rsx_simple_element() {
    let node = rsx! { <Element /> };
    match node {
        RsxNode::Element(_) => {}
        _ => panic!("expected element"),
    }
}

#[test]
fn rsx_text_area_skeleton_builds() {
    let node = rsx! { <TextArea /> };
    match node {
        RsxNode::Element(ref el) => assert_eq!(el.tag, "TextArea"),
        _ => panic!("expected element"),
    }
}

#[test]
fn rsx_text_area_projection_segment_builds() {
    let node = rsx! {
        <TextAreaProjectionSegment char_range_start=0 char_range_end=5 />
    };
    match node {
        RsxNode::Element(ref el) => {
            assert_eq!(el.tag, "TextAreaProjectionSegment");
            let has_start = el.props.iter().any(|(k, _)| *k == "char_range_start");
            let has_end = el.props.iter().any(|(k, _)| *k == "char_range_end");
            assert!(has_start && has_end);
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn rsx_element_with_style_object() {
    let node = rsx! {
        <Element style={{
            width: Length::px(100.0),
            background_color: crate::style::Color::hex("#111111"),
        }}>
            <Element />
            <Element />
        </Element>
    };
    match node {
        RsxNode::Element(ref el) => {
            assert_eq!(el.children.len(), 2);
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn rsx_nested_hover_object() {
    let node = rsx! {
        <Element style={{
            background_color: crate::style::Color::hex("#111111"),
            hover: {
                background_color: crate::style::Color::hex("#222222"),
            },
        }} />
    };
    match node {
        RsxNode::Element(_) => {}
        _ => panic!("expected element"),
    }
}

// ---------- #[component] + rsx end-to-end ----------

#[crate::ui::component]
pub fn V2PanelLabel(text: String, color: Option<crate::style::Color>) -> RsxNode {
    // Render path doesn't matter for this test; just return an empty element.
    // Intentionally use old `rsx!` inside component body to confirm
    // v1 body still compiles within a v2-tagged component.
    let _ = text;
    let _ = color;
    crate::ui::rsx! { <Element /> }
}

#[crate::ui::component]
pub fn V2ContainerOnly(children: Vec<RsxNode>) -> RsxNode {
    crate::ui::rsx! { <Element>{children}</Element> }
}

#[test]
fn rsx_user_component_with_required_prop() {
    let node = rsx! {
        <V2PanelLabel text={"hello".to_string()} />
    };
    match node {
        RsxNode::Element(_) => {}
        _ => panic!("expected element"),
    }
}

#[test]
fn rsx_user_component_optional_prop() {
    let node = rsx! {
        <V2PanelLabel
            text={"greet".to_string()}
            color={crate::style::Color::hex("#aabbcc")} />
    };
    match node {
        RsxNode::Element(_) => {}
        _ => panic!("expected element"),
    }
}

#[test]
#[should_panic(expected = "missing required prop `text`")]
fn rsx_user_component_missing_required_panics() {
    // Bypass the rsx macro; directly construct init struct and feed to
    // create_element to hit the From impl panic path.
    let init: <V2PanelLabel as crate::ui::RsxTag>::Props = Default::default();
    let _ = crate::ui::create_element::<V2PanelLabel>(init, Vec::new(), None);
}

#[test]
fn rsx_user_component_with_children() {
    let node = rsx! {
        <V2ContainerOnly>
            <Element />
            <Element />
            <Element />
        </V2ContainerOnly>
    };
    // V2ContainerOnly's body `rsx! { <Element>{children}</Element> }` flattens
    // the Vec<RsxNode> into the outer Element's children via IntoRsxChildren,
    // so the returned element has 3 children.
    match node {
        RsxNode::Element(ref el) => assert_eq!(el.children.len(), 3),
        _ => panic!(),
    }
}

#[test]
fn rsx_text_with_literal() {
    let node = rsx! {
        <Element>
            <Text>{"hello"}</Text>
        </Element>
    };
    match node {
        RsxNode::Element(ref el) => assert_eq!(el.children.len(), 1),
        _ => panic!(),
    }
}

// Test D: 4000 siblings via new path — share single monomorphization.
#[test]
fn v2_element_4000_siblings() {
    let mut children = Vec::with_capacity(4000);
    for _ in 0..4000 {
        children.push(create_element::<Element>(
            Default::default(),
            Vec::new(),
            None,
        ));
    }
    let root = create_element::<Element>(Default::default(), children, None);
    match root {
        RsxNode::Element(ref el) => assert_eq!(el.children.len(), 4000),
        _ => panic!(),
    }
}
