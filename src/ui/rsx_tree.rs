#![allow(missing_docs)]

//! Core RSX node and prop data structures.

use crate::FontSize;
use crate::TextAlign;
use crate::ui::{
    BlurHandlerProp, ClickHandlerProp, ContextMenuHandlerProp, CopyHandlerProp, CutHandlerProp,
    DragEndHandlerProp, DragLeaveHandlerProp, DragOverHandlerProp, DragStartHandlerProp,
    DropHandlerProp, FocusHandlerProp, ImeCommitHandlerProp, ImeDisabledHandlerProp,
    ImeEnabledHandlerProp, KeyDownHandlerProp, KeyUpHandlerProp, PasteHandlerProp,
    PointerDownHandlerProp, PointerEnterHandlerProp, PointerLeaveHandlerProp,
    PointerMoveHandlerProp, PointerUpHandlerProp, TextAreaFocusHandlerProp,
    TextAreaRenderHandlerProp, TextChangeHandlerProp, WheelHandlerProp,
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
    Element(Rc<RsxElementNode>),
    Text(Rc<RsxTextNode>),
    Fragment(Rc<RsxFragmentNode>),
    /// React parity P1: deferred user-component description.
    ///
    /// No producer emits this variant in P1 — it is plumbing for P2 where
    /// `rsx-macro` starts emitting `Component` nodes for user components
    /// and an `unwrap_components` walker invokes `vtable.render` top-down
    /// before the reconciler sees the tree. Match sites across the crate
    /// treat this variant as unreachable (panic) since it should never
    /// survive past `unwrap_components` in normal flow.
    Component(Rc<crate::ui::ComponentNodeInner>),
    /// Walker-ancestry context provider node. Emitted by
    /// [`crate::ui::provide_context_node`]. The walker pushes
    /// `(type_id, value)` onto `CONTEXT_STACK` before recursing into
    /// `child`, pops after, and returns the walked child transparently
    /// — Provider never survives past `unwrap_components`. Match sites
    /// outside the walker treat this variant as unreachable.
    Provider(Rc<RsxProviderNode>),
}

impl RsxNode {
    /// Fast pointer-equality check between two nodes.
    ///
    /// Returns `true` only when both variants hold an [`Rc`] to the exact same
    /// allocation. Used by the reconciler as a bailout fast-path: if the caller
    /// can guarantee a subtree has not been rebuilt, the whole subtree diff can
    /// be skipped.
    pub fn ptr_eq(a: &RsxNode, b: &RsxNode) -> bool {
        match (a, b) {
            (RsxNode::Element(x), RsxNode::Element(y)) => Rc::ptr_eq(x, y),
            (RsxNode::Text(x), RsxNode::Text(y)) => Rc::ptr_eq(x, y),
            (RsxNode::Fragment(x), RsxNode::Fragment(y)) => Rc::ptr_eq(x, y),
            (RsxNode::Component(x), RsxNode::Component(y)) => Rc::ptr_eq(x, y),
            (RsxNode::Provider(x), RsxNode::Provider(y)) => Rc::ptr_eq(x, y),
            _ => false,
        }
    }
}

/// Walker-ancestry context provider. Carries a type-erased value that the
/// walker pushes onto `CONTEXT_STACK` for the duration of its `child`
/// subtree walk. See [`RsxNode::Provider`].
pub struct RsxProviderNode {
    pub identity: RsxNodeIdentity,
    pub type_id: TypeId,
    /// Type-erased provider value. `Rc<dyn Any>` so cloning the node is
    /// cheap and the walker can clone-push the same allocation onto
    /// `CONTEXT_STACK`.
    pub value: Rc<dyn Any>,
    pub child: RsxNode,
}

impl Clone for RsxProviderNode {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity,
            type_id: self.type_id,
            value: Rc::clone(&self.value),
            child: self.child.clone(),
        }
    }
}

impl fmt::Debug for RsxProviderNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RsxProviderNode")
            .field("type_id", &self.type_id)
            .field("value_ptr", &Rc::as_ptr(&self.value))
            .field("child", &self.child)
            .finish()
    }
}

impl PartialEq for RsxProviderNode {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id
            && Rc::ptr_eq(&self.value, &other.value)
            && self.child == other.child
    }
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

