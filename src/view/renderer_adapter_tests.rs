//! Tests for `renderer_adapter.rs` — extracted to keep the
//! production module readable. Sibling test module so it can
//! reach `pub(crate)` helpers via `crate::view::renderer_adapter::*`.

#![cfg(test)]

use crate::style::style_props::{StylePropError, TextStyleSet, validate_style};
use crate::style::{
    Border, BorderRadius, Color, ColorLike, Cursor, FontFamily, FontSize, FontWeight, IntoColor,
    Layout, Length, Padding, ParsedValue, PropertyId, Style, TextWrap, Unit, VerticalAlign,
};
use crate::ui::{
    GlobalKey, IntoPropValue, RsxKey, RsxNode, RsxNodeIdentity, RsxTagDescriptor,
    identity_token_from_node_identity, rendered_node_id, rsx,
};
use crate::view::base_component::text_area::TextAreaTextRun;
use crate::view::base_component::{
    Element as BaseElement, Text, TextArea, get_cursor_by_id, hit_test,
};
use crate::view::renderer_adapter::{
    StyleCascadeContext, as_element_style, as_text_style, computed_parent_from_style_cascade,
};
use crate::view::test_support::{commit_rsx_tree, measure_and_place};
use crate::view::{
    Element as HostElement, ElementStylePropSchema, Text as HostText, TextArea as HostTextArea,
    TextStylePropSchema,
};

fn host_element_node() -> RsxNode {
    RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<HostElement>())
}

fn host_text_node() -> RsxNode {
    RsxNode::tagged("Text", RsxTagDescriptor::for_tag::<HostText>())
}

fn host_text_area_node() -> RsxNode {
    RsxNode::tagged("TextArea", RsxTagDescriptor::for_tag::<HostTextArea>())
}

fn empty_element_style() -> ElementStylePropSchema {
    ElementStylePropSchema::default()
}

fn empty_text_style() -> TextStylePropSchema {
    TextStylePropSchema::default()
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

fn style_bg_border(bg_hex: &str, border_hex: &str, border_width: f32) -> ElementStylePropSchema {
    ElementStylePropSchema {
        background: Some(crate::style::Background::Color(Box::new(
            IntoColor::<Color>::into_color(Color::hex(bg_hex)),
        ))),
        border: Some(Border::uniform(
            Length::px(border_width),
            &Color::hex(border_hex),
        )),
        ..empty_element_style()
    }
}

fn style_with_radius(style: ElementStylePropSchema, radius: f32) -> ElementStylePropSchema {
    ElementStylePropSchema {
        border_radius: Some(BorderRadius::uniform(Unit::px(radius))),
        ..style
    }
}

fn style_with_size(
    style: ElementStylePropSchema,
    width: f32,
    height: f32,
) -> ElementStylePropSchema {
    ElementStylePropSchema {
        width: Some(Length::px(width)),
        height: Some(Length::px(height)),
        ..style
    }
}

#[test]
fn computed_parent_from_style_cascade_maps_text_cascade_fields() {
    let mut style = Style::new();
    style.insert(
        PropertyId::FontFamily,
        ParsedValue::FontFamily(FontFamily::new(["Inter", "Arial"])),
    );
    style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(18.0)),
    );
    style.insert(
        PropertyId::FontWeight,
        ParsedValue::FontWeight(FontWeight::new(650)),
    );
    style.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x12, 0x34, 0x56).into()),
    );
    style.insert(PropertyId::Cursor, ParsedValue::Cursor(Cursor::Pointer));
    style.insert(
        PropertyId::TextWrap,
        ParsedValue::TextWrap(TextWrap::NoWrap),
    );
    style.insert(
        PropertyId::LineHeight,
        ParsedValue::LineHeight(crate::style::LineHeight::new(1.6)),
    );
    style.insert(
        PropertyId::VerticalAlign,
        ParsedValue::VerticalAlign(VerticalAlign::Middle),
    );

    let cascade = StyleCascadeContext::from_viewport_style(&style, 0.0, 0.0);
    let parent = computed_parent_from_style_cascade(&cascade);

    assert_eq!(
        parent.font_families,
        vec!["Inter".to_string(), "Arial".to_string()]
    );
    assert!((parent.font_size - 18.0).abs() < f32::EPSILON);
    assert_eq!(parent.font_weight, 650);
    assert_eq!(parent.color.to_rgba_u8(), [0x12, 0x34, 0x56, 0xff]);
    assert_eq!(parent.cursor, Cursor::Pointer);
    assert_eq!(parent.text_wrap, TextWrap::NoWrap);
    assert!((parent.line_height - 1.6).abs() < f32::EPSILON);
    assert_eq!(parent.vertical_align, VerticalAlign::Middle);
}

#[test]
fn computed_parent_from_style_cascade_falls_back_to_root_font_size() {
    let mut style = Style::new();
    style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(20.0)),
    );

    let cascade = StyleCascadeContext::from_viewport_style(&style, 0.0, 0.0);
    let parent = computed_parent_from_style_cascade(&cascade);

    assert!((parent.font_size - 20.0).abs() < f32::EPSILON);
}

fn text_style_with_color(color_hex: &str) -> TextStylePropSchema {
    TextStylePropSchema {
        color: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(
            color_hex,
        )))),
        ..empty_text_style()
    }
}

