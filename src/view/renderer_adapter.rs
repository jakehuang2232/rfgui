#![allow(missing_docs)]

//! Adapters that convert RSX trees into low-level retained host elements.
use rustc_hash::FxHashMap;

use crate::style::Style;
use crate::style::{Color, Cursor, Length, ParsedValue, Position, PropertyId, TextWrap};
use crate::ui::{FromPropValue, PropValue, RsxElementNode, RsxNode, RsxTextNode, use_context};
use crate::view::base_component::text_area::TextAreaProjectionSegment;
use crate::view::base_component::{
    Element, ElementTrait, Image, Svg, Text, TextArea, TextAreaImeContext,
};
use crate::view::node_arena::{Node, NodeArena, NodeKey};
use crate::view::{ElementStylePropSchema, ImageSource, SvgSource, TextStylePropSchema};
// 軌 1 #14 Phase 7: identity / global-path / stable-id machinery
// moved to `ui::rsx_tree`. Re-exported here so existing
// `renderer_adapter::*` paths keep working.
pub(crate) use crate::ui::{
    GlobalNodePath, child_global_node_path, child_identity_token, current_global_node_path,
    element_runtime_name, next_identity_ordinal, rendered_node_id_by_index_path,
    stable_node_id_from_parts,
};

#[derive(Clone, Debug, Default)]
pub struct InheritedTextStyle {
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

/// Resolved text-cascading declarations extracted from a `Style`.
///
/// 軌 1 #14 Phase 3: single enumeration of the cascade's prop list
/// (font_family / font_size / font_weight / color / cursor / text_wrap).
/// Callers that need to write these into a host (Text setters,
/// TextArea fields, `InheritedTextStyle` cascade) read from the same
/// `ResolvedTextProps` instead of repeating per-PropertyId match arms.
#[derive(Clone, Debug, Default)]
pub(crate) struct ResolvedTextProps {
    pub(crate) font_families: Option<Vec<String>>,
    pub(crate) font_size: Option<f32>,
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
        let resolved = inherited.resolved_text_props(style);
        if let Some(font_families) = resolved.font_families {
            inherited.font_families = font_families;
        }
        // `from_viewport_style` keeps the explicit root_font_size
        // already computed above — don't downgrade it to whatever
        // `resolve_font_size_from_style` returns mid-cascade.
        inherited.font_weight = resolved.font_weight.or(inherited.font_weight);
        inherited.color = resolved.color.or(inherited.color);
        inherited.cursor = resolved.cursor.or(inherited.cursor);
        inherited.text_wrap = resolved.text_wrap.or(inherited.text_wrap);
        inherited
    }

    /// Mutate `self` with the text-cascading declarations authored in
    /// an Element's `style` prop. Mirrors the merge previously inlined
    /// in `build_container_element_shell`; pulled out so the
    /// incremental-commit path can replay the same cascade walking
    /// the arena parent chain (M6).
    pub(crate) fn merge_style(&mut self, style: &Style) {
        let resolved = self.resolved_text_props(style);
        if let Some(font_families) = resolved.font_families {
            self.font_families = font_families;
        }
        if let Some(font_size) = resolved.font_size {
            self.font_size = Some(font_size);
        }
        if let Some(font_weight) = resolved.font_weight {
            self.font_weight = Some(font_weight);
        }
        if let Some(color) = resolved.color {
            self.color = Some(color);
        }
        if let Some(cursor) = resolved.cursor {
            self.cursor = Some(cursor);
        }
        if let Some(text_wrap) = resolved.text_wrap {
            self.text_wrap = Some(text_wrap);
        }
    }

