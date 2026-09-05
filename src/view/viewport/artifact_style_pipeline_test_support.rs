use super::Viewport;
use crate::view::node_arena::{NodeArena, NodeKey};

/// Run the complete production layout lifecycle, including box-model refresh
/// and its arena dirty-cache repair, on a persistent test scene.
///
/// A layout panic terminates the test with its original payload. This helper
/// does not support catching that panic and resuming the fixture: production
/// run_layout_pass also takes ownership of the arena internally, so restoring
/// only this outer transfer cannot make a failed layout unwind-safe.
pub(crate) fn layout_artifact_style_scene_for_test(
    viewport: &mut Viewport,
    arena: &mut NodeArena,
    root: NodeKey,
    logical_size: [f32; 2],
) {
    viewport.scene.node_arena = std::mem::take(arena);
    viewport.scene.ui_root_keys = vec![root];
    viewport.logical_width = logical_size[0];
    viewport.logical_height = logical_size[1];
    viewport.run_layout_pass();
    *arena = std::mem::take(&mut viewport.scene.node_arena);
}
