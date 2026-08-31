use super::*;
use crate::view::frame_graph::RetainedTextureRole;
use crate::view::viewport::Viewport;

fn generic(key: u64) -> PersistentTextureKey {
    PersistentTextureKey::Generic(key)
}

fn compatibility_fixture() -> RenderTargetCompatibility {
    RenderTargetCompatibility {
        width: 37,
        height: 19,
        format: wgpu::TextureFormat::Rgba8Unorm,
        dimension: wgpu::TextureDimension::D2,
        sample_count: 4,
        label: "Root Effect".to_string(),
    }
}

#[test]
fn persistent_compatibility_query_is_read_only_and_rejects_missing_binding() {
    let pool = OffscreenRenderTargetPool::new();
    let desc = TextureDesc::new(
        37,
        19,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureDimension::D2,
    )
    .with_label("Root Effect");

    assert!(!pool.has_compatible_persistent(generic(7), &desc, 4));
    assert_eq!(pool.frame_epoch, 0);
    assert!(pool.persistent_bindings.is_empty());
}

#[test]
fn persistent_compatibility_rejects_every_recreate_field_mismatch() {
    let expected = compatibility_fixture();
    assert!(persistent_compatibility_matches(Some(&expected), &expected));
    assert!(!persistent_compatibility_matches(None, &expected));

    let mut cases = Vec::new();
    let mut width = expected.clone();
    width.width += 1;
    cases.push(width);
    let mut height = expected.clone();
    height.height += 1;
    cases.push(height);
    let mut format = expected.clone();
    format.format = wgpu::TextureFormat::Rgba16Float;
    cases.push(format);
    let mut dimension = expected.clone();
    dimension.dimension = wgpu::TextureDimension::D1;
    cases.push(dimension);
    let mut sample_count = expected.clone();
    sample_count.sample_count = 1;
    cases.push(sample_count);
    let mut label = expected.clone();
    label.label.push_str(" changed");
    cases.push(label);

    for actual in cases {
        assert!(!persistent_compatibility_matches(Some(&actual), &expected));
    }
}

#[test]
fn targeted_persistent_release_removes_color_depth_pair_only() {
    let mut pool = OffscreenRenderTargetPool::new();
    let color = PersistentTextureKey::retained(RetainedTextureRole::RootEffectColor, 9);
    let depth = color.depth_stencil().expect("root depth key");
    let unrelated = generic(99);
    for (key, entry_id) in [(color, 10), (depth, 11), (unrelated, 12)] {
        pool.persistent_bindings.insert(
            key,
            PersistentRenderTargetBinding {
                entry_id,
                last_used_epoch: 0,
            },
        );
    }

    assert!(pool.release_persistent_pair(color));
    assert!(!pool.persistent_bindings.contains_key(&color));
    assert!(!pool.persistent_bindings.contains_key(&depth));
    assert!(pool.persistent_bindings.contains_key(&unrelated));
    assert!(!pool.release_persistent_pair(color));
}

#[test]
fn logical_scissor_to_target_physical_preserves_fractional_scaled_coverage() {
    let mut viewport = Viewport::new();
    viewport.set_scale_factor(1.25);

    let physical =
        logical_scissor_to_target_physical(&viewport, [10, 20, 101, 51], (3, 7), (200, 200));

    assert_eq!(physical, Some([9, 18, 127, 64]));
}

#[test]
fn target_physical_scissor_bypasses_logical_projection_and_clamps_to_target() {
    let mut viewport = Viewport::new();
    viewport.set_scale_factor(2.0);

    let physical = resolve_graphics_pass_scissor_to_target_physical(
        &viewport,
        Some(GraphicsPassScissor::TargetPhysical([5, 7, 100, 100])),
        None,
        (100, 200),
        (80, 60),
    );

    assert_eq!(physical, Some([5, 7, 75, 53]));
}

#[test]
#[should_panic(expected = "target-physical scissor cannot be consumed as a logical scissor")]
fn logical_scissor_accessor_fails_closed_for_target_physical_input() {
    let context = GraphicsPassContext {
        scissor_rect: Some(GraphicsPassScissor::TargetPhysical([1, 2, 3, 4])),
        ..Default::default()
    };

    let _ = context.logical_scissor_rect();
}

#[test]
fn persistent_binding_expires_after_unused_frame_budget() {
    let mut pool = OffscreenRenderTargetPool::new();
    pool.persistent_bindings.insert(
        generic(7),
        PersistentRenderTargetBinding {
            entry_id: 11,
            last_used_epoch: 0,
        },
    );

    for _ in 0..OffscreenRenderTargetPool::EVICT_UNUSED_AFTER_FRAMES - 1 {
        pool.begin_frame();
    }
    assert!(pool.persistent_bindings.contains_key(&generic(7)));

    pool.begin_frame();
    assert!(!pool.persistent_bindings.contains_key(&generic(7)));
}

#[test]
fn persistent_binding_last_use_refreshes_expiration_budget() {
    let mut pool = OffscreenRenderTargetPool::new();
    pool.persistent_bindings.insert(
        generic(7),
        PersistentRenderTargetBinding {
            entry_id: 11,
            last_used_epoch: 0,
        },
    );

    for _ in 0..30 {
        pool.begin_frame();
    }
    pool.persistent_bindings
        .get_mut(&generic(7))
        .expect("binding should still be alive")
        .last_used_epoch = pool.frame_epoch;
    let epoch_before_observation = pool.frame_epoch;
    let last_used_before_observation = pool.persistent_bindings[&generic(7)].last_used_epoch;
    let _ = pool.persistent_resident_observations();
    assert_eq!(pool.frame_epoch, epoch_before_observation);
    assert_eq!(
        pool.persistent_bindings[&generic(7)].last_used_epoch,
        last_used_before_observation,
        "readonly resident observation must not refresh persistent lifetime"
    );
    for _ in 0..OffscreenRenderTargetPool::EVICT_UNUSED_AFTER_FRAMES - 1 {
        pool.begin_frame();
    }

    assert!(pool.persistent_bindings.contains_key(&generic(7)));
}
