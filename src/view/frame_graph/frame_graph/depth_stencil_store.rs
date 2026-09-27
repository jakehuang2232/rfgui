use super::*;

/// Final uses are original pass indices so one merged GPU pass can consume
/// several logical uses without changing its compatibility or splitting it.
#[derive(Clone, Debug, Default)]
pub(super) struct DepthStencilStores(FxHashMap<AttachmentTarget, usize>);

impl DepthStencilStores {
    pub(super) fn compile(
        resources: &[CompiledResource],
        passes: &[CompiledPass],
        sinks: &[ExternalSink],
    ) -> Self {
        let mut final_uses = FxHashMap::default();
        for resource in resources {
            if resource.lifetime != ResourceLifetime::Transient
                || sinks
                    .iter()
                    .any(|sink| sink.target == ExternalSinkTarget::Resource(resource.handle))
            {
                continue;
            }
            let ResourceHandle::Texture(handle) = resource.handle else {
                continue;
            };
            let last = &passes[resource.last_use_pass_index];
            // A later sample/copy still needs stored contents. Only an actual
            // final attachment use can discard them.
            if let PassDetails::Graphics(graphics) = &last.descriptor.details {
                if graphics
                    .depth_stencil_attachment
                    .as_ref()
                    .is_some_and(|attachment| {
                        attachment.target == AttachmentTarget::Texture(handle)
                    })
                {
                    final_uses.insert(AttachmentTarget::Texture(handle), last.original_index);
                }
            }
        }
        // The viewport depth attachment is frame-local, unlike the surface
        // color image that must survive for presentation.
        for pass in passes {
            if let PassDetails::Graphics(graphics) = &pass.descriptor.details {
                if graphics
                    .depth_stencil_attachment
                    .as_ref()
                    .is_some_and(|attachment| attachment.target == AttachmentTarget::Surface)
                {
                    final_uses.insert(AttachmentTarget::Surface, pass.original_index);
                }
            }
        }
        Self(final_uses)
    }

    pub(super) fn discard_after(&self, target: AttachmentTarget, pass_indices: &[usize]) -> bool {
        self.0
            .get(&target)
            .is_some_and(|last| pass_indices.contains(last))
    }
}
