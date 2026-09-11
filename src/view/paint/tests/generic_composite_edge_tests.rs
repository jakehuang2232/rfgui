use super::*;
use crate::view::paint::composite_edge::{
    emit_paint_composite_edges, paint_composite_edge_opaque_delta,
};
use crate::view::viewport::Viewport;

fn synthetic_generic_resident_stamp(topology_revision: u64) -> RetainedSurfaceRasterStamp {
    let mut arena = new_test_arena();
    let mut element = Element::new_with_id(0xa2_4300, 0.0, 0.0, 64.0, 32.0);
    element.set_background_color_value(Color::rgb(32, 64, 96));
    element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
        2.0, 0.0, 0.0,
    ))));
    let root = commit_element(&mut arena, Box::new(element));
    let (properties, generations) = sync_identity(&arena, &[root]);
    let FrameArtifactRecordOutcome::Artifact { mut artifact, .. } =
        record_surface_dag_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap()
    else {
        panic!("synthetic complete recording")
    };
    for chunk in &mut artifact.chunks {
        chunk.content_revision.topology_revision = topology_revision;
    }
    let context = ArtifactSurfaceRasterContext::new(
        1.0,
        wgpu::TextureFormat::Bgra8Unorm,
        [0.0, 0.0],
        None,
        8192,
        u64::MAX,
    )
    .unwrap();
    let plan = prepare_artifact_surface_raster_plan(artifact, context).unwrap();
    let frame = seal_prepared_artifact_surface_frame(plan).unwrap();
    assert!(frame.residents().is_canonical());
    frame.residents().ordered_entries()[0].stamp().clone()
}

fn synthetic_composite_edge(
    owner: NodeKey,
    role: PaintChunkRole,
    position: [f32; 2],
) -> PaintCompositeEdge {
    let size = match role {
        PaintChunkRole::Caret => [1.0, 18.0],
        PaintChunkRole::TextDecoration => [20.0, 1.0],
        _ => panic!("synthetic fixture accepts only post-composite rect roles"),
    };
    PaintCompositeEdge::new_draw_rect(
        PaintChunkId {
            owner,
            scope: PaintPropertyScope::Contents,
            phase: PaintNodePhase::AfterChildren,
            slot: 1,
            role,
        },
        owner,
        Rect {
            x: position[0],
            y: position[1],
            width: size[0],
            height: size[1],
        },
        PropertyTreeState::default(),
        None,
        DrawRectOp {
            params: RectPassParams {
                position,
                size,
                fill_color: [0.1, 0.2, 0.3, 1.0],
                opacity: 1.0,
                ..RectPassParams::default()
            },
            mode: RectRenderMode::FillOnly,
        },
    )
    .expect("synthetic generic composite edge must be canonical")
}

