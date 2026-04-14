#![allow(missing_docs)]

//! Core RSX node and prop data structures.

use crate::FontSize;
use crate::TextAlign;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp,
    MouseDownHandlerProp, MouseEnterHandlerProp, MouseLeaveHandlerProp, MouseMoveHandlerProp,
    MouseUpHandlerProp, TextAreaFocusHandlerProp, TextAreaRenderHandlerProp, TextChangeHandlerProp,
};
use std::any::{Any, TypeId};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlobalKey {
    id: u64,
}

impl GlobalKey {
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn from<T: Hash>(value: T) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        Self {
            id: hasher.finish(),
        }
    }

    pub fn id(self) -> u64 {
        self.id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RsxKey {
    Local(u64),
    Global(GlobalKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RsxNodeIdentity {
    pub invocation_type: &'static str,
    pub key: Option<RsxKey>,
}

impl RsxNodeIdentity {
    pub fn new(invocation_type: &'static str, key: Option<RsxKey>) -> Self {
        Self {
            invocation_type,
            key,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RsxNode {
    Element(RsxElementNode),
    Text(RsxTextNode),
    Fragment(RsxFragmentNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RsxTagDescriptor {
    pub type_id: TypeId,
    pub type_name: &'static str,
}

impl RsxTagDescriptor {
    pub fn of<T: 'static>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: std::any::type_name::<T>(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsxElementNode {
    pub identity: RsxNodeIdentity,
    pub tag: &'static str,
    pub tag_descriptor: Option<RsxTagDescriptor>,
    pub props: Vec<(&'static str, PropValue)>,
    pub children: Vec<RsxNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsxTextNode {
    pub identity: RsxNodeIdentity,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsxFragmentNode {
    pub identity: RsxNodeIdentity,
    pub children: Vec<RsxNode>,
}

#[derive(Clone)]
pub struct SharedPropValue {
    value: Rc<dyn Any>,
}

impl SharedPropValue {
    pub fn new(value: Rc<dyn Any>) -> Self {
        Self { value }
    }

    pub fn value(&self) -> Rc<dyn Any> {
        self.value.clone()
    }
}

impl fmt::Debug for SharedPropValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedPropValue")
            .field("ptr", &Rc::as_ptr(&self.value))
            .finish()
    }
}

impl PartialEq for SharedPropValue {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.value, &other.value)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RsxProps {
    entries: Vec<(&'static str, PropValue)>,
}

impl RsxProps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, key: &'static str, value: PropValue) {
        self.entries.push((key, value));
    }

    pub fn into_entries(self) -> Vec<(&'static str, PropValue)> {
        self.entries
    }

    pub fn remove_raw(&mut self, key: &str) -> Option<PropValue> {
        let index = self.entries.iter().position(|(k, _)| *k == key)?;
        let (_, value) = self.entries.swap_remove(index);
        Some(value)
    }

    pub fn remove_t<T: FromPropValue>(&mut self, key: &str) -> Result<Option<T>, String> {
        match self.remove_raw(key) {
            Some(value) => Ok(Some(T::from_prop_value(value)?)),
            None => Ok(None),
        }
    }

    pub fn remove_string(&mut self, key: &str) -> Result<Option<String>, String> {
        if let Some(index) = self.entries.iter().position(|(k, _)| *k == key) {
            let (_, value) = self.entries.swap_remove(index);
            return match value {
                PropValue::String(v) => Ok(Some(v)),
                _ => Err(format!("prop `{key}` expects string value")),
            };
        }
        Ok(None)
    }

    pub fn remove_f64(&mut self, key: &str) -> Result<Option<f64>, String> {
        if let Some(index) = self.entries.iter().position(|(k, _)| *k == key) {
            let (_, value) = self.entries.swap_remove(index);
            return match value {
                PropValue::I64(v) => Ok(Some(v as f64)),
                PropValue::F64(v) => Ok(Some(v)),
                _ => Err(format!("prop `{key}` expects numeric value")),
            };
        }
        Ok(None)
    }

    pub fn remove_bool(&mut self, key: &str) -> Result<Option<bool>, String> {
        if let Some(index) = self.entries.iter().position(|(k, _)| *k == key) {
            let (_, value) = self.entries.swap_remove(index);
            return match value {
                PropValue::Bool(v) => Ok(Some(v)),
                _ => Err(format!("prop `{key}` expects bool value")),
            };
        }
        Ok(None)
    }

    pub fn reject_remaining(&self, owner: &str) -> Result<(), String> {
        if let Some((key, _)) = self.entries.first() {
            return Err(format!("unknown prop `{key}` on <{owner}>"));
        }
        Ok(())
    }
}

impl RsxNode {
    pub fn element(tag: &'static str) -> Self {
        Self::Element(RsxElementNode {
            identity: RsxNodeIdentity::new(tag, None),
            tag,
            tag_descriptor: None,
            props: Vec::new(),
            children: Vec::new(),
        })
    }

    pub fn tagged(tag: &'static str, descriptor: RsxTagDescriptor) -> Self {
        Self::Element(RsxElementNode {
            identity: RsxNodeIdentity::new(descriptor.type_name, None),
            tag,
            tag_descriptor: Some(descriptor),
            props: Vec::new(),
            children: Vec::new(),
        })
    }

    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(RsxTextNode {
            identity: RsxNodeIdentity::new("Text", None),
            content: content.into(),
        })
    }

    pub fn fragment(children: Vec<RsxNode>) -> Self {
        Self::Fragment(RsxFragmentNode {
            identity: RsxNodeIdentity::new("Fragment", None),
            children,
        })
    }

    pub fn identity(&self) -> &RsxNodeIdentity {
        match self {
            Self::Element(node) => &node.identity,
            Self::Text(node) => &node.identity,
            Self::Fragment(node) => &node.identity,
        }
    }

    pub fn set_identity(&mut self, identity: RsxNodeIdentity) {
        match self {
            Self::Element(node) => node.identity = identity,
            Self::Text(node) => node.identity = identity,
            Self::Fragment(node) => node.identity = identity,
        }
    }

    pub fn with_identity(mut self, identity: RsxNodeIdentity) -> Self {
        self.set_identity(identity);
        self
    }

    pub fn with_invocation_type(mut self, invocation_type: &'static str) -> Self {
        let mut identity = *self.identity();
        identity.invocation_type = invocation_type;
        self.set_identity(identity);
        self
    }

    pub fn with_key(mut self, key: impl Into<RsxKey>) -> Self {
        let mut identity = *self.identity();
        identity.key = Some(key.into());
        self.set_identity(identity);
        self
    }

    pub fn with_prop(mut self, key: &'static str, value: impl Into<PropValue>) -> Self {
        if let Self::Element(node) = &mut self {
            node.props.push((key, value.into()));
        }
        self
    }

    pub fn with_child(mut self, child: impl IntoRsxNode) -> Self {
        if let Self::Element(node) = &mut self {
            node.children.push(child.into_rsx_node());
        }
        self
    }

    pub fn children(&self) -> Option<&[RsxNode]> {
        match self {
            Self::Element(node) => Some(&node.children),
            Self::Fragment(node) => Some(&node.children),
            Self::Text(_) => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<RsxNode>> {
        match self {
            Self::Element(node) => Some(&mut node.children),
            Self::Fragment(node) => Some(&mut node.children),
            Self::Text(_) => None,
        }
    }

    pub fn tag_descriptor(&self) -> Option<RsxTagDescriptor> {
        match self {
            Self::Element(node) => node.tag_descriptor,
            Self::Text(_) | Self::Fragment(_) => None,
        }
    }
}

pub trait IntoRsxNode {
    fn into_rsx_node(self) -> RsxNode;
}

impl IntoRsxNode for RsxNode {
    fn into_rsx_node(self) -> RsxNode {
        self
    }
}

impl IntoRsxNode for &str {
    fn into_rsx_node(self) -> RsxNode {
        RsxNode::text(self)
    }
}

impl IntoRsxNode for String {
    fn into_rsx_node(self) -> RsxNode {
        RsxNode::text(self)
    }
}

impl IntoRsxNode for Vec<RsxNode> {
    fn into_rsx_node(self) -> RsxNode {
        RsxNode::fragment(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Bool(bool),
    I64(i64),
    F64(f64),
    FontSize(FontSize),
    String(String),
    OnMouseDown(MouseDownHandlerProp),
    OnMouseUp(MouseUpHandlerProp),
    OnMouseMove(MouseMoveHandlerProp),
    OnMouseEnter(MouseEnterHandlerProp),
    OnMouseLeave(MouseLeaveHandlerProp),
    OnClick(ClickHandlerProp),
    OnKeyDown(KeyDownHandlerProp),
    OnKeyUp(KeyUpHandlerProp),
    OnFocus(FocusHandlerProp),
    OnBlur(BlurHandlerProp),
    OnTextAreaFocus(TextAreaFocusHandlerProp),
    OnChange(TextChangeHandlerProp),
    OnTextAreaRender(TextAreaRenderHandlerProp),
    TextAlign(TextAlign),
    Shared(SharedPropValue),
}

pub trait IntoPropValue {
    fn into_prop_value(self) -> PropValue;
}

pub trait FromPropValue: Sized {
    fn from_prop_value(value: PropValue) -> Result<Self, String>;
}

impl From<bool> for PropValue {
    fn from(value: bool) -> Self {
        PropValue::Bool(value)
    }
}

impl From<i32> for PropValue {
    fn from(value: i32) -> Self {
        PropValue::I64(value as i64)
    }
}

impl From<i64> for PropValue {
    fn from(value: i64) -> Self {
        PropValue::I64(value)
    }
}

impl From<u32> for PropValue {
    fn from(value: u32) -> Self {
        PropValue::I64(value as i64)
    }
}

impl From<f32> for PropValue {
    fn from(value: f32) -> Self {
        PropValue::F64(value as f64)
    }
}

impl From<f64> for PropValue {
    fn from(value: f64) -> Self {
        PropValue::F64(value)
    }
}

impl From<FontSize> for PropValue {
    fn from(value: FontSize) -> Self {
        PropValue::FontSize(value)
    }
}

impl From<&str> for PropValue {
    fn from(value: &str) -> Self {
        PropValue::String(value.to_string())
    }
}

impl From<String> for PropValue {
    fn from(value: String) -> Self {
        PropValue::String(value)
    }
}

impl From<MouseDownHandlerProp> for PropValue {
    fn from(value: MouseDownHandlerProp) -> Self {
        PropValue::OnMouseDown(value)
    }
}

impl From<MouseUpHandlerProp> for PropValue {
    fn from(value: MouseUpHandlerProp) -> Self {
        PropValue::OnMouseUp(value)
    }
}

impl From<MouseMoveHandlerProp> for PropValue {
    fn from(value: MouseMoveHandlerProp) -> Self {
        PropValue::OnMouseMove(value)
    }
}

impl From<MouseEnterHandlerProp> for PropValue {
    fn from(value: MouseEnterHandlerProp) -> Self {
        PropValue::OnMouseEnter(value)
    }
}

impl From<MouseLeaveHandlerProp> for PropValue {
    fn from(value: MouseLeaveHandlerProp) -> Self {
        PropValue::OnMouseLeave(value)
    }
}

impl From<ClickHandlerProp> for PropValue {
    fn from(value: ClickHandlerProp) -> Self {
        PropValue::OnClick(value)
    }
}

impl From<KeyDownHandlerProp> for PropValue {
    fn from(value: KeyDownHandlerProp) -> Self {
        PropValue::OnKeyDown(value)
    }
}

impl From<KeyUpHandlerProp> for PropValue {
    fn from(value: KeyUpHandlerProp) -> Self {
        PropValue::OnKeyUp(value)
    }
}

impl From<FocusHandlerProp> for PropValue {
    fn from(value: FocusHandlerProp) -> Self {
        PropValue::OnFocus(value)
    }
}

impl From<BlurHandlerProp> for PropValue {
    fn from(value: BlurHandlerProp) -> Self {
        PropValue::OnBlur(value)
    }
}

impl From<TextAreaFocusHandlerProp> for PropValue {
    fn from(value: TextAreaFocusHandlerProp) -> Self {
        PropValue::OnTextAreaFocus(value)
    }
}

impl From<TextChangeHandlerProp> for PropValue {
    fn from(value: TextChangeHandlerProp) -> Self {
        PropValue::OnChange(value)
    }
}

impl From<TextAreaRenderHandlerProp> for PropValue {
    fn from(value: TextAreaRenderHandlerProp) -> Self {
        PropValue::OnTextAreaRender(value)
    }
}

impl From<TextAlign> for PropValue {
    fn from(value: TextAlign) -> Self {
        PropValue::TextAlign(value)
    }
}

impl IntoPropValue for PropValue {
    fn into_prop_value(self) -> PropValue {
        self
    }
}

impl IntoPropValue for bool {
    fn into_prop_value(self) -> PropValue {
        PropValue::Bool(self)
    }
}

impl IntoPropValue for i32 {
    fn into_prop_value(self) -> PropValue {
        PropValue::I64(self as i64)
    }
}

impl IntoPropValue for i64 {
    fn into_prop_value(self) -> PropValue {
        PropValue::I64(self)
    }
}

impl IntoPropValue for u32 {
    fn into_prop_value(self) -> PropValue {
        PropValue::I64(self as i64)
    }
}

impl IntoPropValue for f32 {
    fn into_prop_value(self) -> PropValue {
        PropValue::F64(self as f64)
    }
}

impl IntoPropValue for f64 {
    fn into_prop_value(self) -> PropValue {
        PropValue::F64(self)
    }
}

impl IntoPropValue for FontSize {
    fn into_prop_value(self) -> PropValue {
        PropValue::FontSize(self)
    }
}

impl IntoPropValue for &str {
    fn into_prop_value(self) -> PropValue {
        PropValue::String(self.to_string())
    }
}

impl IntoPropValue for String {
    fn into_prop_value(self) -> PropValue {
        PropValue::String(self)
    }
}

impl IntoPropValue for MouseDownHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnMouseDown(self)
    }
}

impl IntoPropValue for MouseUpHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnMouseUp(self)
    }
}

impl IntoPropValue for MouseMoveHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnMouseMove(self)
    }
}

impl IntoPropValue for MouseEnterHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnMouseEnter(self)
    }
}

impl IntoPropValue for MouseLeaveHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnMouseLeave(self)
    }
}

impl IntoPropValue for ClickHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnClick(self)
    }
}

