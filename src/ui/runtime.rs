use crate::ui::reconciler::{Patch, reconcile};
use crate::ui::{RenderBackend, RsxNode};

pub struct UiRuntime<B: RenderBackend> {
    backend: B,
    current: Option<RsxNode>,
    root_id: Option<B::NodeId>,
}

impl<B: RenderBackend> UiRuntime<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            current: None,
            root_id: None,
        }
    }

    pub fn mount(&mut self, root: RsxNode) -> Result<(), String> {
        let root_id = self.backend.create_root(&root)?;
        self.current = Some(root);
        self.root_id = Some(root_id);
        self.backend.draw_frame()
    }

    pub fn update(&mut self, next: RsxNode) -> Result<(), String> {
        let Some(root_id) = self.root_id else {
            return self.mount(next);
        };

        let patches = reconcile(self.current.as_ref(), &next);
        self.apply_patches(root_id, &patches, &next)?;
        self.current = Some(next);
        self.backend.draw_frame()
    }

    fn apply_patches(
        &mut self,
        root_id: B::NodeId,
        patches: &[Patch],
        next: &RsxNode,
    ) -> Result<(), String> {
        for patch in patches {
            match patch {
                Patch::ReplaceRoot(node) => self.backend.replace_root(root_id, node)?,
                Patch::UpdateProps(props) => self.backend.update_root_props(root_id, props)?,
                Patch::ReplaceChildren(children) => {
                    self.backend.replace_root_children(root_id, children)?
                }
            }
        }

        if patches.is_empty() && self.current.as_ref() != Some(next) {
            self.backend.replace_root(root_id, next)?;
        }

        Ok(())
    }

    pub fn into_backend(self) -> B {
        self.backend
    }
}
