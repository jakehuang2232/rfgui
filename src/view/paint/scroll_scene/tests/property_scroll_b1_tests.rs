use super::*;

#[test]
fn property_scroll_admission_sidecar_correspondence_rejects_synchronized_hybrid() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let mut boundary = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
        generous_budget(),
    );
    let PropertyScrollHostAdmissionKind::DirectLeaf(direct) =
        boundary.planner.seal.admission.kind
    else {
        panic!("fixture must begin as the direct-leaf corpus")
    };
    let direct_admission = PropertyScrollHostAdmission::direct_leaf(direct);
    let sidecar = fake_text_area_sidecar_from_direct(direct);
    let text_area_admission = PropertyScrollHostAdmission::text_area_subtree(sidecar);
    let interactive_sidecar = RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot {
        boundary_root: sidecar.boundary_root,
        stable_id: sidecar.stable_id,
        content_wrapper: sidecar.content_wrapper,
        content_wrapper_stable_id: sidecar.content_wrapper_stable_id,
        text_area_root: sidecar.text_area_root,
        text_area_stable_id: sidecar.text_area_stable_id,
        paint_grammar: crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedGlyphs,
        caret_oracle_bounds_bits: None,
        source_bounds: sidecar.source_bounds,
        scroll: sidecar.scroll,
    };
    let interactive_admission =
        PropertyScrollHostAdmission::interactive_text_area_subtree(interactive_sidecar);
    let resident = RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs;
    let mut drifted_sidecar = sidecar;
    drifted_sidecar.text_area_stable_id += 1;
    let none = PropertyScrollPostCompositeSchedule::NoneForExistingGrammar;
    assert!(direct_admission.exactly_corresponds_to(None, None, &none));
    assert!(!direct_admission.exactly_corresponds_to(Some(sidecar), None, &none));
    assert!(!text_area_admission.exactly_corresponds_to(None, None, &none));
    assert!(text_area_admission.exactly_corresponds_to(Some(sidecar), None, &none));
    assert!(!text_area_admission.exactly_corresponds_to(Some(drifted_sidecar), None, &none,));
    assert!(direct_admission.exactly_corresponds_to_resident(None));
    assert!(!direct_admission.exactly_corresponds_to_resident(Some(&resident)));
    assert!(text_area_admission.exactly_corresponds_to_resident(None));
    assert!(!text_area_admission.exactly_corresponds_to_resident(Some(&resident)));
    assert!(!interactive_admission.exactly_corresponds_to_resident(None));
    assert!(interactive_admission.exactly_corresponds_to_resident(Some(&resident)));

    boundary.planner.seal.admission = text_area_admission.clone();
    boundary.planner.seal.planned_admission = text_area_admission.clone();
    boundary.seal.planner.admission = text_area_admission.clone();
    boundary.seal.compiler.admission = text_area_admission;
    assert!(boundary.planner.seal.text_area_subtree_admission.is_none());
    assert!(
        boundary
            .planner
            .seal
            .planned_text_area_subtree_admission
            .is_none()
    );
    assert!(!boundary.is_canonical());

    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    assert_eq!(
        prepare_retained_property_scroll_scene_from_pool(
            &mut viewport,
            boundary,
            &mut graph,
            ctx,
        )
        .err(),
        Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
}

#[test]
fn property_scroll_b1_overlay_grammar_preserves_all_three_exact_phases() {
    for scrollbar in ScrollbarCase::ALL {
        let (arena, root, _, properties, generations) = fixture_with_geometry_and_scrollbar(
            [0.0, 20.0],
            [100.0, 80.0],
            [300.0, 300.0],
            scrollbar,
            0.0,
        );
        let sampled_at = crate::time::Instant::now();
        let boundary = validated_property_scroll_boundary_from_fixture(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            generous_budget(),
        );
        assert!(boundary.is_canonical());
        let [
            PropertyScrollCompiledStep::HostBefore {
                artifact: host,
                parent_span: host_span,
                ..
            },
            PropertyScrollCompiledStep::DetachedContent {
                artifact: content,
                parent_before,
                parent_after,
                ..
            },
            PropertyScrollCompiledStep::OverlayAfter {
                artifact: overlay,
                parent_span: overlay_span,
                ..
            },
        ] = boundary.steps.as_slice()
        else {
            panic!("B1 grammar must be host/content/overlay");
        };
        assert_eq!(host.chunks.len(), 1);
        assert_eq!(content.chunks.len(), 1);
        assert_eq!(overlay.chunks.len(), 1);
        assert_eq!(overlay.chunks[0].id.role, PaintChunkRole::ScrollbarOverlay);
        assert_eq!(*parent_before, host_span.end);
        assert_eq!(*parent_after, *parent_before);
        assert_eq!(overlay_span.start, *parent_after);
        match scrollbar {
            ScrollbarCase::Hidden => {
                assert!(overlay.ops.is_empty());
                assert!(overlay.chunks[0].op_range.is_empty());
            }
            ScrollbarCase::Opaque | ScrollbarCase::Translucent => {
                assert!(!overlay.ops.is_empty());
            }
        }
    }
}

