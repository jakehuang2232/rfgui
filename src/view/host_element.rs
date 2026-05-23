//! Host-builder shim for built-in and downstream custom host tags.
//!
//! Host-builder dispatch has replaced the legacy registry /
//! `host_factory` path. A [`HostBuilder`] knows how to build its own
//! [`ElementDescriptor`] (children + side slots) given a [`BuildCtx`];
//! the engine-core descriptor stores a fn pointer
//! ([`ErasedHostBuilder`]) that the renderer dispatches through without
//! enumerating host types. Some built-in builder bodies still delegate
//! to `renderer_adapter`; moving those bodies out of the adapter is
//! pending cleanup.

use std::any::{Any, TypeId};

use crate::ui::{ErasedHostBuilder, GlobalNodePath, RsxElementNode, RsxNode, RsxTagDescriptor};
use crate::view::renderer_adapter::{ElementDescriptor, InheritedTextStyle};

/// Owned conversion context handed to a [`HostBuilder`]. `'static` so
/// the engine-core `&dyn Any` downcast works (borrowed references can't
/// carry non-`'static` lifetimes through `Any`).
pub struct BuildCtx {
    pub global_path: Option<GlobalNodePath>,
    pub inherited: InheritedTextStyle,
}

/// Implement on a tag type to provide a compile-time descriptor build
/// path. Combine with
/// `impl RsxTag for T { const HOST_BUILDER = Some(erased_host_builder::<Self>); }`,
/// or build a bare descriptor via [`host_builder_descriptor`] /
/// [`host_builder_node`] when full `RsxTag` boilerplate isn't needed.
pub trait HostBuilder: 'static {
    fn build_descriptor(
        node: &RsxElementNode,
        path: &[u64],
        ctx: &BuildCtx,
    ) -> Result<ElementDescriptor, String>;
}

/// Sized wrapper that carries an [`ElementDescriptor`] through
/// `Box<dyn Any>` so the engine-core descriptor (which cannot name
/// `ElementDescriptor`) can hold a type-erased builder pointer. View
/// layer downcasts to this wrapper at conversion time.
pub struct HostElementDescBox(pub ElementDescriptor);

/// Generic builder shim. Take the address of `erased_host_builder::<T>`
/// and store as an [`ErasedHostBuilder`] in `T::HOST_BUILDER`.
pub fn erased_host_builder<T: HostBuilder>(
    node: &RsxElementNode,
    path: &[u64],
    ctx: &dyn Any,
) -> Result<Box<dyn Any>, String> {
    let ctx = ctx
        .downcast_ref::<BuildCtx>()
        .ok_or_else(|| "host builder ctx must be view::BuildCtx".to_string())?;
    let desc = T::build_descriptor(node, path, ctx)?;
    Ok(Box::new(HostElementDescBox(desc)) as Box<dyn Any>)
}

/// Concrete `ErasedHostBuilder` constant, useful when assigning to
/// `RsxTag::HOST_BUILDER` in a const context (avoids the
/// `Some(erased_host_builder::<Self>)` fn-coercion gymnastics).
pub const fn host_builder_of<T: HostBuilder>() -> ErasedHostBuilder {
    erased_host_builder::<T>
}

/// Build an [`RsxTagDescriptor`] for a custom `HostBuilder` type
/// without requiring an `impl RsxTag for T`. The descriptor carries the
/// type id, the (compiler-derived) type name, and the host-builder
/// pointer.
pub fn host_builder_descriptor<T: HostBuilder>() -> RsxTagDescriptor {
    RsxTagDescriptor {
        type_id: TypeId::of::<T>(),
        type_name: std::any::type_name::<T>(),
        host_builder: Some(erased_host_builder::<T>),
    }
}

/// Build an `RsxNode::Element` for a custom `HostBuilder` type. The
/// `tag` string is retained for diagnostics and back-compat with
/// consumers that still inspect [`RsxElementNode::tag`].
pub fn host_builder_node<T: HostBuilder>(tag: &'static str) -> RsxNode {
    RsxNode::tagged(tag, host_builder_descriptor::<T>())
}
