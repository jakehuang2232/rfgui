use super::*;

fn synthetic_generic_resident_stamp(topology_revision: u64) -> RetainedSurfaceRasterStamp {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xa2_4300, 0.0, 0.0, 64.0, 32.0)),
    );
    let chunk = RetainedSurfaceChunkStamp {
        id: PaintChunkId {
            owner: root,
            scope: PaintPropertyScope::SelfPaint,
            phase: PaintNodePhase::BeforeChildren,
            slot: 0,
            role: PaintChunkRole::SelfDecoration,
        },
        owner: root,
        bounds_bits: [0.0_f32, 0.0_f32, 64.0_f32, 32.0_f32].map(f32::to_bits),
        clip: None,
        non_boundary_self_paint_revision: None,
        topology_revision,
        non_boundary_composite_revision: None,
        payload_identity: PaintPayloadIdentity::None,
        op_count: 1,
    };
    let span = RetainedSurfaceArtifactSpanStamp {
        step_index: 0,
        owner_topology: vec![PaintOwnerSnapshot {
            owner: root,
            parent: None,
        }],
        clip_nodes: Vec::new(),
        chunks: vec![chunk],
        op_count: 1,
        opaque_order_span: 0..0,
        scroll_placement_normalized_owners: Vec::new(),
    };
    let bounds = crate::view::base_component::RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 64.0,
        height: 32.0,
        corner_radii: [0.0; 4],
    };
    let stable_id = 0xa2_4301;
    let color_key = crate::view::base_component::scroll_content_layer_stable_key(stable_id);
    let color = crate::view::base_component::texture_desc_for_logical_bounds(
        bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let (color, depth) =
        crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
    super::super::compiler::validated_scroll_content_raster_stamp(
        root,
        stable_id,
        RetainedSurfaceRasterInputs {
            color,
            depth,
            scale_factor_bits: 1.0_f32.to_bits(),
            source_bounds_bits: [0.0_f32, 0.0_f32, 64.0_f32, 32.0_f32].map(f32::to_bits),
        },
        span,
        0..0,
    )
    .expect("synthetic generic content must seal one resident raster stamp")
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
