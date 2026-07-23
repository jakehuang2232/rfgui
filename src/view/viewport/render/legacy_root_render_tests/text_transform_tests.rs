use super::*;

#[test]
fn retained_auto_accepts_sampled_element_layout_transition_geometry() {
    let (arena, roots) = prepared_safe_leaf();
    let root = roots[0];
    {
        let mut node = arena.get_mut(root).unwrap();
        let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        element.set_layout_transition_width(44.0);
        element.set_layout_transition_height(26.0);
        element.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    let snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    assert_eq!(snapshot.width.to_bits(), 44.0_f32.to_bits());
    assert_eq!(snapshot.height.to_bits(), 26.0_f32.to_bits());

    assert_native_host_retained_closure(
        "Element sampled layout transition",
        &arena,
        &roots,
        &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
    );
}

#[test]
fn retained_auto_accepts_sampled_inline_span_layout_transition_package() {
    let (arena, roots) = prepared_sampled_inline_span_layout_transition();
    assert_native_host_retained_closure(
        "inline-owned Element sampled layout transition",
        &arena,
        &roots,
        &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
    );
}

#[test]
fn retained_auto_direct_text_transform_selects_seals_emits_and_compiles() {
    let cases = [
        (
            "translation",
            Transform::new([Translate::xy(Length::px(7.0), Length::px(5.0))]),
        ),
        ("scale", Transform::new([Scale::uniform(1.25)])),
        ("rotation", Transform::new([Rotate::deg(12.0)])),
    ];
    for (index, (label, transform)) in cases.into_iter().enumerate() {
        let (arena, roots, _) = prepared_native_text_transform(transform.clone(), false, false);
        assert_native_property_scene_authority(
            &format!("root Text {label}"),
            &arena,
            &roots,
            index == 0,
        );

        let (arena, roots, _) = prepared_native_text_transform(transform, true, false);
        assert_native_property_scene_authority(
            &format!("nested Text {label}"),
            &arena,
            &roots,
            index == 1,
        );
    }
}

#[test]
fn retained_auto_text_transform_coexists_with_sampled_layout_transition() {
    let (arena, roots, _) = prepared_native_text_transform(
        Transform::new([Translate::x(Length::px(6.0))]),
        true,
        true,
    );
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let decision = auto_decision(&arena, &roots, &ctx);
    assert!(
        !matches!(decision, AutoAuthorityDecision::Legacy { .. }),
        "sampled parent + Text transform rejection: {:?}",
        auto_authority_trace(&decision)
            .rejections
            .iter()
            .map(AutoAuthorityRejection::debug_label)
            .collect::<Vec<_>>()
    );
    assert_native_property_scene_authority(
        "sampled parent with direct Text transform",
        &arena,
        &roots,
        true,
    );
}

#[test]
fn retained_auto_text_transform_nonfinite_and_topology_drift_fail_closed() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let nonfinite = Transform::new([TransformEntry::from_matrix([
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        f32::NAN,
        0.0,
        0.0,
        1.0,
    ])]);
    let (arena, roots, _) = prepared_native_text_transform(nonfinite, false, false);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));

    let (mut arena, roots, child) =
        prepared_native_text_transform(Transform::new([Scale::uniform(1.25)]), true, false);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    arena.set_parent(child, None);
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));
}

#[test]
fn transition_relayout_keeps_resource_generation_frozen_until_next_frame_layout() {
    let pixels: Arc<[u8]> = Arc::from([10_u8, 20, 30, 255]);
    let source = ImageSource::Rgba {
        width: 1,
        height: 1,
        pixels: pixels.clone(),
    };
    let handle = image_resource::acquire_image_resource(&source);
    let asset_id = handle.asset_id();
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(Image::new_with_id(0x4d34, source)));
    let mut viewport = Viewport::new();
    viewport.logical_width = 100.0;
    viewport.logical_height = 100.0;
    viewport.scene.node_arena = arena;
    viewport.scene.ui_root_keys = vec![root];

    viewport.run_layout_pass();
    assert_eq!(
        viewport
            .scene
            .node_arena
            .get(root)
            .unwrap()
            .element
            .measured_size(),
        (1.0, 1.0)
    );

    image_resource::replace_ready_image_for_test(
        asset_id,
        2,
        1,
        Arc::from([40_u8, 50, 60, 255, 70, 80, 90, 255]),
    );
    viewport.run_relayout_pass();
    assert_eq!(
        viewport
            .scene
            .node_arena
            .get(root)
            .unwrap()
            .element
            .measured_size(),
        (1.0, 1.0),
        "the second layout pass in one frame must keep the first frozen snapshot"
    );

    viewport.run_layout_pass();
    assert_eq!(
        viewport
            .scene
            .node_arena
            .get(root)
            .unwrap()
            .element
            .measured_size(),
        (2.0, 1.0),
        "the next frame's first layout pass must refresh the resource snapshot"
    );
}
