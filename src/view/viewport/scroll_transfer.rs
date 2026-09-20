//! Preserve scroll only where an incremental batch can destroy host identity.
use super::*;
use crate::view::fiber_work::FiberWork;
use crate::view::node_arena::{NodeArena, NodeKey};

impl Viewport {
    pub(super) fn save_replaced_scroll_states(
        arena: &NodeArena,
        works: &[FiberWork],
    ) -> FxHashMap<u64, (f32, f32)> {
        let mut roots = Vec::new();
        for work in works {
            match work {
                FiberWork::Delete { key, .. } | FiberWork::ReplaceRootAt { key, .. } => {
                    roots.push(*key)
                }
                // Earlier moves/inserts/removals may shift this index. Capture
                // the parent subtree conservatively, never guess the old child.
                FiberWork::ReplaceNode { parent, .. } => roots.push(*parent),
                FiberWork::ReplaceAllRoots { .. } => roots.extend_from_slice(arena.roots()),
                FiberWork::Update {
                    key,
                    changed,
                    removed,
                } => {
                    if arena.get(*key).is_some_and(|node| {
                        changed
                            .iter()
                            .map(|(name, _)| *name)
                            .chain(removed.iter().copied())
                            .any(|name| !node.element.prop_preserves_child_identity(name))
                    }) {
                        roots.push(*key);
                    }
                }
                FiberWork::Create { .. }
                | FiberWork::CreateMany { .. }
                | FiberWork::SetText { .. }
                | FiberWork::Move { .. }
                | FiberWork::ReorderRoots { .. } => {}
            }
        }
        fn walk(
            arena: &NodeArena,
            key: NodeKey,
            seen: &mut FxHashSet<NodeKey>,
            out: &mut FxHashMap<u64, (f32, f32)>,
        ) {
            if !seen.insert(key) {
                return;
            }
            let Some(node) = arena.get(key) else {
                return;
            };
            crate::ui::work_profile::count(|p| p.scroll_save_nodes += 1);
            let offset = node.element.get_scroll_offset();
            if offset != (0.0, 0.0) {
                out.insert(node.element.stable_id(), offset);
            }
            for child in node.element.children() {
                walk(arena, *child, seen, out);
            }
        }
        let mut out = FxHashMap::default();
        let mut seen = FxHashSet::default();
        for key in roots {
            walk(arena, key, &mut seen, &mut out);
        }
        out
    }

    pub(super) fn restore_replaced_scroll_states(
        arena: &NodeArena,
        offsets: &FxHashMap<u64, (f32, f32)>,
    ) {
        // The index is maintained by arena create/remove; only saved nonzero
        // scroll owners need a lookup. Unrelated subtrees are never traversed.
        for (stable_id, offset) in offsets {
            let Some(key) = arena.find_by_stable_id(*stable_id) else {
                continue;
            };
            crate::ui::work_profile::count(|p| p.scroll_restore_nodes += 1);
            let _ = arena.mutate_element_ref_with_invalidation(key, |element, cx| {
                let before = element.get_scroll_offset();
                element.set_scroll_offset(*offset);
                if before != *offset {
                    cx.invalidate(crate::view::base_component::DirtyPassMask::RUNTIME);
                }
            });
        }
    }
}
