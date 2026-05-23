//! P6 — `reconcile_existing_subtree`.
//!
//! Walks an `(old_rsx, new_rsx)` pair in tandem with the live arena
//! subtree rooted at `anchor` and applies the smallest set of arena
//! ops that brings the live tree into shape with `new_rsx`. Preserves
//! `NodeKey` identity for matched (same-identity) elements so any
//! component state, promoted layer, or `use_state` slot keyed on that
//! NodeKey survives a projection rebuild — the win P5's full teardown
//! gave up.
//!
//! Best-effort: any shape the wrapper can't safely reconcile in place
//! returns `Err` with a short reason. The caller
//! (`rebuild_children_full`) falls back to
//! `arena.remove_subtree` + `commit_projection_segment` for that slot.
//!
//! Currently bails on:
//! * top-level identity mismatch
//! * Fragment / Component children at any level (Fragment is
//!   arena-flattened; Component must already be unwrapped by the
//!   user-component walker before reaching v2 projection)
//! * the inserted-child path producing more than one descriptor

#![allow(dead_code)]

use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::style::Style;
use crate::ui::{PropValue, RsxElementNode, RsxNode, RsxNodeIdentity};
use crate::view::base_component::{Element, ElementTrait, Text};
use crate::view::fiber_work::{ApplyContext, PropApplyOutcome};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::renderer_adapter::{
    StyleCascadeContext, arena_insert_child, arena_remove_child,
    rsx_to_descriptors_scoped_with_context, text_with_text_area_ime_preedit,
};

pub(crate) fn reconcile_existing_subtree(
    arena: &mut NodeArena,
    anchor: NodeKey,
    old: &RsxNode,
    new: &RsxNode,
    apply_ctx: &ApplyContext<'_>,
    inherited_style: &Style,
    scope: &[u64],
) -> Result<(), &'static str> {
    let result = with_new_provider_contexts(new, |new_inner| {
        reconcile_existing_subtree_inner(
            arena,
            anchor,
            old,
            new_inner,
            apply_ctx,
            inherited_style,
            scope,
        )
    });
    // Reconcile only diffs RSX props; cascading text props (font / color
    // / text_wrap / cursor) flow through `StyleCascadeContext`, which a
    // pure prop-diff would miss when a TextArea-side cascade input
    // (e.g. `auto_wrap`) flips between renders. Replay the cascade onto
    // the live subtree so existing Text leaves pick up the new values.
    if result.is_ok() {
        let inherited = StyleCascadeContext::from_viewport_style(
            inherited_style,
            apply_ctx.viewport_width,
            apply_ctx.viewport_height,
        );
        cascade_style_cascade(arena, anchor, &inherited);
    }
    result
}

fn cascade_style_cascade(arena: &mut NodeArena, key: NodeKey, inherited: &StyleCascadeContext) {
    let mut child_inherited: Option<StyleCascadeContext> = None;
    arena.mutate_element_with_invalidation(key, |element, cx| {
        element.apply_inherited(inherited);
        if let Some(el) = element.as_any().downcast_ref::<Element>() {
            child_inherited = Some(el.child_style_cascade(inherited));
        }
        cx.invalidate(element.local_dirty_flags());
    });
    let child_cascade = child_inherited.unwrap_or_else(|| inherited.clone());
    let children = arena.children_of(key);
    for child_key in children {
        cascade_style_cascade(arena, child_key, &child_cascade);
    }
}

