use super::*;
use crate::view::paint::{
    ArtifactSurfaceRasterContext, ArtifactSurfaceRasterPlanError, ArtifactSurfaceResidentSealError,
    FrameArtifactFallbackReason, FrameArtifactRecordOutcome, LegacyPaintReason, RendererMode,
    RetainedSurfaceRasterRole, prepare_artifact_surface_raster_plan,
    record_surface_dag_no_scroll_frame_artifact, seal_prepared_artifact_surface_frame,
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
    let outcome = match record_surface_dag_no_scroll_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
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

#[test]
fn surface_dag_no_scroll_producer_frozen_measurement_contract() {
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
fn surface_dag_no_scroll_producer_rejects_a_real_scroll_property_tree() {
    let (arena, roots, properties, generations) =
        prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
    let error = record_surface_dag_no_scroll_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect_err("generic no-scroll authority must reject a real scroll tree");
    assert!(
        error
            .reasons
            .iter()
            .any(|reason| matches!(reason, FrameArtifactFallbackReason::PropertyBoundary(_))),
        "scroll rejection must retain its owning property boundary: {:?}",
        error.reasons
    );
}