fn text_style_with_size(width: f32, height: f32) -> TextStylePropSchema {
    TextStylePropSchema {
        width: Some(Length::px(width)),
        height: Some(Length::px(height)),
        ..empty_text_style()
    }
}

#[test]
fn as_text_style_accepts_text_style_props() {
    let value = TextStylePropSchema {
        color: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(
            "#123456",
        )))),
        font_size: Some(FontSize::px(18.0)),
        cursor: Some(Cursor::Text),
        ..empty_text_style()
    }
    .into_prop_value();

    let style = as_text_style(&value, "style").expect("text style should validate");

    assert!(matches!(
        style.get(PropertyId::Color),
        Some(ParsedValue::Color(_))
    ));
    assert!(matches!(
        style.get(PropertyId::FontSize),
        Some(ParsedValue::FontSize(_))
    ));
    assert_eq!(
        style.get(PropertyId::Cursor),
        Some(&ParsedValue::Cursor(Cursor::Text))
    );
}

#[test]
fn text_style_set_rejects_unsupported_normalized_fields() {
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#123456")),
    );

    assert_eq!(
        validate_style::<TextStyleSet>(&style),
        Err(StylePropError::UnsupportedProperty {
            property: PropertyId::BackgroundColor,
        })
    );
}

#[test]
fn as_element_style_accepts_box_style_props() {
    let value = ElementStylePropSchema {
        layout: Some(Layout::Inline),
        background_color: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(
            "#123456",
        )))),
        ..empty_element_style()
    }
    .into_prop_value();

    let style = as_element_style(&value, "style").expect("element style should validate");

    assert_eq!(
        style.get(PropertyId::Layout),
        Some(&ParsedValue::Layout(Layout::Inline))
    );
    assert!(matches!(
        style.get(PropertyId::BackgroundColor),
        Some(ParsedValue::Color(_))
    ));
}

#[test]
fn rsx_fragmentable_badge_text_aligns_with_sibling_inline_text() {
    for vertical_align in [
        VerticalAlign::Baseline,
        VerticalAlign::Top,
        VerticalAlign::Middle,
        VerticalAlign::Bottom,
    ] {
        let tree = rsx! {
            <HostElement style={ElementStylePropSchema {
                layout: Some(Layout::Inline),
                width: Some(Length::px(960.0)),
                gap: Some(Length::px(8.0)),
                line_height: Some(1.2),
                vertical_align: Some(vertical_align),
                ..empty_element_style()
            }}>
                Inline text starts here,
                <HostElement style={ElementStylePropSchema {
                    padding: Some(Padding::uniform(Length::px(8.0))),
                    ..empty_element_style()
                }}>
                    badge test test test test test test test
                </HostElement>
                <HostText>then more text continues after the badge,</HostText>
                <HostElement style={ElementStylePropSchema {
                    width: Some(Length::px(90.0)),
                    height: Some(Length::px(50.0)),
                    padding: Some(Padding::uniform(Length::px(8.0))),
                    ..empty_element_style()
                }}>
                    <HostText>note note note note note note note</HostText>
                </HostElement>
            </HostElement>
        };
        let mut arena = crate::view::node_arena::NodeArena::new();
        let roots = commit_rsx_tree(&mut arena, &tree);
        let root = roots[0];
        measure_and_place(
            &mut arena,
            root,
            crate::view::base_component::LayoutConstraints {
                max_width: 960.0,
                max_height: 240.0,
                viewport_width: 960.0,
                viewport_height: 240.0,
                percent_base_width: Some(960.0),
                percent_base_height: Some(240.0),
            },
            crate::view::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 960.0,
                available_height: 240.0,
                viewport_width: 960.0,
                viewport_height: 240.0,
                percent_base_width: Some(960.0),
                percent_base_height: Some(240.0),
            },
        );

        let root_children = arena.children_of(root);
        let lead_key = root_children[0];
        let badge_key = root_children[1];
        let trailing_key = root_children[2];
        let note_key = root_children[3];
        let badge_text_key = arena.children_of(badge_key)[0];
        let note_text_key = arena.children_of(note_key)[0];
        let badge_paint_y = {
            let node = arena.get(badge_key).expect("badge node");
            node.element
                .as_any()
                .downcast_ref::<BaseElement>()
                .expect("badge element")
                .inline_fragment_rects()[0]
                .y
        };

        let lead_fragment = {
            let node = arena.get(lead_key).expect("lead node");
            node.element
                .as_any()
                .downcast_ref::<Text>()
                .expect("lead text")
                .inline_fragment_positions()[0]
                .1
        };
        let badge_fragment = {
            let node = arena.get(badge_text_key).expect("badge text node");
            let fragments = node
                .element
                .as_any()
                .downcast_ref::<Text>()
                .expect("badge text")
                .inline_fragment_positions();
            fragments
                .iter()
                .find(|(content, _)| content.contains("badge"))
                .unwrap_or_else(|| panic!("expected visible badge fragment, got {fragments:?}"))
                .1
        };
        let trailing_fragment = {
            let node = arena.get(trailing_key).expect("trailing node");
            node.element
                .as_any()
                .downcast_ref::<Text>()
                .expect("trailing text")
                .inline_fragment_positions()[0]
                .1
        };
        let note_fragment = {
            let node = arena.get(note_text_key).expect("note text node");
            node.element
                .as_any()
                .downcast_ref::<Text>()
                .expect("note text")
                .inline_fragment_positions()[0]
                .1
        };
        assert!(
            (lead_fragment.y - badge_fragment.y).abs() < 0.5,
            "{vertical_align:?}: lead_y={} badge_y={} trailing_y={} note_y={} should match",
            lead_fragment.y,
            badge_fragment.y,
            trailing_fragment.y,
            note_fragment.y
        );
        assert!(
            (lead_fragment.y - badge_paint_y - 8.0).abs() < 0.5,
            "{vertical_align:?}: badge_paint_y={} should be padding-top above lead_y={}",
            badge_paint_y,
            lead_fragment.y
        );
        assert!(
            trailing_fragment.x > badge_fragment.x,
            "{vertical_align:?}: trailing text should follow badge text: trailing_x={} badge_x={}",
            trailing_fragment.x,
            badge_fragment.x
        );
    }
}

