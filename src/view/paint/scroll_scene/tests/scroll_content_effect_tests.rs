use super::*;

#[test]
fn property_boundary_dag_compiler_validates_effect_transform_scroll_graph_inert_token() {
    for (arena, root, properties, generations) in [
        effect_transform_scroll_fixture(),
        effect_transform_scroll_neutral_fixture(),
    ] {
        let scene = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .expect("exact E->T->S compiler token");
        assert!(matches!(
            scene,
            ValidatedPropertyBoundaryDagScene::EffectTransformScroll(_)
        ));
        assert!(scene.is_canonical());
    }
}

#[test]
fn property_boundary_dag_compiler_validates_scroll_content_effect_graph_inert_tokens() {
    for (outer_transform, neutral_wrapper) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let (arena, root, properties, generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                outer_transform,
                neutral_wrapper,
            );
        let scene = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        );
        assert!(
            scene.is_ok(),
            "exact S->E/T->S->E graph-inert compiler token: outer_transform={outer_transform} neutral_wrapper={neutral_wrapper} error={:?}",
            scene.as_ref().err()
        );
        let scene = scene.unwrap();
        assert!(match (&scene, outer_transform) {
            (ValidatedPropertyBoundaryDagScene::ScrollEffect(_), false)
            | (ValidatedPropertyBoundaryDagScene::TransformScrollEffect(_), true) => true,
            _ => false,
        });
        assert!(scene.is_canonical());
        let validated = match &scene {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let (transaction, frozen) = freeze_scroll_content_effect_transaction(validated)
            .unwrap_or_else(|| {
                panic!(
                    "Phase3 E+C(+T) joint transaction: outer={outer_transform} neutral={neutral_wrapper}"
                )
            });
        assert!(transaction.is_canonical());
        assert_eq!(frozen.len(), 1);
        assert_eq!(
            transaction.generic_full_set.len(),
            if outer_transform { 2 } else { 1 }
        );
        assert_eq!(transaction.scroll_groups.len(), 1);
        assert_eq!(frozen[0].outer_stamp.is_some(), outer_transform);
        assert_eq!(
            frozen[0].effect_stamp,
            transaction.generic_full_set[usize::from(outer_transform)]
        );
        assert!(
            !super::super::super::compiler::property_effect_surface_raster_stamp_validates_contract_at_depth(
                &frozen[0].effect_stamp,
                &validated.roots[0].insertion.artifact_contract,
                0,
            ),
            "legacy E validator must reject non-empty scroll-normalization witnesses"
        );
        assert_eq!(
            frozen[0].content_stamp,
            transaction.scroll_groups[0].ordered_stamps()[0]
        );
        for scale_factor in [1.0_f32, 2.0] {
            let (effect, content) = freeze_scroll_content_effect_stamp_pair(
                &validated.roots[0],
                scale_factor.to_bits(),
                wgpu::TextureFormat::Bgra8UnormSrgb,
            )
            .expect("DPR1/2 E+C stamp pair");
            assert_eq!(
                effect.identity.role,
                super::super::RetainedSurfaceRasterRole::PropertyEffect
            );
            assert_eq!(
                content.identity.role,
                super::super::RetainedSurfaceRasterRole::ScrollContent
            );
            assert!(content.ordered_steps.iter().any(|step| {
                matches!(
                    step,
                    crate::view::paint::RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(
                        dependency
                    ) if dependency.child_stamp.as_ref() == &effect
                )
            }));
            let root = &validated.roots[0];
            let group = RetainedPropertyScrollResidentGroup {
                boundary: SceneBoundaryId {
                    ordinal: root.boundary.ordinal,
                    owner: root.boundary.scroll.owner,
                    kind: SceneBoundaryKind::ScrollContents,
                },
                content_root: root.insertion.content_root,
                content_stable_id: root.insertion.content_stable_id,
                signature: RetainedPropertyScrollGroupSignature {
                    content_bounds: exact_u32_bounds_from_bits(
                        content.target.source_bounds_bits,
                    )
                    .unwrap(),
                    tile_edge: SCROLL_CONTENT_TILE_EDGE,
                    gutter: SCROLL_CONTENT_TILE_GUTTER,
                    overscan: 0,
                    scale_factor_bits: scale_factor.to_bits(),
                    color_format: wgpu::TextureFormat::Bgra8UnormSrgb,
                },
                backing: RetainedPropertyScrollResidentBacking::Single(content.clone()),
            };
            assert!(
                !group.is_canonical(),
                "legacy C gate must not admit the Phase3 child dependency"
            );
            assert!(group.is_scroll_content_effect_canonical(
                &effect,
                &root.insertion.artifact_contract,
            ));
            let mut unrelated_effect = effect.clone();
            unrelated_effect.identity.stable_id ^= 1;
            assert!(!group.is_scroll_content_effect_canonical(
                &unrelated_effect,
                &root.insertion.artifact_contract,
            ));
        }
    }
}

