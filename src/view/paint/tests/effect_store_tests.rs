use super::*;

#[test]
fn effect_snapshots_preserve_zero_half_one_and_parent_child_baked_semantics() {
    let (nested, parent, child) = compiler_effect_test_artifact(0.5, 0.25);
    assert_eq!(nested.effect_nodes.len(), 2);
    let parent_snapshot = nested
        .effect_nodes
        .iter()
        .find(|snapshot| snapshot.owner == parent)
        .expect("parent effect snapshot");
    let child_snapshot = nested
        .effect_nodes
        .iter()
        .find(|snapshot| snapshot.owner == child)
        .expect("child effect snapshot");
    assert_eq!(parent_snapshot.opacity.to_bits(), 0.5_f32.to_bits());
    assert_eq!(child_snapshot.opacity.to_bits(), 0.25_f32.to_bits());
    assert_eq!(child_snapshot.parent, Some(parent_snapshot.id));
    assert_eq!(
        compiled_whole_frame_graph(&nested)
            .test_rect_pass_snapshots()
            .len(),
        2
    );

    let (inherited, _, inherited_child) = compiler_effect_test_artifact(0.5, 1.0);
    assert_eq!(inherited.effect_nodes.len(), 1);
    let child_chunk = inherited
        .chunks
        .iter()
        .find(|chunk| chunk.owner == inherited_child)
        .expect("inherited child chunk");
    let PaintOp::DrawRect(child_op) = &inherited.ops[child_chunk.op_range.start] else {
        panic!("Element child must record DrawRect")
    };
    assert_eq!(child_op.params.opacity.to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        compiled_whole_frame_graph(&inherited)
            .test_rect_pass_snapshots()
            .len(),
        2
    );

    let (zero, _, _) = compiler_effect_test_artifact(0.0, 1.0);
    assert!(
        zero.effect_nodes
            .iter()
            .any(|snapshot| snapshot.opacity.to_bits() == 0.0_f32.to_bits())
    );
    let zero_snapshots = compiled_whole_frame_graph(&zero).test_rect_pass_snapshots();
    assert_eq!(zero_snapshots.len(), 2);
    assert_eq!(zero_snapshots[0].opacity_bits, 0.0_f32.to_bits());
    assert_eq!(zero_snapshots[1].opacity_bits, 1.0_f32.to_bits());
}

#[test]
fn compiler_rejects_every_invalid_effect_store_before_any_emit() {
    let (valid, parent, child) = compiler_effect_test_artifact(0.5, 0.25);

    let mut missing = valid.clone();
    missing.effect_nodes.clear();
    assert_compiler_rejects_before_emit(&missing, "missing effect leaf");

    let mut duplicate = valid.clone();
    duplicate.effect_nodes.push(duplicate.effect_nodes[0]);
    assert_compiler_rejects_before_emit(&duplicate, "duplicate effect id");

    let mut wrong_owner = valid.clone();
    wrong_owner.effect_nodes[0].owner = NodeKey::null();
    assert_compiler_rejects_before_emit(&wrong_owner, "wrong effect owner");

    let mut generation_zero = valid.clone();
    generation_zero.effect_nodes[0].generation = 0;
    assert_compiler_rejects_before_emit(&generation_zero, "zero effect generation");

    let mut non_finite = valid.clone();
    non_finite.effect_nodes[0].opacity = f32::NAN;
    assert_compiler_rejects_before_emit(&non_finite, "non-finite effect opacity");

    let mut out_of_range = valid.clone();
    out_of_range.effect_nodes[0].opacity = 1.25;
    assert_compiler_rejects_before_emit(&out_of_range, "out-of-range effect opacity");

    let mut dangling = valid.clone();
    let child_index = dangling
        .effect_nodes
        .iter()
        .position(|snapshot| snapshot.owner == child)
        .unwrap();
    dangling.effect_nodes[child_index].parent = Some(EffectNodeId(NodeKey::null()));
    assert_compiler_rejects_before_emit(&dangling, "dangling effect parent");

    let mut cycle = valid.clone();
    let parent_index = cycle
        .effect_nodes
        .iter()
        .position(|snapshot| snapshot.owner == parent)
        .unwrap();
    cycle.effect_nodes[parent_index].parent = Some(EffectNodeId(child));
    assert_compiler_rejects_before_emit(&cycle, "effect cycle");

    let mut wrong_ref = valid.clone();
    wrong_ref.chunks[0].properties.effect = Some(EffectNodeId(NodeKey::null()));
    assert_compiler_rejects_before_emit(&wrong_ref, "missing chunk effect ref");

    let mut unreferenced = valid.clone();
    let mut key_arena = NodeArena::new();
    let extra_owner = key_arena.insert(Node::new(Box::new(Element::new_with_id(
        0x6bff, 0.0, 0.0, 1.0, 1.0,
    ))));
    unreferenced.effect_nodes.push(EffectNodeSnapshot {
        id: EffectNodeId(extra_owner),
        owner: extra_owner,
        parent: None,
        opacity: 0.75,
        generation: 1,
    });
    assert_compiler_rejects_before_emit(&unreferenced, "unreferenced effect node");

    let mut baked_mismatch = valid;
    let PaintOp::DrawRect(first) = &mut baked_mismatch.ops[0] else {
        panic!("Element fixture must start with DrawRect")
    };
    first.params.opacity = 1.0;
    assert_compiler_rejects_before_emit(&baked_mismatch, "baked local opacity mismatch");
}

