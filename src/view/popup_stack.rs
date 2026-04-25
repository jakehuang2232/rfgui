//! Interaction-ordered stack for viewport-clip absolute nodes.
//!
//! Acts as the single source of truth for two parallel concerns:
//!
//! - **Deferred render order** — `append_defer_render_nodes_to` consults
//!   the stack so the latest-interacted popup is painted last (visually
//!   on top).
//! - **Pointer hit-test priority** — `hit_test_stacked` walks the stack
//!   top-to-bottom so the same popup absorbs clicks first.
//!
//! Entries are `stable_id` (u64) so the stack survives slotmap reallocation
//! across remount cycles.

use crate::view::node_arena::NodeArena;

/// Bottom-to-top ordering of viewport-clip absolute nodes.
///
/// Last entry = top of stack = painted last = hit-tested first.
#[derive(Debug, Default, Clone)]
pub struct PopupStack {
    ids: Vec<u64>,
}

impl PopupStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn contains(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }

    /// Append at the top. No-op if already present.
    pub fn register(&mut self, id: u64) {
        if id == 0 {
            return;
        }
        if !self.ids.contains(&id) {
            self.ids.push(id);
        }
    }

    /// Move id to the top. Inserts at the top if missing.
    pub fn promote(&mut self, id: u64) {
        if id == 0 {
            return;
        }
        if let Some(pos) = self.ids.iter().position(|x| *x == id) {
            if pos + 1 == self.ids.len() {
                return;
            }
            self.ids.remove(pos);
        }
        self.ids.push(id);
    }

    pub fn remove(&mut self, id: u64) {
        self.ids.retain(|x| *x != id);
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    /// Drop ids whose stable_id no longer resolves in arena.
    pub fn compact(&mut self, arena: &NodeArena) {
        self.ids
            .retain(|id| arena.find_by_stable_id(*id).is_some());
    }

    /// Bottom -> top (paint order).
    pub fn iter_bottom_up(&self) -> impl Iterator<Item = u64> + '_ {
        self.ids.iter().copied()
    }

    /// Top -> bottom (hit-test priority).
    pub fn iter_top_down(&self) -> impl Iterator<Item = u64> + '_ {
        self.ids.iter().rev().copied()
    }

    pub fn as_slice(&self) -> &[u64] {
        &self.ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_appends_top() {
        let mut s = PopupStack::new();
        s.register(1);
        s.register(2);
        s.register(3);
        assert_eq!(s.as_slice(), &[1, 2, 3]);
    }

    #[test]
    fn register_dedup() {
        let mut s = PopupStack::new();
        s.register(1);
        s.register(2);
        s.register(1);
        assert_eq!(s.as_slice(), &[1, 2]);
    }

    #[test]
    fn promote_moves_to_top() {
        let mut s = PopupStack::new();
        s.register(1);
        s.register(2);
        s.register(3);
        s.promote(1);
        assert_eq!(s.as_slice(), &[2, 3, 1]);
    }

    #[test]
    fn promote_already_top_is_noop() {
        let mut s = PopupStack::new();
        s.register(1);
        s.register(2);
        s.promote(2);
        assert_eq!(s.as_slice(), &[1, 2]);
    }

    #[test]
    fn promote_missing_inserts_at_top() {
        let mut s = PopupStack::new();
        s.register(1);
        s.promote(2);
        assert_eq!(s.as_slice(), &[1, 2]);
    }

    #[test]
    fn iter_top_down_is_reverse() {
        let mut s = PopupStack::new();
        s.register(1);
        s.register(2);
        s.register(3);
        let collected: Vec<u64> = s.iter_top_down().collect();
        assert_eq!(collected, vec![3, 2, 1]);
    }

    #[test]
    fn zero_id_ignored() {
        let mut s = PopupStack::new();
        s.register(0);
        s.promote(0);
        assert!(s.is_empty());
    }
}
