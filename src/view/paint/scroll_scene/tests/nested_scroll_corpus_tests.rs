use super::*;

#[test]
fn nested_scroll_compiler_seals_keyless_outer_assembly_and_one_leaf_resident() {
    let compiled = compiled_nested_scroll_fixture();
    assert!(compiled.is_canonical());
    assert_eq!(compiled.transaction_shape_for_test(), [1, 2, 0, 0, 1, 1]);
    assert_eq!(compiled.action_keys_for_test().len(), 1);
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &compiled.transaction.generic_authority
    else {
        panic!("dedicated nested authority")
    };
    assert_eq!(
        contract
            .compiled
            .steps
            .iter()
            .map(|step| step.phase)
            .collect::<Vec<_>>(),
        [
            NestedScrollCompilerPhase::OuterHostBefore,
            NestedScrollCompilerPhase::InnerHostBefore,
            NestedScrollCompilerPhase::LeafContent,
            NestedScrollCompilerPhase::InnerOverlayAfter,
            NestedScrollCompilerPhase::OuterOverlayAfter,
        ]
    );
    assert!(contract.compiled.leaf_artifact_span.clip_nodes.is_empty());
    assert!(
        contract
            .compiled
            .leaf_artifact_span
            .chunks
            .iter()
            .all(|chunk| chunk.clip.is_none())
    );
    assert_eq!(
        contract.compiled.leaf_source_bounds_bits,
        [0.0_f32, 0.0, 100.0, 600.0].map(f32::to_bits)
    );
    assert_eq!(
        compiled.action_keys_for_test(),
        FxHashSet::from_iter([compiled.leaf_stamp.identity.resident_key()])
    );
}

#[test]
fn nested_scroll_ready_exact_image_and_svg_leafs_compile_into_the_closed_r1_corpus() {
    for (kind, expected_role) in [
        (NestedMediaLeafKind::Image, PaintChunkRole::ImageContent),
        (NestedMediaLeafKind::Svg, PaintChunkRole::SvgContent),
    ] {
        let (arena, outer, _inner, leaf, properties, generations) =
            nested_scroll_media_fixture(kind);
        let compiled =
            compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);
        assert!(
            compiled.is_canonical(),
            "{kind:?} scene must remain canonical"
        );
        let scaffold = compiled.plan.nested_scroll_planning_scaffold().unwrap();
        let super::super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &scaffold.schedule.steps[2]
        else {
            panic!("{kind:?} fixture must retain one content receiver")
        };
        assert_eq!(receiver.witness.content_root(), leaf);
        assert_eq!(receiver.artifact.artifact().chunks.len(), 1);
        assert_eq!(
            receiver.artifact.artifact().chunks[0].id.role,
            expected_role
        );
        assert_eq!(receiver.artifact.artifact().owner_nodes.len(), 1);

        let geometry = prepare_nested_scroll_receiver_geometry(
            compiled,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap_or_else(|error| panic!("{kind:?} executable geometry rejected: {error:?}"));
        assert_eq!(geometry.scene.leaf_stamp.op_count, 2);
    }
}

