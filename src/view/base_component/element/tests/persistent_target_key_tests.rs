use super::*;

#[test]
fn persistent_target_keys_are_unique_across_roles_and_full_u64_ids() {
    use std::collections::HashSet;

    let mut keys = HashSet::new();
    for node_id in [0, 1, u64::MAX] {
        for color in [
            transformed_layer_stable_key(node_id),
            isolation_layer_stable_key(node_id),
            scroll_content_layer_stable_key(node_id),
        ] {
            assert!(keys.insert(color));
            let depth = color
                .depth_stencil()
                .expect("known color role should produce a depth key");
            assert!(keys.insert(depth));
        }
        assert!(keys.insert(PersistentTextureKey::Generic(node_id)));
    }
    assert_eq!(keys.len(), 21);
    assert!(
        PersistentTextureKey::Generic(u64::MAX)
            .depth_stencil()
            .is_none()
    );
}
