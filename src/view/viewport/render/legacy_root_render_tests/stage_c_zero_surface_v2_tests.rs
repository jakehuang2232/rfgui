use super::*;

fn zero_surface_element(id: u64, x: f32, color: Color) -> Element {
    let mut element = Element::new_with_id(id, x, 20.0, 80.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn prepared_zero_surface_three_chunk_frame() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut arena = new_test_arena();
    let first = commit_element(
        &mut arena,
        Box::new(zero_surface_element(
            0xc3_a001,
            10.0,
            Color::rgb(230, 20, 30),
        )),
    );
    let mut child_element = zero_surface_element(0xc3_a002, 40.0, Color::rgb(20, 210, 40));
    child_element.set_position(0.0, 0.0);
    let child = commit_child(&mut arena, first, Box::new(child_element));
    let second = commit_element(
        &mut arena,
        Box::new(zero_surface_element(
            0xc3_a003,
            110.0,
            Color::rgb(30, 40, 220),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, first, measure, place);
    measure_and_place(&mut arena, second, measure, place);
    (arena, vec![first, second], child)
}

fn recorded_zero_surface_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
) -> (
    crate::view::paint::PaintArtifact,
    crate::view::paint::FrameArtifactEligibility,
) {
    let (properties, generations) = synced_paint_state(arena, roots);
    let crate::view::paint::FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = crate::view::paint::record_closed_single_target_frame_artifact(
        arena,
        roots,
        &properties,
        &generations,
        crate::view::paint::RendererMode::ForcedForTests,
    )
    .expect("zero-surface fixture must be fully recordable")
    else {
        panic!("forced recording cannot silently fall back")
    };
    (artifact, eligibility)
}

fn recorded_single_effect_surface_artifact() -> crate::view::paint::PaintArtifact {
    let (arena, roots, _) = prepared_zero_surface_three_chunk_frame();
    let (mut artifact, _) = recorded_zero_surface_artifact(&arena, &roots);
    let owner = roots[0];
    let effect = crate::view::compositor::property_tree::EffectNodeId(owner);
    artifact
        .effect_nodes
        .push(crate::view::compositor::property_tree::EffectNodeSnapshot {
            id: effect,
            owner,
            parent: None,
            opacity: 0.5,
            generation: 1,
        });
    artifact
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == owner)
        .expect("root owner endpoint")
        .descendants
        .effect = Some(effect);
    artifact
}

fn recorded_zero_surface_child_mask_candidate() -> RecordedArtifactCandidate {
    use crate::style::BorderRadius;

    let mut root_element = zero_surface_element(0xc3_a010, 0.0, Color::rgb(40, 80, 160));
    let mut rounded = Style::new();
    rounded.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    root_element.apply_style(rounded);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    commit_child(
        &mut arena,
        root,
        Box::new(zero_surface_element(
            0xc3_a011,
            12.0,
            Color::rgb(20, 180, 40),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (artifact, eligibility) = recorded_zero_surface_artifact(&arena, &[root]);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .filter(|chunk| chunk.id.slot == crate::view::paint::RETAINED_CHILD_MASK_SLOT)
            .count(),
        2,
    );
    let prepared = crate::view::paint::prepare_single_target_surface_dag_frame(artifact)
        .expect("zero-surface child-mask artifact must prepare");
    RecordedArtifactCandidate {
        payload: RecordedArtifactPayload::SingleTargetSurfaceDag(prepared),
        eligibility,
    }
}

fn prepare_error_label(
    error: crate::view::paint::SingleTargetSurfaceDagPrepareError,
) -> &'static str {
    use crate::view::paint::SingleTargetSurfaceDagPrepareError;
    match error {
        SingleTargetSurfaceDagPrepareError::InvalidArtifactStore => "invalid-artifact-store",
        SingleTargetSurfaceDagPrepareError::UnsupportedTarget(_) => "unsupported-target",
        SingleTargetSurfaceDagPrepareError::SurfaceDag(_) => "surface-dag",
        SingleTargetSurfaceDagPrepareError::DetachedSurfacesUnsupported { .. } => {
            "detached-surfaces-unsupported"
        }
    }
}

#[test]
fn stage_c_zero_surface_prepare_seals_exact_multi_root_artifact_order() {
    let (arena, roots, child) = prepared_zero_surface_three_chunk_frame();
    let (artifact, eligibility) = recorded_zero_surface_artifact(&arena, &roots);
    assert!(eligibility.eligible);
    assert_eq!(
        (eligibility.chunk_count, eligibility.op_count),
        (3, 3),
        "chunks={:?}",
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role, chunk.id.phase, chunk.id.slot))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>(),
        vec![roots[0], child, roots[1]],
    );
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.op_range.clone())
            .collect::<Vec<_>>(),
        vec![0..1, 1..2, 2..3],
    );
    assert_eq!(
        crate::view::paint::artifact_cursors(&artifact)
            .expect("complete chunk/op traversal owns a terminal cursor")
            .len(),
        3,
    );

    let prepared = crate::view::paint::prepare_single_target_surface_dag_frame(artifact)
        .expect("current-target zero-surface artifact must seal");
    assert_eq!(prepared.artifact().chunks.len(), 3);
    assert_eq!(prepared.artifact().ops.len(), 3);
    assert_eq!(prepared.surface_dag().roots().len(), 2);
    assert!(prepared.surface_dag().nodes().is_empty());
    assert_eq!(prepared.execution_order().roots().len(), 2);
    assert!(prepared.execution_order().nodes().is_empty());
    assert!(
        prepared
            .execution_order()
            .roots()
            .iter()
            .all(|root| root.node_span().is_empty())
    );
}