impl IntoPropValue for KeyDownHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnKeyDown(self)
    }
}

impl IntoPropValue for KeyUpHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnKeyUp(self)
    }
}

impl IntoPropValue for FocusHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnFocus(self)
    }
}

impl IntoPropValue for BlurHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnBlur(self)
    }
}

impl IntoPropValue for TextAreaFocusHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnTextAreaFocus(self)
    }
}

impl IntoPropValue for TextChangeHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnChange(self)
    }
}

impl IntoPropValue for TextAlign {
    fn into_prop_value(self) -> PropValue {
        PropValue::TextAlign(self)
    }
}

impl FromPropValue for PropValue {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        Ok(value)
    }
}

impl FromPropValue for String {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::String(v) => Ok(v),
            _ => Err("expected string value".to_string()),
        }
    }
}

impl FromPropValue for bool {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::Bool(v) => Ok(v),
            _ => Err("expected bool value".to_string()),
        }
    }
}

impl FromPropValue for f64 {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::I64(v) => Ok(v as f64),
            PropValue::F64(v) => Ok(v),
            _ => Err("expected numeric value".to_string()),
        }
    }
}

impl FromPropValue for FontSize {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::FontSize(v) => Ok(v),
            _ => Err("expected FontSize value".to_string()),
        }
    }
}

