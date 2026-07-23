//! Tests for `renderer_adapter.rs` — extracted to keep the
//! production module readable. Sibling test module so it can
//! reach `pub(crate)` helpers via `crate::view::renderer_adapter::*`.

#![cfg(test)]

use crate::style::style_props::{StylePropError, TextStyleSet, validate_style};
use crate::style::{
    Border, BorderRadius, Color, ColorLike, Cursor, FontFamily, FontSize, FontWeight, IntoColor,
    Layout, Length, Padding, ParsedValue, PropertyId, Scale, Style, TextWrap, Transform,
    TransformOrigin, Translate, Unit, VerticalAlign,
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
    DebugType, Element as HostElement, ElementStylePropSchema, Text as HostText,
    TextArea as HostTextArea, TextStylePropSchema,
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

mod style_bridge_tests;
mod rsx_identity_tests;
mod layout_scene_tests;
mod style_inheritance_tests;
mod text_area_style_tests;
mod text_area_projection_tests;
mod text_area_wrap_tests;
