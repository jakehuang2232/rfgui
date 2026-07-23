use super::*;

#[test]
fn root_group_element_records_neutral_content_and_composites_effect_once() {
    let (arena, root, properties, generations) =
        prepared_leaf(0x6c10, Color::rgb(220, 40, 30), 0.5, true);
    let (artifact, eligibility) =
        root_group_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible);
    assert!(matches!(
        artifact.target,
        PaintArtifactTarget::RootOpacityGroup {
            root: target_root,
            effect: EffectNodeId(effect_root),
            ..
        } if target_root == root && effect_root == root
    ));
    artifact.ops.iter().for_each(assert_neutral_opacity);

    let mut graph = compiled_whole_frame_graph(&artifact);
    let root_color = crate::view::base_component::root_effect_stable_key(root);
    let declared = graph
        .declared_persistent_texture_keys()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(declared.len(), 2);
    assert!(declared.contains(&root_color));
    assert!(declared.contains(&root_color.depth_stencil().unwrap()));
    let snapshot = graph.test_compile_snapshot().unwrap();
    assert!(
        matches!(
            snapshot.pass_payloads(),
            [
                FramePassTestPayload::Clear(_),
                FramePassTestPayload::DrawRect(_),
                FramePassTestPayload::DrawRect(_),
                FramePassTestPayload::Clear(_),
                FramePassTestPayload::CompositeLayer(composite),
            ] if composite.opacity_bits == 0.5_f32.to_bits()
        ),
        "payloads={:?}",
        snapshot.pass_payloads()
    );
}

#[test]
fn root_opacity_group_records_contents_clip_neutrally_and_metadata_matches_full() {
    const SCISSOR: [u32; 4] = [7, 11, 23, 19];
    let (arena, root, child, properties, generations) =
        root_opacity_contents_clip_fixture(SCISSOR);
    let expected_clip = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let root_state = properties.node_state_for(root).expect("root state");
    let child_state = properties.node_state_for(child).expect("child state");
    assert_eq!(
        root_state.paint.clip, None,
        "contents clip excludes self paint"
    );
    assert_eq!(root_state.descendants.clip, Some(expected_clip));
    assert_eq!(child_state.paint.clip, Some(expected_clip));

    let recording_context = PaintRecordingContext {
        opacity_authority: PaintOpacityAuthority::NeutralRootEffect(EffectNodeId(root)),
        ..PaintRecordingContext::default()
    };
    let metadata = super::super::coverage_manifest::record_coverage_manifest_with_context(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
        recording_context,
        None,
        &Default::default(),
    );
    let mut full = super::super::coverage_manifest::record_coverage_manifest_with_context(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
        recording_context,
        None,
        &Default::default(),
    );
    assert!(super::super::frame_recorder::canonical_manifest_matches(
        &metadata, &full
    ));
    let clip_snapshot = full
        .items
        .iter_mut()
        .find_map(|item| match item {
            PaintCoverageItem::ArtifactChunk { clip_snapshot, .. }
                if !clip_snapshot.is_empty() =>
            {
                Some(clip_snapshot)
            }
            _ => None,
        })
        .expect("child chunk carries the root contents clip snapshot");
    clip_snapshot[0].logical_scissor[0] += 1;
    assert!(
        !super::super::frame_recorder::canonical_manifest_matches(&metadata, &full),
        "clip snapshot drift must fail metadata/full parity"
    );

    let (artifact, eligibility) =
        root_group_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible, "{eligibility:?}");
    assert_eq!(artifact.chunks.len(), 1);
    assert_eq!(artifact.chunks[0].owner, child);
    assert_eq!(artifact.chunks[0].properties.clip, Some(expected_clip));
    assert!(matches!(
        artifact.clip_nodes.as_slice(),
        [ClipNodeSnapshot {
            id,
            owner,
            logical_scissor: SCISSOR,
            behavior: ClipBehavior::Intersect,
            ..
        }] if *id == expected_clip && *owner == root
    ));
    artifact.ops.iter().for_each(assert_neutral_opacity);

    let baseline_stamp =
        validated_root_effect_raster_stamp(&artifact, root_effect_raster_inputs())
            .expect("clipped root group has a valid raster stamp");
    let mut clip_changed = artifact.clone();
    clip_changed.clip_nodes[0].logical_scissor[2] += 1;
    assert_ne!(
        validated_root_effect_raster_stamp(&clip_changed, root_effect_raster_inputs())
            .expect("changed clip remains a valid artifact"),
        baseline_stamp,
        "clip geometry must invalidate root-effect raster reuse"
    );

    let mut graph = compiled_whole_frame_graph(&artifact);
    let rects = graph.test_rect_pass_snapshots();
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[0].effective_scissor_rect, Some(SCISSOR));
    let snapshot = graph
        .test_compile_snapshot()
        .expect("strict graph snapshot");
    assert!(matches!(
        snapshot.pass_payloads().last(),
        Some(FramePassTestPayload::CompositeLayer(composite))
            if composite.opacity_bits == 0.5_f32.to_bits()
    ));
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .len(),
        1,
        "root effect must composite exactly once"
    );
}

