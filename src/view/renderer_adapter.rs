#![allow(missing_docs)]

//! Adapters that convert RSX trees into low-level retained host elements.
use rustc_hash::FxHashMap;

use crate::style::Style;
use crate::ui::{
    Binding, FromPropValue, GlobalKey, PropValue, RsxElementNode, RsxKey, RsxNode,
    RsxNodeIdentity, RsxTagDescriptor, RsxTextNode, use_context,
};
use crate::view::base_component::{
    Element, ElementTrait, Image, Svg, Text, TextArea, TextAreaImeContext,
};
use crate::view::base_component::text_area::{TextAreaProjectionSegment, TextAreaTextRun};
use crate::view::node_arena::{Node, NodeArena, NodeKey};
use crate::view::{
    ElementStylePropSchema, ImageFit, ImageSampling, ImageSource, SvgSource, TextStylePropSchema,
};
use crate::style::{AnchorName, Color, Cursor, Length, ParsedValue, Position, PropertyId, TextWrap};
fn element_runtime_name(node: &RsxElementNode) -> &str {
    node.tag_descriptor
        .map(|descriptor| descriptor.type_name)
        .unwrap_or(node.tag)
}

fn element_display_name(node: &RsxElementNode) -> &str {
    node.tag
}

fn is_text_descriptor(descriptor: RsxTagDescriptor) -> bool {
    let name = descriptor.type_name;
    name.ends_with("::Text") || name == "Text"
}

fn is_text_area_descriptor(descriptor: RsxTagDescriptor) -> bool {
    let name = descriptor.type_name;
    name.ends_with("::TextArea") || name == "TextArea"
}

fn is_text_area_projection_segment_descriptor(descriptor: RsxTagDescriptor) -> bool {
    let name = descriptor.type_name;
    name.ends_with("::TextAreaProjectionSegment") || name == "TextAreaProjectionSegment"
}

fn is_builtin_text_area_projection_segment_node(node: &RsxElementNode) -> bool {
    node.tag_descriptor
        .map(is_text_area_projection_segment_descriptor)
        .unwrap_or(false)
        || node.tag == "TextAreaProjectionSegment"
}

fn is_image_descriptor(descriptor: RsxTagDescriptor) -> bool {
    let name = descriptor.type_name;
    name.ends_with("::Image") || name == "Image"
}

fn is_svg_descriptor(descriptor: RsxTagDescriptor) -> bool {
    let name = descriptor.type_name;
    name.ends_with("::Svg") || name == "Svg"
}

fn is_builtin_text_node(node: &RsxElementNode) -> bool {
    node.tag_descriptor.map(is_text_descriptor).unwrap_or(false) || node.tag == "Text"
}

fn is_builtin_text_area_node(node: &RsxElementNode) -> bool {
    node.tag_descriptor
        .map(is_text_area_descriptor)
        .unwrap_or(false)
        || node.tag == "TextArea"
}

fn is_builtin_image_node(node: &RsxElementNode) -> bool {
    node.tag_descriptor
        .map(is_image_descriptor)
        .unwrap_or(false)
        || node.tag == "Image"
}

fn is_builtin_svg_node(node: &RsxElementNode) -> bool {
    node.tag_descriptor.map(is_svg_descriptor).unwrap_or(false) || node.tag == "Svg"
}

#[derive(Clone, Debug, Default)]
pub(crate) struct InheritedTextStyle {
    pub(crate) font_families: Vec<String>,
    pub(crate) font_size: Option<f32>,
    pub(crate) root_font_size: f32,
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    pub(crate) font_weight: Option<u16>,
    pub(crate) color: Option<Color>,
    pub(crate) cursor: Option<Cursor>,
    pub(crate) text_wrap: Option<TextWrap>,
}

impl InheritedTextStyle {
    pub(crate) fn from_viewport_style(
        style: &Style,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        let default_root_font_size = 16.0;
        let root_font_size = resolve_font_size_from_style(
            style,
            default_root_font_size,
            default_root_font_size,
            viewport_width,
            viewport_height,
        )
        .unwrap_or(default_root_font_size);
        let mut inherited = Self {
            font_families: Vec::new(),
            font_size: Some(root_font_size),
            root_font_size,
            viewport_width,
            viewport_height,
            font_weight: None,
            color: None,
            cursor: None,
            text_wrap: None,
        };
        if let Some(ParsedValue::FontFamily(font_family)) = style.get(PropertyId::FontFamily) {
            inherited.font_families = font_family.as_slice().to_vec();
        }
        if let Some(ParsedValue::FontWeight(font_weight)) = style.get(PropertyId::FontWeight) {
            inherited.font_weight = Some(font_weight.value());
        }
        if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
            inherited.color = Some(color.to_color());
        }
        if let Some(ParsedValue::Cursor(cursor)) = style.get(PropertyId::Cursor) {
            inherited.cursor = Some(*cursor);
        }
        if let Some(ParsedValue::TextWrap(text_wrap)) = style.get(PropertyId::TextWrap) {
            inherited.text_wrap = Some(*text_wrap);
        }
        inherited
    }

    /// Mutate `self` with the text-cascading declarations authored in
    /// an Element's `style` prop. Mirrors the merge previously inlined
    /// in `build_container_element_shell`; pulled out so the
    /// incremental-commit path can replay the same cascade walking
    /// the arena parent chain (M6).
    ///
    /// Keep synchronised with `from_viewport_style` above — those two
    /// are the only places the cascade's property list is enumerated.
    pub(crate) fn merge_style(&mut self, style: &Style) {
        if let Some(ParsedValue::FontFamily(font_family)) = style.get(PropertyId::FontFamily) {
            self.font_families = font_family.as_slice().to_vec();
        }
        if let Some(font_size) = resolve_font_size_from_style(
            style,
            self.font_size.unwrap_or(self.root_font_size),
            self.root_font_size,
            self.viewport_width,
            self.viewport_height,
        ) {
            self.font_size = Some(font_size);
        }
        if let Some(ParsedValue::FontWeight(font_weight)) = style.get(PropertyId::FontWeight) {
            self.font_weight = Some(font_weight.value());
        }
        if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
            self.color = Some(color.to_color());
        }
        if let Some(ParsedValue::Cursor(cursor)) = style.get(PropertyId::Cursor) {
            self.cursor = Some(*cursor);
        }
        if let Some(ParsedValue::TextWrap(text_wrap)) = style.get(PropertyId::TextWrap) {
            self.text_wrap = Some(*text_wrap);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GlobalNodePath {
    key: GlobalKey,
    local_path: Vec<u64>,
}

enum NodeIdSeed<'a> {
    Local {
        kind: &'a str,
        path: &'a [u64],
    },
    Global {
        global_key: GlobalKey,
        kind: &'a str,
        local_path: &'a [u64],
    },
}

pub(crate) fn rendered_node_id_by_index_path(
    root: &RsxNode,
    index_path: &[usize],
) -> Result<Option<u64>, String> {
    let mut token_path = Vec::new();
    rendered_node_id_by_index_path_impl(root, index_path, &mut token_path, None)
}

fn rendered_node_id_by_index_path_impl(
    node: &RsxNode,
    index_path: &[usize],
    token_path: &mut Vec<u64>,
    global_path: Option<GlobalNodePath>,
) -> Result<Option<u64>, String> {
    if index_path.is_empty() {
        return Ok(rendered_node_id(node, token_path, global_path.as_ref()));
    }

    let Some(children) = node.children() else {
        return Err("path traverses through a leaf node".to_string());
    };
    let index = index_path[0];
    let child = children
        .get(index)
        .ok_or_else(|| format!("invalid node path index: {index}"))?;

    let current_global_path = current_global_node_path(node, global_path.as_ref());
    let mut ordinals = FxHashMap::<&'static str, usize>::default();
    for (child_index, candidate) in children.iter().enumerate() {
        let ordinal = next_identity_ordinal(&mut ordinals, candidate.identity());
        let token = child_identity_token(candidate, ordinal);
        if child_index == index {
            token_path.push(token);
            let child_global_path =
                child_global_node_path(current_global_path.as_ref(), child, token);
            let rendered = rendered_node_id_by_index_path_impl(
                child,
                &index_path[1..],
                token_path,
                child_global_path,
            );
            token_path.pop();
            return rendered;
        }
    }

    Err(format!("invalid node path index: {index}"))
}

