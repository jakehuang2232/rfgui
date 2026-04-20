//! Scene tree helpers — hover, scroll, snapshots, transform updates, box models.

use super::*;

impl Viewport {
    pub(super) fn cancel_pointer_interactions(
        arena: &mut crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
    ) -> bool {
        let mut changed = false;
        for &root_key in root_keys {
            changed |= crate::view::base_component::cancel_pointer_interactions(arena, root_key);
        }
        changed
    }

    pub(super) fn apply_hover_target(
        arena: &mut crate::view::node_arena::NodeArena,
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
        arena: &mut crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        hovered_node_id: &mut Option<crate::view::node_arena::NodeKey>,
        next_target: Option<crate::view::node_arena::NodeKey>,
    ) -> (bool, bool) {
        let transition_dispatched = crate::view::base_component::dispatch_hover_transition(
            arena,
            root_keys,
            *hovered_node_id,
            next_target,
        );
        *hovered_node_id = next_target;
        let hover_changed = Self::apply_hover_target(arena, root_keys, next_target);
        (hover_changed, transition_dispatched)
    }

    pub(super) fn sync_hover_visual_only(
        arena: &mut crate::view::node_arena::NodeArena,
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
            node: &mut dyn crate::view::base_component::ElementTrait,
            arena: &crate::view::node_arena::NodeArena,
            map: &FxHashMap<u64, (f32, f32)>,
        ) {
            if let Some(offset) = map.get(&node.stable_id()) {
                node.set_scroll_offset(*offset);
            }
            let child_keys: Vec<crate::view::node_arena::NodeKey> = node.children().to_vec();
            for child_key in child_keys {
                if let Some(mut child_node) = arena.get_mut(child_key) {
                    walk(child_node.element.as_mut(), arena, map);
                }
            }
        }
        for &root_key in root_keys {
            if let Some(mut root_node) = arena.get_mut(root_key) {
                walk(root_node.element.as_mut(), arena, map);
            }
        }
    }

    pub(super) fn save_element_snapshots(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        map: &mut FxHashMap<u64, Box<dyn Any>>,
    ) {
        fn walk(
            node: &dyn crate::view::base_component::ElementTrait,
            arena: &crate::view::node_arena::NodeArena,
            map: &mut FxHashMap<u64, Box<dyn Any>>,
        ) {
            if let Some(snapshot) = node.snapshot_state() {
                map.insert(node.stable_id(), snapshot);
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

    pub(super) fn restore_element_snapshots(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        map: &FxHashMap<u64, Box<dyn Any>>,
    ) {
        fn walk(
            node: &mut dyn crate::view::base_component::ElementTrait,
            arena: &crate::view::node_arena::NodeArena,
            map: &FxHashMap<u64, Box<dyn Any>>,
        ) {
            if let Some(snapshot) = map.get(&node.stable_id()) {
                let _ = node.restore_state(snapshot.as_ref());
            }
            let child_keys: Vec<crate::view::node_arena::NodeKey> = node.children().to_vec();
            for child_key in child_keys {
                if let Some(mut child_node) = arena.get_mut(child_key) {
                    walk(child_node.element.as_mut(), arena, map);
                }
            }
        }
        for &root_key in root_keys {
            if let Some(mut root_node) = arena.get_mut(root_key) {
                walk(root_node.element.as_mut(), arena, map);
            }
        }
    }

    pub(super) fn extract_style_prop(props: &[(&'static str, PropValue)]) -> Result<Option<Style>, String> {
        let Some((_, value)) = props.iter().find(|(key, _)| *key == "style") else {
            return Ok(None);
        };
        Self::extract_style_from_value(value)
    }

    pub(super) fn extract_style_from_value(value: &PropValue) -> Result<Option<Style>, String> {
        Self::extract_style_from_value_owned(value.clone())
    }

    pub(super) fn extract_style_from_value_owned(value: PropValue) -> Result<Option<Style>, String> {
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
        let old_style = Self::extract_style_prop(old_props)?.unwrap_or_default();
        let new_style = Self::extract_style_from_value(&changed[0].1)?.unwrap_or_default();
        if old_style.clone().without_properties_recursive(&Self::PLACEMENT_SAFE_PROPERTIES)
            != new_style.clone().without_properties_recursive(&Self::PLACEMENT_SAFE_PROPERTIES)
        {
            return Ok(false);
        }
        // At least one placement-safe property must have actually changed.
        let any_changed = Self::PLACEMENT_SAFE_PROPERTIES.iter().any(|id| {
            old_style.get(*id) != new_style.get(*id)
        });
        if !any_changed {
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn apply_placement_style_by_node_id(
        arena: &crate::view::node_arena::NodeArena,
        root_keys: &[crate::view::node_arena::NodeKey],
        node_id: u64,
        style: &Style,
    ) -> bool {
        for &root_key in root_keys {
            let Some(mut root_node) = arena.get_mut(root_key) else { continue };
            if let Some(element) = Self::element_by_id_mut(root_node.element.as_mut(), node_id, arena) {
                let mut patch_style = Style::new();
                if let Some(crate::ParsedValue::Transform(transform)) =
                    style.get(PropertyId::Transform)
                {
                    patch_style.set_transform(transform.clone());
                }
                if let Some(crate::ParsedValue::TransformOrigin(origin)) =
                    style.get(PropertyId::TransformOrigin)
                {
                    patch_style.set_transform_origin(*origin);
                }
                if let Some(crate::ParsedValue::Position(position)) =
                    style.get(PropertyId::Position)
                {
                    patch_style.insert(
                        PropertyId::Position,
                        crate::ParsedValue::Position(position.clone()),
                    );
                }
                element.apply_style(patch_style);
                return true;
            }
        }
        false
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
            let Patch::UpdateElementProps { path, changed, removed } = patch else {
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
            let style = Self::extract_style_from_value(&changed[0].1)?.unwrap_or_default();
            let node_id = crate::view::renderer_adapter::rendered_node_id_by_index_path(root, path)?
                .ok_or_else(|| "target redraw patch resolved to a fragment".to_string())?;
            updates.push((node_id, style));
        }

        for (node_id, style) in &updates {
            if !Self::apply_placement_style_by_node_id(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                *node_id,
                style,
            ) {
                return Ok(false);
            }
        }
        self.scene.last_rsx_root = Some(root.clone());
        Ok(true)
    }

    pub(super) fn rsx_node_by_index_path<'a>(node: &'a RsxNode, path: &[usize]) -> Option<&'a RsxNode> {
        if path.is_empty() {
            return Some(node);
        }
        let children = node.children()?;
        let child = children.get(path[0])?;
        Self::rsx_node_by_index_path(child, &path[1..])
    }

    pub(super) fn element_by_id_mut<'a>(
        root: &'a mut dyn crate::view::base_component::ElementTrait,
        node_id: u64,
        _arena: &'a crate::view::node_arena::NodeArena,
    ) -> Option<&'a mut crate::view::base_component::Element> {
        // NOTE: arena-backed children are now accessed via `NodeKey`. This
        // helper only resolves the direct root; full-tree search via the
        // arena is handled by the dispatch/events stack and is out of
        // scope for this refactor step.
        if root.stable_id() == node_id {
            return root
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>();
        }
        None
    }

    pub(super) fn refresh_frame_box_models(&mut self) {
        self.compositor.frame_box_models.clear();
        let arena = &self.scene.node_arena;
        let root_keys = self.scene.ui_root_keys.clone();
        for &root_key in &root_keys {
            let snapshots =
                crate::view::base_component::collect_box_models(root_key, arena);
            self.compositor.frame_box_models.extend(snapshots);
        }
        for &root_key in &root_keys {
            if let Some(mut root_node) = arena.get_mut(root_key) {
                crate::view::base_component::clear_subtree_dirty_flags(
                    root_node.element.as_mut(),
                    crate::view::base_component::DirtyFlags::BOX_MODEL
                        .union(crate::view::base_component::DirtyFlags::HIT_TEST),
                    arena,
                );
            }
        }
    }
}
