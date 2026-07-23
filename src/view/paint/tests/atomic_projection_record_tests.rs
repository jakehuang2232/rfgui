use super::*;

#[test]
fn atomic_projection_text_area_graph_inert_record_and_validator_are_fail_closed() {
    let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
    let root_node = arena.get(root).unwrap();
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    let text_node = arena.get(text_area).unwrap();
    let text_component = text_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .unwrap();
    assert!(
        text_component
            .exact_retained_property_scroll_atomic_projection_subtree(
                text_area,
                &arena,
                [0.0, -20.0],
            )
            .is_some(),
        "source oracle must remain exact after shell placement",
    );
    drop(text_node);
    let admission = root_element
        .exact_retained_scroll_atomic_projection_text_area_subtree_admission(root, &arena, 1.0)
        .expect("atomic projection shell must admit");
    drop(root_node);
    assert_eq!(
        (admission.content_wrapper, admission.text_area_root),
        (wrapper, text_area)
    );
    let (properties, generations) = sync_identity(&arena, &[root]);
    let scroll_id = crate::view::compositor::property_tree::ScrollNodeId(root);
    let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
    let outer_clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let clip_chain = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap();
    let outer_clip = *clip_chain.last().unwrap();
    let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip).unwrap();
    let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id).unwrap();

    let local = super::super::frame_recorder::record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
        &arena,
        &properties,
        &generations,
        &admission,
        outer,
    ).expect("closed local recorder");
    assert_eq!(local.artifact_for_test().chunks.len(), 5);
    let host = super::super::frame_recorder::record_baked_scroll_atomic_projection_text_area_subtree_host_artifact_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        &admission,
        baked,
    ).expect("closed host recorder");
    assert_eq!(host.chunk_count_for_test(), 7);
    assert!(host.is_canonical_for_test());
    let plan_parts =
        super::super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
            host.clone(),
            local.clone(),
        )
        .expect("typed host/local bridge must seal one atomic plan authority");
    assert!(plan_parts.is_canonical());
    assert_eq!(plan_parts.chunk_counts_for_test(), (1, 5, 1));
    assert!(plan_parts.same_authority(&plan_parts.clone()));
    assert_eq!(plan_parts.identity(), plan_parts.clone().identity());
    assert_eq!(plan_parts.local_clip_snapshots().unwrap().len(), 1);
    let content_terminal = plan_parts.content_opaque_order_count().unwrap();
    assert!(
        plan_parts
            .content_artifact_span_stamp(0, 0..content_terminal)
            .is_some()
    );
    assert!(
        !plan_parts
            .clone()
            .tamper_content_bounds_for_test()
            .is_canonical()
    );
    assert!(
        !plan_parts
            .clone()
            .tamper_content_resolved_clips_for_test()
            .is_canonical()
    );
    assert!(!plan_parts.clone().tamper_resident_for_test().is_canonical());
    for tampered_host in [
        host.clone().tamper_cross_parity_bounds_for_test(0),
        host.clone().tamper_cross_parity_bounds_for_test(1),
        host.clone().tamper_cross_parity_bounds_for_test(6),
        host.clone().tamper_cross_parity_payload_for_test(3, 4),
        host.clone().tamper_cross_parity_order_for_test(1, 2),
    ] {
        assert!(
            super::super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
                tampered_host,
                local.clone(),
            )
            .is_none(),
            "synchronized host tamper must reach and fail the bridge parity gate",
        );
    }
    assert!(
        super::super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
            host.clone().tamper_cross_parity_bounds_for_test(1),
            local.clone().tamper_cross_parity_bounds_for_test(0),
        )
        .is_none(),
        "synchronized host/local wrapper drift must fail independent scroll geometry",
    );
    let tampered_host = host.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[0].bounds.x += 1.0;
    });
    assert!(!tampered_host.is_canonical_for_test());
    let tampered_host_payload = host.clone().tamper_artifact_for_test(|artifact| {
        let projection_op = artifact.ops[artifact.chunks[4].op_range.start].clone();
        let projection_payload = artifact.chunks[4].payload_identity.clone();
        artifact.ops[artifact.chunks[3].op_range.start] = projection_op;
        artifact.chunks[3].payload_identity = projection_payload;
    });
    assert!(!tampered_host_payload.is_canonical_for_test());
    let tampered_host_clip = host.clone().tamper_artifact_for_test(|artifact| {
        artifact.clip_nodes[1].logical_scissor[0] ^= 1;
    });
    assert!(!tampered_host_clip.is_canonical_for_test());
    let mut drifted_admission = admission.clone();
    drifted_admission.paint_grammar.projection_text_stable_id ^= 1;
    assert!(
        super::super::frame_recorder::record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
            &arena,
            &properties,
            &generations,
            &drifted_admission,
            outer,
        )
        .is_err(),
        "source/admission drift must fail before recording",
    );
    let validate = |recorded| {
        super::super::frame_recorder::validate_recorded_atomic_projection_text_area_subtree(recorded)
    };
    let validated = validate(local.clone()).expect("dedicated compiler validator");
    assert!(validated.resident_for_test().is_canonical());

    let mut resident_bounds = validated.resident_for_test().clone();
    resident_bounds.text_area_glyph_chunk.bounds_bits[0] ^= 1;
    assert!(!resident_bounds.is_canonical());
    let mut resident_payload = validated.resident_for_test().clone();
    resident_payload.text_area_glyph_chunk.payload_identity = resident_payload
        .projection_glyph_chunk
        .payload_identity
        .clone();
    assert!(!resident_payload.is_canonical());
    let mut resident_clip = validated.resident_for_test().clone();
    resident_clip.contents_clip.logical_scissor[0] ^= 1;
    assert!(!resident_clip.is_canonical());

    let mut cases = Vec::new();
    cases.push(
        local
            .clone()
            .tamper_artifact_for_test(|artifact| artifact.chunks.swap(1, 2)),
    );
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks.remove(4);
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[4].id.slot = 0;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[4].payload_identity = PaintPayloadIdentity::None;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.owner_nodes[2].parent = Some(wrapper);
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[3].owner = text_area;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[2].id.slot = 0;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[2].id.role = PaintChunkRole::SelfDecoration;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.clip_nodes[0].logical_scissor[0] ^= 1;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[2].payload_identity = PaintPayloadIdentity::None;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[1].bounds.x += 1.0;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks[2].bounds.x += 1.0;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        let projection_op = artifact.ops[artifact.chunks[3].op_range.start].clone();
        let projection_payload = artifact.chunks[3].payload_identity.clone();
        artifact.ops[artifact.chunks[2].op_range.start] = projection_op;
        artifact.chunks[2].payload_identity = projection_payload;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        let root_op = artifact.ops[artifact.chunks[2].op_range.start].clone();
        let root_payload = artifact.chunks[2].payload_identity.clone();
        artifact.ops[artifact.chunks[3].op_range.start] = root_op;
        artifact.chunks[3].payload_identity = root_payload;
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.ops.push(artifact.ops[0].clone());
    }));
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.effect_nodes.push(EffectNodeSnapshot {
            id: EffectNodeId(text_area),
            owner: text_area,
            parent: None,
            opacity: 1.0,
            generation: 1,
        });
    }));
    let extra_chunk = local.artifact_for_test().chunks[3].clone();
    cases.push(local.clone().tamper_artifact_for_test(|artifact| {
        artifact.chunks.push(extra_chunk);
    }));
    for (index, recorded) in cases.into_iter().enumerate() {
        assert!(
            validate(recorded).is_none(),
            "tamper case {index} must fail closed",
        );
    }

    let budget =
        super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
            .unwrap();
    let scene = super::super::scroll_scene::plan_property_scroll_scene_scaffold(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        budget,
    )
    .expect("atomic selector must produce one graph-inert property-scroll plan");
    assert!(scene.is_canonical());
    assert!(scene.atomic_projection_contract_for_test());
    assert!(scene.atomic_projection_tamper_matrix_for_test());
    let validated_scene = super::super::scroll_scene::plan_and_validate_property_scroll_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        budget,
    )
    .expect("atomic selector must compiler-seal one graph-inert boundary");
    assert!(validated_scene.is_canonical());
    assert!(
        validated_scene.atomic_projection_prepare_and_collision_are_atomic_for_test(),
        "C3a atomic prepare must succeed exactly once and reject collisions without local declarations",
    );
    let mut viewport = crate::view::viewport::Viewport::new();
    let frame_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("fresh viewport must admit one retained frame owner");
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let prepare = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        validated_scene,
        &mut graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    );
    let prepared = prepare.expect("C3a atomic authority must prepare without graph mutation");
    drop(prepared);
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before,
    );
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
}