#[test]
fn nested_scroll_ready_standalone_text_compiles_into_the_closed_r1_corpus() {
    let (arena, outer, _inner, leaf, properties, generations) = nested_scroll_text_fixture();
    let compiled =
        compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);
    assert!(compiled.is_canonical());
    let scaffold = compiled.plan.nested_scroll_planning_scaffold().unwrap();
    let super::super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &scaffold.schedule.steps[2]
    else {
        panic!("standalone Text fixture must retain one content receiver")
    };
    let artifact = receiver.artifact.artifact();
    assert_eq!(receiver.witness.content_root(), leaf);
    assert_eq!(artifact.chunks.len(), 1);
    assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::TextGlyphs);
    assert_eq!(artifact.chunks[0].id.slot, 0);
    let [super::super::super::PaintOp::PreparedText(text)] = artifact.ops.as_slice() else {
        panic!("standalone Text owns exactly one prepared glyph op")
    };
    assert_eq!(text.params.fragments.len(), 1);
    assert!(!text.params.staging_input.glyphs.is_empty());
    assert_eq!(
        text.params.staging_input.scale_factor.to_bits(),
        1.0_f32.to_bits()
    );
    assert!(text.params.scissor_rect.is_none());
    assert!(text.params.stencil_clip_id.is_none());
    assert!(
        text.params
            .staging_input
            .glyphs
            .iter()
            .all(|glyph| glyph.paint.fragment_index == 0)
    );

    let geometry = prepare_nested_scroll_receiver_geometry(
        compiled,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("standalone Text has canonical executable geometry");
    assert_eq!(geometry.scene.leaf_stamp.op_count, 1);
}

#[test]
fn nested_scroll_empty_text_leaf_fails_closed_after_layout() {
    let (mut arena, outer, _inner, leaf, _properties, _generations) =
        super::super::super::frame_plan::tests::nested_scroll_plan_fixture();
    let text = Text::new_with_id(0x1251_04, 0.0, 0.0, 100.0, 600.0, "");
    {
        let mut node = arena.get_mut(leaf).unwrap();
        *node.element = Box::new(text);
    }
    arena.refresh_stable_id_index();
    layout_nested_media_leaf(&mut arena, leaf);
    let node = arena.get(leaf).unwrap();
    let text = node.element.as_any().downcast_ref::<Text>().unwrap();
    assert!(!text.is_exact_standalone_retained_text_leaf());
    let outer_node = arena.get(outer).unwrap();
    let outer_element = outer_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    assert!(
        outer_element
            .exact_retained_nested_scroll_scene_admission(outer, &arena, 1.0)
            .is_none()
    );
}

#[test]
fn nested_scroll_ready_standalone_text_stamp_ignores_fractional_inner_offset() {
    let (arena, outer, inner, leaf, mut properties, mut generations) =
        nested_scroll_text_fixture();
    let baseline =
        compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);

    let host_origin = [35.0, 51.0];
    let outer_offset_y = 37.0;
    let inner_offset_y = 53.25;
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, outer);
        set_nested_scroll_position(&mut element, host_origin[0], host_origin[1]);
        element.set_scroll_offset((0.0, outer_offset_y));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, inner);
        set_nested_scroll_position(
            &mut element,
            host_origin[0],
            host_origin[1] - outer_offset_y,
        );
        element.set_scroll_offset((0.0, inner_offset_y));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut node = arena.get_mut(leaf).unwrap();
        node.element.translate_in_place(25.0, -59.25);
        node.element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(outer);
    properties.sync(&arena, &[outer]);
    generations.sync(&arena, &[outer], &properties);
    assert!(
        properties.validation_errors.is_empty(),
        "fractional Text property sync: {:?}",
        properties.validation_errors
    );
    let moved = compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);

    assert_eq!(moved.leaf_stamp, baseline.leaf_stamp);
    assert_eq!(
        moved.leaf_stamp.identity.resident_key(),
        baseline.leaf_stamp.identity.resident_key()
    );
    assert_eq!(moved.leaf_stamp.target, baseline.leaf_stamp.target);
}

