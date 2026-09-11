//! Stage A closure note (2026-08-08).
//!
//! - The artifact corpus asserts scroll_nodes empty — it exercises no scroll-graph coverage.
//! - Reuse is one retained scroll-content surface under five content shapes, not five reuse-layer shapes. This is the positive Stage A result: the reuse decision is already component-independent.
//! - Producer independence covers three producers. Downstream is not clean: compiler.rs holds 27 TextArea types / 435 grammar tokens, scroll_scene.rs holds 1 type / 857 tokens.
//!
//! The golden artifact corpus authors generic `sans-serif` at 17.5px. On the
//! closure platform (macOS, Fontique 0.11 CoreText backend), that maps to
//! Helvetica. The frozen glyph, caret, selection, and scrollbar floats also
//! freeze that mapping and the current shaping, layout, and hinting behavior;
//! an intentional change to those inputs requires an explicit golden-contract
//! update and is not automatically a rendering regression.

const LEGACY_ADMISSION_TYPE_NAMES: [&str; 14] = [
    "LegacyTextAreaProjection",
    "PaintLegacyTextAreaCoverageAuthority",
    "PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness",
    "PaintScrollAtomicProjectionTextAreaRecorderWitness",
    "PaintScrollDetachedProjectionSubtreeWitness",
    "PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness",
    "PaintScrollInteractiveTextAreaSubtreeWitness",
    "PaintScrollTextAreaSubtreeWitness",
    "RetainedInteractiveTextAreaResidentRasterSeal",
    "RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot",
    "RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot",
    "RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot",
    "RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot",
    "RetainedScrollTextAreaSubtreeAdmissionSnapshot",
];

const LEGACY_RECORDING_TYPE_NAMES: [&str; 14] = [
    "AtomicProjectionSelectionBackingContract",
    "AtomicProjectionSelectionPostCompositeContract",
    "RecordedRetainedAtomicProjectionSelectionTextAreaHost",
    "RecordedRetainedAtomicProjectionSelectionTextAreaSubtree",
    "RecordedRetainedAtomicProjectionTextAreaHost",
    "RecordedRetainedAtomicProjectionTextAreaSubtree",
    "RecordedRetainedFocusedAtomicProjectionTextAreaHost",
    "RecordedRetainedFocusedAtomicProjectionTextAreaSubtree",
    "RecordedRetainedInteractiveTextAreaSubtree",
    "RetainedAtomicProjectionChunkLiveRasterOracle",
    "RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle",
    "RetainedAtomicProjectionTextAreaLiveRasterOracle",
    "SnapshotMerge",
    "ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority",
];

// Private declarations cannot escape `legacy_recording`, so they belong only
// to its declaration-side closed set. This exclusion also avoids treating the
// unrelated private `frame_recorder::SnapshotMerge` as a legacy-module export.
const LEGACY_RECORDING_PRIVATE_TYPE_NAMES: [&str; 3] = [
    "AtomicProjectionSelectionBackingContract",
    "AtomicProjectionSelectionPostCompositeContract",
    "SnapshotMerge",
];

const LEGACY_REEXPORTED_SELECTOR_NAMES: [&str; 6] = [
    "exact_retained_property_scroll_text_area_paint_source",
    "exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission",
    "exact_retained_scroll_atomic_projection_text_area_subtree_admission",
    "exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission",
    "exact_retained_scroll_interactive_text_area_subtree_admission",
    "exact_retained_scroll_text_area_subtree_admission",
];

/// Exact-shape `FrameArtifactAuthorityPolicy` variants. Matching one of these is
/// grammar dispatch, which is exactly what Stage A removes.
const FORBIDDEN_EXACT_POLICY_VARIANTS: [&str; 6] = [
    "BakedScrollTextAreaSubtreeHost",
    "BakedScrollAtomicProjectionTextAreaSubtreeHost",
    "BakedScrollInteractiveTextAreaSubtreeHost",
    "ScrollTextAreaSubtreeLocal",
    "ScrollAtomicProjectionTextAreaSubtreeLocal",
    "ScrollInteractiveTextAreaSubtreeLocal",
];

