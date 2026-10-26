//! Where a node's layout geometry sits in the viewport.
//!
//! Geometry handed out of layout (event coordinates and target bounds, the
//! IME cursor, box-model captures, debug overlays) and comparisons with
//! viewport points convert here. Placement puts the nodes inside a scroll
//! container at their scroll-zero positions, so a node's layout space is the
//! viewport moved by the offsets of every scroll container above it.
use super::*;
use crate::view::base_component::BoxModelSnapshot;

impl NodeArena {
    /// Physical pixels per logical pixel of the viewport: scroll offsets
    /// move content by whole device pixels at this scale.
    pub(crate) fn device_scale(&self) -> f32 {
        self.device_scale.unwrap_or(1.0)
    }

    pub(crate) fn set_device_scale(&mut self, device_scale: f32) {
        self.device_scale = Some(device_scale);
    }

    /// The translation from `key`'s layout space to the viewport.
    pub(crate) fn viewport_offset(&self, key: NodeKey) -> [f32; 2] {
        let mut offset = [0.0, 0.0];
        let mut cursor = self.parent_of(key);
        for _ in 0..self.slots.len() {
            let Some(ancestor) = cursor else {
                break;
            };
            if let Some((x, y)) = self.get(ancestor).and_then(|node| {
                crate::view::base_component::host_element(node.element.as_ref())
                    .map(|element| element.children_space_offset())
            }) {
                offset[0] -= x;
                offset[1] -= y;
            }
            cursor = self.parent_of(ancestor);
        }
        offset
    }

    /// `snapshot`, `key`'s box model, in the viewport.
    pub(crate) fn box_model_in_viewport(
        &self,
        key: NodeKey,
        snapshot: BoxModelSnapshot,
    ) -> BoxModelSnapshot {
        let [dx, dy] = self.viewport_offset(key);
        BoxModelSnapshot {
            x: snapshot.x + dx,
            y: snapshot.y + dy,
            ..snapshot
        }
    }

    /// A viewport point in `key`'s layout space.
    pub(crate) fn point_in_layout_space(&self, key: NodeKey, x: f32, y: f32) -> (f32, f32) {
        let [dx, dy] = self.viewport_offset(key);
        (x - dx, y - dy)
    }
}
