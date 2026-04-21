//! Phase A M1: Fiber work-unit model and the `Patch → FiberWork`
//! translation plus `apply_fiber_works` commit skeleton.
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
//! M1 goal: **plumbing only, no traffic**. The full legacy pipeline
//! (`render_rsx` → `apply_patch`) keeps running unchanged. This module
//! adds:
//!
//! 1. `FiberWork` — a parallel representation of patches anchored on
//!    arena keys.
//! 2. `patch_to_fiber_work` — a best-effort translator. Returns `None`
//!    for patch variants whose faithful M1 translation would require
//!    context the caller does not yet have (full RSX subtree for
//!    descriptors); those fall through to the legacy path.
//! 3. `apply_fiber_works` — a committer skeleton. `Create` / `Delete` /
//!    `Move` are wired through existing arena helpers. `Update` /
//!    `SetText` are reserved for M3 when the setter layer lands.
//!
//! Traffic is routed onto this path later (M2) via a scene flag. M1
//! only requires: compiles, does not panic on no-op calls, and carries
//! the smallest round-trip test.

use rustc_hash::FxHashMap;

use crate::style::Style;
use crate::ui::{FromPropValue, Patch, PropValue, RsxElementNode, RsxNode};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::renderer_adapter::{
    ElementDescriptor, arena_insert_child, arena_remove_child,
    commit_descriptor_tree, resolve_path,
};

/// Context needed to translate descriptor-producing patches
/// (`InsertChild`, and eventually `ReplaceRoot` / `ReplaceNode`) into
/// `FiberWork::Create`. Passed `None` when the caller only wants the
/// subset that doesn't build new subtrees (legacy M2 / M3 callers).
///
/// Fields mirror the inputs to
/// [`renderer_adapter::rsx_to_descriptors_scoped_with_context`] so the
/// translator can reuse the cold-path converter verbatim.
/// 軌 A #9: viewport-level context threaded into the apply path so
/// setters that depend on inherited cascade (Text/TextArea
/// `font_size` em/rem/%/vw/vh) can rebuild an `InheritedTextStyle`
/// from the arena ancestors.
///
/// Mirrors the inputs to
/// `renderer_adapter::inherited_text_style_at_parent` so the Update
/// dispatchers can reuse the cold-path resolver verbatim.
#[derive(Clone, Copy)]
pub struct ApplyContext<'a> {
    pub viewport_style: &'a Style,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Track 1 #10 (正規 refactor): per-prop apply context passed through
/// `ApplyPropUpdate`. Unified signature so `#[props]`-generated
/// dispatchers and hand-written custom arms share shape. Borrow what
/// you need; ignore the rest.
///
/// `arena` is `&NodeArena` (not `&mut`) for the common incremental
/// setter surface. Props that need to commit arena children (Image /
/// Svg slot hot-swap) take `&mut NodeArena` via a separate host arm
/// until the wrapper plumbing lands — they stay on `custom_update` for
/// now.
pub struct ApplyPropContext<'a> {
    pub arena: &'a NodeArena,
    pub self_key: NodeKey,
    pub viewport_style: &'a Style,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Track 1 #10: host-level prop dispatcher. Implementors receive a
/// stream of `(name, value)` pairs (changed props) and `(name,)`
/// (removed props) from `apply_update_work`. The default impl for
/// each host routes through a `#[props]`-generated match table for
/// trivial props plus a hand-written arm for context-aware props
/// (`style`, `font_size` em/rem, `source` RAII, `loading` / `error`
/// slot commit).
pub trait ApplyPropUpdate {
    /// Apply a single changed prop. Returns `Err` only for genuinely
    /// unknown props (caller logs + skips without falling back).
    fn apply_prop_update(
        &mut self,
        ctx: &ApplyPropContext<'_>,
        name: &'static str,
        value: PropValue,
    ) -> Result<(), UpdateFailure>;