#[test]
fn property_scroll_b1_phase_artifact_injection_and_seal_tamper_fail_closed() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let boundary = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    );
    let artifacts = boundary
        .steps
        .iter()
        .map(|step| match step {
            PropertyScrollCompiledStep::HostBefore { artifact, .. }
            | PropertyScrollCompiledStep::DetachedContent { artifact, .. }
            | PropertyScrollCompiledStep::OverlayAfter { artifact, .. } => artifact.clone(),
            PropertyScrollCompiledStep::AtomicProjectionHostBefore { .. }
            | PropertyScrollCompiledStep::AtomicProjectionDetachedContent { .. }
            | PropertyScrollCompiledStep::AtomicProjectionOverlayAfter { .. }
            | PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore { .. }
            | PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent { .. }
            | PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter { .. } => {
                panic!("direct fixture cannot contain atomic projection authority")
            }
        })
        .collect::<Vec<_>>();
    for (target, source) in [(0, 1), (1, 2), (2, 0)] {
        let mut tampered = boundary.clone();
        match &mut tampered.steps[target] {
            PropertyScrollCompiledStep::HostBefore { artifact, .. }
            | PropertyScrollCompiledStep::DetachedContent { artifact, .. }
            | PropertyScrollCompiledStep::OverlayAfter { artifact, .. } => {
                *artifact = artifacts[source].clone();
            }
            PropertyScrollCompiledStep::AtomicProjectionHostBefore { .. }
            | PropertyScrollCompiledStep::AtomicProjectionDetachedContent { .. }
            | PropertyScrollCompiledStep::AtomicProjectionOverlayAfter { .. }
            | PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore { .. }
            | PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent { .. }
            | PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter { .. } => {
                panic!("direct fixture cannot contain atomic projection authority")
            }
        }
        assert!(!tampered.is_canonical());
    }
    let mut reordered = boundary.clone();
    reordered.steps.swap(0, 2);
    assert!(!reordered.is_canonical());

    let mut bad_store = boundary.clone();
    let PropertyScrollCompiledStep::OverlayAfter { artifact, .. } = &mut bad_store.steps[2]
    else {
        unreachable!();
    };
    artifact.chunks[0].op_range.end += 1;
    assert!(!bad_store.is_canonical());

    let mut bad_planner_seal = boundary.clone();
    bad_planner_seal.planner.seal.semantic.sampled_alpha_bits ^= 1;
    assert!(!bad_planner_seal.is_canonical());
    let mut bad_compiler_seal = boundary.clone();
    bad_compiler_seal.seal.compiler.semantic.sampled_alpha_bits ^= 1;
    assert!(!bad_compiler_seal.is_canonical());
}