#[test]
fn root_opacity_group_explicit_empty_contents_clip_culls_only_contents() {
    let (arena, root, child, properties, generations) =
        root_opacity_contents_clip_fixture([13, 17, 0, 0]);
    let (artifact, eligibility) =
        root_group_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible, "{eligibility:?}");
    assert_eq!(artifact.chunks.len(), 1);
    assert_eq!(artifact.chunks[0].owner, child);
    artifact.ops.iter().for_each(assert_neutral_opacity);

    let graph = compiled_whole_frame_graph(&artifact);
    assert!(
        graph.test_rect_pass_snapshots().is_empty(),
        "explicit empty contents clip must suppress the child raster"
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .len(),
        1,
        "the root effect group itself still composites once"
    );
}

#[test]
fn root_effect_stamp_excludes_root_opacity_and_root_composite_only() {
    let (arena, root, properties, generations) =
        prepared_leaf(0x6c20, Color::rgb(20, 120, 220), 0.5, true);
    let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
    let baseline = validated_root_effect_raster_stamp(&artifact, root_effect_raster_inputs())
        .expect("valid root effect stamp");

    let mut opacity_only = artifact.clone();
    opacity_only.effect_nodes[0].opacity = 0.25;
    for chunk in &mut opacity_only.chunks {
        if chunk.owner == root {
            chunk.content_revision.composite_revision =
                chunk.content_revision.composite_revision.wrapping_add(100);
        }
    }
    assert_eq!(
        validated_root_effect_raster_stamp(&opacity_only, root_effect_raster_inputs())
            .expect("root opacity remains valid"),
        baseline
    );

    let mut self_paint = artifact.clone();
    self_paint.chunks[0].content_revision.self_paint_revision = self_paint.chunks[0]
        .content_revision
        .self_paint_revision
        .wrapping_add(1);
    assert_ne!(
        validated_root_effect_raster_stamp(&self_paint, root_effect_raster_inputs()).unwrap(),
        baseline
    );

    let mut topology = artifact.clone();
    topology.chunks[0].content_revision.topology_revision = topology.chunks[0]
        .content_revision
        .topology_revision
        .wrapping_add(1);
    assert_ne!(
        validated_root_effect_raster_stamp(&topology, root_effect_raster_inputs()).unwrap(),
        baseline
    );
}

