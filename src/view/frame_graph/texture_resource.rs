use super::slot::ResourceType;

pub struct TextureDesc{
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    dimension: wgpu::TextureDimension,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub(crate) u32);

#[derive(Clone, Copy)]
pub struct TextureResource;

impl ResourceType for TextureResource {
    type Handle = TextureHandle;
}

impl TextureDesc {
    pub fn new(
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        dimension: wgpu::TextureDimension,
    ) -> Self {
        Self { width, height, format, dimension }
    }
}
