#![allow(missing_docs)]

//! Adapters that convert RSX trees into low-level retained host elements.

use crate::Style;
use crate::ui::{
    Binding, FromPropValue, GlobalKey, Patch, PropValue, RenderBackend, RsxElementNode, RsxKey,
    RsxNode, RsxNodeIdentity, RsxTagDescriptor,
};
use crate::view::Viewport;
use crate::view::base_component::{Element, ElementTrait, Image, Svg, Text, TextArea};
use crate::view::{
    ElementStylePropSchema, ImageFit, ImageSampling, ImageSource, SvgSource, TextStylePropSchema,
};
use crate::{
    AnchorName, Color, Cursor, Layout, Length, ParsedValue, Position, PropertyId, TextWrap,
};
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

const TEXT_AREA_PROJECTION_TAG: &str = "__rfgui_text_area_projection";

pub type ElementFactory =
    Arc<dyn Fn(&RsxElementNode, &[u64]) -> Result<Box<dyn ElementTrait>, String> + Send + Sync>;

fn element_factories() -> &'static RwLock<HashMap<String, ElementFactory>> {
    static FACTORIES: OnceLock<RwLock<HashMap<String, ElementFactory>>> = OnceLock::new();
    FACTORIES.get_or_init(|| RwLock::new(HashMap::new()))
}

fn typed_element_factories() -> &'static RwLock<HashMap<TypeId, ElementFactory>> {
    static FACTORIES: OnceLock<RwLock<HashMap<TypeId, ElementFactory>>> = OnceLock::new();
    FACTORIES.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn register_element_factory(tag: impl Into<String>, factory: ElementFactory) {
    if let Ok(mut map) = element_factories().write() {
        map.insert(tag.into(), factory);
    }
}

pub fn register_tag_element_factory<T: 'static>(factory: ElementFactory) {
    if let Ok(mut map) = typed_element_factories().write() {
        map.insert(TypeId::of::<T>(), factory);
    }
}

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


pub fn rsx_to_element(root: &RsxNode) -> Result<Box<dyn ElementTrait>, String> {
    let mut nodes = rsx_to_elements(root)?;
    if nodes.len() != 1 {
        return Err("expected single root element".to_string());
    }
    Ok(nodes.remove(0))
}

pub fn rsx_to_element_scoped(
    root: &RsxNode,
    scope_path: &[u64],
) -> Result<Box<dyn ElementTrait>, String> {
    let inherited = InheritedTextStyle::from_viewport_style(&Style::new(), 0.0, 0.0);
    convert_node(root, scope_path, None, &inherited)
}

pub fn rsx_to_elements(root: &RsxNode) -> Result<Vec<Box<dyn ElementTrait>>, String> {
    rsx_to_elements_with_context(root, &Style::new(), 0.0, 0.0)
}

