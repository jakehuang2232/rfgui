use super::buffer_resource::{BufferDesc, BufferHandle, BufferResource};
use super::frame_graph::{
    AttachmentLoadOp, AttachmentTarget, FrameGraphError, GraphicsColorAttachmentDescriptor,
    GraphicsColorAttachmentOps, GraphicsDepthAspectDescriptor,
    GraphicsDepthStencilAttachmentDescriptor, GraphicsPassMergePolicy,
    GraphicsStencilAspectDescriptor, PassDescriptor, PassResourceUsage, ResourceHandle,
    ResourceLifetime, ResourceMetadata, ResourceUsage, SampleCountPolicy, ScissorPolicy,
    ViewportPolicy,
};
use super::slot::{InSlot, OutSlot, ResourceType};
use super::texture_resource::{TextureDesc, TextureHandle, TextureResource};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferReadUsage {
    Uniform,
    Vertex,
    Index,
}

impl BufferReadUsage {
    fn resource_usage(self) -> ResourceUsage {
        match self {
            Self::Uniform => ResourceUsage::UniformRead,
            Self::Vertex => ResourceUsage::VertexRead,
            Self::Index => ResourceUsage::IndexRead,
        }
    }
}

pub trait UsageTrackedResource: ResourceType {
    fn into_resource_handle(handle: Self::Handle) -> ResourceHandle;
    fn missing_input_error() -> &'static str;
    fn missing_output_error() -> &'static str;
}

impl UsageTrackedResource for TextureResource {
    fn into_resource_handle(handle: Self::Handle) -> ResourceHandle {
        ResourceHandle::Texture(handle)
    }

    fn missing_input_error() -> &'static str {
        "texture slot has no handle"
    }

    fn missing_output_error() -> &'static str {
        "texture slot has no handle"
    }
}

impl UsageTrackedResource for BufferResource {
    fn into_resource_handle(handle: Self::Handle) -> ResourceHandle {
        ResourceHandle::Buffer(handle)
    }

    fn missing_input_error() -> &'static str {
        "buffer slot has no handle"
    }

    fn missing_output_error() -> &'static str {
        "buffer slot has no handle"
    }
}

pub(crate) struct PassBuilderState<'a> {
    pub(crate) descriptor: &'a mut PassDescriptor,
    pub(crate) textures: &'a mut Vec<TextureDesc>,
    pub(crate) texture_attachment_pairs: &'a HashMap<TextureHandle, AttachmentTarget>,
    pub(crate) buffers: &'a mut Vec<BufferDesc>,
    pub(crate) texture_metadata: &'a mut Vec<ResourceMetadata>,
    pub(crate) buffer_metadata: &'a mut Vec<ResourceMetadata>,
    pub(crate) usages: &'a mut Vec<PassResourceUsage>,
    pub(crate) build_errors: &'a mut Vec<FrameGraphError>,
}

impl<'a> PassBuilderState<'a> {
    fn push_usage(&mut self, resource: ResourceHandle, usage: ResourceUsage) {
        self.usages.push(PassResourceUsage {
            resource,
            usage,
            read_version: None,
            write_version: None,
        });
    }

