use super::*;

#[test]
fn topology_cache_rejects_equal_hash_with_different_typed_key_mapping() {
    let desc = test_texture_desc();
    let mut cached_graph = FrameGraph::new();
    let _ = cached_graph.declare_persistent_texture_internal::<()>(
        desc.clone(),
        PersistentTextureKey::retained(RetainedTextureRole::TransformedColor, u64::MAX),
    );
    let mut current_graph = FrameGraph::new();
    let _ = current_graph.declare_persistent_texture_internal::<()>(
        desc,
        PersistentTextureKey::retained(RetainedTextureRole::IsolationColor, u64::MAX),
    );

    // Inject the same fast hash to model an actual hash collision. Full
    // canonical equality must remain the correctness boundary.
    let cached = TopologyCacheKey {
        hash: 7,
        signature: cached_graph.topology_signature(),
    };
    let current = TopologyCacheKey {
        hash: 7,
        signature: current_graph.topology_signature(),
    };
    assert!(!topology_cache_matches(&cached, &current));
}

#[test]
fn topology_cache_accepts_equal_hash_and_equal_canonical_signature() {
    let mut graph = FrameGraph::new();
    let _ = graph.declare_persistent_texture_internal::<()>(
        test_texture_desc(),
        PersistentTextureKey::retained(RetainedTextureRole::TransformedColor, u64::MAX),
    );
    let cached = TopologyCacheKey {
        hash: 11,
        signature: graph.topology_signature(),
    };
    let current = cached.clone();

    assert!(topology_cache_matches(&cached, &current));
}