#[test]
fn compiler_requires_effect_owners_to_follow_canonical_owner_ancestry() {
    let (sibling_valid, parent, first, second) = compiler_sibling_effect_artifact();
    let first_effect = sibling_valid
        .effect_nodes
        .iter()
        .find(|snapshot| snapshot.owner == first)
        .unwrap()
        .id;
    let second_effect = sibling_valid
        .effect_nodes
        .iter()
        .find(|snapshot| snapshot.owner == second)
        .unwrap()
        .id;

    let mut sibling_rebind = sibling_valid.clone();
    sibling_rebind
        .chunks
        .iter_mut()
        .find(|chunk| chunk.owner == first)
        .unwrap()
        .properties
        .effect = Some(second_effect);
    sibling_rebind
        .chunks
        .iter_mut()
        .find(|chunk| chunk.owner == second)
        .unwrap()
        .properties
        .effect = Some(first_effect);
    assert_compiler_rejects_before_emit(&sibling_rebind, "sibling effect rebind");

    let (mut descendant_rebind, _, descendant) = compiler_effect_test_artifact(0.5, 0.25);
    descendant_rebind.chunks[0].properties.effect = Some(EffectNodeId(descendant));
    assert_compiler_rejects_before_emit(&descendant_rebind, "descendant effect rebind");

    let mut unrelated_arena = new_test_arena();
    let first_root = commit_element(
        &mut unrelated_arena,
        Box::new(leaf_element(0x6b40, Color::rgb(10, 20, 30), 0.5, false)),
    );
    let second_root = commit_element(
        &mut unrelated_arena,
        Box::new(leaf_element(0x6b41, Color::rgb(40, 50, 60), 0.25, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut unrelated_arena, first_root, measure, place);
    measure_and_place(&mut unrelated_arena, second_root, measure, place);
    let unrelated_roots = [first_root, second_root];
    let (unrelated_properties, unrelated_generations) =
        sync_identity(&unrelated_arena, &unrelated_roots);
    let mut unrelated = whole_frame_artifact(
        &unrelated_arena,
        &unrelated_roots,
        &unrelated_properties,
        &unrelated_generations,
    )
    .0;
    unrelated.chunks[0].properties.effect = Some(EffectNodeId(second_root));
    unrelated.chunks[1].properties.effect = Some(EffectNodeId(first_root));
    assert_compiler_rejects_before_emit(&unrelated, "unrelated-root effect rebind");

    let mut wrong_parent = sibling_valid;
    wrong_parent
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == first)
        .unwrap()
        .parent = Some(second);
    // The store is complete and acyclic, but this claimed parent conflicts
    // with the effect chain captured for the same canonical frame walk.
    assert_compiler_rejects_before_emit(&wrong_parent, "wrong traversal parent");
    let _ = parent;
}

#[test]
fn compiler_rejects_invalid_owner_store_and_late_failure_before_emit() {
    let (valid, _, first, second) = compiler_sibling_effect_artifact();

    let mut missing = valid.clone();
    missing
        .owner_nodes
        .retain(|snapshot| snapshot.owner != first);
    assert_compiler_rejects_before_emit(&missing, "missing chunk owner");

    let mut duplicate = valid.clone();
    duplicate.owner_nodes.push(duplicate.owner_nodes[0]);
    assert_compiler_rejects_before_emit(&duplicate, "duplicate chunk owner");

    let mut null_owner = valid.clone();
    let original_owner = null_owner.chunks[0].owner;
    null_owner.chunks[0].id.owner = NodeKey::null();
    null_owner.chunks[0].owner = NodeKey::null();
    null_owner
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == original_owner)
        .expect("first chunk owner snapshot")
        .owner = NodeKey::null();
    assert_compiler_rejects_before_emit(&null_owner, "null chunk owner");

    let mut cycle = valid.clone();
    cycle
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == first)
        .unwrap()
        .parent = Some(first);
    assert_compiler_rejects_before_emit(&cycle, "owner cycle");

    let mut dangling = valid.clone();
    dangling
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == first)
        .unwrap()
        .parent = Some(NodeKey::null());
    assert_compiler_rejects_before_emit(&dangling, "missing owner parent");

    let mut unreferenced = valid.clone();
    let mut key_arena = NodeArena::new();
    let extra = key_arena.insert(Node::new(Box::new(Element::new_with_id(
        0x6b42, 0.0, 0.0, 1.0, 1.0,
    ))));
    unreferenced.owner_nodes.push(PaintOwnerSnapshot {
        owner: extra,
        parent: None,
    });
    assert_compiler_rejects_before_emit(&unreferenced, "unreferenced owner node");

    let mut late_invalid = valid;
    late_invalid
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == second)
        .unwrap()
        .parent = Some(second);
    assert_compiler_rejects_before_emit(&late_invalid, "late invalid owner cycle");
}