#[test]
fn root_effect_stamp_tracks_nested_exact_self_clip_snapshot() {
    let (arena, roots, anchor) = nested_anchor_parent_mixed_siblings(false);
    let root = roots[0];
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_opacity(0.5);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        root_group_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible, "{eligibility:?}");
    let own = ClipNodeId {
        owner: anchor,
        role: ClipNodeRole::SelfClip,
    };
    let clip_index = artifact
        .clip_nodes
        .iter()
        .position(|clip| clip.id == own)
        .expect("nested exact self clip snapshot");
    let baseline = validated_root_effect_raster_stamp(&artifact, root_effect_raster_inputs())
        .expect("valid nested self clip stamp");

    let mut changed = artifact;
    changed.clip_nodes[clip_index].logical_scissor[2] += 1;
    assert_ne!(
        validated_root_effect_raster_stamp(&changed, root_effect_raster_inputs()).unwrap(),
        baseline
    );
}

#[test]
fn root_effect_stamp_tracks_every_exact_viewport_raster_input() {
    let (arena, root, properties, generations) =
        prepared_leaf(0x6c21, Color::rgb(20, 120, 220), 0.5, true);
    let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
    let baseline_inputs = root_effect_raster_inputs();
    let baseline =
        validated_root_effect_raster_stamp(&artifact, baseline_inputs).expect("baseline");

    let mut mismatches = Vec::new();
    let mut width = baseline_inputs;
    width.width += 1;
    mismatches.push(width);
    let mut height = baseline_inputs;
    height.height += 1;
    mismatches.push(height);
    let mut format = baseline_inputs;
    format.format = wgpu::TextureFormat::Rgba16Float;
    mismatches.push(format);
    let mut samples = baseline_inputs;
    samples.sample_count = 4;
    mismatches.push(samples);
    let mut scale = baseline_inputs;
    scale.scale_factor_bits = 2.0_f32.to_bits();
    mismatches.push(scale);

    for inputs in mismatches {
        assert_ne!(
            validated_root_effect_raster_stamp(&artifact, inputs).unwrap(),
            baseline
        );
    }
}

#[test]
fn root_effect_stamp_tracks_descendant_composite_owner_topology_and_payload() {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(leaf_element(0x6c24, Color::rgb(220, 30, 20), 0.5, false)),
    );
    let mut child_element = leaf_element(0x6c25, Color::rgb(20, 40, 220), 1.0, false);
    child_element.set_position(0.0, 0.0);
    let child = commit_child(&mut arena, root, Box::new(child_element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
    let baseline = validated_root_effect_raster_stamp(&artifact, root_effect_raster_inputs())
        .expect("baseline");

    let mut child_composite = artifact.clone();
    child_composite
        .chunks
        .iter_mut()
        .find(|chunk| chunk.owner == child)
        .expect("child chunk")
        .content_revision
        .composite_revision += 1;
    assert_ne!(
        validated_root_effect_raster_stamp(&child_composite, root_effect_raster_inputs())
            .unwrap(),
        baseline
    );

    let mut owner_order = artifact.clone();
    owner_order.owner_nodes.reverse();
    assert_ne!(
        validated_root_effect_raster_stamp(&owner_order, root_effect_raster_inputs()).unwrap(),
        baseline
    );

    let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
    let (image_arena, image_roots) = prepared_image_fixture(
        pixels,
        crate::view::ImageFit::Fill,
        crate::view::ImageSampling::Linear,
        0.4,
    );
    let (image_properties, image_generations) = sync_identity(&image_arena, &image_roots);
    let (image, _) = root_group_artifact(
        &image_arena,
        &image_roots,
        &image_properties,
        &image_generations,
    );
    let image_baseline =
        validated_root_effect_raster_stamp(&image, root_effect_raster_inputs()).unwrap();
    let mut payload_changed = image.clone();
    let decoration = payload_changed
        .chunks
        .iter()
        .find_map(|chunk| match &chunk.payload_identity {
            PaintPayloadIdentity::Image(_, decoration) => Some(Arc::clone(decoration)),
            _ => None,
        })
        .expect("image chunk composite identity");
    let prepared = payload_changed
        .ops
        .iter_mut()
        .find_map(|op| match op {
            PaintOp::PreparedImage(prepared) => Some(prepared),
            _ => None,
        })
        .expect("prepared image");
    prepared.upload.generation = prepared.upload.generation.wrapping_add(1);
    let identity = PreparedImageIdentity::from_op(prepared);
    payload_changed
        .chunks
        .iter_mut()
        .find(|chunk| chunk.id.role == PaintChunkRole::ImageContent)
        .expect("image chunk")
        .payload_identity = PaintPayloadIdentity::Image(identity, decoration);
    assert_ne!(
        validated_root_effect_raster_stamp(&payload_changed, root_effect_raster_inputs())
            .unwrap(),
        image_baseline
    );
}

#[test]
fn root_effect_compiler_reuse_declares_pair_but_emits_only_composite() {
    let (arena, root, properties, generations) =
        prepared_leaf(0x6c22, Color::rgb(20, 120, 220), 0.5, true);
    let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);

    fn compile(
        artifact: &PaintArtifact,
        action: RootEffectCompileAction,
    ) -> (FrameGraph, BuildState) {
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target,
            },
        ));
        let state = try_compile_root_effect_artifact(artifact, action, &mut graph, ctx)
            .unwrap_or_else(|_| panic!("valid root artifact"));
        (graph, state)
    }

    let (mut reraster, _) = compile(&artifact, RootEffectCompileAction::Reraster);
    let reraster_snapshot = reraster.test_compile_snapshot().unwrap();
    assert!(reraster_snapshot.pass_payloads().len() > 2);

    let (mut reuse, state) = compile(&artifact, RootEffectCompileAction::Reuse);
    let reuse_snapshot = reuse.test_compile_snapshot().unwrap();
    assert!(matches!(
        reuse_snapshot.pass_payloads(),
        [
            FramePassTestPayload::Clear(_),
            FramePassTestPayload::CompositeLayer(_)
        ]
    ));
    let declared = reuse
        .declared_persistent_texture_keys()
        .collect::<std::collections::HashSet<_>>();
    let color = crate::view::base_component::root_effect_stable_key(root);
    assert_eq!(declared.len(), 2);
    assert!(declared.contains(&color));
    assert!(declared.contains(&color.depth_stencil().unwrap()));
    assert!(state.current_target().is_some());
}

