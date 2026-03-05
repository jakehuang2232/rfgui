use super::slot::{InSlot, OutSlot, ResourceType};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DepHandle(pub(crate) u32);

#[derive(Clone, Copy, Debug)]
pub struct DepResource;

impl ResourceType for DepResource {
    type Handle = DepHandle;
}

#[derive(Clone, Copy, Debug)]
pub struct DepTag;

pub type DepIn = InSlot<DepResource, DepTag>;
pub type DepOut = OutSlot<DepResource, DepTag>;
