use super::super::super::{
    ArtifactSurfaceHostPlacementProjection, ResolvedClip, artifact_surface_terminal_clip,
    translate_artifact_surface_logical_clip,
};
use super::*;

#[test]
fn owner_scoped_host_placement_adds_only_the_host_change_to_the_snap_chain() {
    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xc3_e100, 0.0, 0.0, 1.0, 1.0)),
    );
    let child = crate::view::test_support::commit_child(
        &mut arena,
        root,
        Box::new(Element::new_with_id(0xc3_e101, 0.0, 0.0, 1.0, 1.0)),
    );
    let mut projection = ArtifactSurfaceHostPlacementProjection {
        owners: vec![
            crate::view::paint::OwnerSnapPoints {
                owner: child,
                parent: Some(root),
                points_bits: [Some([17.5_f32.to_bits(), 15.0_f32.to_bits()]), None],
            },
            crate::view::paint::OwnerSnapPoints {
                owner: root,
                parent: None,
                points_bits: [Some([5.25_f32.to_bits(), 5.0_f32.to_bits()]), None],
            },
        ]
        .into(),
        resolved: Default::default(),
    };
    let resolved = projection
        .resolve([0.0, 0.0], 1.0)
        .expect("canonical owner-scoped placement");
    // Recorded commands already carry the zero-host snap chain.
    for owner in [root, child] {
        assert_eq!(resolved.owner_paint_offset(owner), Some([0.0, 0.0]));
    }
    let warm = projection.clone().resolve([0.0, 0.0], 1.0).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &resolved.host_delta_bits,
        &warm.host_delta_bits
    ));
    let shifted = projection.resolve([2.0, 3.0], 1.0).unwrap();
    assert_eq!(shifted.owner_paint_offset(child), Some([2.0, 3.0]));
    assert!(!std::sync::Arc::ptr_eq(
        &warm.host_delta_bits,
        &shifted.host_delta_bits
    ));
    // A half-pixel host moves the root from 5.25 to 5.75, which snaps to 6
    // instead of 5. Snapping its own frame offset directly, the child would
    // stay at 18; it must inherit the root's correction instead.
    let fractional = projection.resolve([0.5, 0.0], 1.0).unwrap();
    assert_eq!(fractional.owner_paint_offset(root), Some([1.0, 0.0]));
    assert_eq!(
        fractional.owner_paint_offset(child),
        Some([1.0, 0.0]),
        "child snapping must inherit the parent's correction instead of recomputing directly from the frame offset"
    );
    assert!(projection.resolve([f32::NAN, 0.0], 1.0).is_err());
    // Replacing a same-id owner observation cannot inherit another allocation's proof.
    std::sync::Arc::make_mut(&mut projection.owners)[1].points_bits[0] =
        Some([5.5_f32.to_bits(), 5.0_f32.to_bits()]);
    let changed = projection.resolve([0.5, 0.0], 1.0).unwrap();
    assert_eq!(changed.owner_paint_offset(child), Some([0.0, 0.0]));
}

#[test]
fn effect_sampling_origin_tracks_finalized_destination_and_normalized_source() {
    let cases = [
        (
            [7.0, 11.0, 10.0, 8.0],
            1.0_f32,
            [7.0, 11.0, 10.0, 8.0],
            [7.0, 11.0],
        ),
        (
            [7.0, 11.0, 10.0, 8.0],
            1.0_f32,
            [10.0, 9.0, 10.0, 8.0],
            [10.0, 9.0],
        ),
        (
            [7.25, 11.5, 10.0, 8.0],
            2.0_f32,
            [10.75, 9.25, 10.0, 8.0],
            [21.0, 18.5],
        ),
    ];

    for (raw_source, scale, destination, expected_origin) in cases {
        let raster_origin = ArtifactSurfaceRasterOriginProjection::new(
            raw_source.map(f32::to_bits),
            scale.to_bits(),
        )
        .expect("canonical raster origin");
        let geometry = ArtifactSurfaceCompositeGeometryStamp::Effect {
            source_bounds_bits: raster_origin.normalized_source_bounds_bits,
            destination_bounds_bits: destination.map(f32::to_bits),
            opacity_bits: 0.625_f32.to_bits(),
            generation: 1,
            receiver_clip: None,
            resolved_receiver_clip: ArtifactSurfaceResolvedClip::Unclipped,
        };
        let PreparedArtifactSurfaceComposite::Layer {
            source_physical_origin,
            ..
        } = prepare_composite_geometry(geometry, raster_origin)
            .expect("canonical Effect composite")
        else {
            panic!("Effect geometry must remain a layer composite")
        };

        assert_eq!(
            source_physical_origin.map(f32::to_bits),
            expected_origin.map(f32::to_bits),
            "Effect sampling must follow finalized destination placement instead of the raw raster origin"
        );
    }
}

#[test]
fn scene_root_receiver_clip_moves_before_incoming_scissor_intersection() {
    let moved = translate_artifact_surface_logical_clip(
        ResolvedClip::Scissor([10, 20, 100, 80]),
        [4.0, -2.0],
    )
    .expect("finite owner placement");
    assert_eq!(moved, ResolvedClip::Scissor([14, 18, 100, 80]));
    assert_eq!(
        artifact_surface_terminal_clip(moved, &[], Some([0, 0, 50, 50])),
        ResolvedClip::Scissor([14, 18, 36, 32]),
        "the owner clip moves with its content while the frame scissor stays fixed"
    );
}

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
            ArtifactSurfaceCompositeGeometryStamp::Effect {
                source_bounds_bits,
                destination_bounds_bits,
                ..
            } => Some(
                node.raster_origin
                    .composite_source_physical_origin(source_bounds_bits, destination_bounds_bits)
                    .expect("sealed Effect sampling origin")
                    .map(f32::to_bits),
            ),
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
                source_bounds_bits,
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
                    node.raster_origin
                        .composite_source_physical_origin(
                            source_bounds_bits,
                            destination_bounds_bits,
                        )
                        .expect("sealed Effect sampling origin")
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
