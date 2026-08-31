use super::*;
use crate::view::paint::{
    ArtifactSurfaceRasterContext, ArtifactSurfaceRasterPlanError, ArtifactSurfaceResidentSealError,
    FrameArtifactFallbackReason, FrameArtifactRecordOutcome, LegacyPaintReason, RendererMode,
    RetainedSurfaceCompileAction, RetainedSurfaceRasterRole, SingleTargetSurfaceDagPrepareError,
    prepare_artifact_surface_raster_plan, record_surface_dag_frame_artifact,
    seal_prepared_artifact_surface_frame,
};

const MEASUREMENT_BUDGET_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy)]
enum ProducerCase {
    NativeTransform {
        label: &'static str,
        host: &'static str,
        state: &'static str,
    },
    NativeEffect {
        label: &'static str,
        host: &'static str,
        state: &'static str,
    },
    CoLocatedTransformEffect,
}

impl ProducerCase {
    fn label(self) -> &'static str {
        match self {
            Self::NativeTransform { label, .. } | Self::NativeEffect { label, .. } => label,
            Self::CoLocatedTransformEffect => "co-located-transform-effect",
        }
    }

    fn build(self) -> (NodeArena, Vec<NodeKey>) {
        match self {
            Self::NativeTransform { host, state, .. } => {
                prepared_native_media_transform(host, state)
            }
            Self::NativeEffect { host, state, .. } => {
                let (arena, roots, _) = prepared_nested_native_effect(host, state);
                (arena, roots)
            }
            Self::CoLocatedTransformEffect => {
                let (arena, roots, _, _, _) = prepared_transform_child_isolation_tree();
                (arena, roots)
            }
        }
    }
}

const CASES: [ProducerCase; 14] = [
    ProducerCase::NativeTransform {
        label: "transform-image-ready",
        host: "Image",
        state: "ready",
    },
    ProducerCase::NativeTransform {
        label: "transform-image-loading",
        host: "Image",
        state: "loading",
    },
    ProducerCase::NativeTransform {
        label: "transform-image-error",
        host: "Image",
        state: "error",
    },
    ProducerCase::NativeTransform {
        label: "transform-svg-ready",
        host: "Svg",
        state: "ready",
    },
    ProducerCase::NativeTransform {
        label: "transform-svg-loading",
        host: "Svg",
        state: "loading",
    },
    ProducerCase::NativeTransform {
        label: "transform-svg-error",
        host: "Svg",
        state: "error",
    },
    ProducerCase::NativeEffect {
        label: "effect-text-ready",
        host: "Text",
        state: "ready",
    },
    ProducerCase::NativeEffect {
        label: "effect-image-ready",
        host: "Image",
        state: "ready",
    },
    ProducerCase::NativeEffect {
        label: "effect-image-loading",
        host: "Image",
        state: "loading",
    },
    ProducerCase::NativeEffect {
        label: "effect-image-error",
        host: "Image",
        state: "error",
    },
    ProducerCase::NativeEffect {
        label: "effect-svg-ready",
        host: "Svg",
        state: "ready",
    },
    ProducerCase::NativeEffect {
        label: "effect-svg-loading",
        host: "Svg",
        state: "loading",
    },
    ProducerCase::NativeEffect {
        label: "effect-svg-error",
        host: "Svg",
        state: "error",
    },
    ProducerCase::CoLocatedTransformEffect,
];

