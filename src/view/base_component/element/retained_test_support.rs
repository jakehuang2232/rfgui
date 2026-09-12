//! Helpers used only by unit tests.

use super::*;

pub(crate) fn root_effect_stable_key(root: NodeKey) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::RootEffectColor, root.data().as_ffi())
}

impl UiBuildContext {
    pub(crate) fn allocate_persistent_full_viewport_target(
        &mut self,
        graph: &mut FrameGraph,
        stable_key: PersistentTextureKey,
    ) -> RenderTargetOut {
        let desc = self.persistent_full_viewport_target_desc(stable_key);
        self.next_persistent_target_with_desc(graph, desc, stable_key)
    }

    pub(crate) fn persistent_full_viewport_target_desc(
        &self,
        stable_key: PersistentTextureKey,
    ) -> TextureDesc {
        let desc = TextureDesc::new(
            self.viewport.target_width,
            self.viewport.target_height,
            self.viewport.target_format,
            wgpu::TextureDimension::D2,
        );
        persistent_target_texture_descriptors(desc, stable_key).0
    }
}

impl Element {
    /// Offset-zero recorder oracle for a generalized native content subtree.
    ///
    /// Unlike the original direct-leaf and TextArea-wrapper admissions, this
    /// helper deliberately permits children and absolute positioning. The
    /// typed subtree recorder remains responsible for each descendant's paint
    /// grammar; this oracle only proves that the content root itself has a
    /// stable, property-neutral placement from which the complete 2D scroll
    /// offset can be normalized.
    pub(crate) fn exact_retained_scroll_content_subtree_recording_offset(
        &self,
        parent_offset: [f32; 2],
    ) -> Option<[f32; 2]> {
        if !self.layout_state.should_render
            || ![
                self.layout_state.layout_position.x,
                self.layout_state.layout_position.y,
                self.layout_state.layout_size.width,
                self.layout_state.layout_size.height,
            ]
            .into_iter()
            .all(f32::is_finite)
            || self.layout_state.layout_size.width <= 0.0
            || self.layout_state.layout_size.height <= 0.0
            || self.scroll_direction != ScrollDirection::None
            || self.resolved_transform.is_some()
            || self.has_active_layout_transition()
            || self.has_active_animator()
            || self.should_append_to_root_viewport_render()
            || parent_offset.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let paint_x = self.layout_state.layout_position.x + parent_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_offset[1];
        Some([
            parent_offset[0] + round_layout_value(paint_x) - paint_x,
            parent_offset[1] + round_layout_value(paint_y) - paint_y,
        ])
    }

    pub(crate) fn exact_retained_scroll_content_wrapper_recording_offset(
        &self,
        parent_offset: [f32; 2],
    ) -> Option<[f32; 2]> {
        // The direct-leaf helper includes `children.is_empty()`. Spell the
        // otherwise identical wrapper contract here instead of weakening that
        // established oracle.
        if self.children.len() != 1
            || !self.layout_state.should_render
            || !self.core.should_paint
            || ![
                self.layout_state.layout_position.x,
                self.layout_state.layout_position.y,
                self.layout_state.layout_size.width,
                self.layout_state.layout_size.height,
            ]
            .into_iter()
            .all(f32::is_finite)
            || self.layout_state.layout_size.width <= 0.0
            || self.layout_state.layout_size.height <= 0.0
            || self.opacity.to_bits() != 1.0_f32.to_bits()
            || self.scroll_direction != ScrollDirection::None
            || self.resolved_transform.is_some()
            || !self.box_shadows.is_empty()
            || self.has_active_layout_transition()
            || self.has_active_animator()
            || self.inline_ifc_owned_by_root
            || self.is_owning_inline_ifc_root_role()
            || self.is_fragmentable_inline_element()
            || self.should_append_to_root_viewport_render()
            || self.absolute_clip_scissor_rect().is_some()
            || self.retained_paint_properties().has_rounded_clip
            || self.computed_style.position.mode() == PositionMode::Absolute
            || parent_offset.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let paint_x = self.layout_state.layout_position.x + parent_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_offset[1];
        Some([
            parent_offset[0] + round_layout_value(paint_x) - paint_x,
            parent_offset[1] + round_layout_value(paint_y) - paint_y,
        ])
    }
}

use slotmap::Key;

impl UiBuildContext {
    pub(crate) fn merge_child_render_state(&mut self, child: &BuildState) {
        self.state.merge_child_render_state(child);
    }
}