#[test]
fn nested_scroll_ready_exact_image_and_svg_leaf_stamps_ignore_host_and_both_offsets() {
    for kind in [NestedMediaLeafKind::Image, NestedMediaLeafKind::Svg] {
        let (mut arena, outer, inner, leaf, mut properties, mut generations) =
            nested_scroll_media_fixture(kind);
        let baseline =
            compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);
        let baseline_payload = nested_media_payload_identity(&baseline);

        let host_origin = [35.0, 51.0];
        let outer_offset_y = 37.0;
        let inner_offset_y = 53.0;
        {
            let mut element =
                crate::view::test_support::get_element_mut::<Element>(&arena, outer);
            set_nested_scroll_position(&mut element, host_origin[0], host_origin[1]);
            element.set_scroll_offset((0.0, outer_offset_y));
            element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut element =
                crate::view::test_support::get_element_mut::<Element>(&arena, inner);
            set_nested_scroll_position(
                &mut element,
                host_origin[0],
                host_origin[1] - outer_offset_y,
            );
            element.set_scroll_offset((0.0, inner_offset_y));
            element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut node = arena.get_mut(leaf).unwrap();
            node.element.translate_in_place(25.0, -59.0);
            node.element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        prepare_nested_media_leaf(&mut arena, leaf, 3);
        arena.refresh_subtree_dirty_cache(outer);
        properties.sync(&arena, &[outer]);
        generations.sync(&arena, &[outer], &properties);
        let moved =
            compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);
        let moved_payload = nested_media_payload_identity(&moved);

        assert_eq!(moved.leaf_stamp, baseline.leaf_stamp, "{kind:?}");
        assert_eq!(
            (moved_payload.0, moved_payload.1, moved_payload.2),
            (baseline_payload.0, baseline_payload.1, baseline_payload.2),
            "{kind:?}: inherited clips and scroll offsets must not enter frozen upload identity"
        );
        assert_eq!(
            moved.leaf_stamp.identity.resident_key(),
            baseline.leaf_stamp.identity.resident_key(),
            "{kind:?}"
        );
        assert_eq!(
            moved.leaf_stamp.target, baseline.leaf_stamp.target,
            "{kind:?}"
        );
    }
}

#[test]
fn nested_scroll_leaf_stamp_is_invariant_to_host_and_both_scroll_offsets() {
    let (arena, outer, inner, leaf, mut properties, mut generations) =
        super::super::super::frame_plan::tests::nested_scroll_plan_fixture();
    let baseline =
        compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);

    move_nested_scroll_fixture(&arena, outer, inner, leaf);
    arena.refresh_subtree_dirty_cache(outer);
    properties.sync(&arena, &[outer]);
    generations.sync(&arena, &[outer], &properties);
    let moved = compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);

    assert_eq!(moved.leaf_stamp, baseline.leaf_stamp);
    assert_eq!(
        moved.leaf_stamp.identity.resident_key(),
        baseline.leaf_stamp.identity.resident_key()
    );
    assert_eq!(moved.leaf_stamp.target, baseline.leaf_stamp.target);
}

#[test]
fn nested_scroll_element_border_gradients_keep_full_leaf_stamp_under_nested_offsets() {
    let (arena, outer, inner, leaf, mut properties, mut generations) =
        super::super::super::frame_plan::tests::nested_scroll_plan_fixture();
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, leaf);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(20, 20, 20)),
        );
        style.set_border(Border::uniform(Length::px(3.0), &Color::hex("#102030")));
        style.set_background_image(nested_scroll_test_gradient("#ff0000", "#0000ff"));
        style.set_border_image(nested_scroll_test_gradient("#ffffff", "#000000"));
        element.apply_style(style);
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(outer);
    properties.sync(&arena, &[outer]);
    generations.sync(&arena, &[outer], &properties);
    let baseline =
        compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);
    assert!(baseline.is_canonical());
    assert!(baseline.leaf_stamp.op_count >= 2);

    move_nested_scroll_fixture(&arena, outer, inner, leaf);
    arena.refresh_subtree_dirty_cache(outer);
    properties.sync(&arena, &[outer]);
    generations.sync(&arena, &[outer], &properties);
    let moved = compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);

    assert_eq!(moved.leaf_stamp, baseline.leaf_stamp);
    assert_eq!(
        moved.leaf_stamp.identity.resident_key(),
        baseline.leaf_stamp.identity.resident_key()
    );
    assert_eq!(moved.leaf_stamp.target, baseline.leaf_stamp.target);
}
