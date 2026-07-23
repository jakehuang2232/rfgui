use super::*;

#[test]
fn text_area_projection_atomic_wrapper_is_transparent_and_matches_legacy() {
    assert_whole_frame_structural_parity(
        || {
            let (arena, roots, ..) = prepared_projection_text_area_tree();
            (arena, roots)
        },
        PaintParityConfig::default(),
    );

    let (arena, roots, root, projection, projected_text) = prepared_projection_text_area_tree();
    let projection_node = arena.get(projection).unwrap();
    assert_eq!(
        projection_node.element.shadow_paint_recording_capability(
            &arena,
            false,
            PaintRecordingContext {
                inside_text_area: true,
                ..Default::default()
            },
        ),
        ShadowPaintRecordingCapability::Transparent
    );
    drop(projection_node);

    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::TextGlyphs),
            (projected_text, PaintChunkRole::TextGlyphs),
        ]
    );
    assert!(
        artifact
            .owner_nodes
            .iter()
            .any(|snapshot| { snapshot.owner == projection && snapshot.parent == Some(root) })
    );
    assert!(
        artifact
            .owner_nodes
            .iter()
            .any(|snapshot| { snapshot.owner == projected_text && snapshot.parent.is_some() })
    );
    assert_eq!(
        artifact.chunks[1].properties,
        properties.node_state_for(projected_text).unwrap().paint
    );
    assert_eq!(
        artifact.chunks[1].properties.clip,
        properties.node_state_for(root).unwrap().descendants.clip
    );
}

#[test]
fn atomic_projection_emission_constructor_requires_the_full_canonical_stamp() {
    let (plan, stamp) =
        atomic_projection_emission_fixture_for_test("projected", 0xc3a_4100).unwrap();
    assert!(
        super::super::compiler::prepare_validated_scroll_scene_atomic_projection_text_area_emission(
            plan.clone(),
            &stamp,
        )
        .is_some()
    );

    let mut drifted = stamp;
    drifted.chunks[0].bounds_bits[0] ^= 1;
    assert!(
        super::super::compiler::prepare_validated_scroll_scene_atomic_projection_text_area_emission(
            plan, &drifted,
        )
        .is_none()
    );
}

#[test]
fn atomic_projection_reraster_emission_consumes_host_content_overlay_in_order() {
    let (plan, stamp) =
        atomic_projection_emission_fixture_for_test("projected", 0xc3a_4101).unwrap();
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    take_artifact_compile_count();

    let host =
        super::super::compiler::prepare_validated_scroll_scene_atomic_projection_text_area_emission(
            plan, &stamp,
        )
        .unwrap();
    let content = super::super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_host(
        host, &mut graph, &mut ctx,
    );
    let overlay =
        super::super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_content(
            content, &mut graph, &mut ctx,
        );
    super::super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_overlay(
        overlay, &mut graph, &mut ctx,
    );

    assert_eq!(take_artifact_compile_count(), 3);
}

#[test]
fn atomic_projection_reuse_emission_skips_only_detached_content_compile() {
    let (plan, stamp) =
        atomic_projection_emission_fixture_for_test("projected", 0xc3a_4102).unwrap();
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    take_artifact_compile_count();

    let host =
        super::super::compiler::prepare_validated_scroll_scene_atomic_projection_text_area_emission(
            plan, &stamp,
        )
        .unwrap();
    let content = super::super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_host(
        host, &mut graph, &mut ctx,
    );
    let overlay =
        super::super::compiler::reuse_validated_scroll_scene_atomic_projection_text_area_content(
            content,
        );
    super::super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_overlay(
        overlay, &mut graph, &mut ctx,
    );

    assert_eq!(take_artifact_compile_count(), 2);
}

#[test]
fn focused_atomic_projection_element_admission_is_graph_inert_and_exact() {
    for caret_visible in [true, false] {
        let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = caret_visible;
            text_area.cursor_char = 7;
        }
        let root_node = arena.get(root).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        let admission = root_element
            .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                root, &arena, 1.0,
            )
            .expect("focused atomic projection source must admit without planning");
        assert_eq!(
            (admission.content_wrapper, admission.text_area_root),
            (wrapper, text_area),
        );
        assert!(admission.paint_grammar.is_canonical());
        assert!(admission.bitwise_eq(&admission.clone()));
        assert_eq!(admission.paint_grammar.caret.caret_visible, caret_visible);
        assert!(matches!(
            (&admission.paint_grammar.caret.paint, caret_visible),
            (
                crate::view::base_component::text_area::FocusedAtomicCaretSourcePaintSeal::Present { .. },
                true,
            ) | (
                crate::view::base_component::text_area::FocusedAtomicCaretSourcePaintSeal::Hidden,
                false,
            )
        ));
        assert!(
            root_element
                .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                    root, &arena, 1.0,
                )
                .is_none(),
            "existing non-focused atomic admission must remain closed",
        );
        assert!(
            root_element
                .exact_retained_scroll_interactive_text_area_subtree_admission(
                    root, &arena, 1.0,
                )
                .is_none(),
            "generated-run interactive admission must remain projection-free",
        );
        drop(root_node);
        let (properties, _) = sync_identity(&arena, &[root]);
        let scroll = properties
            .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
            .unwrap();
        assert!(admission.matches_scroll_node(scroll));
    }
}