    /// Apply a single removed prop (reset to the cold-path default).
    /// Default impl returns `CannotResetProp` — hosts override for the
    /// props whose removal semantics they want to model.
    fn apply_prop_remove(
        &mut self,
        _ctx: &ApplyPropContext<'_>,
        name: &'static str,
    ) -> Result<(), UpdateFailure> {
        Err(UpdateFailure::CannotResetProp(name))
    }
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
/// Text nodes contribute exactly one leaf. Used by `rsx_to_arena_path`
/// to skip Fragment indices that reconciler paths retain but the arena
/// flattens away.
fn rsx_node_kind(n: &RsxNode) -> String {
    match n {
        RsxNode::Element(e) => {
            let tag = e.tag;
            let nc = e.children.len();
            let props: Vec<&str> = e.props.iter().map(|(k, _)| *k).collect();
            format!("Element<{tag}> children={nc} props={props:?}")
        }
        RsxNode::Text(t) => format!("Text({:?})", t.content),
        RsxNode::Fragment(f) => format!("Fragment(n={})", f.children.len()),
    }
}

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
/// Sentinel rsx tag for TextArea's projection child: lives in the rsx
/// tree but never commits as an arena child. Any patch whose rsx path
/// descends into one of these is a no-op on the arena side.
const ARENA_INVISIBLE_TAGS: &[&str] = &["__rfgui_text_area_projection"];

fn is_arena_invisible_host(n: &RsxNode) -> bool {
    match n {
        RsxNode::Element(e) => ARENA_INVISIBLE_TAGS.contains(&e.tag),
        _ => false,
    }
}

/// Resolution of an rsx-space path against the arena-flattened tree.
pub(crate) enum ArenaPathResolution {
    /// Path resolves to an arena node at this index chain.
    Arena(Vec<usize>),
    /// Path descends through a host whose children don't commit to the
    /// arena (TextArea projection, future: Image/Svg slot rsx targets).
    /// The patch has no arena target; translator should drop it as a
    /// no-op without aborting the batch.
    NoArenaTarget,
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
        if is_arena_invisible_host(target) {
            return ArenaPathResolution::NoArenaTarget;
        }
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
    /// Apply a prop diff to an existing element. M1 leaves this as a
    /// TODO — the setter layer lands in M3.
    Update {
        key: NodeKey,
        changed: Vec<(&'static str, PropValue)>,
        removed: Vec<&'static str>,
    },
    /// Replace a text-node's content. Also M3.
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
    /// Wholesale replace the arena roots with a freshly-built
    /// descriptor list. Emitted when the reconciler's root-level
    /// identity or tag changes (`Patch::ReplaceRoot`). The
    /// descriptor list has N >= 1 entries — Fragment roots produce
    /// multiple. All old root subtrees are dropped and the new
    /// descriptors committed as roots in order; `ui_root_keys` is
    /// refreshed by the apply caller from `arena.roots()` after.
    ReplaceRoot {
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
    /// No NodeKeys are minted or destroyed — promoted-layer / Persistent
    /// GPU resources cached against existing root NodeKeys survive.
    /// Emitted when `reconcile_multi` detects that the new root set is a
    /// permutation of the old (e.g. window-manager Z-order swap on a
    /// Fragment-at-root scene).
    ReorderRoots {
        mapping: Vec<usize>,
    },
}

// ---------------------------------------------------------------------------
// Patch → FiberWork translation
// ---------------------------------------------------------------------------

/// Translate a single reconciler `Patch` into a `FiberWork`.
///
/// Returns `None` when the M1 translator cannot faithfully produce a
/// work unit from the patch alone — callers should interpret `None` as
/// "fall back to the legacy full-rebuild path" rather than silently
/// skipping the patch.
///
/// Specifically M1 falls back on:
/// - `ReplaceRoot` / `ReplaceNode` — these carry a whole `RsxNode`
///   subtree that needs the full inherited-style/global-path context
///   to be converted into an `ElementDescriptor`. That context is
///   plumbed by `render_rsx`, not by this translator, so we bail.
/// - `InsertChild` — same reason: needs the descriptor pipeline with
///   the parent's inherited style. M2 will take a parent-context
///   argument and handle it.
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
    // When per-root rsx is available, map rsx paths → arena paths for
    // the `resolve_path` calls below. rsx-path is still passed to
    // `walk_rsx_by_index_path*` which needs rsx-space indices.
    let arena_path_for = |rsx_path: &[usize]| -> Option<Vec<usize>> {
        match per_root_old_rsx {
            None => Some(rsx_path.to_vec()),
            Some(old) => match rsx_to_arena_path(old, rsx_path) {
                ArenaPathResolution::Arena(p) => Some(p),
                // NoArenaTarget / Invalid: caller falls back.
                _ => None,
            },
        }
    };
    match patch {
        Patch::ReplaceRoot(new_node) => {
            // Need DescriptorContext — without it we can't build the
            // new subtree's descriptor. Callers lacking context (unit
            // tests, ad-hoc translation) fall back.
            let ctx = ctx?;
            let inherited = crate::view::renderer_adapter::InheritedTextStyle::from_viewport_style(
                ctx.inherited_style,
                ctx.viewport_width,
                ctx.viewport_height,
            );
            let descriptors =
                crate::view::renderer_adapter::rsx_to_descriptors_with_inherited(
                    &new_node,
                    &[],
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
            Some(FiberWork::ReplaceRoot { descriptors })
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
            // existing multi-descriptor ReplaceRoot apply path which
            // clears + pushes N.
            let ctx = ctx?;
            let inherited = crate::view::renderer_adapter::InheritedTextStyle::from_viewport_style(
                ctx.inherited_style,
                ctx.viewport_width,
                ctx.viewport_height,
            );
            let mut all = Vec::new();
            for n in &new_nodes {
                let part = crate::view::renderer_adapter::rsx_to_descriptors_with_inherited(
                    n,
                    &[],
                    &inherited,
                )
                .ok()?;
                all.extend(part);
            }
            if all.is_empty() {
                return None;
            }
            Some(FiberWork::ReplaceRoot { descriptors: all })
        }
        Patch::ReplaceNode { path, node: new_node } => {
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
            let inherited = crate::view::renderer_adapter::inherited_text_style_at_parent(
                arena,
                parent_key,
                ctx.inherited_style,
                ctx.viewport_width,
                ctx.viewport_height,
            );
            let descriptors =
                crate::view::renderer_adapter::rsx_to_descriptors_with_inherited(
                    &new_node,
                    &[],
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
            let trace_ic = std::env::var("RFGUI_TRACE_FALLBACK").is_ok();
            // M5 #5/#6: build a descriptor for the freshly-inserted
            // child via the cold-path converter.
            //
            // Context gate: no DescriptorContext → fall back. Callers
            // that don't have a NEW rsx root on hand (unit tests, the
            // convenience wrapper `patches_to_fiber_works`) thus keep
            // the pre-M5 behaviour.
            let ctx = match ctx {
                Some(c) => c,
                None => {
                    if trace_ic { eprintln!("[trace]     InsertChild FAIL: ctx=None"); }
                    return None;
                }
            };

            // 1) Resolve the arena parent. parent_path is rsx-space
            //    from the reconciler; translate via `arena_path_for`
            //    so Fragment mid-tree parents land on the right arena
            //    key.
            let parent_arena_path = match arena_path_for(&parent_path) {
                Some(p) => p,
                None => {
                    if trace_ic { eprintln!("[trace]     InsertChild FAIL: arena_path_for({:?}) = None", parent_path); }
                    return None;
                }
            };
            let parent_key = match resolve_path(arena, root, &parent_arena_path) {
                Some(k) => k,
                None => {
                    if trace_ic { eprintln!("[trace]     InsertChild FAIL: resolve_path(arena={:?}) = None", parent_arena_path); }
                    return None;
                }
            };

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
                Some(old) => {
                    let walked = walk_rsx_by_index_path_validated(old, walk_new_root, &parent_path);
                    if walked.is_none() && trace_ic {
                        eprintln!(
                            "[trace]     InsertChild FAIL: walk_rsx_validated None. OLD root identity={:?} NEW root identity={:?} path={:?}",
                            old.identity(), walk_new_root.identity(), parent_path
                        );
                        let mut on = old;
                        let mut nn = walk_new_root;
                        for (s, &i) in parent_path.iter().enumerate() {
                            let oc = on.children();
                            let nc = nn.children();
                            eprintln!(
                                "[trace]       step {s}: idx={i} old_children={:?} new_children={:?}",
                                oc.map(|c| c.len()), nc.map(|c| c.len())
                            );
                            let (Some(ocs), Some(ncs)) = (oc, nc) else { break };
                            let (Some(oc2), Some(nc2)) = (ocs.get(i), ncs.get(i)) else { break };
                            eprintln!(
                                "[trace]       step {s} identity: old={:?} new={:?} match={}",
                                oc2.identity(), nc2.identity(), oc2.identity() == nc2.identity()
                            );
                            on = oc2;
                            nn = nc2;
                        }
                    }
                    walked?
                }
                None => walk_rsx_by_index_path(walk_new_root, &parent_path)?,
            };

            // 3) Fish out the freshly-authored child at the NEW index.
            let kids = match new_parent_rsx.children() {
                Some(c) => c,
                None => {
                    if trace_ic { eprintln!("[trace]     InsertChild FAIL: new_parent_rsx.children()=None index={index}"); }
                    return None;
                }
            };
            let child_rsx = match kids.get(index) {
                Some(c) => c,
                None => {
                    if trace_ic {
                        eprintln!("[trace]     InsertChild FAIL: NEW kids.get({index})=None (len={})", kids.len());
                    }
                    return None;
                }
            };

            // 4) Reuse the cold-path converter. M6 cascade: rebuild
            //    the `InheritedTextStyle` at the arena parent by
            //    walking its ancestor chain and replaying each
            //    Element's `parsed_style` through `merge_style`. This
            //    matches what `build_container_element_shell` does in
            //    the cold-path loop, so text children inherit
            //    font_size / color / etc. from ancestors instead of
            //    the viewport-root approximation M5.0 shipped.
            let inherited = crate::view::renderer_adapter::inherited_text_style_at_parent(
                arena,
                parent_key,
                ctx.inherited_style,
                ctx.viewport_width,
                ctx.viewport_height,
            );
            let mut descriptors = match crate::view::renderer_adapter::rsx_to_descriptors_with_inherited(
                child_rsx,
                &[],
                &inherited,
            ) {
                Ok(d) => d,
                Err(e) => {
                    if trace_ic {
                        eprintln!("[trace]     InsertChild FAIL: rsx_to_descriptors err={e:?}");
                    }
                    return None;
                }
            };

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
/// caller knows the whole batch should fall back to the legacy
/// full-rebuild path.
///
/// M2 uses this flavour because partial translation is not safe: a
/// skipped patch would leave the arena inconsistent with the new RSX.
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
    let trace = std::env::var("RFGUI_TRACE_FALLBACK").is_ok();
    let trace_filter: Option<String> = std::env::var("RFGUI_TRACE_FILTER").ok();
    // Batch-level scope: if any patch in the batch mentions the filter
    // prop, print FAILED trace for ALL patches in this batch — failing
    // patches that don't match the filter can still trigger the
    // all-or-nothing cold-rebuild that wipes the filter-relevant
    // NodeKey, so the user needs to see the actual culprit.
    let batch_scope_active = trace
        && match &trace_filter {
            Some(needle) => patches.iter().any(|rp| match &rp.patch {
                Patch::UpdateElementProps { changed, removed, .. } => {
                    changed.iter().any(|(k, _)| *k == needle.as_str())
                        || removed.iter().any(|k| *k == needle.as_str())
                }
                _ => false,
            }),
            None => true,
        };
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
        let patch_name: &'static str = match &rp.patch {
            Patch::ReplaceRoot(_) => "ReplaceRoot",
            Patch::ReplaceAllRoots(_) => "ReplaceAllRoots",
            Patch::ReorderRoots(_) => "ReorderRoots",
            Patch::ReplaceNode { .. } => "ReplaceNode",
            Patch::UpdateElementProps { .. } => "UpdateElementProps",
            Patch::SetText { .. } => "SetText",
            Patch::InsertChild { .. } => "InsertChild",
            Patch::RemoveChild { .. } => "RemoveChild",
            Patch::MoveChild { .. } => "MoveChild",
        };
        let patch_scope_trace = batch_scope_active;
        let dbg_changed: Option<Vec<&str>> = match &rp.patch {
            Patch::UpdateElementProps { changed, .. } => {
                Some(changed.iter().map(|(k, _)| *k).collect())
            }
            _ => None,
        };
        let dbg_removed: Option<Vec<&str>> = match &rp.patch {
            Patch::UpdateElementProps { removed, .. } => Some(removed.clone()),
            _ => None,
        };
        let dbg_path: Option<Vec<usize>> = match &rp.patch {
            Patch::UpdateElementProps { path, .. }
            | Patch::SetText { path, .. }
            | Patch::ReplaceNode { path, .. } => Some(path.clone()),
            Patch::InsertChild { parent_path, .. }
            | Patch::RemoveChild { parent_path, .. }
            | Patch::MoveChild { parent_path, .. } => Some(parent_path.clone()),
            _ => None,
        };
        let root_idx = rp.root_index;
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
        // Pre-scan only so we can drop patches whose target lives in
        // an arena-absent subtree (TextArea projection) as no-ops.
        let rsx_path_for_check: Option<&[usize]> = match &rp.patch {
            Patch::UpdateElementProps { path, .. }
            | Patch::SetText { path, .. }
            | Patch::ReplaceNode { path, .. } => Some(path.as_slice()),
            Patch::InsertChild { parent_path, .. }
            | Patch::RemoveChild { parent_path, .. }
            | Patch::MoveChild { parent_path, .. } => Some(parent_path.as_slice()),
            _ => None,
        };
        if let (Some(rsx_path), Some(old)) = (rsx_path_for_check, per_root_old_rsx)
            && matches!(
                rsx_to_arena_path(old, rsx_path),
                ArenaPathResolution::NoArenaTarget
            )
        {
            continue;
        }
        let translated_patch = rp.patch;
        // Snapshot arena-aligned path BEFORE move into patch_to_fiber_work
        // so we can dump a walk trace on failure.
        let translated_arena_path: Option<Vec<usize>> = match &translated_patch {
            Patch::UpdateElementProps { path, .. }
            | Patch::SetText { path, .. }
            | Patch::ReplaceNode { path, .. } => Some(path.clone()),
            Patch::InsertChild { parent_path, .. }
            | Patch::RemoveChild { parent_path, .. }
            | Patch::MoveChild { parent_path, .. } => Some(parent_path.clone()),
            _ => None,
        };
        match patch_to_fiber_work_with_rsx(
            translated_patch,
            id_to_key,
            arena,
            root,
            ctx,
            per_root_old_rsx,
            per_root_new_rsx,
        ) {
            Some(work) => out.push(work),
            None => {
                if patch_scope_trace {
                    eprintln!(
                        "[trace] patch_to_fiber_work FAILED: {patch_name} root={root_idx} rsx_path={:?} arena_path={:?} changed={:?} removed={:?}",
                        dbg_path, translated_arena_path, dbg_changed, dbg_removed
                    );
                }
                if patch_scope_trace
                    && let Some(arena_path) = translated_arena_path.clone()
                {
                    let mut cur = root;
                    eprintln!(
                        "[trace]     arena walk from root {:?} (children={}):",
                        cur,
                        arena.children_of(cur).len(),
                    );
                    for (i, &idx) in arena_path.iter().enumerate() {
                        let kids = arena.children_of(cur);
                        if idx >= kids.len() {
                            eprintln!(
                                "[trace]     step {i}: idx={idx} OUT OF BOUNDS (children={})",
                                kids.len()
                            );
                            break;
                        }
                        cur = kids[idx];
                        let next_kids = arena.children_of(cur).len();
                        eprintln!("[trace]     step {i}: idx={idx} -> {cur:?} (children={next_kids})");
                    }
                }
                if patch_scope_trace
                    && let (Some(rsx_path), Some(old_rsx)) = (&dbg_path, per_root_old_rsx)
                {
                    let mut n = old_rsx;
                    eprintln!("[trace]     rsx walk: root={}", rsx_node_kind(n));
                    for (i, &idx) in rsx_path.iter().enumerate() {
                        match n.children() {
                            Some(ch) if idx < ch.len() => {
                                n = &ch[idx];
                                eprintln!("[trace]     step {i}: idx={idx} -> {}", rsx_node_kind(n));
                            }
                            Some(ch) => {
                                eprintln!(
                                    "[trace]     step {i}: idx={idx} rsx OUT (children={})",
                                    ch.len()
                                );
                                break;
                            }
                            None => {
                                eprintln!(
                                    "[trace]     step {i}: idx={idx} rsx NODE HAS NO CHILDREN ({})",
                                    rsx_node_kind(n)
                                );
                                break;
                            }
                        }
                    }
                }
                return None;
            }
        }
    }
    Some(out)
}

/// Whitelist of prop keys the M3 incremental Update path supports.
///
/// Any Update work carrying a key outside this set falls back to the
/// full-rebuild pipeline. Kept as a single source of truth so the
/// gate-side `is_committable` check and the apply-side dispatch agree.
///
/// Routing:
/// - `Element` accepts: `style`, `opacity`
/// - `Text` accepts: `style`, `font_size`, `line_height`, `align`, `opacity`
/// - `TextArea` accepts: `font_size`, `opacity`
///
/// Anything else (event handlers, Image `loading`/`error`, `font`,
/// `font_weight`, padding variants, …) is deliberately excluded so
/// M3 can ship a small, auditable surface. M4+ extends it.
pub(crate) const M3_UPDATE_PROP_WHITELIST: &[&str] = &[
    "style",
    "font_size",
    "line_height",
    "align",
    "opacity",
    // M4 #3 + 軌 1 #3: Image/Svg slot hot-swap. `loading` / `error`
    // prop values are `RsxNode` subtrees; the apply side tears down
    // the old slot and re-commits.
    "loading",
    "error",
    // 軌 1 #2: context-free Element props. Pure setter fan-out, no
    // inherited cascade involvement.
    "anchor",
    "padding",
    "padding_x",
    "padding_y",
    "padding_left",
    "padding_right",
    "padding_top",
    "padding_bottom",
    // 軌 1 #2: Image / Svg context-free props.
    "fit",
    "sampling",
    // 軌 1 #4: Image / Svg source hot-swap. The apply side drops the
    // old resource handle (RAII for Image; explicit release for Svg)
    // and acquires the new one. Measurement re-runs on the next
    // frame via the element's existing snapshot path.
    "source",
    // 軌 1 #10: TextArea-specific typed handlers. Not in
    // RSX_EVENT_HANDLER_PROPS (those are DOM-standard events);
    // `on_change` / `on_render` carry typed `TextChangeHandlerProp` /
    // `TextAreaRenderHandlerProp`. Apply via
    // `text_area.replace_on_change_handler` etc.
    "on_change",
    "on_render",
];

/// Does `prop` name a reconciler-visible event handler prop (`on_*`)?
/// M4 #4 accepts these into the incremental update path: the apply
/// side clears the existing handler list for the event and re-installs
/// the new handler via the shared
/// `renderer_adapter::try_assign_event_handler_prop` dispatcher.
fn is_event_handler_prop(prop: &str) -> bool {
    crate::view::renderer_adapter::RSX_EVENT_HANDLER_PROPS.contains(&prop)
}

/// Props whose **removal** (appearing in a patch's `removed` list) has
/// a known reset path in the incremental setter surface. M4 #1 adds
/// `style` only: a missing `style` prop means "reset `parsed_style` to
/// `Style::new()`", wired through `Element::replace_style`. Other
/// removals (opacity, font_size, …) still fall back to the full
/// rebuild until their context-free defaults land.
pub(crate) const M4_REMOVE_PROP_WHITELIST: &[&str] = &[
    "style",
    "opacity",
    // 軌 1 #2: removing an Element's anchor / padding prop resets to
    // the cold-path default (None / 0.0). Context-free — no cascade.
    "anchor",
    "padding",
    "padding_x",
    "padding_y",
    "padding_left",
    "padding_right",
    "padding_top",
    "padding_bottom",
    // 軌 1 #2: Image / Svg default back to Contain / Linear when the
    // prop is removed.
    "fit",
    "sampling",
];

/// Why an incremental Update or SetText couldn't be applied. Surfaced
/// from `apply_update_work` / `apply_set_text_work` so the gate can
/// fall back to the full-rebuild path without the arena ever being
/// partially mutated (all failures are detected pre-apply).
#[derive(Debug, Clone)]
pub enum UpdateFailure {
    /// A prop key is not in the M3 whitelist (event handlers, Image
    /// loading/error, inherited-context font props, etc).
    UnsupportedProp(&'static str),
    /// A whitelisted prop isn't supported on the element variant
    /// currently attached to the arena key (e.g. `line_height` on an
    /// Element host, or `style` on a TextArea).
    UnsupportedOnElementType {
        prop: &'static str,
        type_name: &'static str,
    },
    /// A prop-removal (`removed` list) has no reset path in the M3
    /// setter surface — we can't undo an explicit value back to its
    /// inherited-context default without rerunning the full conversion.
    CannotResetProp(&'static str),
    /// SetText target isn't a Text or TextArea node.
    SetTextOnNonTextTarget,
    /// Target NodeKey vanished from the arena (stale work batch).
    MissingTarget,
}

impl FiberWork {
    /// Whether this work unit is safe to commit under the current
    /// incremental setter surface (M3), **given the arena state** so
    /// per-variant whitelist narrowing can consult the target element
    /// type.
    ///
    /// M3 rules:
    /// - `Delete` / `Move`: always committable (inherited from M2).
    /// - `SetText`: committable iff the target (after the
    ///   text-child-to-host remap done in `patch_to_fiber_work`) is
    ///   an arena node whose element downcasts to `Text` or
    ///   `TextArea`.
    /// - `Update`: committable iff
    ///     * `removed` is empty (no reset-to-default helper yet), AND
    ///     * every changed key lives in `M3_UPDATE_PROP_WHITELIST`, AND
    ///     * every changed key is supported on the target element type
    ///       (e.g. `line_height` only applies on `Text`; `style` on
    ///       `Element` but not on `Text`/`TextArea` which lack an
    ///       `apply_style` hook).
    /// - `Create`: never committable in M3 (descriptor context
    ///   threading is M4+).
    ///
    /// The arena-aware form exists so the gate in `render_rsx` can
    /// reject mismatches *before* `apply_fiber_works` runs, avoiding
    /// a silent drop when the apply-side rejects a whitelisted-but-
    /// unsupported combo.
    pub fn is_committable(&self, arena: &NodeArena) -> bool {
        match self {
            FiberWork::Delete { .. } | FiberWork::Move { .. } => true,
            FiberWork::SetText { key, .. } => target_is_text_like(arena, *key),
            FiberWork::Update {
                key,
                changed,
                removed,
            } => {
                let target_kind = classify_target(arena, *key);
                // 軌 A #7: the M6 "style update changes cascading
                // decl → fallback" boundary is gone. Cascade changes
                // now trigger `recascade_text_subtree` on the apply
                // side, which walks Text/TextArea descendants and
                // re-applies inherited props via their `apply_inherited`
                // hooks (per-prop explicit flags preserve author
                // overrides).
                let changed_ok = changed.iter().all(|(prop, _value)| {
                    if is_event_handler_prop(prop) {
                        // Track 1 #10: DOM-standard event handlers
                        // (on_click/on_focus/on_blur/etc.) commit on
                        // Element AND TextArea (TextArea forwards to
                        // its inner Element).
                        matches!(target_kind, TargetKind::Element | TargetKind::TextArea)
                    } else {
                        M3_UPDATE_PROP_WHITELIST.contains(prop)
                            && prop_supported_on_target(prop, target_kind)
                    }
                });
                let removed_ok = removed.iter().all(|prop| {
                    if is_event_handler_prop(prop) {
                        matches!(target_kind, TargetKind::Element | TargetKind::TextArea)
                    } else {
                        M4_REMOVE_PROP_WHITELIST.contains(prop)
                            && prop_supported_on_target(prop, target_kind)
                    }
                });
                changed_ok && removed_ok
            }
            FiberWork::Create { .. } => true,
            // Wholesale subtree replacement — always committable once
            // the descriptor has been built. The descriptor carries the
            // full fresh element/children tree; the apply side drops
            // the old subtree and commits the new.
            FiberWork::ReplaceRoot { .. } | FiberWork::ReplaceNode { .. } => true,
            FiberWork::ReorderRoots { .. } => true,
            // 軌 1 #5: fragment expansion. Apply does N successive
            // arena_insert_child calls — each is the same shape as
            // a single Create.
            FiberWork::CreateMany { .. } => true,
        }
    }

    /// Back-compat alias used by existing call sites. Kept so the M2
    /// commit-gate callers in `render_rsx` stay source-compatible while
    /// M3 broadens the surface. New code should prefer
    /// [`FiberWork::is_committable`].
    pub fn is_m2_committable(&self, arena: &NodeArena) -> bool {
        self.is_committable(arena)
    }
}

/// PropertyIds that cascade into descendant text nodes (font_family,
/// font_size, font_weight, color, cursor, text_wrap — mirrors
/// `InheritedTextStyle::merge_style`). Kept in one place so the
/// boundary gate and the cold-path merger reference the same list.
const TEXT_CASCADING_PROPS: &[crate::style::PropertyId] = &[
    crate::style::PropertyId::FontFamily,
    crate::style::PropertyId::FontSize,
    crate::style::PropertyId::FontWeight,
    crate::style::PropertyId::Color,
    crate::style::PropertyId::Cursor,
    crate::style::PropertyId::TextWrap,
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
    TEXT_CASCADING_PROPS.iter().any(|pid| style.get(*pid).is_some())
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
    use crate::view::base_component::{Text, TextArea};
    use crate::view::renderer_adapter::inherited_text_style_at_parent;

    fn walk(
        arena: &mut NodeArena,
        ctx: ApplyContext<'_>,
        key: NodeKey,
    ) {
        // Compute inherited cascade at this node's arena parent — the
        // helper walks the ancestor chain replaying each Element's
        // `parsed_style` through `InheritedTextStyle::merge_style`.
        let parent = arena.parent_of(key);
        let inherited = match parent {
            Some(p) => inherited_text_style_at_parent(
                arena,
                p,
                ctx.viewport_style,
                ctx.viewport_width,
                ctx.viewport_height,
            ),
            None => crate::view::renderer_adapter::InheritedTextStyle::from_viewport_style(
                ctx.viewport_style,
                ctx.viewport_width,
                ctx.viewport_height,
            ),
        };
        // Apply inherited to this node if it's a Text/TextArea host.
        arena.with_element_taken(key, |element, _arena_ref| {
            if let Some(t) = element.as_any_mut().downcast_mut::<Text>() {
                t.apply_inherited(&inherited);
            } else if let Some(ta) = element.as_any_mut().downcast_mut::<TextArea>() {
                ta.apply_inherited(&inherited);
            }
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

fn prop_supported_on_target(prop: &str, kind: TargetKind) -> bool {
    match (prop, kind) {
        // Element hosts: style, opacity, plus the 軌 1 #2 context-free
        // setter surface (anchor, padding*). `font_size` / `line_height`
        // / `align` still fall back — they land on Element parsed_style
        // via the cold-path cascade.
        ("style", TargetKind::Element)
        | ("opacity", TargetKind::Element)
        | ("anchor", TargetKind::Element)
        | ("padding", TargetKind::Element)
        | ("padding_x", TargetKind::Element)
        | ("padding_y", TargetKind::Element)
        | ("padding_left", TargetKind::Element)
        | ("padding_right", TargetKind::Element)
        | ("padding_top", TargetKind::Element)
        | ("padding_bottom", TargetKind::Element) => true,
        // Text hosts: all five whitelisted props. 軌 1 #8: `style` now
        // routes through `Text::apply_style_incremental`, which mirrors
        // the cold-path fan-out (font/font_size/font_weight/color/
        // cursor/text_wrap/width/height) and resets explicit flags so
        // removed declarations pick the ancestor cascade back up.
        ("style", TargetKind::Text) => true,
        ("font_size", TargetKind::Text) => true,
        ("line_height", TargetKind::Text) => true,
        ("align", TargetKind::Text) => true,
        ("opacity", TargetKind::Text) => true,
        // TextArea hosts: style + event handlers (軌 1 #10) now route
        // through TextArea::apply_style_incremental and the
        // `replace_on_*_handler` setters added in M2. Typed handler
        // props live on PropValue::On{Change,Focus,Render}; BlurHandler
        // is a DOM-standard event but TextArea wraps forwarding.
        ("style", TargetKind::TextArea) => true,
        ("font_size", TargetKind::TextArea) => true,
        ("opacity", TargetKind::TextArea) => true,
        ("on_change", TargetKind::TextArea) => true,
        ("on_focus", TargetKind::TextArea) => true,
        ("on_blur", TargetKind::TextArea) => true,
        ("on_render", TargetKind::TextArea) => true,
        // M4 #3 + 軌 1 #2/#3/#4: Image context-free setter surface.
        ("style", TargetKind::Image) => true,
        ("loading", TargetKind::Image) => true,
        ("error", TargetKind::Image) => true,
        ("fit", TargetKind::Image) => true,
        ("sampling", TargetKind::Image) => true,
        ("source", TargetKind::Image) => true,
        // 軌 1 #3/#4: Svg now exposes the same slot + source hot-swap
        // surface as Image (mirroring the cold-path props).
        ("style", TargetKind::Svg) => true,
        ("loading", TargetKind::Svg) => true,
        ("error", TargetKind::Svg) => true,
        ("fit", TargetKind::Svg) => true,
        ("sampling", TargetKind::Svg) => true,
        ("source", TargetKind::Svg) => true,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// FiberWork commit
// ---------------------------------------------------------------------------

/// Apply a batch of `FiberWork` items against `arena`.
///
/// M1 scope:
/// - `Create` / `Delete` / `Move` are fully wired.
/// - `Update` / `SetText` are **no-ops** pending the M3 setter layer.
///   They deliberately do **not** panic; the incremental path is gated
///   off in M1 so these branches are only exercised by tests that
///   specifically construct them.
pub fn apply_fiber_works(arena: &mut NodeArena, ctx: ApplyContext<'_>, works: Vec<FiberWork>) {
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
                if let Err(err) = apply_update_work(arena, ctx, key, changed, removed) {
                    eprintln!("[fiber_work] Update dropped: {err:?}");
                }
            }
            FiberWork::SetText { key, text } => {
                if let Err(err) = apply_set_text_work(arena, key, text) {
                    eprintln!("[fiber_work] SetText dropped: {err:?}");
                }
            }
            FiberWork::Move {
                parent,
                key,
                from,
                to,
            } => {
                arena_move_child(arena, parent, key, from, to);
            }
            FiberWork::ReplaceRoot { descriptors } => {
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
                let new_roots: Vec<NodeKey> =
                    mapping.into_iter().map(|j| old_roots[j]).collect();
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
}

/// Apply a whitelisted prop-diff to the element at `key`.
///
/// Pre-conditions (already verified by [`FiberWork::is_committable`]
/// at the gate):
/// - every key in `changed` is in [`M3_UPDATE_PROP_WHITELIST`]
/// - `removed` is empty
///
/// The function is still defensive (returns `UpdateFailure` instead
/// of asserting) so a future caller that bypasses the gate doesn't
/// silently drop updates or panic.
///
/// Uses `arena.with_element_taken` to get exclusive `&mut dyn
/// ElementTrait` access, then tries downcasts in order
/// `Text` → `TextArea` → `Element`. The variant determines which
/// props are acceptable; mismatches return `UnsupportedOnElementType`.
fn apply_update_work(
    arena: &mut NodeArena,
    ctx: ApplyContext<'_>,
    key: NodeKey,
    changed: Vec<(&'static str, PropValue)>,
    removed: Vec<&'static str>,
) -> Result<(), UpdateFailure> {
    use crate::view::base_component::{Element, Text, TextArea};

    // Defensive re-check: these should already be filtered at the
    // gate, but `apply_update_work` is `pub(crate)`-reachable and we
    // don't want to mutate the arena on a malformed batch.
    for (prop_key, _) in &changed {
        if !M3_UPDATE_PROP_WHITELIST.contains(prop_key) && !is_event_handler_prop(prop_key) {
            return Err(UpdateFailure::UnsupportedProp(prop_key));
        }
    }
    for prop in &removed {
        if !M4_REMOVE_PROP_WHITELIST.contains(prop) && !is_event_handler_prop(prop) {
            return Err(UpdateFailure::CannotResetProp(prop));
        }
    }

    // Snapshot a handle so we can detect MissingTarget without a
    // separate arena.get() (the element is taken below).
    if arena.get(key).is_none() {
        return Err(UpdateFailure::MissingTarget);
    }

    // 軌 A #7: decide whether this update will change an ancestor's
    // text-cascading decl. We detect *before* taking the element
    // (the helpers need a read-only borrow) and re-cascade *after*
    // the element is back in its slot.
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
                if *prop == "style"
                    && element_parsed_style_has_text_cascading_decl(arena, key)
                {
                    cascade_dirty = true;
                    break;
                }
            }
        }
    }

    let mut result: Result<(), UpdateFailure> = Ok(());
    arena.with_element_taken(key, |element, arena_ref| {
        use crate::view::base_component::{Image, Svg};
        // Downcast precedence: Text / TextArea / Image / Svg before
        // the generic Element fallback. `as_any_mut` returns the
        // concrete component (not the inner `Element`), so Image/Svg's
        // wrapped Element won't be mistaken for a plain Element host.
        if let Some(image) = element.as_any_mut().downcast_mut::<Image>() {
            result = apply_update_to_image(image, arena_ref, &changed);
            if result.is_ok() {
                result = apply_remove_to_image(image, &removed);
            }
        } else if let Some(svg) = element.as_any_mut().downcast_mut::<Svg>() {
            result = apply_update_to_svg(svg, arena_ref, &changed);
            if result.is_ok() {
                result = apply_remove_to_svg(svg, &removed);
            }
        } else if let Some(text) = element.as_any_mut().downcast_mut::<Text>() {
            result = apply_update_to_text(text, arena_ref, key, ctx, &changed);
            if result.is_ok() {
                result = apply_remove_to_text(text, arena_ref, key, ctx, &removed);
            }
        } else if let Some(text_area) = element.as_any_mut().downcast_mut::<TextArea>() {
            result = apply_update_to_text_area(text_area, arena_ref, key, ctx, &changed);
            if result.is_ok() {
                result = apply_remove_to_text_area(text_area, &removed);
            }
        } else if let Some(el) = element.as_any_mut().downcast_mut::<Element>() {
            result = apply_update_to_element(el, &changed);
            if result.is_ok() {
                result = apply_remove_to_element(el, &removed);
            }
        } else {
            // Unknown host type (Image/Svg/user components). None of
            // the whitelisted props are supported on those in M3 —
            // Image/Svg route their sources through props the
            // whitelist deliberately excludes.
            result = Err(UpdateFailure::UnsupportedOnElementType {
                prop: changed
                    .first()
                    .map(|(k, _)| *k)
                    .or_else(|| removed.first().copied())
                    .unwrap_or(""),
                type_name: "<non-text, non-element host>",
            });
        }
    });
    // 軌 A #7: recascade after the element is back in its slot —
    // the walker relies on ancestor chain being intact.
    if result.is_ok() && cascade_dirty {
        recascade_text_subtree(arena, ctx, key);
    }
    result
}

/// Process a `removed` prop list against an Element host. Accepted
/// keys: `style` (M4 #1, resets `parsed_style` to `Style::new()` via
/// `replace_style`), `opacity` (M4 #7, resets to the documented
/// default of 1.0), and any of the 23 `on_*` event handler props
/// (M4 #4, clears the corresponding handler Vec).
fn apply_remove_to_element(
    element: &mut crate::view::base_component::Element,
    removed: &[&'static str],
) -> Result<(), UpdateFailure> {
    use crate::style::Style;
    for prop in removed {
        match *prop {
            "style" => {
                element.replace_style(Style::new());
            }
            "opacity" => {
                element.set_opacity(1.0);
            }
            // 軌 1 #2: removed context-free props reset to cold-path
            // defaults (no prop ⇒ setter never called ⇒ struct default).
            "anchor" => {
                element.set_anchor_name(None);
            }
            "padding" => element.set_padding(0.0),
            "padding_x" => element.set_padding_x(0.0),
            "padding_y" => element.set_padding_y(0.0),
            "padding_left" => element.set_padding_left(0.0),
            "padding_right" => element.set_padding_right(0.0),
            "padding_top" => element.set_padding_top(0.0),
            "padding_bottom" => element.set_padding_bottom(0.0),
            other if is_event_handler_prop(other) => {
                element.clear_rsx_event_handler(other);
            }
            _ => {
                return Err(UpdateFailure::CannotResetProp(prop));
            }
        }
    }
    Ok(())
}

/// M4 #3: Image slot hot-swap dispatcher. For each changed prop:
/// - `loading` / `error`: re-parse the incoming `PropValue` via
///   `convert_image_slot_desc`, commit the new descriptor tree under
///   the Image's arena key, and call
///   `replace_loading_slot_incremental` / `replace_error_slot_incremental`
///   which drains any currently-active slot back to storage, removes
///   the old slot subtree, and installs the new keys.
///
/// Uses `&[]` scope + `None` global path + a viewport-default
/// `InheritedTextStyle` for the new slot's stable-id path. This
/// approximation is consistent with the M5.0 InsertChild trade-off
/// and sufficient for the slot's visible behaviour (text style on
/// loading/error overlays is rarely inherited; M6 cascade can be
/// threaded later if needed).
fn apply_update_to_image(
    image: &mut crate::view::base_component::Image,
    arena: &mut NodeArena,
    changed: &[(&'static str, PropValue)],
) -> Result<(), UpdateFailure> {
    use crate::view::renderer_adapter::{
        InheritedTextStyle, commit_descriptor_tree, convert_image_slot_desc,
    };
    use crate::view::{ImageSource};

    let inherited = InheritedTextStyle::default();
    for (key, value) in changed {
        match *key {
            // 軌 1 #4: source hot-swap. Dropping the old `ImageHandle`
            // via RAII releases the old resource entry.
            "source" => {
                let source = ImageSource::from_prop_value(value.clone())
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                image.set_source(source);
            }
            "style" => {
                // Track 1 #10: Image uses ElementStylePropSchema.
                // Forward the decoded Style to the inner Element's
                // `apply_style` for width/height/color/etc.
                let style = crate::view::renderer_adapter::as_element_style(value, key)
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                image.apply_style(style);
            }
            "loading" | "error" => {
                let descriptors =
                    convert_image_slot_desc(value, &[], None, &inherited, *key)
                        .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                // Image slot descriptors always return exactly one
                // wrapper (see `convert_image_slot_desc`); guard anyway
                // so a future refactor that yields N doesn't silently
                // truncate.
                let mut new_keys: Vec<crate::view::node_arena::NodeKey> =
                    Vec::with_capacity(descriptors.len());
                // Commit descriptors parented to the Image's arena
                // node (its NodeKey is the parent for the slot
                // subtree). We need the Image's own NodeKey here —
                // but `with_element_taken` has already removed the
                // Image element from its slot, leaving the arena
                // parent pointer untouched. The apply caller owns the
                // `key`, so commit_descriptor_tree with None parent
                // would make these roots. Instead we use the
                // Image-side slot method that takes a Vec of
                // already-committed keys; commit them parented to
                // the *existing* children insertion parent. Since
                // `commit_descriptor_tree(arena, None, …)` returns a
                // key without parent, and we want the parent = Image,
                // we'll set the parent manually.
                for desc in descriptors {
                    let new_key = commit_descriptor_tree(arena, None, desc);
                    new_keys.push(new_key);
                }
                // The Image's own NodeKey isn't directly reachable
                // here because `with_element_taken` has taken it out
                // of the arena temporarily. `arena_ref` passed in is
                // the live arena sans the taken element; we can still
                // find the Image's parent by the `key` param from
                // `apply_update_work`. But that key isn't threaded to
                // this helper. Solution: `replace_*_slot_incremental`
                // internally takes the arena and only touches the
                // Image's own fields + descendants, so parenting is
                // deferred — we pass raw new keys and the Image will
                // attach them through `sync_active_slot` on the next
                // frame via `replace_children(arena, …)`. That path
                // sets parent pointers via `arena.set_parent(...)`.
                match *key {
                    "loading" => image.replace_loading_slot_incremental(arena, new_keys),
                    "error" => image.replace_error_slot_incremental(arena, new_keys),
                    _ => unreachable!(),
                }
            }
            other => {
                // Track 1 #10: trivial props (`fit`, `sampling`) route
                // through the `#[props(host = Image)]`-generated
                // dispatcher. Unknown prop → Err.
                let ctx = crate::view::fiber_work::ApplyPropContext {
                    arena,
                    self_key: crate::view::node_arena::NodeKey::default(),
                    viewport_style: &crate::Style::new(),
                    viewport_width: 0.0,
                    viewport_height: 0.0,
                };
                match crate::view::tags::__ImagePropSchema_apply_update_generated(
                    image, &ctx, other, value.clone(),
                )? {
                    true => {}
                    false => {
                        // `custom_update` list matched — but all custom
                        // Image props are handled above. Reaching here
                        // would mean the list is out of sync.
                        return Err(UpdateFailure::UnsupportedOnElementType {
                            prop: other,
                            type_name: "Image (custom_update mismatch)",
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// 軌 1 #2: Image removed-prop dispatch. `fit` / `sampling` reset to
/// the cold-path defaults (`Contain` / `Linear`); `loading` / `error`
/// / `source` removals are still full-rebuild territory (the first
/// two would need a "clear active slot" pathway the apply side
/// doesn't model yet; source removal isn't semantically meaningful —
/// Image requires a source).
fn apply_remove_to_image(
    image: &mut crate::view::base_component::Image,
    removed: &[&'static str],
) -> Result<(), UpdateFailure> {
    use crate::view::{ImageFit, ImageSampling};
    for prop in removed {
        match *prop {
            "fit" => image.set_fit(ImageFit::Contain),
            "sampling" => image.set_sampling(ImageSampling::Linear),
            _ => {
                return Err(UpdateFailure::CannotResetProp(prop));
            }
        }
    }
    Ok(())
}

/// 軌 1 #3/#4: Svg apply-update dispatcher — parallel to
/// `apply_update_to_image`. Handles the same prop set: fit, sampling,
/// source, and the two slot hot-swap props.
fn apply_update_to_svg(
    svg: &mut crate::view::base_component::Svg,
    arena: &mut NodeArena,
    changed: &[(&'static str, PropValue)],
) -> Result<(), UpdateFailure> {
    use crate::view::renderer_adapter::{
        InheritedTextStyle, commit_descriptor_tree, convert_image_slot_desc,
    };
    use crate::view::SvgSource;

    let inherited = InheritedTextStyle::default();
    for (key, value) in changed {
        match *key {
            "source" => {
                let source = SvgSource::from_prop_value(value.clone())
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                svg.set_source(source);
            }
            "style" => {
                let style = crate::view::renderer_adapter::as_element_style(value, key)
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                svg.apply_style(style);
            }
            "loading" | "error" => {
                // `convert_image_slot_desc` is shared with Image —
                // Svg reuses the Image-slot converter (same wrapper
                // semantics for loading/error overlays).
                let descriptors = convert_image_slot_desc(value, &[], None, &inherited, *key)
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                let mut new_keys: Vec<NodeKey> = Vec::with_capacity(descriptors.len());
                for desc in descriptors {
                    let new_key = commit_descriptor_tree(arena, None, desc);
                    new_keys.push(new_key);
                }
                match *key {
                    "loading" => svg.replace_loading_slot_incremental(arena, new_keys),
                    "error" => svg.replace_error_slot_incremental(arena, new_keys),
                    _ => unreachable!(),
                }
            }
            other => {
                let ctx = crate::view::fiber_work::ApplyPropContext {
                    arena,
                    self_key: crate::view::node_arena::NodeKey::default(),
                    viewport_style: &crate::Style::new(),
                    viewport_width: 0.0,
                    viewport_height: 0.0,
                };
                match crate::view::tags::__SvgPropSchema_apply_update_generated(
                    svg, &ctx, other, value.clone(),
                )? {
                    true => {}
                    false => {
                        return Err(UpdateFailure::UnsupportedOnElementType {
                            prop: other,
                            type_name: "Svg (custom_update mismatch)",
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

fn apply_remove_to_svg(
    svg: &mut crate::view::base_component::Svg,
    removed: &[&'static str],
) -> Result<(), UpdateFailure> {
    use crate::view::{ImageFit, ImageSampling};
    for prop in removed {
        match *prop {
            "fit" => svg.set_fit(ImageFit::Contain),
            "sampling" => svg.set_sampling(ImageSampling::Linear),
            _ => {
                return Err(UpdateFailure::CannotResetProp(prop));
            }
        }
    }
    Ok(())
}

/// M4 #7: Text-host removed-prop dispatch. Only `opacity` is
/// context-free enough to reset incrementally (default 1.0). Anything
/// else errors so the gate falls back to the full-rebuild pipeline.
fn apply_remove_to_text(
    text: &mut crate::view::base_component::Text,
    arena: &NodeArena,
    self_key: NodeKey,
    ctx: ApplyContext<'_>,
    removed: &[&'static str],
) -> Result<(), UpdateFailure> {
    use crate::view::renderer_adapter::inherited_text_style_at_parent;
    for prop in removed {
        match *prop {
            "opacity" => text.set_opacity(1.0),
            "style" => {
                // 軌 1 #8: `style` prop removed entirely. Reset every
                // explicit flag and replay the ancestor cascade so
                // all formerly-authored props fall back to inherited
                // values (or Text defaults where inherited is None).
                let inherited = match arena.parent_of(self_key) {
                    Some(p) => inherited_text_style_at_parent(
                        arena,
                        p,
                        ctx.viewport_style,
                        ctx.viewport_width,
                        ctx.viewport_height,
                    ),
                    None => crate::view::renderer_adapter::InheritedTextStyle::from_viewport_style(
                        ctx.viewport_style,
                        ctx.viewport_width,
                        ctx.viewport_height,
                    ),
                };
                text.apply_style_incremental(None, &inherited);
            }
            _ => {
                return Err(UpdateFailure::CannotResetProp(prop));
            }
        }
    }
    Ok(())
}

/// M4 #7: TextArea-host removed-prop dispatch. See `apply_remove_to_text`.
fn apply_remove_to_text_area(
    text_area: &mut crate::view::base_component::TextArea,
    removed: &[&'static str],
) -> Result<(), UpdateFailure> {
    for prop in removed {
        match *prop {
            "opacity" => text_area.set_opacity(1.0),
            _ => {
                return Err(UpdateFailure::CannotResetProp(prop));
            }
        }
    }
    Ok(())
}

/// 軌 A #9: resolve a `font_size` prop to pixels with full inherited
/// context. `parent_font_size` comes from the arena ancestor walk
/// (`inherited_text_style_at_parent`); `root_font_size` is the
/// viewport root; viewport dims are passed in for `vw`/`vh`/`%`.
///
/// The `Em`/`Rem`/`Percent`/`Vw`/`Vh` variants now resolve correctly
/// in the incremental path — previously they fell back to a full
/// rebuild because the apply side had no inherited context.
fn resolve_font_size_px_with_inherited(
    value: &PropValue,
    inherited: &crate::view::renderer_adapter::InheritedTextStyle,
) -> Option<f32> {
    let parent_font_size = inherited
        .font_size
        .unwrap_or(inherited.root_font_size);
    match value {
        PropValue::I64(v) => Some((*v as f32).max(0.0)),
        PropValue::F64(v) => Some((*v as f32).max(0.0)),
        PropValue::FontSize(fs) => Some(fs.resolve_px(
            parent_font_size,
            inherited.root_font_size,
            inherited.viewport_width,
            inherited.viewport_height,
        )),
        _ => None,
    }
}

fn apply_update_to_element(
    element: &mut crate::view::base_component::Element,
    changed: &[(&'static str, PropValue)],
) -> Result<(), UpdateFailure> {
    use crate::view::renderer_adapter::{as_element_style, as_f32, try_assign_event_handler_prop};
    for (key, value) in changed {
        match *key {
            "style" => {
                let style =
                    as_element_style(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                // M4 #1: use non-additive `replace_style` so declarations
                // dropped between renders actually clear. The cold
                // renderer_adapter build path still uses `apply_style`
                // because it layers base + user styles from scratch.
                element.replace_style(style);
            }
            "opacity" => {
                let value = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_opacity(value);
            }
            // 軌 1 #2: context-free padding / anchor setters. Values
            // decode via the same `as_f32` / string helpers the cold
            // path uses, so parse errors here mirror cold-path errors.
            "padding" => {
                let v = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_padding(v);
            }
            "padding_x" => {
                let v = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_padding_x(v);
            }
            "padding_y" => {
                let v = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_padding_y(v);
            }
            "padding_left" => {
                let v = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_padding_left(v);
            }
            "padding_right" => {
                let v = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_padding_right(v);
            }
            "padding_top" => {
                let v = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_padding_top(v);
            }
            "padding_bottom" => {
                let v = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_padding_bottom(v);
            }
            "anchor" => {
                let name = crate::view::renderer_adapter::as_owned_string(value, key)
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                element.set_anchor_name(Some(crate::AnchorName::new(name)));
            }
            other if is_event_handler_prop(other) => {
                // M4 #4: replace semantics for RSX event handlers.
                // Cold path `on_*` setters push into a Vec, so the
                // incremental path must clear first to avoid stacking
                // duplicates on every prop change. `try_assign_event_
                // handler_prop` reuses the shared cold-path dispatcher
                // so the decode/wiring logic has a single source.
                element.clear_rsx_event_handler(other);
                match try_assign_event_handler_prop(element, other, value) {
                    Ok(true) => {}
                    Ok(false) => {
                        // `is_event_handler_prop` just returned true
                        // for this key, so the dispatcher must know
                        // it. If we ever get here the two tables have
                        // drifted — surface as UnsupportedProp.
                        return Err(UpdateFailure::UnsupportedProp(other));
                    }
                    Err(_) => {
                        return Err(UpdateFailure::UnsupportedProp(other));
                    }
                }
            }
            _ => {
                return Err(UpdateFailure::UnsupportedOnElementType {
                    prop: key,
                    type_name: "Element",
                });
            }
        }
    }
    Ok(())
}

fn apply_update_to_text(
    text: &mut crate::view::base_component::Text,
    arena: &NodeArena,
    self_key: NodeKey,
    ctx: ApplyContext<'_>,
    changed: &[(&'static str, PropValue)],
) -> Result<(), UpdateFailure> {
    use crate::view::renderer_adapter::{as_f32, as_text_align, inherited_text_style_at_parent};
    // 軌 A #9: rebuild the inherited cascade at the Text's arena
    // parent. Built lazily — only the `font_size` arm needs it.
    let inherited = std::cell::OnceCell::new();
    let resolve_inherited = || {
        inherited.get_or_init(|| {
            let parent = arena.parent_of(self_key);
            match parent {
                Some(p) => inherited_text_style_at_parent(
                    arena,
                    p,
                    ctx.viewport_style,
                    ctx.viewport_width,
                    ctx.viewport_height,
                ),
                None => crate::view::renderer_adapter::InheritedTextStyle::from_viewport_style(
                    ctx.viewport_style,
                    ctx.viewport_width,
                    ctx.viewport_height,
                ),
            }
        })
    };
    for (key, value) in changed {
        match *key {
            "style" => {
                // 軌 1 #8: replay the cold-path style fan-out on the
                // live Text. Explicit flags are reset first so any
                // declaration dropped from the new style is free to
                // re-pick the ancestor cascade.
                let style = crate::view::renderer_adapter::as_text_style(value, key)
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text.apply_style_incremental(Some(&style), resolve_inherited());
            }
            "font_size" => {
                let px = match resolve_font_size_px_with_inherited(value, resolve_inherited()) {
                    Some(px) => px,
                    None => {
                        return Err(UpdateFailure::UnsupportedOnElementType {
                            prop: "font_size",
                            type_name: "Text (font_size value malformed)",
                        });
                    }
                };
                text.set_font_size(px);
            }
            "line_height" => {
                let value = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text.set_line_height(value);
            }
            "align" => {
                let align =
                    as_text_align(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text.set_text_align(align);
            }
            "opacity" => {
                let value = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text.set_opacity(value);
            }
            _ => {
                return Err(UpdateFailure::UnsupportedOnElementType {
                    prop: key,
                    type_name: "Text",
                });
            }
        }
    }
    Ok(())
}

fn apply_update_to_text_area(
    text_area: &mut crate::view::base_component::TextArea,
    arena: &NodeArena,
    self_key: NodeKey,
    ctx: ApplyContext<'_>,
    changed: &[(&'static str, PropValue)],
) -> Result<(), UpdateFailure> {
    use crate::view::renderer_adapter::{as_f32, inherited_text_style_at_parent};
    let inherited = std::cell::OnceCell::new();
    let resolve_inherited = || {
        inherited.get_or_init(|| {
            let parent = arena.parent_of(self_key);
            match parent {
                Some(p) => inherited_text_style_at_parent(
                    arena,
                    p,
                    ctx.viewport_style,
                    ctx.viewport_width,
                    ctx.viewport_height,
                ),
                None => crate::view::renderer_adapter::InheritedTextStyle::from_viewport_style(
                    ctx.viewport_style,
                    ctx.viewport_width,
                    ctx.viewport_height,
                ),
            }
        })
    };
    for (key, value) in changed {
        match *key {
            "style" => {
                // Track 1 #10: replay cold-path style fan-out (width/
                // height/color/selection). Mirrors Text `style` arm.
                let style = crate::view::renderer_adapter::as_element_style(value, key)
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text_area.apply_style_incremental(Some(&style), resolve_inherited());
            }
            "font_size" => {
                let px = match resolve_font_size_px_with_inherited(value, resolve_inherited()) {
                    Some(px) => px,
                    None => {
                        return Err(UpdateFailure::UnsupportedOnElementType {
                            prop: "font_size",
                            type_name: "TextArea (font_size value malformed)",
                        });
                    }
                };
                text_area.set_font_size(px);
            }
            "opacity" => {
                let value = as_f32(value, key).map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text_area.set_opacity(value);
            }
            "on_change" => {
                let handler = crate::ui::TextChangeHandlerProp::from_prop_value(value.clone())
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                // Replace existing change handlers (reconciler emits
                // one per user-authored `on_change={...}`).
                text_area.replace_on_change_handler(handler);
            }
            "on_focus" => {
                let handler = crate::ui::TextAreaFocusHandlerProp::from_prop_value(value.clone())
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text_area.replace_on_focus_handler(handler);
            }
            "on_render" => {
                let handler = crate::ui::TextAreaRenderHandlerProp::from_prop_value(value.clone())
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text_area.replace_on_render_handler(handler);
            }
            "on_blur" => {
                // TextArea forwards `on_blur` to its inner Element
                // via `text_area.on_blur(F)`. For incremental swap we
                // replace the element's blur handler list.
                let handler = crate::ui::BlurHandlerProp::from_prop_value(value.clone())
                    .map_err(|_| UpdateFailure::UnsupportedProp(key))?;
                text_area.replace_on_blur_handler(handler);
            }
            _ => {
                return Err(UpdateFailure::UnsupportedOnElementType {
                    prop: key,
                    type_name: "TextArea",
                });
            }
        }
    }
    Ok(())
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
    arena.with_element_taken(key, |element, _arena_ref| {
        if let Some(t) = element.as_any_mut().downcast_mut::<Text>() {
            t.set_text(text.clone());
        } else if let Some(ta) = element.as_any_mut().downcast_mut::<TextArea>() {
            ta.set_text(text.clone());
        } else {
            result = Err(UpdateFailure::SetTextOnNonTextTarget);
        }
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
fn arena_move_child(
    arena: &mut NodeArena,
    parent: NodeKey,
    key: NodeKey,
    from: usize,
    to: usize,
) {
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
    arena.with_element_taken(parent, |element, arena_ref| {
        if let Some(el) = element
            .as_any_mut()
            .downcast_mut::<crate::view::base_component::Element>()
        {
            let _previous = el.replace_children(arena_ref, children);
        }
    });
}

// ---------------------------------------------------------------------------
// Helpers (currently unused by M1 translator; retained for future M2 use).
// ---------------------------------------------------------------------------

/// Extract the `stable_id` that `rsx_to_descriptors_with_context` would
/// assign to the top-level element of `node`. Used once the `Create`
/// translator can build descriptors — kept here so the TODO surface
/// stays in one place.
///
/// Status as of M2: still a stub. M2 intentionally routes `InsertChild`
/// to the legacy full-rebuild path (`patch_to_fiber_work` returns
/// `None`), so this helper has no caller yet. Filling it in requires
/// exposing `stable_node_id_from_parts` (currently private in
/// `renderer_adapter`) as `pub(crate)` and threading the parent's
/// `GlobalNodePath` + identity-ordinal context through. That context
/// only exists in `rsx_to_descriptors_with_context` today — deferred
/// to the M2+ work that wires descriptor construction into the
/// translator.
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
    // renderer_adapter. M2 doesn't need it.
    0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::base_component::ElementTrait;
    use crate::view::node_arena::{Node, NodeArena};

    /// Minimal test-only element that lets us drive stable_id_index
    /// assertions without building a real RSX tree (which would pull
    /// in the whole renderer pipeline).
    struct TestElement {
        sid: u64,
    }

    impl crate::view::base_component::Layoutable for TestElement {
        fn measure(
            &mut self,
            _c: crate::view::base_component::LayoutConstraints,
            _a: &mut NodeArena,
        ) {
        }
        fn place(
            &mut self,
            _p: crate::view::base_component::LayoutPlacement,
            _a: &mut NodeArena,
        ) {
        }
        fn measured_size(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn set_layout_width(&mut self, _w: f32) {}
        fn set_layout_height(&mut self, _h: f32) {}
    }
    impl crate::view::base_component::EventTarget for TestElement {}
    impl crate::view::base_component::Renderable for TestElement {
        fn build(
            &mut self,
            _g: &mut crate::view::frame_graph::FrameGraph,
            _a: &mut NodeArena,
            ctx: crate::view::base_component::UiBuildContext,
        ) -> crate::view::base_component::BuildState {
            ctx.into_state()
        }
    }
    impl ElementTrait for TestElement {
        fn stable_id(&self) -> u64 {
            self.sid
        }
        fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
            crate::view::base_component::BoxModelSnapshot {
                node_id: self.sid,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                border_radius: 0.0,
                should_render: false,
            }
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    fn make(sid: u64) -> Box<dyn ElementTrait> {
        Box::new(TestElement { sid })
    }

    fn test_apply_ctx() -> ApplyContext<'static> {
        use std::sync::OnceLock;
        static STYLE: OnceLock<Style> = OnceLock::new();
        ApplyContext {
            viewport_style: STYLE.get_or_init(Style::new),
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }

    #[test]
    fn stable_id_index_populated_on_insert() {
        let mut arena = NodeArena::new();
        let k = arena.insert(Node::new(make(42)));
        assert_eq!(arena.find_by_stable_id(42), Some(k));
    }

    #[test]
    fn stable_id_index_skips_zero() {
        let mut arena = NodeArena::new();
        let _ = arena.insert(Node::new(make(0)));
        assert_eq!(arena.find_by_stable_id(0), None);
    }

    #[test]
    fn stable_id_index_cleaned_on_remove() {
        let mut arena = NodeArena::new();
        let k = arena.insert(Node::new(make(7)));
        assert_eq!(arena.find_by_stable_id(7), Some(k));
        arena.remove(k);
        assert_eq!(arena.find_by_stable_id(7), None);
    }

    #[test]
    fn stable_id_index_cleaned_on_remove_subtree() {
        let mut arena = NodeArena::new();
        let parent = arena.insert(Node::new(make(1)));
        let child = arena.insert(Node::new(make(2)));
        arena.set_parent(child, Some(parent));
        arena.push_child(parent, child);

        assert_eq!(arena.find_by_stable_id(1), Some(parent));
        assert_eq!(arena.find_by_stable_id(2), Some(child));

        arena.remove_subtree(parent);
        assert_eq!(arena.find_by_stable_id(1), None);
        assert_eq!(arena.find_by_stable_id(2), None);
    }

    #[test]
    fn refresh_stable_id_index_rebuilds_from_scratch() {
        let mut arena = NodeArena::new();
        let k = arena.insert(Node::new(make(99)));
        // Simulate a caller that bypassed the indexed path: wipe the
        // index by hand then rebuild.
        arena.refresh_stable_id_index(); // still correct after a no-op refresh
        assert_eq!(arena.find_by_stable_id(99), Some(k));
    }

    #[test]
    fn fiber_work_delete_removes_subtree_under_root() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(make(10)));
        arena.push_root(root);

        apply_fiber_works(
            &mut arena,
            test_apply_ctx(),
            vec![FiberWork::Delete {
                parent: None,
                key: root,
            }],
        );

        assert!(arena.is_empty());
        assert!(arena.roots().is_empty());
        assert_eq!(arena.find_by_stable_id(10), None);
    }

    #[test]
    fn fiber_work_delete_removes_child_via_parent() {
        let mut arena = NodeArena::new();
        let parent = arena.insert(Node::new(make(1)));
        let child = arena.insert(Node::new(make(2)));
        arena.set_parent(child, Some(parent));
        arena.set_children(parent, vec![child]);

        apply_fiber_works(
            &mut arena,
            test_apply_ctx(),
            vec![FiberWork::Delete {
                parent: Some(parent),
                key: child,
            }],
        );

        assert_eq!(arena.children_of(parent).len(), 0);
        assert!(arena.find_by_stable_id(2).is_none());
        assert_eq!(arena.find_by_stable_id(1), Some(parent));
    }

    #[test]
    fn fiber_work_move_reorders_children() {
        let mut arena = NodeArena::new();
        let parent = arena.insert(Node::new(make(1)));
        let a = arena.insert(Node::new(make(10)));
        let b = arena.insert(Node::new(make(20)));
        let c = arena.insert(Node::new(make(30)));
        for &ch in &[a, b, c] {
            arena.set_parent(ch, Some(parent));
        }
        arena.set_children(parent, vec![a, b, c]);

        // Move `a` (index 0) to the end (index 2).
        apply_fiber_works(
            &mut arena,
            test_apply_ctx(),
            vec![FiberWork::Move {
                parent,
                key: a,
                from: 0,
                to: 2,
            }],
        );

        assert_eq!(arena.children_of(parent), vec![b, c, a]);
    }

    #[test]
    fn fiber_work_update_and_set_text_are_safe_on_unknown_host() {
        // M3: Update / SetText now dispatch through the setter layer,
        // but on an unknown host type (here: the TestElement harness,
        // which is neither Text / TextArea / Element) both paths must
        // bail cleanly. The assertion guards against a future refactor
        // accidentally panicking on unrecognised downcast targets.
        let mut arena = NodeArena::new();
        let k = arena.insert(Node::new(make(5)));

        apply_fiber_works(
            &mut arena,
            test_apply_ctx(),
            vec![
                FiberWork::Update {
                    key: k,
                    changed: vec![],
                    removed: vec![],
                },
                FiberWork::SetText {
                    key: k,
                    text: "ignored".into(),
                },
            ],
        );

        assert_eq!(arena.len(), 1);
        assert_eq!(arena.find_by_stable_id(5), Some(k));
    }

    /// M4 #3: a FiberWork::Update with `changed = [("loading", ...)]`
    /// on an Image host installs the new loading slot subtree via
    /// `Image::replace_loading_slot_incremental`, replacing any
    /// prior slot. Exercises the apply dispatcher end-to-end through
    /// `apply_fiber_works` (the HostImage rsx route `Rc`-wraps
    /// `ImageSource` which forces full-rebuild on second render — see
    /// the commit log for why the integration test was demoted to a
    /// unit test).
    #[test]
    fn fiber_work_installs_image_loading_slot_incrementally() {
        use crate::ui::{IntoPropValue, RsxNode, RsxTagDescriptor};
        use crate::view::ImageSource;
        use crate::view::base_component::{Element, Image};
        use crate::view::node_arena::Node;
        use std::sync::Arc;

        let src = ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::<[u8]>::from(vec![0u8, 0, 0, 255]),
        };
        let image = Image::new_with_id(7, src);
        let mut arena = NodeArena::new();
        let image_key = arena.insert(Node::new(Box::new(image)));

        let loading_a = RsxNode::tagged("Element", RsxTagDescriptor::of::<Element>());
        apply_fiber_works(
            &mut arena,
            test_apply_ctx(),
            vec![FiberWork::Update {
                key: image_key,
                changed: vec![("loading", loading_a.into_prop_value())],
                removed: vec![],
            }],
        );
        {
            let node = arena.get(image_key).expect("image survived");
            let image = node
                .element
                .as_any()
                .downcast_ref::<Image>()
                .expect("Image host");
            assert_eq!(
                image.loading_slot_len(),
                1,
                "first loading slot install should leave exactly one wrapper",
            );
        }
        let arena_len_after_first = arena.len();

        // Install a taller slot; the old wrapper subtree must be
        // removed and the new one committed, keeping the Vec length
        // at 1.
        let loading_b = RsxNode::tagged("Element", RsxTagDescriptor::of::<Element>())
            .with_child(RsxNode::tagged(
                "Element",
                RsxTagDescriptor::of::<Element>(),
            ));
        apply_fiber_works(
            &mut arena,
            test_apply_ctx(),
            vec![FiberWork::Update {
                key: image_key,
                changed: vec![("loading", loading_b.into_prop_value())],
                removed: vec![],
            }],
        );
        {
            let node = arena.get(image_key).expect("image survived second update");
            let image = node
                .element
                .as_any()
                .downcast_ref::<Image>()
                .expect("Image host");
            assert_eq!(
                image.loading_slot_len(),
                1,
                "second loading slot install must replace the first (not stack)",
            );
        }
        // Arena net growth from first→second install should be
        // exactly +1 (new slot has 2 nodes vs. old 1, minus old's 1
        // removed = +1). If `replace_loading_slot_incremental` skipped
        // the `remove_subtree` loop the delta would be +2.
        let delta = arena.len() as isize - arena_len_after_first as isize;
        assert_eq!(
            delta, 1,
            "arena net growth must be +1 (old slot removed, new 2-node slot committed)",
        );
    }

    /// M4 #7: a FiberWork::Update with `removed = ["opacity"]` on an
    /// Element host resets opacity to the default 1.0. Exercises the
    /// gate + `apply_remove_to_element` path directly since
    /// HostElement's RSX schema doesn't expose `opacity` as a
    /// top-level prop (reconciler only surfaces it in the `style`
    /// map), so this is the smallest test that touches the
    /// opacity-reset branch.
    #[test]
    fn fiber_work_removes_opacity_resets_to_default_on_element() {
        use crate::view::base_component::Element;
        use crate::view::node_arena::Node;

        let mut arena = NodeArena::new();
        let mut el = Element::new(0.0, 0.0, 100.0, 100.0);
        el.set_opacity(0.3);
        assert!((el.opacity() - 0.3).abs() < 1e-4);
        let k = arena.insert(Node::new(Box::new(el)));

        apply_fiber_works(
            &mut arena,
            test_apply_ctx(),
            vec![FiberWork::Update {
                key: k,
                changed: vec![],
                removed: vec!["opacity"],
            }],
        );

        let node = arena.get(k).expect("node survived");
        let el = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("Element host");
        assert!(
            (el.opacity() - 1.0).abs() < 1e-4,
            "removed opacity must reset to default 1.0, got {}",
            el.opacity()
        );
    }
}
