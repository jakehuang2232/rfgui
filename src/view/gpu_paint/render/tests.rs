use super::*;

#[test]
fn source_allocation_capacity_is_bounded_and_stable_between_growth_boundaries() {
    for payload in [0, 1, 4, 8, 24, 48, 72, 96, 120, 64 * 1024 * 1024] {
        let capacity = vertex_buffer_capacity(payload);
        assert!(capacity >= payload as u64);
        assert!(capacity <= (payload as u64).max(4) * 2);
        assert!(capacity <= 64 * 1024 * 1024);
    }
    assert_eq!(vertex_buffer_capacity(72), vertex_buffer_capacity(96));
    assert_ne!(vertex_buffer_capacity(48), vertex_buffer_capacity(72));
}