fn rendered_node_id(
    node: &RsxNode,
    path: &[u64],
    global_path: Option<&GlobalNodePath>,
) -> Option<u64> {
    match node {
        RsxNode::Element(element) => Some(stable_node_id_from_parts(
            element_runtime_name(element),
            path,
            global_path,
        )),
        RsxNode::Text(_) => Some(stable_node_id_from_parts("TextNode", path, global_path)),
        RsxNode::Fragment(_) => None,
        RsxNode::Component(_) => {
            unreachable!("Component node should be unwrapped before renderer_adapter")
        }
        RsxNode::Provider(_) => {
            unreachable!("Provider node should be unwrapped before renderer_adapter")
        }
    }
}

/// Build a `Box<dyn ElementTrait>` for a bare `RsxNode::Text` leaf — i.e.
/// raw text content sitting inside a parent container. Distinct from
/// [`convert_text_element`], which builds the `<Text>` host tag from an
/// `RsxNode::Element` (props + concatenated child content).
///
/// 軌 A #7: a bare text leaf has no explicit style. All inherited props go
/// through `apply_inherited` so per-prop explicit flags stay `false` —
/// future ancestor style changes re-cascade into this node.
fn convert_text_leaf(
    text: &RsxTextNode,
    path: &[u64],
    global_path: Option<&GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Box<dyn ElementTrait> {
    let content = text_with_text_area_ime_preedit(text.content.clone());
    let mut text_node = Text::from_content_with_id(
        stable_node_id_from_parts("TextNode", path, global_path),
        content,
    );
    text_node.apply_inherited(inherited_text_style);
    Box::new(text_node)
}

/// Compile-time host-element dispatch: read the factory pointer carried
/// by `descriptor.host_factory` and call it. Returns `Ok(None)` when the
/// node has no descriptor or its descriptor carries no factory.
fn invoke_host_factory(
    node: &RsxElementNode,
    path: &[u64],
) -> Result<Option<Box<dyn ElementTrait>>, String> {
    let Some(descriptor) = node.tag_descriptor else {
        return Ok(None);
    };
    let Some(factory) = descriptor.host_factory else {
        return Ok(None);
    };
    let any = factory(node, path)?;
    let host_box: Box<crate::view::host_element::HostElementBox> = any
        .downcast()
        .map_err(|_| format!("host factory for `{}` returned wrong type", descriptor.type_name))?;
    Ok(Some(host_box.0))
}

/// Build the container Element plus child-inherited text style without
/// walking children. Used by the descriptor path
/// ([`convert_container_element_desc`]).
fn build_container_element_shell(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<&GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<(Element, InheritedTextStyle), String> {
    let initial_size = if path.is_empty() { 10_000.0 } else { 0.0 };
    let mut element = Element::new_with_id(
        stable_node_id_from_parts(element_runtime_name(node), path, global_path),
        0.0,
        0.0,
        initial_size,
        initial_size,
    );
    element.set_intrinsic_size_as_percent_base(false);
    let mut base_style = Style::new();
    base_style.insert(PropertyId::Width, ParsedValue::Auto);
    base_style.insert(PropertyId::Height, ParsedValue::Auto);
    if let Some(cursor) = inherited_text_style.cursor {
        base_style.insert(PropertyId::Cursor, ParsedValue::Cursor(cursor));
    }

    let mut child_inherited_text_style = inherited_text_style.clone();
    let mut user_style = Style::new();
    let mut has_user_style = false;
    for (key, value) in node.props.iter() {
        if *key == "style" {
            let style = as_element_style(value, key)?;
            child_inherited_text_style.merge_style(&style);
            user_style = user_style + style;
            has_user_style = true;
        }
    }
    let effective_style = if has_user_style {
        base_style + user_style
    } else {
        base_style
    };
    element.apply_style(effective_style);
    for (key, value) in node.props.iter() {
        if *key == "key" {
            continue;
        }
        match *key {
            "anchor" => {
                element.set_anchor_name(Some(AnchorName::new(as_owned_string(value, key)?)))
            }
            "padding" => element.set_padding(as_f32(value, key)?),
            "padding_x" => element.set_padding_x(as_f32(value, key)?),
            "padding_y" => element.set_padding_y(as_f32(value, key)?),
            "padding_left" => element.set_padding_left(as_f32(value, key)?),
            "padding_right" => element.set_padding_right(as_f32(value, key)?),
            "padding_top" => element.set_padding_top(as_f32(value, key)?),
            "padding_bottom" => element.set_padding_bottom(as_f32(value, key)?),
            "opacity" => element.set_opacity(as_f32(value, key)?),
            "style" => {}
            other => {
                // Handler props (23 `on_*` keys) are handled by the
                // shared dispatcher so `fiber_work` can reuse the same
                // decode path. Returns Ok(false) iff `other` isn't a
                // known handler key.
                if try_assign_event_handler_prop(&mut element, other, value)? {
                    // handled
                } else {
                    return Err(format!(
                        "unknown prop `{}` on <{}>",
                        key,
                        element_display_name(node)
                    ));
                }
            }
        }
    }

    Ok((element, child_inherited_text_style))
}

fn convert_text_element(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    let mut text_content = String::new();
    let mut text = Text::from_content_with_id(
        stable_node_id_from_parts("Text", path, global_path.as_ref()),
        "",
    );
    let mut style: Option<Style> = None;
    let mut width: Option<f32> = None;
    let mut height: Option<f32> = None;

    for (key, value) in node.props.iter() {
        if *key == "key" {
            continue;
        }
        match *key {
            "style" => style = Some(as_text_style(value, key)?),
            "font_size" => {
                text.set_font_size(as_font_size_px(
                    value,
                    key,
                    inherited_text_style
                        .font_size
                        .unwrap_or(inherited_text_style.root_font_size),
                    inherited_text_style.root_font_size,
                    inherited_text_style.viewport_width,
                    inherited_text_style.viewport_height,
                )?);
            }
            "line_height" => text.set_line_height(as_f32(value, key)?),
            "align" => {
                text.set_text_align(as_text_align(value, key)?);
            }
            "font" => {
                text.set_font(as_string(value, key)?);
            }
            "opacity" => text.set_opacity(as_f32(value, key)?),
            _ => return Err(format!("unknown prop `{}` on <Text>", key,)),
        }
    }

    if let Some(style) = &style {
        if let Some(value) = style.get(PropertyId::Width) {
            width = length_from_parsed_value(value, "Text style.width")?;
        }
        if let Some(value) = style.get(PropertyId::Height) {
            height = length_from_parsed_value(value, "Text style.height")?;
        }
        if let Some(font_size) = resolve_font_size_from_style(
            style,
            inherited_text_style
                .font_size
                .unwrap_or(inherited_text_style.root_font_size),
            inherited_text_style.root_font_size,
            inherited_text_style.viewport_width,
            inherited_text_style.viewport_height,
        ) {
            text.set_font_size(font_size);
        }
        if let Some(ParsedValue::FontWeight(font_weight)) = style.get(PropertyId::FontWeight) {
            text.set_font_weight(font_weight.value());
        }
        if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
            text.set_color(color.clone());
        }
        if let Some(ParsedValue::Cursor(cursor)) = style.get(PropertyId::Cursor) {
            text.set_cursor(*cursor);
        }
        if let Some(ParsedValue::TextWrap(text_wrap)) = style.get(PropertyId::TextWrap) {
            text.set_text_wrap(*text_wrap);
        }
    }

    // 軌 A #7: the explicit setters above now flip per-prop flags
    // on `Text`. `apply_inherited` consults those flags and only
    // writes props the author didn't author. Replaces the 6-block
    // `has_explicit_*` fan-out.
    text.apply_inherited(inherited_text_style);
    if let Some(width) = width {
        text.set_width(width);
    } else {
        text.set_auto_width(true);
    }
    if let Some(height) = height {
        text.set_height(height);
    } else {
        text.set_auto_height(true);
    }

    for child in &node.children {
        append_text_children(&mut text_content, child)?;
    }

    text.set_text(text_with_text_area_ime_preedit(text_content));
    Ok(Box::new(text))
}

pub(crate) fn text_with_text_area_ime_preedit(mut content: String) -> String {
    let Some(ctx) = use_context::<TextAreaImeContext>() else {
        return content;
    };
    if ctx.preedit.is_empty() {
        return content;
    }
    let char_len = content.chars().count();
    let insert_at = ctx.local_cursor_in_projection.min(char_len);
    let byte = content
        .char_indices()
        .nth(insert_at)
        .map(|(byte, _)| byte)
        .unwrap_or(content.len());
    content.insert_str(byte, ctx.preedit.as_str());
    content
}

pub(crate) fn length_from_parsed_value(
    value: &ParsedValue,
    context: &str,
) -> Result<Option<f32>, String> {
    match value {
        ParsedValue::Length(Length::Px(v)) => Ok(Some(*v)),
        ParsedValue::Length(Length::Zero) => Ok(Some(0.0)),
        ParsedValue::Length(length @ Length::Calc(_)) => {
            if length.needs_percent_base() {
                return Err(format!("{context} does not support relative length"));
            }
            Ok(Some(length.resolve_without_percent_base(0.0, 0.0)))
        }
        ParsedValue::Auto => Ok(None),
        ParsedValue::Length(Length::Percent(_) | Length::Vh(_) | Length::Vw(_)) => {
            Err(format!("{context} does not support relative length"))
        }
        _ => Err(format!("{context} expects Length value")),
    }
}

pub(crate) fn resolve_font_size_from_style(
    style: &Style,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    let value = style.get(PropertyId::FontSize)?;
    resolve_font_size_parsed_value(
        value,
        parent_font_size,
        root_font_size,
        viewport_width,
        viewport_height,
    )
}

fn resolve_font_size_parsed_value(
    value: &ParsedValue,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    match value {
        ParsedValue::FontSize(font_size) => Some(font_size.resolve_px(
            parent_font_size,
            root_font_size,
            viewport_width,
            viewport_height,
        )),
        ParsedValue::Length(Length::Px(px)) => Some((*px).max(0.0)),
        _ => None,
    }
}

fn append_text_children(out: &mut String, node: &RsxNode) -> Result<(), String> {
    match node {
        RsxNode::Text(content) => {
            out.push_str(&content.content);
            Ok(())
        }
        RsxNode::Fragment(fragment) => {
            for child in &fragment.children {
                append_text_children(out, child)?;
            }
            Ok(())
        }
        _ => Err("<Text> children must be string".to_string()),
    }
}
/// `<TextArea>` descriptor builder.
///
/// Spawns a single `TextAreaTextRun` child at descriptor-build time for
/// the no-`on_render` case (and the placeholder fallback when content is
/// empty). Wires text-side style sources:
/// - explicit `font_size` / `font` props
/// - `style.color` → text_area.color (cascaded to the Run)
/// - inherited font-family / font-size / font-weight / color
///
/// Box-model props (`background` / `border` / `border_radius` / `padding`
/// / `width` / `height`) deliberately do *not* apply to TextArea — per
/// design A1 the user wraps `<TextArea>` in an `<Element>` for those.
/// See `docs/design/textarea-v2.md`.
fn convert_text_area_element_desc(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<ElementDescriptor, String> {
    let stable_id = stable_node_id_from_parts("TextArea", path, global_path.as_ref());
    let mut text_area = TextArea::with_stable_id(stable_id);
    let mut style: Option<Style> = None;
    let mut explicit_font_size: Option<f32> = None;
    let mut explicit_font: Option<String> = None;

    for (key, value) in node.props.iter() {
        if *key == "key" {
            continue;
        }
        match *key {
            "content" => text_area.content = as_owned_string(value, key)?,
            "placeholder" => text_area.placeholder = as_owned_string(value, key)?,
            "binding" => text_area.text_binding = Some(as_binding_string(value, key)?),
            "multiline" => text_area.multiline = as_bool(value, key)?,
            "auto_wrap" => text_area.auto_wrap = as_bool(value, key)?,
            "read_only" => text_area.read_only = as_bool(value, key)?,
            "max_length" => text_area.max_length = as_usize(value, key)?,
            "style" => style = Some(as_element_style(value, key)?),
            "font_size" => {
                explicit_font_size = Some(as_font_size_px(
                    value,
                    key,
                    inherited_text_style
                        .font_size
                        .unwrap_or(inherited_text_style.root_font_size),
                    inherited_text_style.root_font_size,
                    inherited_text_style.viewport_width,
                    inherited_text_style.viewport_height,
                )?);
            }
            "font" => explicit_font = Some(as_owned_string(value, key)?),
            "on_focus" => text_area
                .on_focus_handlers
                .push(as_text_area_focus_handler(value, key)?),
            "on_blur" => text_area
                .on_blur_handlers
                .push(as_blur_handler(value, key)?),
            "on_change" => text_area
                .on_change_handlers
                .push(as_text_change_handler(value, key)?),
            "on_render" => {
                let handler =
                    crate::ui::TextAreaRenderHandlerProp::from_prop_value(value.clone())
                        .map_err(|_| {
                            format!("prop `{key}` expects text area render handler value")
                        })?;
                text_area.on_render_handler = Some(handler);
            }
            _ => return Err(format!("unknown prop `{}` on <TextArea>", key)),
        }
    }

    // Mirror apply_prop's normalization: collapse `\n` when single-line,
    // and truncate to `max_length`. Order matches v1 setter semantics.
    if !text_area.multiline && text_area.content.contains('\n') {
        text_area.content = text_area.content.replace('\n', " ");
    }
    if let Some(limit) = text_area.max_length
        && text_area.content.chars().count() > limit
    {
        text_area.content = text_area.content.chars().take(limit).collect();
    }

    // Inherited cascade first (fills only what the author didn't set).
    if text_area.font_families.is_empty() {
        text_area.font_families = inherited_text_style.font_families.clone();
    }
    if let Some(inherited_size) = inherited_text_style.font_size {
        text_area.font_size = inherited_size;
    }
    text_area.font_weight = inherited_text_style.font_weight.unwrap_or(400);
    if let Some(inherited_color) = inherited_text_style.color {
        text_area.color = inherited_color;
    }

    // Style block (v1-style extraction): only text-side properties apply.
    if let Some(style) = &style {
        if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
            text_area.color = color.to_color();
        }
        if let Some(ParsedValue::Cursor(cursor)) = style.get(PropertyId::Cursor) {
            text_area.cursor = *cursor;
        }
        if let Some(ParsedValue::FontSize(size)) = style.get(PropertyId::FontSize) {
            text_area.font_size = size.resolve_px(
                inherited_text_style
                    .font_size
                    .unwrap_or(inherited_text_style.root_font_size),
                inherited_text_style.root_font_size,
                inherited_text_style.viewport_width,
                inherited_text_style.viewport_height,
            );
        }
        if let Some(ParsedValue::FontFamily(family)) = style.get(PropertyId::FontFamily) {
            text_area.font_families = family.as_slice().to_vec();
        }
        if let Some(ParsedValue::FontWeight(fw)) = style.get(PropertyId::FontWeight) {
            text_area.font_weight = fw.value();
        }
    }

    // Explicit `font_size` / `font` props win over `style.*` and
    // inherited (mirrors v1's setter-priority semantics).
    if let Some(size) = explicit_font_size {
        text_area.font_size = size;
    }
    if let Some(family) = explicit_font {
        text_area.font_families = vec![family];
    }

    let mut child_descriptors: Vec<ElementDescriptor> = Vec::new();
    let (display_text, is_placeholder) = if !text_area.content.is_empty() {
        (text_area.content.clone(), false)
    } else if !text_area.placeholder.is_empty() {
        (text_area.placeholder.clone(), true)
    } else {
        (String::new(), false)
    };
    if !display_text.is_empty() {
        let char_count = display_text.chars().count();
        let mut run = TextAreaTextRun::new(display_text, 0..char_count);
        run.is_placeholder = is_placeholder;
        run.cascade_style(
            text_area.font_families.clone(),
            text_area.font_size,
            text_area.line_height,
            text_area.font_weight,
            if is_placeholder {
                text_area.placeholder_color
            } else {
                text_area.color
            },
            text_area.cursor,
            text_area.auto_wrap,
        );
        child_descriptors.push(ElementDescriptor::leaf(
            Box::new(run) as Box<dyn ElementTrait>
        ));
    }

    let post_commit: Option<Box<dyn FnOnce(&mut NodeArena, NodeKey)>> =
        Some(Box::new(|arena: &mut NodeArena, text_area_key: NodeKey| {
            arena.with_element_taken(text_area_key, |element, _arena_ref| {
                if let Some(ta) = element.as_any_mut().downcast_mut::<TextArea>() {
                    ta.set_self_node_key(text_area_key);
                }
            });
        }));

    Ok(ElementDescriptor {
        element: Box::new(text_area) as Box<dyn ElementTrait>,
        children: child_descriptors,
        post_commit,
    })
}

/// `<TextAreaProjectionSegment>` descriptor builder.
///
/// Internal host emitted only by `<TextArea>` schema render to wrap a
/// single user projection in the inline child list. Carries the source
/// `char_range`; payload is the user RSX subtree which is converted
/// using the same inherited text style as TextArea's other inline
/// children (M3 will narrow this to a TextArea-resolved cascade).
fn convert_text_area_projection_segment_element_desc(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<ElementDescriptor, String> {
    let stable_id = stable_node_id_from_parts(
        "TextAreaProjectionSegment",
        path,
        global_path.as_ref(),
    );
    let mut segment = TextAreaProjectionSegment::with_stable_id(stable_id);
    let mut range_start: usize = 0;
    let mut range_end: usize = 0;
    for (key, value) in node.props.iter() {
        if *key == "key" {
            continue;
        }
        match *key {
            "char_range_start" => range_start = as_usize(value, key)?.unwrap_or(0),
            "char_range_end" => range_end = as_usize(value, key)?.unwrap_or(0),
            _ => return Err(format!("unknown prop `{}` on <TextAreaProjectionSegment>", key)),
        }
    }
    if range_end < range_start {
        range_end = range_start;
    }
    segment.set_char_range(range_start..range_end);

    let mut child_path = Vec::with_capacity(path.len().saturating_add(1));
    child_path.extend_from_slice(path);
    let current_global_path = current_global_node_path(
        &RsxNode::Element(std::rc::Rc::new(node.clone())),
        global_path.as_ref(),
    );
    let mut ordinals = FxHashMap::<&'static str, usize>::default();
    let mut child_descriptors: Vec<ElementDescriptor> = Vec::new();
    for child in &node.children {
        let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
        let token = child_identity_token(child, ordinal);
        child_path.push(token);
        let child_global_path =
            child_global_node_path(current_global_path.as_ref(), child, token);
        append_child_nodes_flattening_fragments_desc(
            child,
            &child_path,
            child_global_path,
            inherited_text_style,
            &mut child_descriptors,
        )?;
        child_path.pop();
    }

    Ok(ElementDescriptor {
        element: Box::new(segment) as Box<dyn ElementTrait>,
        children: child_descriptors,
        post_commit: None,
    })
}

pub(crate) fn convert_image_slot_desc(
    value: &PropValue,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
    slot_name: &str,
) -> Result<Vec<ElementDescriptor>, String> {
    let slot_node = RsxNode::from_prop_value(value.clone())?;
    let mut wrapper = Element::new_with_id(
        stable_node_id_from_parts(slot_name, path, global_path.as_ref()),
        0.0,
        0.0,
        10_000.0,
        10_000.0,
    );
    wrapper.set_intrinsic_size_as_percent_base(false);
    let mut wrapper_style = Style::new();
    wrapper_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(0.0))
                .right(Length::px(0.0))
                .top(Length::px(0.0))
                .bottom(Length::px(0.0)),
        ),
    );
    wrapper.apply_style(wrapper_style);

    let mut slot_path = Vec::with_capacity(path.len() + 1);
    slot_path.extend_from_slice(path);
    slot_path.push(stable_node_id(NodeIdSeed::Local {
        kind: slot_name,
        path,
    }));
    let slot_global_path = child_global_node_path(
        global_path.as_ref(),
        &slot_node,
        slot_path[slot_path.len() - 1],
    );
    let mut wrapper_children: Vec<ElementDescriptor> = Vec::new();
    append_child_nodes_flattening_fragments_desc(
        &slot_node,
        &slot_path,
        slot_global_path,
        inherited_text_style,
        &mut wrapper_children,
    )?;

    Ok(vec![ElementDescriptor {
        element: Box::new(wrapper) as Box<dyn ElementTrait>,
        children: wrapper_children,
        post_commit: None,
    }])
}