fn reconcile_existing_subtree_inner(
    arena: &mut NodeArena,
    anchor: NodeKey,
    old: &RsxNode,
    new_inner: &RsxNode,
    apply_ctx: &ApplyContext<'_>,
    inherited_style: &Style,
    scope: &[u64],
) -> Result<(), &'static str> {
    let old_inner = unwrap_providers(old);

    if old_inner.identity() != new_inner.identity() {
        return Err("identity mismatch at subtree root");
    }
    match (old_inner, new_inner) {
        (RsxNode::Element(o), RsxNode::Element(n)) => {
            reconcile_element_pair(arena, anchor, o, n, apply_ctx, inherited_style, scope)
        }
        (RsxNode::Text(_o), RsxNode::Text(n)) => {
            let new_content = text_with_text_area_ime_preedit(n.content.clone());
            arena.mutate_element_with_invalidation(anchor, |el, cx| {
                if let Some(text) = el.as_any_mut().downcast_mut::<Text>() {
                    text.set_text(new_content);
                    cx.invalidate(text.local_dirty_flags());
                }
            });
            Ok(())
        }
        _ => Err("unsupported subtree variant"),
    }
}

fn with_new_provider_contexts<R>(node: &RsxNode, f: impl FnOnce(&RsxNode) -> R) -> R {
    match node {
        RsxNode::Provider(provider) => {
            crate::ui::with_pushed_context_raw(provider.type_id, Rc::clone(&provider.value), || {
                with_new_provider_contexts(&provider.child, f)
            })
        }
        _ => f(node),
    }
}

fn unwrap_providers(node: &RsxNode) -> &RsxNode {
    let mut cursor = node;
    while let RsxNode::Provider(p) = cursor {
        cursor = &p.child;
    }
    cursor
}

fn reconcile_element_pair(
    arena: &mut NodeArena,
    anchor: NodeKey,
    old: &Rc<RsxElementNode>,
    new: &Rc<RsxElementNode>,
    apply_ctx: &ApplyContext<'_>,
    inherited_style: &Style,
    scope: &[u64],
) -> Result<(), &'static str> {
    let same_tag = match (old.tag_descriptor, new.tag_descriptor) {
        (Some(a), Some(b)) => a == b,
        _ => old.tag == new.tag,
    };
    if !same_tag {
        return Err("tag mismatch");
    }

    if !Rc::ptr_eq(&old.props, &new.props) {
        diff_and_apply_props(arena, anchor, &old.props, &new.props, apply_ctx)?;
    }

    // `<Text>` (and other host tags that flatten RSX children into a
    // single string content prop on the element itself) leaves the
    // arena slot childless. Walk the new RSX children, fold them into
    // a single string, and push that into the Text element directly
    // via `set_text`. Skips the per-child reconcile pass below, which
    // would otherwise hit `arena/old child count mismatch` immediately.
    if new.tag == "Text" {
        let mut content = String::new();
        for child in &new.children {
            flatten_text_children(child, &mut content);
        }
        content = text_with_text_area_ime_preedit(content);
        arena.mutate_element_with_invalidation(anchor, |el, cx| {
            if let Some(text) = el.as_any_mut().downcast_mut::<Text>() {
                text.set_text(content);
                cx.invalidate(text.local_dirty_flags());
            }
        });
        return Ok(());
    }

    reconcile_children_subtree(
        arena,
        anchor,
        &old.children,
        &new.children,
        apply_ctx,
        inherited_style,
        scope,
    )
}

fn flatten_text_children(node: &RsxNode, out: &mut String) {
    match node {
        RsxNode::Text(t) => out.push_str(&t.content),
        RsxNode::Fragment(f) => {
            for c in &f.children {
                flatten_text_children(c, out);
            }
        }
        RsxNode::Provider(p) => flatten_text_children(&p.child, out),
        RsxNode::Element(_) | RsxNode::Component(_) => {}
    }
}