fn forbidden_symbol_counts(source: &str) -> (usize, usize, usize) {
    let legacy_exports = LEGACY_ADMISSION_TYPE_NAMES
        .iter()
        .chain(
            LEGACY_RECORDING_TYPE_NAMES
                .iter()
                .filter(|name| !LEGACY_RECORDING_PRIVATE_TYPE_NAMES.contains(name)),
        )
        .chain(LEGACY_REEXPORTED_SELECTOR_NAMES.iter())
        .map(|name| source.matches(name).count())
        .sum();
    let variants = FORBIDDEN_EXACT_POLICY_VARIANTS
        .iter()
        .map(|name| source.matches(name).count())
        .sum();
    let imports =
        source.matches("legacy_admission").count() + source.matches("legacy_recording").count();
    (legacy_exports, variants, imports)
}

fn declared_text_area_types(source: &str) -> std::collections::BTreeSet<String> {
    super::declared_type_names_at_any_depth(source)
        .into_iter()
        .filter(|name| name.contains("TextArea"))
        .collect()
}

#[test]
fn producer_declaration_scan_reaches_nested_modules() {
    let nested = "mod nested {\n    pub(crate) struct NestedTextAreaToken;\n}";
    assert_eq!(
        declared_text_area_types(nested),
        ["NestedTextAreaToken".to_string()].into_iter().collect(),
        "producer and Element gates must scan declarations at every module depth",
    );
    assert_eq!(
        super::declared_top_level_type_names(nested),
        std::collections::BTreeSet::new(),
        "deletion closed sets intentionally remain top-level-only",
    );
}

/// The Stage A producer gate.
///
/// A paint producer must be able to record without knowing which component
/// grammar it is recording. Bare `TextArea` substrings are deliberately not
/// banned — diagnostics and generic payload names may legitimately mention it.
/// What is banned is declaring a component-specific paint type, naming a
/// concrete legacy token, matching an exact grammar variant, or depending on
/// the legacy modules at all: the dependency has to point the other way.
#[test]
fn producers_are_component_independent() {
    for (name, source) in [
        ("artifact.rs", include_str!("../artifact.rs")),
        ("frame_recorder.rs", include_str!("../frame_recorder.rs")),
        (
            "recording_context.rs",
            include_str!("../recording_context.rs"),
        ),
    ] {
        assert_eq!(
            declared_text_area_types(source),
            std::collections::BTreeSet::new(),
            "{name} must not declare a component-specific paint type",
        );
        assert_eq!(
            forbidden_symbol_counts(source),
            (0, 0, 0),
            "{name}: (concrete legacy token refs, exact grammar variant refs, legacy module refs) must all be zero",
        );
    }
}

/// A generic variant name is not proof of a generic variant.
///
/// `ScrollContentLocal` carried an `Option<PaintScrollTextAreaSubtreeWitness>`
/// second field: a durable name smuggling grammar dispatch past a
/// name-matching check. This reads the payload types out of the policy
/// declaration itself, so the gate cannot be satisfied by renaming.
#[test]
fn durable_authority_policy_carries_no_component_payload() {
    let source = include_str!("../frame_recorder.rs");
    let body = source
        .split_once("enum FrameArtifactAuthorityPolicy {")
        .expect("the recorder declares its authority policy")
        .1
        .split_once("\n}")
        .expect("the declaration is brace-terminated")
        .0;
    let payload_identifiers: Vec<&str> = body
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .collect();
    let offenders: Vec<&&str> = payload_identifiers
        .iter()
        .filter(|token| {
            token.contains("TextArea")
                || LEGACY_ADMISSION_TYPE_NAMES.contains(token)
                || (LEGACY_RECORDING_TYPE_NAMES.contains(token)
                    && !LEGACY_RECORDING_PRIVATE_TYPE_NAMES.contains(token))
                || LEGACY_REEXPORTED_SELECTOR_NAMES.contains(token)
        })
        .collect();
    assert_eq!(
        offenders,
        Vec::<&&str>::new(),
        "no durable authority variant may carry a component-specific payload, whatever the variant is called",
    );
}

