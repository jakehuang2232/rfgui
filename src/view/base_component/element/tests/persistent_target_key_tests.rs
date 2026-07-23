use super::*;

#[test]
fn persistent_target_keys_are_unique_across_roles_and_full_u64_ids() {
    use std::collections::HashSet;

    let mut keys = HashSet::new();
    for node_id in [0, 1, u64::MAX] {
        for color in [
            transformed_layer_stable_key(node_id),
            isolation_layer_stable_key(node_id),
            scroll_host_layer_stable_key(node_id),
            scroll_content_layer_stable_key(node_id),
        ] {
            assert!(keys.insert(color));
            let depth = persistent_depth_stencil_stable_key(color)
                .expect("known color role should produce a depth key");
            assert!(keys.insert(depth));
        }
        assert!(keys.insert(PersistentTextureKey::Generic(node_id)));
    }
    assert_eq!(keys.len(), 27);
    assert!(persistent_depth_stencil_stable_key(PersistentTextureKey::Generic(u64::MAX)).is_none());
}

#[test]
fn root_effect_key_uses_the_full_generational_node_key() {
    let mut slots = slotmap::SlotMap::<NodeKey, ()>::with_key();
    let first = slots.insert(());
    let first_key = root_effect_stable_key(first);
    slots.remove(first);
    let replacement = slots.insert(());
    let replacement_key = root_effect_stable_key(replacement);

    assert_ne!(first, replacement, "fixture must reuse a bumped generation");
    assert_ne!(first_key, replacement_key);
    assert_eq!(
        first_key,
        PersistentTextureKey::retained(RetainedTextureRole::RootEffectColor, first.data().as_ffi(),)
    );
}

#[test]
fn full_viewport_persistent_target_uses_exact_physical_descriptor_and_pair() {
    let format = wgpu::TextureFormat::Rgba16Float;
    let mut ctx = UiBuildContext::new(641, 359, format, 2.75);
    let mut graph = FrameGraph::new();
    let color_key = PersistentTextureKey::retained(RetainedTextureRole::RootEffectColor, 0xC2A);
    let color = ctx.allocate_persistent_full_viewport_target(&mut graph, color_key);
    ctx.set_current_target(color);

    let color_desc = graph
        .texture_desc(color.handle().expect("root color handle"))
        .expect("root color descriptor");
    assert_eq!((color_desc.width(), color_desc.height()), (641, 359));
    assert_eq!(color_desc.origin(), (0, 0));
    assert_eq!(color_desc.format(), format);
    assert_eq!(color_desc.dimension(), wgpu::TextureDimension::D2);
    assert_eq!(color_desc.sample_count(), 1);

    let AttachmentTarget::Texture(depth_handle) =
        ctx.depth_stencil_target().expect("root depth target")
    else {
        panic!("root depth target must be texture-backed");
    };
    let depth_desc = graph
        .texture_desc(depth_handle)
        .expect("root depth descriptor");
    assert_eq!((depth_desc.width(), depth_desc.height()), (641, 359));
    assert_eq!(depth_desc.origin(), (0, 0));
    assert_eq!(
        depth_desc.format(),
        wgpu::TextureFormat::Depth24PlusStencil8
    );

    let declared = graph
        .declared_persistent_texture_keys()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(declared.len(), 2);
    assert!(declared.contains(&color_key));
    assert!(declared.contains(&color_key.depth_stencil().unwrap()));
}


