use super::texture_composite_resource_descriptor_matches;

#[test]
fn canonical_scope_is_checked_even_when_cache_lookup_key_collides() {
    assert!(texture_composite_resource_descriptor_matches(
        1,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        4,
        1,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        4,
    ));
    assert!(!texture_composite_resource_descriptor_matches(
        1,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        4,
        2,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        4,
    ));
}