fn diff_and_apply_props(
    arena: &mut NodeArena,
    anchor: NodeKey,
    old_props: &[(&'static str, PropValue)],
    new_props: &[(&'static str, PropValue)],
    apply_ctx: &ApplyContext<'_>,
) -> Result<(), &'static str> {
    let mut changed: Vec<(&'static str, PropValue)> = Vec::new();
    let mut removed: Vec<&'static str> = Vec::new();
    for (k, ov) in old_props.iter() {
        match new_props.iter().find(|(nk, _)| nk == k) {
            Some((_, nv)) if nv != ov => changed.push((*k, nv.clone())),
            None => removed.push(*k),
            _ => {}
        }
    }
    for (k, nv) in new_props.iter() {
        if !old_props.iter().any(|(ok, _)| ok == k) {
            changed.push((*k, nv.clone()));
        }
    }
    if changed.is_empty() && removed.is_empty() {
        return Ok(());
    }

    let mut all_ok = true;
    arena.mutate_element_with_invalidation(anchor, |element, cx| {
        for (name, value) in changed {
            match element.apply_prop(cx.arena(), anchor, apply_ctx, name, value) {
                PropApplyOutcome::Applied | PropApplyOutcome::NeedsCascade => {}
                PropApplyOutcome::UnknownProp
                | PropApplyOutcome::DecodeFailed(_)
                | PropApplyOutcome::CannotReset(_) => {
                    all_ok = false;
                }
            }
        }
        for name in removed {
            match element.reset_prop(cx.arena(), anchor, apply_ctx, name) {
                PropApplyOutcome::Applied | PropApplyOutcome::NeedsCascade => {}
                PropApplyOutcome::UnknownProp
                | PropApplyOutcome::DecodeFailed(_)
                | PropApplyOutcome::CannotReset(_) => {
                    all_ok = false;
                }
            }
        }
        cx.invalidate(element.local_dirty_flags());
    });
    if all_ok {
        Ok(())
    } else {
        Err("prop apply failed")
    }
}

enum ChildSlotPlan<'a> {
    Reuse(NodeKey, usize), // (existing key, old_index)
    Insert(&'a RsxNode),
}

fn reconcile_children_subtree(
    arena: &mut NodeArena,
    parent: NodeKey,
    old_children: &[RsxNode],
    new_children: &[RsxNode],
    apply_ctx: &ApplyContext<'_>,
    inherited_style: &Style,
    scope: &[u64],
) -> Result<(), &'static str> {
    for c in old_children.iter().chain(new_children.iter()) {
        match unwrap_providers(c) {
            RsxNode::Fragment(_) | RsxNode::Component(_) => {
                return Err("fragment/component child not supported");
            }
            _ => {}
        }
    }

    let arena_kids_old = arena.children_of(parent);
    if arena_kids_old.len() != old_children.len() {
        return Err("arena/old child count mismatch");
    }

    // Identity-keyed matching (post-Provider-unwrap).
    let mut old_keyed: FxHashMap<RsxNodeIdentity, Vec<usize>> = FxHashMap::default();
    let mut old_unkeyed: FxHashMap<&'static str, Vec<usize>> = FxHashMap::default();
    for (idx, c) in old_children.iter().enumerate() {
        let id = *unwrap_providers(c).identity();
        if id.key.is_some() {
            old_keyed.entry(id).or_default().push(idx);
        } else {
            old_unkeyed.entry(id.invocation_type).or_default().push(idx);
        }
    }

    let mut matches: Vec<Option<usize>> = Vec::with_capacity(new_children.len());
    let mut matched_old = vec![false; old_children.len()];
    for nc in new_children.iter() {
        let id = *unwrap_providers(nc).identity();
        let m = if id.key.is_some() {
            old_keyed.get_mut(&id).and_then(|q| q.pop())
        } else {
            old_unkeyed
                .get_mut(id.invocation_type)
                .and_then(|q| q.pop())
        };
        if let Some(idx) = m {
            matched_old[idx] = true;
        }
        matches.push(m);
    }

    // Build per-slot plan.
    let plan: Vec<ChildSlotPlan<'_>> = matches
        .iter()
        .enumerate()
        .map(|(new_idx, m)| match m {
            Some(old_idx) => ChildSlotPlan::Reuse(arena_kids_old[*old_idx], *old_idx),
            None => ChildSlotPlan::Insert(&new_children[new_idx]),
        })
        .collect();

    // Pass 1: recursively reconcile matched pairs (props/children) before
    // any structural mutation, so old-index → live-key mapping is still
    // valid.
    for (new_idx, slot) in plan.iter().enumerate() {
        if let ChildSlotPlan::Reuse(key, old_idx) = slot {
            let child_scope_tail = scope_for_child(scope, new_idx);
            reconcile_existing_subtree(
                arena,
                *key,
                &old_children[*old_idx],
                &new_children[new_idx],
                apply_ctx,
                inherited_style,
                &child_scope_tail,
            )?;
        }
    }

    // Pass 2: drop unmatched old children. Walk in reverse so live
    // indices stay valid.
    for old_idx in (0..old_children.len()).rev() {
        if matched_old[old_idx] {
            continue;
        }
        let target_key = arena_kids_old[old_idx];
        let live = arena.children_of(parent);
        if let Some(pos) = live.iter().position(|&k| k == target_key) {
            arena_remove_child(arena, parent, pos);
        }
    }

    // Pass 3: walk new order left-to-right, moving matched keys into
    // place and inserting fresh subtrees for unmatched slots.
    for (target_idx, slot) in plan.iter().enumerate() {
        match slot {
            ChildSlotPlan::Reuse(key, _old_idx) => {
                let live = arena.children_of(parent);
                let cur_pos = live
                    .iter()
                    .position(|&k| k == *key)
                    .ok_or("matched key vanished")?;
                if cur_pos != target_idx {
                    arena_move_child(arena, parent, cur_pos, target_idx);
                }
            }
            ChildSlotPlan::Insert(node) => {
                let child_scope_tail = scope_for_child(scope, target_idx);
                let mut descs = descriptors_unwrap_providers(
                    node,
                    &child_scope_tail,
                    inherited_style,
                    apply_ctx.viewport_width,
                    apply_ctx.viewport_height,
                )
                .map_err(|_| "descriptor build failed")?;
                if descs.len() != 1 {
                    return Err("multi-descriptor projection child not supported");
                }
                arena_insert_child(arena, parent, target_idx, descs.remove(0));
            }
        }
    }

    Ok(())
}

fn arena_move_child(arena: &mut NodeArena, parent: NodeKey, from: usize, to: usize) {
    let mut children = arena.children_of(parent);
    if from >= children.len() {
        return;
    }
    let key = children.remove(from);
    let target = to.min(children.len());
    children.insert(target, key);
    arena.set_children(parent, children.clone());
    arena.mutate_element_with_invalidation(parent, |element, cx| {
        if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
            let _previous = el.replace_children(cx.arena(), children);
        } else if let Some(mirror) = element.children_mut() {
            *mirror = children;
        }
        cx.invalidate(crate::view::base_component::DirtyFlags::ALL);
    });
}

fn scope_for_child(parent_scope: &[u64], child_index: usize) -> Vec<u64> {
    let mut s = Vec::with_capacity(parent_scope.len() + 1);
    s.extend_from_slice(parent_scope);
    s.push(child_index as u64);
    s
}

/// Mirror of `projection::descriptors_unwrap_providers`. Inlined here
/// to keep `reconcile.rs` independent of `projection.rs`'s private
/// free fn.
fn descriptors_unwrap_providers(
    node: &RsxNode,
    scope: &[u64],
    inherited_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<Vec<crate::view::renderer_adapter::ElementDescriptor>, String> {
    if let RsxNode::Provider(provider) = node {
        crate::ui::with_pushed_context_raw(
            provider.type_id,
            std::rc::Rc::clone(&provider.value),
            || {
                descriptors_unwrap_providers(
                    &provider.child,
                    scope,
                    inherited_style,
                    viewport_width,
                    viewport_height,
                )
            },
        )
    } else {
        rsx_to_descriptors_scoped_with_context(
            node,
            scope,
            inherited_style,
            viewport_width,
            viewport_height,
        )
    }
}