/// Shared, reference-counted element props.
///
/// Stored as `Rc<Vec<_>>` so that props can be cheaply cloned and so the
/// reconciler can take a `Rc::ptr_eq` fast path — two elements that reuse the
/// same props allocation (e.g. from a memoized component) skip the O(n) prop
/// diff entirely. Mutation uses `Rc::make_mut` (copy-on-write).
pub type RsxElementProps = Rc<Vec<(&'static str, PropValue)>>;

#[derive(Clone, Debug, PartialEq)]
pub struct RsxElementNode {
    pub identity: RsxNodeIdentity,
    pub tag: &'static str,
    pub tag_descriptor: Option<RsxTagDescriptor>,
    pub props: RsxElementProps,
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

    /// Move out the inner `Rc` without bumping the refcount.
    pub fn into_inner(self) -> Rc<dyn Any> {
        self.value
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
        Self::Element(Rc::new(RsxElementNode {
            identity: RsxNodeIdentity::new(tag, None),
            tag,
            tag_descriptor: None,
            props: Rc::new(Vec::new()),
            children: Vec::new(),
        }))
    }

    pub fn tagged(tag: &'static str, descriptor: RsxTagDescriptor) -> Self {
        Self::Element(Rc::new(RsxElementNode {
            identity: RsxNodeIdentity::new(descriptor.type_name, None),
            tag,
            tag_descriptor: Some(descriptor),
            props: Rc::new(Vec::new()),
            children: Vec::new(),
        }))
    }

    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(Rc::new(RsxTextNode {
            identity: RsxNodeIdentity::new("Text", None),
            content: content.into(),
        }))
    }

    pub fn fragment(children: Vec<RsxNode>) -> Self {
        Self::Fragment(Rc::new(RsxFragmentNode {
            identity: RsxNodeIdentity::new("Fragment", None),
            children,
        }))
    }

    pub fn identity(&self) -> &RsxNodeIdentity {
        match self {
            Self::Element(node) => &node.identity,
            Self::Text(node) => &node.identity,
            Self::Fragment(node) => &node.identity,
            Self::Component(node) => &node.identity,
            Self::Provider(node) => &node.identity,
        }
    }

    pub fn set_identity(&mut self, identity: RsxNodeIdentity) {
        match self {
            Self::Element(node) => Rc::make_mut(node).identity = identity,
            Self::Text(node) => Rc::make_mut(node).identity = identity,
            Self::Fragment(node) => Rc::make_mut(node).identity = identity,
            // P1: ComponentNodeInner is not Clone (owns boxed props).
            // Callers reach this only with Rc::strong_count == 1 because
            // the only P1 producers (none) create the Rc locally.
            Self::Component(node) => {
                Rc::get_mut(node)
                    .expect("Component node identity update requires unique Rc")
                    .identity = identity;
            }
            Self::Provider(node) => Rc::make_mut(node).identity = identity,
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
            let node = Rc::make_mut(node);
            Rc::make_mut(&mut node.props).push((key, value.into()));
        }
        self
    }

    pub fn with_child(mut self, child: impl IntoRsxNode) -> Self {
        if let Self::Element(node) = &mut self {
            Rc::make_mut(node).children.push(child.into_rsx_node());
        }
        self
    }

    pub fn children(&self) -> Option<&[RsxNode]> {
        match self {
            Self::Element(node) => Some(&node.children),
            Self::Fragment(node) => Some(&node.children),
            Self::Component(node) => Some(&node.children),
            Self::Text(_) | Self::Provider(_) => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<RsxNode>> {
        match self {
            Self::Element(node) => Some(&mut Rc::make_mut(node).children),
            Self::Fragment(node) => Some(&mut Rc::make_mut(node).children),
            Self::Component(node) => Some(
                &mut Rc::get_mut(node)
                    .expect("Component node children_mut requires unique Rc")
                    .children,
            ),
            Self::Text(_) | Self::Provider(_) => None,
        }
    }

    pub fn tag_descriptor(&self) -> Option<RsxTagDescriptor> {
        match self {
            Self::Element(node) => node.tag_descriptor,
            Self::Text(_) | Self::Fragment(_) | Self::Component(_) | Self::Provider(_) => None,
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
    OnPointerDown(PointerDownHandlerProp),
    OnPointerUp(PointerUpHandlerProp),
    OnPointerMove(PointerMoveHandlerProp),
    OnPointerEnter(PointerEnterHandlerProp),
    OnPointerLeave(PointerLeaveHandlerProp),
    OnClick(ClickHandlerProp),
    OnContextMenu(ContextMenuHandlerProp),
    OnWheel(WheelHandlerProp),
    OnKeyDown(KeyDownHandlerProp),
    OnKeyUp(KeyUpHandlerProp),
    OnFocus(FocusHandlerProp),
    OnBlur(BlurHandlerProp),
    OnImeCommit(ImeCommitHandlerProp),
    OnImeEnabled(ImeEnabledHandlerProp),
    OnImeDisabled(ImeDisabledHandlerProp),
    OnDragStart(DragStartHandlerProp),
    OnDragOver(DragOverHandlerProp),
    OnDragLeave(DragLeaveHandlerProp),
    OnDrop(DropHandlerProp),
    OnDragEnd(DragEndHandlerProp),
    OnCopy(CopyHandlerProp),
    OnCut(CutHandlerProp),
    OnPaste(PasteHandlerProp),
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

impl From<PointerDownHandlerProp> for PropValue {
    fn from(value: PointerDownHandlerProp) -> Self {
        PropValue::OnPointerDown(value)
    }
}

impl From<PointerUpHandlerProp> for PropValue {
    fn from(value: PointerUpHandlerProp) -> Self {
        PropValue::OnPointerUp(value)
    }
}

impl From<PointerMoveHandlerProp> for PropValue {
    fn from(value: PointerMoveHandlerProp) -> Self {
        PropValue::OnPointerMove(value)
    }
}

impl From<PointerEnterHandlerProp> for PropValue {
    fn from(value: PointerEnterHandlerProp) -> Self {
        PropValue::OnPointerEnter(value)
    }
}

impl From<PointerLeaveHandlerProp> for PropValue {
    fn from(value: PointerLeaveHandlerProp) -> Self {
        PropValue::OnPointerLeave(value)
    }
}

impl From<ClickHandlerProp> for PropValue {
    fn from(value: ClickHandlerProp) -> Self {
        PropValue::OnClick(value)
    }
}

impl From<ContextMenuHandlerProp> for PropValue {
    fn from(value: ContextMenuHandlerProp) -> Self {
        PropValue::OnContextMenu(value)
    }
}

impl From<WheelHandlerProp> for PropValue {
    fn from(value: WheelHandlerProp) -> Self {
        PropValue::OnWheel(value)
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

impl From<ImeCommitHandlerProp> for PropValue {
    fn from(value: ImeCommitHandlerProp) -> Self {
        PropValue::OnImeCommit(value)
    }
}
impl From<ImeEnabledHandlerProp> for PropValue {
    fn from(value: ImeEnabledHandlerProp) -> Self {
        PropValue::OnImeEnabled(value)
    }
}
impl From<ImeDisabledHandlerProp> for PropValue {
    fn from(value: ImeDisabledHandlerProp) -> Self {
        PropValue::OnImeDisabled(value)
    }
}
impl From<DragStartHandlerProp> for PropValue {
    fn from(value: DragStartHandlerProp) -> Self {
        PropValue::OnDragStart(value)
    }
}
impl From<DragOverHandlerProp> for PropValue {
    fn from(value: DragOverHandlerProp) -> Self {
        PropValue::OnDragOver(value)
    }
}
impl From<DragLeaveHandlerProp> for PropValue {
    fn from(value: DragLeaveHandlerProp) -> Self {
        PropValue::OnDragLeave(value)
    }
}
impl From<DropHandlerProp> for PropValue {
    fn from(value: DropHandlerProp) -> Self {
        PropValue::OnDrop(value)
    }
}
impl From<DragEndHandlerProp> for PropValue {
    fn from(value: DragEndHandlerProp) -> Self {
        PropValue::OnDragEnd(value)
    }
}
impl From<CopyHandlerProp> for PropValue {
    fn from(value: CopyHandlerProp) -> Self {
        PropValue::OnCopy(value)
    }
}
impl From<CutHandlerProp> for PropValue {
    fn from(value: CutHandlerProp) -> Self {
        PropValue::OnCut(value)
    }
}
impl From<PasteHandlerProp> for PropValue {
    fn from(value: PasteHandlerProp) -> Self {
        PropValue::OnPaste(value)
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

impl IntoPropValue for PointerDownHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnPointerDown(self)
    }
}

impl IntoPropValue for PointerUpHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnPointerUp(self)
    }
}

impl IntoPropValue for PointerMoveHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnPointerMove(self)
    }
}

impl IntoPropValue for PointerEnterHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnPointerEnter(self)
    }
}

impl IntoPropValue for PointerLeaveHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnPointerLeave(self)
    }
}

