//! Scene tree helpers — hover, scroll, snapshots, transform updates, box models.

use super::dispatch::local_point_for_node;
use super::*;
use crate::ui::{PointerEnterEvent, PointerLeaveEvent};
use crate::view::base_component::{
    BoxModelSnapshot, DirtyFlags, ElementTrait, PromotionCompositeBounds, UiBuildContext,
    round_layout_value,
};

impl Viewport {
    pub(super) fn cancel_pointer_interactions(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
    ) -> bool {
        let mut changed = false;
        for &root_key in root_keys {
            changed |=
                crate::view::viewport::scene_helpers::cancel_pointer_interactions(arena, root_key);
        }
        changed
    }

    pub(super) fn apply_hover_target(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        target: Option<crate::view::node_arena::NodeKey>,
    ) -> bool {
        let mut changed = false;
        for &root_key in root_keys {
            if crate::view::viewport::scene_helpers::update_hover_state(arena, root_key, target) {
                changed = true;
            }
        }
        changed
    }

    pub(super) fn sync_hover_target(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        hovered_node_id: &mut Option<crate::view::node_arena::NodeKey>,
        next_target: Option<crate::view::node_arena::NodeKey>,
        pointer: crate::ui::PointerEventData,
    ) -> (bool, bool) {
        let transition_dispatched = crate::view::viewport::scene_helpers::dispatch_hover_transition(
            arena,
            root_keys,
            *hovered_node_id,
            next_target,
            pointer,
        );
        *hovered_node_id = next_target;
        let hover_changed = Self::apply_hover_target(arena, root_keys, next_target);
        (hover_changed, transition_dispatched)
    }

    pub(super) fn sync_hover_visual_only(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        hovered_node_id: &mut Option<crate::view::node_arena::NodeKey>,
        next_target: Option<crate::view::node_arena::NodeKey>,
    ) -> bool {
        *hovered_node_id = next_target;
        Self::apply_hover_target(arena, root_keys, next_target)
    }

    pub(super) fn save_scroll_states(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        map: &mut FxHashMap<u64, (f32, f32)>,
    ) {
        fn walk(
            node: &dyn crate::view::base_component::ElementTrait,
            arena: &crate::view::node_arena::NodeArena,
            map: &mut FxHashMap<u64, (f32, f32)>,
        ) {
            let offset = node.get_scroll_offset();
            if offset != (0.0, 0.0) {
                map.insert(node.stable_id(), offset);
            }
            for child_key in node.children() {
                if let Some(child_node) = arena.get(*child_key) {
                    walk(child_node.element.as_ref(), arena, map);
                }
            }
        }
        for &root_key in root_keys {
            if let Some(root_node) = arena.get(root_key) {
                walk(root_node.element.as_ref(), arena, map);
            }
        }
    }

    pub(super) fn restore_scroll_states(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        map: &FxHashMap<u64, (f32, f32)>,
    ) {
        fn walk(
            arena: &crate::view::node_arena::NodeArena,
            key: crate::view::node_arena::NodeKey,
            map: &FxHashMap<u64, (f32, f32)>,
        ) {
            let Some(node) = arena.get(key) else {
                return;
            };
            let stable_id = node.element.stable_id();
            let child_keys = node.children.clone();
            drop(node);

            if let Some(offset) = map.get(&stable_id) {
                let _ = arena.mutate_element_ref_with_invalidation(key, |element, cx| {
                    let before = element.get_scroll_offset();
                    element.set_scroll_offset(*offset);
                    if before != *offset {
                        cx.invalidate(crate::view::base_component::DirtyPassMask::RUNTIME);
                    }
                });
            }
            for child_key in child_keys {
                walk(arena, child_key, map);
            }
        }
        for &root_key in root_keys {
            walk(arena, root_key, map);
        }
    }

    // Phase B (軌 1 #1-#6 unlocked): scene-side `save_element_snapshots`
    // / `restore_element_snapshots` removed. Incremental commit no
    // longer rebuilds Element instances on every render, so the
    // host-state save/restore hack has nothing to compensate for on
    // the happy path. The remaining full-rebuild fallbacks
    // (Fragment-at-root, multi-descriptor Replace, Text cascade
    // boundary, em/rem font_size) accept the documented state loss.