/// The recorder capability context holds capabilities, not grammars.
///
/// Its six per-shape admission fields are gone; what replaced them are
/// behavior flags coverage re-derives per node. Naming one after the shape it
/// happens to serve today would reintroduce grammar dispatch one field at a
/// time, so the field names are part of the contract.
#[test]
fn recording_context_capabilities_are_behavior_named() {
    let source = include_str!("../recording_context.rs");
    let retired = [
        "baked_scroll_atomic_projection_text_area_subtree",
        "baked_scroll_interactive_text_area_subtree",
        "baked_scroll_text_area_subtree",
        "scroll_atomic_projection_text_area_subtree",
        "scroll_interactive_text_area_subtree",
        "scroll_text_area_subtree",
    ];
    let surviving: Vec<&str> = retired
        .into_iter()
        .filter(|field| source.contains(&format!("{field}:")))
        .collect();
    assert_eq!(
        surviving,
        Vec::<&str>::new(),
        "these per-shape admission capabilities are decomposed, not relocated",
    );

    let shape_named: Vec<String> = source
        .lines()
        .filter_map(|line| {
            let field = line.trim_start().strip_prefix("pub(crate) ")?;
            let name = field.split_once(':')?.0;
            if !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                return None;
            }
            [
                "text_area",
                "atomic_projection",
                "interactive",
                "baked_scroll",
            ]
            .iter()
            .any(|shape| name.contains(shape))
            .then(|| name.to_string())
        })
        .collect();
    assert_eq!(
        shape_named,
        [
            "inside_text_area",
            "text_area_selection",
            "text_area_preedit",
            "baked_scroll_host"
        ]
        .map(str::to_string)
        .to_vec(),
        "a new capability field must be named for the behavior it authorizes, not the grammar it serves",
    );
}

/// The Stage A5 gate: the built-in `Element` host owns no TextArea admission.
///
/// `Element` used to declare five TextArea admission snapshots, carry their
/// `paint_grammar`, and mint them from five exact selectors. All of that is
/// component-specific admission authority living in a durable host. What may
/// remain is exactly one registered legacy extraction seam, so the boundary
/// cannot quietly regrow one method at a time.
#[test]
fn element_host_owns_no_text_area_admission() {
    let source = include_str!("../../base_component/element/mod.rs");
    assert_eq!(
        declared_text_area_types(source),
        std::collections::BTreeSet::new(),
        "element/mod.rs must not declare a TextArea admission snapshot",
    );
    assert_eq!(
        forbidden_symbol_counts(source),
        (0, 0, 0),
        "element/mod.rs must not name a concrete legacy paint token, match an exact grammar variant, or depend on the legacy modules",
    );
    for forbidden in [
        "RetainedTextAreaPaintGrammar",
        "RetainedAtomicProjectionTextAreaPaintGrammar",
        "RetainedAtomicProjectionSelectionTextAreaPaintGrammar",
        "RetainedFocusedAtomicProjectionTextAreaPaintGrammar",
        "RetainedInteractiveTextAreaPaintGrammar",
        "paint_grammar",
        "exact_retained_scroll_text_area_subtree_admission",
        "exact_retained_scroll_atomic_projection_text_area_subtree_admission",
        "exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission",
        "exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission",
        "exact_retained_scroll_interactive_text_area_subtree_admission",
    ] {
        assert_eq!(
            source.matches(forbidden).count(),
            0,
            "element/mod.rs must not reference {forbidden}",
        );
    }

    // Exactly one registered retained-admission seam, and it must be named as
    // the legacy extraction it is rather than as a generic capability. The
    // `legacy_retained_` prefix is the reserved marker: anything else that
    // wants to hand exact retained-admission shape to the paint side has to
    // take this name, and then it lands here.
    let legacy_exports: Vec<&str> = source
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("pub(crate) fn "))
        .filter_map(|line| line.split_once('(').map(|split| split.0))
        .filter(|name| name.starts_with("legacy_retained_"))
        .collect();
    assert_eq!(
        legacy_exports,
        Vec::<&str>::new(),
        "retired single-child extraction seams must not return",
    );
}