#[test]
fn scroll_content_effect_prepare_freezes_atomic_cold_and_warm_actions() {
    for outer_transform in [false, true] {
        let (arena, root, properties, generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                outer_transform,
                false,
            );
        let scene = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let scene = match scene {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let mut viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let prepared = prepare_retained_scroll_content_effect_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.125, 0.25, 0.5, 1.0],
            owner,
        )
        .unwrap();
        assert_eq!(prepared.actions.len(), if outer_transform { 3 } else { 2 });
        assert!(
            prepared
                .actions
                .values()
                .all(|action| *action == RetainedSurfaceCompileAction::Reraster)
        );
        assert_eq!(prepared.graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            prepared
                .viewport
                .retained_surface_transaction_shape_for_test(),
            pool_before
        );
        let _outcome = emit_prepared_retained_scroll_content_effect_scene(prepared);
        assert_eq!(
            graph.test_graphics_passes::<ClearPass>().len(),
            if outer_transform { 4 } else { 3 }
        );
        assert_eq!(
            graph.test_graphics_passes::<TextureCompositePass>().len(),
            if outer_transform { 2 } else { 1 }
        );
        assert_eq!(graph.test_graphics_passes::<CompositeLayerPass>().len(), 1);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

        let (arena, root, properties, generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                outer_transform,
                false,
            );
        let warm_scene = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let warm_scene = match warm_scene {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut warm_graph = FrameGraph::new();
        let mut warm = prepare_retained_scroll_content_effect_scene_from_pool(
            &mut viewport,
            warm_scene,
            &mut warm_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.125, 0.25, 0.5, 1.0],
            warm_owner,
        )
        .unwrap();
        warm.refresh_actions_from_committed_test_pool();
        assert!(
            warm.actions
                .values()
                .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
        );
        assert_eq!(warm.graph.build_state_snapshot_for_test(), graph_before);
        let _outcome = emit_prepared_retained_scroll_content_effect_scene(warm);
        assert_eq!(warm_graph.test_graphics_passes::<ClearPass>().len(), 1);
        assert_eq!(
            warm_graph
                .test_graphics_passes::<TextureCompositePass>()
                .len(),
            1
        );
        assert!(
            warm_graph
                .test_graphics_passes::<CompositeLayerPass>()
                .is_empty()
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), false,)
        );
    }
}

