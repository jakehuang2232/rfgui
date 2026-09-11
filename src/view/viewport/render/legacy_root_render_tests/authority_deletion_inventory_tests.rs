use super::*;

fn register_stage_c_authority_payload<T>() -> &'static str {
    std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .expect("a Rust type has a terminal name")
}

fn auto_authority_variant_payloads(
    source: &str,
) -> std::collections::BTreeMap<String, Option<String>> {
    // This declaration reader intentionally accepts the current braced,
    // single-line payload shape. A multi-line payload type is read as `None`
    // and fails the exact mapping below. A unit variant is skipped here, so
    // the exhaustive matcher remains the compiler-enforced protection for
    // that shape and must not be treated as redundant with this parser.
    let body = source
        .split_once("enum AutoAuthorityDecision {")
        .expect("render.rs declares AutoAuthorityDecision")
        .1
        .split_once("\n}")
        .expect("the authority declaration is brace-terminated")
        .0;
    let mut variants = std::collections::BTreeMap::new();
    let mut current = None;
    let mut payload = None;
    for line in body.lines().map(str::trim) {
        if let Some(name) = line.strip_suffix(" {") {
            current = Some(name.to_string());
            payload = None;
        } else if line == "}," {
            let name = current.take().expect("a variant block is open");
            assert!(variants.insert(name, payload.take()).is_none());
        } else if let Some((field, ty)) =
            line.strip_suffix(',').and_then(|line| line.split_once(':'))
            && field != "trace"
        {
            assert!(payload.is_none(), "one authority variant owns one payload");
            payload = Some(
                ty.trim()
                    .rsplit("::")
                    .next()
                    .expect("a payload type has a terminal name")
                    .to_string(),
            );
        }
    }
    assert!(current.is_none(), "every authority variant block is closed");
    variants
}

fn retained_authority_label(decision: AutoAuthorityDecision) -> Option<&'static str> {
    // The strings are diagnostic names, not a behavioral contract. The
    // contract is this match's exhaustiveness over all nine retained and two
    // non-retained variants, including unit variants the parser cannot see.
    match decision {
        AutoAuthorityDecision::NativeScrollForest { .. } => Some("native-scroll-forest"),
        AutoAuthorityDecision::PropertyBoundaryDagScene { .. } => {
            Some("property-boundary-dag-scene")
        }
        AutoAuthorityDecision::DirectScrollTransformScene { .. } => {
            Some("direct-scroll-transform-scene")
        }
        AutoAuthorityDecision::PropertyScrollScene { .. } => Some("property-scroll-scene"),
        AutoAuthorityDecision::FrameRootScrollScene { .. } => Some("frame-root-scroll-scene"),
        AutoAuthorityDecision::TransformScrollScene { .. } => Some("transform-scroll-scene"),
        AutoAuthorityDecision::EffectScrollScene { .. } => Some("effect-scroll-scene"),
        AutoAuthorityDecision::TransformEffectScrollScene { .. } => {
            Some("transform-effect-scroll-scene")
        }
        AutoAuthorityDecision::PropertyScene { .. } => Some("property-scene"),
        AutoAuthorityDecision::Artifact { .. } | AutoAuthorityDecision::Legacy { .. } => None,
    }
}

