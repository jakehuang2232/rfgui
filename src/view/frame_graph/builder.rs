use super::buffer_resource::{BufferDesc, BufferHandle, BufferResource};
use super::frame_graph::{
    AttachmentTarget, FrameGraphError, GraphicsColorAttachmentDescriptor,
    GraphicsDepthStencilAttachmentDescriptor, GraphicsPassRecordingMode, PassDescriptor,
    PassResourceUsage, ResourceHandle, ResourceLifetime, ResourceMetadata, ResourceUsage,
    SampleCountPolicy, ScissorPolicy, ViewportPolicy,
};
use super::slot::{InSlot, OutSlot};
use super::texture_resource::{TextureDesc, TextureHandle, TextureResource};

pub struct PassBuilder<'a> {
    pub(crate) descriptor: &'a mut PassDescriptor,
    pub(crate) textures: &'a mut Vec<TextureDesc>,
    pub(crate) buffers: &'a mut Vec<BufferDesc>,
    pub(crate) texture_metadata: &'a mut Vec<ResourceMetadata>,
    pub(crate) buffer_metadata: &'a mut Vec<ResourceMetadata>,
    pub(crate) usages: &'a mut Vec<PassResourceUsage>,
    pub(crate) build_errors: &'a mut Vec<FrameGraphError>,
}

impl<'a> PassBuilder<'a> {
    fn push_usage(&mut self, resource: ResourceHandle, usage: ResourceUsage) {
        self.usages.push(PassResourceUsage { resource, usage });
    }

    fn texture_target_from_output<Tag>(
        &mut self,
        output: &OutSlot<TextureResource, Tag>,
    ) -> Option<AttachmentTarget> {
        output
            .handle()
            .map(|handle| AttachmentTarget::Texture(handle))
    }

    pub fn descriptor(&self) -> &PassDescriptor {
        self.descriptor
    }

    pub fn descriptor_mut(&mut self) -> &mut PassDescriptor {
        self.descriptor
    }

    pub fn set_sample_count(&mut self, sample_count: SampleCountPolicy) {
        self.descriptor.graphics_mut().sample_count = sample_count;
    }

    pub fn set_viewport_policy(&mut self, policy: ViewportPolicy) {
        self.descriptor.graphics_mut().viewport_policy = policy;
    }

    pub fn set_scissor_policy(&mut self, policy: ScissorPolicy) {
        self.descriptor.graphics_mut().scissor_policy = policy;
    }

    pub fn set_graphics_recording_mode(&mut self, mode: GraphicsPassRecordingMode) {
        self.descriptor.graphics_mut().recording_mode = mode;
    }

    pub fn create_texture<Tag>(&mut self, desc: TextureDesc) -> OutSlot<TextureResource, Tag> {
        self.create_texture_internal(desc, ResourceLifetime::Transient, None)
    }

    pub(crate) fn create_texture_internal<Tag>(
        &mut self,
        desc: TextureDesc,
        lifetime: ResourceLifetime,
        stable_key: Option<u64>,
    ) -> OutSlot<TextureResource, Tag> {
        let handle = TextureHandle(self.textures.len() as u32);
        self.textures.push(desc);
        self.texture_metadata.push(ResourceMetadata {
            stable_key,
            kind: super::frame_graph::ResourceKind::Texture,
            allocation_class: super::frame_graph::AllocationClass::Texture,
            lifetime,
        });
        self.push_usage(ResourceHandle::Texture(handle), ResourceUsage::Produced);
        OutSlot::with_handle(handle)
    }

    pub fn create_buffer<Tag>(&mut self, desc: BufferDesc) -> OutSlot<BufferResource, Tag> {
        self.create_buffer_internal(desc, ResourceLifetime::Transient, None)
    }

    pub(crate) fn create_buffer_internal<Tag>(
        &mut self,
        desc: BufferDesc,
        lifetime: ResourceLifetime,
        stable_key: Option<u64>,
    ) -> OutSlot<BufferResource, Tag> {
        let handle = BufferHandle(self.buffers.len() as u32);
        self.buffers.push(desc);
        self.buffer_metadata.push(ResourceMetadata {
            stable_key,
            kind: super::frame_graph::ResourceKind::Buffer,
            allocation_class: super::frame_graph::AllocationClass::Buffer,
            lifetime,
        });
        self.push_usage(ResourceHandle::Buffer(handle), ResourceUsage::Produced);
        OutSlot::with_handle(handle)
    }

