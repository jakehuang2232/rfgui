#![allow(missing_docs)]

//! Adapters that convert RSX trees into low-level retained host elements.
use rustc_hash::{FxHashMap, FxHashSet};

use crate::style::style_props::{StylePropTrait, property_is_inherited, validate_style};
use crate::style::{Color, Cursor, Length, ParsedValue, Position, PropertyId, TextWrap};
use crate::style::{ComputedStyle, Style, StyleComputeContext, compute_style_with_context};
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
    element_runtime_name, next_identity_ordinal, stable_node_id_from_parts,
};

#[derive(Clone, Debug)]
pub struct StyleCascadeContext {
    pub(crate) parent: ComputedStyle,
    active_inherited_properties: FxHashSet<PropertyId>,
    pub(crate) root_font_size: f32,
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
}

impl Default for StyleCascadeContext {
    fn default() -> Self {
        let mut active = FxHashSet::default();
        active.insert(PropertyId::FontSize);
        Self {
            parent: ComputedStyle::default(),
            active_inherited_properties: active,
            root_font_size: 16.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
        }
    }
}

impl StyleCascadeContext {
    pub(crate) fn from_viewport_style(
        style: &Style,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        let parent = compute_style_with_context(
            style,
            StyleComputeContext {
                parent: None,
                viewport_width,
                viewport_height,
                root_font_size: 16.0,
                hovered: false,
            },
        );
        let root_font_size = parent.font_size;
        let mut active = active_inherited_properties(style);
        active.insert(PropertyId::FontSize);
        Self {
            parent,
            active_inherited_properties: active,
            root_font_size,
            viewport_width,
            viewport_height,
        }
    }

    /// Mutate this runtime cascade with declarations authored in an
    /// ancestor style. The registry decides which authored properties
    /// become active inherited values; `compute_style_with_context`
    /// produces the next computed parent snapshot.
    pub(crate) fn merge_style(&mut self, style: &Style) {
        self.parent = compute_style_with_context(
            style,
            StyleComputeContext {
                parent: Some(&self.parent),
                viewport_width: self.viewport_width,
                viewport_height: self.viewport_height,
                root_font_size: self.root_font_size,
                hovered: false,
            },
        );
        self.active_inherited_properties
            .extend(active_inherited_properties(style));
    }

    pub(crate) fn has_inherited(&self, property: PropertyId) -> bool {
        self.active_inherited_properties.contains(&property)
    }

    pub(crate) fn inherited_font_families(&self) -> Option<&[String]> {
        self.has_inherited(PropertyId::FontFamily)
            .then_some(self.parent.font_families.as_slice())
    }

    pub(crate) fn inherited_font_size(&self) -> Option<f32> {
        self.has_inherited(PropertyId::FontSize)
            .then_some(self.parent.font_size)
    }

    pub(crate) fn inherited_font_weight(&self) -> Option<u16> {
        self.has_inherited(PropertyId::FontWeight)
            .then_some(self.parent.font_weight)
    }

    pub(crate) fn inherited_color(&self) -> Option<Color> {
        self.has_inherited(PropertyId::Color)
            .then_some(self.parent.color)
    }

    pub(crate) fn inherited_cursor(&self) -> Option<Cursor> {
        self.has_inherited(PropertyId::Cursor)
            .then_some(self.parent.cursor)
    }

    pub(crate) fn inherited_text_wrap(&self) -> Option<TextWrap> {
        self.has_inherited(PropertyId::TextWrap)
            .then_some(self.parent.text_wrap)
    }

    pub(crate) fn inherited_line_height(&self) -> Option<f32> {
        self.has_inherited(PropertyId::LineHeight)
            .then_some(self.parent.line_height)
    }

    pub(crate) fn inherited_vertical_align(&self) -> Option<crate::style::VerticalAlign> {
        self.has_inherited(PropertyId::VerticalAlign)
            .then_some(self.parent.vertical_align)
    }
}