fn convert_image_element_desc(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<ElementDescriptor, String> {
    if !node.children.is_empty() {
        return Err("<Image> does not accept children; use loading/error props".to_string());
    }

    let mut source: Option<ImageSource> = None;
    let mut fit = ImageFit::Contain;
    let mut sampling = ImageSampling::Linear;
    let mut style: Option<Style> = None;
    let mut loading_descs: Vec<ElementDescriptor> = Vec::new();
    let mut error_descs: Vec<ElementDescriptor> = Vec::new();

    for (key, value) in node.props.iter() {
        if *key == "key" {
            continue;
        }
        match *key {
            "source" => source = Some(ImageSource::from_prop_value(value.clone())?),
            "fit" => fit = ImageFit::from_prop_value(value.clone())?,
            "sampling" => sampling = ImageSampling::from_prop_value(value.clone())?,
            "style" => style = Some(as_element_style(value, key)?),
            "loading" => {
                loading_descs = convert_image_slot_desc(
                    value,
                    path,
                    global_path.clone(),
                    inherited_text_style,
                    "loading",
                )?;
            }
            "error" => {
                error_descs = convert_image_slot_desc(
                    value,
                    path,
                    global_path.clone(),
                    inherited_text_style,
                    "error",
                )?;
            }
            _ => return Err(format!("unknown prop `{}` on <Image>", key)),
        }
    }

    let mut image = Image::new_with_id(
        stable_node_id_from_parts("Image", path, global_path.as_ref()),
        source.ok_or_else(|| "<Image> requires `source`".to_string())?,
    );
    image.set_fit(fit);
    image.set_sampling(sampling);
    if let Some(style) = style {
        image.apply_style(style);
    } else {
        image.apply_style(Style::new());
    }

    let has_slots = !loading_descs.is_empty() || !error_descs.is_empty();
    let post_commit: Option<Box<dyn FnOnce(&mut NodeArena, NodeKey)>> = if has_slots {
        Some(Box::new(
            move |arena: &mut NodeArena, image_key: NodeKey| {
                let loading_keys: Vec<NodeKey> = loading_descs
                    .into_iter()
                    .map(|d| commit_descriptor_tree(arena, Some(image_key), d))
                    .collect();
                let error_keys: Vec<NodeKey> = error_descs
                    .into_iter()
                    .map(|d| commit_descriptor_tree(arena, Some(image_key), d))
                    .collect();
                arena.with_element_taken(image_key, |element, _arena_ref| {
                    if let Some(img) = element.as_any_mut().downcast_mut::<Image>() {
                        img.set_loading_slot(loading_keys);
                        img.set_error_slot(error_keys);
                    }
                });
            },
        ))
    } else {
        None
    };

    Ok(ElementDescriptor {
        element: Box::new(image) as Box<dyn ElementTrait>,
        children: Vec::new(),
        post_commit,
    })
}

fn convert_svg_element_desc(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<ElementDescriptor, String> {
    if !node.children.is_empty() {
        return Err("<Svg> does not accept children; use loading/error props".to_string());
    }

    let mut source: Option<SvgSource> = None;
    let mut fit = ImageFit::Contain;
    let mut sampling = ImageSampling::Linear;
    let mut style: Option<Style> = None;
    let mut loading_descs: Vec<ElementDescriptor> = Vec::new();
    let mut error_descs: Vec<ElementDescriptor> = Vec::new();

    for (key, value) in node.props.iter() {
        if *key == "key" {
            continue;
        }
        match *key {
            "source" => source = Some(SvgSource::from_prop_value(value.clone())?),
            "fit" => fit = ImageFit::from_prop_value(value.clone())?,
            "sampling" => sampling = ImageSampling::from_prop_value(value.clone())?,
            "style" => style = Some(as_element_style(value, key)?),
            "loading" => {
                loading_descs = convert_image_slot_desc(
                    value,
                    path,
                    global_path.clone(),
                    inherited_text_style,
                    "loading",
                )?;
            }
            "error" => {
                error_descs = convert_image_slot_desc(
                    value,
                    path,
                    global_path.clone(),
                    inherited_text_style,
                    "error",
                )?;
            }
            _ => return Err(format!("unknown prop `{}` on <Svg>", key)),
        }
    }

    let mut svg = Svg::new_with_id(
        stable_node_id_from_parts("Svg", path, global_path.as_ref()),
        source.ok_or_else(|| "<Svg> requires `source`".to_string())?,
    );
    svg.set_fit(fit);
    svg.set_sampling(sampling);
    if let Some(style) = style {
        svg.apply_style(style);
    } else {
        svg.apply_style(Style::new());
    }

    let has_slots = !loading_descs.is_empty() || !error_descs.is_empty();
    let post_commit: Option<Box<dyn FnOnce(&mut NodeArena, NodeKey)>> = if has_slots {
        Some(Box::new(move |arena: &mut NodeArena, svg_key: NodeKey| {
            let loading_keys: Vec<NodeKey> = loading_descs
                .into_iter()
                .map(|d| commit_descriptor_tree(arena, Some(svg_key), d))
                .collect();
            let error_keys: Vec<NodeKey> = error_descs
                .into_iter()
                .map(|d| commit_descriptor_tree(arena, Some(svg_key), d))
                .collect();
            arena.with_element_taken(svg_key, |element, _arena_ref| {
                if let Some(s) = element.as_any_mut().downcast_mut::<Svg>() {
                    s.set_loading_slot(loading_keys);
                    s.set_error_slot(error_keys);
                }
            });
        }))
    } else {
        None
    };

    Ok(ElementDescriptor {
        element: Box::new(svg) as Box<dyn ElementTrait>,
        children: Vec::new(),
        post_commit,
    })
}

fn stable_node_id(seed: NodeIdSeed<'_>) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    match seed {
        NodeIdSeed::Local { kind, path } => {
            hash ^= 0x01;
            hash = hash.wrapping_mul(FNV_PRIME);
            for &byte in kind.as_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0xff;
            hash = hash.wrapping_mul(FNV_PRIME);
            for &index in path {
                for byte in index.to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(FNV_PRIME);
                }
                hash ^= 0xfe;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        NodeIdSeed::Global {
            global_key,
            kind,
            local_path,
        } => {
            hash ^= 0x02;
            hash = hash.wrapping_mul(FNV_PRIME);
            for byte in global_key.id().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0xfd;
            hash = hash.wrapping_mul(FNV_PRIME);
            for &byte in kind.as_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0xff;
            hash = hash.wrapping_mul(FNV_PRIME);
            for &index in local_path {
                for byte in index.to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(FNV_PRIME);
                }
                hash ^= 0xfe;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
    }

    if hash == 0 { 1 } else { hash }
}

fn stable_node_id_from_parts(
    kind: &str,
    path: &[u64],
    global_path: Option<&GlobalNodePath>,
) -> u64 {
    stable_node_id(node_id_seed(kind, path, global_path))
}

fn node_id_seed<'a>(
    kind: &'a str,
    path: &'a [u64],
    global_path: Option<&'a GlobalNodePath>,
) -> NodeIdSeed<'a> {
    if let Some(global_path) = global_path {
        NodeIdSeed::Global {
            global_key: global_path.key,
            kind,
            local_path: &global_path.local_path,
        }
    } else {
        NodeIdSeed::Local { kind, path }
    }
}

fn current_global_node_path(
    node: &RsxNode,
    inherited: Option<&GlobalNodePath>,
) -> Option<GlobalNodePath> {
    if let Some(RsxKey::Global(global_key)) = node.identity().key {
        return Some(GlobalNodePath {
            key: global_key,
            local_path: Vec::new(),
        });
    }
    inherited.cloned()
}

fn child_global_node_path(
    current: Option<&GlobalNodePath>,
    child: &RsxNode,
    token: u64,
) -> Option<GlobalNodePath> {
    if let Some(RsxKey::Global(global_key)) = child.identity().key {
        return Some(GlobalNodePath {
            key: global_key,
            local_path: Vec::new(),
        });
    }
    let current = current?;
    let mut local_path = current.local_path.clone();
    local_path.push(token);
    Some(GlobalNodePath {
        key: current.key,
        local_path,
    })
}

fn next_identity_ordinal(
    ordinals: &mut FxHashMap<&'static str, usize>,
    identity: &RsxNodeIdentity,
) -> usize {
    let entry = ordinals.entry(identity.invocation_type).or_insert(0);
    let ordinal = *entry;
    *entry += 1;
    ordinal
}

fn child_identity_token(node: &RsxNode, fallback_ordinal: usize) -> u64 {
    identity_token_from_node_identity(node.identity(), fallback_ordinal)
}

fn identity_token_from_node_identity(identity: &RsxNodeIdentity, fallback_ordinal: usize) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in identity.invocation_type.as_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    match &identity.key {
        Some(RsxKey::Local(key)) => {
            hash ^= 0x4c;
            hash = hash.wrapping_mul(FNV_PRIME);
            for byte in key.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        Some(RsxKey::Global(global_key)) => {
            hash ^= 0x47;
            hash = hash.wrapping_mul(FNV_PRIME);
            for byte in global_key.id().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        None => {
            hash ^= 0x55;
            hash = hash.wrapping_mul(FNV_PRIME);
            for byte in (fallback_ordinal as u64).to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
    }
    hash
}

pub(crate) fn as_f32(value: &PropValue, key: &str) -> Result<f32, String> {
    match value {
        PropValue::I64(v) => Ok(*v as f32),
        PropValue::F64(v) => Ok(*v as f32),
        _ => Err(format!("prop `{key}` expects numeric value")),
    }
}

pub(crate) fn as_font_size_px(
    value: &PropValue,
    key: &str,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<f32, String> {
    match value {
        PropValue::I64(v) => Ok((*v as f32).max(0.0)),
        PropValue::F64(v) => Ok((*v as f32).max(0.0)),
        PropValue::FontSize(v) => Ok(v.resolve_px(
            parent_font_size,
            root_font_size,
            viewport_width,
            viewport_height,
        )),
        _ => Err(format!("prop `{key}` expects numeric or FontSize value")),
    }
}

pub(crate) fn as_string<'a>(value: &'a PropValue, key: &str) -> Result<&'a str, String> {
    match value {
        PropValue::String(v) => Ok(v.as_str()),
        _ => Err(format!("prop `{key}` expects string value")),
    }
}

pub(crate) fn as_owned_string(value: &PropValue, key: &str) -> Result<String, String> {
    Ok(as_string(value, key)?.to_string())
}

pub(crate) fn as_text_align(value: &PropValue, key: &str) -> Result<crate::style::TextAlign, String> {
    match value {
        PropValue::TextAlign(v) => Ok(*v),
        _ => Err(format!("prop `{key}` expects TextAlign value")),
    }
}

pub(crate) fn as_binding_string(value: &PropValue, key: &str) -> Result<Binding<String>, String> {
    Binding::<String>::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects Binding<String> value"))
}

pub(crate) fn as_bool(value: &PropValue, key: &str) -> Result<bool, String> {
    match value {
        PropValue::Bool(v) => Ok(*v),
        _ => Err(format!("prop `{key}` expects bool value")),
    }
}

pub(crate) fn as_usize(value: &PropValue, key: &str) -> Result<Option<usize>, String> {
    match value {
        PropValue::I64(v) => {
            if *v < 0 {
                Err(format!("prop `{key}` expects non-negative integer value"))
            } else {
                Ok(Some(*v as usize))
            }
        }
        PropValue::F64(v) => {
            if *v < 0.0 {
                Err(format!("prop `{key}` expects non-negative numeric value"))
            } else {
                Ok(Some(*v as usize))
            }
        }
        _ => Err(format!("prop `{key}` expects numeric value")),
    }
}

pub(crate) fn as_element_style(value: &PropValue, key: &str) -> Result<Style, String> {
    ElementStylePropSchema::from_prop_value(value.clone())
        .map(|style| style.to_style())
        .map_err(|_| format!("prop `{key}` expects ElementStylePropSchema value"))
}

pub(crate) fn as_text_style(value: &PropValue, key: &str) -> Result<Style, String> {
    TextStylePropSchema::from_prop_value(value.clone())
        .map(|style| style.to_style())
        .map_err(|_| format!("prop `{key}` expects TextStylePropSchema value"))
}

/// Dispatch a single `on_*` handler prop to the matching `Element`
/// setter. Returns `Ok(true)` if `key` is one of the 23 RSX handler
/// prop names and the handler was installed; `Ok(false)` if `key`
/// isn't a handler prop (caller should continue its own dispatch);
/// `Err` on decode failure (wrong `PropValue` variant).
///
/// Callers:
/// - cold convert path in this file (`build_container_element_shell`)
/// - `fiber_work::apply_update_to_element` for the M4 #4 incremental
///   handler path (pairs with `Element::clear_rsx_event_handler` for
///   replace semantics)
pub(crate) fn try_assign_event_handler_prop(
    element: &mut Element,
    key: &str,
    value: &PropValue,
) -> Result<bool, String> {
    match key {
        "on_pointer_down" => {
            let handler = as_mouse_down_handler(value, key)?;
            element.on_pointer_down(move |event, _control| handler.call(event));
        }
        "on_pointer_up" => {
            let handler = as_mouse_up_handler(value, key)?;
            element.on_pointer_up(move |event, _control| handler.call(event));
        }
        "on_pointer_move" => {
            let handler = as_mouse_move_handler(value, key)?;
            element.on_pointer_move(move |event, _control| handler.call(event));
        }
        "on_pointer_enter" => {
            let handler = as_mouse_enter_handler(value, key)?;
            element.on_pointer_enter(move |event| handler.call(event));
        }
        "on_pointer_leave" => {
            let handler = as_mouse_leave_handler(value, key)?;
            element.on_pointer_leave(move |event| handler.call(event));
        }
        "on_click" => {
            let handler = as_click_handler(value, key)?;
            element.on_click(move |event, _control| handler.call(event));
        }
        "on_context_menu" => {
            let handler = as_context_menu_handler(value, key)?;
            element.on_context_menu(move |event, _control| handler.call(event));
        }
        "on_wheel" => {
            let handler = as_wheel_handler(value, key)?;
            element.on_wheel(move |event, _control| handler.call(event));
        }
        "on_key_down" => {
            let handler = as_key_down_handler(value, key)?;
            element.on_key_down(move |event, _control| handler.call(event));
        }
        "on_key_up" => {
            let handler = as_key_up_handler(value, key)?;
            element.on_key_up(move |event, _control| handler.call(event));
        }
        "on_focus" => {
            let handler = as_focus_handler(value, key)?;
            element.on_focus(move |event, _control| handler.call(event));
        }
        "on_blur" => {
            let handler = as_blur_handler(value, key)?;
            element.on_blur(move |event, _control| handler.call(event));
        }
        "on_ime_commit" => {
            let handler = as_ime_commit_handler(value, key)?;
            element.on_ime_commit(move |event, _control| handler.call(event));
        }
        "on_ime_enabled" => {
            let handler = as_ime_enabled_handler(value, key)?;
            element.on_ime_enabled(move |event, _control| handler.call(event));
        }
        "on_ime_disabled" => {
            let handler = as_ime_disabled_handler(value, key)?;
            element.on_ime_disabled(move |event, _control| handler.call(event));
        }
        "on_drag_start" => {
            let handler = as_drag_start_handler(value, key)?;
            element.on_drag_start(move |event, _control| handler.call(event));
        }
        "on_drag_over" => {
            let handler = as_drag_over_handler(value, key)?;
            element.on_drag_over(move |event, _control| handler.call(event));
        }
        "on_drag_leave" => {
            let handler = as_drag_leave_handler(value, key)?;
            element.on_drag_leave(move |event, _control| handler.call(event));
        }
        "on_drop" => {
            let handler = as_drop_handler(value, key)?;
            element.on_drop(move |event, _control| handler.call(event));
        }
        "on_drag_end" => {
            let handler = as_drag_end_handler(value, key)?;
            element.on_drag_end(move |event, _control| handler.call(event));
        }
        "on_copy" => {
            let handler = as_copy_handler(value, key)?;
            element.on_copy(move |event, _control| handler.call(event));
        }
        "on_cut" => {
            let handler = as_cut_handler(value, key)?;
            element.on_cut(move |event, _control| handler.call(event));
        }
        "on_paste" => {
            let handler = as_paste_handler(value, key)?;
            element.on_paste(move |event, _control| handler.call(event));
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// `&'static str` table of the 23 RSX event handler prop names. Used
/// by the incremental fiber_work whitelist gate so every `on_*` prop
/// that the cold path recognises is also committable incrementally.
pub(crate) const RSX_EVENT_HANDLER_PROPS: &[&str] = &[
    "on_pointer_down",
    "on_pointer_up",
    "on_pointer_move",
    "on_pointer_enter",
    "on_pointer_leave",
    "on_click",
    "on_context_menu",
    "on_wheel",
    "on_key_down",
    "on_key_up",
    "on_focus",
    "on_blur",
    "on_ime_commit",
    "on_ime_enabled",
    "on_ime_disabled",
    "on_drag_start",
    "on_drag_over",
    "on_drag_leave",
    "on_drop",
    "on_drag_end",
    "on_copy",
    "on_cut",
    "on_paste",
];

fn as_mouse_down_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerDownHandlerProp, String> {
    match value {
        PropValue::OnPointerDown(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer down handler value")),
    }
}

fn as_mouse_up_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerUpHandlerProp, String> {
    match value {
        PropValue::OnPointerUp(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer up handler value")),
    }
}

fn as_mouse_move_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerMoveHandlerProp, String> {
    match value {
        PropValue::OnPointerMove(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer move handler value")),
    }
}

fn as_mouse_enter_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerEnterHandlerProp, String> {
    match value {
        PropValue::OnPointerEnter(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer enter handler value")),
    }
}

fn as_mouse_leave_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::PointerLeaveHandlerProp, String> {
    match value {
        PropValue::OnPointerLeave(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects pointer leave handler value")),
    }
}

fn as_click_handler(value: &PropValue, key: &str) -> Result<crate::ui::ClickHandlerProp, String> {
    match value {
        PropValue::OnClick(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects click handler value")),
    }
}

fn as_context_menu_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::ContextMenuHandlerProp, String> {
    match value {
        PropValue::OnContextMenu(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects context menu handler value")),
    }
}

fn as_wheel_handler(value: &PropValue, key: &str) -> Result<crate::ui::WheelHandlerProp, String> {
    match value {
        PropValue::OnWheel(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects wheel handler value")),
    }
}

fn as_key_down_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::KeyDownHandlerProp, String> {
    match value {
        PropValue::OnKeyDown(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects key down handler value")),
    }
}

fn as_key_up_handler(value: &PropValue, key: &str) -> Result<crate::ui::KeyUpHandlerProp, String> {
    match value {
        PropValue::OnKeyUp(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects key up handler value")),
    }
}

fn as_focus_handler(value: &PropValue, key: &str) -> Result<crate::ui::FocusHandlerProp, String> {
    match value {
        PropValue::OnFocus(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects focus handler value")),
    }
}

fn as_blur_handler(value: &PropValue, key: &str) -> Result<crate::ui::BlurHandlerProp, String> {
    match value {
        PropValue::OnBlur(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects blur handler value")),
    }
}

macro_rules! as_event_handler_fn {
    ($fn_name:ident, $ty:ty, $variant:ident, $label:expr) => {
        fn $fn_name(value: &PropValue, key: &str) -> Result<$ty, String> {
            match value {
                PropValue::$variant(v) => Ok(v.clone()),
                _ => Err(format!("prop `{}` expects {} handler value", key, $label)),
            }
        }
    };
}

as_event_handler_fn!(
    as_ime_commit_handler,
    crate::ui::ImeCommitHandlerProp,
    OnImeCommit,
    "ime commit"
);
as_event_handler_fn!(
    as_ime_enabled_handler,
    crate::ui::ImeEnabledHandlerProp,
    OnImeEnabled,
    "ime enabled"
);
as_event_handler_fn!(
    as_ime_disabled_handler,
    crate::ui::ImeDisabledHandlerProp,
    OnImeDisabled,
    "ime disabled"
);
as_event_handler_fn!(
    as_drag_start_handler,
    crate::ui::DragStartHandlerProp,
    OnDragStart,
    "drag start"
);
as_event_handler_fn!(
    as_drag_over_handler,
    crate::ui::DragOverHandlerProp,
    OnDragOver,
    "drag over"
);
as_event_handler_fn!(
    as_drag_leave_handler,
    crate::ui::DragLeaveHandlerProp,
    OnDragLeave,
    "drag leave"
);
as_event_handler_fn!(as_drop_handler, crate::ui::DropHandlerProp, OnDrop, "drop");
as_event_handler_fn!(
    as_drag_end_handler,
    crate::ui::DragEndHandlerProp,
    OnDragEnd,
    "drag end"
);
as_event_handler_fn!(as_copy_handler, crate::ui::CopyHandlerProp, OnCopy, "copy");
as_event_handler_fn!(as_cut_handler, crate::ui::CutHandlerProp, OnCut, "cut");
as_event_handler_fn!(
    as_paste_handler,
    crate::ui::PasteHandlerProp,
    OnPaste,
    "paste"
);

fn as_text_area_focus_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::TextAreaFocusHandlerProp, String> {
    match value {
        PropValue::OnTextAreaFocus(v) => Ok(v.clone()),
        _ => Err(format!(
            "prop `{key}` expects text area focus handler value"
        )),
    }
}

fn as_text_change_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::TextChangeHandlerProp, String> {
    match value {
        PropValue::OnChange(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects change handler value")),
    }
}

// -----------------------------------------------------------------------------
// Approach-C descriptor pipeline.
//
// The legacy `rsx_to_elements_*` functions above return boxed element
// trees whose `Element.children` field (now `Vec<NodeKey>`) cannot be
// populated outside an arena. The two-phase pipeline below builds an
// arena-independent `ElementDescriptor` tree from an `RsxNode`, then
// commits it into a `NodeArena` depth-first, setting parent/children
// wiring and mirroring the child-key list onto `Element.children` so the
// `ElementTrait::children()` API stays in sync.
//
// Callers (`render_rsx`, reconcile patch paths) hold the arena and use
// `commit_descriptor_tree` / `arena_insert_child` / `arena_remove_child`
// / `arena_replace_child` to apply full subtrees or individual edits.

/// Arena-independent tree node produced during RSX → element conversion.
///
/// Holds a fully-populated element plus its child descriptors. Committed
/// into a `NodeArena` by [`commit_descriptor_tree`], which inserts nodes
/// depth-first and wires parent/child links.
///
/// Exposed for downstream test fixtures only; not part of the stable API.
#[doc(hidden)]
pub struct ElementDescriptor {
    pub element: Box<dyn ElementTrait>,
    pub children: Vec<ElementDescriptor>,
    /// Runs after the element is inserted and its `children` committed.
    /// Used by nodes that own side-channel subtrees (e.g. Image/Svg
    /// loading/error slots) which live in the arena parented to this
    /// node but stay outside its `Node.children` list until activated.
    pub post_commit: Option<Box<dyn FnOnce(&mut NodeArena, NodeKey)>>,
}

impl ElementDescriptor {
    pub fn leaf(element: Box<dyn ElementTrait>) -> Self {
        Self {
            element,
            children: Vec::new(),
            post_commit: None,
        }
    }
}

/// Commit a descriptor tree into the arena under the given parent.
///
/// Depth-first: inserts each element into a fresh slot, sets its
/// `Node.parent`, recurses for children, then records the full child-key
/// list on both `Node.children` and the parent element's
/// `Element.children` mirror (when the element is an `Element`). Returns
/// the freshly allocated root key.
///
/// Exposed for downstream test fixtures only; not part of the stable API.
#[doc(hidden)]
pub fn commit_descriptor_tree(
    arena: &mut NodeArena,
    parent: Option<NodeKey>,
    desc: ElementDescriptor,
) -> NodeKey {
    let ElementDescriptor {
        element,
        children,
        post_commit,
    } = desc;
    let key = arena.insert(Node::with_parent(element, parent));
    let mut child_keys = Vec::with_capacity(children.len());
    for child in children {
        let child_key = commit_descriptor_tree(arena, Some(key), child);
        child_keys.push(child_key);
    }
    // Source of truth: Node.children in the arena wrapper.
    arena.set_children(key, child_keys.clone());
    // Mirror into Element.children so `ElementTrait::children()` works.
    // Use a take/replace dance so `replace_children` can inspect
    // child elements in the arena to recompute the absolute-descendant
    // flag without double-borrowing the current slot.
    arena.with_element_taken(key, |element, arena_ref| {
        if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
            let _previous = el.replace_children(arena_ref, child_keys);
        } else if let Some(mirror) = element.children_mut() {
            *mirror = child_keys;
        }
        let _ = arena_ref;
    });
    if let Some(cb) = post_commit {
        cb(arena, key);
    }
    key
}

/// Walk `path` indices through `arena.children_of` to resolve the target key.
pub(crate) fn resolve_path(arena: &NodeArena, root: NodeKey, path: &[usize]) -> Option<NodeKey> {
    let mut current = root;
    for &idx in path {
        let children = arena.children_of(current);
        current = *children.get(idx)?;
    }
    Some(current)
}

/// Commit `desc` as a new child of `parent` at `index`, keeping both
/// `Node.children` and the parent element's `Element.children` mirror in
/// sync.
pub(crate) fn arena_insert_child(
    arena: &mut NodeArena,
    parent: NodeKey,
    index: usize,
    desc: ElementDescriptor,
) -> NodeKey {
    let key = commit_descriptor_tree(arena, Some(parent), desc);
    // Remove the tail-appended key that commit_descriptor_tree attached to
    // the parent's own children? commit_descriptor_tree only touches the
    // freshly-inserted root's children, not the parent. We still need to
    // insert `key` into the parent's child list at `index`.
    let mut children = arena.children_of(parent);
    let insert_at = index.min(children.len());
    children.insert(insert_at, key);
    arena.set_children(parent, children.clone());
    arena.with_element_taken(parent, |element, arena_ref| {
        if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
            let _previous = el.replace_children(arena_ref, children);
        }
    });
    key
}

/// Remove the child of `parent` at `index` (and its whole subtree),
/// keeping both child-list stores in sync.
pub(crate) fn arena_remove_child(arena: &mut NodeArena, parent: NodeKey, index: usize) {
    let mut children = arena.children_of(parent);
    if index >= children.len() {
        return;
    }
    let child_key = children.remove(index);
    arena.set_children(parent, children.clone());
    arena.with_element_taken(parent, |element, arena_ref| {
        if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
            let _previous = el.replace_children(arena_ref, children);
        }
    });
    arena.remove_subtree(child_key);
}

// --- RSX → ElementDescriptor conversion -------------------------------------

/// Top-level: convert an `RsxNode` tree into a list of root descriptors
/// (fragments flattened). Inherited text style derived from the provided
/// viewport style.
///
/// Exposed for downstream test fixtures only; not part of the stable API.
#[doc(hidden)]
pub fn rsx_to_descriptors_with_context(
    root: &RsxNode,
    viewport_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> (Vec<ElementDescriptor>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut path = Vec::new();
    let inherited =
        InheritedTextStyle::from_viewport_style(viewport_style, viewport_width, viewport_height);
    let global_path = current_global_node_path(root, None);
    append_nodes_with_path_desc(
        root,
        &mut out,
        &mut path,
        global_path,
        &inherited,
        &mut errors,
    );
    (out, errors)
}

/// Like `rsx_to_descriptors_with_context` but seeded with an explicit
/// path scope + inherited style (not a viewport style). Used by
/// TextArea projection rebuilding to emit descriptor trees that
/// commit into an existing arena under the owning TextArea.
pub(crate) fn rsx_to_descriptors_scoped_with_context(
    root: &RsxNode,
    scope: &[u64],
    inherited_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<Vec<ElementDescriptor>, String> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut path: Vec<u64> = scope.to_vec();
    let inherited =
        InheritedTextStyle::from_viewport_style(inherited_style, viewport_width, viewport_height);
    let global_path = current_global_node_path(root, None);
    append_nodes_with_path_desc(
        root,
        &mut out,
        &mut path,
        global_path,
        &inherited,
        &mut errors,
    );
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(out)
}

/// M6 cascade: build descriptors for `root` using an already-computed
/// `InheritedTextStyle`. Sibling of
/// `rsx_to_descriptors_scoped_with_context` for callers that don't
/// start from the viewport root — the incremental-commit path in
/// `fiber_work` uses this after reconstructing the cascade at the
/// arena parent of a newly-authored child.
pub(crate) fn rsx_to_descriptors_with_inherited(
    root: &RsxNode,
    scope: &[u64],
    inherited: &InheritedTextStyle,
) -> Result<Vec<ElementDescriptor>, String> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut path: Vec<u64> = scope.to_vec();
    let global_path = current_global_node_path(root, None);
    append_nodes_with_path_desc(
        root,
        &mut out,
        &mut path,
        global_path,
        inherited,
        &mut errors,
    );
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(out)
}

/// M6 cascade: rebuild the `InheritedTextStyle` that the cold-path
/// converter would see at `parent_key`. Walks the arena parent chain
/// root→parent and replays each Element ancestor's `parsed_style`
/// through `InheritedTextStyle::merge_style`, matching exactly what
/// `build_container_element_shell` does during cold convert.
///
/// Non-Element ancestors (Text, TextArea, user hosts) contribute no
/// cascading style — the cold path treats them as leaves in the
/// cascade accumulation loop — so they're skipped here.
pub(crate) fn inherited_text_style_at_parent(
    arena: &NodeArena,
    parent_key: NodeKey,
    viewport_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> InheritedTextStyle {
    // Collect ancestor chain parent→root, then reverse to walk root→parent.
    let mut chain: Vec<NodeKey> = Vec::new();
    let mut cursor = Some(parent_key);
    while let Some(k) = cursor {
        chain.push(k);
        cursor = arena.get(k).and_then(|node| node.parent);
    }
    chain.reverse();

    let mut inherited =
        InheritedTextStyle::from_viewport_style(viewport_style, viewport_width, viewport_height);
    for key in chain {
        let Some(node) = arena.get(key) else { continue };
        if let Some(el) = node
            .element
            .as_any()
            .downcast_ref::<Element>()
        {
            inherited.merge_style(el.parsed_style());
        }
    }
    inherited
}

fn append_nodes_with_path_desc(
    node: &RsxNode,
    out: &mut Vec<ElementDescriptor>,
    path: &mut Vec<u64>,
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
    errors: &mut Vec<String>,
) {
    match node {
        RsxNode::Fragment(fragment) => {
            let current_global_path = current_global_node_path(node, global_path.as_ref());
            let mut ordinals = FxHashMap::<&'static str, usize>::default();
            for child in &fragment.children {
                let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
                let token = child_identity_token(child, ordinal);
                path.push(token);
                let child_global_path =
                    child_global_node_path(current_global_path.as_ref(), child, token);
                append_nodes_with_path_desc(
                    child,
                    out,
                    path,
                    child_global_path,
                    inherited_text_style,
                    errors,
                );
                path.pop();
            }
        }
        _ => {
            let current_global_path = current_global_node_path(node, global_path.as_ref());
            match convert_node_desc(node, path, current_global_path, inherited_text_style) {
                Ok(desc) => out.push(desc),
                Err(err) => errors.push(format!("node_path={path:?}: {err}")),
            }
        }
    }
}

fn convert_node_desc(
    node: &RsxNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<ElementDescriptor, String> {
    match node {
        RsxNode::Component(_) => {
            Err("Component node must be unwrapped before conversion".to_string())
        }
        RsxNode::Provider(_) => {
            Err("Provider node must be unwrapped before conversion".to_string())
        }
        RsxNode::Text(text) => Ok(ElementDescriptor::leaf(convert_text_leaf(
            text,
            path,
            global_path.as_ref(),
            inherited_text_style,
        ))),
        RsxNode::Fragment(_) => Err("fragment must be flattened before conversion".to_string()),
        RsxNode::Element(el) => {
            if is_builtin_image_node(el) {
                return convert_image_element_desc(el, path, global_path, inherited_text_style);
            }
            if is_builtin_svg_node(el) {
                return convert_svg_element_desc(el, path, global_path, inherited_text_style);
            }
            if is_builtin_text_area_node(el) {
                return convert_text_area_element_desc(el, path, global_path, inherited_text_style);
            }
            if is_builtin_text_area_projection_segment_node(el) {
                return convert_text_area_projection_segment_element_desc(
                    el,
                    path,
                    global_path,
                    inherited_text_style,
                );
            }
            if is_builtin_text_node(el) {
                return convert_text_element(el, path, global_path, inherited_text_style)
                    .map(ElementDescriptor::leaf);
            }
            if let Some(element) = invoke_host_factory(el, path)? {
                return Ok(ElementDescriptor::leaf(element));
            }
            convert_container_element_desc(el, path, global_path, inherited_text_style)
        }
    }
}

fn convert_container_element_desc(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<ElementDescriptor, String> {
    let (element, child_inherited_text_style) =
        build_container_element_shell(node, path, global_path.as_ref(), inherited_text_style)?;
    let element_box: Box<dyn ElementTrait> = Box::new(element);

    let mut children: Vec<ElementDescriptor> = Vec::new();
    let mut child_path = Vec::with_capacity(path.len().saturating_add(1));
    child_path.extend_from_slice(path);
    let current_global_path = current_global_node_path(
        &RsxNode::Element(std::rc::Rc::new(node.clone())),
        global_path.as_ref(),
    );
    let mut ordinals = FxHashMap::<&'static str, usize>::default();
    for child in &node.children {
        let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
        let token = child_identity_token(child, ordinal);
        child_path.push(token);
        let child_global_path = child_global_node_path(current_global_path.as_ref(), child, token);
        append_child_nodes_flattening_fragments_desc(
            child,
            &child_path,
            child_global_path,
            &child_inherited_text_style,
            &mut children,
        )?;
        child_path.pop();
    }

    Ok(ElementDescriptor {
        element: element_box,
        children,
        post_commit: None,
    })
}

fn append_child_nodes_flattening_fragments_desc(
    node: &RsxNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
    out: &mut Vec<ElementDescriptor>,
) -> Result<(), String> {
    match node {
        RsxNode::Fragment(fragment) => {
            let mut child_path = Vec::with_capacity(path.len().saturating_add(1));
            child_path.extend_from_slice(path);
            let current_global_path = current_global_node_path(node, global_path.as_ref());
            let mut ordinals = FxHashMap::<&'static str, usize>::default();
            for child in &fragment.children {
                let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
                let token = child_identity_token(child, ordinal);
                child_path.push(token);
                let child_global_path =
                    child_global_node_path(current_global_path.as_ref(), child, token);
                append_child_nodes_flattening_fragments_desc(
                    child,
                    &child_path,
                    child_global_path,
                    inherited_text_style,
                    out,
                )?;
                child_path.pop();
            }
            Ok(())
        }
        _ => {
            out.push(convert_node_desc(
                node,
                path,
                global_path,
                inherited_text_style,
            )?);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{identity_token_from_node_identity, rendered_node_id};
    use crate::ui::{GlobalKey, RsxKey, RsxNode, RsxNodeIdentity, RsxTagDescriptor, rsx};
    use crate::view::base_component::{Text, TextArea, get_cursor_by_id, hit_test};
    use crate::view::base_component::text_area::TextAreaTextRun;
    use crate::view::test_support::{commit_rsx_tree, measure_and_place};
    use crate::view::{
        Element as HostElement, ElementStylePropSchema, Text as HostText,
        TextArea as HostTextArea, TextStylePropSchema,
    };
    use crate::style::{
        Border, BorderRadius, Color, ColorLike, Cursor, FontSize, IntoColor, Layout, Length,
        ParsedValue, PropertyId, Style, Unit,
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
        let global =
            RsxNodeIdentity::new("Button", Some(RsxKey::Global(GlobalKey::from("item-a"))));
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

    fn style_bg_border(
        bg_hex: &str,
        border_hex: &str,
        border_width: f32,
    ) -> ElementStylePropSchema {
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
        let root_cursor =
            get_cursor_by_id(&arena, root, root_stable_id).expect("root cursor exists");
        assert_eq!(root_cursor, Cursor::Pointer);

        let run = *arena
            .children_of(root)
            .first()
            .expect("TextArea should spawn a generated run");
        let run_stable_id = arena.get(run).unwrap().element.stable_id();
        let run_cursor =
            get_cursor_by_id(&arena, root, run_stable_id).expect("run cursor exists");
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
                            host_element_node().with_child(
                                host_text_node().with_child(RsxNode::text("/v1/users/")),
                            )
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
                            host_element_node().with_child(
                                host_text_node().with_child(RsxNode::text("{{API_HOST}}")),
                            )
                        });
                        render.range(22..33, |_text_area_node| {
                            host_element_node().with_child(
                                host_text_node().with_child(RsxNode::text("{{USER_ID}}")),
                            )
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
        assert!(is_placeholder, "placeholder Run must carry is_placeholder=true");
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
}