#[test]
fn atomic_projection_selection_record_consume_is_typed_and_fail_closed() {
    let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
    {
        let mut node = arena.get_mut(text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(0);
        text_area.selection_focus_char = Some(6);
    }
    let root_node = arena.get(root).unwrap();
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    let admission = root_element
        .exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
            root, &arena, 1.0,
        )
        .expect("disjoint root selection plus one projection must admit");
    assert!(admission.paint_grammar.is_canonical());
    assert!(admission.bitwise_eq(&admission.clone()));
    assert!(
        root_element
            .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                root, &arena, 1.0,
            )
            .is_none(),
        "existing atomic glyph selector must remain selection-free",
    );
    assert!(
        root_element
            .exact_retained_scroll_text_area_subtree_admission(root, &arena, 1.0)
            .is_none(),
        "C1/C2 selector must remain projection-free",
    );
    drop(root_node);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let scroll = properties
        .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
        .unwrap();
    let outer_clip = *properties
        .clip_snapshot_for(Some(ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        }))
        .unwrap()
        .last()
        .unwrap();
    let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip).unwrap();
    let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id).unwrap();
    let local = super::super::frame_recorder::record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
        &arena,
        &properties,
        &generations,
        &admission,
        outer,
    )
    .expect("typed closed-union local recording");
    let host = super::super::frame_recorder::record_baked_scroll_atomic_projection_selection_text_area_subtree_host_artifact_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        &admission,
        baked,
    )
    .expect("typed H/content/O recording");
    assert_eq!(host.chunk_count_for_test(), 8);
    assert_eq!(local.chunk_count_for_test(), 6);
    assert!(host.is_canonical_for_test());
    assert!(local.is_canonical_for_test());
    let authority = super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
        host.clone(),
        local.clone(),
    )
    .expect("normalized typed pair must consume");
    assert!(authority.is_canonical_for_test());
    assert_eq!(authority.chunk_counts_for_test(), (8, 6));
    assert!(
        authority.localized_selection_changed_for_test(),
        "nonzero outer scroll must localize selection rectangles",
    );
    let plan_parts = super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_plan_parts(authority)
        .expect("typed authority must consume into opaque fixed H/content/O plan parts");
    assert!(plan_parts.is_canonical());
    assert_eq!(plan_parts.chunk_counts_for_test(), (1, 6, 1));
    assert_eq!(
        (
            plan_parts.host_before_opaque_order_count(),
            plan_parts.content_opaque_order_count(),
            plan_parts.overlay_opaque_order_count(),
        ),
        (Some(0), Some(0), Some(0)),
    );
    assert!(plan_parts.same_authority(&plan_parts.clone()));
    assert!(
        !plan_parts.clone().tamper_host_for_test().is_canonical()
            && !plan_parts
                .clone()
                .tamper_content_order_for_test()
                .is_canonical()
            && !plan_parts.clone().tamper_geometry_for_test().is_canonical()
            && !plan_parts.clone().tamper_topology_for_test().is_canonical()
            && !plan_parts
                .tamper_selection_synchronized_for_test()
                .is_canonical(),
        "private plan identity must reject H/local order/geometry/topology/selection drift",
    );

    assert!(
        super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host.clone().tamper_order_for_test(2, 3),
            local.clone(),
        )
        .is_none(),
        "synchronized host order tamper must fail",
    );
    assert!(
        super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host.clone(),
            local.clone().tamper_selection_payload_for_test(),
        )
        .is_none(),
        "synchronized local payload tamper must fail",
    );
    assert!(
        super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host.clone().tamper_selection_payload_for_test(),
            local.clone(),
        )
        .is_none(),
        "synchronized host payload tamper must fail",
    );
    assert!(
        super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host.clone().tamper_wrapper_bounds_for_test(),
            local.clone().tamper_wrapper_bounds_for_test(),
        )
        .is_none(),
        "synchronized host/local bounds drift must fail independent scroll geometry",
    );
    assert!(
        super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host.clone(),
            local.clone().tamper_owner_parent_for_test(),
        )
        .is_none(),
        "synchronized owner topology tamper must fail",
    );
    assert!(
        super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host.clone().tamper_source_line_for_test(),
            local.clone().tamper_source_line_for_test(),
        )
        .is_none(),
        "synchronized public source grammar tamper must fail private identity",
    );
    assert!(
        super::super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host,
            local.tamper_local_clip_for_test(),
        )
        .is_none(),
        "synchronized local clip tamper must fail",
    );
}