#[test]
fn stage_c_zero_surface_prepare_rejects_a_purpose_named_detached_surface() {
    let artifact = recorded_single_effect_surface_artifact();

    let error = crate::view::paint::prepare_single_target_surface_dag_frame(artifact)
        .expect_err("C3a must reject rather than reinterpret a detached candidate");
    assert_eq!(
        prepare_error_label(error),
        "detached-surfaces-unsupported",
        "unexpected rejection: {error:?}",
    );
    assert_eq!(
        error,
        crate::view::paint::SingleTargetSurfaceDagPrepareError::DetachedSurfacesUnsupported {
            candidates: 1,
        }
    );

    let trace = AutoAuthorityTrace {
        capture_rejections: true,
        rejections: vec![AutoAuthorityRejection::ArtifactPrepare { error }],
    };
    assert_eq!(
        auto_artifact_legacy_fallback_stage(&trace),
        PaintAuthorityFallbackStage::Prepare,
    );
}

#[test]
fn stage_c_valid_detached_rejection_keeps_graph_and_compile_state_unchanged() {
    let artifact = recorded_single_effect_surface_artifact();
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let before = graph.build_state_snapshot_for_test();
    crate::view::paint::take_artifact_compile_count();

    let error = crate::view::paint::prepare_single_target_surface_dag_frame(artifact)
        .expect_err("valid detached surface remains outside the C3a acceptance slice");
    assert_eq!(
        error,
        crate::view::paint::SingleTargetSurfaceDagPrepareError::DetachedSurfacesUnsupported {
            candidates: 1,
        },
    );
    assert_eq!(graph.build_state_snapshot_for_test(), before);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);
}

#[test]
fn stage_c_zero_surface_prepare_rejects_invalid_store_without_graph_mutation() {
    let (arena, roots, _) = prepared_zero_surface_three_chunk_frame();
    let (mut artifact, _) = recorded_zero_surface_artifact(&arena, &roots);
    artifact.chunks[1].op_range.end = artifact.ops.len() + 1;

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let before = graph.build_state_snapshot_for_test();
    let error = crate::view::paint::prepare_single_target_surface_dag_frame(artifact)
        .expect_err("invalid store must reject before emission");
    assert_eq!(prepare_error_label(error), "invalid-artifact-store");
    assert_eq!(
        error,
        crate::view::paint::SingleTargetSurfaceDagPrepareError::InvalidArtifactStore,
    );
    assert_eq!(graph.build_state_snapshot_for_test(), before);
}