#[test]
fn malformed_root_effect_reuse_rejects_before_any_emit() {
    let (arena, root, properties, generations) =
        prepared_leaf(0x6c23, Color::rgb(20, 120, 220), 0.5, true);
    let (mut artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
    let PaintOp::DrawRect(op) = &mut artifact.ops[0] else {
        panic!("fixture begins with rect");
    };
    op.params.opacity = 0.5;
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);

    assert!(
        try_compile_root_effect_artifact(
            &artifact,
            RootEffectCompileAction::Reuse,
            &mut graph,
            ctx,
        )
        .is_err()
    );
    assert!(graph.pass_descriptors().is_empty());
    assert_eq!(graph.declared_persistent_texture_keys().count(), 0);
}

#[test]
fn zero_and_half_root_group_have_identical_raster_payloads() {
    fn snapshot(opacity: f32) -> FrameGraphTestSnapshot {
        let (arena, root, properties, generations) =
            prepared_leaf(0x6c11, Color::rgb(20, 120, 220), opacity, true);
        let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
        artifact.ops.iter().for_each(assert_neutral_opacity);
        let mut graph = compiled_whole_frame_graph(&artifact);
        graph.test_compile_snapshot().unwrap()
    }
    let zero = snapshot(0.0);
    let half = snapshot(0.5);
    assert_eq!(zero.pass_payloads().len(), half.pass_payloads().len());
    for (zero, half) in zero.pass_payloads().iter().zip(half.pass_payloads()) {
        match (zero, half) {
            (
                FramePassTestPayload::CompositeLayer(zero),
                FramePassTestPayload::CompositeLayer(half),
            ) => {
                assert_eq!(zero.opacity_bits, 0.0_f32.to_bits());
                assert_eq!(half.opacity_bits, 0.5_f32.to_bits());
                let mut zero = zero.clone();
                zero.opacity_bits = half.opacity_bits;
                assert_eq!(&zero, half);
            }
            _ => assert_eq!(zero, half),
        }
    }
}