#[test]
fn scroll_content_effect_transaction_action_and_prepare_tamper_fail_closed() {
    for outer_transform in [false, true] {
        let (arena, root, properties, generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                outer_transform,
                true,
            );
        let scene = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let mut scene = match scene {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let (transaction, frozen) = freeze_scroll_content_effect_transaction(&scene).unwrap();
        let mut actions = transaction
            .ordered_stamps()
            .into_iter()
            .map(|stamp| {
                (
                    stamp.identity.resident_key(),
                    RetainedSurfaceCompileAction::Reuse,
                )
            })
            .collect::<FxHashMap<_, _>>();
        actions.insert(
            frozen[0].effect_stamp.identity.resident_key(),
            RetainedSurfaceCompileAction::Reraster,
        );
        assert!(upgrade_scroll_content_effect_actions(&frozen, &mut actions).is_some());
        assert_eq!(
            actions.get(&frozen[0].content_stamp.identity.resident_key()),
            Some(&RetainedSurfaceCompileAction::Reraster)
        );
        if let Some(outer) = &frozen[0].outer_stamp {
            assert_eq!(
                actions.get(&outer.identity.resident_key()),
                Some(&RetainedSurfaceCompileAction::Reraster)
            );
        }
        let mut missing = actions.clone();
        missing.remove(&frozen[0].content_stamp.identity.resident_key());
        assert!(upgrade_scroll_content_effect_actions(&frozen, &mut missing).is_none());

        let mut missing_stamp = transaction.clone();
        missing_stamp.generic_full_set.pop();
        assert!(!missing_stamp.is_canonical());

        let mut descriptor = transaction.clone();
        let effect_index = usize::from(outer_transform);
        let depth = &descriptor.generic_full_set[effect_index].target.depth;
        descriptor.generic_full_set[effect_index].target.depth = TextureDesc::new(
            depth.width().saturating_add(1),
            depth.height(),
            depth.format(),
            wgpu::TextureDimension::D2,
        );
        assert!(!descriptor.is_canonical());

        let mut child = transaction.clone();
        let RetainedPropertyScrollResidentBacking::Single(content) =
            &mut child.scroll_groups[0].backing
        else {
            unreachable!()
        };
        let dependency = content
            .ordered_steps
            .iter_mut()
            .find_map(|step| match step {
                crate::view::paint::RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(child) => {
                    Some(child)
                }
                _ => None,
            })
            .unwrap();
        dependency.child_effect_generation =
            dependency.child_effect_generation.saturating_add(1);
        assert!(!child.is_canonical());

        let effect_index = usize::from(outer_transform);
        for tamper in ["missing", "extra", "kind", "stable", "topology"] {
            let mut witness = transaction.clone();
            let span = witness.generic_full_set[effect_index]
                .ordered_steps
                .iter_mut()
                .find_map(|step| match step {
                    crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                        Some(span)
                    }
                    _ => None,
                })
                .unwrap();
            match tamper {
                "missing" => {
                    span.scroll_placement_normalized_owners.pop();
                }
                "extra" => {
                    span.scroll_placement_normalized_owners
                        .push(span.scroll_placement_normalized_owners[0]);
                }
                "kind" => {
                    span.scroll_placement_normalized_owners[0].kind =
                        crate::view::base_component::RetainedScrollNormalizedPaintKind::Image;
                }
                "stable" => {
                    span.scroll_placement_normalized_owners[0].stable_id ^= 1;
                }
                "topology" => {
                    span.scroll_placement_normalized_owners[0].topology_revision += 1;
                }
                _ => unreachable!(),
            }
            assert!(!witness.is_canonical(), "typed witness {tamper} tamper");
        }

        if outer_transform {
            let mut geometry = transaction.clone();
            let RetainedPropertyScrollGenericAuthority::TransformScrollContentEffectCompiler(
                contracts,
            ) = &mut geometry.generic_authority
            else {
                unreachable!()
            };
            contracts[0].outer_geometry.source_bounds.width += 1.0;
            assert!(!geometry.is_canonical());
        }

        scene.roots[0].boundary.contents_clip.generation = 0;
        assert!(
            !scene.is_canonical(),
            "malformed Phase3 contents clip must fail before prepare mutates graph/pool"
        );
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        assert_eq!(
            prepare_retained_scroll_content_effect_scene_from_pool(
                &mut viewport,
                scene,
                &mut graph,
                UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0,),
                [0.125, 0.25, 0.5, 1.0],
                owner,
            )
            .err(),
            Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(viewport.retained_surface_frame_stage_owner_is_active(owner));
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));

        let (arena, root, properties, generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                outer_transform,
                true,
            );
        let scene = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let mut scene = match scene {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        scene.roots[0].boundary.scroll.offset.x = f32::NAN;
        assert!(!scene.is_canonical());
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        assert_eq!(
            prepare_retained_scroll_content_effect_scene_from_pool(
                &mut viewport,
                scene,
                &mut graph,
                UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0,),
                [0.125, 0.25, 0.5, 1.0],
                owner,
            )
            .err(),
            Some(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(viewport.retained_surface_frame_stage_owner_is_active(owner));
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
    }
}

