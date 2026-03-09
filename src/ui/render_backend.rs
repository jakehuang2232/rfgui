use crate::ui::reconciler::Patch;
use crate::ui::RsxNode;

pub trait RenderBackend {
    type NodeId: Copy;

    fn create_root(&mut self, node: &RsxNode) -> Result<Self::NodeId, String>;
    fn replace_root(&mut self, root: Self::NodeId, node: &RsxNode) -> Result<(), String>;
    fn apply_patch(&mut self, root: Self::NodeId, patch: &Patch) -> Result<(), String>;
    fn draw_frame(&mut self) -> Result<(), String>;
    fn request_redraw(&mut self) -> Result<(), String>;
}