#[test]
fn generic_preedit_edge_survives_absent_caret_edge() {
    let (arena, roots, text_area_root, _, _) =
        prepared_projection_text_area_preedit_tree(8, "中", Some((0, "中".len())));
    {
        let mut node = arena.get_mut(text_area_root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.caret_visible = false;
    }
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert!(
        artifact
            .chunks
            .iter()
            .all(|chunk| chunk.id.role != PaintChunkRole::Caret),
        "hidden caret must not create a composite edge",
    );
    let underline_chunks = artifact
        .chunks
        .iter()
        .filter(|chunk| {
            chunk.owner == text_area_root
                && chunk.id.phase == PaintNodePhase::AfterChildren
                && chunk.id.role == PaintChunkRole::TextDecoration
        })
        .collect::<Vec<_>>();
    let [underline_chunk] = underline_chunks.as_slice() else {
        panic!("preedit must create exactly one independent underline edge")
    };
    let underline = PaintCompositeEdge::from_artifact_chunk(&artifact, underline_chunk)
        .expect("generic underline edge must be self-canonical");
    let opaque_delta = paint_composite_edge_opaque_delta(std::slice::from_ref(&underline))
        .expect("generic underline edge must have a sealed opaque delta");

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    emit_paint_composite_edges(&[underline], &mut graph, &mut ctx);

    assert_eq!(ctx.opaque_rect_order(), opaque_delta);
    let transparent = graph
        .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>()
        .len();
    let opaque = graph
        .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::OpaqueRectPass>()
        .len();
    assert_eq!(transparent + opaque, 1);
}

#[test]
fn generic_composite_edges_do_not_enter_resident_raster_identity() {
    fn edge_for(edges: &[PaintCompositeEdge], role: PaintChunkRole) -> Option<&PaintCompositeEdge> {
        edges.iter().find(|edge| edge.id.role == role)
    }

    let mut arena = new_test_arena();
    let owner = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xa2_4302, 0.0, 0.0, 64.0, 32.0)),
    );
    let caret = synthetic_composite_edge(owner, PaintChunkRole::Caret, [8.0, 4.0]);
    let underline = synthetic_composite_edge(owner, PaintChunkRole::TextDecoration, [10.0, 22.0]);
    let shifted_underline =
        synthetic_composite_edge(owner, PaintChunkRole::TextDecoration, [11.0, 22.0]);
    let baseline_edges = vec![caret.clone(), underline.clone()];
    let hidden_caret_edges = vec![underline];
    let shifted_underline_edges = vec![caret, shifted_underline];
    let baseline_stamp = synthetic_generic_resident_stamp(1);
    let baseline_transition = PaintArtifactSpaceTransition::from_bits(
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        1,
    )
    .unwrap();
    let scrolled_transition = PaintArtifactSpaceTransition::from_bits(
        [0.0_f32.to_bits(), 20.0_f32.to_bits()],
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        1,
    )
    .unwrap();

    assert!(edge_for(&baseline_edges, PaintChunkRole::Caret).is_some());
    assert!(edge_for(&hidden_caret_edges, PaintChunkRole::Caret).is_none());
    assert_eq!(
        edge_for(&baseline_edges, PaintChunkRole::TextDecoration),
        edge_for(&hidden_caret_edges, PaintChunkRole::TextDecoration),
        "caret presence cannot gate the independent underline edge",
    );
    assert_ne!(
        edge_for(&baseline_edges, PaintChunkRole::TextDecoration),
        edge_for(&shifted_underline_edges, PaintChunkRole::TextDecoration),
        "fixture must change the generic underline edge payload",
    );
    assert_ne!(
        scrolled_transition, baseline_transition,
        "fixture must change artifact-space composite geometry",
    );

    let unchanged_stamp = synthetic_generic_resident_stamp(1);
    assert_eq!(
        unchanged_stamp, baseline_stamp,
        "caret presence and underline geometry have no field in resident raster identity",
    );
    let content_changed_stamp = synthetic_generic_resident_stamp(2);
    assert_eq!(
        content_changed_stamp.identity.resident_key(),
        baseline_stamp.identity.resident_key(),
    );
    assert_ne!(
        content_changed_stamp, baseline_stamp,
        "resident content still participates in raster identity",
    );
}

#[test]
fn generic_composite_edge_tampers_are_typed_owner_attributed_and_mutation_free() {
    let mut arena = new_test_arena();
    let owner = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xa2_4310, 0.0, 0.0, 64.0, 32.0)),
    );
    let other_owner = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xa2_4311, 0.0, 0.0, 64.0, 32.0)),
    );
    let edge = synthetic_composite_edge(owner, PaintChunkRole::Caret, [8.0, 4.0]);
    let mut viewport = Viewport::new();
    let (_, residents) =
        crate::view::paint::prepared_depth_four_surface_frame_for_test().into_parts();
    let residents = viewport
        .prepare_artifact_surface_pool_emission_for_forced_test(residents)
        .unwrap()
        .into_canonical_residents();
    let original_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    assert!(viewport.stage_artifact_surface_resident_set(original_owner, residents));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(original_owner), true));
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();

    let mut wrong_owner = edge.clone();
    wrong_owner.id.owner = other_owner;
    assert_eq!(
        wrong_owner.validate(),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::CompositeOwner,
        })
    );

    let mut bounds = edge.clone();
    bounds.bounds_bits[0] ^= 1;
    assert_eq!(
        bounds.validate(),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::CompositeBounds,
        })
    );

    let mut payload = edge.clone();
    payload.payload_identity = PaintPayloadIdentity::None;
    assert_eq!(
        payload.validate(),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::CompositePayload,
        })
    );

    let mut clip = edge.clone();
    clip.logical_scissor = Some([0, 0, 0, 10]);
    assert_eq!(
        clip.validate(),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::CompositeClip,
        })
    );

    let mut phase = edge.clone();
    phase.id.phase = PaintNodePhase::BeforeChildren;
    let mut order = edge.clone();
    order.id.slot ^= 1;
    for phase_or_order in [phase, order] {
        assert_eq!(
            phase_or_order.validate_schedule(
                owner,
                PaintNodePhase::AfterChildren,
                1,
                PaintChunkRole::Caret,
            ),
            Err(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::CompositePhaseOrder,
            })
        );
    }

    let mut synchronized = edge.clone();
    synchronized.op.params.position[0] += 1.0;
    synchronized.bounds_bits[0] = synchronized.op.params.position[0].to_bits();
    synchronized.payload_identity =
        PaintPayloadIdentity::prepared_rects([&synchronized.op]).unwrap();
    assert!(synchronized.validate().is_ok());
    assert_eq!(
        synchronized.validate_source_parity(&edge),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::CompositeSourceParity,
        })
    );

    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), false));
}