pub fn rsx_to_elements_with_context(
    root: &RsxNode,
    viewport_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<Vec<Box<dyn ElementTrait>>, String> {
    let mut out = Vec::new();
    append_nodes(
        root,
        &mut out,
        viewport_style,
        viewport_width,
        viewport_height,
    )?;
    Ok(out)
}

pub fn rsx_to_elements_scoped_with_context(
    root: &RsxNode,
    scope_path: &[u64],
    viewport_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<Vec<Box<dyn ElementTrait>>, String> {
    let mut out = Vec::new();
    let mut path = Vec::with_capacity(scope_path.len().saturating_add(8));
    path.extend_from_slice(scope_path);
    let inherited =
        InheritedTextStyle::from_viewport_style(viewport_style, viewport_width, viewport_height);
    let global_path = current_global_node_path(root, None);
    append_nodes_with_path(root, &mut out, &mut path, global_path, &inherited)?;
    Ok(out)
}

pub fn rsx_to_elements_lossy(root: &RsxNode) -> (Vec<Box<dyn ElementTrait>>, Vec<String>) {
    rsx_to_elements_lossy_with_context(root, &Style::new(), 0.0, 0.0)
}

pub fn rsx_to_elements_lossy_with_context(
    root: &RsxNode,
    viewport_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> (Vec<Box<dyn ElementTrait>>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut path = Vec::new();
    let inherited =
        InheritedTextStyle::from_viewport_style(viewport_style, viewport_width, viewport_height);
    let global_path = current_global_node_path(root, None);
    append_nodes_with_path_lossy(
        root,
        &mut out,
        &mut path,
        global_path,
        &inherited,
        &mut errors,
    );
    (out, errors)
}

#[derive(Clone, Debug, Default)]
struct InheritedTextStyle {
    font_families: Vec<String>,
    font_size: Option<f32>,
    root_font_size: f32,
    viewport_width: f32,
    viewport_height: f32,
    font_weight: Option<u16>,
    color: Option<Color>,
    cursor: Option<Cursor>,
    text_wrap: Option<TextWrap>,
}

impl InheritedTextStyle {
    fn from_viewport_style(style: &Style, viewport_width: f32, viewport_height: f32) -> Self {
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
}

pub struct ViewportRenderBackend<'a> {
    viewport: &'a mut Viewport,
    current_root: Option<RsxNode>,
    global_key_registry: HashMap<GlobalKey, RenderedGlobalKeyEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedGlobalKeyEntry {
    path: Vec<u64>,
    node_id: Option<u64>,
    invocation_type: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GlobalNodePath {
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

impl<'a> ViewportRenderBackend<'a> {
    pub fn new(viewport: &'a mut Viewport) -> Self {
        Self {
            viewport,
            current_root: None,
            global_key_registry: HashMap::new(),
        }
    }

    fn root_mut(&mut self, root: u64) -> Result<&mut RsxNode, String> {
        if root != 0 {
            return Err(format!("invalid root id: {root}"));
        }
        self.current_root
            .as_mut()
            .ok_or_else(|| "root is not initialized".to_string())
    }

    fn node_mut_by_path<'b>(
        node: &'b mut RsxNode,
        path: &[usize],
    ) -> Result<&'b mut RsxNode, String> {
        if path.is_empty() {
            return Ok(node);
        }
        let Some(children) = node.children_mut() else {
            return Err("path traverses through a leaf node".to_string());
        };
        let index = path[0];
        let child = children
            .get_mut(index)
            .ok_or_else(|| format!("invalid node path index: {index}"))?;
        Self::node_mut_by_path(child, &path[1..])
    }

    fn children_mut_by_path<'b>(
        node: &'b mut RsxNode,
        path: &[usize],
    ) -> Result<&'b mut Vec<RsxNode>, String> {
        let target = Self::node_mut_by_path(node, path)?;
        target
            .children_mut()
            .ok_or_else(|| "target node does not accept children".to_string())
    }

    fn rebuild_global_key_registry(&mut self) -> Result<(), String> {
        self.global_key_registry = if let Some(root) = self.current_root.as_ref() {
            collect_global_key_registry(root)?
        } else {
            HashMap::new()
        };
        Ok(())
    }

    #[cfg(test)]
    fn rendered_global_key(&self, key: GlobalKey) -> Option<&RenderedGlobalKeyEntry> {
        self.global_key_registry.get(&key)
    }
}

impl<'a> RenderBackend for ViewportRenderBackend<'a> {
    type NodeId = u64;

    fn create_root(&mut self, node: &RsxNode) -> Result<Self::NodeId, String> {
        self.current_root = Some(node.clone());
        self.rebuild_global_key_registry()?;
        Ok(0)
    }

    fn replace_root(&mut self, root: Self::NodeId, node: &RsxNode) -> Result<(), String> {
        if root != 0 {
            return Err(format!("invalid root id: {root}"));
        }
        self.current_root = Some(node.clone());
        self.rebuild_global_key_registry()?;
        Ok(())
    }

    fn apply_patch(&mut self, root: Self::NodeId, patch: &Patch) -> Result<(), String> {
        let root_node = self.root_mut(root)?;
        match patch {
            Patch::ReplaceRoot(node) => {
                *root_node = node.clone();
            }
            Patch::ReplaceNode { path, node } => {
                let target = Self::node_mut_by_path(root_node, path)?;
                *target = node.clone();
            }
            Patch::UpdateElementProps { path, changed, removed } => {
                let target = Self::node_mut_by_path(root_node, path)?;
                let RsxNode::Element(element) = target else {
                    return Err("cannot update props on non-element node".to_string());
                };
                let element = std::rc::Rc::make_mut(element);
                let props = std::rc::Rc::make_mut(&mut element.props);
                props.retain(|(k, _)| !removed.contains(k));
                for &(key, ref value) in changed {
                    if let Some((_, v)) = props.iter_mut().find(|(k, _)| *k == key) {
                        *v = value.clone();
                    } else {
                        props.push((key, value.clone()));
                    }
                }
            }
            Patch::SetText { path, text } => {
                let target = Self::node_mut_by_path(root_node, path)?;
                let RsxNode::Text(node) = target else {
                    return Err("cannot set text on non-text node".to_string());
                };
                std::rc::Rc::make_mut(node).content = text.clone();
            }
            Patch::InsertChild {
                parent_path,
                index,
                node,
            } => {
                let children = Self::children_mut_by_path(root_node, parent_path)?;
                if *index > children.len() {
                    return Err(format!("invalid child insert index: {index}"));
                }
                children.insert(*index, node.clone());
            }
            Patch::RemoveChild { parent_path, index } => {
                let children = Self::children_mut_by_path(root_node, parent_path)?;
                if *index >= children.len() {
                    return Err(format!("invalid child remove index: {index}"));
                }
                children.remove(*index);
            }
            Patch::MoveChild {
                parent_path,
                from,
                to,
            } => {
                let children = Self::children_mut_by_path(root_node, parent_path)?;
                if *from >= children.len() || *to > children.len() {
                    return Err("invalid child move indices".to_string());
                }
                let node = children.remove(*from);
                children.insert(*to, node);
            }
        }
        self.rebuild_global_key_registry()?;
        Ok(())
    }

    fn draw_frame(&mut self) -> Result<(), String> {
        let Some(root) = self.current_root.as_ref() else {
            return Ok(());
        };
        self.viewport.render_rsx(root)
    }

    fn request_redraw(&mut self) -> Result<(), String> {
        self.viewport.request_redraw();
        Ok(())
    }
}

fn append_nodes(
    node: &RsxNode,
    out: &mut Vec<Box<dyn ElementTrait>>,
    viewport_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<(), String> {
    let mut path = Vec::new();
    let inherited =
        InheritedTextStyle::from_viewport_style(viewport_style, viewport_width, viewport_height);
    let global_path = current_global_node_path(node, None);
    append_nodes_with_path(node, out, &mut path, global_path, &inherited)
}

pub(crate) fn rendered_node_id_by_index_path(
    root: &RsxNode,
    index_path: &[usize],
) -> Result<Option<u64>, String> {
    let mut token_path = Vec::new();
    rendered_node_id_by_index_path_impl(root, index_path, &mut token_path, None)
}

fn collect_global_key_registry(
    root: &RsxNode,
) -> Result<HashMap<GlobalKey, RenderedGlobalKeyEntry>, String> {
    let mut registry = HashMap::new();
    let mut path = Vec::new();
    let global_path = current_global_node_path(root, None);
    collect_global_key_registry_with_path(root, &mut path, global_path, &mut registry)?;
    Ok(registry)
}

fn collect_global_key_registry_with_path(
    node: &RsxNode,
    path: &mut Vec<u64>,
    global_path: Option<GlobalNodePath>,
    registry: &mut HashMap<GlobalKey, RenderedGlobalKeyEntry>,
) -> Result<(), String> {
    let current_global_path = current_global_node_path(node, global_path.as_ref());
    if let Some(RsxKey::Global(global_key)) = node.identity().key {
        let entry = RenderedGlobalKeyEntry {
            path: path.clone(),
            node_id: rendered_node_id(node, path, current_global_path.as_ref()),
            invocation_type: node.identity().invocation_type,
        };
        if registry.insert(global_key, entry).is_some() {
            return Err("duplicate GlobalKey detected in renderer registry".to_string());
        }
    }

    if let Some(children) = node.children() {
        let mut ordinals = HashMap::<&'static str, usize>::new();
        for child in children {
            let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
            let token = child_identity_token(child, ordinal);
            path.push(token);
            let child_global_path =
                child_global_node_path(current_global_path.as_ref(), child, token);
            collect_global_key_registry_with_path(child, path, child_global_path, registry)?;
            path.pop();
        }
    }

    Ok(())
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
    let mut ordinals = HashMap::<&'static str, usize>::new();
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
    }
}

fn append_nodes_with_path(
    node: &RsxNode,
    out: &mut Vec<Box<dyn ElementTrait>>,
    path: &mut Vec<u64>,
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<(), String> {
    match node {
        RsxNode::Fragment(fragment) => {
            let current_global_path = current_global_node_path(node, global_path.as_ref());
            let mut ordinals = HashMap::<&'static str, usize>::new();
            for child in &fragment.children {
                let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
                let token = child_identity_token(child, ordinal);
                path.push(token);
                let child_global_path =
                    child_global_node_path(current_global_path.as_ref(), child, token);
                append_nodes_with_path(child, out, path, child_global_path, inherited_text_style)?;
                path.pop();
            }
            Ok(())
        }
        _ => {
            let current_global_path = current_global_node_path(node, global_path.as_ref());
            out.push(convert_node(
                node,
                path,
                current_global_path,
                inherited_text_style,
            )?);
            Ok(())
        }
    }
}

fn append_nodes_with_path_lossy(
    node: &RsxNode,
    out: &mut Vec<Box<dyn ElementTrait>>,
    path: &mut Vec<u64>,
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
    errors: &mut Vec<String>,
) {
    match node {
        RsxNode::Fragment(fragment) => {
            let current_global_path = current_global_node_path(node, global_path.as_ref());
            let mut ordinals = HashMap::<&'static str, usize>::new();
            for child in &fragment.children {
                let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
                let token = child_identity_token(child, ordinal);
                path.push(token);
                let child_global_path =
                    child_global_node_path(current_global_path.as_ref(), child, token);
                append_nodes_with_path_lossy(
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
            match convert_node(node, path, current_global_path, inherited_text_style) {
                Ok(element) => out.push(element),
                Err(err) => errors.push(format!("node_path={path:?}: {err}")),
            }
        }
    }
}

fn convert_node(
    node: &RsxNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    match node {
        RsxNode::Text(text) => {
            let mut text_node = Text::from_content_with_id(
                stable_node_id_from_parts("TextNode", path, global_path.as_ref()),
                text.content.clone(),
            );
            if !inherited_text_style.font_families.is_empty() {
                text_node.set_fonts(inherited_text_style.font_families.clone());
            }
            if let Some(font_size) = inherited_text_style.font_size {
                text_node.set_font_size(font_size);
            }
            if let Some(font_weight) = inherited_text_style.font_weight {
                text_node.set_font_weight(font_weight);
            }
            if let Some(color) = inherited_text_style.color {
                text_node.set_color(color);
            }
            if let Some(cursor) = inherited_text_style.cursor {
                text_node.set_cursor(cursor);
            }
            Ok(Box::new(text_node))
        }
        RsxNode::Fragment(_) => Err("fragment must be flattened before conversion".to_string()),
        RsxNode::Element(el) => convert_element(el, path, global_path, inherited_text_style),
    }
}

fn convert_element(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    if is_builtin_text_node(node) {
        return convert_text_element(node, path, global_path, inherited_text_style);
    }
    if is_builtin_text_area_node(node) {
        return convert_text_area_element(node, path, global_path, inherited_text_style);
    }
    if is_builtin_image_node(node) {
        return convert_image_element(node, path, global_path, inherited_text_style);
    }
    if is_builtin_svg_node(node) {
        return convert_svg_element(node, path, global_path, inherited_text_style);
    }
    if let Some(descriptor) = node.tag_descriptor
        && let Ok(map) = typed_element_factories().read()
        && let Some(factory) = map.get(&descriptor.type_id)
    {
        return factory(node, path);
    }

    if let Ok(map) = element_factories().read() {
        if let Some(factory) = map.get(element_runtime_name(node)) {
            return factory(node, path);
        }
        if let Some(factory) = map.get(node.tag) {
            return factory(node, path);
        }
    }

    convert_container_element(node, path, global_path, inherited_text_style)
}

fn convert_container_element(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    let initial_size = if path.is_empty() { 10_000.0 } else { 0.0 };
    let mut element = Element::new_with_id(
        stable_node_id_from_parts(element_runtime_name(node), path, global_path.as_ref()),
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
            if let Some(ParsedValue::FontFamily(font_family)) = style.get(PropertyId::FontFamily) {
                child_inherited_text_style.font_families = font_family.as_slice().to_vec();
            }
            if let Some(font_size) = resolve_font_size_from_style(
                &style,
                child_inherited_text_style
                    .font_size
                    .unwrap_or(child_inherited_text_style.root_font_size),
                child_inherited_text_style.root_font_size,
                child_inherited_text_style.viewport_width,
                child_inherited_text_style.viewport_height,
            ) {
                child_inherited_text_style.font_size = Some(font_size);
            }
            if let Some(ParsedValue::FontWeight(font_weight)) = style.get(PropertyId::FontWeight) {
                child_inherited_text_style.font_weight = Some(font_weight.value());
            }
            if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
                child_inherited_text_style.color = Some(color.to_color());
            }
            if let Some(ParsedValue::Cursor(cursor)) = style.get(PropertyId::Cursor) {
                child_inherited_text_style.cursor = Some(*cursor);
            }
            if let Some(ParsedValue::TextWrap(text_wrap)) = style.get(PropertyId::TextWrap) {
                child_inherited_text_style.text_wrap = Some(*text_wrap);
            }
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
            "on_mouse_down" => {
                let handler = as_mouse_down_handler(value, key)?;
                element.on_mouse_down(move |event, _control| handler.call(event));
            }
            "on_mouse_up" => {
                let handler = as_mouse_up_handler(value, key)?;
                element.on_mouse_up(move |event, _control| handler.call(event));
            }
            "on_mouse_move" => {
                let handler = as_mouse_move_handler(value, key)?;
                element.on_mouse_move(move |event, _control| handler.call(event));
            }
            "on_mouse_enter" => {
                let handler = as_mouse_enter_handler(value, key)?;
                element.on_mouse_enter(move |event| handler.call(event));
            }
            "on_mouse_leave" => {
                let handler = as_mouse_leave_handler(value, key)?;
                element.on_mouse_leave(move |event| handler.call(event));
            }
            "on_click" => {
                let handler = as_click_handler(value, key)?;
                element.on_click(move |event, _control| handler.call(event));
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
            _ => {
                return Err(format!(
                    "unknown prop `{}` on <{}>",
                    key,
                    element_display_name(node)
                ));
            }
        }
    }

    let mut child_path = Vec::with_capacity(path.len().saturating_add(1));
    child_path.extend_from_slice(path);
    let current_global_path = current_global_node_path(
        &RsxNode::Element(std::rc::Rc::new(node.clone())),
        global_path.as_ref(),
    );
    let mut ordinals = HashMap::<&'static str, usize>::new();
    for child in &node.children {
        let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
        let token = child_identity_token(child, ordinal);
        child_path.push(token);
        let child_global_path = child_global_node_path(current_global_path.as_ref(), child, token);
        append_child_nodes_flattening_fragments(
            &mut element,
            child,
            &child_path,
            child_global_path,
            &child_inherited_text_style,
        )?;
        child_path.pop();
    }

    Ok(Box::new(element))
}

fn append_child_nodes_flattening_fragments(
    parent: &mut Element,
    node: &RsxNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<(), String> {
    match node {
        RsxNode::Fragment(fragment) => {
            let mut child_path = Vec::with_capacity(path.len().saturating_add(1));
            child_path.extend_from_slice(path);
            let current_global_path = current_global_node_path(node, global_path.as_ref());
            let mut ordinals = HashMap::<&'static str, usize>::new();
            for child in &fragment.children {
                let ordinal = next_identity_ordinal(&mut ordinals, child.identity());
                let token = child_identity_token(child, ordinal);
                child_path.push(token);
                let child_global_path =
                    child_global_node_path(current_global_path.as_ref(), child, token);
                append_child_nodes_flattening_fragments(
                    parent,
                    child,
                    &child_path,
                    child_global_path,
                    inherited_text_style,
                )?;
                child_path.pop();
            }
            Ok(())
        }
        _ => {
            parent.add_child(convert_node(node, path, global_path, inherited_text_style)?);
            Ok(())
        }
    }
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
    let mut has_explicit_font = false;
    let mut has_explicit_font_size = false;
    let mut has_explicit_font_weight = false;
    let mut has_explicit_color = false;
    let mut has_explicit_cursor = false;
    let mut has_explicit_text_wrap = false;

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
                has_explicit_font_size = true;
            }
            "line_height" => text.set_line_height(as_f32(value, key)?),
            "align" => {
                text.set_text_align(as_text_align(value, key)?);
            }
            "font" => {
                text.set_font(as_string(value, key)?);
                has_explicit_font = true;
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
        if !has_explicit_font_size
            && let Some(font_size) = resolve_font_size_from_style(
                style,
                inherited_text_style
                    .font_size
                    .unwrap_or(inherited_text_style.root_font_size),
                inherited_text_style.root_font_size,
                inherited_text_style.viewport_width,
                inherited_text_style.viewport_height,
            )
        {
            text.set_font_size(font_size);
            has_explicit_font_size = true;
        }
        if let Some(ParsedValue::FontWeight(font_weight)) = style.get(PropertyId::FontWeight) {
            text.set_font_weight(font_weight.value());
            has_explicit_font_weight = true;
        }
        if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
            text.set_color(color.clone());
            has_explicit_color = true;
        }
        if let Some(ParsedValue::Cursor(cursor)) = style.get(PropertyId::Cursor) {
            text.set_cursor(*cursor);
            has_explicit_cursor = true;
        }
        if let Some(ParsedValue::TextWrap(text_wrap)) = style.get(PropertyId::TextWrap) {
            text.set_text_wrap(*text_wrap);
            has_explicit_text_wrap = true;
        }
    }

    if !has_explicit_font && !inherited_text_style.font_families.is_empty() {
        text.set_fonts(inherited_text_style.font_families.clone());
    }
    if !has_explicit_font_size && let Some(font_size) = inherited_text_style.font_size {
        text.set_font_size(font_size);
    }
    if !has_explicit_font_weight && let Some(font_weight) = inherited_text_style.font_weight {
        text.set_font_weight(font_weight);
    }
    if !has_explicit_color && let Some(color) = inherited_text_style.color {
        text.set_color(color);
    }
    if !has_explicit_cursor && let Some(cursor) = inherited_text_style.cursor {
        text.set_cursor(cursor);
    }
    if !has_explicit_text_wrap && let Some(text_wrap) = inherited_text_style.text_wrap {
        text.set_text_wrap(text_wrap);
    }
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

    text.set_text(text_content);
    Ok(Box::new(text))
}

fn length_from_parsed_value(value: &ParsedValue, context: &str) -> Result<Option<f32>, String> {
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

fn size_length_from_parsed_value(
    value: &ParsedValue,
    context: &str,
) -> Result<Option<Length>, String> {
    match value {
        ParsedValue::Length(length) => Ok(Some(*length)),
        ParsedValue::Auto => Ok(None),
        _ => Err(format!("{context} expects Length value")),
    }
}

fn resolve_font_size_from_style(
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

fn convert_text_area_element(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    let mut text_content = String::new();
    let mut placeholder = String::new();
    let mut binding: Option<Binding<String>> = None;
    let mut text_area = TextArea::from_content_with_id(
        stable_node_id_from_parts("TextArea", path, global_path.as_ref()),
        "",
    );
    let mut style: Option<Style> = None;
    let mut x: Option<f32> = None;
    let mut y: Option<f32> = None;
    let mut width: Option<Length> = None;
    let mut height: Option<Length> = None;
    let mut source_text_start: Option<usize> = None;
    let mut source_text_end: Option<usize> = None;
    let mut projection_nodes: Vec<(std::ops::Range<usize>, Box<dyn ElementTrait>)> = Vec::new();
    let mut has_explicit_font = false;
    let mut has_explicit_font_size = false;
    let mut has_explicit_color = false;

    for (key, value) in node.props.iter() {
        if *key == "key" {
            continue;
        }
        match *key {
            "style" => style = Some(as_element_style(value, key)?),
            "on_focus" => {
                let handler = as_text_area_focus_handler(value, key)?;
                text_area.on_focus(move |event| handler.call(event));
            }
            "on_blur" => {
                let handler = as_blur_handler(value, key)?;
                text_area.on_blur(move |event, _control| handler.call(event));
            }
            "on_change" => {
                let handler = as_text_change_handler(value, key)?;
                text_area.on_change(move |event| handler.call(event));
            }
            "content" => {
                text_content = as_owned_string(value, key)?;
            }
            "placeholder" => {
                placeholder = as_owned_string(value, key)?;
            }
            "binding" => {
                binding = Some(as_binding_string(value, key)?);
            }
            "x" => {
                x = Some(as_f32(value, key)?);
            }
            "y" => {
                y = Some(as_f32(value, key)?);
            }
            "font_size" => {
                text_area.set_font_size(as_font_size_px(
                    value,
                    key,
                    inherited_text_style
                        .font_size
                        .unwrap_or(inherited_text_style.root_font_size),
                    inherited_text_style.root_font_size,
                    inherited_text_style.viewport_width,
                    inherited_text_style.viewport_height,
                )?);
                has_explicit_font_size = true;
            }
            "font" => {
                text_area.set_font(as_string(value, key)?);
                has_explicit_font = true;
            }
            "opacity" => text_area.set_opacity(as_f32(value, key)?),
            "multiline" => text_area.set_multiline(as_bool(value, key)?),
            "read_only" => text_area.set_read_only(as_bool(value, key)?),
            "max_length" => text_area.set_max_length(as_usize(value, key)?),
            "source_text_start" => {
                source_text_start = as_usize(value, key)?;
            }
            "source_text_end" => {
                source_text_end = as_usize(value, key)?;
            }
            _ => return Err(format!("unknown prop `{}` on <TextArea>", key,)),
        }
    }

    if let (Some(start), Some(end)) = (source_text_start, source_text_end)
        && start <= end
    {
        text_area.set_source_text_range(Some(start..end));
    }

    if let Some(style) = &style {
        if let Some(value) = style.get(PropertyId::Width) {
            width = size_length_from_parsed_value(value, "TextArea style.width")?;
        }
        if let Some(value) = style.get(PropertyId::Height) {
            height = size_length_from_parsed_value(value, "TextArea style.height")?;
        }
        if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
            text_area.set_color(color.clone());
            has_explicit_color = true;
        }
        if let Some(selection) = style.selection()
            && let Some(background) = selection.background_color()
        {
            text_area.set_selection_background_color(background.clone());
        }
    }

    text_area.set_position(x.unwrap_or(0.0), y.unwrap_or(0.0));
    if !has_explicit_font && !inherited_text_style.font_families.is_empty() {
        text_area.set_fonts(inherited_text_style.font_families.clone());
    }
    if !has_explicit_font_size && let Some(font_size) = inherited_text_style.font_size {
        text_area.set_font_size(font_size);
    }
    if !has_explicit_color && let Some(color) = inherited_text_style.color {
        text_area.set_color(color);
    }
    if let Some(cursor) = inherited_text_style.cursor {
        text_area.set_cursor(cursor);
    }
    text_area.set_style_width(width);
    text_area.set_style_height(height);

    for (child_index, child) in node.children.iter().enumerate() {
        if let Some(projection) =
            convert_text_area_projection_child(child, path, child_index, inherited_text_style)?
        {
            projection_nodes.push(projection);
            continue;
        }
        if binding.is_none() && text_content.is_empty() {
            match child {
                RsxNode::Text(content) => text_content.push_str(&content.content),
                RsxNode::Fragment(fragment) => {
                    for nested in &fragment.children {
                        append_text_children(&mut text_content, nested)?;
                    }
                }
                _ => return Err("<TextArea> children must be text".to_string()),
            }
        }
    }

    if binding.is_none() {
        text_area.set_text(text_content);
    } else if let Some(bound) = binding.as_ref() {
        text_area.set_text(bound.get());
    }

    if let Some(bound) = binding {
        text_area.bind_text(bound);
    }
    if !projection_nodes.is_empty() {
        text_area.set_render_projection_nodes(projection_nodes);
    }
    if !placeholder.is_empty() {
        text_area.set_placeholder(placeholder);
    }
    Ok(Box::new(text_area))
}

fn convert_text_area_projection_child(
    node: &RsxNode,
    path: &[u64],
    child_index: usize,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Option<(std::ops::Range<usize>, Box<dyn ElementTrait>)>, String> {
    let RsxNode::Element(element) = node else {
        return Ok(None);
    };
    if element.tag != TEXT_AREA_PROJECTION_TAG {
        return Ok(None);
    }

    let mut start = None;
    let mut end = None;
    for (key, value) in element.props.iter() {
        match *key {
            "source_text_start" => start = as_usize(value, key)?,
            "source_text_end" => end = as_usize(value, key)?,
            _ => {
                return Err(format!(
                    "unknown prop `{}` on <{}>",
                    key, TEXT_AREA_PROJECTION_TAG
                ));
            }
        }
    }

    let (Some(start), Some(end)) = (start, end) else {
        return Err(
            "TextArea projection child requires source_text_start/source_text_end".to_string(),
        );
    };

    let fragment = RsxNode::fragment(element.children.clone());
    let scope = [path, &[0x5458_5052, child_index as u64]].concat();
    let mut children = rsx_to_elements_scoped_with_context(
        &fragment,
        &scope,
        &projection_inherited_style_from_context(inherited_text_style),
        0.0,
        0.0,
    )?;
    let mut root = wrap_projection_children_for_adapter(path, child_index, &mut children)?;
    apply_text_source_range_to_projection(root.as_mut(), start..end);
    Ok(Some((start..end, root)))
}

fn projection_inherited_style_from_context(inherited_text_style: &InheritedTextStyle) -> Style {
    let mut style = Style::new();
    if !inherited_text_style.font_families.is_empty() {
        style.insert(
            PropertyId::FontFamily,
            ParsedValue::FontFamily(crate::FontFamily::new(
                inherited_text_style.font_families.clone(),
            )),
        );
    }
    if let Some(font_size) = inherited_text_style.font_size {
        style.insert(
            PropertyId::FontSize,
            ParsedValue::FontSize(crate::FontSize::px(font_size)),
        );
    }
    if let Some(color) = inherited_text_style.color {
        style.insert(PropertyId::Color, ParsedValue::Color(color.into()));
    }
    style
}

fn wrap_projection_children_for_adapter(
    path: &[u64],
    child_index: usize,
    children: &mut Vec<Box<dyn ElementTrait>>,
) -> Result<Box<dyn ElementTrait>, String> {
    if children.is_empty() {
        return Err("projection produced no elements".to_string());
    }
    if children.len() == 1 {
        return Ok(children.remove(0));
    }

    let wrapper_id = stable_node_id_from_parts(
        "TextAreaProjectionWrapper",
        &[path, &[0x5458_5057, child_index as u64]].concat(),
        None,
    );
    let mut wrapper = Element::new_with_id(wrapper_id, 0.0, 0.0, 0.0, 0.0);
    wrapper.set_intrinsic_size_as_percent_base(false);
    let mut style = Style::new();
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
    );
    style.insert(PropertyId::Width, ParsedValue::Auto);
    style.insert(PropertyId::Height, ParsedValue::Auto);
    wrapper.apply_style(style);
    for child in children.drain(..) {
        wrapper.add_child(child);
    }
    Ok(Box::new(wrapper))
}

fn apply_text_source_range_to_projection(
    node: &mut dyn ElementTrait,
    range: std::ops::Range<usize>,
) {
    if let Some(text_area) = node.as_any_mut().downcast_mut::<TextArea>() {
        text_area.set_source_text_range(Some(range.clone()));
    }
    if let Some(children) = node.children_mut() {
        for child in children {
            apply_text_source_range_to_projection(child.as_mut(), range.clone());
        }
    }
}

fn convert_image_element(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    if !node.children.is_empty() {
        return Err("<Image> does not accept children; use loading/error props".to_string());
    }

    let mut source: Option<ImageSource> = None;
    let mut fit = ImageFit::Contain;
    let mut sampling = ImageSampling::Linear;
    let mut style: Option<Style> = None;
    let mut loading = Vec::new();
    let mut error = Vec::new();

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
                loading = convert_image_slot(
                    value,
                    path,
                    global_path.clone(),
                    inherited_text_style,
                    "loading",
                )?
            }
            "error" => {
                error = convert_image_slot(
                    value,
                    path,
                    global_path.clone(),
                    inherited_text_style,
                    "error",
                )?
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
    image.set_loading_slot(loading);
    image.set_error_slot(error);
    Ok(Box::new(image))
}

fn convert_svg_element(
    node: &RsxElementNode,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
) -> Result<Box<dyn ElementTrait>, String> {
    if !node.children.is_empty() {
        return Err("<Svg> does not accept children; use loading/error props".to_string());
    }

    let mut source: Option<SvgSource> = None;
    let mut fit = ImageFit::Contain;
    let mut sampling = ImageSampling::Linear;
    let mut style: Option<Style> = None;
    let mut loading = Vec::new();
    let mut error = Vec::new();

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
                loading = convert_image_slot(
                    value,
                    path,
                    global_path.clone(),
                    inherited_text_style,
                    "loading",
                )?
            }
            "error" => {
                error = convert_image_slot(
                    value,
                    path,
                    global_path.clone(),
                    inherited_text_style,
                    "error",
                )?
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
    svg.set_loading_slot(loading);
    svg.set_error_slot(error);
    Ok(Box::new(svg))
}

fn convert_image_slot(
    value: &PropValue,
    path: &[u64],
    global_path: Option<GlobalNodePath>,
    inherited_text_style: &InheritedTextStyle,
    slot_name: &str,
) -> Result<Vec<Box<dyn ElementTrait>>, String> {
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
    append_child_nodes_flattening_fragments(
        &mut wrapper,
        &slot_node,
        &slot_path,
        slot_global_path,
        inherited_text_style,
    )?;
    Ok(vec![Box::new(wrapper)])
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
    ordinals: &mut HashMap<&'static str, usize>,
    identity: &RsxNodeIdentity,
) -> usize {
    let entry = ordinals
        .entry(identity.invocation_type)
        .or_insert(0);
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
        Some(crate::ui::RsxKey::Local(key)) => {
            hash ^= 0x4c;
            hash = hash.wrapping_mul(FNV_PRIME);
            for byte in key.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        Some(crate::ui::RsxKey::Global(global_key)) => {
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

fn as_f32(value: &PropValue, key: &str) -> Result<f32, String> {
    match value {
        PropValue::I64(v) => Ok(*v as f32),
        PropValue::F64(v) => Ok(*v as f32),
        _ => Err(format!("prop `{key}` expects numeric value")),
    }
}

fn as_font_size_px(
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

fn as_string<'a>(value: &'a PropValue, key: &str) -> Result<&'a str, String> {
    match value {
        PropValue::String(v) => Ok(v.as_str()),
        _ => Err(format!("prop `{key}` expects string value")),
    }
}

fn as_owned_string(value: &PropValue, key: &str) -> Result<String, String> {
    Ok(as_string(value, key)?.to_string())
}

fn as_text_align(value: &PropValue, key: &str) -> Result<crate::TextAlign, String> {
    match value {
        PropValue::TextAlign(v) => Ok(*v),
        _ => Err(format!("prop `{key}` expects TextAlign value")),
    }
}

fn as_binding_string(value: &PropValue, key: &str) -> Result<Binding<String>, String> {
    Binding::<String>::from_prop_value(value.clone())
        .map_err(|_| format!("prop `{key}` expects Binding<String> value"))
}

fn as_bool(value: &PropValue, key: &str) -> Result<bool, String> {
    match value {
        PropValue::Bool(v) => Ok(*v),
        _ => Err(format!("prop `{key}` expects bool value")),
    }
}

fn as_usize(value: &PropValue, key: &str) -> Result<Option<usize>, String> {
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

fn as_element_style(value: &PropValue, key: &str) -> Result<Style, String> {
    ElementStylePropSchema::from_prop_value(value.clone())
        .map(|style| style.to_style())
        .map_err(|_| format!("prop `{key}` expects ElementStylePropSchema value"))
}

fn as_text_style(value: &PropValue, key: &str) -> Result<Style, String> {
    TextStylePropSchema::from_prop_value(value.clone())
        .map(|style| style.to_style())
        .map_err(|_| format!("prop `{key}` expects TextStylePropSchema value"))
}

fn as_mouse_down_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::MouseDownHandlerProp, String> {
    match value {
        PropValue::OnMouseDown(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects mouse down handler value")),
    }
}

fn as_mouse_up_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::MouseUpHandlerProp, String> {
    match value {
        PropValue::OnMouseUp(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects mouse up handler value")),
    }
}

fn as_mouse_move_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::MouseMoveHandlerProp, String> {
    match value {
        PropValue::OnMouseMove(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects mouse move handler value")),
    }
}

fn as_mouse_enter_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::MouseEnterHandlerProp, String> {
    match value {
        PropValue::OnMouseEnter(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects mouse enter handler value")),
    }
}

fn as_mouse_leave_handler(
    value: &PropValue,
    key: &str,
) -> Result<crate::ui::MouseLeaveHandlerProp, String> {
    match value {
        PropValue::OnMouseLeave(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects mouse leave handler value")),
    }
}

fn as_click_handler(value: &PropValue, key: &str) -> Result<crate::ui::ClickHandlerProp, String> {
    match value {
        PropValue::OnClick(v) => Ok(v.clone()),
        _ => Err(format!("prop `{key}` expects click handler value")),
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

#[cfg(test)]
mod tests {
    use super::{
        RenderedGlobalKeyEntry, ViewportRenderBackend, element_runtime_name,
        identity_token_from_node_identity, register_tag_element_factory, rendered_node_id,
        rsx_to_elements, rsx_to_elements_lossy, rsx_to_elements_with_context,
        stable_node_id_from_parts,
    };
    use crate::ui::{
        GlobalKey, RenderBackend, RsxKey, RsxNode, RsxNodeIdentity, RsxTagDescriptor, rsx,
    };
    use crate::view::Viewport;
    use crate::view::base_component::{
        Element, ElementTrait, Text, TextArea, get_cursor_by_id, hit_test,
    };
    use crate::view::{
        Element as HostElement, ElementStylePropSchema, Svg as HostSvg, SvgSource,
        Text as HostText, TextArea as HostTextArea, TextStylePropSchema,
    };
    use crate::{
        Border, BorderRadius, Color, ColorLike, Cursor, FontSize, IntoColor, Layout, Length,
        ParsedValue, PropertyId, Style, Unit,
    };
    use std::sync::Arc;

    fn host_element_node() -> RsxNode {
        RsxNode::tagged("Element", RsxTagDescriptor::of::<HostElement>())
    }

    fn host_text_node() -> RsxNode {
        RsxNode::tagged("Text", RsxTagDescriptor::of::<HostText>())
    }

    fn host_text_area_node() -> RsxNode {
        RsxNode::tagged("TextArea", RsxTagDescriptor::of::<HostTextArea>())
    }

    fn host_svg_node() -> RsxNode {
        RsxNode::tagged("Svg", RsxTagDescriptor::of::<HostSvg>())
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
    fn key_prop_is_accepted_for_element_node() {
        let node = host_element_node()
            .with_prop("key", "feature-1")
            .with_prop("style", empty_element_style());
        let converted = rsx_to_elements(&node);
        assert!(converted.is_ok());
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
    fn typed_factory_registration_uses_tag_descriptor_type_id() {
        struct CustomContainer;

        register_tag_element_factory::<CustomContainer>(Arc::new(|node, path| {
            let mut element = Element::new_with_id(
                stable_node_id_from_parts(element_runtime_name(node), path, None),
                0.0,
                0.0,
                64.0,
                32.0,
            );
            element.apply_style(Style::new());
            Ok(Box::new(element))
        }));

        let node = RsxNode::tagged(
            "CustomContainer",
            crate::ui::RsxTagDescriptor::of::<CustomContainer>(),
        );
        let converted = rsx_to_elements(&node).expect("typed factory should convert");
        assert_eq!(converted.len(), 1);
    }

    #[test]
    fn metadata_key_is_accepted_for_element_node() {
        let node = host_element_node()
            .with_key(GlobalKey::from("feature-1"))
            .with_invocation_type("Button")
            .with_prop("style", empty_element_style());
        let converted = rsx_to_elements(&node);
        assert!(converted.is_ok());
    }

    #[test]
    fn svg_node_accepts_typed_source_prop() {
        let node = host_svg_node()
            .with_prop(
                "source",
                SvgSource::Content(
                    r#"<svg width="8" height="4" xmlns="http://www.w3.org/2000/svg"></svg>"#
                        .to_string(),
                ),
            )
            .with_prop("style", empty_element_style());
        let converted = rsx_to_elements(&node);
        assert!(converted.is_ok());
    }

    #[test]
    fn global_key_registry_keeps_fragment_path_without_node_id() {
        let global_key = GlobalKey::from("fragment-root");
        let node = RsxNode::fragment(vec![
            host_element_node().with_prop("style", empty_element_style()),
        ])
        .with_key(global_key)
        .with_invocation_type("Button");

        let registry = super::collect_global_key_registry(&node).expect("registry should build");
        let entry = registry.get(&global_key).expect("global key entry");
        assert_eq!(
            entry,
            &RenderedGlobalKeyEntry {
                path: Vec::new(),
                node_id: None,
                invocation_type: "Button",
            }
        );
    }

    #[test]
    fn backend_rebuilds_global_key_registry_after_replace_root() {
        let global_key = GlobalKey::from("moving-root");
        let first_root = host_element_node()
            .with_prop("style", empty_element_style())
            .with_child(
                host_element_node()
                    .with_prop("style", empty_element_style())
                    .with_child(
                        host_element_node()
                            .with_key(global_key)
                            .with_invocation_type("Button")
                            .with_prop("style", empty_element_style()),
                    ),
            );
        let second_root = host_element_node()
            .with_prop("style", empty_element_style())
            .with_child(host_element_node().with_prop("style", empty_element_style()))
            .with_child(
                host_element_node()
                    .with_prop("style", empty_element_style())
                    .with_child(
                        host_element_node()
                            .with_key(global_key)
                            .with_invocation_type("Button")
                            .with_prop("style", empty_element_style()),
                    ),
            );

        let mut viewport = Viewport::new();
        let mut backend = ViewportRenderBackend::new(&mut viewport);
        backend.create_root(&first_root).expect("create root");
        let first = backend
            .rendered_global_key(global_key)
            .expect("first registry entry")
            .clone();
        backend
            .replace_root(0, &second_root)
            .expect("replace root should rebuild registry");
        let second = backend
            .rendered_global_key(global_key)
            .expect("second registry entry")
            .clone();
        let second_global_node = &second_root.children().unwrap()[1].children().unwrap()[0];

        assert_ne!(first.path, second.path);
        assert_eq!(first.invocation_type, "Button");
        assert_eq!(second.invocation_type, "Button");
        assert_eq!(first.node_id, second.node_id);
        assert_eq!(
            second.node_id,
            rendered_node_id(
                second_global_node,
                &second.path,
                super::current_global_node_path(second_global_node, None).as_ref(),
            )
        );
    }

    #[test]
    fn global_key_subtree_node_id_is_stable_across_parent_move() {
        let global_key = GlobalKey::from("stable-subtree");
        let first_root = host_element_node()
            .with_prop("style", empty_element_style())
            .with_child(
                host_element_node()
                    .with_prop("style", empty_element_style())
                    .with_child(
                        host_element_node()
                            .with_key(global_key)
                            .with_invocation_type("Card")
                            .with_prop("style", empty_element_style())
                            .with_child(
                                host_element_node().with_prop("style", empty_element_style()),
                            ),
                    ),
            );
        let second_root = host_element_node()
            .with_prop("style", empty_element_style())
            .with_child(host_element_node().with_prop("style", empty_element_style()))
            .with_child(
                host_element_node()
                    .with_prop("style", empty_element_style())
                    .with_child(
                        host_element_node()
                            .with_key(global_key)
                            .with_invocation_type("Card")
                            .with_prop("style", empty_element_style())
                            .with_child(
                                host_element_node().with_prop("style", empty_element_style()),
                            ),
                    ),
            );

        let first_registry =
            super::collect_global_key_registry(&first_root).expect("first registry");
        let second_registry =
            super::collect_global_key_registry(&second_root).expect("second registry");
        let first_entry = first_registry.get(&global_key).expect("first global entry");
        let second_entry = second_registry
            .get(&global_key)
            .expect("second global entry");

        assert_ne!(first_entry.path, second_entry.path);
        assert_eq!(first_entry.node_id, second_entry.node_id);
    }

    #[test]
    fn element_anchor_prop_and_style_position_are_supported() {
        let root_style = ElementStylePropSchema {
            width: Some(Length::px(120.0)),
            height: Some(Length::px(80.0)),
            ..empty_element_style()
        };
        let child_style = ElementStylePropSchema {
            position: Some(
                crate::Position::absolute()
                    .anchor("card_anchor")
                    .top(Length::px(8.0)),
            ),
            width: Some(Length::px(20.0)),
            height: Some(Length::px(10.0)),
            ..empty_element_style()
        };
        let tree = host_element_node()
            .with_prop("anchor", "card_anchor")
            .with_prop("style", root_style)
            .with_child(host_element_node().with_prop("style", child_style));
        let converted = rsx_to_elements(&tree);
        assert!(converted.is_ok());
    }

    fn style_bg_border(
        bg_hex: &str,
        border_hex: &str,
        border_width: f32,
    ) -> ElementStylePropSchema {
        ElementStylePropSchema {
            background: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(bg_hex)))),
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

    fn walk_layout(
        node: &mut dyn crate::view::base_component::ElementTrait,
        out: &mut Vec<(f32, f32, f32, f32)>,
    ) {
        let s = node.box_model_snapshot();
        out.push((s.x, s.y, s.width, s.height));
        if let Some(children) = node.children_mut() {
            for child in children {
                walk_layout(child.as_mut(), out);
            }
        }
    }

    fn collect_text_like_boxes(node: &dyn ElementTrait, out: &mut Vec<(f32, f32)>) {
        if node.as_any().is::<Text>() || node.as_any().is::<TextArea>() {
            let snapshot = node.box_model_snapshot();
            out.push((snapshot.width, snapshot.height));
        }
        if let Some(children) = node.children() {
            for child in children {
                collect_text_like_boxes(child.as_ref(), out);
            }
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

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        for root in &mut roots {
            root.measure(crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            });
            root.place(crate::view::base_component::LayoutPlacement {
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
            });
        }

        let mut boxes = Vec::new();
        for root in &mut roots {
            walk_layout(root.as_mut(), &mut boxes);
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

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        for root in &mut roots {
            root.measure(crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            });
            root.place(crate::view::base_component::LayoutPlacement {
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
            });
        }

        let mut boxes = Vec::new();
        for root in &mut roots {
            walk_layout(root.as_mut(), &mut boxes);
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

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let snapshot = root.box_model_snapshot();
        assert_eq!(snapshot.width, 282.0);
        assert_eq!(snapshot.height, 34.0);
    }

    #[test]
    fn nested_fragment_children_are_flattened_during_conversion() {
        let tree = host_element_node()
            .with_prop("style", style_with_size(empty_element_style(), 120.0, 60.0))
            .with_child(RsxNode::fragment(vec![
                host_text_node()
                    .with_prop("style", text_style_with_size(16.0, 16.0))
                    .with_child(RsxNode::text("A")),
                RsxNode::fragment(vec![
                    host_text_node()
                        .with_prop("style", text_style_with_size(16.0, 16.0))
                        .with_child(RsxNode::text("B")),
                ]),
            ]));

        let converted = rsx_to_elements(&tree);
        assert!(converted.is_ok());
    }

    #[test]
    fn lossy_conversion_skips_bad_nodes_and_keeps_good_nodes() {
        let good = host_element_node().with_prop("style", empty_element_style());
        let bad = host_element_node().with_prop("not_exists", true);
        let tree = RsxNode::fragment(vec![good, bad]);

        let (converted, errors) = rsx_to_elements_lossy(&tree);
        assert_eq!(converted.len(), 1);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn cursor_style_inherits_to_child_when_child_has_no_cursor() {
        let parent_style = ElementStylePropSchema {
            width: Some(Length::px(100.0)),
            height: Some(Length::px(100.0)),
            background: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(
                "#101010",
            )))),
            cursor: Some(Cursor::Pointer),
            ..empty_element_style()
        };

        let child_style = ElementStylePropSchema {
            width: Some(Length::px(40.0)),
            height: Some(Length::px(40.0)),
            background: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(
                "#ff0000",
            )))),
            ..empty_element_style()
        };

        let tree = host_element_node()
            .with_prop("style", parent_style)
            .with_child(host_element_node().with_prop("style", child_style));

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let target_id = hit_test(root.as_ref(), 10.0, 10.0).expect("hit child");
        let cursor = get_cursor_by_id(root.as_ref(), target_id).expect("cursor exists");
        assert_eq!(cursor, Cursor::Pointer);
    }

    #[test]
    fn cursor_style_inherits_to_text_child() {
        let parent_style = ElementStylePropSchema {
            width: Some(Length::px(200.0)),
            height: Some(Length::px(80.0)),
            background: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(
                "#101010",
            )))),
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

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let target_id = hit_test(root.as_ref(), 10.0, 10.0).expect("hit text child");
        let cursor = get_cursor_by_id(root.as_ref(), target_id).expect("cursor exists");
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

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let mut text_boxes = Vec::new();
        collect_text_like_boxes(root.as_ref(), &mut text_boxes);
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

        let mut small = rsx_to_elements_with_context(&text_tree, &small_root_style, 800.0, 600.0)
            .expect("convert with small root style");
        let mut large = rsx_to_elements_with_context(&text_tree, &large_root_style, 800.0, 600.0)
            .expect("convert with large root style");

        for root in &mut small {
            root.measure(crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            });
            root.place(crate::view::base_component::LayoutPlacement {
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
            });
        }
        for root in &mut large {
            root.measure(crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            });
            root.place(crate::view::base_component::LayoutPlacement {
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
            });
        }

        let small_snapshot = small.first().expect("small root").box_model_snapshot();
        let large_snapshot = large.first().expect("large root").box_model_snapshot();
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

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let mut text_boxes = Vec::new();
        collect_text_like_boxes(root.as_ref(), &mut text_boxes);
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

        let inherited = rsx_to_elements(&inherited_tree).expect("convert inherited tree");
        let explicit = rsx_to_elements(&explicit_tree).expect("convert explicit tree");

        let inherited_textarea = inherited
            .first()
            .and_then(|root| root.children())
            .and_then(|children| children.first())
            .and_then(|node| node.as_any().downcast_ref::<TextArea>())
            .expect("inherited textarea");
        let explicit_textarea = explicit
            .first()
            .and_then(|root| root.children())
            .and_then(|children| children.first())
            .and_then(|node| node.as_any().downcast_ref::<TextArea>())
            .expect("explicit textarea");

        assert_eq!(
            inherited_textarea.color_rgba_f32(),
            parent_color.to_rgba_f32()
        );
        assert_eq!(
            explicit_textarea.color_rgba_f32(),
            local_color.to_rgba_f32()
        );
    }

    #[test]
    fn textarea_rejects_legacy_color_prop() {
        let tree = host_text_area_node()
            .with_prop("color", "#ff0000")
            .with_prop("content", "hello")
            .with_prop("multiline", false);

        let error = rsx_to_elements(&tree)
            .err()
            .expect("legacy color prop should fail");
        assert!(error.contains("unknown prop `color` on <TextArea>"));
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

        let roots = rsx_to_elements(&tree).expect("convert rsx");
        assert_eq!(roots.len(), 1);
        assert!(roots[0].as_any().downcast_ref::<TextArea>().is_some());
    }

    #[test]
    fn textarea_uses_style_width_and_height() {
        let textarea_style = ElementStylePropSchema {
            width: Some(Length::px(296.0)),
            height: Some(Length::px(78.0)),
            ..empty_element_style()
        };

        let tree = host_text_area_node()
            .with_prop("style", textarea_style)
            .with_prop("content", "hello")
            .with_prop("multiline", true);

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let snapshot = root.box_model_snapshot();
        assert_eq!(snapshot.width, 296.0);
        assert_eq!(snapshot.height, 78.0);
    }

    #[test]
    fn textarea_uses_percent_size_from_parent_inner() {
        let parent_style = ElementStylePropSchema {
            width: Some(Length::px(400.0)),
            height: Some(Length::px(200.0)),
            ..empty_element_style()
        };

        let textarea_style = ElementStylePropSchema {
            width: Some(Length::percent(50.0)),
            height: Some(Length::percent(25.0)),
            ..empty_element_style()
        };

        let tree = host_element_node()
            .with_prop("style", parent_style)
            .with_child(
                host_text_area_node()
                    .with_prop("style", textarea_style)
                    .with_prop("content", "hello")
                    .with_prop("multiline", true),
            );

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let textarea = root
            .children()
            .expect("parent children")
            .first()
            .expect("textarea child");
        let snapshot = textarea.box_model_snapshot();
        assert_eq!(snapshot.width, 200.0);
        assert_eq!(snapshot.height, 50.0);
    }

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

        let mut roots = rsx_to_elements(&tree).expect("convert rsx");
        let root = roots.first_mut().expect("single root");
        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let child = root
            .children()
            .expect("root children")
            .first()
            .expect("child");
        let root_snapshot = root.box_model_snapshot();
        let child_snapshot = child.box_model_snapshot();
        assert_eq!(root_snapshot.height, 0.0);
        assert_eq!(child_snapshot.height, 0.0);
    }
}