    /// Single source of truth for "what does this `Style` cascade?".
    /// Resolves font_size relative to `self`'s font_size / root /
    /// viewport. Other props are direct extractions.
    pub(crate) fn resolved_text_props(&self, style: &Style) -> ResolvedTextProps {
        ResolvedTextProps {
            font_families: match style.get(PropertyId::FontFamily) {
                Some(ParsedValue::FontFamily(family)) => Some(family.as_slice().to_vec()),
                _ => None,
            },
            font_size: resolve_font_size_from_style(
                style,
                self.font_size.unwrap_or(self.root_font_size),
                self.root_font_size,
                self.viewport_width,
                self.viewport_height,
            ),
            font_weight: match style.get(PropertyId::FontWeight) {
                Some(ParsedValue::FontWeight(fw)) => Some(fw.value()),
                _ => None,
            },
            color: match style.get(PropertyId::Color) {
                Some(ParsedValue::Color(color)) => Some(color.to_color()),
                _ => None,
            },
            cursor: match style.get(PropertyId::Cursor) {
                Some(ParsedValue::Cursor(cursor)) => Some(*cursor),
                _ => None,
            },
            text_wrap: match style.get(PropertyId::TextWrap) {
                Some(ParsedValue::TextWrap(text_wrap)) => Some(*text_wrap),
                _ => None,
            },
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
    let host_box: Box<crate::view::host_element::HostElementBox> =
        any.downcast().map_err(|_| {
            format!(
                "host factory for `{}` returned wrong type",
                descriptor.type_name
            )
        })?;
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

    let mut user_style = Style::new();
    let mut has_user_style = false;
    for (key, value) in node.props.iter() {
        if *key == "style" {
            let style = as_element_style(value, key)?;
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
    // 軌 1 #14 Phase 1: cold-path prop dispatch lives on the host
    // (`Element::ingest_props`), the same code path the incremental
    // commit reaches via `apply_prop`. The shell here only owns
    // identity, layered style, and child cascade — anchor / padding /
    // opacity / event handlers all flow through the host.
    element.ingest_props(node)?;
    // Phase 3: child cascade goes through the host. Element merges
    // its `parsed_style()` (which now includes the layered base+user)
    // onto `parent`. Equivalent to the previous inline merge of
    // user_style alone — base_style only adds {Width:Auto,
    // Height:Auto, Cursor:inherited.cursor}, none of which alter the
    // text cascade beyond what's already inherited.
    let child_inherited_text_style = element.child_inherited_text_style(inherited_text_style);

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

    // Cold-path-owned props: style (extracted for downstream
    // width/height/font_weight/color/cursor/text_wrap fan-out) and
    // font_size (cascade-resolved). Everything else flows through
    // `Text::ingest_props`.
    for (key, value) in node.props.iter() {
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
            _ => {}
        }
    }
    text.ingest_props(node)?;

    if let Some(style) = &style {
        if let Some(value) = style.get(PropertyId::Width) {
            width = length_from_parsed_value(value, "Text style.width")?;
        }
        if let Some(value) = style.get(PropertyId::Height) {
            height = length_from_parsed_value(value, "Text style.height")?;
        }
        // Phase 3: shared cascade-prop resolver. Text doesn't read
        // `font_family` from style (cascade-only), so font_families
        // is intentionally ignored.
        let resolved = inherited_text_style.resolved_text_props(style);
        if let Some(font_size) = resolved.font_size {
            text.set_font_size(font_size);
        }
        if let Some(font_weight) = resolved.font_weight {
            text.set_font_weight(font_weight);
        }
        if let Some(color) = resolved.color {
            text.set_color(color);
        }
        if let Some(cursor) = resolved.cursor {
            text.set_cursor(cursor);
        }
        if let Some(text_wrap) = resolved.text_wrap {
            text.set_text_wrap(text_wrap);
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

    // Cold-path-owned props: layered style + explicit font-priority
    // (font, font_size with cascade resolution). The rest of the
    // schema flows through `TextArea::ingest_props`.
    for (key, value) in node.props.iter() {
        match *key {
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
            _ => {}
        }
    }
    text_area.ingest_props(node)?;

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

    // Phase 3: text-side declarations from `style` go through the
    // shared cascade-prop resolver. TextArea only reads font / size /
    // weight / color / cursor (no text_wrap).
    if let Some(style) = &style {
        let resolved = inherited_text_style.resolved_text_props(style);
        if let Some(color) = resolved.color {
            text_area.color = color;
        }
        if let Some(cursor) = resolved.cursor {
            text_area.cursor = cursor;
        }
        if let Some(font_size) = resolved.font_size {
            text_area.font_size = font_size;
        }
        if let Some(font_families) = resolved.font_families {
            text_area.font_families = font_families;
        }
        if let Some(font_weight) = resolved.font_weight {
            text_area.font_weight = font_weight;
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

    let child_descriptors =
        text_area.build_children(node, path, global_path.as_ref(), inherited_text_style)?;

    Ok(ElementDescriptor {
        element: Box::new(text_area) as Box<dyn ElementTrait>,
        children: child_descriptors,
        side_slots: Vec::new(),
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
    let stable_id =
        stable_node_id_from_parts("TextAreaProjectionSegment", path, global_path.as_ref());
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
            _ => {
                return Err(format!(
                    "unknown prop `{}` on <TextAreaProjectionSegment>",
                    key
                ));
            }
        }
    }
    if range_end < range_start {
        range_end = range_start;
    }
    segment.set_char_range(range_start..range_end);

    let child_descriptors =
        segment.build_children(node, path, global_path.as_ref(), inherited_text_style)?;

    Ok(ElementDescriptor {
        element: Box::new(segment) as Box<dyn ElementTrait>,
        children: child_descriptors,
        side_slots: Vec::new(),
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
    slot_path.push(stable_node_id_from_parts(slot_name, path, None));
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
        side_slots: Vec::new(),
    }])
}

fn convert_image_element_desc(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<ElementDescriptor, String> {
    let mut source: Option<ImageSource> = None;
    let mut style: Option<Style> = None;
    let mut loading_descs: Vec<ElementDescriptor> = Vec::new();
    let mut error_descs: Vec<ElementDescriptor> = Vec::new();

    // Cold-path-owned props: required `source` constructor arg,
    // layered style, and slot subtrees (need cold path / global_path).
    for (key, value) in node.props.iter() {
        match *key {
            "source" => source = Some(ImageSource::from_prop_value(value.clone())?),
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
            _ => {}
        }
    }

    let mut image = Image::new_with_id(
        stable_node_id_from_parts("Image", path, global_path.as_ref()),
        source.ok_or_else(|| "<Image> requires `source`".to_string())?,
    );
    image.ingest_props(node)?;
    if let Some(style) = style {
        image.apply_style(style);
    } else {
        image.apply_style(Style::new());
    }

    let mut side_slots: Vec<SideSlot> = Vec::new();
    if !loading_descs.is_empty() {
        side_slots.push(SideSlot {
            name: "loading",
            descriptors: loading_descs,
        });
    }
    if !error_descs.is_empty() {
        side_slots.push(SideSlot {
            name: "error",
            descriptors: error_descs,
        });
    }

    let children = image.build_children(node, path, global_path.as_ref(), inherited_text_style)?;

    Ok(ElementDescriptor {
        element: Box::new(image) as Box<dyn ElementTrait>,
        children,
        side_slots,
    })
}

fn convert_svg_element_desc(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<ElementDescriptor, String> {
    let mut source: Option<SvgSource> = None;
    let mut style: Option<Style> = None;
    let mut loading_descs: Vec<ElementDescriptor> = Vec::new();
    let mut error_descs: Vec<ElementDescriptor> = Vec::new();

    // Cold-path-owned props: required `source` constructor arg,
    // layered style, and slot subtrees (need cold path / global_path).
    for (key, value) in node.props.iter() {
        match *key {
            "source" => source = Some(SvgSource::from_prop_value(value.clone())?),
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
            _ => {}
        }
    }

    let mut svg = Svg::new_with_id(
        stable_node_id_from_parts("Svg", path, global_path.as_ref()),
        source.ok_or_else(|| "<Svg> requires `source`".to_string())?,
    );
    svg.ingest_props(node)?;
    if let Some(style) = style {
        svg.apply_style(style);
    } else {
        svg.apply_style(Style::new());
    }

    let mut side_slots: Vec<SideSlot> = Vec::new();
    if !loading_descs.is_empty() {
        side_slots.push(SideSlot {
            name: "loading",
            descriptors: loading_descs,
        });
    }
    if !error_descs.is_empty() {
        side_slots.push(SideSlot {
            name: "error",
            descriptors: error_descs,
        });
    }

    let children = svg.build_children(node, path, global_path.as_ref(), inherited_text_style)?;

    Ok(ElementDescriptor {
        element: Box::new(svg) as Box<dyn ElementTrait>,
        children,
        side_slots,
    })
}

// 軌 1 #14 Phase 7: PropValue→typed-value decoders moved to
// `ui::rsx_tree`. Re-exported here so existing
// `renderer_adapter::as_*` paths keep working.
pub(crate) use crate::ui::{
    as_binding_string, as_bool, as_f32, as_font_size_px, as_owned_string, as_string, as_text_align,
    as_usize,
};

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
    /// Side-channel subtrees: in the arena their parent is this node,
    /// but they stay outside `Node.children` so they don't enter
    /// layout / paint flow until the host activates them. Image / Svg
    /// use this for `loading` / `error` slots. After commit, the host
    /// receives the freshly allocated keys via
    /// [`ElementTrait::attach_side_slot`].
    pub side_slots: Vec<SideSlot>,
}

/// Named side-channel subtree on an [`ElementDescriptor`].
#[doc(hidden)]
pub struct SideSlot {
    pub name: &'static str,
    pub descriptors: Vec<ElementDescriptor>,
}

impl ElementDescriptor {
    pub fn leaf(element: Box<dyn ElementTrait>) -> Self {
        Self {
            element,
            children: Vec::new(),
            side_slots: Vec::new(),
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
        side_slots,
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
    // Phase 4: side-channel subtrees commit under the same parent
    // but bypass `Node.children`. Each slot's NodeKeys go back to the
    // host via `attach_side_slot`.
    for slot in side_slots {
        let SideSlot { name, descriptors } = slot;
        let slot_keys: Vec<NodeKey> = descriptors
            .into_iter()
            .map(|d| commit_descriptor_tree(arena, Some(key), d))
            .collect();
        arena.with_element_taken(key, |element, _arena_ref| {
            element.attach_side_slot(name, slot_keys);
        });
    }
    arena.with_element_taken(key, |element, arena_ref| {
        element.after_commit(arena_ref, key);
    });
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
        if let Some(el) = node.element.as_any().downcast_ref::<Element>() {
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
    use std::any::TypeId;
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
            // 軌 1 #14 Phase 6a: builtin host dispatch by TypeId. The
            // descriptor's `type_id` is either the zero-sized tag
            // marker (`view::tags::*`, stamped by the rsx! macro via
            // `for_tag::<T>()`) or the host-implementation type
            // (`view::base_component::*`, when test fixtures stamp
            // `RsxTagDescriptor::of::<HostType>()` directly). Match
            // both for each builtin.
            if let Some(descriptor) = el.tag_descriptor {
                let type_id = descriptor.type_id;
                if type_id == TypeId::of::<crate::view::tags::Image>()
                    || type_id == TypeId::of::<Image>()
                {
                    return convert_image_element_desc(el, path, global_path, inherited_text_style);
                }
                if type_id == TypeId::of::<crate::view::tags::Svg>()
                    || type_id == TypeId::of::<Svg>()
                {
                    return convert_svg_element_desc(el, path, global_path, inherited_text_style);
                }
                if type_id == TypeId::of::<crate::view::tags::TextArea>()
                    || type_id == TypeId::of::<TextArea>()
                {
                    return convert_text_area_element_desc(
                        el,
                        path,
                        global_path,
                        inherited_text_style,
                    );
                }
                if type_id == TypeId::of::<crate::view::tags::TextAreaProjectionSegment>()
                    || type_id == TypeId::of::<TextAreaProjectionSegment>()
                {
                    return convert_text_area_projection_segment_element_desc(
                        el,
                        path,
                        global_path,
                        inherited_text_style,
                    );
                }
                if type_id == TypeId::of::<crate::view::tags::Text>()
                    || type_id == TypeId::of::<Text>()
                {
                    return convert_text_element(el, path, global_path, inherited_text_style)
                        .map(ElementDescriptor::leaf);
                }
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
    let children = element.build_children(
        node,
        path,
        global_path.as_ref(),
        &child_inherited_text_style,
    )?;
    Ok(ElementDescriptor {
        element: Box::new(element) as Box<dyn ElementTrait>,
        children,
        side_slots: Vec::new(),
    })
}

/// 軌 1 #14 Phase 5: standard "walk `node.children`, flatten fragments,
/// recurse via `convert_node_desc`" loop. The default
/// `ElementTrait::build_children` calls this; hosts whose child shape
/// matches the standard pattern (Element, TextAreaProjectionSegment)
/// don't override.
pub(crate) fn walk_children_descriptors(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<&GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Vec<ElementDescriptor>, String> {
    let mut children: Vec<ElementDescriptor> = Vec::new();
    let mut child_path = Vec::with_capacity(path.len().saturating_add(1));
    child_path.extend_from_slice(path);
    let current_global_path = current_global_node_path(
        &RsxNode::Element(std::rc::Rc::new(node.clone())),
        global_path,
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
            inherited_text_style,
            &mut children,
        )?;
        child_path.pop();
    }
    Ok(children)
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
use crate::ui::{identity_token_from_node_identity, rendered_node_id};

#[cfg(test)]
mod tests {
    use super::{identity_token_from_node_identity, rendered_node_id};
    use crate::style::{
        Border, BorderRadius, Color, ColorLike, Cursor, FontSize, IntoColor, Layout, Length,
        ParsedValue, PropertyId, Style, Unit,
    };
    use crate::ui::{GlobalKey, RsxKey, RsxNode, RsxNodeIdentity, RsxTagDescriptor, rsx};
    use crate::view::base_component::text_area::TextAreaTextRun;
    use crate::view::base_component::{Text, TextArea, get_cursor_by_id, hit_test};
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
        assert!(
            is_placeholder,
            "placeholder Run must carry is_placeholder=true"
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
}
