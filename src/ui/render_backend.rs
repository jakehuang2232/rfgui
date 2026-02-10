use crate::ui::{PropValue, RsxNode};

pub trait RenderBackend {
    type NodeId: Copy;

    fn create_root(&mut self, node: &RsxNode) -> Result<Self::NodeId, String>;
    fn replace_root(&mut self, root: Self::NodeId, node: &RsxNode) -> Result<(), String>;
    fn update_root_props(
        &mut self,
        root: Self::NodeId,
        props: &[(String, PropValue)],
    ) -> Result<(), String>;
    fn replace_root_children(
        &mut self,
        root: Self::NodeId,
        children: &[RsxNode],
    ) -> Result<(), String>;
    fn draw_frame(&mut self) -> Result<(), String>;
}
