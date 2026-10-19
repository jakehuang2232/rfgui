use super::*;
use std::rc::Rc;

/// Autoref dispatch is resolved at the macro call site, where concrete field
/// types (or the declared generic bounds) are known. No extra props bounds.
#[doc(hidden)]
pub struct MemoCompare<'a, T>(pub &'a T);

#[doc(hidden)]
pub trait MemoCompareValue<T> {
    fn memo_eq(self, other: &T) -> bool;
}
impl<T: PartialEq> MemoCompareValue<T> for &&MemoCompare<'_, T> {
    fn memo_eq(self, other: &T) -> bool {
        self.0 == other
    }
}
impl<T> MemoCompareValue<T> for &MemoCompare<'_, T> {
    fn memo_eq(self, _other: &T) -> bool {
        false
    }
}

#[derive(Clone)]
struct MemoInput(Rc<ComponentNodeInner>);
impl PartialEq for MemoInput {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.0, &other.0);
        if a.type_id != b.type_id
            || !std::ptr::eq(a.vtable, b.vtable)
            || a.identity != b.identity
            || a.key != b.key
            || a.children.len() != b.children.len()
        {
            return false;
        }
        // Children are independent inputs. Do not recursively compare a new
        // lazy tree: unchanged shared children or equal text are sufficient.
        if !a.children.iter().zip(&b.children).all(|(a, b)| {
            RsxNode::ptr_eq(a, b)
                || matches!((a, b), (RsxNode::Text(a), RsxNode::Text(b)) if a == b)
        }) {
            return false;
        }
        // Both pointers have the same concrete type and vtable. Owners stay
        // alive for the entire comparison; neither pointer is consumed here.
        a.vtable
            .props_eq
            .is_some_and(|eq| unsafe { eq(a.props, b.props) })
    }
}

pub(super) fn render_deferred(inner: Rc<ComponentNodeInner>) -> RsxNode {
    with_component_key(inner.key, || {
        if inner.vtable.props_eq.is_some() {
            crate::ui::state::render_memoized_component_by_type_id(
                inner.type_id,
                MemoInput(inner),
                |input| render_resolved(input.0.clone()),
            )
        } else {
            // An explicitly volatile descendant must not be hidden by an
            // otherwise reusable ancestor's resolved output cache.
            crate::ui::state::record_volatile_render();
            crate::ui::render_component_by_type_id(inner.type_id, || render_resolved(inner))
        }
    })
}

fn render_resolved(mut inner: Rc<ComponentNodeInner>) -> RsxNode {
    crate::ui::work_profile::count(|p| p.component_renders += 1);
    // The node keeps its props and render borrows them, so a node the memo
    // cache shares renders without a copy. Children move unless shared.
    let children = match Rc::get_mut(&mut inner) {
        Some(unique) => std::mem::take(&mut unique.children),
        None => inner.children.clone(),
    };
    // Safety: the vtable's render shim reads `props` as the props type it was
    // emitted for, and `inner` keeps them alive, including on unwind.
    let rendered = unsafe { (inner.vtable.render)(inner.props, children) };
    let mut walked = unwrap_components(rendered);
    walked.set_identity(inner.identity);
    if let RsxNode::Element(el) = &mut walked {
        let inner_builder = el.tag_descriptor.and_then(|d| d.host_builder);
        Rc::make_mut(el).tag_descriptor = Some(RsxTagDescriptor {
            type_id: inner.type_id,
            type_name: inner.identity.invocation_type,
            host_builder: inner_builder,
        });
    }
    walked
}

#[cfg(test)]
mod tests;
