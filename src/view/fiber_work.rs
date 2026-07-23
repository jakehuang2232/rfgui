//! Fiber work-unit model, `Patch -> FiberWork` translation, and
//! incremental commit application.
//!
//! # Why this exists
//!
//! The React-alignment refactor splits the retained tree into a Fiber
//! layer (logical nodes, identity, props diff) sitting on top of the
//! existing `NodeArena` host instances. Reconciler output today is a
//! `Vec<Patch>` that walks *index paths* from the RSX root. The Fiber
//! layer wants *keyed work units* anchored on `NodeKey` / `stable_id`
//! so the commit phase can be scheduled, batched, and (later)
//! interrupted.
//!
//! The current viewport path enables incremental commit by default for
//! work that can be translated and applied safely, and falls back to a
//! full rebuild otherwise. This module provides:
//!
//! 1. `FiberWork` — a parallel representation of patches anchored on
//!    arena keys.
//! 2. `patch_to_fiber_work` — a best-effort translator. Returns `None`
//!    for patch variants whose faithful translation requires context
//!    the caller does not have; those fall through to the full rebuild
//!    path.
//! 3. `apply_fiber_works` — the committer for Create/Delete/Move/
//!    Replace/Update/SetText work that is safe for the current arena.

use rustc_hash::FxHashMap;

use crate::style::Style;
use crate::ui::{Patch, PropValue, RsxElementNode, RsxNode};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::renderer_adapter::{
    ElementDescriptor, arena_insert_child, arena_remove_child, commit_descriptor_tree,
    resolve_font_size_prop_with_inherited, resolve_path,
};

/// Context needed to translate descriptor-producing patches
/// (`InsertChild`, and eventually `ReplaceRoot` / `ReplaceNode`) into
/// `FiberWork::Create`. Passed `None` when the caller only wants the
/// subset that does not build new subtrees.
///
/// Fields mirror the inputs to
/// [`renderer_adapter::rsx_to_descriptors_scoped_with_context`] so the
/// translator can reuse the cold-path converter verbatim.
/// 軌 A #9: viewport-level context threaded into the apply path so
/// setters that depend on inherited cascade (Text/TextArea
/// `font_size` em/rem/%/vw/vh) can rebuild an `StyleCascadeContext`
/// from the arena ancestors.
///
/// Mirrors the inputs to
/// `renderer_adapter::style_cascade_at_parent` so the Update
/// dispatchers can reuse the cold-path resolver verbatim.
#[derive(Clone, Copy)]
pub struct ApplyContext<'a> {
    pub viewport_style: &'a Style,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

pub struct DescriptorContext<'a> {
    /// The RSX root *after* this reconcile step — so the translator
    /// can walk `parent_path` to find the freshly-authored child.
    pub new_rsx_root: &'a RsxNode,
    /// 軌 1 #6: the RSX root *before* this reconcile step. When
    /// present, descriptor-producing translators run an
    /// identity-validated walk along `parent_path` (both trees
    /// stepped in lockstep, abort on identity mismatch). `None`
    /// disables the check — callers that don't have an old tree on
    /// hand (unit tests) keep the original happy-path behaviour.
    pub old_rsx_root: Option<&'a RsxNode>,
    /// Viewport-level base style for the cascade. Currently passed
    /// straight into `rsx_to_descriptors_scoped_with_context`; when
    /// M5 ships inherited-cascade reconstruction this becomes the
    /// parent's accumulated style instead.
    pub inherited_style: &'a Style,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Walk `root` along `index_path` (OLD-tree child indices from the
/// reconciler). Returns `None` if any index is out of bounds or the
/// reached node doesn't have a `children()` slot (text leaves).
///
/// M5 #5/#6 happy-path assumption: the NEW tree has the same index
/// structure above the insertion point as the OLD tree. If that
/// breaks (structural change higher up), the walk silently misaligns;
/// the resulting descriptor would be wrong. We guard by returning
/// `None` when indices don't exist, but **do not** currently validate
/// node identity along the way. Callers should downgrade to the full
/// rebuild on any irregular patch shape.
/// Count this node's flattened arena-leaf descendants. Fragments are
/// transparent — they contribute the sum of their children. Element /
/// Count this node's flattened arena-leaf descendants. Fragments are
/// transparent — they contribute the sum of their children. Element /
/// Text nodes contribute exactly one leaf. Used by `rsx_to_arena_path`
/// to skip Fragment indices that reconciler paths retain but the arena
/// flattens away.
fn count_arena_leaves(node: &RsxNode) -> usize {
    match node {
        RsxNode::Fragment(frag) => frag.children.iter().map(count_arena_leaves).sum(),
        _ => 1,
    }
}

/// Translate a reconciler-space `rsx_path` (indices through rsx
/// children including Fragment nodes) to an arena-space path (indices
/// through the flattened arena children). Walks `root` rsx tree,
/// summing flattened leaf counts for preceding siblings whenever
/// descending past a Fragment.
///
/// Returns `None` if the path strays off the rsx tree (out-of-bounds
/// or hits a childless node too early). Terminates on rsx-leaf targets
/// (Elements) by emitting a final arena step and stopping; terminates
/// on Fragment targets without emitting a final step (Fragment has no
/// arena presence of its own).
///
/// Resolution of an rsx-space path against the arena-flattened tree.
pub(crate) enum ArenaPathResolution {
    /// Path resolves to an arena node at this index chain.
    Arena(Vec<usize>),
    /// Path is malformed (out of bounds / hits a childless node
    /// prematurely).
    Invalid,
}

pub(crate) fn rsx_to_arena_path(root: &RsxNode, rsx_path: &[usize]) -> ArenaPathResolution {
    let mut arena_path = Vec::with_capacity(rsx_path.len());
    let mut node = root;
    let mut offset: usize = 0;
    for &rsx_idx in rsx_path {
        let Some(children) = node.children() else {
            return ArenaPathResolution::Invalid;
        };
        if rsx_idx >= children.len() {
            return ArenaPathResolution::Invalid;
        }
        for child in &children[..rsx_idx] {
            offset += count_arena_leaves(child);
        }
        let target = &children[rsx_idx];
        match target {
            RsxNode::Fragment(_) => {
                // Fragment is arena-transparent: stay at the same
                // arena level, carrying `offset` forward so subsequent
                // rsx siblings inside this Fragment resolve into the
                // parent's flattened arena child list.
                node = target;
            }
            _ => {
                arena_path.push(offset);
                offset = 0;
                node = target;
            }
        }
    }
    ArenaPathResolution::Arena(arena_path)
}

fn walk_rsx_by_index_path<'a>(root: &'a RsxNode, index_path: &[usize]) -> Option<&'a RsxNode> {
    let mut node = root;
    for &i in index_path {
        let children = node.children()?;
        node = children.get(i)?;
    }
    Some(node)
}

