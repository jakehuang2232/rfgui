use super::*;

#[test]
fn nested_scroll_emits_two_surfaces() {
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let emission = crate::view::viewport::emit_retained_auto_artifact_surface_for_test(
        &mut viewport,
        &arena,
        &roots,
        &properties,
        &generations,
        &mut graph,
        &ctx,
    )
    .expect("nested scroll must emit");
    assert_eq!(emission.surface_count, 2);
    assert_eq!(emission.aggregate_texture_bytes, 1_440_000);
    assert_eq!(
        emission.actions,
        [
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
        ]
    );
    assert_eq!(graph.declared_persistent_texture_keys().count(), 2);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        2,
        "two cold Artifact surfaces must each clear once"
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .len(),
        2,
        "inner and outer ScrollContent surfaces each composite once"
    );
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(emission.frame_owner), true)
    );
}
