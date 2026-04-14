#![allow(missing_docs)]

//! Tree reconciliation helpers used by the RSX runtime.

use crate::ui::{PropValue, RsxElementNode, RsxNode, RsxNodeIdentity};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq)]
pub enum Patch {
    ReplaceRoot(RsxNode),
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

// ---------------------------------------------------------------------------
// Thread-local scratch pool
// ---------------------------------------------------------------------------

/// Per-level working storage for `reconcile_children`.
///
/// Kept in a thread-local pool so that repeated reconcile calls reuse the
/// already-allocated HashMap tables and Vecs instead of allocating fresh ones
/// every frame.  The pool grows to the maximum tree depth encountered and then
/// stays constant.
struct ChildrenScratch {
    old_keyed: HashMap<RsxNodeIdentity, usize>,
    old_unkeyed: HashMap<&'static str, VecDeque<usize>>,
    matches: Vec<Option<usize>>,
    matched_old: Vec<bool>,
    current_order: Vec<usize>,
    /// Maps old-child-index → its current position in `current_order`.
    /// Updated incrementally as virtual moves/inserts are simulated, giving
    /// O(1) lookup instead of the previous O(n) linear scan.
    pos_lookup: HashMap<usize, usize>,
    /// Sequence of matched old-child indices in new order, used for LIS.
    target_seq: Vec<usize>,
}

impl ChildrenScratch {
    fn new() -> Self {
        Self {
            old_keyed: HashMap::new(),
            old_unkeyed: HashMap::new(),
            matches: Vec::new(),
            matched_old: Vec::new(),
            current_order: Vec::new(),
            pos_lookup: HashMap::new(),
            target_seq: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.old_keyed.clear();
        self.old_unkeyed.clear(); // drops VecDeque values; HashMap capacity is retained
        self.matches.clear();
        self.matched_old.clear();
        self.current_order.clear();
        self.pos_lookup.clear();
        self.target_seq.clear();
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

impl Drop for ScratchGuard {
    fn drop(&mut self) {
        if let Some(mut s) = self.scratch.take() {
            s.clear();
            SCRATCH_POOL.with(|pool| pool.borrow_mut().push(s));
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

// ---------------------------------------------------------------------------
// Private recursive helpers
// ---------------------------------------------------------------------------

fn reconcile_node(
    old: &RsxNode,
    new: &RsxNode,
    path: &mut Vec<usize>,
    patches: &mut Vec<Patch>,
) {
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
    old: &RsxElementNode,
    new: &RsxElementNode,
    path: &mut Vec<usize>,
    patches: &mut Vec<Patch>,
) {
    let same_tag = match (old.tag_descriptor, new.tag_descriptor) {
        (Some(old_desc), Some(new_desc)) => old_desc == new_desc,
        _ => old.tag == new.tag,
    };

    if !same_tag {
        if path.is_empty() {
            patches.push(Patch::ReplaceRoot(RsxNode::Element(new.clone())));
        } else {
            patches.push(Patch::ReplaceNode {
                path: path.to_vec(),
                node: RsxNode::Element(new.clone()),
            });
        }
        return;
    }

    // Diff props: find changed/added keys and removed keys.
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    for &(old_key, ref old_val) in &old.props {
        match new.props.iter().find(|&&(k, _)| k == old_key) {
            Some((_, new_val)) if new_val != old_val => changed.push((old_key, new_val.clone())),
            None => removed.push(old_key),
            _ => {}
        }
    }
    for &(new_key, ref new_val) in &new.props {
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
