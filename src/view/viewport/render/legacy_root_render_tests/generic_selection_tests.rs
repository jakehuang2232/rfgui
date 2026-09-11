use super::*;

#[test]
fn generic_primary_replaces_bridge_selection_for_complete_recordings() {
    let ctx = UiBuildContext::new(800, 700, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let cases = [
        prepared_transform_scroll_scene(glam::Mat4::from_translation(glam::Vec3::new(
            7.0, 5.0, 0.0,
        ))),
        prepared_same_owner_transform_scroll_scene(),
        crate::view::paint::native_scroll_forest_plan_fixture(),
    ];
    for (arena, roots, properties, generations) in cases {
        assert_generic_primary(&arena, &roots, &properties, &generations, &ctx);
    }
}

#[test]
fn generic_primary_accepts_heterogeneous_roots_but_rejects_invalid_snapshot_generations() {
    let (mut arena, mut roots, _, _) = crate::view::paint::native_scroll_forest_plan_fixture();
    let plain = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xc3_ff, 0.0, 0.0, 40.0, 40.0)),
    );
    roots.push(plain);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let ctx = UiBuildContext::new(800, 700, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    // This was a grammar restriction in the old forest planner, not malformed
    // recording. A plain root coexists with detached surfaces in the new path.
    assert_generic_primary(&arena, &roots, &properties, &generations, &ctx);
    for clip in [true, false] {
        let (mut invalid, _) = synced_paint_state(&arena, &roots);
        if clip {
            invalid.clips.values_mut().next().unwrap().generation = 0;
        } else {
            invalid.scrolls.values_mut().next().unwrap().generation = 0;
        }
        let selected =
            select_retained_auto_authority(&arena, &roots, &invalid, &generations, &ctx, true);
        assert!(
            matches!(selected, AutoAuthorityDecision::Legacy { .. }),
            "zero generation must not gain authority: {:?}",
            auto_authority_trace(&selected).rejections
        );
    }
}