#[test]
fn stage_c_deletion_inventory_closes_auto_authority_variants_and_payloads() {
    let payloads: std::collections::BTreeSet<&str> =
        [
            register_stage_c_authority_payload::<crate::view::paint::FramePaintPlan>(),
            register_stage_c_authority_payload::<
                crate::view::paint::ValidatedPropertyBoundaryDagScene,
            >(),
            register_stage_c_authority_payload::<
                crate::view::paint::ValidatedDirectScrollTransformTransaction,
            >(),
            register_stage_c_authority_payload::<crate::view::paint::ValidatedPropertyScrollScene>(
            ),
            register_stage_c_authority_payload::<crate::view::paint::ValidatedFrameRootScrollScene>(
            ),
            register_stage_c_authority_payload::<crate::view::paint::ValidatedTransformScrollScene>(
            ),
            register_stage_c_authority_payload::<
                crate::view::paint::ValidatedEffectScrollSceneCheckpoint,
            >(),
            register_stage_c_authority_payload::<
                crate::view::paint::ValidatedTransformEffectScrollScene,
            >(),
        ]
        .into_iter()
        .collect();
    assert_eq!(
        payloads,
        [
            "FramePaintPlan",
            "ValidatedDirectScrollTransformTransaction",
            "ValidatedEffectScrollSceneCheckpoint",
            "ValidatedFrameRootScrollScene",
            "ValidatedPropertyBoundaryDagScene",
            "ValidatedPropertyScrollScene",
            "ValidatedTransformEffectScrollScene",
            "ValidatedTransformScrollScene",
        ]
        .into_iter()
        .collect(),
        "all eight unique retained authority payload types must be deleted together",
    );

    let actual = auto_authority_variant_payloads(include_str!("../../render.rs"));
    let expected = [
        ("Artifact", Some("RecordedArtifactCandidate")),
        (
            "DirectScrollTransformScene",
            Some("ValidatedDirectScrollTransformTransaction"),
        ),
        (
            "EffectScrollScene",
            Some("ValidatedEffectScrollSceneCheckpoint"),
        ),
        (
            "FrameRootScrollScene",
            Some("ValidatedFrameRootScrollScene"),
        ),
        ("Legacy", None),
        ("NativeScrollForest", Some("FramePaintPlan")),
        (
            "PropertyBoundaryDagScene",
            Some("ValidatedPropertyBoundaryDagScene"),
        ),
        ("PropertyScene", Some("FramePaintPlan")),
        ("PropertyScrollScene", Some("ValidatedPropertyScrollScene")),
        (
            "TransformEffectScrollScene",
            Some("ValidatedTransformEffectScrollScene"),
        ),
        (
            "TransformScrollScene",
            Some("ValidatedTransformScrollScene"),
        ),
    ]
    .into_iter()
    .map(|(variant, payload)| (variant.to_string(), payload.map(str::to_string)))
    .collect();
    assert_eq!(
        actual, expected,
        "the nine retained authority variants, their payload mapping, and the two non-retained variants form one closed declaration",
    );

    let exhaustive_matcher: fn(AutoAuthorityDecision) -> Option<&'static str> =
        retained_authority_label;
    let _ = exhaustive_matcher;
    assert_authority_deletion_line_ledger(&actual);
}

#[derive(Clone, Copy)]
enum LedgerSource {
    ScrollScene,
    FramePlan,
    RetainedExecutor,
}

impl LedgerSource {
    fn text(self) -> &'static str {
        match self {
            Self::ScrollScene => include_str!("../../../paint/scroll_scene.rs"),
            Self::FramePlan => include_str!("../../../paint/frame_plan.rs"),
            Self::RetainedExecutor => include_str!("../../../paint/retained_surface_executor.rs"),
        }
    }
}

// A deliberately format-sensitive census, not a Rust parser or an acceptance
// gate. Names identify the first/last function; their declaration indentation
// identifies the closing line. Moving/reformatting these items requires a
// reviewed recount, not silently adopting whatever the new source measures.
fn ledger_function_lines(source: &str, name: &str) -> (usize, usize) {
    let needle = format!("fn {name}");
    let declarations = source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            line.split_once(&needle)
                .is_some_and(|(_, suffix)| suffix.starts_with(['(', '<']))
        })
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 1, "one declaration for {name}");
    let (first, declaration) = declarations[0];
    let indent = declaration.len() - declaration.trim_start().len();
    let closing = format!("{}}}", " ".repeat(indent));
    let last = source
        .lines()
        .enumerate()
        .skip(first + 1)
        .find(|(_, line)| *line == closing)
        .map(|(index, _)| index)
        .expect("the measured declaration has a closing line");
    (first, last)
}

type LedgerSpan = (LedgerSource, &'static str, &'static str, usize);

fn function(source: LedgerSource, name: &'static str, lines: usize) -> LedgerSpan {
    (source, name, name, lines)
}

fn measured_ledger_span((source, first, last, expected): LedgerSpan) -> usize {
    let (start, _) = ledger_function_lines(source.text(), first);
    let (_, end) = ledger_function_lines(source.text(), last);
    assert!(start <= end);
    let actual = end - start + 1;
    assert_eq!(actual, expected, "review the census for {first}..={last}");
    actual
}