#[test]
fn root_group_text_and_image_record_native_neutral_payloads_and_identities() {
    let (text_arena, text_roots, _) = prepared_text_tree(false);
    let (text_properties, text_generations) = sync_identity(&text_arena, &text_roots);
    let (text, _) = root_group_artifact(
        &text_arena,
        &text_roots,
        &text_properties,
        &text_generations,
    );
    assert!(
        text.ops
            .iter()
            .any(|op| matches!(op, PaintOp::PreparedText(_)))
    );
    text.ops.iter().for_each(assert_neutral_opacity);

    let (inline_arena, inline_roots, inline_owner) =
        prepared_inline_owned_text_tree(InlineOwnedTextDamage::None);
    let (inline_properties, inline_generations) = sync_identity(&inline_arena, &inline_roots);
    let (inline_text, _) = root_group_artifact(
        &inline_arena,
        &inline_roots,
        &inline_properties,
        &inline_generations,
    );
    assert!(matches!(
        inline_text.target,
        PaintArtifactTarget::RootOpacityGroup { root, .. } if root == inline_owner
    ));
    assert!(matches!(
        inline_text.chunks[0].payload_identity,
        PaintPayloadIdentity::PreparedTexts(_)
    ));
    inline_text.ops.iter().for_each(assert_neutral_opacity);
    let inline_graph = compiled_whole_frame_graph(&inline_text);
    assert_eq!(
        inline_graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .len(),
        1
    );

    let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
    let (image_arena, image_roots) = prepared_image_fixture(
        pixels,
        crate::view::ImageFit::Fill,
        crate::view::ImageSampling::Linear,
        0.4,
    );
    let (image_properties, image_generations) = sync_identity(&image_arena, &image_roots);
    let (image, _) = root_group_artifact(
        &image_arena,
        &image_roots,
        &image_properties,
        &image_generations,
    );
    image.ops.iter().for_each(assert_neutral_opacity);
    let prepared = image
        .ops
        .iter()
        .find_map(|op| match op {
            PaintOp::PreparedImage(op) => Some(op),
            _ => None,
        })
        .expect("image group must own a prepared image");
    assert!(matches!(
        image.chunks[0].payload_identity,
        PaintPayloadIdentity::Image(actual, _)
            if actual == PreparedImageIdentity::from_op(prepared)
    ));
    let PaintPayloadIdentity::Image(identity, _) = image.chunks[0].payload_identity else {
        unreachable!()
    };
    assert_eq!(identity.opacity_bits, 1.0_f32.to_bits());
}