    fn bind_read_usage<R: UsageTrackedResource, Tag>(
        &mut self,
        input: &mut InSlot<R, Tag>,
        source: &OutSlot<R, Tag>,
        usage: ResourceUsage,
    ) {
        match source.handle {
            Some(handle) => {
                input.handle = Some(handle);
                self.push_usage(R::into_resource_handle(handle), usage);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingInput(R::missing_input_error()));
            }
        }
    }

    fn declare_usage<R: UsageTrackedResource, Tag>(
        &mut self,
        output: &OutSlot<R, Tag>,
        usage: ResourceUsage,
    ) {
        match output.handle() {
            Some(handle) => {
                self.push_usage(R::into_resource_handle(handle), usage);
            }
            None => {
                self.build_errors
                    .push(FrameGraphError::MissingOutput(R::missing_output_error()));
            }
        }
    }

    fn texture_target_from_output<Tag>(
        &mut self,
        output: &OutSlot<TextureResource, Tag>,
    ) -> Option<AttachmentTarget> {
        output
            .handle()
            .map(|handle| AttachmentTarget::Texture(handle))
    }

    fn inferred_depth_stencil_target(&mut self) -> Option<AttachmentTarget> {
        self.descriptor
            .graphics_mut()
            .color_attachments
            .last()
            .and_then(|attachment| match attachment.target {
                AttachmentTarget::Surface => Some(AttachmentTarget::Surface),
                AttachmentTarget::Texture(handle) => {
                    self.texture_attachment_pairs.get(&handle).copied()
                }
            })
    }

    fn descriptor(&self) -> &PassDescriptor {
        self.descriptor
    }

    fn descriptor_mut(&mut self) -> &mut PassDescriptor {
        self.descriptor
    }

    fn create_texture<Tag>(&mut self, desc: TextureDesc) -> OutSlot<TextureResource, Tag> {
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

    fn create_buffer<Tag>(&mut self, desc: BufferDesc) -> OutSlot<BufferResource, Tag> {
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

    fn read_texture<Tag>(
        &mut self,
        input: &mut InSlot<TextureResource, Tag>,
        source: &OutSlot<TextureResource, Tag>,
    ) {
        self.bind_read_usage(input, source, ResourceUsage::SampledRead);
    }

    fn read_buffer<Tag>(&mut self, buffer: &OutSlot<BufferResource, Tag>, usage: BufferReadUsage) {
        self.declare_usage(buffer, usage.resource_usage());
    }

    fn write_buffer<Tag>(&mut self, buffer: &OutSlot<BufferResource, Tag>) {
        self.declare_usage(buffer, ResourceUsage::BufferWrite);
    }

    fn copy_src<R: UsageTrackedResource, Tag>(&mut self, resource: &OutSlot<R, Tag>) {
        self.declare_usage(resource, ResourceUsage::CopySrc);
    }

    fn copy_dst<R: UsageTrackedResource, Tag>(&mut self, resource: &OutSlot<R, Tag>) {
        self.declare_usage(resource, ResourceUsage::CopyDst);
    }

    fn read_storage<R: UsageTrackedResource, Tag>(&mut self, resource: &OutSlot<R, Tag>) {
        self.declare_usage(resource, ResourceUsage::StorageRead);
    }

    fn write_storage<R: UsageTrackedResource, Tag>(&mut self, resource: &OutSlot<R, Tag>) {
        self.declare_usage(resource, ResourceUsage::StorageWrite);
    }

    fn write_color_attachment<Tag>(
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

    fn write_surface_color_attachment(&mut self, attachment: GraphicsColorAttachmentDescriptor) {
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

    fn merge_depth_stencil_attachment(
        &mut self,
        target: AttachmentTarget,
        depth: Option<GraphicsDepthAspectDescriptor>,
        stencil: Option<GraphicsStencilAspectDescriptor>,
    ) {
        let requirements = &mut self.descriptor.graphics_mut().requirements;
        if let Some(depth) = depth {
            requirements.uses_depth = true;
            requirements.writes_depth = depth.usage == ResourceUsage::DepthWrite;
        }
        if let Some(stencil) = stencil {
            requirements.uses_stencil = true;
            requirements.writes_stencil = stencil.usage == ResourceUsage::StencilWrite;
        }
        if let AttachmentTarget::Texture(handle) = target {
            if let Some(depth) = depth {
                self.push_usage(ResourceHandle::Texture(handle), depth.usage);
            }
            if let Some(stencil) = stencil {
                self.push_usage(ResourceHandle::Texture(handle), stencil.usage);
            }
        }
        let mut attachment = self
            .descriptor
            .graphics_mut()
            .depth_stencil_attachment
            .unwrap_or(GraphicsDepthStencilAttachmentDescriptor {
                target,
                depth: None,
                stencil: None,
            });
        if attachment.target != target {
            self.build_errors.push(FrameGraphError::Validation(
                "depth/stencil attachment target does not match existing target".into(),
            ));
            return;
        }
        if let Some(depth) = depth {
            attachment.depth = Some(depth);
        }
        if let Some(stencil) = stencil {
            attachment.stencil = Some(stencil);
        }
        self.descriptor.graphics_mut().depth_stencil_attachment = Some(attachment);
    }

    fn read_depth(&mut self, target: AttachmentTarget) {
        self.merge_depth_stencil_attachment(
            target,
            Some(GraphicsDepthAspectDescriptor::read()),
            None,
        );
    }

    fn write_depth(
        &mut self,
        target: AttachmentTarget,
        load_op: AttachmentLoadOp,
        clear_depth: Option<f32>,
    ) {
        self.merge_depth_stencil_attachment(
            target,
            Some(GraphicsDepthAspectDescriptor::write(load_op, clear_depth)),
            None,
        );
    }

    fn read_stencil(&mut self, target: AttachmentTarget) {
        self.merge_depth_stencil_attachment(
            target,
            None,
            Some(GraphicsStencilAspectDescriptor::read()),
        );
    }

    fn write_stencil(
        &mut self,
        target: AttachmentTarget,
        load_op: AttachmentLoadOp,
        clear_stencil: Option<u32>,
    ) {
        self.merge_depth_stencil_attachment(
            target,
            None,
            Some(GraphicsStencilAspectDescriptor::write(
                load_op,
                clear_stencil,
            )),
        );
    }

    fn surface_target(&self) -> AttachmentTarget {
        AttachmentTarget::Surface
    }

    fn texture_target<Tag>(
        &mut self,
        output: &OutSlot<TextureResource, Tag>,
    ) -> Option<AttachmentTarget> {
        self.texture_target_from_output(output)
    }
}

macro_rules! impl_shared_builder_api {
    ($ty:ident) => {
        impl<'a, 'b> $ty<'a, 'b> {
            pub fn descriptor(&self) -> &PassDescriptor {
                self.state.descriptor()
            }

            pub fn descriptor_mut(&mut self) -> &mut PassDescriptor {
                self.state.descriptor_mut()
            }

            pub fn create_texture<Tag>(
                &mut self,
                desc: TextureDesc,
            ) -> OutSlot<TextureResource, Tag> {
                self.state.create_texture(desc)
            }

            #[allow(dead_code)]
            pub(crate) fn create_texture_internal<Tag>(
                &mut self,
                desc: TextureDesc,
                lifetime: ResourceLifetime,
                stable_key: Option<u64>,
            ) -> OutSlot<TextureResource, Tag> {
                self.state
                    .create_texture_internal(desc, lifetime, stable_key)
            }

            pub fn create_buffer<Tag>(&mut self, desc: BufferDesc) -> OutSlot<BufferResource, Tag> {
                self.state.create_buffer(desc)
            }

            #[allow(dead_code)]
            pub(crate) fn create_buffer_internal<Tag>(
                &mut self,
                desc: BufferDesc,
                lifetime: ResourceLifetime,
                stable_key: Option<u64>,
            ) -> OutSlot<BufferResource, Tag> {
                self.state
                    .create_buffer_internal(desc, lifetime, stable_key)
            }

            pub fn read_texture<Tag>(
                &mut self,
                input: &mut InSlot<TextureResource, Tag>,
                source: &OutSlot<TextureResource, Tag>,
            ) {
                self.state.read_texture(input, source);
            }

            pub fn read_buffer<Tag>(
                &mut self,
                buffer: &OutSlot<BufferResource, Tag>,
                usage: BufferReadUsage,
            ) {
                self.state.read_buffer(buffer, usage);
            }

            pub fn write_buffer<Tag>(&mut self, buffer: &OutSlot<BufferResource, Tag>) {
                self.state.write_buffer(buffer);
            }

            pub fn copy_src<R: UsageTrackedResource, Tag>(&mut self, resource: &OutSlot<R, Tag>) {
                self.state.copy_src(resource);
            }

            pub fn copy_dst<R: UsageTrackedResource, Tag>(&mut self, resource: &OutSlot<R, Tag>) {
                self.state.copy_dst(resource);
            }

            pub fn read_storage<R: UsageTrackedResource, Tag>(
                &mut self,
                resource: &OutSlot<R, Tag>,
            ) {
                self.state.read_storage(resource);
            }

            pub fn write_storage<R: UsageTrackedResource, Tag>(
                &mut self,
                resource: &OutSlot<R, Tag>,
            ) {
                self.state.write_storage(resource);
            }

            pub fn surface_target(&self) -> AttachmentTarget {
                self.state.surface_target()
            }

            pub fn texture_target<Tag>(
                &mut self,
                output: &OutSlot<TextureResource, Tag>,
            ) -> Option<AttachmentTarget> {
                self.state.texture_target(output)
            }
        }
    };
}

pub struct GraphicsPassBuilder<'a, 'b> {
    state: &'a mut PassBuilderState<'b>,
}

impl<'a, 'b> GraphicsPassBuilder<'a, 'b> {
    pub(crate) fn new(state: &'a mut PassBuilderState<'b>) -> Self {
        state.descriptor.kind = super::frame_graph::PassKind::Graphics;
        if !matches!(
            state.descriptor.details,
            super::frame_graph::PassDetails::Graphics(_)
        ) {
            state.descriptor.details =
                super::frame_graph::PassDetails::Graphics(Default::default());
        }
        Self { state }
    }

    pub fn set_sample_count(&mut self, sample_count: SampleCountPolicy) {
        self.state.descriptor.graphics_mut().sample_count = sample_count;
    }

    pub fn set_viewport_policy(&mut self, policy: ViewportPolicy) {
        self.state.descriptor.graphics_mut().viewport_policy = policy;
    }

    pub fn set_scissor_policy(&mut self, policy: ScissorPolicy) {
        self.state.descriptor.graphics_mut().scissor_policy = policy;
    }

    pub fn set_graphics_merge_policy(&mut self, policy: GraphicsPassMergePolicy) {
        self.state.descriptor.graphics_mut().merge_policy = policy;
    }

    pub fn write_color<Tag>(
        &mut self,
        output: &OutSlot<TextureResource, Tag>,
        ops: GraphicsColorAttachmentOps,
    ) {
        let Some(target) = self.state.texture_target_from_output(output) else {
            self.state
                .build_errors
                .push(FrameGraphError::MissingOutput("texture slot has no handle"));
            return;
        };
        if let Some(handle) = output.handle() {
            if let Some(desc) = self.state.textures.get(handle.0 as usize) {
                self.state.descriptor.graphics_mut().sample_count =
                    SampleCountPolicy::Fixed(desc.sample_count());
            }
        }
        self.state.write_color_attachment(
            output,
            GraphicsColorAttachmentDescriptor::from_ops(target, ops),
        );
    }

    pub fn write_surface_color(&mut self, ops: GraphicsColorAttachmentOps) {
        self.state
            .write_surface_color_attachment(GraphicsColorAttachmentDescriptor::from_ops(
                AttachmentTarget::Surface,
                ops,
            ));
    }

    pub fn read_depth(&mut self, target: AttachmentTarget) {
        self.state.read_depth(target);
    }

    pub fn read_output_depth(&mut self) {
        if let Some(target) = self.state.inferred_depth_stencil_target() {
            self.state.read_depth(target);
        }
    }

    pub fn write_depth(
        &mut self,
        target: AttachmentTarget,
        load_op: AttachmentLoadOp,
        clear_depth: Option<f32>,
    ) {
        self.state.write_depth(target, load_op, clear_depth);
    }

    pub fn write_output_depth(&mut self, load_op: AttachmentLoadOp, clear_depth: Option<f32>) {
        if let Some(target) = self.state.inferred_depth_stencil_target() {
            self.state.write_depth(target, load_op, clear_depth);
        }
    }

    pub fn read_stencil(&mut self, target: AttachmentTarget) {
        self.state.read_stencil(target);
    }

    pub fn read_output_stencil(&mut self) {
        if let Some(target) = self.state.inferred_depth_stencil_target() {
            self.state.read_stencil(target);
        }
    }

    pub fn write_stencil(
        &mut self,
        target: AttachmentTarget,
        load_op: AttachmentLoadOp,
        clear_stencil: Option<u32>,
    ) {
        self.state.write_stencil(target, load_op, clear_stencil);
    }

    pub fn write_output_stencil(&mut self, load_op: AttachmentLoadOp, clear_stencil: Option<u32>) {
        if let Some(target) = self.state.inferred_depth_stencil_target() {
            self.state.write_stencil(target, load_op, clear_stencil);
        }
    }
}

impl_shared_builder_api!(GraphicsPassBuilder);

pub struct ComputePassBuilder<'a, 'b> {
    state: &'a mut PassBuilderState<'b>,
}

impl<'a, 'b> ComputePassBuilder<'a, 'b> {
    pub(crate) fn new(state: &'a mut PassBuilderState<'b>) -> Self {
        state.descriptor.kind = super::frame_graph::PassKind::Compute;
        if !matches!(
            state.descriptor.details,
            super::frame_graph::PassDetails::Compute(_)
        ) {
            state.descriptor.details = super::frame_graph::PassDetails::Compute(Default::default());
        }
        Self { state }
    }
}

impl_shared_builder_api!(ComputePassBuilder);

pub struct TransferPassBuilder<'a, 'b> {
    state: &'a mut PassBuilderState<'b>,
}

impl<'a, 'b> TransferPassBuilder<'a, 'b> {
    pub(crate) fn new(state: &'a mut PassBuilderState<'b>) -> Self {
        state.descriptor.kind = super::frame_graph::PassKind::Transfer;
        if !matches!(
            state.descriptor.details,
            super::frame_graph::PassDetails::Transfer(_)
        ) {
            state.descriptor.details =
                super::frame_graph::PassDetails::Transfer(Default::default());
        }
        Self { state }
    }
}

impl_shared_builder_api!(TransferPassBuilder);