impl FromPropValue for MouseDownHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnMouseDown(v) => Ok(v),
            _ => Err("expected mouse down handler value".to_string()),
        }
    }
}

impl FromPropValue for MouseUpHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnMouseUp(v) => Ok(v),
            _ => Err("expected mouse up handler value".to_string()),
        }
    }
}

impl FromPropValue for MouseMoveHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnMouseMove(v) => Ok(v),
            _ => Err("expected mouse move handler value".to_string()),
        }
    }
}

impl FromPropValue for MouseEnterHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnMouseEnter(v) => Ok(v),
            _ => Err("expected mouse enter handler value".to_string()),
        }
    }
}

impl FromPropValue for MouseLeaveHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnMouseLeave(v) => Ok(v),
            _ => Err("expected mouse leave handler value".to_string()),
        }
    }
}

impl FromPropValue for ClickHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnClick(v) => Ok(v),
            _ => Err("expected click handler value".to_string()),
        }
    }
}

impl FromPropValue for KeyDownHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnKeyDown(v) => Ok(v),
            _ => Err("expected key down handler value".to_string()),
        }
    }
}

impl FromPropValue for KeyUpHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnKeyUp(v) => Ok(v),
            _ => Err("expected key up handler value".to_string()),
        }
    }
}

impl FromPropValue for FocusHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnFocus(v) => Ok(v),
            _ => Err("expected focus handler value".to_string()),
        }
    }
}