#[test]
fn compiler_requires_complete_nearest_active_effect_chain() {
    let (valid, grandparent, parent, child) = compiler_three_level_effect_artifact();
    let mut skip_nearest = valid.clone();
    skip_nearest
        .chunks
        .iter_mut()
        .find(|chunk| chunk.owner == child)
        .unwrap()
        .properties
        .effect = Some(EffectNodeId(grandparent));
    assert_compiler_rejects_before_emit(&skip_nearest, "skipped nearest parent effect");

    let mut missing_inherited = valid;
    missing_inherited
        .chunks
        .iter_mut()
        .find(|chunk| chunk.owner == child)
        .unwrap()
        .properties
        .effect = None;
    assert_compiler_rejects_before_emit(&missing_inherited, "missing inherited effect");

    let _ = parent;
}

#[test]
fn compiler_checks_baked_opacity_for_text_image_svg_and_decorations() {
    let (text_arena, text_roots, _) = prepared_text_tree(false);
    let (text_properties, text_generations) = sync_identity(&text_arena, &text_roots);
    let mut text = whole_frame_artifact(
        &text_arena,
        &text_roots,
        &text_properties,
        &text_generations,
    )
    .0;
    let PaintOp::PreparedText(text_op) = &mut text.ops[0] else {
        panic!("text fixture must record PreparedText")
    };
    text_op.params.staging_input.glyphs[0].paint.opacity = 1.0;
    assert_compiler_rejects_before_emit(&text, "prepared text glyph opacity mismatch");

    let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
    let (image_arena, image_roots) = bare_image_fixture(
        pixels.clone(),
        crate::view::ImageFit::Fill,
        crate::view::ImageSampling::Linear,
        0.5,
    );
    let (image_properties, image_generations) = sync_identity(&image_arena, &image_roots);
    let mut image = whole_frame_artifact(
        &image_arena,
        &image_roots,
        &image_properties,
        &image_generations,
    )
    .0;
    let PaintOp::PreparedImage(image_op) = image.ops.last_mut().unwrap() else {
        panic!("image fixture must record PreparedImage")
    };
    image_op.params.opacity = 1.0;
    image.chunks[0].payload_identity = PaintPayloadIdentity::image_with_decoration(
        PreparedImageIdentity::from_op(image_op),
        std::iter::empty(),
    )
    .unwrap();
    assert_compiler_rejects_before_emit(&image, "prepared image opacity mismatch");

    let (svg_arena, svg_roots) = bare_image_fixture(
        pixels,
        crate::view::ImageFit::Fill,
        crate::view::ImageSampling::Linear,
        0.5,
    );
    let (svg_properties, svg_generations) = sync_identity(&svg_arena, &svg_roots);
    let mut svg =
        whole_frame_artifact(&svg_arena, &svg_roots, &svg_properties, &svg_generations).0;
    svg.chunks[0].id.role = PaintChunkRole::SvgContent;
    let PaintOp::PreparedImage(image_op) = svg.ops.last_mut().unwrap() else {
        panic!("source fixture must record PreparedImage")
    };
    image_op.upload.id = crate::view::sampled_texture::SampledTextureId::SvgRaster(
        crate::view::sampled_texture::SvgRasterAssetId::for_test(0x6b11),
    );
    let mut svg_op = PreparedSvgOp {
        params: image_op.params,
        upload: image_op.upload.clone(),
    };
    svg_op.params.opacity = 1.0;
    svg.chunks[0].payload_identity = PaintPayloadIdentity::svg_with_decoration(
        PreparedSvgIdentity::from_op(&svg_op).expect("typed SVG identity"),
        std::iter::empty(),
    )
    .unwrap();
    *svg.ops.last_mut().unwrap() = PaintOp::PreparedSvg(svg_op);
    assert_compiler_rejects_before_emit(&svg, "prepared SVG opacity mismatch");

    let (decorated_arena, decorated_roots) = prepared_image_fixture(
        Arc::from([255_u8; 16]),
        crate::view::ImageFit::Fill,
        crate::view::ImageSampling::Linear,
        0.5,
    );
    let (decorated_properties, decorated_generations) =
        sync_identity(&decorated_arena, &decorated_roots);
    let mut decorated = whole_frame_artifact(
        &decorated_arena,
        &decorated_roots,
        &decorated_properties,
        &decorated_generations,
    )
    .0;
    let PaintOp::DrawRect(decoration) = &mut decorated.ops[0] else {
        panic!("decorated image must begin with DrawRect")
    };
    decoration.params.opacity = 1.0;
    assert_compiler_rejects_before_emit(&decorated, "image decoration opacity mismatch");
}

