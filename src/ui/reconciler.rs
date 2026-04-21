#![allow(missing_docs)]

//! Tree reconciliation helpers used by the RSX runtime.
use rustc_hash::FxHashMap;

use crate::ui::{PropValue, RsxElementNode, RsxNode, RsxNodeIdentity};
use std::cell::RefCell;
use std::collections::{VecDeque};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Patch {
    ReplaceRoot(RsxNode),
    /// Wholesale replace of the entire root set (arity or identity change at
    /// root level). Apply side clears all arena roots and commits N new ones.
    /// Only emitted by `reconcile_multi`; always carried on a `RootedPatch`
    /// with `root_index == 0`.
    ReplaceAllRoots(Vec<RsxNode>),
    /// Reorder the arena root set without minting new keys.
    /// `new_arena_roots[i] == old_arena_roots[mapping[i]]`.
    /// Emitted by `reconcile_multi` when the new root set is a permutation of
    /// the old (keyed identity multiset matches but order differs). Apply
    /// side rearranges `arena.roots` only — NodeKeys stay alive so any
    /// promoted-layer / Persistent GPU resource cached against them
    /// survives. Always carried on a `RootedPatch` with `root_index == 0`;
    /// must be processed before any subsequent per-pair patches in the
    /// same batch (which are tagged with the *new* root_index).
    ReorderRoots(Vec<usize>),
    ReplaceNode {
        path: Vec<usize>,
        node: RsxNode,
    },
    UpdateElementProps {
        path: Vec<usize>,
        /// Props whose value was added or changed (key existed with different value, or is new).
        changed: Vec<(&'static str, PropValue)>,
        /// Keys that existed in the old props but are absent in the new props.
        removed: Vec<&'static str>,
    },
    SetText {
        path: Vec<usize>,
        text: String,
    },
    InsertChild {
        parent_path: Vec<usize>,
        index: usize,
        node: RsxNode,
    },
    RemoveChild {
        parent_path: Vec<usize>,
        index: usize,
    },
    MoveChild {
        parent_path: Vec<usize>,
        from: usize,
        to: usize,
    },
}

/// Patch tagged with the arena root index it applies to. Produced by
/// `reconcile_multi`; dispatcher passes `roots[root_index]` to the translator.
#[derive(Clone, Debug, PartialEq)]
pub struct RootedPatch {
    pub root_index: usize,
    pub patch: Patch,
}

// ---------------------------------------------------------------------------
// Thread-local scratch pool
// ---------------------------------------------------------------------------

/// Per-level working storage for `reconcile_children`.
///
/// Kept in a thread-local pool so that repeated reconcile calls reuse the
/// already-allocated FxHashMap tables and Vecs instead of allocating fresh ones
/// every frame.  The pool grows to the maximum tree depth encountered and then
/// stays constant.
struct ChildrenScratch {
    old_keyed: FxHashMap<RsxNodeIdentity, usize>,
    old_unkeyed: FxHashMap<&'static str, VecDeque<usize>>,
    matches: Vec<Option<usize>>,
    matched_old: Vec<bool>,
    current_order: Vec<usize>,
    /// Maps old-child-index → its current position in `current_order`.
    /// Updated incrementally as virtual moves/inserts are simulated, giving
    /// O(1) lookup instead of the previous O(n) linear scan.
    pos_lookup: FxHashMap<usize, usize>,
    /// Sequence of matched old-child indices in new order, used for LIS.
    target_seq: Vec<usize>,
}

impl ChildrenScratch {
    fn new() -> Self {
        Self {
            old_keyed: FxHashMap::default(),
            old_unkeyed: FxHashMap::default(),
            matches: Vec::new(),
            matched_old: Vec::new(),
            current_order: Vec::new(),
            pos_lookup: FxHashMap::default(),
            target_seq: Vec::new(),
        }
    }

    /// Shrink threshold — if a collection grew beyond this capacity (e.g. due
    /// to a single very large subtree), shrink it back so memory doesn't stay
    /// pinned.  Modelled after Servo/Stylo's arena-reset pattern.
    const SHRINK_HASHMAP_TO: usize = 64;
    const SHRINK_VEC_TO: usize = 128;

    fn clear(&mut self) {
        self.old_keyed.clear();
        self.old_unkeyed.clear();
        self.matches.clear();
        self.matched_old.clear();
        self.current_order.clear();
        self.pos_lookup.clear();
        self.target_seq.clear();

        // Reclaim bloated capacity so a one-time spike doesn't hold memory
        // forever (Servo/Stylo arena-reset inspired).
        if self.old_keyed.capacity() > Self::SHRINK_HASHMAP_TO {
            self.old_keyed.shrink_to(Self::SHRINK_HASHMAP_TO);
        }
        if self.old_unkeyed.capacity() > Self::SHRINK_HASHMAP_TO {
            self.old_unkeyed.shrink_to(Self::SHRINK_HASHMAP_TO);
        }
        if self.pos_lookup.capacity() > Self::SHRINK_HASHMAP_TO {
            self.pos_lookup.shrink_to(Self::SHRINK_HASHMAP_TO);
        }
        if self.matches.capacity() > Self::SHRINK_VEC_TO {
            self.matches.shrink_to(Self::SHRINK_VEC_TO);
        }
        if self.matched_old.capacity() > Self::SHRINK_VEC_TO {
            self.matched_old.shrink_to(Self::SHRINK_VEC_TO);
        }
        if self.current_order.capacity() > Self::SHRINK_VEC_TO {
            self.current_order.shrink_to(Self::SHRINK_VEC_TO);
        }
        if self.target_seq.capacity() > Self::SHRINK_VEC_TO {
            self.target_seq.shrink_to(Self::SHRINK_VEC_TO);
        }
    }
}

/// RAII guard that pops a `ChildrenScratch` from the pool on creation and
/// returns it (after clearing) on drop — even if the caller panics.
struct ScratchGuard {
    scratch: Option<ChildrenScratch>,
}

impl ScratchGuard {
    fn acquire() -> Self {
        let scratch = SCRATCH_POOL.with(|pool| {
            pool.borrow_mut()
                .pop()
                .unwrap_or_else(ChildrenScratch::new)
        });
        ScratchGuard {
            scratch: Some(scratch),
        }
    }

    fn get(&mut self) -> &mut ChildrenScratch {
        self.scratch.as_mut().unwrap()
    }
}

/// Max pooled scratch entries — enough for reasonable recursion depth.
/// Excess entries are dropped rather than pooled to bound memory.
const MAX_SCRATCH_POOL_SIZE: usize = 8;

impl Drop for ScratchGuard {
    fn drop(&mut self) {
        if let Some(mut s) = self.scratch.take() {
            s.clear();
            SCRATCH_POOL.with(|pool| {
                let mut pool = pool.borrow_mut();
                if pool.len() < MAX_SCRATCH_POOL_SIZE {
                    pool.push(s);
                }
                // else: drop `s` — pool is full
            });
        }
    }
}

thread_local! {
    static SCRATCH_POOL: RefCell<Vec<ChildrenScratch>> = RefCell::new(Vec::new());
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn reconcile(old: Option<&RsxNode>, new: &RsxNode) -> Vec<Patch> {
    let Some(old) = old else {
        return vec![Patch::ReplaceRoot(new.clone())];
    };

    let mut patches = Vec::new();
    // `path` is threaded through the whole traversal as a shared scratch Vec.
    // Each call level pushes a child index before recursing and pops it after,
    // so `path.to_vec()` is only called when a patch actually needs to be emitted.
    let mut path = Vec::new();
    reconcile_node(old, new, &mut path, &mut patches);
    patches
}

/// Multi-root reconcile entry point. Each slot in `old` / `new` corresponds to
/// one arena root. Caller is responsible for normalizing Fragment-at-root by
/// unpacking its children into this slice.
///
/// Semantics:
/// - `old = None` (cold render) → single `ReplaceAllRoots(new.clone())`.
/// - Arity mismatch or any per-index root identity mismatch →
///   `ReplaceAllRoots(new.clone())` (wholesale root-set swap).
/// - Otherwise per-root reconcile; emitted patches tagged with `root_index = i`.
pub fn reconcile_multi(old: Option<&[&RsxNode]>, new: &[&RsxNode]) -> Vec<RootedPatch> {
    let Some(old) = old else {
        return vec![RootedPatch {
            root_index: 0,
            patch: Patch::ReplaceAllRoots(new.iter().map(|n| (*n).clone()).collect()),
        }];
    };

    if old.len() != new.len() {
        return vec![RootedPatch {
            root_index: 0,
            patch: Patch::ReplaceAllRoots(new.iter().map(|n| (*n).clone()).collect()),
        }];
    }

    // Keyed pairing across the root set. Examples like a window manager
    // re-order Fragment-at-root children every frame; per-index identity
    // comparison would mark every index as a mismatch and trigger
    // ReplaceAllRoots, wiping NodeKeys (and any promoted-layer GPU
    // resources cached against them). Match by `RsxNodeIdentity` instead,
    // emit a `ReorderRoots` permutation when the multiset matches but
    // order differs, and fall back to `ReplaceAllRoots` only when an
    // identity is genuinely missing.
    //
    // Same identity appearing multiple times: pair them in occurrence
    // order (FIFO), matching the per-position semantics
    // `reconcile_children` uses for unkeyed siblings.
    let mut by_identity: FxHashMap<RsxNodeIdentity, std::collections::VecDeque<usize>> =
        FxHashMap::default();
    for (i, o) in old.iter().enumerate() {
        by_identity.entry(o.identity().clone()).or_default().push_back(i);
    }

    let mut mapping: Vec<usize> = Vec::with_capacity(new.len());
    for n in new.iter() {
        let Some(queue) = by_identity.get_mut(&n.identity()) else {
            return vec![RootedPatch {
                root_index: 0,
                patch: Patch::ReplaceAllRoots(new.iter().map(|n| (*n).clone()).collect()),
            }];
        };
        let Some(j) = queue.pop_front() else {
            return vec![RootedPatch {
                root_index: 0,
                patch: Patch::ReplaceAllRoots(new.iter().map(|n| (*n).clone()).collect()),
            }];
        };
        mapping.push(j);
    }

    let mut out = Vec::new();
    let is_permutation_identity = mapping.iter().enumerate().all(|(i, j)| i == *j);
    if !is_permutation_identity {
        out.push(RootedPatch {
            root_index: 0,
            patch: Patch::ReorderRoots(mapping.clone()),
        });
    }

    let mut scratch = Vec::new();
    let mut path = Vec::new();
    for (new_index, &old_index) in mapping.iter().enumerate() {
        let o = old[old_index];
        let n = new[new_index];
        scratch.clear();
        path.clear();
        reconcile_node(o, n, &mut path, &mut scratch);
        for p in scratch.drain(..) {
            // Patches reference roots by their *new* (post-reorder)
            // index, so the dispatcher can rely on `roots[root_index]`
            // after applying the leading `ReorderRoots` patch.
            out.push(RootedPatch {
                root_index: new_index,
                patch: p,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Private recursive helpers
// ---------------------------------------------------------------------------

fn reconcile_node(
    old: &RsxNode,
    new: &RsxNode,
    path: &mut Vec<usize>,
    patches: &mut Vec<Patch>,
) {
    // Fast path: if both variants hold the exact same `Rc` allocation, the
    // entire subtree is guaranteed structurally identical — no patches needed.
    // Enables memoized components (Step D) and any caller that reuses an
    // `Rc<RsxNode>` across renders to skip subtree reconciliation entirely.
    if RsxNode::ptr_eq(old, new) {
        return;
    }

    if old.identity() != new.identity() {
        if path.is_empty() {
            patches.push(Patch::ReplaceRoot(new.clone()));
        } else {
            patches.push(Patch::ReplaceNode {
                path: path.to_vec(),
                node: new.clone(),
            });
        }
        return;
    }

    match (old, new) {
        (RsxNode::Element(old_node), RsxNode::Element(new_node)) => {
            reconcile_element(old_node, new_node, path, patches);
        }
        (RsxNode::Text(old_text), RsxNode::Text(new_text)) => {
            if old_text.content != new_text.content {
                patches.push(Patch::SetText {
                    path: path.to_vec(),
                    text: new_text.content.clone(),
                });
            }
        }
        (RsxNode::Fragment(old_frag), RsxNode::Fragment(new_frag)) => {
            reconcile_children(&old_frag.children, &new_frag.children, path, patches);
        }
        _ => {
            if path.is_empty() {
                patches.push(Patch::ReplaceRoot(new.clone()));
            } else {
                patches.push(Patch::ReplaceNode {
                    path: path.to_vec(),
                    node: new.clone(),
                });
            }
        }
    }
}

fn reconcile_element(
    old: &Rc<RsxElementNode>,
    new: &Rc<RsxElementNode>,
    path: &mut Vec<usize>,
    patches: &mut Vec<Patch>,
) {
    let same_tag = match (old.tag_descriptor, new.tag_descriptor) {
        (Some(old_desc), Some(new_desc)) => old_desc == new_desc,
        _ => old.tag == new.tag,
    };

    if !same_tag {
        if path.is_empty() {
            patches.push(Patch::ReplaceRoot(RsxNode::Element(Rc::clone(new))));
        } else {
            patches.push(Patch::ReplaceNode {
                path: path.to_vec(),
                node: RsxNode::Element(Rc::clone(new)),
            });
        }
        return;
    }

    // Fast path: if both props point to the same `Rc` allocation, no prop diff
    // is needed — this is the common case when a component reuses its props
    // object across renders.
    if !Rc::ptr_eq(&old.props, &new.props) {
        // Diff props: find changed/added keys and removed keys.
        let mut changed = Vec::new();
        let mut removed = Vec::new();

        for &(old_key, ref old_val) in old.props.iter() {
            match new.props.iter().find(|&&(k, _)| k == old_key) {
                Some((_, new_val)) if new_val != old_val => {
                    changed.push((old_key, new_val.clone()))
                }
                None => removed.push(old_key),
                _ => {}
            }
        }
        for &(new_key, ref new_val) in new.props.iter() {
            if !old.props.iter().any(|&(k, _)| k == new_key) {
                changed.push((new_key, new_val.clone()));
            }
        }

        if !changed.is_empty() || !removed.is_empty() {
            patches.push(Patch::UpdateElementProps {
                path: path.to_vec(),
                changed,
                removed,
            });
        }
    }

    reconcile_children(&old.children, &new.children, path, patches);
}

/// Returns a boolean mask over `seq` where `mask[i] == true` means `seq[i]`
/// is part of the Longest Increasing Subsequence.
///
/// Uses patience-sort (binary-search) in O(n log n).  All values in `seq`
/// are assumed to be distinct (which is always the case for old-child indices).
fn lis_stable_mask(seq: &[usize]) -> Vec<bool> {
    let n = seq.len();
    if n == 0 {
        return vec![];
    }

    // tails[k]         = smallest tail value of any IS of length k+1 seen so far
    // index_of_tail[k] = index in `seq` of that tail value
    // predecessor[i]   = index in `seq` of the element just before seq[i] in its IS
    let mut tails: Vec<usize> = Vec::with_capacity(n);
    let mut index_of_tail: Vec<usize> = Vec::with_capacity(n);
    let mut predecessor: Vec<usize> = vec![usize::MAX; n];

    for (i, &val) in seq.iter().enumerate() {
        // First position where tails[pos] >= val  (strictly increasing IS).
        let pos = tails.partition_point(|&t| t < val);
        if pos == tails.len() {
            tails.push(val);
            index_of_tail.push(i);
        } else {
            tails[pos] = val;
            index_of_tail[pos] = i;
        }
        predecessor[i] = if pos > 0 {
            index_of_tail[pos - 1]
        } else {
            usize::MAX
        };
    }

    // Backtrack through predecessor links to mark the LIS elements.
    let lis_len = tails.len();
    let mut mask = vec![false; n];
    let mut cur = *index_of_tail.last().unwrap(); // safe: n > 0
    for _ in 0..lis_len {
        mask[cur] = true;
        cur = predecessor[cur];
    }
    mask
}

fn reconcile_children(
    old_children: &[RsxNode],
    new_children: &[RsxNode],
    parent_path: &mut Vec<usize>,
    patches: &mut Vec<Patch>,
) {
    // Acquire a reusable scratch buffer for this call level.
    // If this function recurses (via reconcile_node), each nested call gets its
    // own scratch from the pool, so there is no aliasing.
    let mut guard = ScratchGuard::acquire();
    let s = guard.get();

    // Index old children by identity.
    for (index, child) in old_children.iter().enumerate() {
        let identity = child.identity();
        if identity.key.is_some() {
            s.old_keyed.insert(*identity, index);
        } else {
            s.old_unkeyed
                .entry(identity.invocation_type)
                .or_default()
                .push_back(index);
        }
    }

    // Match each new child to an old child.
    s.matches.reserve(new_children.len());
    s.matched_old.resize(old_children.len(), false);
    for new_child in new_children {
        let identity = new_child.identity();
        let matched = if identity.key.is_some() {
            s.old_keyed.remove(identity)
        } else {
            s.old_unkeyed
                .get_mut(identity.invocation_type)
                .and_then(VecDeque::pop_front)
        };
        if let Some(old_index) = matched {
            s.matched_old[old_index] = true;
        }
        s.matches.push(matched);
    }

    // Recursively reconcile matched pairs.
    // `parent_path` is used as a shared mutable scratch: we push the child's
    // old index before recursing and pop it afterwards, so `path.to_vec()` is
    // only called inside reconcile_node when a patch actually needs to be emitted.
    for (new_index, maybe_old_index) in s.matches.iter().enumerate() {
        if let Some(old_index) = maybe_old_index {
            parent_path.push(*old_index);
            reconcile_node(
                &old_children[*old_index],
                &new_children[new_index],
                parent_path,
                patches,
            );
            parent_path.pop();
        }
    }

    // Emit removals in reverse order to keep indices stable.
    for old_index in (0..old_children.len()).rev() {
        if !s.matched_old[old_index] {
            patches.push(Patch::RemoveChild {
                parent_path: parent_path.to_vec(),
                index: old_index,
            });
        }
    }

    // Build the current logical order of surviving old children.
    s.current_order.extend(
        s.matched_old
            .iter()
            .enumerate()
            .filter_map(|(i, &matched)| matched.then_some(i)),
    );

    // Collect the matched old-child indices in new order and find which
    // positions are already in the Longest Increasing Subsequence.
    // LIS-stable nodes are already in the correct relative order and need not
    // be moved; only the remaining n − |LIS| nodes require a MoveChild patch.
    // This reduces worst-case patch count while the O(n log n) LIS algorithm
    // keeps the total work sub-quadratic.
    s.target_seq
        .extend(s.matches.iter().filter_map(|m| *m));
    let stable_mask = lis_stable_mask(&s.target_seq);

    // Build an O(1) position lookup so we can find each node's current index
    // without an O(n) linear scan.
    for (pos, &old_idx) in s.current_order.iter().enumerate() {
        s.pos_lookup.insert(old_idx, pos);
    }

    // Emit MoveChild / InsertChild patches.
    // `matched_seq_pos` tracks our position within `target_seq`/`stable_mask`
    // (which only advances for matched, not inserted, children).
    let mut matched_seq_pos = 0usize;

    for (new_index, maybe_old_index) in s.matches.iter().enumerate() {
        match maybe_old_index {
            Some(old_index) => {
                let is_stable = stable_mask[matched_seq_pos];
                matched_seq_pos += 1;

                if !is_stable {
                    let current_pos = s.pos_lookup[old_index];
                    if current_pos != new_index {
                        patches.push(Patch::MoveChild {
                            parent_path: parent_path.to_vec(),
                            from: current_pos,
                            to: new_index,
                        });
                        // Simulate the move and keep pos_lookup coherent for
                        // all elements whose position shifts in the process.
                        s.current_order.remove(current_pos);
                        s.current_order.insert(new_index, *old_index);
                        let lo = current_pos.min(new_index);
                        let hi = current_pos.max(new_index);
                        for pos in lo..=hi {
                            s.pos_lookup.insert(s.current_order[pos], pos);
                        }
                    }
                }
            }
            None => {
                patches.push(Patch::InsertChild {
                    parent_path: parent_path.to_vec(),
                    index: new_index,
                    node: new_children[new_index].clone(),
                });
                // Insert a sentinel so positions of existing nodes stay correct.
                s.current_order.insert(new_index, usize::MAX);
                for pos in new_index..s.current_order.len() {
                    let idx = s.current_order[pos];
                    if idx != usize::MAX {
                        s.pos_lookup.insert(idx, pos);
                    }
                }
            }
        }
    }

    // `guard` drops here, clearing the scratch and returning it to the pool.
}

#[cfg(test)]
mod bailout_tests {
    use super::*;
    use crate::ui::{RsxFragmentNode, RsxNodeIdentity};

    fn element_with_children(children: Vec<RsxNode>) -> RsxNode {
        RsxNode::Fragment(Rc::new(RsxFragmentNode {
            identity: RsxNodeIdentity::new("Fragment", None),
            children,
        }))
    }

    #[test]
    fn ptr_eq_node_is_skipped_without_patches() {
        let shared = RsxNode::text("hello");
        let old = element_with_children(vec![shared.clone(), RsxNode::text("b")]);
        // Reuse the exact same Rc for the first child; change the second.
        let new = element_with_children(vec![shared, RsxNode::text("B")]);

        let patches = reconcile(Some(&old), &new);
        // Only the second child should produce a patch; the first is bailed
        // out by the `Rc::ptr_eq` fast path in `reconcile_node`.
        assert_eq!(patches.len(), 1);
        assert!(matches!(patches[0], Patch::SetText { .. }));
    }

    #[test]
    fn ptr_eq_whole_tree_yields_no_patches() {
        let tree = element_with_children(vec![RsxNode::text("a"), RsxNode::text("b")]);
        let patches = reconcile(Some(&tree), &tree.clone());
        assert!(patches.is_empty(), "got patches: {patches:?}");
    }

    #[test]
    fn shared_props_rc_skips_prop_diff() {
        use crate::ui::{PropValue, RsxElementNode, RsxElementProps};

        // Build shared props: an `Rc<Vec<_>>` reused across two distinct
        // element allocations. The reconciler must take the `Rc::ptr_eq`
        // fast path and emit NO `UpdateElementProps` patch.
        let shared_props: RsxElementProps =
            Rc::new(vec![("width", PropValue::I64(100)), ("color", PropValue::I64(1))]);

        let make = |children: Vec<RsxNode>| {
            RsxNode::Element(Rc::new(RsxElementNode {
                identity: RsxNodeIdentity::new("Element", None),
                tag: "Element",
                tag_descriptor: None,
                props: shared_props.clone(),
                children,
            }))
        };

        let old = make(vec![RsxNode::text("a")]);
        let new = make(vec![RsxNode::text("b")]);
        let patches = reconcile(Some(&old), &new);
        // Only SetText for the changed child; no UpdateElementProps.
        assert_eq!(patches.len(), 1, "got patches: {patches:?}");
        assert!(matches!(patches[0], Patch::SetText { .. }));
    }
}
