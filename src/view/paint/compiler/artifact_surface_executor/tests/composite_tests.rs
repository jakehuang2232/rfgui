use super::*;

#[test]
fn three_surface_roles_emit_only_their_sealed_typed_composites() {
    let prepared = prepared_co_located_surface_frame();
    assert_eq!(prepared.raster_plan().nodes().len(), 3);
    let effect_opacity_bits = prepared
        .raster_plan()
        .nodes()
        .iter()
        .find_map(|node| match node.geometry() {
            ArtifactSurfaceCompositeGeometryStamp::Effect { opacity_bits, .. } => {
                Some(opacity_bits)
            }
            ArtifactSurfaceCompositeGeometryStamp::Transform { .. }
            | ArtifactSurfaceCompositeGeometryStamp::ScrollContent { .. } => None,
        })
        .expect("co-located Effect geometry");
    assert_ne!(
        effect_opacity_bits,
        1.0_f32.to_bits(),
        "fixture must distinguish Effect opacity from ScrollContent opacity"
    );
    let mut expected_layer_source_origin_bits = prepared
        .raster_plan()
        .nodes()
        .iter()
        .filter_map(|node| match node.geometry() {
            ArtifactSurfaceCompositeGeometryStamp::Effect { .. } => {
                Some(node.raster_origin.physical_origin_f32().map(f32::to_bits))
            }
            ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
                source_bounds_bits,
                destination_bounds_bits,
                ..
            } => Some(
                node.raster_origin
                    .composite_source_physical_origin(source_bounds_bits, destination_bounds_bits)
                    .expect("sealed ScrollContent sampling origin")
                    .map(f32::to_bits),
            ),
            ArtifactSurfaceCompositeGeometryStamp::Transform { .. } => None,
        })
        .collect::<Vec<_>>();
    expected_layer_source_origin_bits.sort_unstable();
    for node in prepared.raster_plan().nodes() {
        let geometry = node.geometry();
        let typed = prepare_composite_geometry(geometry, node.raster_origin)
            .expect("sealed typed geometry");
        match geometry {
            ArtifactSurfaceCompositeGeometryStamp::Transform {
                source_bounds_bits,
                destination_bounds_bits,
                resolved_receiver_clip,
                ..
            } => {
                let PreparedArtifactSurfaceComposite::Transform {
                    params,
                    resolved_clip,
                } = typed
                else {
                    panic!("transform geometry must stay a transform composite")
                };
                assert_eq!(params.bounds.map(f32::to_bits), destination_bounds_bits);
                assert_eq!(
                    params
                        .uv_bounds
                        .expect("transform UV bounds")
                        .map(f32::to_bits),
                    source_bounds_bits
                );
                assert_eq!(resolved_clip, resolved_receiver_clip);
            }
            ArtifactSurfaceCompositeGeometryStamp::Effect {
                destination_bounds_bits,
                opacity_bits,
                resolved_receiver_clip,
                ..
            } => {
                let PreparedArtifactSurfaceComposite::Layer {
                    rect_pos,
                    rect_size,
                    opacity,
                    source_physical_origin,
                    resolved_clip,
                } = typed
                else {
                    panic!("effect geometry must stay a layer composite")
                };
                assert_eq!(
                    [rect_pos[0], rect_pos[1], rect_size[0], rect_size[1]].map(f32::to_bits),
                    destination_bounds_bits
                );
                assert_eq!(opacity.to_bits(), opacity_bits);
                assert_eq!(
                    source_physical_origin,
                    node.raster_origin.physical_origin_f32()
                );
                assert_eq!(resolved_clip, resolved_receiver_clip);
            }
            ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
                source_bounds_bits,
                destination_bounds_bits,
                resolved_receiver_clip,
                ..
            } => {
                let PreparedArtifactSurfaceComposite::Layer {
                    rect_pos,
                    rect_size,
                    opacity,
                    source_physical_origin,
                    resolved_clip,
                } = typed
                else {
                    panic!("scroll geometry must stay a layer composite")
                };
                assert_eq!(
                    [rect_pos[0], rect_pos[1], rect_size[0], rect_size[1]].map(f32::to_bits),
                    destination_bounds_bits
                );
                assert_eq!(opacity.to_bits(), 1.0_f32.to_bits());
                assert_eq!(
                    source_physical_origin,
                    node.raster_origin
                        .composite_source_physical_origin(
                            source_bounds_bits,
                            destination_bounds_bits,
                        )
                        .expect("sealed ScrollContent sampling origin")
                );
                assert_eq!(resolved_clip, resolved_receiver_clip);
            }
        }
    }
    let mut viewport = Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("composite owner");
    let mut graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        owner,
        prepared,
        &mut graph,
        execution_context(),
    )
    .expect("three-role emission");

    assert_eq!(
        graph.test_graphics_passes::<TextureCompositePass>().len(),
        1
    );
    let layers = graph.test_graphics_passes::<CompositeLayerPass>();
    assert_eq!(layers.len(), 2);
    let mut actual_layer_source_origin_bits = layers
        .iter()
        .map(|pass| {
            pass.test_source_physical_origin_bits()
                .expect("artifact layer composite owns one sealed source origin")
        })
        .collect::<Vec<_>>();
    actual_layer_source_origin_bits.sort_unstable();
    assert_eq!(
        actual_layer_source_origin_bits, expected_layer_source_origin_bits,
        "artifact Effect must retain its sealed raster origin while ScrollContent derives sampling from finalized placement and normalized source bounds"
    );
    let mut actual_opacity_bits = layers
        .iter()
        .map(|pass| pass.test_params().opacity.to_bits())
        .collect::<Vec<_>>();
    actual_opacity_bits.sort_unstable();
    let mut expected_opacity_bits = vec![effect_opacity_bits, 1.0_f32.to_bits()];
    expected_opacity_bits.sort_unstable();
    assert_eq!(actual_opacity_bits, expected_opacity_bits);
    assert!(layers.iter().all(|pass| {
        let params = pass.test_params();
        params.rect_size[0] > 0.0
            && params.rect_size[1] > 0.0
            && (0.0..=1.0).contains(&params.opacity)
    }));
}