#[test]
fn neutral_element_text_and_image_direct_artifacts_compile_after_arena_drop() {
    fn record_direct(arena: &NodeArena, owner: NodeKey) -> PaintArtifact {
        arena
            .get(owner)
            .expect("direct host exists")
            .element
            .record_shadow_paint_artifact(
                owner,
                PropertyTreeState::default(),
                PaintContentRevision {
                    self_paint_revision: 1,
                    composite_revision: 1,
                    topology_revision: 1,
                },
                arena,
                PaintRecordingContext::default(),
            )
            .expect("neutral direct host must record")
    }

    fn assert_owning_root_and_compile(artifact: PaintArtifact, owner: NodeKey) {
        assert_eq!(
            artifact.owner_nodes,
            vec![PaintOwnerSnapshot {
                owner,
                parent: None,
            }]
        );
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        assert!(try_compile_artifact(&artifact, &mut graph, ctx).is_ok());
    }

    let mut element_arena = new_test_arena();
    let element = commit_element(
        &mut element_arena,
        Box::new(leaf_element(0x6b50, Color::rgb(10, 20, 30), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut element_arena, element, measure, place);
    let element_artifact = record_direct(&element_arena, element);
    drop(element_arena);
    assert_owning_root_and_compile(element_artifact, element);

    let mut text_arena = new_test_arena();
    let mut text = Text::new_with_id(0x6b51, 0.0, 0.0, 120.0, 40.0, "owning text");
    text.set_opacity(1.0);
    let text = commit_element(&mut text_arena, Box::new(text));
    measure_and_place(&mut text_arena, text, measure, place);
    let text_artifact = record_direct(&text_arena, text);
    drop(text_arena);
    assert_owning_root_and_compile(text_artifact, text);

    let (image_arena, image_roots) = bare_image_fixture(
        Arc::from([255_u8; 16]),
        crate::view::ImageFit::Fill,
        crate::view::ImageSampling::Linear,
        1.0,
    );
    let image = image_roots[0];
    let image_artifact = record_direct(&image_arena, image);
    drop(image_arena);
    assert_owning_root_and_compile(image_artifact, image);
}