fn std_constraints() -> crate::view::base_component::LayoutConstraints {
    crate::view::base_component::LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    }
}

fn std_placement() -> crate::view::base_component::LayoutPlacement {
    crate::view::base_component::LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    }
}

fn first_text_descendant(
    arena: &crate::view::node_arena::NodeArena,
    root: crate::view::node_arena::NodeKey,
) -> crate::view::node_arena::NodeKey {
    let mut stack: Vec<_> = arena.children_of(root).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if arena
            .get(key)
            .is_some_and(|node| node.element.as_any().is::<Text>())
        {
            return key;
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    panic!("expected Text descendant");
}

fn walk_layout(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    out: &mut Vec<(f32, f32, f32, f32)>,
) {
    let Some(node) = arena.get(key) else {
        return;
    };
    let s = node.element.box_model_snapshot();
    out.push((s.x, s.y, s.width, s.height));
    let children = node.children.clone();
    drop(node);
    for child in children {
        walk_layout(arena, child, out);
    }
}

fn collect_text_like_boxes(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    out: &mut Vec<(f32, f32)>,
) {
    let Some(node) = arena.get(key) else {
        return;
    };
    let el = node.element.as_ref();
    if el.as_any().is::<Text>() || el.as_any().is::<TextArea>() {
        let s = el.box_model_snapshot();
        out.push((s.width, s.height));
    }
    let children = node.children.clone();
    drop(node);
    for child in children {
        collect_text_like_boxes(arena, child, out);
    }
}

#[test]
fn text_nodes_keep_expected_layout_bounds_in_scene() {
    let first_panel = host_element_node()
        .with_prop(
            "style",
            style_with_size(
                style_with_radius(style_bg_border("#4CC9F0", "#1D3557", 8.0), 10.0),
                240.0,
                140.0,
            ),
        )
        .with_child(host_element_node().with_prop(
            "style",
            style_with_size(style_bg_border("#FFD166", "#EF476F", 3.0), 72.0, 48.0),
        ))
        .with_child(host_element_node().with_prop(
            "style",
            style_with_size(style_bg_border("#F72585", "#B5179E", 4.0), 120.0, 80.0),
        ))
        .with_child(
            host_text_node()
                .with_prop("font_size", 26)
                .with_prop("style", text_style_with_color("#0F172A"))
                .with_prop("font", "Noto Sans CJK TC")
                .with_child(RsxNode::text("Hello Rust GUI Text Test")),
        );

    let second_panel = host_element_node()
        .with_prop(
            "style",
            style_with_size(
                style_with_radius(style_bg_border("#1E293B", "#38BDF8", 3.0), 16.0),
                240.0,
                180.0,
            ),
        )
        .with_child(
            host_text_node()
                .with_prop("font_size", 22)
                .with_prop("style", text_style_with_color("#E2E8F0"))
                .with_prop("font", "Noto Sans CJK TC")
                .with_child(RsxNode::text("Test Component")),
        )
        .with_child(
            host_text_node()
                .with_prop("font_size", 14)
                .with_prop("style", text_style_with_color("#CBD5E1"))
                .with_prop("font", "Noto Sans CJK TC")
                .with_child(RsxNode::text(
                    "Used to verify event hit-testing and bubbling.",
                )),
        )
        .with_child(
            host_text_node()
                .with_prop("font_size", 14)
                .with_prop("style", text_style_with_color("#F8FAFC"))
                .with_prop("font", "Noto Sans CJK TC")
                .with_child(RsxNode::text("Click Count: 0")),
        );

    let tree = RsxNode::fragment(vec![first_panel, second_panel]);

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    for root in &roots {
        measure_and_place(&mut arena, *root, std_constraints(), std_placement());
    }

    let mut boxes = Vec::new();
    for root in &roots {
        walk_layout(&arena, *root, &mut boxes);
    }
    println!("boxes={boxes:?}");

    assert!(boxes.iter().any(|&(x, y, w, h)| (x - 3.0).abs() < 0.1
        && (y - 3.0).abs() < 0.1
        && w > 120.0
        && h > 20.0));
    assert!(boxes.iter().any(|&(x, y, w, h)| (x - 3.0).abs() < 0.1
        && (y - 3.0).abs() < 0.1
        && w > 80.0
        && h > 12.0));
}

#[test]
fn element_padding_offsets_child_layout() {
    let tree = host_element_node()
        .with_prop(
            "style",
            style_with_size(empty_element_style(), 200.0, 120.0),
        )
        .with_prop("padding_left", 8)
        .with_prop("padding_top", 12)
        .with_prop("padding_right", 16)
        .with_prop("padding_bottom", 10)
        .with_child(
            host_text_node()
                .with_prop("style", text_style_with_size(300.0, 300.0))
                .with_child(RsxNode::text("inner")),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    for root in &roots {
        measure_and_place(&mut arena, *root, std_constraints(), std_placement());
    }

    let mut boxes = Vec::new();
    for root in &roots {
        walk_layout(&arena, *root, &mut boxes);
    }

    assert!(
        boxes
            .iter()
            .any(|&(x, y, w, h)| x == 0.0 && y == 0.0 && w == 200.0 && h == 120.0)
    );
    assert!(
        boxes
            .iter()
            .any(|&(x, y, w, h)| x == 8.0 && y == 12.0 && w > 0.0 && h > 0.0),
        "boxes={boxes:?}"
    );
}

#[test]
fn flow_row_without_explicit_size_uses_children_content_size() {
    let row_style = ElementStylePropSchema {
        layout: Some(Layout::flex().row().into()),
        gap: Some(Length::px(8.0)),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", row_style)
        .with_child(
            host_element_node()
                .with_prop("style", style_with_size(empty_element_style(), 98.0, 34.0)),
        )
        .with_child(
            host_element_node()
                .with_prop("style", style_with_size(empty_element_style(), 98.0, 34.0)),
        )
        .with_child(
            host_element_node()
                .with_prop("style", style_with_size(empty_element_style(), 70.0, 34.0)),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    assert_eq!(snapshot.width, 282.0);
    assert_eq!(snapshot.height, 34.0);
}

#[test]
fn cursor_style_inherits_to_child_when_child_has_no_cursor() {
    let parent_style = ElementStylePropSchema {
        width: Some(Length::px(100.0)),
        height: Some(Length::px(100.0)),
        background: Some(crate::style::Background::Color(Box::new(
            IntoColor::<Color>::into_color(Color::hex("#101010")),
        ))),
        cursor: Some(Cursor::Pointer),
        ..empty_element_style()
    };

    let child_style = ElementStylePropSchema {
        width: Some(Length::px(40.0)),
        height: Some(Length::px(40.0)),
        background: Some(crate::style::Background::Color(Box::new(
            IntoColor::<Color>::into_color(Color::hex("#ff0000")),
        ))),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(host_element_node().with_prop("style", child_style));

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let target_key = hit_test(&arena, root, 10.0, 10.0).expect("hit child");
    let target_stable_id = arena.get(target_key).unwrap().element.stable_id();
    let cursor = get_cursor_by_id(&arena, root, target_stable_id).expect("cursor exists");
    assert_eq!(cursor, Cursor::Pointer);
}

#[test]
fn cursor_style_inherits_to_text_child() {
    let parent_style = ElementStylePropSchema {
        width: Some(Length::px(200.0)),
        height: Some(Length::px(80.0)),
        background: Some(crate::style::Background::Color(Box::new(
            IntoColor::<Color>::into_color(Color::hex("#101010")),
        ))),
        cursor: Some(Cursor::Pointer),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_node()
                .with_prop("font_size", 16.0)
                .with_child(RsxNode::text("Button label")),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let target_key = hit_test(&arena, root, 10.0, 10.0).expect("hit text child");
    let target_stable_id = arena.get(target_key).unwrap().element.stable_id();
    let cursor = get_cursor_by_id(&arena, root, target_stable_id).expect("cursor exists");
    assert_eq!(cursor, Cursor::Pointer);
}

#[test]
fn text_style_font_size_em_inherits_from_parent_font_size() {
    let parent_style = ElementStylePropSchema {
        font_size: Some(FontSize::px(20.0)),
        ..empty_element_style()
    };
    let child_style = TextStylePropSchema {
        font_size: Some(FontSize::em(1.5)),
        ..empty_text_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_node()
                .with_prop("style", child_style)
                .with_child(RsxNode::text("MMMMMMMM")),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let mut text_boxes = Vec::new();
    collect_text_like_boxes(&arena, root, &mut text_boxes);
    let (width, height) = text_boxes.first().copied().expect("text box should exist");
    assert!(width > 150.0);
    assert!(height >= 30.0);
}

#[test]
fn rem_font_size_uses_viewport_style_root_font_size() {
    let text_tree = host_text_node()
        .with_prop(
            "style",
            TextStylePropSchema {
                font_size: Some(FontSize::rem(2.0)),
                ..empty_text_style()
            },
        )
        .with_child(RsxNode::text("MMMMMMMM"));

    let mut small_root_style = Style::new();
    small_root_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(10.0)),
    );
    let mut large_root_style = Style::new();
    large_root_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(20.0)),
    );

    let mut small_arena = crate::view::test_support::new_test_arena();
    let small = crate::view::test_support::commit_rsx_tree_with_context(
        &mut small_arena,
        &text_tree,
        &small_root_style,
        800.0,
        600.0,
    );
    let mut large_arena = crate::view::test_support::new_test_arena();
    let large = crate::view::test_support::commit_rsx_tree_with_context(
        &mut large_arena,
        &text_tree,
        &large_root_style,
        800.0,
        600.0,
    );

    for root in &small {
        measure_and_place(&mut small_arena, *root, std_constraints(), std_placement());
    }
    for root in &large {
        measure_and_place(&mut large_arena, *root, std_constraints(), std_placement());
    }

    let small_snapshot = small_arena
        .get(*small.first().expect("small root"))
        .unwrap()
        .element
        .box_model_snapshot();
    let large_snapshot = large_arena
        .get(*large.first().expect("large root"))
        .unwrap()
        .element
        .box_model_snapshot();
    assert!(large_snapshot.width > small_snapshot.width * 1.5);
    assert!(large_snapshot.height > small_snapshot.height * 1.5);
}

#[test]
fn textarea_inherits_font_size_from_parent_style() {
    let parent_style = ElementStylePropSchema {
        font_size: Some(FontSize::px(24.0)),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_area_node()
                .with_prop("content", "hello")
                .with_prop("multiline", false),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let mut text_boxes = Vec::new();
    collect_text_like_boxes(&arena, root, &mut text_boxes);
    let (_width, height) = text_boxes
        .iter()
        .copied()
        .find(|(_, h)| *h > 0.0)
        .expect("textarea box should exist");
    assert!(height >= 24.0);
}

#[test]
fn textarea_uses_style_color_and_inherits_parent_color() {
    let parent_color = IntoColor::<Color>::into_color(Color::hex("#336699"));
    let local_color = IntoColor::<Color>::into_color(Color::hex("#aa5500"));

    let parent_style = ElementStylePropSchema {
        color: Some(Box::new(parent_color)),
        ..empty_element_style()
    };

    let textarea_style = ElementStylePropSchema {
        color: Some(Box::new(local_color)),
        ..empty_element_style()
    };

    let inherited_tree = host_element_node()
        .with_prop("style", parent_style.clone())
        .with_child(
            host_text_area_node()
                .with_prop("content", "hello")
                .with_prop("multiline", false),
        );
    let explicit_tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_area_node()
                .with_prop("style", textarea_style)
                .with_prop("content", "hello")
                .with_prop("multiline", false),
        );

    let mut inherited_arena = crate::view::test_support::new_test_arena();
    let inherited = commit_rsx_tree(&mut inherited_arena, &inherited_tree);
    let mut explicit_arena = crate::view::test_support::new_test_arena();
    let explicit = commit_rsx_tree(&mut explicit_arena, &explicit_tree);

    let inherited_ta_key = {
        let root = *inherited.first().expect("inherited root");
        *inherited_arena
            .children_of(root)
            .first()
            .expect("inherited ta child")
    };
    let explicit_ta_key = {
        let root = *explicit.first().expect("explicit root");
        *explicit_arena
            .children_of(root)
            .first()
            .expect("explicit ta child")
    };

    let inherited_rgba = inherited_arena
        .get(inherited_ta_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("inherited textarea")
        .color
        .to_rgba_f32();
    let explicit_rgba = explicit_arena
        .get(explicit_ta_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("explicit textarea")
        .color
        .to_rgba_f32();

    assert_eq!(inherited_rgba, parent_color.to_rgba_f32());
    assert_eq!(explicit_rgba, local_color.to_rgba_f32());
}

#[test]
fn textarea_style_bridge_resolves_em_font_size_from_inherited_parent() {
    let parent_style = ElementStylePropSchema {
        font_size: Some(FontSize::px(24.0)),
        ..empty_element_style()
    };
    let textarea_style = ElementStylePropSchema {
        font_size: Some(FontSize::em(1.25)),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_area_node()
                .with_prop("style", textarea_style)
                .with_prop("content", "hello")
                .with_prop("multiline", false),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let text_area_key = *arena.children_of(root).first().expect("textarea child");
    let text_area_node = arena.get(text_area_key).unwrap();
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("textarea");

    assert!((text_area.font_size - 30.0).abs() < f32::EPSILON);
}

#[test]
fn textarea_style_bridge_applies_existing_text_fields() {
    let local_color = IntoColor::<Color>::into_color(Color::hex("#aa5500"));
    let textarea_style = ElementStylePropSchema {
        color: Some(Box::new(local_color)),
        font: Some(FontFamily::new(["Inter", "system-ui"])),
        font_weight: Some(FontWeight::new(650)),
        line_height: Some(1.7),
        cursor: Some(Cursor::Pointer),
        ..empty_element_style()
    };

    let tree = host_text_area_node()
        .with_prop("style", textarea_style)
        .with_prop("content", "hello")
        .with_prop("multiline", false);

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let text_area_node = arena.get(*roots.first().expect("textarea root")).unwrap();
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("textarea");

    assert_eq!(text_area.color.to_rgba_f32(), local_color.to_rgba_f32());
    assert_eq!(text_area.font_families, vec!["Inter", "system-ui"]);
    assert_eq!(text_area.font_weight, 650);
    assert!((text_area.line_height - 1.7).abs() < f32::EPSILON);
    assert_eq!(text_area.cursor, Cursor::Pointer);
}

#[test]
fn textarea_style_width_height_remain_box_model_noops() {
    let textarea_style = ElementStylePropSchema {
        width: Some(Length::px(120.0)),
        height: Some(Length::px(48.0)),
        font_size: Some(FontSize::px(18.0)),
        ..empty_element_style()
    };

    let tree = host_text_area_node()
        .with_prop("style", textarea_style)
        .with_prop("content", "hello")
        .with_prop("multiline", false);

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("textarea root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    assert!(
        snapshot.width > 120.0,
        "TextArea style.width should remain a no-op; got {}",
        snapshot.width
    );
    assert!(
        (snapshot.height - 48.0).abs() > 0.5,
        "TextArea style.height should remain a no-op; got {}",
        snapshot.height
    );
}

#[test]
fn textarea_accepts_on_blur_prop() {
    let tree = rsx! {
        <crate::view::TextArea
            on_blur={move |event: &mut crate::ui::BlurEvent| event.meta.stop_propagation()}
            content="hello"
            multiline={false}
        />
    };

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    assert_eq!(roots.len(), 1);
    assert!(
        arena
            .get(roots[0])
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .is_some()
    );
}

// v1 TextArea accepted width/height directly; per design A1 v2 does
// not — the box model lives on a wrapping `<Element>`. The two old
// size-on-textarea tests were dropped in P7.

#[test]
fn nested_container_percent_height_without_definite_parent_does_not_keep_placeholder_size() {
    let root_style = ElementStylePropSchema {
        width: Some(Length::px(200.0)),
        ..empty_element_style()
    };

    let child_style = ElementStylePropSchema {
        height: Some(Length::percent(100.0)),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", root_style)
        .with_child(host_element_node().with_prop("style", child_style));

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let child_key = *arena.children_of(root).first().expect("child");
    let root_snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    let child_snapshot = arena.get(child_key).unwrap().element.box_model_snapshot();
    assert_eq!(root_snapshot.height, 0.0);
    assert_eq!(child_snapshot.height, 0.0);
}

// ---------- TextArea (v2 — formerly TextArea) acceptance ----------

fn measured_run_size(
    arena: &crate::view::node_arena::NodeArena,
    text_area_key: crate::view::node_arena::NodeKey,
) -> (f32, f32, bool) {
    let child_keys = arena.children_of(text_area_key);
    let run_key = *child_keys.first().expect("TextArea spawns one Run");
    let snapshot = arena.get(run_key).unwrap().element.box_model_snapshot();
    let is_run = arena
        .get(run_key)
        .unwrap()
        .element
        .as_any()
        .is::<crate::view::base_component::text_area::TextAreaTextRun>();
    (snapshot.width, snapshot.height, is_run)
}

fn subtree_has_text_descendant(
    arena: &crate::view::node_arena::NodeArena,
    root: crate::view::node_arena::NodeKey,
) -> bool {
    let mut stack = arena.children_of(root);
    while let Some(key) = stack.pop() {
        if arena
            .get(key)
            .is_some_and(|node| node.element.as_any().is::<Text>())
        {
            return true;
        }
        stack.extend(arena.children_of(key));
    }
    false
}

#[test]
fn text_area_v2_content_spawns_a_text_run_and_shapes() {
    let tree = host_text_area_node().with_prop("content", "hello world");
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let (w, h, is_run) = measured_run_size(&arena, root);
    assert!(is_run, "TextArea's first child must be a TextAreaTextRun");
    assert!(w > 0.0, "Run must have shaped width, got {w}");
    assert!(h > 0.0, "Run must have shaped height, got {h}");

    // TextArea itself wraps the run and reports the same content extent.
    let ta_snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    assert!(ta_snapshot.width >= w - 0.5);
    assert!(ta_snapshot.height >= h - 0.5);
}

#[test]
fn text_area_v2_cursor_style_cascades_to_generated_run() {
    let style = ElementStylePropSchema {
        cursor: Some(Cursor::Pointer),
        ..empty_element_style()
    };
    let tree = host_text_area_node()
        .with_prop("content", "hello world")
        .with_prop("style", style);
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let root_stable_id = arena.get(root).unwrap().element.stable_id();
    let root_cursor = get_cursor_by_id(&arena, root, root_stable_id).expect("root cursor exists");
    assert_eq!(root_cursor, Cursor::Pointer);

    let run = *arena
        .children_of(root)
        .first()
        .expect("TextArea should spawn a generated run");
    let run_stable_id = arena.get(run).unwrap().element.stable_id();
    let run_cursor = get_cursor_by_id(&arena, root, run_stable_id).expect("run cursor exists");
    assert_eq!(run_cursor, Cursor::Pointer);
}

#[test]
fn text_area_v2_cursor_style_cascades_to_projection_text() {
    let style = ElementStylePropSchema {
        cursor: Some(Cursor::Text),
        ..empty_element_style()
    };
    let tree = host_text_area_node()
        .with_prop("content", "aa/v1/users/bb")
        .with_prop("style", style)
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(2..12, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("/v1/users/")))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let projection = arena.children_of(root)[1];
    let mut stack = arena.children_of(projection);
    let mut projection_text = None;
    while let Some(key) = stack.pop() {
        if arena
            .get(key)
            .is_some_and(|node| node.element.as_any().is::<Text>())
        {
            projection_text = Some(key);
            break;
        }
        stack.extend(arena.children_of(key));
    }
    let projection_text = projection_text.expect("projection should contain Text");
    let stable_id = arena.get(projection_text).unwrap().element.stable_id();
    let cursor = get_cursor_by_id(&arena, root, stable_id).expect("cursor exists");
    assert_eq!(cursor, Cursor::Text);
}

#[test]
fn text_area_v2_plain_run_between_projections_hit_tests_as_text_cursor() {
    let tree = host_text_area_node()
        .with_prop("content", "{{API_HOST}}/v1/users/{{USER_ID}}/activity")
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(0..12, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("{{API_HOST}}")))
                    });
                    render.range(22..33, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("{{USER_ID}}")))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let children = arena.children_of(root);
    assert_eq!(children.len(), 4);
    let middle_run = children[1];
    assert!(
        arena
            .get(middle_run)
            .is_some_and(|node| node.element.as_any().is::<TextAreaTextRun>()),
        "expected /v1/users/ to be a generated TextAreaTextRun",
    );
    let snap = arena.get(middle_run).unwrap().element.box_model_snapshot();
    let target = hit_test(
        &arena,
        root,
        snap.x + snap.width * 0.5,
        snap.y + snap.height * 0.5,
    )
    .expect("hit-test should find the middle plain run");
    let stable_id = arena.get(target).unwrap().element.stable_id();
    let cursor = get_cursor_by_id(&arena, root, stable_id).expect("cursor exists");
    assert_eq!(cursor, Cursor::Text);
}

#[test]
fn text_area_v2_projection_applies_on_first_measure() {
    let tree = host_text_area_node()
        .with_prop("content", "abXYZcd")
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(2..5, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("XYZ")))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let children = arena.children_of(root);
    assert_eq!(
        children.len(),
        3,
        "first measure should rebuild into Run / projection / Run",
    );
    assert!(
        !arena
            .get(children[1])
            .unwrap()
            .element
            .as_any()
            .is::<crate::view::base_component::text_area::TextAreaTextRun>(),
        "middle child should be projection output, not the original plain Run",
    );
    assert!(
        subtree_has_text_descendant(&arena, children[1]),
        "projection subtree should contain the projected Text on the first frame",
    );
}

#[test]
fn text_area_v2_empty_content_with_placeholder_spawns_placeholder_run() {
    let tree = host_text_area_node().with_prop("placeholder", "type here");
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let (_, _, is_run) = measured_run_size(&arena, root);
    assert!(is_run, "Placeholder fallback must spawn a Run");
    let run_key = *arena.children_of(root).first().unwrap();
    let is_placeholder = arena
        .get(run_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
        .unwrap()
        .is_placeholder;
    assert!(
        is_placeholder,
        "placeholder Run must carry is_placeholder=true"
    );
}

#[test]
fn text_area_auto_wrap_false_cascades_nowrap_to_projection_text() {
    // Repro for the bug where `auto_wrap=false` on TextArea did not
    // disable wrapping inside projection subtrees: the projection's inner
    // <Text> kept its default TextWrap::Wrap, and once preceding inline
    // content consumed line space, projection Text could still wrap and
    // force the trailing run to a new visual line even though
    // `solver_wrap=false`.
    use crate::style::TextWrap;

    let tree = host_text_area_node()
        .with_prop("auto_wrap", false)
        .with_prop("content", "abXYZcd")
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(2..5, |_text_area_node| {
                        host_element_node()
                            .with_child(host_text_node().with_child(RsxNode::text("XYZ")))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let projection_key = arena.children_of(root)[1];
    let inner_text_key = first_text_descendant(&arena, projection_key);
    let text_wrap = {
        let node = arena.get(inner_text_key).expect("inner Text node");
        node.element
            .as_any()
            .downcast_ref::<Text>()
            .expect("inner Text component")
            .text_wrap()
    };
    assert_eq!(
        text_wrap,
        TextWrap::NoWrap,
        "projection's inner Text must inherit TextWrap::NoWrap when TextArea auto_wrap=false",
    );
}

#[test]
fn text_area_auto_wrap_false_keeps_projection_and_trailing_run_on_same_line() {
    // End-to-end repro of the visual bug: with `auto_wrap=false`, a
    // projection placed mid-line (after a wide preceding run that consumed
    // most of the available width) used to wrap the projection's inner
    // Text, pushing the trailing plain Run onto a new line. With the cascade
    // fix, the projection emits a single non-breaking fragment and the
    // outer line stays intact (overflowing horizontally instead — the
    // expected NoWrap behavior).
    let tree = host_text_area_node()
        .with_prop("auto_wrap", false)
        .with_prop("content", "aaaaaaaaaaaaaaaaaaaa{{LONG_TOKEN_NAME}}bbbb")
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(20..38, |_text_area_node| {
                        host_element_node().with_child(
                            host_text_node().with_child(RsxNode::text("{{LONG_TOKEN_NAME}}")),
                        )
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let narrow = crate::view::base_component::LayoutConstraints {
        max_width: 120.0,
        max_height: 600.0,
        viewport_width: 120.0,
        viewport_height: 600.0,
        percent_base_width: Some(120.0),
        percent_base_height: Some(600.0),
    };
    let placement = crate::view::base_component::LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 120.0,
        available_height: 600.0,
        viewport_width: 120.0,
        viewport_height: 600.0,
        percent_base_width: Some(120.0),
        percent_base_height: Some(600.0),
    };
    measure_and_place(&mut arena, root, narrow, placement);

    let children = arena.children_of(root);
    assert_eq!(children.len(), 3, "expected Run / projection / Run layout");
    let y_values: Vec<f32> = children
        .iter()
        .map(|key| arena.get(*key).unwrap().element.box_model_snapshot().y)
        .collect();
    assert_eq!(
        y_values[0], y_values[1],
        "projection should stay on the same line as the preceding Run",
    );
    assert_eq!(
        y_values[1], y_values[2],
        "trailing Run should stay on the same line as the projection (no force_break leak)",
    );
}

#[test]
fn text_area_projection_wrap_moves_token_to_next_line_when_remaining_width_is_too_small() {
    let tree = host_text_area_node()
        .with_prop("auto_wrap", true)
        .with_prop(
            "content",
            "First line with a long value that can wrap when auto wrap is enabled. {{API_HOST}}/v1/users/",
        )
        .with_prop(
            "on_render",
            crate::ui::on_text_area_render(
                |render: &mut crate::view::base_component::TextAreaRenderString| {
                    render.range(70..82, |_text_area_node| {
                        host_element_node().with_child(host_text_node().with_child(
                            RsxNode::text("{{API_HOST}}"),
                        ))
                    });
                },
            ),
        );
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let narrow = crate::view::base_component::LayoutConstraints {
        max_width: 160.0,
        max_height: 600.0,
        viewport_width: 160.0,
        viewport_height: 600.0,
        percent_base_width: Some(160.0),
        percent_base_height: Some(600.0),
    };
    let placement = crate::view::base_component::LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 160.0,
        available_height: 600.0,
        viewport_width: 160.0,
        viewport_height: 600.0,
        percent_base_width: Some(160.0),
        percent_base_height: Some(600.0),
    };
    measure_and_place(&mut arena, root, narrow, placement);

    let children = arena.children_of(root);
    assert_eq!(children.len(), 3, "expected Run / projection / Run layout");
    let text_key = first_text_descendant(&arena, children[1]);
    let fragments = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Text>()
        .expect("projection Text")
        .inline_fragment_positions();
    assert_eq!(
        fragments.first().map(|(content, _)| content.as_str()),
        Some("{{API_HOST}}"),
        "projection token should not be split to fill a tiny remaining line fragment: {fragments:?}",
    );
}

#[test]
fn text_area_auto_wrap_toggle_cascades_to_existing_projection_text() {
    // Toggling `auto_wrap` after the initial commit goes through the
    // projection reconcile path (identity-matched RSX, same Element +
    // Text shape). Reconcile must re-cascade `TextWrap` onto the live
    // Text node — otherwise the cached default `TextWrap::Wrap` survives
    // and the visual bug recurs every time the user flips the toggle.
    use crate::style::TextWrap;

    let make_tree = |auto_wrap: bool| {
        host_text_area_node()
            .with_prop("auto_wrap", auto_wrap)
            .with_prop("content", "abXYZcd")
            .with_prop(
                "on_render",
                crate::ui::on_text_area_render(
                    |render: &mut crate::view::base_component::TextAreaRenderString| {
                        render.range(2..5, |_text_area_node| {
                            host_element_node()
                                .with_child(host_text_node().with_child(RsxNode::text("XYZ")))
                        });
                    },
                ),
            )
    };
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &make_tree(true));
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let ctx = crate::view::fiber_work::ApplyContext {
        viewport_style: &crate::style::Style::new(),
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    arena.with_element_taken(root, |element, arena_ref| {
        element.apply_prop(
            arena_ref,
            root,
            &ctx,
            "auto_wrap",
            crate::ui::PropValue::Bool(false),
        );
    });
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let projection_key = arena.children_of(root)[1];
    let inner_text_key = first_text_descendant(&arena, projection_key);
    let text_wrap = {
        let node = arena.get(inner_text_key).expect("inner Text node");
        node.element
            .as_any()
            .downcast_ref::<Text>()
            .expect("inner Text component")
            .text_wrap()
    };
    assert_eq!(
        text_wrap,
        TextWrap::NoWrap,
        "after toggling auto_wrap=true→false, projection's existing Text must pick up the new cascade",
    );

    arena.with_element_taken(root, |element, arena_ref| {
        element.apply_prop(
            arena_ref,
            root,
            &ctx,
            "auto_wrap",
            crate::ui::PropValue::Bool(true),
        );
    });
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let inner_text_key = first_text_descendant(&arena, projection_key);
    let text_wrap = {
        let node = arena.get(inner_text_key).expect("inner Text node");
        node.element
            .as_any()
            .downcast_ref::<Text>()
            .expect("inner Text component")
            .text_wrap()
    };
    assert_eq!(
        text_wrap,
        TextWrap::Wrap,
        "after toggling auto_wrap=false→true, projection's existing Text must fall back to its own/default wrap",
    );
}

#[test]
fn text_area_v2_no_content_no_placeholder_has_no_children() {
    let tree = host_text_area_node();
    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    assert!(arena.children_of(root).is_empty());
}

// ---------------------------------------------------------------------------
// Phase 6b regression: a downstream-style custom host that implements
// `HostBuilder` (no `RsxTag` boilerplate) gets dispatched through
// `host_builder_node` without any change to renderer_adapter.
// ---------------------------------------------------------------------------

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