fn assert_authority_deletion_line_ledger(
    variants: &std::collections::BTreeMap<String, Option<String>>,
) {
    use LedgerSource::{FramePlan as F, RetainedExecutor as R, ScrollScene as S};

    // These are retirement candidates, NOT lines already proven deletable.
    // Each complete retirement removes one decision variant. Merely adding an
    // earlier Artifact attempt while retaining fallback removes zero variants
    // and guarantees zero deleted implementation lines.
    let rows: [(&str, &[LedgerSpan]); 9] = [
        (
            "DirectScrollTransformScene",
            &[(
                S,
                "plan_direct_scroll_transform_scene_scaffold",
                "emit_prepared_direct_scroll_transform_scene",
                1091,
            )],
        ),
        (
            "TransformScrollScene",
            &[
                function(S, "plan_and_validate_transform_scroll_scene", 253),
                function(S, "prepare_retained_transform_scroll_scene_from_pool", 376),
                function(S, "emit_prepared_retained_transform_scroll_scene", 184),
            ],
        ),
        (
            "EffectScrollScene",
            &[
                function(S, "plan_and_validate_effect_scroll_scene_checkpoint", 225),
                function(S, "prepare_retained_effect_scroll_scene_from_pool", 382),
                function(S, "emit_prepared_retained_effect_scroll_scene", 198),
            ],
        ),
        (
            "TransformEffectScrollScene",
            &[
                function(S, "plan_and_validate_transform_effect_scroll_scene", 295),
                function(
                    S,
                    "prepare_retained_transform_effect_scroll_scene_from_pool",
                    519,
                ),
                function(
                    S,
                    "emit_prepared_retained_transform_effect_scroll_scene",
                    286,
                ),
            ],
        ),
        (
            "FrameRootScrollScene",
            &[
                function(S, "plan_and_validate_frame_root_scroll_scene", 393),
                function(S, "prepare_frame_root_scroll_scene", 235),
                function(S, "emit_prepared_frame_root_scroll_scene", 132),
            ],
        ),
        (
            "PropertyScrollScene",
            &[
                function(S, "plan_and_validate_property_scroll_scene", 129),
                function(S, "prepare_retained_property_scroll_forest_from_pool", 153),
                function(S, "emit_prepared_retained_property_scroll_forest", 77),
            ],
        ),
        (
            "NativeScrollForest",
            &[
                function(F, "plan_native_scroll_forest_scaffold_with_context", 337),
                function(S, "prepare_native_scroll_forest_transaction_from_pool", 7),
                function(S, "emit_prepared_native_scroll_forest_transaction", 49),
            ],
        ),
        (
            "PropertyBoundaryDagScene",
            &[
                function(S, "plan_and_validate_after_fixed_grammar_cascade", 26),
                function(S, "prepare_property_boundary_dag_scene_from_pool", 95),
                function(S, "emit_prepared_property_boundary_dag_scene", 52),
            ],
        ),
        (
            "PropertyScene",
            &[
                function(F, "plan_transform_property_scene_with_context", 164),
                function(F, "plan_property_effect_scene_with_context", 151),
                function(R, "prepare_retained_property_scene_from_pool", 8),
                function(R, "emit_prepared_retained_property_scene", 31),
            ],
        ),
    ];
    let ledger_names = rows
        .iter()
        .map(|(name, _)| *name)
        .collect::<std::collections::BTreeSet<_>>();
    let retained_names = variants
        .keys()
        .map(String::as_str)
        .filter(|name| !matches!(*name, "Artifact" | "Legacy"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        ledger_names, retained_names,
        "every surviving authority needs a ledger row"
    );
    assert_eq!(rows.len(), ledger_names.len(), "no duplicate rows");
    let mut outside_four_files = 0;
    let mut entry_total = 0;
    for (_, spans) in rows {
        for &span in spans {
            let lines = measured_ledger_span(span);
            entry_total += lines;
            if matches!(span.0, R) {
                outside_four_files += lines;
            }
        }
    }
    assert_eq!((entry_total, outside_four_files), (5848, 39));

    // Shared lower bounds: direct calls from multiple surviving authorities,
    // not a complete transitive dependency closure. A second textual call is
    // NOT proof of a second production execution: the DAG continuation passes
    // include_fixed_cascade_grammars=false and skips the four prior grammars.
    // Retiring their decision variants also needs their dormant/test-facing
    // DAG adapters redirected; do not count that as live duplicate planning.
    let shared = [
        (S, "bounds_bits", 8),
        (S, "canonical_pair_bytes", 8),
        (S, "checked_property_scroll_opaque_order_count", 4),
        (S, "emit_prepared_property_scroll_boundary_parts", 67),
        (S, "plan_exact_effect_scroll_boundary_checkpoint", 176),
        (
            F,
            "plan_property_scroll_interleave_scaffold_with_context",
            898,
        ),
        (S, "prepare_retained_property_scroll_boundary_parts", 51),
        (S, "property_scroll_budget", 9),
        (S, "property_scroll_plan_from_exact_scene", 876),
        (S, "transform_scroll_receiver_raster_bounds", 47),
        (S, "validate_parent_target", 24),
        (S, "validate_property_scroll_boundary_from_frozen_plan", 25),
        (F, "opaque_order_count", 25),
        (F, "property_scene_error", 5),
        (F, "property_scene_plan_is_sealed", 230),
    ];
    let shared_total: usize = shared
        .into_iter()
        .map(|(source, name, lines)| measured_ledger_span((source, name, name, lines)))
        .sum();
    assert_eq!(shared_total, 2453);

    // Fourth number, on the same bounded census: shared lines that must remain
    // when ONLY this authority retires and the other eight remain alive.
    // These are lower bounds, not complete ratios. Rows overlap: never sum
    // these per-authority numbers as though each counted a distinct function.
    // The DAG's 3,478 delegated lines are the four other ledger rows, not new
    // unassigned code. Its production continuation skips those grammars, but
    // the corresponding leaf implementations still serve their own selectors.
    let retained: [(&str, &[usize], usize, usize); 9] = [
        ("DirectScrollTransformScene", &[0, 1, 10], 0, 40),
        (
            "TransformScrollScene",
            &[0, 1, 2, 3, 5, 6, 7, 8, 9, 10, 11],
            0,
            2017,
        ),
        (
            "EffectScrollScene",
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            0,
            2185,
        ),
        (
            "TransformEffectScrollScene",
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            0,
            2193,
        ),
        ("FrameRootScrollScene", &[0, 1, 5, 10], 0, 938),
        ("PropertyScrollScene", &[3, 6, 8, 10, 11], 0, 1043),
        ("NativeScrollForest", &[12, 13, 14], 0, 260),
        ("PropertyBoundaryDagScene", &[5], 3478, 4376),
        ("PropertyScene", &[12, 13, 14], 0, 260),
    ];
    assert_eq!(
        retained
            .iter()
            .map(|row| row.0)
            .collect::<std::collections::BTreeSet<_>>(),
        ledger_names
    );
    for (name, indices, delegated, expected) in retained {
        let actual = delegated + indices.iter().map(|&index| shared[index].2).sum::<usize>();
        assert_eq!(actual, expected, "shared retained lower bound for {name}");
    }

    let four_file_lines = [
        (S.text(), 25069),
        (F.text(), 15267),
        // C-1 spatial coexistence: 12 existing preflights use the established
        // four-dimensional comparison. Multiline formatting adds 36 lines,
        // and its contract comment adds 4; no bridge items were added.
        (include_str!("../../../paint/legacy_recording.rs"), 4986),
        (include_str!("../../../paint/legacy_admission.rs"), 1506),
    ]
    .into_iter()
    .map(|(source, expected)| {
        assert_eq!(
            source.lines().count(),
            expected,
            "review the four-file denominator"
        );
        expected
    })
    .sum::<usize>();
    assert_eq!(four_file_lines, 46828);
    assert_eq!(four_file_lines - (entry_total - outside_four_files), 41019);
    assert_eq!(
        four_file_lines - (entry_total - outside_four_files) - shared_total,
        38566
    );
    // Unattributed is NOT permanently retained. Neither the entry subtotal nor
    // the shared lower bound licenses leaving the other 38,566 lines behind.
    // Retirement must close production callers, port behavioral tests, and
    // leave named hardware evidence at TAKEOVER time whose oracle and execution
    // do not pass through the implementation being deleted. No gate execution
    // or failure means no cutover; an inventory passing proves none of those.
}