impl IntoPropValue for ClickHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnClick(self)
    }
}

impl IntoPropValue for ContextMenuHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnContextMenu(self)
    }
}

impl IntoPropValue for WheelHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnWheel(self)
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

impl IntoPropValue for ImeCommitHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnImeCommit(self)
    }
}
impl IntoPropValue for ImeEnabledHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnImeEnabled(self)
    }
}
impl IntoPropValue for ImeDisabledHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnImeDisabled(self)
    }
}
impl IntoPropValue for DragStartHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnDragStart(self)
    }
}
impl IntoPropValue for DragOverHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnDragOver(self)
    }
}
impl IntoPropValue for DragLeaveHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnDragLeave(self)
    }
}
impl IntoPropValue for DropHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnDrop(self)
    }
}
impl IntoPropValue for DragEndHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnDragEnd(self)
    }
}
impl IntoPropValue for CopyHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnCopy(self)
    }
}
impl IntoPropValue for CutHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnCut(self)
    }
}
impl IntoPropValue for PasteHandlerProp {
    fn into_prop_value(self) -> PropValue {
        PropValue::OnPaste(self)
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

impl FromPropValue for PointerDownHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnPointerDown(v) => Ok(v),
            _ => Err("expected pointer down handler value".to_string()),
        }
    }
}

