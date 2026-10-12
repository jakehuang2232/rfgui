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
                &PaintRecordingContext::default(),
            )
            .expect("neutral direct host must record")
    }

    fn assert_owning_root_and_compile(artifact: PaintArtifact, owner: NodeKey) {
        assert!(matches!(
            artifact.owner_nodes.as_slice(),
            [PaintOwnerSnapshot {
                owner: root,
                parent: None,
                ..
            }] if *root == owner
        ));
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
