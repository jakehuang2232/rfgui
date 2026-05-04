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
    /// Multiplier-style line height (e.g. 1.2 = 120% of font_size).
    /// Inherited typography prop. Default is the `Text` field's own
    /// initial value when unset (cascade is opt-in).
    pub(crate) line_height: Option<f32>,
    /// Cross-axis alignment cascaded from an Element ancestor's
    /// `style` prop. See `docs/design/inline-baseline.md` D5/D5a.
    pub(crate) vertical_align: Option<crate::style::VerticalAlign>,
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
    pub(crate) line_height: Option<f32>,
    pub(crate) vertical_align: Option<crate::style::VerticalAlign>,
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
            line_height: None,
            vertical_align: None,
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
        inherited.line_height = resolved.line_height.or(inherited.line_height);
        inherited.vertical_align = resolved.vertical_align.or(inherited.vertical_align);
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
        if let Some(line_height) = resolved.line_height {
            self.line_height = Some(line_height);
        }
        if let Some(vertical_align) = resolved.vertical_align {
            self.vertical_align = Some(vertical_align);
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
            line_height: match style.get(PropertyId::LineHeight) {
                // Multiplier value clamped to >= 0 to mirror compute_style.
                Some(ParsedValue::LineHeight(lh)) => Some(lh.value().max(0.0)),
                _ => None,
            },
            vertical_align: match style.get(PropertyId::VerticalAlign) {
                Some(ParsedValue::VerticalAlign(va)) => Some(*va),
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

pub(crate) fn convert_text_element(
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
pub(crate) fn convert_text_area_element_desc(
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
pub(crate) fn convert_text_area_projection_segment_element_desc(
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

pub(crate) fn convert_image_element_desc(
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

pub(crate) fn convert_svg_element_desc(
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
            // Phase 6b: host-owned dispatch. Each tag's
            // `host_builder` fn pointer (carried by the descriptor)
            // builds the full ElementDescriptor — children, side
            // slots, the lot. Adapter no longer enumerates host types.
            let descriptor = el
                .tag_descriptor
                .ok_or_else(|| format!("element `{}` missing tag descriptor", el.tag))?;
            let builder = descriptor
                .host_builder
                .ok_or_else(|| format!("tag `{}` missing host_builder", descriptor.type_name))?;
            let ctx = crate::view::host_element::BuildCtx {
                global_path,
                inherited: inherited_text_style.clone(),
            };
            let any = builder(el, path, &ctx as &dyn std::any::Any)?;
            let boxed: Box<crate::view::host_element::HostElementDescBox> =
                any.downcast().map_err(|_| {
                    format!(
                        "host builder for `{}` returned wrong type",
                        descriptor.type_name
                    )
                })?;
            Ok(boxed.0)
        }
    }
}

pub(crate) fn convert_container_element_desc(
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