/// 軌 1 #6: identity-validated lockstep walk of OLD and NEW trees
/// along `index_path`. At each step the OLD child's identity is
/// compared with the NEW child's identity; any mismatch returns
/// `None` so the caller drops the patch and falls back to a full
/// rebuild instead of authoring against a misaligned subtree.
///
/// Returns the NEW node at the path (the OLD node is only used for
/// the identity comparison and discarded).
fn walk_rsx_by_index_path_validated<'a>(
    old_root: &RsxNode,
    new_root: &'a RsxNode,
    index_path: &[usize],
) -> Option<&'a RsxNode> {
    // Root identity must match for the path to be meaningful — a
    // root-type swap would otherwise be silently accepted.
    if old_root.identity() != new_root.identity() {
        return None;
    }
    let mut old_node = old_root;
    let mut new_node = new_root;
    for &i in index_path {
        let old_children = old_node.children()?;
        let new_children = new_node.children()?;
        let old_child = old_children.get(i)?;
        let new_child = new_children.get(i)?;
        if old_child.identity() != new_child.identity() {
            return None;
        }
        old_node = old_child;
        new_node = new_child;
    }
    Some(new_node)
}

/// A single unit of reconciliation work, keyed on arena handles rather
/// than RSX path indices.
///
/// Not `Clone` / `Debug` on purpose — `ElementDescriptor` is neither,
/// and the commit pipeline always consumes works by-move.
pub enum FiberWork {
    /// Insert `descriptor` as the `index`-th child of `parent` (or as a
    /// new root when `parent` is `None`). `stable_id` is the expected
    /// stable id of the freshly-committed root, carried alongside so
    /// the caller can re-index without re-walking the element.
    Create {
        parent: Option<NodeKey>,
        index: usize,
        descriptor: ElementDescriptor,
        stable_id: u64,
    },
    /// Apply a prop diff to an existing element through the incremental
    /// setter layer.
    Update {
        key: NodeKey,
        changed: Vec<(&'static str, PropValue)>,
        removed: Vec<&'static str>,
    },
    /// Replace a text-node or TextArea host's content.
    SetText { key: NodeKey, text: String },
    /// Reorder child within `parent`.
    Move {
        parent: NodeKey,
        key: NodeKey,
        from: usize,
        to: usize,
    },
    /// Remove a subtree.
    Delete {
        parent: Option<NodeKey>,
        key: NodeKey,
    },
    /// Wholesale replace the arena root *set* with a freshly-built
    /// descriptor list. Emitted for `Patch::ReplaceAllRoots` only —
    /// every old root subtree is dropped and the new descriptors
    /// committed as roots in order; `ui_root_keys` is refreshed by the
    /// apply caller from `arena.roots()` after. The descriptor list has
    /// N >= 1 entries — Fragment roots produce multiple.
    ReplaceAllRoots { descriptors: Vec<ElementDescriptor> },
    /// Replace exactly one root — the subtree currently rooted at
    /// `key` — with N freshly-built descriptors (N >= 1; a Fragment
    /// root node yields multiple). Emitted when the reconciler's
    /// root-level identity or tag changes for a single root
    /// (`Patch::ReplaceRoot`). Sibling roots keep their NodeKeys and
    /// any GPU resources cached against them.
    ///
    /// Targeted by NodeKey rather than root index: a `ReorderRoots`
    /// leading the same batch permutes `arena.roots()` before this
    /// work applies, so a translate-time index would be stale.
    ReplaceRootAt {
        key: NodeKey,
        descriptors: Vec<ElementDescriptor>,
    },
    /// Wholesale replace the `index`-th child of `parent` with N
    /// freshly-built descriptors (N >= 1; Fragment new-node yields
    /// multiple). Emitted when the reconciler's mid-tree identity
    /// or tag changes (`Patch::ReplaceNode`). The old child subtree
    /// is removed and the N new descriptors are inserted at
    /// `index..index+N`.
    ReplaceNode {
        parent: NodeKey,
        index: usize,
        descriptors: Vec<ElementDescriptor>,
    },
    /// 軌 1 #5: insert N descriptors as consecutive children of
    /// `parent` starting at `index_start`. Emitted when an
    /// `InsertChild` patch's RSX child expands into a Fragment with
    /// multiple top-level descriptors. The single-descriptor case
    /// still goes through `Create` for clarity.
    CreateMany {
        parent: NodeKey,
        index_start: usize,
        descriptors: Vec<ElementDescriptor>,
    },
    /// 軌 1 #4: rearrange the arena's root list according to `mapping`,
    /// where `new_arena_roots[i] == old_arena_roots[mapping[i]]`.
    /// No NodeKeys are minted or destroyed — retained-surface / Persistent
    /// GPU resources cached against existing root NodeKeys survive.
    /// Emitted when `reconcile_multi` detects that the new root set is a
    /// permutation of the old (e.g. window-manager Z-order swap on a
    /// Fragment-at-root scene).
    ReorderRoots { mapping: Vec<usize> },
}

// ---------------------------------------------------------------------------
// Patch → FiberWork translation
// ---------------------------------------------------------------------------

/// Translate a single reconciler `Patch` into a `FiberWork`.
///
/// Returns `None` when the translator cannot faithfully produce a
/// work unit from the patch alone — callers should interpret `None` as
/// "fall back to the full-rebuild path" rather than silently skipping
/// the patch.
///
/// Specifically this falls back on:
/// - `ReplaceRoot` / `ReplaceNode` — these carry a whole `RsxNode`
///   subtree that needs the full inherited-style/global-path context
///   to be converted into an `ElementDescriptor`. That context is
///   plumbed by `render_rsx`, not by this translator, so we bail.
/// - `InsertChild` without descriptor context — same reason: needs the
///   descriptor pipeline with the parent's inherited style.
///
/// `_root` is passed through to `resolve_path`; callers typically pass
/// `arena.roots()[0]` for single-root scenes.
pub fn patch_to_fiber_work(
    patch: Patch,
    _id_to_key: &FxHashMap<u64, NodeKey>,
    arena: &NodeArena,
    root: NodeKey,
    ctx: Option<&DescriptorContext<'_>>,
) -> Option<FiberWork> {
    patch_to_fiber_work_with_rsx(patch, _id_to_key, arena, root, ctx, None, None)
}

/// Track 1 #10 extension: same as `patch_to_fiber_work` but accepts
/// the per-root OLD rsx tree so the dispatcher can:
/// (a) translate rsx-space paths to arena-space via
///     `rsx_to_arena_path` before `resolve_path`, and
/// (b) keep the rsx-space path for `walk_rsx_by_index_path*`
///     (which walks the rsx tree, not the arena).
///
/// When `per_root_old_rsx` is `None`, behaves exactly like the old
/// entry point (no translation; paths assumed arena-aligned).
pub fn patch_to_fiber_work_with_rsx(
    patch: Patch,
    _id_to_key: &FxHashMap<u64, NodeKey>,
    arena: &NodeArena,
    root: NodeKey,
    ctx: Option<&DescriptorContext<'_>>,
    per_root_old_rsx: Option<&RsxNode>,
    per_root_new_rsx: Option<&RsxNode>,
) -> Option<FiberWork> {
    patch_to_fiber_work_with_rsx_at_root(
        patch,
        _id_to_key,
        arena,
        root,
        ctx,
        per_root_old_rsx,
        per_root_new_rsx,
        None,
    )
}

fn patch_to_fiber_work_with_rsx_at_root(
    patch: Patch,
    _id_to_key: &FxHashMap<u64, NodeKey>,
    arena: &NodeArena,
    root: NodeKey,
    ctx: Option<&DescriptorContext<'_>>,
    per_root_old_rsx: Option<&RsxNode>,
    per_root_new_rsx: Option<&RsxNode>,
    new_root_index: Option<usize>,
) -> Option<FiberWork> {
    // When per-root rsx is available, map rsx paths → arena paths for
    // the `resolve_path` calls below. rsx-path is still passed to
    // `walk_rsx_by_index_path*` which needs rsx-space indices.
    let arena_path_for = |rsx_path: &[usize]| -> Option<Vec<usize>> {
        match per_root_old_rsx {
            None => Some(rsx_path.to_vec()),
            Some(old) => match rsx_to_arena_path(old, rsx_path) {
                ArenaPathResolution::Arena(p) => Some(p),
                ArenaPathResolution::Invalid => None,
            },
        }
    };
    let full_new_path_for = |relative_path: &[usize]| -> Option<Vec<usize>> {
        let ctx = ctx?;
        match new_root_index {
            Some(root_index) => match ctx.new_rsx_root {
                RsxNode::Fragment(fragment) => {
                    fragment.children.get(root_index)?;
                    let mut full_path = Vec::with_capacity(relative_path.len() + 1);
                    full_path.push(root_index);
                    full_path.extend_from_slice(relative_path);
                    Some(full_path)
                }
                _ if root_index == 0 => Some(relative_path.to_vec()),
                _ => None,
            },
            None => Some(relative_path.to_vec()),
        }
    };
    match patch {
        Patch::ReplaceRoot(new_node) => {
            // Need DescriptorContext — without it we can't build the
            // new subtree's descriptor. Callers lacking context (unit
            // tests, ad-hoc translation) fall back.
            let ctx = ctx?;
            let inherited = crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
                ctx.inherited_style,
                ctx.viewport_width,
                ctx.viewport_height,
            );
            let full_path = full_new_path_for(&[])?;
            let target = walk_rsx_by_index_path(ctx.new_rsx_root, &full_path)?;
            if target.identity() != new_node.identity() {
                return None;
            }
            let descriptors =
                crate::view::renderer_adapter::rsx_subtree_to_descriptors_with_inherited(
                    ctx.new_rsx_root,
                    &full_path,
                    &inherited,
                )
                .ok()?;
            // 軌 1 #5: Fragment root → N descriptors. Empty result
            // (e.g. an empty Fragment) is rejected — apply would
            // leave the arena root-less which the dispatch loop
            // can't handle.
            if descriptors.is_empty() {
                return None;
            }
            // Single-root replacement: `root` is the currently-live
            // NodeKey for this patch's root (the caller already
            // resolved it through any pending root permutation).
            // Sibling roots must survive, so never route this through
            // the root-set wipe.
            Some(FiberWork::ReplaceRootAt {
                key: root,
                descriptors,
            })
        }
        Patch::ReorderRoots(mapping) => {
            // 軌 1 #4: pure index permutation of arena.roots. No GPU
            // resources or NodeKeys touched.
            Some(FiberWork::ReorderRoots { mapping })
        }
        Patch::ReplaceAllRoots(new_nodes) => {
            // 軌 1 #4 Fragment-at-root: wholesale root-set swap. Convert
            // each new root RsxNode into descriptors (each one may itself
            // expand to N via nested Fragment), flatten, feed to the
            // multi-descriptor ReplaceAllRoots apply path which
            // clears + pushes N.
            let ctx = ctx?;
            let inherited = crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
                ctx.inherited_style,
                ctx.viewport_width,
                ctx.viewport_height,
            );
            // Conversion runs over `ctx.new_rsx_root` (so sibling
            // ordinals and global paths match the cold path), but the
            // patch carries its own root list. Verify the two describe
            // the same root set before trusting the walk — same
            // identity gate the single-subtree arms apply.
            let authored_roots: &[RsxNode] = match ctx.new_rsx_root {
                RsxNode::Fragment(fragment) => fragment.children.as_slice(),
                other => std::slice::from_ref(other),
            };
            if authored_roots.len() != new_nodes.len()
                || authored_roots
                    .iter()
                    .zip(new_nodes.iter())
                    .any(|(authored, patched)| authored.identity() != patched.identity())
            {
                return None;
            }
            let all = crate::view::renderer_adapter::rsx_subtree_to_descriptors_with_inherited(
                ctx.new_rsx_root,
                &[],
                &inherited,
            )
            .ok()?;
            if all.is_empty() {
                return None;
            }
            Some(FiberWork::ReplaceAllRoots { descriptors: all })
        }
        Patch::ReplaceNode {
            path,
            node: new_node,
        } => {
            let ctx = ctx?;
            // Non-empty path: reconciler emits ReplaceRoot for path==[].
            if path.is_empty() {
                return None;
            }
            let (&index, parent_path) = path.split_last()?;
            let parent_arena_path = arena_path_for(parent_path)?;
            let parent_key = resolve_path(arena, root, &parent_arena_path)?;
            // Rebuild the inherited cascade at the arena parent so the
            // new subtree sees the same ancestor style the cold path
            // would.
            let inherited = crate::view::renderer_adapter::style_cascade_at_parent(
                arena,
                parent_key,
                ctx.inherited_style,
                ctx.viewport_width,
                ctx.viewport_height,
            );
            let full_path = full_new_path_for(&path)?;
            let target = walk_rsx_by_index_path(ctx.new_rsx_root, &full_path)?;
            if target.identity() != new_node.identity() {
                return None;
            }
            let descriptors =
                crate::view::renderer_adapter::rsx_subtree_to_descriptors_with_inherited(
                    ctx.new_rsx_root,
                    &full_path,
                    &inherited,
                )
                .ok()?;
            // 軌 1 #5: Fragment new-node → N descriptors at the
            // replaced slot. Empty result rejected (same rationale
            // as ReplaceRoot).
            if descriptors.is_empty() {
                return None;
            }
            Some(FiberWork::ReplaceNode {
                parent: parent_key,
                index,
                descriptors,
            })
        }
        Patch::UpdateElementProps {
            path,
            changed,
            removed,
        } => {
            let arena_path = arena_path_for(&path)?;
            let key = resolve_path(arena, root, &arena_path)?;
            Some(FiberWork::Update {
                key,
                changed,
                removed,
            })
        }
        Patch::SetText { path, text } => {
            // The RSX reconciler emits `SetText` on an `RsxNode::Text`
            // leaf — which, when the leaf is an Element `<Text>`'s
            // child string, has no separate arena node. Instead the
            // Text host swallows the content into its `.content`.
            //
            // If the full path resolves, use it (this is the
            // stand-alone text-leaf root case). Otherwise, try the
            // parent path: if the parent arena node is a Text or
            // TextArea host, route the SetText to *that* key so
            // `apply_set_text_work` can call `set_text` on the host.
            let arena_path = arena_path_for(&path)?;
            if let Some(key) = resolve_path(arena, root, &arena_path) {
                Some(FiberWork::SetText { key, text })
            } else if !arena_path.is_empty() {
                let parent_arena_path = &arena_path[..arena_path.len() - 1];
                let parent_key = resolve_path(arena, root, parent_arena_path)?;
                Some(FiberWork::SetText {
                    key: parent_key,
                    text,
                })
            } else {
                None
            }
        }
        Patch::InsertChild {
            parent_path,
            index,
            node: _new_node,
        } => {
            // M5 #5/#6: build a descriptor for the freshly-inserted
            // child via the cold-path converter.
            //
            // Context gate: no DescriptorContext → fall back. Callers
            // that don't have a NEW rsx root on hand (unit tests, the
            // convenience wrapper `patches_to_fiber_works`) thus keep
            // the pre-M5 behaviour.
            let ctx = ctx?;

            // 1) Resolve the arena parent. parent_path is rsx-space
            //    from the reconciler; translate via `arena_path_for`
            //    so Fragment mid-tree parents land on the right arena
            //    key.
            let parent_arena_path = arena_path_for(&parent_path)?;
            let parent_key = resolve_path(arena, root, &parent_arena_path)?;

            // 2) Walk the NEW rsx tree along the SAME OLD rsx-space
            //    path to find the parent node. 軌 1 #6: when an
            //    old-tree snapshot is available in `ctx`, run the
            //    identity-validated lockstep walk instead — any
            //    structural drift higher up aborts the translation
            //    so the caller falls back to a full rebuild.
            //
            //    Note: per-root `old_rsx_root` lives in `ctx` (the
            //    full fragment root); the per-root tree we passed
            //    into `arena_path_for` for arena alignment isn't
            //    used here because `ctx.new_rsx_root` is the full
            //    NEW tree of matching shape.
            // Track 1 #10 fix: use per-root rsx when caller provided.
            // ctx.{old,new}_rsx_root is the FULL (fragment-at-root) tree,
            // but reconcile_multi emits paths relative to each root_index
            // subtree. Walking from the full root silently misaligns by
            // one level for root_index > 0.
            let walk_old_root: Option<&RsxNode> = per_root_old_rsx.or(ctx.old_rsx_root);
            let walk_new_root: &RsxNode = per_root_new_rsx.unwrap_or(ctx.new_rsx_root);
            let new_parent_rsx = match walk_old_root {
                Some(old) => walk_rsx_by_index_path_validated(old, walk_new_root, &parent_path)?,
                None => walk_rsx_by_index_path(walk_new_root, &parent_path)?,
            };

            // 3) Fish out the freshly-authored child at the NEW index.
            let kids = new_parent_rsx.children()?;
            let child_rsx = kids.get(index)?;

            // 4) Reuse the cold-path converter. M6 cascade: rebuild
            //    the `StyleCascadeContext` at the arena parent by
            //    walking its ancestor chain and replaying each
            //    Element's `parsed_style` through `merge_style`. This
            //    matches what `build_container_element_shell` does in
            //    the cold-path loop, so text children inherit
            //    font_size / color / etc. from ancestors instead of
            //    the viewport-root approximation M5.0 shipped.
            let inherited = crate::view::renderer_adapter::style_cascade_at_parent(
                arena,
                parent_key,
                ctx.inherited_style,
                ctx.viewport_width,
                ctx.viewport_height,
            );
            let mut child_path = parent_path.clone();
            child_path.push(index);
            let full_path = full_new_path_for(&child_path)?;
            let target = walk_rsx_by_index_path(ctx.new_rsx_root, &full_path)?;
            if target.identity() != child_rsx.identity() {
                return None;
            }
            let mut descriptors =
                crate::view::renderer_adapter::rsx_subtree_to_descriptors_with_inherited(
                    ctx.new_rsx_root,
                    &full_path,
                    &inherited,
                )
                .ok()?;

            // 5) Single-descriptor case → Create. Multi-descriptor
            //    (Fragment expansion) → 軌 1 #5 CreateMany, inserted
            //    as consecutive children starting at `index`.
            match descriptors.len() {
                0 => None,
                1 => {
                    let descriptor = descriptors.pop().unwrap();
                    let stable_id = descriptor.element.stable_id();
                    Some(FiberWork::Create {
                        parent: Some(parent_key),
                        index,
                        descriptor,
                        stable_id,
                    })
                }
                _ => Some(FiberWork::CreateMany {
                    parent: parent_key,
                    index_start: index,
                    descriptors,
                }),
            }
        }
        Patch::RemoveChild { parent_path, index } => {
            let parent_arena_path = arena_path_for(&parent_path)?;
            let parent = resolve_path(arena, root, &parent_arena_path)?;
            let children = arena.children_of(parent);
            let key = *children.get(index)?;
            Some(FiberWork::Delete {
                parent: Some(parent),
                key,
            })
        }
        Patch::MoveChild {
            parent_path,
            from,
            to,
        } => {
            let parent_arena_path = arena_path_for(&parent_path)?;
            let parent = resolve_path(arena, root, &parent_arena_path)?;
            let children = arena.children_of(parent);
            let key = *children.get(from)?;
            Some(FiberWork::Move {
                parent,
                key,
                from,
                to,
            })
        }
    }
}