    pub(super) fn extract_style_prop(
        props: &[(&'static str, PropValue)],
    ) -> Result<Option<Style>, String> {
        let Some((_, value)) = props.iter().find(|(key, _)| *key == "style") else {
            return Ok(None);
        };
        Self::extract_style_from_value(value)
    }

    pub(super) fn extract_style_from_value(value: &PropValue) -> Result<Option<Style>, String> {
        Self::extract_style_from_value_owned(value.clone())
    }

    pub(super) fn extract_style_from_value_owned(
        value: PropValue,
    ) -> Result<Option<Style>, String> {
        let schema = ElementStylePropSchema::from_prop_value(value)
            .map_err(|_| "prop `style` expects ElementStylePropSchema value".to_string())?;
        Ok(Some(schema.to_style()))
    }

    /// Placement-safe property IDs: changes to these never require a tree rebuild
    /// or a measure pass — they only affect placement (place pass) or paint.
    const PLACEMENT_SAFE_PROPERTIES: [PropertyId; 3] = [
        PropertyId::Transform,
        PropertyId::TransformOrigin,
        PropertyId::Position,
    ];

    /// Returns `Ok(true)` when the only difference between old and new props is
    /// a change to placement-safe properties (transform, transform-origin,
    /// position) inside the `style` prop.
    pub(super) fn is_placement_only_update(
        old_props: &[(&'static str, PropValue)],
        changed: &[(&'static str, PropValue)],
        removed: &[&'static str],
    ) -> Result<bool, String> {
        if !removed.is_empty() || changed.len() != 1 || changed[0].0 != "style" {
            return Ok(false);
        }
        // Text/TextArea hosts ship `TextStylePropSchema`-typed style
        // values that don't round-trip through `ElementStylePropSchema`.
        // Placement-only optimization only applies to Element hosts;
        // downgrade schema mismatch to "not placement-only" instead of
        // propagating the error to `render_rsx`.
        let Ok(old_style) = Self::extract_style_prop(old_props) else {
            return Ok(false);
        };
        let old_style = old_style.unwrap_or_default();
        let Ok(new_style) = Self::extract_style_from_value(&changed[0].1) else {
            return Ok(false);
        };
        let new_style = new_style.unwrap_or_default();
        if old_style
            .clone()
            .without_properties_recursive(&Self::PLACEMENT_SAFE_PROPERTIES)
            != new_style
                .clone()
                .without_properties_recursive(&Self::PLACEMENT_SAFE_PROPERTIES)
        {
            return Ok(false);
        }
        // At least one placement-safe property must have actually changed.
        let any_changed = Self::PLACEMENT_SAFE_PROPERTIES
            .iter()
            .any(|id| old_style.get(*id) != new_style.get(*id));
        if !any_changed {
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn apply_placement_style_by_node_key(
        arena: &crate::view::node_arena::NodeArena,
        target_key: crate::view::node_arena::NodeKey,
        style: &Style,
    ) -> bool {
        arena
            .mutate_element_ref_with_invalidation(target_key, |element, cx| {
                let Some(element) = element
                    .as_any_mut()
                    .downcast_mut::<crate::view::base_component::Element>()
                else {
                    return false;
                };
                let mut patch_style = Style::new();
                if let Some(crate::style::ParsedValue::Transform(transform)) =
                    style.get(PropertyId::Transform)
                {
                    patch_style.set_transform(transform.clone());
                }
                if let Some(crate::style::ParsedValue::TransformOrigin(origin)) =
                    style.get(PropertyId::TransformOrigin)
                {
                    patch_style.set_transform_origin(*origin);
                }
                if let Some(crate::style::ParsedValue::Position(position)) =
                    style.get(PropertyId::Position)
                {
                    patch_style.insert(
                        PropertyId::Position,
                        crate::style::ParsedValue::Position(position.clone()),
                    );
                }
                element.apply_style(patch_style);
                cx.invalidate(crate::view::base_component::DirtyPassMask::RUNTIME);
                true
            })
            .unwrap_or(false)
    }

    fn root_set(root: &RsxNode) -> Vec<&RsxNode> {
        match root {
            RsxNode::Fragment(fragment) => fragment.children.iter().collect(),
            other => vec![other],
        }
    }

    pub(super) fn arena_key_for_rsx_path(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        root: &RsxNode,
        path: &[usize],
    ) -> Option<crate::view::node_arena::NodeKey> {
        let roots = Self::root_set(root);
        if roots.len() != root_keys.len() {
            return None;
        }

        let (root_index, per_root, per_root_path) = match root {
            RsxNode::Fragment(_) => {
                let (&root_index, rest) = path.split_first()?;
                (root_index, *roots.get(root_index)?, rest)
            }
            _ => (0, roots[0], path),
        };

        let arena_path = match crate::view::fiber_work::rsx_to_arena_path(per_root, per_root_path) {
            crate::view::fiber_work::ArenaPathResolution::Arena(path) => path,
            crate::view::fiber_work::ArenaPathResolution::Invalid => return None,
        };
        crate::view::renderer_adapter::resolve_path(arena, root_keys[root_index], &arena_path)
    }

    pub(super) fn try_apply_placement_updates(&mut self, root: &RsxNode) -> Result<bool, String> {
        let Some(previous_root) = self.scene.last_rsx_root.as_ref() else {
            return Ok(false);
        };
        let patches = reconcile(Some(previous_root), root);
        if patches.is_empty() {
            self.scene.last_rsx_root = Some(root.clone());
            return Ok(true);
        }

        let mut updates = Vec::new();
        for patch in &patches {
            let Patch::UpdateElementProps {
                path,
                changed,
                removed,
            } = patch
            else {
                return Ok(false);
            };
            let old_node = Self::rsx_node_by_index_path(previous_root, path)
                .ok_or_else(|| "invalid old RSX node path".to_string())?;
            let RsxNode::Element(old_element) = old_node else {
                return Ok(false);
            };
            if !Self::is_placement_only_update(&old_element.props, changed, removed)? {
                return Ok(false);
            }
            // Same schema-mismatch guard as in `is_placement_only_update`.
            let Ok(style) = Self::extract_style_from_value(&changed[0].1) else {
                return Ok(false);
            };
            let style = style.unwrap_or_default();
            let Some(target_key) = Self::arena_key_for_rsx_path(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                root,
                path,
            ) else {
                return Ok(false);
            };
            updates.push((target_key, style));
        }

        for (target_key, style) in &updates {
            if !Self::apply_placement_style_by_node_key(&self.scene.node_arena, *target_key, style)
            {
                return Ok(false);
            }
        }
        self.scene.last_rsx_root = Some(root.clone());
        Ok(true)
    }

    pub(super) fn rsx_node_by_index_path<'a>(
        node: &'a RsxNode,
        path: &[usize],
    ) -> Option<&'a RsxNode> {
        if path.is_empty() {
            return Some(node);
        }
        let children = node.children()?;
        let child = children.get(path[0])?;
        Self::rsx_node_by_index_path(child, &path[1..])
    }

    pub(super) fn refresh_frame_box_models(&mut self) {
        self.compositor.frame_box_models.clear();
        #[cfg(test)]
        {
            self.compositor.box_model_refresh_stats = BoxModelRefreshStats::default();
        }
        let root_keys = self.scene.ui_root_keys.clone();
        let active_roots: FxHashSet<_> = root_keys.iter().copied().collect();
        self.compositor
            .frame_box_model_cache
            .retain(|root_key, _| active_roots.contains(root_key));

        for &root_key in &root_keys {
            let flags = crate::view::base_component::DirtyPassMask::BOX_MODEL
                .union(crate::view::base_component::DirtyPassMask::HIT_TEST);
            let can_reuse = self
                .compositor
                .frame_box_model_cache
                .contains_key(&root_key)
                && !self
                    .scene
                    .node_arena
                    .subtree_dirty_intersects(root_key, flags);

            if can_reuse {
                let snapshots = self
                    .compositor
                    .frame_box_model_cache
                    .get(&root_key)
                    .expect("cache entry checked")
                    .clone();
                #[cfg(test)]
                {
                    self.compositor.box_model_refresh_stats.reused_roots += 1;
                    self.compositor.box_model_refresh_stats.reused_snapshots += snapshots.len();
                }
                self.compositor.frame_box_models.extend(snapshots);
                continue;
            }

            let snapshots = crate::view::viewport::scene_helpers::collect_box_models(
                root_key,
                &self.scene.node_arena,
            );
            #[cfg(test)]
            {
                self.compositor.box_model_refresh_stats.collected_roots += 1;
                self.compositor.box_model_refresh_stats.collected_snapshots += snapshots.len();
            }
            self.compositor
                .frame_box_model_cache
                .insert(root_key, snapshots.clone());
            self.compositor.frame_box_models.extend(snapshots);
            crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
                &mut self.scene.node_arena,
                root_key,
                flags,
            );
        }
    }
}

pub(crate) fn collect_box_models(
    root_key: crate::view::node_arena::NodeKey,
    arena: &crate::view::node_arena::NodeArena,
) -> Vec<BoxModelSnapshot> {
    fn walk(
        node: &dyn ElementTrait,
        arena: &crate::view::node_arena::NodeArena,
        out: &mut Vec<BoxModelSnapshot>,
    ) {
        out.push(node.box_model_snapshot());
        for child_key in node.children() {
            if let Some(child_node) = arena.get(*child_key) {
                walk(child_node.element.as_ref(), arena, out);
            }
        }
    }

    let mut out = Vec::new();
    if let Some(root_node) = arena.get(root_key) {
        walk(root_node.element.as_ref(), arena, &mut out);
    }
    out
}

/// Recursive walker kept as a reference / correctness oracle. The hot
/// layout paths now read [`NodeArena::cached_subtree_dirty`] instead,
/// which is refreshed once per pass by
/// [`NodeArena::refresh_subtree_dirty_cache`]. Kept `pub(crate)` + allow
/// dead for any future slow-path callers and for parity with existing
/// tests.
#[allow(dead_code)]
pub(crate) fn subtree_dirty_flags(
    root: &dyn ElementTrait,
    arena: &crate::view::node_arena::NodeArena,
) -> DirtyFlags {
    let mut flags = root.local_dirty_flags();
    for child_key in root.children() {
        if let Some(child_node) = arena.get(*child_key) {
            flags = flags.union(subtree_dirty_flags(child_node.element.as_ref(), arena));
        }
    }
    flags
}

fn clear_subtree_dirty_flags_by_key(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    flags: DirtyFlags,
) -> bool {
    let children = arena.children_of(root_key);
    let Some(mut root_node) = arena.get_mut(root_key) else {
        return false;
    };
    root_node.element.clear_local_dirty_flags(flags);
    drop(root_node);

    for child_key in children {
        clear_subtree_dirty_flags_by_key(arena, child_key, flags);
    }
    true
}

#[allow(dead_code)]
pub(crate) fn clear_subtree_dirty_flags_with_arena_dirty(
    arena: &mut crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    flags: DirtyFlags,
) -> bool {
    if !clear_subtree_dirty_flags_by_key(arena, root_key, flags) {
        return false;
    }

    arena.clear_arena_dirty_subtree(root_key, flags);
    true
}

pub(crate) fn can_reuse_promoted_subtree(
    node: &dyn ElementTrait,
    _ctx: &UiBuildContext,
    arena: &crate::view::node_arena::NodeArena,
) -> bool {
    fn walk(node: &dyn ElementTrait, arena: &crate::view::node_arena::NodeArena) -> bool {
        for child_key in node.children() {
            let Some(child_node) = arena.get(*child_key) else {
                continue;
            };
            if !walk(child_node.element.as_ref(), arena) {
                return false;
            }
        }
        true
    }

    walk(node, arena)
}

pub(crate) fn paint_snapped_promotion_composite_bounds(
    node: &dyn ElementTrait,
    bounds: PromotionCompositeBounds,
    paint_offset: [f32; 2],
) -> PromotionCompositeBounds {
    let snap = node.box_model_snapshot();
    let dx = round_layout_value(snap.x + paint_offset[0]) - snap.x;
    let dy = round_layout_value(snap.y + paint_offset[1]) - snap.y;
    PromotionCompositeBounds {
        x: bounds.x + dx,
        y: bounds.y + dy,
        ..bounds
    }
}

pub(crate) fn update_hover_state(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
    target_key: Option<crate::view::node_arena::NodeKey>,
) -> bool {
    fn walk(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
        target_key: Option<crate::view::node_arena::NodeKey>,
    ) -> (bool, bool) {
        arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let mut contains_target = target_key == Some(key);
                let mut changed = false;
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    let (child_contains_target, child_changed) =
                        walk(cx.arena(), child_key, target_key);
                    contains_target |= child_contains_target;
                    changed |= child_changed;
                }
                changed |= element.set_hovered(contains_target);
                if changed {
                    cx.invalidate(element.local_dirty_flags());
                }
                (contains_target, changed)
            })
            .unwrap_or((false, false))
    }

    walk(arena, root_key, target_key).1
}

/// Build a root-to-target path using `arena.parent_of`. Returns empty when
/// `target_key` is not reachable from any provided root.
pub(crate) fn hover_path_for_target(
    arena: &crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    target_key: Option<crate::view::node_arena::NodeKey>,
) -> Vec<crate::view::node_arena::NodeKey> {
    let Some(target_key) = target_key else {
        return Vec::new();
    };
    if !arena.contains_key(target_key) {
        return Vec::new();
    }

    // Walk up from target, collecting keys.
    let mut up = Vec::new();
    let mut cur = Some(target_key);
    while let Some(k) = cur {
        up.push(k);
        cur = arena.parent_of(k);
    }
    // Verify the uppermost ancestor is one of the roots.
    let root_reached = up.last().copied();
    if let Some(last) = root_reached {
        if root_keys.iter().any(|&r| r == last) {
            up.reverse();
            return up;
        }
    }
    Vec::new()
}

fn dispatch_pointer_enter_to_key(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    related: Option<crate::view::node_arena::NodeKey>,
    pointer: crate::ui::PointerEventData,
) -> bool {
    arena
        .mutate_element_ref_with_invalidation(key, |element, cx| {
            let snapshot = element.box_model_snapshot();
            let (local_x, local_y) = local_point_for_node(
                element.as_ref(),
                &snapshot,
                pointer.viewport_x,
                pointer.viewport_y,
            );
            let mut pointer = pointer;
            pointer.local_x = local_x;
            pointer.local_y = local_y;
            let target = crate::ui::EventTarget::snapshot(
                key,
                crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
            );
            let mut meta = crate::ui::EventMeta::with_target(target);
            meta.set_related_target(related.map(crate::ui::EventTarget::bare));
            meta.set_bubbles(false);
            meta.set_source(crate::ui::EventSource::Synthetic);
            let mut event = PointerEnterEvent { meta, pointer };
            element.dispatch_pointer_enter(&mut event, cx.arena(), key);
            cx.invalidate(element.local_dirty_flags());
            true
        })
        .unwrap_or(false)
}

fn dispatch_pointer_leave_to_key(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    related: Option<crate::view::node_arena::NodeKey>,
    pointer: crate::ui::PointerEventData,
) -> bool {
    arena
        .mutate_element_ref_with_invalidation(key, |element, cx| {
            let snapshot = element.box_model_snapshot();
            let (local_x, local_y) = local_point_for_node(
                element.as_ref(),
                &snapshot,
                pointer.viewport_x,
                pointer.viewport_y,
            );
            let mut pointer = pointer;
            pointer.local_x = local_x;
            pointer.local_y = local_y;
            let target = crate::ui::EventTarget::snapshot(
                key,
                crate::ui::Rect::new(snapshot.x, snapshot.y, snapshot.width, snapshot.height),
                crate::ui::Rect::new(0.0, 0.0, snapshot.width, snapshot.height),
            );
            let mut meta = crate::ui::EventMeta::with_target(target);
            meta.set_related_target(related.map(crate::ui::EventTarget::bare));
            meta.set_bubbles(false);
            meta.set_source(crate::ui::EventSource::Synthetic);
            let mut event = PointerLeaveEvent { meta, pointer };
            element.dispatch_pointer_leave(&mut event, cx.arena(), key);
            cx.invalidate(element.local_dirty_flags());
            true
        })
        .unwrap_or(false)
}

pub(crate) fn dispatch_hover_transition(
    arena: &crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    previous_target: Option<crate::view::node_arena::NodeKey>,
    next_target: Option<crate::view::node_arena::NodeKey>,
    pointer: crate::ui::PointerEventData,
) -> bool {
    if previous_target == next_target {
        return false;
    }

    let previous_path = hover_path_for_target(arena, root_keys, previous_target);
    let next_path = hover_path_for_target(arena, root_keys, next_target);

    let mut common_prefix_len = 0;
    while common_prefix_len < previous_path.len()
        && common_prefix_len < next_path.len()
        && previous_path[common_prefix_len] == next_path[common_prefix_len]
    {
        common_prefix_len += 1;
    }

    let mut dispatched = false;

    for &k in previous_path[common_prefix_len..].iter().rev() {
        if dispatch_pointer_leave_to_key(arena, k, next_target, pointer) {
            dispatched = true;
        }
    }

    for &k in &next_path[common_prefix_len..] {
        if dispatch_pointer_enter_to_key(arena, k, previous_target, pointer) {
            dispatched = true;
        }
    }

    dispatched
}

pub(crate) fn cancel_pointer_interactions(
    arena: &crate::view::node_arena::NodeArena,
    root_key: crate::view::node_arena::NodeKey,
) -> bool {
    fn walk(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
    ) -> bool {
        arena
            .mutate_element_ref_with_invalidation(key, |element, cx| {
                let mut changed = element.cancel_pointer_interaction();
                let children: Vec<_> = element.children().to_vec();
                for child_key in children.into_iter().rev() {
                    changed |= walk(cx.arena(), child_key);
                }
                if changed {
                    cx.invalidate(element.local_dirty_flags());
                }
                changed
            })
            .unwrap_or(false)
    }

    walk(arena, root_key)
}

#[cfg(test)]
mod hover_tests {
    use super::*;

    use crate::ui::{Modifiers, PointerButtons, PointerEventData};
    use crate::view::base_component::Element;
    use crate::view::test_support::{commit_child, commit_element, new_test_arena};

    use std::cell::RefCell;
    use std::rc::Rc;

    fn test_pointer_data() -> PointerEventData {
        PointerEventData {
            viewport_x: 0.0,
            viewport_y: 0.0,
            local_x: 0.0,
            local_y: 0.0,
            button: None,
            buttons: PointerButtons::default(),
            modifiers: Modifiers::default(),
            pointer_id: 0,
            pointer_type: crate::platform::input::PointerType::Mouse,
            pressure: 0.0,
            timestamp: crate::time::Instant::now(),
        }
    }

    #[test]
    fn hover_transition_dispatches_enter_leave_on_changed_ancestors_only() {
        let order = Rc::new(RefCell::new(Vec::new()));

        let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
        let root_order = order.clone();
        root.on_pointer_enter(move |_event| root_order.borrow_mut().push("root-enter"));
        let root_order = order.clone();
        root.on_pointer_leave(move |_event| root_order.borrow_mut().push("root-leave"));

        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let parent_order = order.clone();
        parent.on_pointer_enter(move |_event| parent_order.borrow_mut().push("parent-enter"));
        let parent_order = order.clone();
        parent.on_pointer_leave(move |_event| parent_order.borrow_mut().push("parent-leave"));

        let mut child = Element::new(0.0, 0.0, 60.0, 60.0);
        let child_order = order.clone();
        child.on_pointer_enter(move |_event| child_order.borrow_mut().push("child-enter"));
        let child_order = order.clone();
        child.on_pointer_leave(move |_event| child_order.borrow_mut().push("child-leave"));

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));

        let roots = [root_key];

        assert!(dispatch_hover_transition(
            &mut arena,
            &roots,
            None,
            Some(child_key),
            test_pointer_data()
        ));
        assert_eq!(
            order.borrow().as_slice(),
            &["root-enter", "parent-enter", "child-enter"]
        );

        order.borrow_mut().clear();
        assert!(dispatch_hover_transition(
            &mut arena,
            &roots,
            Some(child_key),
            Some(parent_key),
            test_pointer_data(),
        ));
        assert_eq!(order.borrow().as_slice(), &["child-leave"]);

        order.borrow_mut().clear();
        assert!(dispatch_hover_transition(
            &mut arena,
            &roots,
            Some(parent_key),
            None,
            test_pointer_data(),
        ));
        assert_eq!(order.borrow().as_slice(), &["parent-leave", "root-leave"]);

        order.borrow_mut().clear();
        assert!(!dispatch_hover_transition(
            &mut arena,
            &roots,
            Some(root_key),
            Some(root_key),
            test_pointer_data(),
        ));
        assert!(order.borrow().is_empty());
    }
}
