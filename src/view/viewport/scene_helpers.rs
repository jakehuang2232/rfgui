//! Scene tree helpers — hover, scroll, snapshots, transform updates, box models.

use super::*;

impl Viewport {
    pub(super) fn cancel_pointer_interactions(
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
    ) -> bool {
        let mut changed = false;
        for root in roots.iter_mut() {
            changed |= crate::view::base_component::cancel_pointer_interactions(root.as_mut());
        }
        changed
    }

    pub(super) fn apply_hover_target(
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        target: Option<u64>,
    ) -> bool {
        let mut changed = false;
        for root in roots.iter_mut() {
            if crate::view::base_component::update_hover_state(root.as_mut(), target) {
                changed = true;
            }
        }
        changed
    }

    pub(super) fn sync_hover_target(
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        hovered_node_id: &mut Option<u64>,
        next_target: Option<u64>,
    ) -> (bool, bool) {
        let transition_dispatched =
            crate::view::base_component::dispatch_hover_transition(roots, *hovered_node_id, next_target);
        *hovered_node_id = next_target;
        let hover_changed = Self::apply_hover_target(roots, next_target);
        (hover_changed, transition_dispatched)
    }

    pub(super) fn sync_hover_visual_only(
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        hovered_node_id: &mut Option<u64>,
        next_target: Option<u64>,
    ) -> bool {
        *hovered_node_id = next_target;
        Self::apply_hover_target(roots, next_target)
    }

    pub(super) fn save_scroll_states(
        roots: &[Box<dyn crate::view::base_component::ElementTrait>],
        map: &mut HashMap<u64, (f32, f32)>,
    ) {
        for root in roots {
            let offset = root.get_scroll_offset();
            if offset != (0.0, 0.0) {
                map.insert(root.id(), offset);
            }
            if let Some(children) = root.children() {
                Self::save_scroll_states(children, map);
            }
        }
    }

    pub(super) fn restore_scroll_states(
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        map: &HashMap<u64, (f32, f32)>,
    ) {
        for root in roots {
            if let Some(offset) = map.get(&root.id()) {
                root.set_scroll_offset(*offset);
            }
            if let Some(children) = root.children_mut() {
                Self::restore_scroll_states(children, map);
            }
        }
    }

    pub(super) fn save_element_snapshots(
        roots: &[Box<dyn crate::view::base_component::ElementTrait>],
        map: &mut HashMap<u64, Box<dyn Any>>,
    ) {
        for root in roots {
            if let Some(snapshot) = root.snapshot_state() {
                map.insert(root.id(), snapshot);
            }
            if let Some(children) = root.children() {
                Self::save_element_snapshots(children, map);
            }
        }
    }

    pub(super) fn restore_element_snapshots(
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        map: &HashMap<u64, Box<dyn Any>>,
    ) {
        for root in roots {
            if let Some(snapshot) = map.get(&root.id()) {
                let _ = root.restore_state(snapshot.as_ref());
            }
            if let Some(children) = root.children_mut() {
                Self::restore_element_snapshots(children, map);
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
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
        node_id: u64,
        style: &Style,
    ) -> bool {
        for root in roots.iter_mut() {
            if let Some(element) = Self::element_by_id_mut(root.as_mut(), node_id) {
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
            if !Self::apply_placement_style_by_node_id(&mut self.scene.ui_roots, *node_id, style) {
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

    pub(super) fn element_by_id_mut(
        root: &mut dyn crate::view::base_component::ElementTrait,
        node_id: u64,
    ) -> Option<&mut crate::view::base_component::Element> {
        if root.id() == node_id {
            return root
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>();
        }
        if let Some(children) = root.children_mut() {
            for child in children.iter_mut() {
                if let Some(found) = Self::element_by_id_mut(child.as_mut(), node_id) {
                    return Some(found);
                }
            }
        }
        None
    }

    pub(super) fn refresh_frame_box_models(
        &mut self,
        roots: &mut [Box<dyn crate::view::base_component::ElementTrait>],
    ) {
        self.compositor.frame_box_models.clear();
        let mut active_root_ids = HashSet::new();
        for root in roots.iter_mut() {
            let root_id = root.id();
            active_root_ids.insert(root_id);
            let dirty = crate::view::base_component::subtree_dirty_flags(root.as_ref());
            let needs_refresh = dirty.intersects(
                crate::view::base_component::DirtyFlags::LAYOUT
                    .union(crate::view::base_component::DirtyFlags::PLACE)
                    .union(crate::view::base_component::DirtyFlags::BOX_MODEL)
                    .union(crate::view::base_component::DirtyFlags::HIT_TEST),
            ) || !self.compositor.cached_root_box_models.contains_key(&root_id);
            if needs_refresh {
                let snapshots = crate::view::base_component::collect_box_models(root.as_ref());
                self.compositor.cached_root_box_models.insert(root_id, snapshots);
            }
            if let Some(snapshots) = self.compositor.cached_root_box_models.get(&root_id) {
                self.compositor.frame_box_models.extend_from_slice(snapshots);
            }
        }
        self.compositor.cached_root_box_models
            .retain(|root_id, _| active_root_ids.contains(root_id));
        for root in roots.iter_mut() {
            crate::view::base_component::clear_subtree_dirty_flags(
                root.as_mut(),
                crate::view::base_component::DirtyFlags::BOX_MODEL
                    .union(crate::view::base_component::DirtyFlags::HIT_TEST),
            );
        }
    }
}