fn active_inherited_properties(style: &Style) -> FxHashSet<PropertyId> {
    let mut active = FxHashSet::default();
    for declaration in style.declarations() {
        if property_is_inherited(declaration.property) {
            active.insert(declaration.property);
        }
    }
    active
}

pub(crate) fn computed_parent_from_style_cascade(cascade: &StyleCascadeContext) -> ComputedStyle {
    cascade.parent.clone()
}

/// Resolve an explicit `font_size` prop using the same inherited text
/// context as the computed-style bridge parent. This is for the
/// standalone `font_size` prop only; local `style.font_size` flows
/// through `compute_style_with_context`.
pub(crate) fn resolve_font_size_prop_with_inherited(
    value: &PropValue,
    cascade: &StyleCascadeContext,
) -> Option<f32> {
    let parent_font_size = cascade.parent.font_size;
    match value {
        PropValue::I64(v) => Some((*v as f32).max(0.0)),
        PropValue::F64(v) => Some((*v as f32).max(0.0)),
        PropValue::FontSize(fs) => Some(fs.resolve_px(
            parent_font_size,
            cascade.root_font_size,
            cascade.viewport_width,
            cascade.viewport_height,
        )),
        _ => None,
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
    style_cascade: &StyleCascadeContext,
) -> Box<dyn ElementTrait> {
    let content = text_with_text_area_ime_preedit(text.content.clone());
    let mut text_node = Text::from_content_with_id(
        stable_node_id_from_parts("TextNode", path, global_path),
        content,
    );
    text_node.apply_inherited(style_cascade);
    Box::new(text_node)
}

pub(crate) fn element_base_style_from_inherited(cascade: &StyleCascadeContext) -> Style {
    let mut base_style = Style::new();
    base_style.insert(PropertyId::Width, ParsedValue::Auto);
    base_style.insert(PropertyId::Height, ParsedValue::Auto);
    if let Some(cursor) = cascade.inherited_cursor() {
        base_style.insert(PropertyId::Cursor, ParsedValue::Cursor(cursor));
    }
    if let Some(line_height) = cascade.inherited_line_height() {
        base_style.insert(
            PropertyId::LineHeight,
            ParsedValue::LineHeight(crate::style::LineHeight::new(line_height)),
        );
    }
    if let Some(vertical_align) = cascade.inherited_vertical_align() {
        base_style.insert(
            PropertyId::VerticalAlign,
            ParsedValue::VerticalAlign(vertical_align),
        );
    }
    if let Some(text_wrap) = cascade.inherited_text_wrap() {
        base_style.insert(PropertyId::TextWrap, ParsedValue::TextWrap(text_wrap));
    }
    base_style
}

/// Build the container Element plus child-inherited text style without
/// walking children. Used by the descriptor path
/// ([`convert_container_element_desc`]).
fn build_container_element_shell(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<&GlobalNodePath>,
    style_cascade: &StyleCascadeContext,
) -> Result<(Element, StyleCascadeContext), String> {
    let initial_size = if path.is_empty() { 10_000.0 } else { 0.0 };
    let mut element = Element::new_with_id(
        stable_node_id_from_parts(element_runtime_name(node), path, global_path),
        0.0,
        0.0,
        initial_size,
        initial_size,
    );
    element.set_intrinsic_size_as_percent_base(false);
    let base_style = element_base_style_from_inherited(style_cascade);

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
        base_style + user_style.clone()
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
    element.set_text_cascade_style(user_style);
    // Phase 3: child cascade goes through the host. Element merges
    // its user-authored text cascade style onto `parent`. The layered
    // base style may include inherited text props needed for this
    // Element's own computed style, but replaying that base during a
    // later incremental recascade would resurrect stale inherited
    // values.
    let child_style_cascade = element.child_style_cascade(style_cascade);

    Ok((element, child_style_cascade))
}

pub(crate) fn convert_text_element(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    style_cascade: &StyleCascadeContext,
) -> Result<Box<dyn ElementTrait>, String> {
    let mut text_content = String::new();
    let mut text = Text::from_content_with_id(
        stable_node_id_from_parts("Text", path, global_path.as_ref()),
        "",
    );
    let mut style: Option<Style> = None;

    // Cold-path-owned props: local style is decoded once and then
    // applied by `TextComputedStyleBridge`; standalone `font_size`
    // keeps its existing explicit-prop priority. Everything else flows
    // through `Text::ingest_props`.
    for (key, value) in node.props.iter() {
        match *key {
            "style" => style = Some(as_text_style(value, key)?),
            "font_size" => {
                let Some(px) = resolve_font_size_prop_with_inherited(value, style_cascade) else {
                    return Err(format!("prop `{key}` expects numeric or FontSize value"));
                };
                text.set_font_size(px);
            }
            _ => {}
        }
    }
    text.ingest_props(node)?;

    text.apply_style_cold(style.as_ref(), style_cascade)?;

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
    style_cascade: &StyleCascadeContext,
) -> Result<ElementDescriptor, String> {
    let stable_id = stable_node_id_from_parts("TextArea", path, global_path.as_ref());
    let mut text_area = TextArea::with_stable_id(stable_id);
    let mut style: Option<Style> = None;
    let mut explicit_font_size: Option<f32> = None;
    let mut explicit_font: Option<String> = None;

    // Cold-path-owned props: local style is decoded once and then
    // applied by `TextAreaComputedStyleBridge`; explicit font/font_size
    // props keep their existing priority. The rest of the schema flows
    // through `TextArea::ingest_props`.
    for (key, value) in node.props.iter() {
        match *key {
            "style" => style = Some(as_element_style(value, key)?),
            "font_size" => {
                let Some(px) = resolve_font_size_prop_with_inherited(value, style_cascade) else {
                    return Err(format!("prop `{key}` expects numeric or FontSize value"));
                };
                explicit_font_size = Some(px);
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

    text_area.apply_style_cold(
        style.as_ref(),
        style_cascade,
        explicit_font_size,
        explicit_font,
    );

    let child_descriptors =
        text_area.build_children(node, path, global_path.as_ref(), style_cascade)?;

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
    style_cascade: &StyleCascadeContext,
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
        segment.build_children(node, path, global_path.as_ref(), style_cascade)?;

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
    style_cascade: &StyleCascadeContext,
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
        style_cascade,
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
    style_cascade: &StyleCascadeContext,
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
                    style_cascade,
                    "loading",
                )?;
            }
            "error" => {
                error_descs = convert_image_slot_desc(
                    value,
                    path,
                    global_path.clone(),
                    style_cascade,
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

    let children = image.build_children(node, path, global_path.as_ref(), style_cascade)?;

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
    style_cascade: &StyleCascadeContext,
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
                    style_cascade,
                    "loading",
                )?;
            }
            "error" => {
                error_descs = convert_image_slot_desc(
                    value,
                    path,
                    global_path.clone(),
                    style_cascade,
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

    let children = svg.build_children(node, path, global_path.as_ref(), style_cascade)?;

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
    as_binding_string, as_bool, as_f32, as_owned_string, as_string, as_text_align, as_usize,
};

pub(crate) fn as_element_style(value: &PropValue, key: &str) -> Result<Style, String> {
    style_from_prop_value::<ElementStylePropSchema>(value, key, "ElementStylePropSchema")
}

pub(crate) fn as_text_style(value: &PropValue, key: &str) -> Result<Style, String> {
    style_from_prop_value::<TextStylePropSchema>(value, key, "TextStylePropSchema")
}

fn style_from_prop_value<P>(value: &PropValue, key: &str, expected: &str) -> Result<Style, String>
where
    P: FromPropValue + StylePropTrait,
{
    let prop = P::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects {expected} value"))?;
    let style = prop.to_style();
    validate_style::<P::Accepted>(&style).map_err(|err| format!("prop `{key}` contains {err}"))?;
    Ok(style)
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
        } else {
            element.sync_children_mirror(&child_keys);
        }
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
    arena.mutate_element_with_invalidation(parent, |element, cx| {
        if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
            let _previous = el.replace_children(cx.arena(), children);
            cx.invalidate(crate::view::base_component::DirtyFlags::ALL);
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
    arena.mutate_element_with_invalidation(parent, |element, cx| {
        if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
            let _previous = el.replace_children(cx.arena(), children);
            cx.invalidate(crate::view::base_component::DirtyFlags::ALL);
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
        StyleCascadeContext::from_viewport_style(viewport_style, viewport_width, viewport_height);
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
        StyleCascadeContext::from_viewport_style(inherited_style, viewport_width, viewport_height);
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
/// `StyleCascadeContext`. Sibling of
/// `rsx_to_descriptors_scoped_with_context` for callers that don't
/// start from the viewport root — the incremental-commit path in
/// `fiber_work` uses this after reconstructing the cascade at the
/// arena parent of a newly-authored child.
pub(crate) fn rsx_to_descriptors_with_inherited(
    root: &RsxNode,
    scope: &[u64],
    inherited: &StyleCascadeContext,
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

/// M6 cascade: rebuild the `StyleCascadeContext` that the cold-path
/// converter would see at `parent_key`. Walks the arena parent chain
/// root→parent and replays each Element ancestor's user-authored text
/// cascade style through `StyleCascadeContext::merge_style`, matching
/// exactly what `build_container_element_shell` does during cold
/// convert.
///
/// Non-Element ancestors (Text, TextArea, user hosts) contribute no
/// cascading style — the cold path treats them as leaves in the
/// cascade accumulation loop — so they're skipped here.
pub(crate) fn style_cascade_at_parent(
    arena: &NodeArena,
    parent_key: NodeKey,
    viewport_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> StyleCascadeContext {
    // Collect ancestor chain parent→root, then reverse to walk root→parent.
    let mut chain: Vec<NodeKey> = Vec::new();
    let mut cursor = Some(parent_key);
    while let Some(k) = cursor {
        chain.push(k);
        cursor = arena.get(k).and_then(|node| node.parent);
    }
    chain.reverse();

    let mut inherited =
        StyleCascadeContext::from_viewport_style(viewport_style, viewport_width, viewport_height);
    for key in chain {
        let Some(node) = arena.get(key) else { continue };
        if let Some(el) = node.element.as_any().downcast_ref::<Element>() {
            inherited.merge_style(el.text_cascade_style());
        }
    }
    inherited
}

fn append_nodes_with_path_desc(
    node: &RsxNode,
    out: &mut Vec<ElementDescriptor>,
    path: &mut Vec<u64>,
    global_path: Option<GlobalNodePath>,
    style_cascade: &StyleCascadeContext,
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
                    style_cascade,
                    errors,
                );
                path.pop();
            }
        }
        _ => {
            let current_global_path = current_global_node_path(node, global_path.as_ref());
            match convert_node_desc(node, path, current_global_path, style_cascade) {
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
    style_cascade: &StyleCascadeContext,
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
            style_cascade,
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
                inherited: style_cascade.clone(),
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
    style_cascade: &StyleCascadeContext,
) -> Result<ElementDescriptor, String> {
    let (element, child_style_cascade) =
        build_container_element_shell(node, path, global_path.as_ref(), style_cascade)?;
    let children =
        element.build_children(node, path, global_path.as_ref(), &child_style_cascade)?;
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
    style_cascade: &StyleCascadeContext,
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
            style_cascade,
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
    style_cascade: &StyleCascadeContext,
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
                    style_cascade,
                    out,
                )?;
                child_path.pop();
            }
            Ok(())
        }
        _ => {
            out.push(convert_node_desc(node, path, global_path, style_cascade)?);
            Ok(())
        }
    }
}