impl FromPropValue for BlurHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnBlur(v) => Ok(v),
            _ => Err("expected blur handler value".to_string()),
        }
    }
}

impl FromPropValue for TextAreaFocusHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnTextAreaFocus(v) => Ok(v),
            _ => Err("expected text area focus handler value".to_string()),
        }
    }
}

impl FromPropValue for TextChangeHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnChange(v) => Ok(v),
            _ => Err("expected change handler value".to_string()),
        }
    }
}

impl FromPropValue for TextAreaRenderHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnTextAreaRender(v) => Ok(v),
            _ => Err("expected textarea render handler value".to_string()),
        }
    }
}

impl FromPropValue for TextAlign {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::TextAlign(v) => Ok(v),
            _ => Err("expected TextAlign value".to_string()),
        }
    }
}

impl<T> IntoPropValue for Rc<T>
where
    T: Any + 'static,
{
    fn into_prop_value(self) -> PropValue {
        PropValue::Shared(SharedPropValue::new(self))
    }
}

impl<T> FromPropValue for Rc<T>
where
    T: Any + 'static,
{
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::Shared(shared) => shared
                .value()
                .downcast::<T>()
                .map_err(|_| "expected shared prop value of requested type".to_string()),
            _ => Err("expected  shared prop value".to_string()),
        }
    }
}