/// Convenience: run `patch_to_fiber_work` over a batch. Skips patches
/// that translate to `None` (caller decides whether to fall back for
/// the whole batch or cherry-pick).
pub fn patches_to_fiber_works(
    patches: Vec<Patch>,
    id_to_key: &FxHashMap<u64, NodeKey>,
    arena: &NodeArena,
    root: NodeKey,
    ctx: Option<&DescriptorContext<'_>>,
) -> Vec<FiberWork> {
    patches
        .into_iter()
        .filter_map(|p| patch_to_fiber_work(p, id_to_key, arena, root, ctx))
        .collect()
}

/// Translate `patches` into works, stopping at the first patch that
/// cannot be faithfully translated. Returns `None` in that case so the
/// caller knows the whole batch should fall back to the full-rebuild path.
///
/// Partial translation is not safe: a skipped patch would leave the arena
/// inconsistent with the new RSX.
pub fn translate_patches_all_or_nothing(
    patches: Vec<Patch>,
    id_to_key: &FxHashMap<u64, NodeKey>,
    arena: &NodeArena,
    root: NodeKey,
    ctx: Option<&DescriptorContext<'_>>,
) -> Option<Vec<FiberWork>> {
    let mut out = Vec::with_capacity(patches.len());
    for p in patches {
        out.push(patch_to_fiber_work(p, id_to_key, arena, root, ctx)?);
    }
    Some(out)
}

