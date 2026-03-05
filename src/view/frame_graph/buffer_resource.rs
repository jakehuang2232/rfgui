use super::slot::ResourceType;

#[derive(Clone, Copy)]
pub struct BufferDesc {
    pub size: u64,
    pub usage: wgpu::BufferUsages,
    pub label: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferHandle(pub(crate) u32);

pub struct BufferResource;

impl ResourceType for BufferResource {
    type Handle = BufferHandle;
}
