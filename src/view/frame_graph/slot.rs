use std::{fmt::Debug, hash::Hash, marker::PhantomData};

pub trait ResourceType {
    type Handle: Copy + Debug + PartialEq + Eq + Hash;
}
#[derive(Clone, Copy, Debug)]
pub struct OutSlot<R: ResourceType, Tag> {
    pub(crate) handle: Option<R::Handle>,
    _pd: PhantomData<(R, Tag)>,
}

#[derive(Clone, Copy, Debug)]
pub struct InSlot<R: ResourceType, Tag> {
    pub(crate) handle: Option<R::Handle>,
    _pd: PhantomData<(R, Tag)>,
}

impl<R: ResourceType, Tag> Default for OutSlot<R, Tag> {
    fn default() -> Self {
        Self { handle: None, _pd: PhantomData }
    }
}

impl<R: ResourceType, Tag> Default for InSlot<R, Tag> {
    fn default() -> Self {
        Self { handle: None, _pd: PhantomData }
    }
}

impl<R: ResourceType, Tag> OutSlot<R, Tag> {
    pub fn handle(&self) -> Option<R::Handle> {
        self.handle
    }

    pub(crate) fn with_handle(handle: R::Handle) -> Self {
        Self { handle: Some(handle), _pd: PhantomData }
    }
}

impl<R: ResourceType, Tag> InSlot<R, Tag> {
    pub fn handle(&self) -> Option<R::Handle> {
        self.handle
    }

    pub(crate) fn with_handle(handle: R::Handle) -> Self {
        Self { handle: Some(handle), _pd: PhantomData }
    }
}