/// Multi-root variant of `translate_patches_all_or_nothing`. Each
/// `RootedPatch` is dispatched against `roots[root_index]`. Used by the
/// viewport multi-root incremental path (軌 1 #4 Fragment-at-root).
///
/// `ReplaceAllRoots` is routed through any arena root (its apply side
/// clears + rebuilds all roots regardless), so a missing or stale
/// `root_index` there is fine; every other variant strictly requires the
/// right per-root key.
pub fn translate_rooted_patches_all_or_nothing(
    patches: Vec<crate::ui::RootedPatch>,
    id_to_key: &FxHashMap<u64, NodeKey>,
    arena: &NodeArena,
    roots: &[NodeKey],
    old_roots: &[&RsxNode],
    new_roots: &[&RsxNode],
    ctx: Option<&DescriptorContext<'_>>,
) -> Option<Vec<FiberWork>> {
    let mut out = Vec::with_capacity(patches.len());
    // Reconciler emits patches keyed by *new* root_index. When a
    // ReorderRoots patch leads the batch, subsequent patches reference
    // post-reorder indices but the arena still holds the OLD root order
    // at translate time. Track the active permutation so the per-pair
    // dispatcher resolves to the right currently-live NodeKey.
    let mut permutation: Option<Vec<usize>> = None;
    for rp in patches {
        let root = match &rp.patch {
            Patch::ReplaceAllRoots(_) => roots.first().copied().unwrap_or(NodeKey::default()),
            Patch::ReorderRoots(mapping) => {
                permutation = Some(mapping.clone());
                roots.first().copied().unwrap_or(NodeKey::default())
            }
            _ => {
                let resolved_index = match &permutation {
                    Some(perm) => *perm.get(rp.root_index)?,
                    None => rp.root_index,
                };
                *roots.get(resolved_index)?
            }
        };
        // Rewrite patch paths from rsx-space (reconciler emits paths
        // through Fragment nodes) to arena-space (Fragment children
        // are flattened into the parent's arena child list). Without
        // this translation `resolve_path` walks the wrong arena
        // children for any subtree whose ancestors contain mid-tree
        // Fragments.
        let per_root_old_rsx: Option<&RsxNode> = match &rp.patch {
            Patch::ReplaceAllRoots(_) | Patch::ReorderRoots(_) | Patch::ReplaceRoot(_) => None,
            _ => {
                let old_root_idx = match &permutation {
                    Some(perm) => *perm.get(rp.root_index)?,
                    None => rp.root_index,
                };
                old_roots.get(old_root_idx).copied()
            }
        };
        // NEW-side per-root tree (post-reorder index). InsertChild walks
        // NEW rsx by parent_path; patch paths are root_index-relative,
        // so use per-root NEW subtree, not ctx.new_rsx_root (full root).
        let per_root_new_rsx: Option<&RsxNode> = match &rp.patch {
            Patch::ReplaceAllRoots(_) | Patch::ReorderRoots(_) | Patch::ReplaceRoot(_) => None,
            _ => new_roots.get(rp.root_index).copied(),
        };
        // Track 1 #10 fix: do NOT pre-translate paths. The dispatcher
        // (`patch_to_fiber_work_with_rsx`) needs the rsx-space path
        // for NEW-tree walks (InsertChild) and internally maps
        // rsx → arena via `rsx_to_arena_path` for `resolve_path`.
        let translated_patch = rp.patch;
        // Keep a copy for the per-patch ReplaceNode fallback path below.
        let patch_snapshot = translated_patch.clone();
        match patch_to_fiber_work_with_rsx_at_root(
            translated_patch,
            id_to_key,
            arena,
            root,
            ctx,
            per_root_old_rsx,
            per_root_new_rsx,
            Some(rp.root_index),
        ) {
            Some(work) => out.push(work),
            None => {
                // Per-patch fallback: convert to `ReplaceNode` at the
                // failing subtree. Keeps other roots / siblings on the
                // incremental path; only the one subtree rebuilds.
                if let Some(fallback) =
                    fallback_replace_node_patch(&patch_snapshot, per_root_old_rsx, per_root_new_rsx)
                    && let Some(work) = patch_to_fiber_work_with_rsx_at_root(
                        fallback,
                        id_to_key,
                        arena,
                        root,
                        ctx,
                        per_root_old_rsx,
                        per_root_new_rsx,
                        Some(rp.root_index),
                    )
                {
                    out.push(work);
                    continue;
                }
                return None;
            }
        }
    }
    Some(out)
}

