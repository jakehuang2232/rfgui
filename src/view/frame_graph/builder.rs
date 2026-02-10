use super::buffer_resource::{BufferDesc, BufferHandle, BufferResource};
use super::frame_graph::{FrameGraphError, ResourceHandle};
use super::slot::{InSlot, OutSlot};
use super::texture_resource::{TextureDesc, TextureHandle, TextureResource};

pub struct BuildContext<'a>{
    pub(crate) textures: &'a mut Vec<TextureDesc>,
    pub(crate) buffers: &'a mut Vec<BufferDesc>,
    pub(crate) reads: &'a mut Vec<ResourceHandle>,
    pub(crate) writes: &'a mut Vec<ResourceHandle>,
    pub(crate) build_errors: &'a mut Vec<FrameGraphError>,
}

impl<'a> BuildContext<'a> {
    pub fn create_texture<Tag>(&mut self, desc: TextureDesc) -> OutSlot<TextureResource, Tag> {
        let handle = TextureHandle(self.textures.len() as u32);
        self.textures.push(desc);
        self.writes.push(ResourceHandle::Texture(handle));
        OutSlot::with_handle(handle)
    }

    pub fn create_buffer<Tag>(&mut self, desc: BufferDesc) -> OutSlot<BufferResource, Tag> {
        let handle = BufferHandle(self.buffers.len() as u32);
        self.buffers.push(desc);
        self.writes.push(ResourceHandle::Buffer(handle));
        OutSlot::with_handle(handle)
    }

    pub fn read_texture<Tag>(
        &mut self,
        input: &mut InSlot<TextureResource, Tag>,
        source: &OutSlot<TextureResource, Tag>,
    ) {
        match source.handle {
            Some(handle) => {
                input.handle = Some(handle);
                self.reads.push(ResourceHandle::Texture(handle));
            }
            None => {
                self.build_errors.push(FrameGraphError::MissingInput("texture slot has no handle"));
            }
        }
    }

    pub fn read_buffer<Tag>(
        &mut self,
        input: &mut InSlot<BufferResource, Tag>,
        source: &OutSlot<BufferResource, Tag>,
    ) {
        match source.handle {
            Some(handle) => {
                input.handle = Some(handle);
                self.reads.push(ResourceHandle::Buffer(handle));
            }
            None => {
                self.build_errors.push(FrameGraphError::MissingInput("buffer slot has no handle"));
            }
        }
    }

    pub fn write_texture<Tag>(&mut self, output: &mut OutSlot<TextureResource, Tag>) {
        match output.handle {
            Some(handle) => {
                self.writes.push(ResourceHandle::Texture(handle));
            }
            None => {
                self.build_errors.push(FrameGraphError::MissingOutput("texture slot has no handle"));
            }
        }
    }

    pub fn write_buffer<Tag>(&mut self, output: &mut OutSlot<BufferResource, Tag>) {
        match output.handle {
            Some(handle) => {
                self.writes.push(ResourceHandle::Buffer(handle));
            }
            None => {
                self.build_errors.push(FrameGraphError::MissingOutput("buffer slot has no handle"));
            }
        }
    }
}