#[derive(Debug, PartialEq, Eq)]
enum ProducerMeasurementOutcome {
    RecordRejected(Vec<FrameArtifactFallbackReason>),
    RecordFallback(Vec<FrameArtifactFallbackReason>),
    PlanRejected(ArtifactSurfaceRasterPlanError),
    SealRejected(ArtifactSurfaceResidentSealError),
    Prepared {
        roles: Vec<RetainedSurfaceRasterRole>,
        pair_bytes: Vec<u64>,
        aggregate_bytes: u64,
        resident_count: usize,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct ProducerMeasurementRow {
    label: &'static str,
    dpr_bits: u32,
    outcome: ProducerMeasurementOutcome,
}

fn measure_case(case: ProducerCase, dpr: f32) -> ProducerMeasurementRow {
    let label = case.label();
    let (arena, roots) = case.build();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    measure_recorded_scene(label, dpr, &arena, &roots, &properties, &generations)
}

fn measure_recorded_scene(
    label: &'static str,
    dpr: f32,
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> ProducerMeasurementRow {
    let outcome = match record_surface_dag_frame_artifact(
        arena,
        roots,
        properties,
        generations,
        RendererMode::ForcedForTests,
    ) {
        Err(error) => ProducerMeasurementOutcome::RecordRejected(error.reasons),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            ProducerMeasurementOutcome::RecordFallback(eligibility.reasons)
        }
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => {
            let context = ArtifactSurfaceRasterContext::new(
                dpr,
                wgpu::TextureFormat::Bgra8Unorm,
                [0.0, 0.0],
                None,
                4096,
                MEASUREMENT_BUDGET_BYTES,
            )
            .expect("measurement raster context");
            match prepare_artifact_surface_raster_plan(artifact, context) {
                Err(error) => ProducerMeasurementOutcome::PlanRejected(error),
                Ok(plan) => {
                    let roles = plan
                        .nodes()
                        .iter()
                        .map(|node| node.identity().role)
                        .collect::<Vec<_>>();
                    let pair_bytes = plan
                        .nodes()
                        .iter()
                        .map(|node| {
                            let target = node.target();
                            crate::view::raster_cost::texture_desc_payload_bytes(&target.color)
                                .bytes
                                .checked_add(
                                    crate::view::raster_cost::texture_desc_payload_bytes(
                                        &target.depth,
                                    )
                                    .bytes,
                                )
                                .expect("one measured descriptor pair fits u64")
                        })
                        .collect::<Vec<_>>();
                    let aggregate_bytes = pair_bytes.iter().copied().fold(0_u64, |total, bytes| {
                        total
                            .checked_add(bytes)
                            .expect("one measured frame aggregate fits u64")
                    });
                    match seal_prepared_artifact_surface_frame(plan) {
                        Err(error) => ProducerMeasurementOutcome::SealRejected(error),
                        Ok(frame) => ProducerMeasurementOutcome::Prepared {
                            roles,
                            pair_bytes,
                            aggregate_bytes,
                            resident_count: frame.residents().len(),
                        },
                    }
                }
            }
        }
    };
    ProducerMeasurementRow {
        label,
        dpr_bits: dpr.to_bits(),
        outcome,
    }
}

fn prepared_recorded_scene(
    dpr: f32,
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> crate::view::paint::PreparedArtifactSurfaceFrame {
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        arena,
        roots,
        properties,
        generations,
        RendererMode::ForcedForTests,
    )
    .expect("surface DAG scene must record") else {
        panic!("forced surface DAG scene cannot silently fall back")
    };
    let context = ArtifactSurfaceRasterContext::new(
        dpr,
        wgpu::TextureFormat::Bgra8Unorm,
        [0.0, 0.0],
        None,
        4096,
        MEASUREMENT_BUDGET_BYTES,
    )
    .expect("surface DAG test raster context");
    let plan = prepare_artifact_surface_raster_plan(artifact, context)
        .expect("surface DAG scene must prepare");
    seal_prepared_artifact_surface_frame(plan).expect("surface DAG scene must seal")
}

fn prepared_row(
    label: &'static str,
    dpr: f32,
    roles: &[RetainedSurfaceRasterRole],
    pair_bytes: &[u64],
) -> ProducerMeasurementRow {
    ProducerMeasurementRow {
        label,
        dpr_bits: dpr.to_bits(),
        outcome: ProducerMeasurementOutcome::Prepared {
            roles: roles.to_vec(),
            pair_bytes: pair_bytes.to_vec(),
            aggregate_bytes: pair_bytes.iter().copied().sum(),
            resident_count: roles.len(),
        },
    }
}

fn missing_paint_identity_row(label: &'static str, dpr: f32) -> ProducerMeasurementRow {
    ProducerMeasurementRow {
        label,
        dpr_bits: dpr.to_bits(),
        outcome: ProducerMeasurementOutcome::RecordRejected(vec![
            FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::MissingPaintIdentity),
        ]),
    }
}

fn assert_invalid_store_row(row: &ProducerMeasurementRow, label: &'static str, dpr: f32) {
    assert_eq!((row.label, row.dpr_bits), (label, dpr.to_bits()));
    assert_eq!(
        row.outcome,
        ProducerMeasurementOutcome::PlanRejected(ArtifactSurfaceRasterPlanError::ArtifactProgram(
            SingleTargetSurfaceDagPrepareError::InvalidArtifactStore,
        ),),
    );
}

#[test]
fn surface_dag_producer_preserves_the_frozen_no_scroll_measurement_contract() {
    let rows = CASES
        .into_iter()
        .flat_map(|case| [1.0_f32, 2.0_f32].map(|dpr| measure_case(case, dpr)))
        .collect::<Vec<_>>();
    let transform = [RetainedSurfaceRasterRole::Transform];
    let effect = [RetainedSurfaceRasterRole::PropertyEffect];
    let co_located = [
        RetainedSurfaceRasterRole::Transform,
        RetainedSurfaceRasterRole::PropertyEffect,
    ];
    let expected = vec![
        prepared_row("transform-image-ready", 1.0, &transform, &[20_736]),
        prepared_row("transform-image-ready", 2.0, &transform, &[82_944]),
        prepared_row("transform-image-loading", 1.0, &transform, &[20_736]),
        prepared_row("transform-image-loading", 2.0, &transform, &[82_944]),
        prepared_row("transform-image-error", 1.0, &transform, &[20_736]),
        prepared_row("transform-image-error", 2.0, &transform, &[82_944]),
        prepared_row("transform-svg-ready", 1.0, &transform, &[20_736]),
        prepared_row("transform-svg-ready", 2.0, &transform, &[82_944]),
        prepared_row("transform-svg-loading", 1.0, &transform, &[20_736]),
        prepared_row("transform-svg-loading", 2.0, &transform, &[82_944]),
        prepared_row("transform-svg-error", 1.0, &transform, &[20_736]),
        prepared_row("transform-svg-error", 2.0, &transform, &[82_944]),
        prepared_row("effect-text-ready", 1.0, &effect, &[3_024]),
        prepared_row("effect-text-ready", 2.0, &effect, &[12_096]),
        prepared_row("effect-image-ready", 1.0, &effect, &[3_024]),
        prepared_row("effect-image-ready", 2.0, &effect, &[12_096]),
        missing_paint_identity_row("effect-image-loading", 1.0),
        missing_paint_identity_row("effect-image-loading", 2.0),
        missing_paint_identity_row("effect-image-error", 1.0),
        missing_paint_identity_row("effect-image-error", 2.0),
        prepared_row("effect-svg-ready", 1.0, &effect, &[3_024]),
        prepared_row("effect-svg-ready", 2.0, &effect, &[12_096]),
        missing_paint_identity_row("effect-svg-loading", 1.0),
        missing_paint_identity_row("effect-svg-loading", 2.0),
        missing_paint_identity_row("effect-svg-error", 1.0),
        missing_paint_identity_row("effect-svg-error", 2.0),
        prepared_row(
            "co-located-transform-effect",
            1.0,
            &co_located,
            &[12_300, 2_280],
        ),
        prepared_row(
            "co-located-transform-effect",
            2.0,
            &co_located,
            &[46_080, 8_880],
        ),
    ];
    assert_eq!(rows, expected, "the 28-row producer corpus is frozen");

    let non_empty_rows = rows
        .iter()
        .filter_map(|row| match &row.outcome {
            ProducerMeasurementOutcome::Prepared {
                aggregate_bytes, ..
            } => Some((row.label, *aggregate_bytes)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let non_empty_labels = non_empty_rows
        .iter()
        .map(|(label, _)| *label)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(non_empty_labels.len(), 10);
    assert_eq!(non_empty_rows.len(), 20);
    assert!(
        non_empty_labels
            .iter()
            .any(|label| label.starts_with("transform-"))
    );
    assert!(
        non_empty_labels
            .iter()
            .any(|label| label.starts_with("effect-"))
    );
    assert!(non_empty_labels.contains("co-located-transform-effect"));

    let mut aggregate_bytes = non_empty_rows
        .iter()
        .map(|(_, bytes)| *bytes)
        .collect::<Vec<_>>();
    aggregate_bytes.sort_unstable();
    let median = (aggregate_bytes[9] + aggregate_bytes[10]) / 2;
    let p90 = aggregate_bytes[17];
    let maximum = *aggregate_bytes.last().expect("non-empty producer rows");
    assert_eq!((median, p90, maximum), (20_736, 82_944, 82_944));
    assert!(
        maximum <= 32 * 1024 * 1024,
        "exceeding the precommitted headroom bound stops C3b3c1b-1"
    );
}

#[test]
fn co_located_raster_origin_redirects_source_bits_without_changing_pair_bytes() {
    let (arena, roots) = ProducerCase::CoLocatedTransformEffect.build();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let expectations = [
        (
            1.0_f32,
            [0.5, 0.5, 40.0, 24.0],
            [0.75, 0.0, 18.0, 10.0],
            [12_300_u64, 2_280],
        ),
        (
            2.0_f32,
            [0.0, 0.0, 40.0, 24.0],
            [0.25, 0.0, 18.0, 10.0],
            [46_080_u64, 8_880],
        ),
    ];
    for (dpr, transform_source, effect_source, expected_pair_bytes) in expectations {
        let frame = prepared_recorded_scene(dpr, &arena, &roots, &properties, &generations);
        let source_for_role = |role| {
            frame
                .raster_plan()
                .nodes()
                .iter()
                .find(|node| node.identity().role == role)
                .expect("co-located role")
                .target()
                .source_bounds_bits
                .map(f32::from_bits)
        };
        assert_eq!(
            source_for_role(RetainedSurfaceRasterRole::Transform).map(f32::to_bits),
            transform_source.map(f32::to_bits),
        );
        assert_eq!(
            source_for_role(RetainedSurfaceRasterRole::PropertyEffect).map(f32::to_bits),
            effect_source.map(f32::to_bits),
        );
        assert_ne!(
            effect_source.map(f32::to_bits),
            [4.75_f32, 2.0, 18.0, 10.0].map(f32::to_bits),
            "the effect source must actually move into texture-local space",
        );
        let pair_bytes = frame
            .raster_plan()
            .nodes()
            .iter()
            .map(|node| {
                let target = node.target();
                crate::view::raster_cost::texture_desc_payload_bytes(&target.color)
                    .bytes
                    .checked_add(
                        crate::view::raster_cost::texture_desc_payload_bytes(&target.depth).bytes,
                    )
                    .expect("co-located descriptor pair bytes")
            })
            .collect::<Vec<_>>();
        assert_eq!(pair_bytes, expected_pair_bytes);
    }
}

#[test]
fn surface_dag_producer_records_five_real_scroll_property_trees_and_freezes_downstream_outcomes() {
    let cases = [
        {
            let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
            ("scroll", arena, roots, properties, generations)
        },
        {
            let (arena, roots, properties, generations) =
                prepared_same_owner_transform_scroll_scene();
            ("transform-scroll", arena, roots, properties, generations)
        },
        {
            let (arena, roots, properties, generations) = prepared_same_owner_effect_scroll_scene();
            ("effect-scroll", arena, roots, properties, generations)
        },
        {
            let (arena, roots, properties, generations) = prepared_transform_effect_scroll_scene();
            (
                "transform-effect-scroll",
                arena,
                roots,
                properties,
                generations,
            )
        },
        {
            let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
            ("nested-scroll", arena, roots, properties, generations)
        },
    ];
    let rows = cases
        .into_iter()
        .flat_map(|(label, arena, roots, properties, generations)| {
            [1.0_f32, 2.0_f32].map(move |dpr| {
                measure_recorded_scene(label, dpr, &arena, &roots, &properties, &generations)
            })
        })
        .collect::<Vec<_>>();
    assert!(
        rows.iter()
            .all(|row| !matches!(row.outcome, ProducerMeasurementOutcome::RecordRejected(_))),
        "all five real scroll shapes at both DPR values must pass recorder admission: {rows:#?}",
    );
    let scroll = [RetainedSurfaceRasterRole::ScrollContent];
    let nested_scroll = [
        RetainedSurfaceRasterRole::ScrollContent,
        RetainedSurfaceRasterRole::ScrollContent,
    ];
    let transform_scroll = [
        RetainedSurfaceRasterRole::Transform,
        RetainedSurfaceRasterRole::ScrollContent,
    ];
    let transform_effect_scroll = [
        RetainedSurfaceRasterRole::Transform,
        RetainedSurfaceRasterRole::PropertyEffect,
        RetainedSurfaceRasterRole::ScrollContent,
    ];
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0], prepared_row("scroll", 1.0, &scroll, &[360_000]));
    assert_eq!(rows[1], prepared_row("scroll", 2.0, &scroll, &[1_440_000]));
    // Raster-origin redirection is now established for transform-scroll and
    // transform-effect-scroll. The natural two-frame differential below is a
    // separate required proof of the reuse consequence:
    // 1. call set_scroll_offset, real measure_and_place, resync, and record;
    //    never compensate by changing an authored position;
    // 2. never manipulate dirty flags to manufacture the second frame;
    // 3. offset-only must change composite geometry, preserve resident stamps,
    //    and produce Reuse from the real pool action;
    // 4. a content-change control must change the stamp and produce Reraster.
    assert_eq!(
        rows[2],
        prepared_row(
            "transform-scroll",
            1.0,
            &transform_scroll,
            &[345_600, 345_600],
        )
    );
    assert_eq!(
        rows[3],
        prepared_row(
            "transform-scroll",
            2.0,
            &transform_scroll,
            &[1_382_400, 1_382_400],
        )
    );
    // This is a different-stage artifact-store gap. The raster-origin batch
    // must classify it explicitly: either turn both rows into Prepared with
    // measured bytes, or retain this rejection with a named owning batch.
    assert_invalid_store_row(&rows[4], "effect-scroll", 1.0);
    assert_invalid_store_row(&rows[5], "effect-scroll", 2.0);
    assert_eq!(
        rows[6],
        prepared_row(
            "transform-effect-scroll",
            1.0,
            &transform_effect_scroll,
            &[345_600, 345_600, 345_600],
        )
    );
    assert_eq!(
        rows[7],
        prepared_row(
            "transform-effect-scroll",
            2.0,
            &transform_effect_scroll,
            &[1_382_400, 1_382_400, 1_382_400],
        )
    );
    assert_eq!(
        rows[8],
        prepared_row("nested-scroll", 1.0, &nested_scroll, &[720_000, 720_000])
    );
    assert_eq!(
        rows[9],
        prepared_row(
            "nested-scroll",
            2.0,
            &nested_scroll,
            &[2_880_000, 2_880_000],
        )
    );
}

#[test]
fn natural_scroll_offset_changes_only_composite_geometry_until_content_changes() {
    let (mut arena, roots, _, _) = prepared_exact_scroll_scene();
    let content = arena.children_of(roots[0])[0];
    let content_style = |color| {
        let mut style = Style::new();
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        style
    };
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .apply_style(content_style(Color::rgb(24, 48, 72)));
    let run_layout = |arena: &mut NodeArena| {
        crate::view::test_support::measure_and_place(
            arena,
            roots[0],
            LayoutConstraints {
                max_width: 100.0,
                max_height: 80.0,
                viewport_width: 100.0,
                viewport_height: 80.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(80.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 80.0,
                viewport_width: 100.0,
                viewport_height: 80.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(80.0),
            },
        );
    };
    run_layout(&mut arena);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let baseline = prepared_recorded_scene(1.0, &arena, &roots, &properties, &generations);
    let baseline_geometry = baseline
        .raster_plan()
        .nodes()
        .iter()
        .map(|node| node.geometry())
        .collect::<Vec<_>>();

    crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
        .set_scroll_offset((0.0, 37.0));
    run_layout(&mut arena);
    let (moved_properties, moved_generations) = synced_paint_state(&arena, &roots);
    let moved = prepared_recorded_scene(1.0, &arena, &roots, &moved_properties, &moved_generations);
    let moved_geometry = moved
        .raster_plan()
        .nodes()
        .iter()
        .map(|node| node.geometry())
        .collect::<Vec<_>>();
    assert_ne!(
        baseline_geometry, moved_geometry,
        "offset-only must remain a composite placement change",
    );
    assert_eq!(
        baseline.residents(),
        moved.residents(),
        "offset-only must not alter texture-local resident stamps",
    );

    let mut viewport = crate::view::viewport::Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("baseline resident owner");
    let baseline_emission = viewport
        .prepare_artifact_surface_pool_emission_from_pool(baseline.residents().clone())
        .expect("baseline resident emission");
    assert!(
        viewport.stage_artifact_surface_resident_set(
            owner,
            baseline_emission.into_canonical_residents(),
        )
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    let moved_emission = viewport
        .prepare_artifact_surface_pool_emission_for_forced_test(moved.residents().clone())
        .expect("offset-only pool actions");
    assert!(
        moved_emission
            .ordered_actions()
            .iter()
            .all(|(_, action)| *action == RetainedSurfaceCompileAction::Reuse)
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .apply_style(content_style(Color::rgb(15, 90, 180)));
    let (changed_properties, changed_generations) = synced_paint_state(&arena, &roots);
    let changed = prepared_recorded_scene(
        1.0,
        &arena,
        &roots,
        &changed_properties,
        &changed_generations,
    );
    assert_ne!(
        moved.residents(),
        changed.residents(),
        "content change must alter the resident stamp",
    );
    let changed_emission = viewport
        .prepare_artifact_surface_pool_emission_for_forced_test(changed.residents().clone())
        .expect("content-change pool actions");
    assert!(
        changed_emission
            .ordered_actions()
            .iter()
            .any(|(_, action)| *action == RetainedSurfaceCompileAction::Reraster)
    );
}