fn fallback_replace_node_patch(
    patch: &Patch,
    per_root_old_rsx: Option<&RsxNode>,
    per_root_new_rsx: Option<&RsxNode>,
) -> Option<Patch> {
    let new_root = per_root_new_rsx?;
    let path: Vec<usize> = match patch {
        Patch::UpdateElementProps { path, .. } | Patch::SetText { path, .. } => path.clone(),
        Patch::InsertChild { parent_path, .. }
        | Patch::RemoveChild { parent_path, .. }
        | Patch::MoveChild { parent_path, .. } => parent_path.clone(),
        _ => return None,
    };
    // The fallback rewrites the patch as `ReplaceNode { path, NEW[path] }`.
    // That only round-trips when `OLD[path]` and `NEW[path]` describe the
    // same arena slot — e.g. a shape-change at a stable position. If a
    // keyed sibling reorder above this path made `NEW[path]` a different
    // keyed node than `OLD[path]`, replacing the OLD slot with NEW's
    // contents would clobber an unrelated arena node (the one the
    // pending MoveChild is still going to address by its OLD index),
    // and end up with a duplicate row. Refuse the fallback in that case
    // so the caller falls through to the legacy full-rebuild path.
    let old_root = per_root_old_rsx?;
    let old_at_path = walk_rsx_by_index_path(old_root, &path)?;
    let new_at_path = walk_rsx_by_index_path(new_root, &path)?;
    if old_at_path.identity() != new_at_path.identity() {
        return None;
    }
    Some(Patch::ReplaceNode {
        path,
        node: new_at_path.clone(),
    })
}

