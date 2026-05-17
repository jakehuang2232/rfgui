//! Scene tree helpers — hover, scroll, snapshots, transform updates, box models.

use super::*;

impl Viewport {
    pub(super) fn cancel_pointer_interactions(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
    ) -> bool {
        let mut changed = false;
        for &root_key in root_keys {
            changed |= crate::view::base_component::cancel_pointer_interactions(arena, root_key);
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
            if crate::view::base_component::update_hover_state(arena, root_key, target) {
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
        let transition_dispatched = crate::view::base_component::dispatch_hover_transition(
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

            let snapshots =
                crate::view::base_component::collect_box_models(root_key, &self.scene.node_arena);
            #[cfg(test)]
            {
                self.compositor.box_model_refresh_stats.collected_roots += 1;
                self.compositor.box_model_refresh_stats.collected_snapshots += snapshots.len();
            }
            self.compositor
                .frame_box_model_cache
                .insert(root_key, snapshots.clone());
            self.compositor.frame_box_models.extend(snapshots);
            crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
                &mut self.scene.node_arena,
                root_key,
                flags,
            );
        }
    }
}