impl FromPropValue for PointerUpHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnPointerUp(v) => Ok(v),
            _ => Err("expected pointer up handler value".to_string()),
        }
    }
}

impl FromPropValue for PointerMoveHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnPointerMove(v) => Ok(v),
            _ => Err("expected pointer move handler value".to_string()),
        }
    }
}

impl FromPropValue for PointerEnterHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnPointerEnter(v) => Ok(v),
            _ => Err("expected pointer enter handler value".to_string()),
        }
    }
}

impl FromPropValue for PointerLeaveHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnPointerLeave(v) => Ok(v),
            _ => Err("expected pointer leave handler value".to_string()),
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

impl FromPropValue for ContextMenuHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnContextMenu(v) => Ok(v),
            _ => Err("expected context menu handler value".to_string()),
        }
    }
}

impl FromPropValue for WheelHandlerProp {
    fn from_prop_value(value: PropValue) -> Result<Self, String> {
        match value {
            PropValue::OnWheel(v) => Ok(v),
            _ => Err("expected wheel handler value".to_string()),
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

macro_rules! impl_from_prop_value_event {
    ($ty:ident, $variant:ident, $label:expr) => {
        impl FromPropValue for $ty {
            fn from_prop_value(value: PropValue) -> Result<Self, String> {
                match value {
                    PropValue::$variant(v) => Ok(v),
                    _ => Err(concat!("expected ", $label, " handler value").to_string()),
                }
            }
        }
    };
}

impl_from_prop_value_event!(ImeCommitHandlerProp, OnImeCommit, "ime commit");
impl_from_prop_value_event!(ImeEnabledHandlerProp, OnImeEnabled, "ime enabled");
impl_from_prop_value_event!(ImeDisabledHandlerProp, OnImeDisabled, "ime disabled");
impl_from_prop_value_event!(DragStartHandlerProp, OnDragStart, "drag start");
impl_from_prop_value_event!(DragOverHandlerProp, OnDragOver, "drag over");
impl_from_prop_value_event!(DragLeaveHandlerProp, OnDragLeave, "drag leave");
impl_from_prop_value_event!(DropHandlerProp, OnDrop, "drop");
impl_from_prop_value_event!(DragEndHandlerProp, OnDragEnd, "drag end");
impl_from_prop_value_event!(CopyHandlerProp, OnCopy, "copy");
impl_from_prop_value_event!(CutHandlerProp, OnCut, "cut");
impl_from_prop_value_event!(PasteHandlerProp, OnPaste, "paste");

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
