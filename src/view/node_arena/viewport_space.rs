//! Where a node's layout geometry sits in the viewport.
//!
//! Geometry handed out of layout (event coordinates and target bounds, the
//! IME cursor, box-model captures, debug overlays) and comparisons with
//! viewport points convert here. Placement applies each scroll container's
//! offset to the positions it gives the nodes inside, so a node's layout
//! space is currently the viewport itself.
use super::*;
use crate::view::base_component::BoxModelSnapshot;

impl NodeArena {
    /// The translation from `key`'s layout space to the viewport.
    pub(crate) fn viewport_offset(&self, _key: NodeKey) -> [f32; 2] {
        [0.0, 0.0]
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
