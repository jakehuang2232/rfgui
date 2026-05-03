//! Host-element trait for user-authored custom `ElementTrait` types.
//!
//! Pairs with [`crate::ui::RsxTag::HOST_FACTORY`] to let any tag carry
//! its own compile-time factory pointer. Eliminates the runtime
//! `register_element_factory` registry — the descriptor itself knows how
//! to build its `Box<dyn ElementTrait>`.

use std::any::{Any, TypeId};

use crate::ui::{ErasedHostFactory, RsxElementNode, RsxNode, RsxTagDescriptor};
use crate::view::base_component::ElementTrait;

/// Implement on a tag type to provide a compile-time host-element build
/// path. Combine with `impl RsxTag for T { const HOST_FACTORY = Some(erased_host_factory::<Self>); }`.
pub trait HostElement: 'static {
    fn build(node: &RsxElementNode, path: &[u64]) -> Result<Box<dyn ElementTrait>, String>;
}

/// Sized wrapper that carries a `Box<dyn ElementTrait>` through `Box<dyn Any>`
/// so the engine-core descriptor (which cannot name `ElementTrait`) can hold a
/// type-erased factory pointer. View-layer downcasts to this wrapper at
/// conversion time.
pub struct HostElementBox(pub Box<dyn ElementTrait>);

/// Generic factory shim. Take the address of `erased_host_factory::<T>`
/// and store as an [`ErasedHostFactory`] in `T::HOST_FACTORY`.
pub fn erased_host_factory<T: HostElement>(
    node: &RsxElementNode,
    path: &[u64],
) -> Result<Box<dyn Any>, String> {
    T::build(node, path).map(|b| Box::new(HostElementBox(b)) as Box<dyn Any>)
}

/// Concrete `ErasedHostFactory` constant, useful when assigning to
/// `RsxTag::HOST_FACTORY` in a const context (avoids the
/// `Some(erased_host_factory::<Self>)` fn-coercion gymnastics).
pub const fn host_factory_of<T: HostElement>() -> ErasedHostFactory {
    erased_host_factory::<T>
}

/// Build an [`RsxTagDescriptor`] for a custom host-element type without
/// requiring an `impl RsxTag for T`. The descriptor carries the type id,
/// the (compiler-derived) type name, and the host-element factory pointer.
pub fn host_descriptor<T: HostElement>() -> RsxTagDescriptor {
    RsxTagDescriptor {
        type_id: TypeId::of::<T>(),
        type_name: std::any::type_name::<T>(),
        host_factory: Some(erased_host_factory::<T>),
    }
}

/// Build an `RsxNode::Element` for a custom host-element type. Equivalent
/// to `RsxNode::tagged(tag, host_descriptor::<T>())`. The `tag` string is
/// retained for diagnostics and back-compat with consumers that still
/// inspect [`RsxElementNode::tag`].
pub fn host_node<T: HostElement>(tag: &'static str) -> RsxNode {
    RsxNode::tagged(tag, host_descriptor::<T>())
}