#[test]
fn root_group_preflight_rejects_nested_non_effect_and_deferred_before_full_hooks() {
    fn nested_fixture() -> (NodeArena, Vec<NodeKey>) {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(leaf_element(0x6c20, Color::rgb(220, 30, 20), 0.5, false)),
        );
        commit_child(
            &mut arena,
            root,
            Box::new(leaf_element(0x6c21, Color::rgb(20, 40, 220), 0.25, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root])
    }

    let (nested_arena, nested_roots) = nested_fixture();
    let (nested_properties, nested_generations) = sync_identity(&nested_arena, &nested_roots);
    let _ = take_full_artifact_record_count();
    let nested = record_root_group_opacity_frame_artifact(
        &nested_arena,
        &nested_roots,
        &nested_properties,
        &nested_generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert!(
        nested
            .reasons
            .iter()
            .any(|reason| matches!(reason, FrameArtifactFallbackReason::NestedEffect(_)))
    );
    assert_eq!(take_full_artifact_record_count(), 0);

    let (neutral_arena, neutral_root, neutral_properties, neutral_generations) =
        prepared_leaf(0x6c25, Color::rgb(80, 90, 100), 1.0, false);
    let missing = record_root_group_opacity_frame_artifact(
        &neutral_arena,
        &[neutral_root],
        &neutral_properties,
        &neutral_generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert_eq!(
        missing.reasons,
        vec![FrameArtifactFallbackReason::MissingRootEffect(neutral_root)]
    );

    let (arena, root, mut properties, generations) =
        prepared_leaf(0x6c22, Color::rgb(30, 200, 80), 0.5, false);
    let state = properties.states.get_mut(&root).unwrap();
    state.paint.transform = Some(TransformNodeId(root));
    state.descendants.transform = Some(TransformNodeId(root));
    let non_effect = record_root_group_opacity_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert!(
        non_effect
            .reasons
            .contains(&FrameArtifactFallbackReason::NonEffectProperty(root))
    );

    let (arena, root, mut properties, generations) =
        prepared_leaf(0x6c26, Color::rgb(30, 200, 80), 0.5, false);
    let state = properties.states.get_mut(&root).unwrap();
    state.paint.scroll = Some(crate::view::compositor::property_tree::ScrollNodeId(root));
    state.descendants.scroll = Some(crate::view::compositor::property_tree::ScrollNodeId(root));
    let scroll = record_root_group_opacity_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert!(
        scroll
            .reasons
            .contains(&FrameArtifactFallbackReason::NonEffectProperty(root))
    );

    let mut deferred = leaf_element(0x6c24, Color::rgb(30, 200, 80), 0.5, false);
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(1.0))
                .top(Length::px(1.0))
                .clip(ClipMode::Viewport),
        ),
    );
    style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
    deferred.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(deferred));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let deferred = record_root_group_opacity_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert!(
        deferred
            .reasons
            .contains(&FrameArtifactFallbackReason::DeferredBoundary(root)),
        "{deferred:?}"
    );
}

#[test]
fn root_group_compiler_rejects_missing_dangling_baked_and_double_applied_effects_before_emit() {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(leaf_element(0x6c30, Color::rgb(220, 30, 20), 0.5, false)),
    );
    let mut child_element = leaf_element(0x6c31, Color::rgb(20, 40, 220), 1.0, false);
    child_element.set_position(0.0, 0.0);
    let child = commit_child(&mut arena, root, Box::new(child_element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let (valid, _) = root_group_artifact(&arena, &[root], &properties, &generations);

    let mut missing = valid.clone();
    missing.effect_nodes.clear();
    assert_compiler_rejects_before_emit(&missing, "missing root group effect");

    let mut dangling = valid.clone();
    dangling.effect_nodes[0].parent = Some(EffectNodeId(NodeKey::null()));
    assert_compiler_rejects_before_emit(&dangling, "dangling root group effect");

    let mut baked = valid.clone();
    let PaintOp::DrawRect(op) = &mut baked.ops[0] else {
        panic!("root fixture begins with DrawRect")
    };
    op.params.opacity = 0.5;
    assert_compiler_rejects_before_emit(&baked, "baked root group opacity");

    let mut transformed = valid.clone();
    transformed.chunks[0].properties.transform = Some(TransformNodeId(root));
    assert_compiler_rejects_before_emit(&transformed, "root group transform property");

    let mut scrolled = valid.clone();
    scrolled.chunks[0].properties.scroll =
        Some(crate::view::compositor::property_tree::ScrollNodeId(root));
    assert_compiler_rejects_before_emit(&scrolled, "root group scroll property");

    let mut double_applied = valid;
    let child_chunk = double_applied
        .chunks
        .iter()
        .find(|chunk| chunk.owner == child)
        .unwrap();
    let PaintOp::DrawRect(op) = &mut double_applied.ops[child_chunk.op_range.start] else {
        panic!("child fixture begins with DrawRect")
    };
    op.params.opacity = 0.5;
    assert_compiler_rejects_before_emit(&double_applied, "double-applied child opacity");
}
