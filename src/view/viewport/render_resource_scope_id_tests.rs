use super::allocate_render_resource_scope_id;
use std::sync::atomic::AtomicU64;

#[test]
fn allocator_is_non_zero_and_monotonic() {
    let counter = AtomicU64::new(1);
    assert_eq!(allocate_render_resource_scope_id(&counter), 1);
    assert_eq!(allocate_render_resource_scope_id(&counter), 2);
}

#[test]
#[should_panic(expected = "render resource scope allocator emitted zero")]
fn allocator_rejects_zero() {
    let counter = AtomicU64::new(0);
    let _ = allocate_render_resource_scope_id(&counter);
}

#[test]
#[should_panic(expected = "render resource scope ID space exhausted")]
fn allocator_fails_closed_at_exhaustion() {
    let counter = AtomicU64::new(u64::MAX);
    let _ = allocate_render_resource_scope_id(&counter);
}