#[test]
fn property_scroll_b1_single_and_tiled_stamps_are_owning_and_tamper_evident() {
    let sampled_at = crate::time::Instant::now();
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let single = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    );
    let (_, single_stamp, _, _, _, _) = compiled_content_step(&single);
    let PropertyScrollContentBackingCompileStamp::Single(single_backing) =
        &single_stamp.backing
    else {
        panic!("small B1 content must use single backing");
    };
    assert_eq!(single_backing.content, single_stamp.content);
    for tamper in [
        (|backing: &mut PropertyScrollSingleCompileStamp| {
            backing.color_key = PersistentTextureKey::Generic(0xb1)
        }) as fn(&mut PropertyScrollSingleCompileStamp),
        |backing| {
            backing.color_desc = backing
                .color_desc
                .clone()
                .with_size(backing.color_desc.width() + 1, backing.color_desc.height());
        },
        |backing| {
            backing.depth_desc = backing
                .depth_desc
                .clone()
                .with_size(backing.depth_desc.width() + 1, backing.depth_desc.height());
        },
        |backing| backing.pair_bytes += 1,
        |backing| backing.budget.max_active_pair_bytes = 1,
    ] {
        let mut bad_single = single.clone();
        let PropertyScrollCompiledStep::DetachedContent { stamp, .. } =
            &mut bad_single.steps[1]
        else {
            unreachable!();
        };
        let PropertyScrollContentBackingCompileStamp::Single(backing) = &mut stamp.backing
        else {
            unreachable!();
        };
        tamper(backing);
        assert!(!bad_single.is_canonical());
    }

    let (arena, root, _, properties, generations) =
        fixture_with_geometry([0.0, 1000.0], [100.0, 80.0], [300.0, 3000.0]);
    let tiled = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        tiled_budget(),
    );
    let (_, tiled_stamp, _, _, _, _) = compiled_content_step(&tiled);
    let PropertyScrollContentBackingCompileStamp::Tiled(tiled_backing) = &tiled_stamp.backing
    else {
        panic!("oversized B1 content must use tiled backing");
    };
    assert!(
        tiled_backing
            .tiles
            .windows(2)
            .all(|pair| pair[0].index < pair[1].index)
    );
    assert!(
        tiled_backing
            .tiles
            .iter()
            .all(|tile| tile.content == tiled_stamp.content)
    );
    assert_eq!(
        tiled_backing.total_pair_bytes,
        tiled_backing
            .tiles
            .iter()
            .map(|tile| tile.pair_bytes)
            .sum::<u64>()
    );
    for tamper in [
        (|backing: &mut PropertyScrollTiledCompileStamp| backing.tiles.swap(0, 1))
            as fn(&mut PropertyScrollTiledCompileStamp),
        |backing| backing.tiles[0].bounds.interior[0] += 1,
        |backing| backing.tiles[0].index.row += 1,
        |backing| backing.tiles[0].color_key = PersistentTextureKey::Generic(0xb1),
        |backing| {
            backing.tiles[0].color_desc = backing.tiles[0].color_desc.clone().with_size(
                backing.tiles[0].color_desc.width() + 1,
                backing.tiles[0].color_desc.height(),
            );
        },
        |backing| backing.tiles[0].pair_bytes += 1,
        |backing| backing.total_pair_bytes += 1,
        |backing| backing.gutter = 0,
        |backing| backing.overscan += 1,
        |backing| backing.tile_edge -= 1,
        |backing| backing.budget.max_active_pair_bytes = 1,
    ] {
        let mut bad = tiled.clone();
        let PropertyScrollCompiledStep::DetachedContent { stamp, .. } = &mut bad.steps[1]
        else {
            unreachable!();
        };
        let PropertyScrollContentBackingCompileStamp::Tiled(backing) = &mut stamp.backing
        else {
            unreachable!();
        };
        tamper(backing);
        assert!(!bad.is_canonical());
    }
}

#[test]
fn property_scroll_b1_offset_generation_and_alpha_identity_matrix() {
    let sampled_at = crate::time::Instant::now();
    let compile_offset = |offset| {
        let (arena, root, _, properties, generations) = fixture_at_offset(offset);
        validated_property_scroll_boundary_from_fixture(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            generous_budget(),
        )
    };
    let offset_a = compile_offset([0.0, 0.0]);
    let offset_b = compile_offset([0.0, 47.25]);
    let (_, stamp_a, composite_a, _, _, _) = compiled_content_step(&offset_a);
    let (_, stamp_b, composite_b, _, _, _) = compiled_content_step(&offset_b);
    assert_eq!(stamp_a, stamp_b);
    assert_ne!(composite_a, composite_b);

    let compile_tiled_offset = |offset| {
        let (arena, root, _, properties, generations) =
            fixture_with_geometry(offset, [100.0, 80.0], [300.0, 3000.0]);
        validated_property_scroll_boundary_from_fixture(
            &arena,
            root,
            &properties,
            &generations,
            sampled_at,
            tiled_budget(),
        )
    };
    let tiled_offset_a = compile_tiled_offset([0.0, 1000.0]);
    let tiled_offset_b = compile_tiled_offset([0.0, 1000.25]);
    assert_eq!(
        compiled_content_step(&tiled_offset_a).1,
        compiled_content_step(&tiled_offset_b).1
    );
    assert_ne!(
        compiled_content_step(&tiled_offset_a).2,
        compiled_content_step(&tiled_offset_b).2
    );

    let (arena, root, _, mut properties, mut generations) = fixture_at_offset([0.0, 20.0]);
    let generation_a = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    );
    properties
        .scrolls
        .get_mut(&crate::view::compositor::property_tree::ScrollNodeId(root))
        .unwrap()
        .generation += 1;
    generations.sync(&arena, &[root], &properties);
    let generation_b = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    );
    assert_ne!(
        generation_a.seal.compiler.scroll.generation,
        generation_b.seal.compiler.scroll.generation
    );
    assert_eq!(
        compiled_content_step(&generation_a).1,
        compiled_content_step(&generation_b).1
    );

    let (early_arena, early_root, early_properties, early_generations, early_time) =
        translucent_fixture_at(950);
    let (late_arena, late_root, late_properties, late_generations, late_time) =
        translucent_fixture_at(1_100);
    let early = validated_property_scroll_boundary_from_fixture(
        &early_arena,
        early_root,
        &early_properties,
        &early_generations,
        early_time,
        generous_budget(),
    );
    let late = validated_property_scroll_boundary_from_fixture(
        &late_arena,
        late_root,
        &late_properties,
        &late_generations,
        late_time,
        generous_budget(),
    );
    assert_eq!(
        compiled_content_step(&early).1,
        compiled_content_step(&late).1
    );
    assert_ne!(early.seal.compiler.semantic, late.seal.compiler.semantic);
    let PropertyScrollCompiledStep::OverlayAfter {
        dependency: early_overlay,
        ..
    } = &early.steps[2]
    else {
        unreachable!();
    };
    let PropertyScrollCompiledStep::OverlayAfter {
        dependency: late_overlay,
        ..
    } = &late.steps[2]
    else {
        unreachable!();
    };
    assert_ne!(early_overlay, late_overlay);
}