/// 軌 1 #11: per-prop apply outcome reported by `ElementTrait::apply_prop`.
/// Hosts return one of these for each `(name, value)` pair they're
/// asked to apply. fiber_work aggregates: any `NeedsCascade` triggers
/// `recascade_text_subtree` after the element is back in its slot;
/// `UnknownProp` / `DecodeFailed` log + continue. Structural failures are
/// distinct: they abort the batch so the viewport can cold rebuild from the
/// authoritative RSX tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropApplyOutcome {
    /// Prop applied; no further action.
    Applied,
    /// Prop applied; descendants need text-style recascade after the
    /// element is returned to its arena slot.
    NeedsCascade,
    /// Host doesn't recognise this prop name. Caller logs and skips.
    UnknownProp,
    /// Host recognises the prop but couldn't decode the `PropValue`
    /// to its expected shape. Caller logs and skips.
    DecodeFailed(&'static str),
    /// The prop decoded, but applying it would violate retained arena
    /// topology. The caller must stop this batch and cold rebuild.
    RequiresColdRebuild(&'static str),
    /// `reset_prop` only: this host can't reset the named prop without
    /// a full rebuild. Caller logs and skips.
    CannotReset(&'static str),
}

/// Why an incremental Update or SetText couldn't be applied. Surfaced
/// from `apply_update_work` / `apply_set_text_work` so the gate can
/// fall back to the full-rebuild path without the arena ever being
/// partially mutated (all failures are detected pre-apply).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateFailure {
    /// SetText target isn't a Text or TextArea node.
    SetTextOnNonTextTarget,
    /// Target NodeKey vanished from the arena (stale work batch).
    MissingTarget,
    /// A decoded prop could not be installed without violating retained
    /// topology. The incremental batch is no longer authoritative.
    StructuralPropApplyFailed(&'static str),
}

impl FiberWork {
    /// Whether this work unit is safe to commit under the current
    /// incremental setter surface, **given the arena state** so
    /// per-variant checks can consult the target element type.
    ///
    /// Rules:
    /// - `Delete` / `Move` / `Create` / `CreateMany` /
    ///   `ReplaceAllRoots` / `ReplaceNode` / `ReorderRoots`: always
    ///   committable.
    /// - `ReplaceRootAt`: committable iff the target key is still a
    ///   live arena root.
    /// - `SetText`: committable iff the target (after the
    ///   text-child-to-host remap done in `patch_to_fiber_work`) is
    ///   an arena node whose element downcasts to `Text` or
    ///   `TextArea`.
    /// - `Update`: committable iff the target NodeKey still exists.
    ///   Unknown / unsupported props are logged and skipped on the
    ///   apply side (`apply_update_to_*` `_` arms emit
    ///   `[fiber_work] Update skipped prop ...`), so a single
    ///   unrecognised key no longer forces a full cold rebuild.
    pub fn is_committable(&self, arena: &NodeArena) -> bool {
        match self {
            FiberWork::Delete { .. } | FiberWork::Move { .. } => true,
            FiberWork::SetText { key, .. } => target_is_text_like(arena, *key),
            FiberWork::Update { key, .. } => arena.get(*key).is_some(),
            FiberWork::Create { .. } => true,
            // Wholesale subtree replacement — always committable once
            // the descriptor has been built. The descriptor carries the
            // full fresh element/children tree; the apply side drops
            // the old subtree and commits the new.
            FiberWork::ReplaceAllRoots { .. } | FiberWork::ReplaceNode { .. } => true,
            // Single-root replacement splices `key` out of `roots()`,
            // so the target must still be a live root. Root membership
            // is invariant under a preceding `ReorderRoots` (which
            // permutes but never adds or drops keys), so a gate-time
            // check stays valid through the batch.
            FiberWork::ReplaceRootAt { key, .. } => arena.roots().contains(key),
            FiberWork::ReorderRoots { .. } => true,
            // 軌 1 #5: fragment expansion. Apply does N successive
            // arena_insert_child calls — each is the same shape as
            // a single Create.
            FiberWork::CreateMany { .. } => true,
        }
    }
}

/// PropertyIds that cascade into descendant text nodes (font_family,
/// font_size, font_weight, color, cursor, text_wrap — mirrors
/// `StyleCascadeContext::merge_style`). Kept in one place so the
/// boundary gate and the cold-path merger reference the same list.
const TEXT_CASCADING_PROPS: &[crate::style::PropertyId] = &[
    crate::style::PropertyId::FontFamily,
    crate::style::PropertyId::FontSize,
    crate::style::PropertyId::FontWeight,
    crate::style::PropertyId::Color,
    crate::style::PropertyId::Cursor,
    crate::style::PropertyId::TextWrap,
    crate::style::PropertyId::LineHeight,
    crate::style::PropertyId::VerticalAlign,
];

/// Does `key`'s arena node have any descendant? Cheap check:
/// `children_of(key).is_empty()` — enough to decide whether a
/// text-cascading style change could reach a Text/TextArea leaf and
/// therefore needs the full-rebuild fallback.
fn arena_has_descendants(arena: &NodeArena, key: NodeKey) -> bool {
    !arena.children_of(key).is_empty()
}

/// Peek at the Element at `key` (if any) and report whether its
/// currently-authored `parsed_style` contains any text-cascading
/// declaration. Used by the boundary gate when the whole `style` prop
/// is being removed: if the dropped style carried cascading props,
/// descendants' resolved values would drift.
fn element_parsed_style_has_text_cascading_decl(arena: &NodeArena, key: NodeKey) -> bool {
    use crate::view::base_component::Element;
    let Some(node) = arena.get(key) else {
        return false;
    };
    let Some(el) = node.element.as_any().downcast_ref::<Element>() else {
        return false;
    };
    let style = el.parsed_style();
    TEXT_CASCADING_PROPS
        .iter()
        .any(|pid| style.get(*pid).is_some())
}

/// Would applying `new_value` as the `style` prop of the Element at
/// `key` change any text-cascading decl relative to its current
/// `parsed_style`? Returns `false` when:
/// - the element has no descendants (cascade has nowhere to flow)
/// - the element isn't an `Element` host (Text/TextArea/Image handle
///   their own style fan-out via dedicated setters)
/// - the PropValue doesn't decode to an `ElementStylePropSchema`
///   (malformed — let the downstream decoder err)
/// - old and new have the same value for every cascading PropertyId
///
/// Returns `true` when any of the 6 text-cascading decls would flip
/// between old and new — these changes would need subtree recascade
/// that the incremental path doesn't implement, so the gate forces a
/// full rebuild.
fn would_change_text_cascade(arena: &NodeArena, key: NodeKey, new_value: &PropValue) -> bool {
    use crate::view::base_component::Element;
    use crate::view::renderer_adapter::as_element_style;

    let Some(node) = arena.get(key) else {
        return false;
    };
    let Some(el) = node.element.as_any().downcast_ref::<Element>() else {
        return false;
    };
    if !arena_has_descendants(arena, key) {
        return false;
    }
    let Ok(new_style) = as_element_style(new_value, "style") else {
        return false;
    };
    let old_style = el.parsed_style();
    TEXT_CASCADING_PROPS
        .iter()
        .any(|pid| old_style.get(*pid) != new_style.get(*pid))
}

/// 軌 A #7: walk Text/TextArea descendants of `root_key` and re-apply
/// the inherited-cascade values they would have received under the
/// freshly-updated ancestor chain. Each descendant's per-prop
/// `*_explicit` flags preserve author overrides; only non-explicit
/// props get overwritten.
///
/// The walk is recursive depth-first via `arena.children_of`. Element
/// descendants themselves don't carry Text props — we just traverse
/// through them.
pub(crate) fn recascade_text_subtree(
    arena: &mut NodeArena,
    ctx: ApplyContext<'_>,
    root_key: NodeKey,
) {
    use crate::view::renderer_adapter::style_cascade_at_parent;

    fn walk(arena: &mut NodeArena, ctx: ApplyContext<'_>, key: NodeKey) {
        // Compute inherited cascade at this node's arena parent — the
        // helper walks the ancestor chain replaying each Element's
        // `parsed_style` through `StyleCascadeContext::merge_style`.
        let parent = arena.parent_of(key);
        let inherited = match parent {
            Some(p) => style_cascade_at_parent(
                arena,
                p,
                ctx.viewport_style,
                ctx.viewport_width,
                ctx.viewport_height,
            ),
            None => crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
                ctx.viewport_style,
                ctx.viewport_width,
                ctx.viewport_height,
            ),
        };
        // Phase 3: cascade replay goes through `ElementTrait::apply_inherited`.
        // Text / TextArea override; other hosts use the no-op default.
        arena.mutate_element_with_invalidation(key, |element, cx| {
            element.apply_inherited(&inherited);
            cx.invalidate(element.local_dirty_flags());
        });
        for child in arena.children_of(key) {
            walk(arena, ctx, child);
        }
    }

    // Don't re-cascade at `root_key` itself — the caller is the one
    // whose style just changed; start with its children.
    for child in arena.children_of(root_key) {
        walk(arena, ctx, child);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum TargetKind {
    Text,
    TextArea,
    Element,
    Image,
    Svg,
    Other,
    Missing,
}

fn classify_target(arena: &NodeArena, key: NodeKey) -> TargetKind {
    use crate::view::base_component::{Element, Image, Svg, Text, TextArea};
    let Some(node) = arena.get(key) else {
        return TargetKind::Missing;
    };
    let any = node.element.as_any();
    if any.is::<Text>() {
        TargetKind::Text
    } else if any.is::<TextArea>() {
        TargetKind::TextArea
    } else if any.is::<Image>() {
        TargetKind::Image
    } else if any.is::<Svg>() {
        TargetKind::Svg
    } else if any.is::<Element>() {
        TargetKind::Element
    } else {
        TargetKind::Other
    }
}

fn target_is_text_like(arena: &NodeArena, key: NodeKey) -> bool {
    matches!(
        classify_target(arena, key),
        TargetKind::Text | TargetKind::TextArea
    )
}

// ---------------------------------------------------------------------------
// FiberWork commit
// ---------------------------------------------------------------------------

/// Apply a batch of `FiberWork` items against `arena`.
///
/// Supports Create/Delete/Move/Replace/Update/SetText work. The batch is not
/// transactional: if a later Update/SetText fails, earlier work may already
/// be visible in `arena`. Callers receiving `Err` must discard or roll back
/// the partial arena; the viewport immediately cold rebuilds from RSX.
pub fn apply_fiber_works(
    arena: &mut NodeArena,
    ctx: ApplyContext<'_>,
    works: Vec<FiberWork>,
) -> Result<(), UpdateFailure> {
    for work in works {
        match work {
            FiberWork::Create {
                parent,
                index,
                descriptor,
                stable_id: _,
            } => {
                match parent {
                    Some(parent_key) => {
                        arena_insert_child(arena, parent_key, index, descriptor);
                    }
                    None => {
                        // Root-level create: commit with no parent,
                        // then splice the new key into `roots()` at
                        // `index`.
                        let new_key = commit_descriptor_tree(arena, None, descriptor);
                        let mut roots = arena.roots().to_vec();
                        let at = index.min(roots.len());
                        roots.insert(at, new_key);
                        arena.set_roots(roots);
                    }
                }
            }
            FiberWork::Update {
                key,
                changed,
                removed,
            } => {
                // The gate (`is_committable`) pre-filters the whole
                // batch, so by the time we get here every key is in
                // the M3 whitelist and `removed` is empty. Any residual
                // decode failure (wrong PropValue variant for a
                // whitelisted key) is logged and the work is dropped —
                // the reconciler's typed PropValue produces these only
                // for user-authoring bugs already surfaced on the cold
                // render path, so a silent log matches existing
                // convert_* error handling.
                apply_update_work(arena, ctx, key, changed, removed)?;
            }
            FiberWork::SetText { key, text } => {
                apply_set_text_work(arena, key, text)?;
            }
            FiberWork::Move {
                parent,
                key,
                from,
                to,
            } => {
                arena_move_child(arena, parent, key, from, to);
            }
            FiberWork::ReplaceAllRoots { descriptors } => {
                // Drop every existing root subtree (Fragment-root
                // case may have N>1 old roots).
                let old_roots: Vec<NodeKey> = arena.roots().to_vec();
                arena.clear_roots();
                for old in old_roots {
                    arena.remove_subtree(old);
                }
                for desc in descriptors {
                    let new_key = commit_descriptor_tree(arena, None, desc);
                    arena.push_root(new_key);
                }
            }
            FiberWork::ReplaceRootAt { key, descriptors } => {
                // Splice the replacement in at the old root's slot so
                // sibling roots keep both their NodeKeys and their
                // relative order. The gate guarantees `key` is a live
                // root; a vanished target means the batch is stale.
                let position = arena
                    .roots()
                    .iter()
                    .position(|&root_key| root_key == key)
                    .ok_or(UpdateFailure::MissingTarget)?;
                let new_keys: Vec<NodeKey> = descriptors
                    .into_iter()
                    .map(|desc| commit_descriptor_tree(arena, None, desc))
                    .collect();
                let mut roots = arena.roots().to_vec();
                roots.splice(position..position + 1, new_keys);
                arena.set_roots(roots);
                arena.remove_subtree(key);
            }
            FiberWork::ReplaceNode {
                parent,
                index,
                descriptors,
            } => {
                // Remove old child subtree, then commit each new
                // descriptor at successive indices.
                arena_remove_child(arena, parent, index);
                for (offset, desc) in descriptors.into_iter().enumerate() {
                    arena_insert_child(arena, parent, index + offset, desc);
                }
            }
            FiberWork::ReorderRoots { mapping } => {
                // 軌 1 #4: pure permutation. `new_roots[i] = old_roots[mapping[i]]`.
                let old_roots = arena.roots().to_vec();
                let new_roots: Vec<NodeKey> = mapping.into_iter().map(|j| old_roots[j]).collect();
                arena.set_roots(new_roots);
            }
            FiberWork::CreateMany {
                parent,
                index_start,
                descriptors,
            } => {
                // Insert in original order; each insert shifts later
                // indices, so step by `index_start + offset` rather
                // than recomputing from arena.children_of.
                for (offset, desc) in descriptors.into_iter().enumerate() {
                    arena_insert_child(arena, parent, index_start + offset, desc);
                }
            }
            FiberWork::Delete { parent, key } => match parent {
                Some(parent_key) => {
                    let children = arena.children_of(parent_key);
                    if let Some(index) = children.iter().position(|&c| c == key) {
                        arena_remove_child(arena, parent_key, index);
                    } else {
                        // Child already gone — drop the subtree
                        // defensively so we don't leak slots.
                        arena.remove_subtree(key);
                    }
                }
                None => {
                    let mut roots = arena.roots().to_vec();
                    roots.retain(|&r| r != key);
                    arena.set_roots(roots);
                    arena.remove_subtree(key);
                }
            },
        }
    }
    Ok(())
}

/// Apply a prop-diff to the element at `key` via host-owned dispatch
/// (軌 1 #11). Each host implements `ElementTrait::apply_prop` /
/// `reset_prop`; this function only handles the cross-cutting
/// concerns (target liveness, ancestor text-cascade detection,
/// recascade after commit).
fn apply_update_work(
    arena: &mut NodeArena,
    ctx: ApplyContext<'_>,
    key: NodeKey,
    changed: Vec<(&'static str, PropValue)>,
    removed: Vec<&'static str>,
) -> Result<(), UpdateFailure> {
    if arena.get(key).is_none() {
        return Err(UpdateFailure::MissingTarget);
    }

    // 軌 A #7: decide whether this update will change an ancestor's
    // text-cascading decl. Detected *before* taking the element (the
    // helpers need a read-only borrow) and recascaded *after* the
    // element is back in its slot.
    let mut cascade_dirty = false;
    let is_element_target = matches!(classify_target(arena, key), TargetKind::Element);
    if is_element_target && arena_has_descendants(arena, key) {
        for (prop, value) in &changed {
            if *prop == "style" && would_change_text_cascade(arena, key, value) {
                cascade_dirty = true;
                break;
            }
        }
        if !cascade_dirty {
            for prop in &removed {
                if *prop == "style" && element_parsed_style_has_text_cascading_decl(arena, key) {
                    cascade_dirty = true;
                    break;
                }
            }
        }
    }

    let mut structural_failure = None;
    arena.mutate_element_with_invalidation(key, |element, cx| {
        for (name, value) in changed {
            match element.apply_prop(cx.arena(), key, &ctx, name, value) {
                PropApplyOutcome::Applied => {}
                PropApplyOutcome::NeedsCascade => {
                    cascade_dirty = true;
                }
                PropApplyOutcome::UnknownProp => {
                    eprintln!("[fiber_work] Update skipped unknown prop {name:?}");
                }
                PropApplyOutcome::DecodeFailed(p) => {
                    eprintln!("[fiber_work] Update skipped prop {p:?} (decode failed)");
                }
                PropApplyOutcome::RequiresColdRebuild(p) => {
                    structural_failure = Some(UpdateFailure::StructuralPropApplyFailed(p));
                    break;
                }
                PropApplyOutcome::CannotReset(_) => {
                    unreachable!("apply_prop never returns CannotReset")
                }
            }
        }
        if structural_failure.is_none() {
            for name in removed {
                match element.reset_prop(cx.arena(), key, &ctx, name) {
                    PropApplyOutcome::Applied => {}
                    PropApplyOutcome::NeedsCascade => {
                        cascade_dirty = true;
                    }
                    PropApplyOutcome::CannotReset(p) => {
                        eprintln!("[fiber_work] Reset skipped prop {p:?} (no reset path)");
                    }
                    PropApplyOutcome::UnknownProp | PropApplyOutcome::DecodeFailed(_) => {
                        eprintln!("[fiber_work] Reset skipped unknown prop {name:?}");
                    }
                    PropApplyOutcome::RequiresColdRebuild(p) => {
                        structural_failure = Some(UpdateFailure::StructuralPropApplyFailed(p));
                        break;
                    }
                }
            }
        }
        cx.invalidate(element.local_dirty_flags());
    });
    if let Some(failure) = structural_failure {
        return Err(failure);
    }
    // 軌 A #7: recascade after the element is back in its slot —
    // the walker relies on ancestor chain being intact.
    if cascade_dirty {
        recascade_text_subtree(arena, ctx, key);
    }
    Ok(())
}

/// 軌 A #9: resolve a `font_size` prop to pixels with full inherited
/// context. `parent_font_size` comes from the arena ancestor walk
/// (`style_cascade_at_parent`); `root_font_size` is the
/// viewport root; viewport dims are passed in for `vw`/`vh`/`%`.
///
/// The `Em`/`Rem`/`Percent`/`Vw`/`Vh` variants now resolve correctly
/// in the incremental path — previously they fell back to a full
/// rebuild because the apply side had no inherited context.
pub(crate) fn resolve_font_size_px_with_inherited(
    value: &PropValue,
    inherited: &crate::view::renderer_adapter::StyleCascadeContext,
) -> Option<f32> {
    resolve_font_size_prop_with_inherited(value, inherited)
}

/// Apply a `Patch::SetText` to a Text or TextArea host at `key`.
///
/// The arena-side variant check happens here rather than in
/// `is_committable` because the gate has no arena reference — a
/// SetText landing on an Image/Svg/Element host is rare enough that
/// silent fallback is acceptable.
///
/// Q4 (fiber apply must not trigger user handlers): verified —
/// `Text::set_text` and `TextArea::set_text` both mutate `content`
/// and invalidate caches but do *not* call `notify_change_handlers`
/// (those are only invoked from `dispatch_key_down` /
/// `dispatch_text_input` on the event path). `TextArea::set_text`
/// does call `sync_bound_text`, which reflects the incoming value
/// back into a bound `Binding<String>`; that's the intended
/// direction for fiber-driven updates and does not re-enter a user
/// handler.
fn apply_set_text_work(
    arena: &mut NodeArena,
    key: NodeKey,
    text: String,
) -> Result<(), UpdateFailure> {
    use crate::view::base_component::{Text, TextArea};

    if arena.get(key).is_none() {
        return Err(UpdateFailure::MissingTarget);
    }

    let mut result: Result<(), UpdateFailure> = Ok(());
    arena.mutate_element_with_invalidation(key, |element, cx| {
        if let Some(t) = element.as_any_mut().downcast_mut::<Text>() {
            t.set_text(text.clone());
        } else if let Some(ta) = element.as_any_mut().downcast_mut::<TextArea>() {
            ta.set_text(text.clone());
        } else {
            result = Err(UpdateFailure::SetTextOnNonTextTarget);
        }
        cx.invalidate(element.local_dirty_flags());
    });
    result
}

/// Reorder `key` from position `from` to position `to` inside
/// `parent.children`. Mirrors the update onto the parent element's
/// `Element.children` list via `with_element_taken`, matching the
/// invariant maintained by `arena_insert_child` / `arena_remove_child`.
///
/// Out-of-range or missing `key` is a silent no-op — consistent with
/// the other arena_* helpers, so a stale FiberWork batch doesn't panic
/// the host.
fn arena_move_child(arena: &mut NodeArena, parent: NodeKey, key: NodeKey, from: usize, to: usize) {
    let mut children = arena.children_of(parent);
    if from >= children.len() {
        return;
    }
    if children[from] != key {
        // Positions drifted — trust `key` and look it up directly.
        let Some(actual_from) = children.iter().position(|&c| c == key) else {
            return;
        };
        let moved = children.remove(actual_from);
        let at = to.min(children.len());
        children.insert(at, moved);
    } else {
        let moved = children.remove(from);
        let at = to.min(children.len());
        children.insert(at, moved);
    }
    arena.set_children(parent, children.clone());
    arena.mutate_element_with_invalidation(parent, |element, cx| {
        if let Some(el) = element
            .as_any_mut()
            .downcast_mut::<crate::view::base_component::Element>()
        {
            let _previous = el.replace_children(cx.arena(), children);
            cx.invalidate(crate::view::base_component::DirtyFlags::ALL);
        }
    });
}

// ---------------------------------------------------------------------------
// Helpers retained for descriptor/stable-id cleanup.
// ---------------------------------------------------------------------------

/// Extract the `stable_id` that `rsx_to_descriptors_with_context` would
/// assign to the top-level element of `node`. Used once the `Create`
/// translator can build descriptors — kept here so the TODO surface
/// stays in one place.
///
/// Still a stub. Filling it in requires exposing
/// `stable_node_id_from_parts` (currently private in `renderer_adapter`)
/// as `pub(crate)` and threading the parent's `GlobalNodePath` +
/// identity-ordinal context through. Descriptor-producing translators
/// currently get stable identity from the descriptor pipeline instead.
#[allow(dead_code)]
fn expected_stable_id(node: &RsxNode) -> u64 {
    match node {
        RsxNode::Element(el) => element_tag_hint(el),
        _ => 0,
    }
}

#[allow(dead_code)]
fn element_tag_hint(_el: &RsxElementNode) -> u64 {
    // See `expected_stable_id` — filling this in is gated on exposing
    // `element_runtime_name` + `stable_node_id_from_parts` from
    // renderer_adapter.
    0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