#[test]
fn stage_c_zero_surface_child_mask_depth_seam_accepts_254_and_rejects_255_before_graph_mutation() {
    let mut accepted_graph = FrameGraph::new();
    let mut accepted_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let accepted_target = accepted_ctx.allocate_target(&mut accepted_graph);
    accepted_ctx.set_current_target(accepted_target);
    for expected in 1..=254_u8 {
        assert_eq!(accepted_ctx.push_clip_id(), Some(expected));
    }
    crate::view::paint::take_artifact_compile_count();
    assert!(matches!(
        try_compile_recorded_artifact_frame(
            &mut accepted_graph,
            recorded_zero_surface_child_mask_candidate(),
            &accepted_ctx,
            None,
        ),
        PropertyNeutralArtifactAttempt::Compiled { .. }
    ));
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);

    let mut rejected_graph = FrameGraph::new();
    let mut rejected_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let rejected_target = rejected_ctx.allocate_target(&mut rejected_graph);
    rejected_ctx.set_current_target(rejected_target);
    for expected in 1..=u8::MAX {
        assert_eq!(rejected_ctx.push_clip_id(), Some(expected));
    }
    assert_eq!(rejected_ctx.push_clip_id(), None);
    let before = rejected_graph.build_state_snapshot_for_test();
    crate::view::paint::take_artifact_compile_count();
    let rejection = try_compile_recorded_artifact_frame(
        &mut rejected_graph,
        recorded_zero_surface_child_mask_candidate(),
        &rejected_ctx,
        None,
    );
    assert!(matches!(
        rejection,
        PropertyNeutralArtifactAttempt::CompileRejected(
            crate::view::paint::ArtifactCompileErrorKind::ChildMaskDepthOverflow {
                incoming_depth: u8::MAX,
                max_mask_depth: 1,
            }
        )
    ));
    assert_eq!(rejected_graph.build_state_snapshot_for_test(), before);
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);
}

#[test]
fn stage_c_zero_surface_retained_auto_emits_once_and_matches_legacy() {
    let (arena, roots) = prepared_safe_leaf();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let selection_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let AutoAuthorityDecision::Artifact { candidate, trace } = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &selection_ctx,
        true,
    ) else {
        panic!("property-neutral frame must select artifact authority")
    };
    assert!(trace.rejections.is_empty());
    assert_eq!(
        (
            candidate.eligibility.chunk_count,
            candidate.eligibility.op_count
        ),
        (1, 1),
    );
    let RecordedArtifactPayload::SingleTargetSurfaceDag(prepared) = &candidate.payload else {
        panic!("RetainedAuto current target must carry the C3a seal")
    };
    assert!(prepared.surface_dag().nodes().is_empty());
    assert!(prepared.execution_order().nodes().is_empty());

    crate::view::paint::take_artifact_compile_count();
    let mut graph = FrameGraph::new();
    let mut compile_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = compile_ctx.allocate_target(&mut graph);
    compile_ctx.set_current_target(target);
    assert!(matches!(
        try_compile_recorded_artifact_frame(&mut graph, candidate, &compile_ctx, None),
        PropertyNeutralArtifactAttempt::Compiled {
            root_effect_transaction: None,
            ..
        }
    ));
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 1);

    let (legacy_arena, legacy_roots) = prepared_safe_leaf();
    let legacy = build_roots_graph(legacy_arena, &legacy_roots, false);
    assert_eq!(
        graph.test_rect_pass_snapshots(),
        legacy.test_rect_pass_snapshots(),
    );
}

#[test]
fn stage_c_zero_surface_keeps_root_opacity_on_the_existing_artifact_path() {
    let (arena, roots) = prepared_native_text_with_opacity(0.5);
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let AutoAuthorityDecision::Artifact { candidate, trace } = auto_decision(&arena, &roots, &ctx)
    else {
        panic!("native root opacity must retain artifact authority")
    };
    assert!(trace.rejections.is_empty());
    let RecordedArtifactPayload::ExistingArtifact(artifact) = candidate.payload else {
        panic!("root opacity is excluded from the C3a zero-surface claim")
    };
    assert!(matches!(
        artifact.target,
        crate::view::paint::PaintArtifactTarget::RootOpacityGroup { .. }
    ));
}