#[test]
fn property_scroll_b1_clip_detach_and_target_local_cursor_are_sealed() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let boundary = validated_property_scroll_boundary_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    );
    let (artifact, stamp, composite, clip_split, parent_before, parent_after) =
        compiled_content_step(&boundary);
    assert!(artifact.clip_nodes.is_empty());
    assert!(stamp.local_raster_clips.is_empty());
    assert!(clip_split.local_raster_clips.is_empty());
    assert_eq!(
        clip_split.own_contents_clip,
        boundary.seal.compiler.contents_clip
    );
    assert!(clip_split.ancestor_composite_clips.is_empty());
    assert_eq!(
        composite.contents_clip,
        clip_split.own_contents_clip.logical_scissor
    );
    assert_eq!(parent_before, parent_after);

    for tamper in [
        (|boundary: &mut ValidatedPropertyScrollBoundary| {
            let PropertyScrollCompiledStep::DetachedContent { stamp, .. } =
                &mut boundary.steps[1]
            else {
                unreachable!();
            };
            stamp.local_opaque_terminal += 1;
        }) as fn(&mut ValidatedPropertyScrollBoundary),
        |boundary| {
            let PropertyScrollCompiledStep::DetachedContent { clip_split, .. } =
                &mut boundary.steps[1]
            else {
                unreachable!();
            };
            clip_split
                .ancestor_composite_clips
                .push(clip_split.own_contents_clip);
        },
        |boundary| {
            let PropertyScrollCompiledStep::DetachedContent { parent_after, .. } =
                &mut boundary.steps[1]
            else {
                unreachable!();
            };
            *parent_after += 1;
        },
        |boundary| {
            let PropertyScrollCompiledStep::HostBefore { parent_span, .. } =
                &mut boundary.steps[0]
            else {
                unreachable!();
            };
            parent_span.end += 1;
        },
        |boundary| {
            let PropertyScrollCompiledStep::OverlayAfter { parent_span, .. } =
                &mut boundary.steps[2]
            else {
                unreachable!();
            };
            parent_span.start += 1;
        },
    ] {
        let mut bad = boundary.clone();
        tamper(&mut bad);
        assert!(!bad.is_canonical());
    }
}

#[test]
fn property_scroll_b1_stale_time_and_exact_live_tree_drift_fail_closed() {
    let (arena, root, _, mut properties, mut generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let plan = property_scroll_plan_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    )
    .unwrap();
    assert_eq!(
        validate_property_scroll_boundary(
            plan.clone(),
            &arena,
            &[root],
            &properties,
            &generations,
            sampled_at + crate::time::Duration::from_millis(1),
        )
        .err(),
        Some(PropertyScrollBoundaryValidationError::LiveSnapshotDrift)
    );

    properties.transforms.insert(
        crate::view::compositor::property_tree::TransformNodeId(root),
        crate::view::compositor::property_tree::TransformNode {
            owner: root,
            parent: None,
            viewport_matrix: glam::Mat4::IDENTITY,
            generation: 1,
        },
    );
    generations.sync(&arena, &[root], &properties);
    assert_eq!(
        validate_property_scroll_boundary(
            plan,
            &arena,
            &[root],
            &properties,
            &generations,
            sampled_at,
        )
        .err(),
        Some(PropertyScrollBoundaryValidationError::LiveSnapshotDrift)
    );
}