#[test]
fn scroll_content_effect_emitter_accepts_only_forward_dependency_actions() {
    // E reuse + C reraster is the opacity/wrapper-paint shape. C reuse +
    // T reraster is an outer-only invalidation. The cold path covers the
    // complete E -> C -> T reraster chain.
    for mode in [0_u8, 1, 2] {
        let outer_transform = mode != 0;
        let (arena, root, properties, generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                outer_transform,
                false,
            );
        let scene = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let scene = match scene {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let mut prepared = prepare_retained_scroll_content_effect_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0; 4],
            owner,
        )
        .unwrap();
        let frozen = &prepared.roots[0].frozen;
        match mode {
            0 => {
                prepared.actions.insert(
                    frozen.effect_stamp.identity.resident_key(),
                    RetainedSurfaceCompileAction::Reuse,
                );
                // C remains reraster.
            }
            1 => {
                prepared.actions.insert(
                    frozen.effect_stamp.identity.resident_key(),
                    RetainedSurfaceCompileAction::Reuse,
                );
                prepared.actions.insert(
                    frozen.content_stamp.identity.resident_key(),
                    RetainedSurfaceCompileAction::Reuse,
                );
                // T remains reraster.
            }
            2 => {
                // Cold E/C/T reraster chain.
            }
            _ => unreachable!(),
        }
        let _ = emit_prepared_retained_scroll_content_effect_scene(prepared);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }
}

#[test]
fn scroll_content_effect_program_preserves_host_mask_content_mask_overlay_order() {
    for outer_transform in [false, true] {
        let (arena, root, properties, generations) =
            super::super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
                outer_transform,
                true,
            );
        let scene = PropertyBoundaryDagCompiler::plan_and_validate(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap();
        let scene = match scene {
            ValidatedPropertyBoundaryDagScene::ScrollEffect(scene)
            | ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => scene,
            _ => unreachable!(),
        };
        let root = &scene.roots[0];
        let [
            crate::view::paint::frame_recorder::RecordedTransformSurfaceStep::Artifact(host),
            crate::view::paint::frame_recorder::RecordedTransformSurfaceStep::Artifact(
                mask_begin,
            ),
            crate::view::paint::frame_recorder::RecordedTransformSurfaceStep::Boundary(content),
            crate::view::paint::frame_recorder::RecordedTransformSurfaceStep::Artifact(
                mask_end,
            ),
            crate::view::paint::frame_recorder::RecordedTransformSurfaceStep::Artifact(overlay),
        ] = root.scroll_host_steps.as_slice()
        else {
            panic!("Phase3 host program is H/mask/C/mask/O")
        };
        assert_eq!(*content, root.scroll_content_marker);
        let has = |artifact: &super::super::PaintArtifact, role, phase, slot| {
            artifact.chunks.iter().any(|chunk| {
                chunk.id.role == role && chunk.id.phase == phase && chunk.id.slot == slot
            })
        };
        assert!(has(
            host,
            crate::view::paint::PaintChunkRole::SelfDecoration,
            crate::view::paint::PaintNodePhase::BeforeChildren,
            0,
        ));
        assert!(has(
            mask_begin,
            crate::view::paint::PaintChunkRole::SelfDecoration,
            crate::view::paint::PaintNodePhase::BeforeChildren,
            crate::view::paint::artifact::RETAINED_CHILD_MASK_SLOT,
        ));
        assert!(has(
            mask_end,
            crate::view::paint::PaintChunkRole::SelfDecoration,
            crate::view::paint::PaintNodePhase::AfterChildren,
            crate::view::paint::artifact::RETAINED_CHILD_MASK_SLOT,
        ));
        assert!(has(
            overlay,
            crate::view::paint::PaintChunkRole::ScrollbarOverlay,
            crate::view::paint::PaintNodePhase::AfterChildren,
            0,
        ));
        assert_eq!(
            root.receiver_steps
                .iter()
                .filter(|step| matches!(step, crate::view::paint::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) if *marker == root.insertion.effect_cutout))
                .count(),
            1,
            "C owns exactly one E insertion between its artifact spans",
        );
    }
}
