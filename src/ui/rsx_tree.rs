use crate::FontSize;
use crate::Style;
use crate::TextAlign;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp,
    MouseDownHandlerProp, MouseMoveHandlerProp, MouseUpHandlerProp,
};
use std::any::Any;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, PartialEq)]
pub enum RsxNode {
    Element(RsxElementNode),
    Text(String),
    Fragment(Vec<RsxNode>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsxElementNode {
    pub tag: String,
    pub props: Vec<(String, PropValue)>,
    pub children: Vec<RsxNode>,
}

#[derive(Clone)]
pub struct SharedPropValue {
    id: u64,
    value: Rc<dyn Any>,
}

impl SharedPropValue {
    pub fn new(value: Rc<dyn Any>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            value,
        }
    }

    pub fn value(&self) -> Rc<dyn Any> {
        self.value.clone()
    }
}

impl fmt::Debug for SharedPropValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedPropValue")
            .field("id", &self.id)
            .finish()
    }
}

impl PartialEq for SharedPropValue {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RsxProps {
    entries: Vec<(String, PropValue)>,
}

impl RsxProps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, key: impl Into<String>, value: PropValue) {
        self.entries.push((key.into(), value));
    }

    pub fn into_entries(self) -> Vec<(String, PropValue)> {
        self.entries
    }

    pub fn remove_raw(&mut self, key: &str) -> Option<PropValue> {
        let index = self.entries.iter().position(|(k, _)| k == key)?;
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
        if let Some(index) = self.entries.iter().position(|(k, _)| k == key) {
            let (_, value) = self.entries.swap_remove(index);
            return match value {
                PropValue::String(v) => Ok(Some(v)),
                _ => Err(format!("prop `{key}` expects string value")),
            };
        }
        Ok(None)
    }

    pub fn remove_f64(&mut self, key: &str) -> Result<Option<f64>, String> {
        if let Some(index) = self.entries.iter().position(|(k, _)| k == key) {
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
        if let Some(index) = self.entries.iter().position(|(k, _)| k == key) {
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
    pub fn element(tag: impl Into<String>) -> Self {
        Self::Element(RsxElementNode {
            tag: tag.into(),
            props: Vec::new(),
            children: Vec::new(),
        })
    }

    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }

    pub fn fragment(children: Vec<RsxNode>) -> Self {
        Self::Fragment(children)
    }

    pub fn with_prop(mut self, key: impl Into<String>, value: impl Into<PropValue>) -> Self {
        if let Self::Element(node) = &mut self {
            node.props.push((key.into(), value.into()));
        }
        self
    }

    pub fn with_child(mut self, child: impl IntoRsxNode) -> Self {
        if let Self::Element(node) = &mut self {
            node.children.push(child.into_rsx_node());
        }
        self
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
    Style(Style),
    OnMouseDown(MouseDownHandlerProp),
    OnMouseUp(MouseUpHandlerProp),
    OnMouseMove(MouseMoveHandlerProp),
    OnClick(ClickHandlerProp),
    OnKeyDown(KeyDownHandlerProp),
    OnKeyUp(KeyUpHandlerProp),
    OnFocus(FocusHandlerProp),
    OnBlur(BlurHandlerProp),
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

impl From<Style> for PropValue {
    fn from(value: Style) -> Self {
        PropValue::Style(value)
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

impl IntoPropValue for Style {
    fn into_prop_value(self) -> PropValue {
        PropValue::Style(self)
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
            PropValue::I64(v) => Ok(FontSize::px(v as f32)),
            PropValue::F64(v) => Ok(FontSize::px(v as f32)),
            _ => Err("expected FontSize value".to_string()),
        }
    }
}

impl FromPropValue for f32 {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        Ok(f64::from_prop_value(value)? as f32)
    }
}

impl FromPropValue for i64 {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        Ok(f64::from_prop_value(value)? as i64)
    }
}

impl FromPropValue for i32 {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        Ok(f64::from_prop_value(value)? as i32)
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

impl<T: Clone + 'static> IntoPropValue for Vec<T> {
    fn into_prop_value(self) -> PropValue {
        let erased: Rc<dyn Any> = Rc::new(self);
        PropValue::Shared(SharedPropValue::new(erased))
    }
}

impl<T: Clone + 'static> FromPropValue for Vec<T> {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::Shared(shared) => {
                let erased = shared.value();
                let vec = Rc::downcast::<Vec<T>>(erased)
                    .map_err(|_| "expected Vec value with matching type".to_string())?;
                Ok((*vec).clone())
            }
            _ => Err("expected Vec value".to_string()),
        }
    }
}
