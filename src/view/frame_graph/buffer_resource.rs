use super::slot::ResourceType;

pub struct BufferDesc {
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferHandle(pub(crate) u32);

pub struct BufferResource;

impl ResourceType for BufferResource {
    type Handle = BufferHandle;
}