    pub fn declare_sampled_texture<Tag>(
        &mut self,
        input: &mut InSlot<TextureResource, Tag>,
        source: &OutSlot<TextureResource, Tag>,
    ) {
        match source.handle {
            Some(handle) => {
                input.handle = Some(handle);
                self.push_usage(ResourceHandle::Texture(handle), ResourceUsage::SampledRead);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingInput("texture slot has no handle"));
            }
        }
    }

    pub fn declare_uniform_buffer<Tag>(&mut self, buffer: &OutSlot<BufferResource, Tag>) {
        match buffer.handle() {
            Some(handle) => {
                self.push_usage(ResourceHandle::Buffer(handle), ResourceUsage::UniformRead);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingInput("buffer slot has no handle"));
            }
        }
    }

    pub fn declare_vertex_buffer<Tag>(&mut self, buffer: &OutSlot<BufferResource, Tag>) {
        match buffer.handle() {
            Some(handle) => {
                self.push_usage(ResourceHandle::Buffer(handle), ResourceUsage::VertexRead);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingInput("buffer slot has no handle"));
            }
        }
    }

    pub fn declare_index_buffer<Tag>(&mut self, buffer: &OutSlot<BufferResource, Tag>) {
        match buffer.handle() {
            Some(handle) => {
                self.push_usage(ResourceHandle::Buffer(handle), ResourceUsage::IndexRead);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingInput("buffer slot has no handle"));
            }
        }
    }

    pub fn declare_copy_src_buffer<Tag>(&mut self, buffer: &OutSlot<BufferResource, Tag>) {
        match buffer.handle() {
            Some(handle) => {
                self.push_usage(ResourceHandle::Buffer(handle), ResourceUsage::CopySrc);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingInput("buffer slot has no handle"));
            }
        }
    }

    pub fn declare_copy_dst_buffer<Tag>(&mut self, buffer: &OutSlot<BufferResource, Tag>) {
        match buffer.handle() {
            Some(handle) => {
                self.push_usage(ResourceHandle::Buffer(handle), ResourceUsage::CopyDst);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingOutput("buffer slot has no handle"));
            }
        }
    }

    pub fn declare_read_buffer<Tag>(
        &mut self,
        input: &mut InSlot<BufferResource, Tag>,
        source: &OutSlot<BufferResource, Tag>,
        usage: ResourceUsage,
    ) {
        match source.handle {
            Some(handle) => {
                input.handle = Some(handle);
                self.push_usage(ResourceHandle::Buffer(handle), usage);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingInput("buffer slot has no handle"));
            }
        }
    }

    pub fn declare_color_attachment<Tag>(
        &mut self,
        output: &OutSlot<TextureResource, Tag>,
        attachment: GraphicsColorAttachmentDescriptor,
    ) {
        let expected_target = self.texture_target_from_output(output);
        if expected_target != Some(attachment.target) {
            self.build_errors.push(FrameGraphError::MissingOutput(
                "color attachment target does not match output slot handle",
            ));
            return;
        }

        if let Some(handle) = output.handle() {
            self.push_usage(
                ResourceHandle::Texture(handle),
                ResourceUsage::ColorAttachmentWrite,
            );
        }
        self.descriptor
            .graphics_mut()
            .requirements
            .requires_color_attachment = true;
        self.descriptor
            .graphics_mut()
            .color_attachments
            .push(attachment);
    }

    pub fn declare_surface_color_attachment(
        &mut self,
        attachment: GraphicsColorAttachmentDescriptor,
    ) {
        if attachment.target != AttachmentTarget::Surface {
            self.build_errors.push(FrameGraphError::MissingOutput(
                "surface color attachment must target the surface",
            ));
            return;
        }
        self.descriptor
            .graphics_mut()
            .requirements
            .requires_color_attachment = true;
        self.descriptor
            .graphics_mut()
            .color_attachments
            .push(attachment);
    }

    pub fn declare_depth_stencil_attachment(
        &mut self,
        attachment: GraphicsDepthStencilAttachmentDescriptor,
    ) {
        let requirements = &mut self.descriptor.graphics_mut().requirements;
        if let Some(depth) = attachment.depth {
            requirements.uses_depth = true;
            requirements.writes_depth = depth.usage == ResourceUsage::DepthWrite;
        }
        if let Some(stencil) = attachment.stencil {
            requirements.uses_stencil = true;
            requirements.writes_stencil = stencil.usage == ResourceUsage::StencilWrite;
        }
        if let AttachmentTarget::Texture(handle) = attachment.target {
            if let Some(depth) = attachment.depth {
                self.push_usage(ResourceHandle::Texture(handle), depth.usage);
            }
            if let Some(stencil) = attachment.stencil {
                self.push_usage(ResourceHandle::Texture(handle), stencil.usage);
            }
        }
        self.descriptor.graphics_mut().depth_stencil_attachment = Some(attachment);
    }

    pub fn declare_texture_usage<Tag>(
        &mut self,
        output: &OutSlot<TextureResource, Tag>,
        usage: ResourceUsage,
    ) {
        match output.handle() {
            Some(handle) => {
                self.push_usage(ResourceHandle::Texture(handle), usage);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingOutput("texture slot has no handle"));
            }
        }
    }

    pub fn declare_buffer_usage<Tag>(
        &mut self,
        output: &OutSlot<BufferResource, Tag>,
        usage: ResourceUsage,
    ) {
        match output.handle() {
            Some(handle) => {
                self.push_usage(ResourceHandle::Buffer(handle), usage);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingOutput("buffer slot has no handle"));
            }
        }
    }

    pub fn surface_target(&self) -> AttachmentTarget {
        AttachmentTarget::Surface
    }

    pub fn texture_target<Tag>(
        &mut self,
        output: &OutSlot<TextureResource, Tag>,
    ) -> Option<AttachmentTarget> {
        self.texture_target_from_output(output)
    }
}