#[test]
fn focused_atomic_projection_local_recorder_suppresses_caret_into_post_fact() {
    for caret_visible in [true, false] {
        let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = caret_visible;
            text_area.cursor_char = 7;
        }
        let root_node = arena.get(root).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        let admission = root_element
            .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                root, &arena, 1.0,
            )
            .expect("focused atomic projection source must admit");
        drop(root_node);

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
        let local = super::super::frame_recorder::record_scroll_focused_atomic_projection_text_area_subtree_local_artifact_for_plan(
            &arena,
            &properties,
            &generations,
            &admission,
            outer,
        )
        .expect("focused atomic projection local recorder");

        assert!(local.is_canonical_for_test());
        assert_eq!(local.caret_for_test().caret_visible, caret_visible);
        assert_eq!(
            local
                .artifact_for_test()
                .chunks
                .iter()
                .map(|chunk| chunk.id.role)
                .collect::<Vec<_>>(),
            vec![
                PaintChunkRole::SelfDecoration,
                PaintChunkRole::SelfDecoration,
                PaintChunkRole::TextGlyphs,
                PaintChunkRole::TextGlyphs,
                PaintChunkRole::SelfDecoration,
            ],
            "caret must stay out of the mask-bounded resident local artifact",
        );
        assert!(
            !local
                .artifact_for_test()
                .chunks
                .iter()
                .any(|chunk| chunk.id.role == PaintChunkRole::Caret)
        );
    }
}

#[test]
fn focused_atomic_projection_host_local_plan_keeps_caret_out_of_resident() {
    for (caret_visible, cursor_char) in [(true, 0), (true, 7), (false, 7)] {
        let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = caret_visible;
            text_area.cursor_char = cursor_char;
        }
        let root_node = arena.get(root).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        let admission = root_element
            .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                root, &arena, 1.0,
            )
            .expect("focused source admission");
        assert!(
            root_element
                .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                    root, &arena, 1.0,
                )
                .is_none(),
            "focused path must not widen unfocused C3a admission",
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
        let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id)
            .expect("baked scroll host");
        let host = super::super::frame_recorder::record_baked_scroll_focused_atomic_projection_text_area_subtree_host_artifact_for_plan(
            &arena,
            &[root],
            &properties,
            &generations,
            &admission,
            baked,
        )
        .expect("focused host recorder");
        let local = super::super::frame_recorder::record_scroll_focused_atomic_projection_text_area_subtree_local_artifact_for_plan(
            &arena,
            &properties,
            &generations,
            &admission,
            outer,
        )
        .expect("focused local recorder");
        let plan =
            super::super::frame_recorder::validate_recorded_focused_atomic_projection_text_area_plan_parts(
                host, local,
            )
            .expect("focused plan parts");

        assert!(plan.is_canonical());
        assert_eq!(plan.caret_for_test().caret_visible, caret_visible);
        assert_eq!(
            plan.resident_for_test().source_grammar,
            admission.paint_grammar.atomic_source,
            "resident stamp must carry only the base atomic glyph grammar",
        );
    }
}

#[test]
fn focused_atomic_projection_scroll_scene_plan_is_canonical_and_live_exact() {
    for (caret_visible, cursor_char) in [(true, 0), (true, 7), (false, 7)] {
        let (arena, root, _, text_area) = prepared_atomic_projection_scroll_shell();
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = caret_visible;
            text_area.cursor_char = cursor_char;
        }
        let (properties, generations) = sync_identity(&arena, &[root]);
        let budget =
            super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
                .unwrap();
        let sampled_at = crate::time::Instant::now();
        let plan = super::super::scroll_scene::plan_property_scroll_scene_scaffold(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        )
        .expect("focused atomic projection shell must plan as a property-scroll scene");

        assert!(plan.is_canonical());
        assert!(plan.matches_live_inputs(
            &arena,
            &[root],
            &properties,
            &generations,
            sampled_at,
        ));
    }
}
