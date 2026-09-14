use rustc_hash::FxHashSet;

use super::{DirtyFlags, Element, ElementTrait};
use crate::view::node_arena::{NodeArena, NodeKey};

/// Immutable inputs admitted for slot reuse. Unknown hosts, callbacks and shared
/// mutable props deliberately keep the replacement path. Style is copied by value
/// here; the global SharedPropValue equality contract stays pointer-based.
#[derive(Clone, PartialEq)]
pub(super) enum StaticSlotNode {
    Text(crate::ui::RsxNodeIdentity, String),
    Fragment(crate::ui::RsxNodeIdentity, Vec<Self>),
    Host(
        crate::ui::RsxNodeIdentity,
        std::any::TypeId,
        Vec<(&'static str, StaticSlotProp)>,
        Vec<Self>,
    ),
}

#[derive(Clone, PartialEq)]
pub(super) enum StaticSlotProp {
    Value(crate::ui::PropValue),
    Style(crate::style::Style),
}

impl StaticSlotNode {
    pub(super) fn from_value(value: &crate::ui::PropValue) -> Option<Self> {
        use crate::ui::FromPropValue;
        Self::from_node(&crate::ui::RsxNode::from_prop_value(value.clone()).ok()?)
    }

    fn from_node(node: &crate::ui::RsxNode) -> Option<Self> {
        use crate::ui::{PropValue, RsxNode};
        use crate::view::renderer_adapter::{as_element_style, as_text_style};
        use std::any::TypeId;
        match node {
            RsxNode::Text(text) => Some(Self::Text(text.identity, text.content.clone())),
            RsxNode::Fragment(f) => Some(Self::Fragment(
                f.identity,
                f.children
                    .iter()
                    .map(Self::from_node)
                    .collect::<Option<_>>()?,
            )),
            RsxNode::Element(el) => {
                let kind = el.tag_descriptor.as_ref()?.type_id;
                let is_text = kind == TypeId::of::<crate::view::tags::Text>();
                if !is_text && kind != TypeId::of::<crate::view::tags::Element>() {
                    return None;
                }
                let props = el
                    .props
                    .iter()
                    .map(|(name, value)| {
                        let prop = if *name == "style" {
                            StaticSlotProp::Style(if is_text {
                                as_text_style(value, name).ok()?
                            } else {
                                as_element_style(value, name).ok()?
                            })
                        } else {
                            match value {
                                PropValue::Bool(_)
                                | PropValue::I64(_)
                                | PropValue::F64(_)
                                | PropValue::FontSize(_)
                                | PropValue::String(_)
                                | PropValue::TextAlign(_) => StaticSlotProp::Value(value.clone()),
                                _ => return None,
                            }
                        };
                        Some((*name, prop))
                    })
                    .collect::<Option<_>>()?;
                Some(Self::Host(
                    el.identity,
                    kind,
                    props,
                    el.children
                        .iter()
                        .map(Self::from_node)
                        .collect::<Option<_>>()?,
                ))
            }
            RsxNode::Component(_) | RsxNode::Provider(_) => None,
        }
    }
}

#[derive(Default)]
pub(super) struct SlotInputs {
    pub(super) path: Vec<u64>,
    pub(super) global_path: Option<crate::ui::GlobalNodePath>,
    pub(super) loading: Option<(
        StaticSlotNode,
        crate::view::renderer_adapter::StyleCascadeContext,
    )>,
    pub(super) error: Option<(
        StaticSlotNode,
        crate::view::renderer_adapter::StyleCascadeContext,
    )>,
}

impl SlotInputs {
    pub(super) fn cold(
        node: &crate::ui::RsxElementNode,
        path: &[u64],
        global_path: Option<crate::ui::GlobalNodePath>,
        inherited: &crate::view::renderer_adapter::StyleCascadeContext,
    ) -> Self {
        let mut inputs = Self {
            path: path.to_vec(),
            global_path,
            ..Self::default()
        };
        for (name, value) in node.props.iter() {
            match *name {
                "loading" => {
                    inputs.loading =
                        StaticSlotNode::from_value(value).map(|node| (node, inherited.clone()))
                }
                "error" => {
                    inputs.error =
                        StaticSlotNode::from_value(value).map(|node| (node, inherited.clone()))
                }
                _ => {}
            }
        }
        inputs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActiveSlot {
    None,
    Loading,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotReplacementError {
    MissingOwner,
    DuplicateRoot(NodeKey),
    MissingRoot(NodeKey),
    WrongParent {
        root: NodeKey,
        actual: Option<NodeKey>,
    },
    AliasedRoot(NodeKey),
    ChildrenMirrorMismatch,
    UnexpectedActiveChildren,
}

pub(super) fn attach_slot_cold(
    active_slot: ActiveSlot,
    target: &mut Vec<NodeKey>,
    roots: Vec<NodeKey>,
) {
    assert_eq!(
        active_slot,
        ActiveSlot::None,
        "cold slot attachment requires an inactive host"
    );
    assert!(
        target.is_empty(),
        "cold slot attachment must not replace an existing slot"
    );
    *target = roots;
}

pub(super) fn sync_active_slot(
    arena: &mut NodeArena,
    owner: Option<NodeKey>,
    element: &mut Element,
    loading_slot: &mut Vec<NodeKey>,
    error_slot: &mut Vec<NodeKey>,
    active_slot: &mut ActiveSlot,
    next_slot: ActiveSlot,
) {
    if *active_slot == next_slot {
        return;
    }

    let next_children = match next_slot {
        ActiveSlot::None => Vec::new(),
        ActiveSlot::Loading => std::mem::take(loading_slot),
        ActiveSlot::Error => std::mem::take(error_slot),
    };
    let previous_children = element.replace_children(arena, next_children);
    if let Some(owner) = owner {
        arena.set_children(owner, element.children().to_vec());
    }
    match *active_slot {
        ActiveSlot::None => {}
        ActiveSlot::Loading => *loading_slot = previous_children,
        ActiveSlot::Error => *error_slot = previous_children,
    }
    *active_slot = next_slot;
}

pub(super) fn replace_slot(
    arena: &mut NodeArena,
    owner: NodeKey,
    element: &mut Element,
    loading_slot: &mut Vec<NodeKey>,
    error_slot: &mut Vec<NodeKey>,
    active_slot: &mut ActiveSlot,
    target_slot: ActiveSlot,
    new_roots: &[NodeKey],
) -> Result<(), SlotReplacementError> {
    // All fallible validation precedes topology mutation. Callers may safely
    // retain the current slot tree when this returns `Err`.
    if !arena.contains_key(owner) {
        return Err(SlotReplacementError::MissingOwner);
    }

    let arena_children = arena.children_of(owner);
    if arena_children != element.children() {
        return Err(SlotReplacementError::ChildrenMirrorMismatch);
    }
    if *active_slot == ActiveSlot::None && !arena_children.is_empty() {
        return Err(SlotReplacementError::UnexpectedActiveChildren);
    }

    let (target_roots, other_roots) = match target_slot {
        ActiveSlot::Loading => (
            if *active_slot == ActiveSlot::Loading {
                element.children()
            } else {
                loading_slot.as_slice()
            },
            if *active_slot == ActiveSlot::Error {
                element.children()
            } else {
                error_slot.as_slice()
            },
        ),
        ActiveSlot::Error => (
            if *active_slot == ActiveSlot::Error {
                element.children()
            } else {
                error_slot.as_slice()
            },
            if *active_slot == ActiveSlot::Loading {
                element.children()
            } else {
                loading_slot.as_slice()
            },
        ),
        ActiveSlot::None => unreachable!("None is not a replaceable resource slot"),
    };

    let exact_noop = new_roots == target_roots;
    let target_roots: FxHashSet<NodeKey> = target_roots.iter().copied().collect();
    let other_roots: FxHashSet<NodeKey> = other_roots.iter().copied().collect();
    let mut unique_roots = FxHashSet::default();
    for &root in new_roots {
        if !unique_roots.insert(root) {
            return Err(SlotReplacementError::DuplicateRoot(root));
        }
        if !arena.contains_key(root) {
            return Err(SlotReplacementError::MissingRoot(root));
        }
        let actual_parent = arena.parent_of(root);
        if actual_parent != Some(owner) {
            return Err(SlotReplacementError::WrongParent {
                root,
                actual: actual_parent,
            });
        }
        if (!exact_noop && target_roots.contains(&root)) || other_roots.contains(&root) {
            return Err(SlotReplacementError::AliasedRoot(root));
        }
    }
    if exact_noop {
        return Ok(());
    }

    // Topology changes only here or in the pre-layout sync above. Resource
    // preparation after layout has no arena access and cannot enter this path.
    sync_active_slot(
        arena,
        Some(owner),
        element,
        loading_slot,
        error_slot,
        active_slot,
        ActiveSlot::None,
    );

    let target = match target_slot {
        ActiveSlot::Loading => loading_slot,
        ActiveSlot::Error => error_slot,
        ActiveSlot::None => unreachable!("None is not a replaceable resource slot"),
    };
    let old_roots = std::mem::take(target);
    for old_root in old_roots {
        arena.remove_subtree(old_root);
    }
    *target = new_roots.to_vec();
    element.mark_layout_dirty();
    arena.mark_dirty(owner, DirtyFlags::ALL);
    Ok(())
}

#[cfg(test)]
mod tests;
