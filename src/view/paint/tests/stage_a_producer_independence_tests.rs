use super::*;

use super::super::legacy_recording::{
    RecordedRetainedAtomicProjectionSelectionTextAreaHost,
    RecordedRetainedAtomicProjectionSelectionTextAreaSubtree,
    RecordedRetainedAtomicProjectionTextAreaHost, RecordedRetainedAtomicProjectionTextAreaSubtree,
    RecordedRetainedFocusedAtomicProjectionTextAreaHost,
    RecordedRetainedFocusedAtomicProjectionTextAreaSubtree,
    RecordedRetainedInteractiveTextAreaSubtree,
    RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    RetainedAtomicProjectionTextAreaLiveRasterOracle,
    ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority,
};

fn register_stage_c_deletion_type<T>() {}

fn declared_text_area_types(source: &str) -> Vec<String> {
    let mut names: Vec<String> = source
        .lines()
        .filter_map(|line| {
            let mut tokens = line.trim_start().split_whitespace();
            let first = tokens.next()?;
            let declaration = if first.starts_with("pub") {
                tokens.next()?
            } else {
                first
            };
            if !matches!(declaration, "struct" | "enum" | "type") {
                return None;
            }
            let name = tokens.next()?.trim_end_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '_'
            });
            name.contains("TextArea").then(|| name.to_string())
        })
        .collect();
    names.sort();
    names
}

/// Concrete legacy paint tokens. A producer that names one of these has learned
/// which component grammar it is recording.
const FORBIDDEN_LEGACY_TYPES: [&str; 12] = [
    "PaintScrollTextAreaSubtreeWitness",
    "PaintScrollInteractiveTextAreaSubtreeWitness",
    "PaintScrollAtomicProjectionTextAreaRecorderWitness",
    "AtomicProjectionRecorderWitness",
    "PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness",
    "PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness",
    "PaintLegacyTextAreaCoverageAuthority",
    "LegacyTextAreaProjection",
    "RetainedInteractiveTextAreaResidentRasterSeal",
    "RetainedAtomicProjectionTextAreaLiveRasterOracle",
    "RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle",
    "RetainedAtomicProjectionChunkLiveRasterOracle",
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
    let types = FORBIDDEN_LEGACY_TYPES
        .iter()
        .map(|name| source.matches(name).count())
        .sum();
    let variants = FORBIDDEN_EXACT_POLICY_VARIANTS
        .iter()
        .map(|name| source.matches(name).count())
        .sum();
    let imports = source.matches("legacy_admission").count()
        + source.matches("legacy_recording").count();
    (types, variants, imports)
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
        ("recording_context.rs", include_str!("../recording_context.rs")),
    ] {
        assert_eq!(
            declared_text_area_types(source),
            Vec::<String>::new(),
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
        .filter(|token| token.contains("TextArea") || FORBIDDEN_LEGACY_TYPES.contains(token))
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
            ["text_area", "atomic_projection", "interactive", "baked_scroll"]
                .iter()
                .any(|shape| name.contains(shape))
                .then(|| name.to_string())
        })
        .collect();
    assert_eq!(
        shape_named,
        ["inside_text_area", "text_area_selection", "text_area_preedit", "baked_scroll_host"]
            .map(str::to_string)
            .to_vec(),
        "a new capability field must be named for the behavior it authorizes, not the grammar it serves",
    );
}

/// `legacy_admission` and `legacy_recording` are deleted whole in the Stage C
/// hard cutover. Linking every type here makes that batch fail to compile if any
/// survive, so the deletion cannot be partial.
#[test]
fn stage_c_deletion_inventory_keeps_legacy_modules_compile_time_linked() {
    register_stage_c_deletion_type::<PaintScrollTextAreaSubtreeWitness>();
    register_stage_c_deletion_type::<PaintScrollInteractiveTextAreaSubtreeWitness>();
    register_stage_c_deletion_type::<PaintScrollAtomicProjectionTextAreaRecorderWitness>();
    register_stage_c_deletion_type::<PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness>();
    register_stage_c_deletion_type::<PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness>();
    register_stage_c_deletion_type::<RetainedInteractiveTextAreaResidentRasterSeal>();
    register_stage_c_deletion_type::<PaintLegacyTextAreaCoverageAuthority>();
    register_stage_c_deletion_type::<LegacyTextAreaProjection>();

    register_stage_c_deletion_type::<RecordedRetainedAtomicProjectionTextAreaSubtree>();
    register_stage_c_deletion_type::<RecordedRetainedAtomicProjectionTextAreaHost>();
    register_stage_c_deletion_type::<RecordedRetainedAtomicProjectionSelectionTextAreaSubtree>();
    register_stage_c_deletion_type::<RecordedRetainedAtomicProjectionSelectionTextAreaHost>();
    register_stage_c_deletion_type::<RecordedRetainedFocusedAtomicProjectionTextAreaSubtree>();
    register_stage_c_deletion_type::<RecordedRetainedFocusedAtomicProjectionTextAreaHost>();
    register_stage_c_deletion_type::<RecordedRetainedInteractiveTextAreaSubtree>();
    register_stage_c_deletion_type::<ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority>();
    register_stage_c_deletion_type::<RetainedAtomicProjectionTextAreaLiveRasterOracle>();
    register_stage_c_deletion_type::<RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle>();
}

#[test]
fn stage_c_deletion_inventory_rejects_unregistered_legacy_types() {
    assert_eq!(
        declared_text_area_types(include_str!("../legacy_admission.rs")),
        [
            "LegacyTextAreaProjection",
            "PaintLegacyTextAreaCoverageAuthority",
            "PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness",
            "PaintScrollAtomicProjectionTextAreaRecorderWitness",
            "PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness",
            "PaintScrollInteractiveTextAreaSubtreeWitness",
            "PaintScrollTextAreaSubtreeWitness",
            "RetainedInteractiveTextAreaResidentRasterSeal",
        ]
        .map(str::to_string)
        .to_vec(),
        "legacy_admission.rs is a closed set awaiting Stage C deletion; do not add items to it",
    );
    assert_eq!(
        declared_text_area_types(include_str!("../legacy_recording.rs")),
        [
            "RecordedRetainedAtomicProjectionSelectionTextAreaHost",
            "RecordedRetainedAtomicProjectionSelectionTextAreaSubtree",
            "RecordedRetainedAtomicProjectionTextAreaHost",
            "RecordedRetainedAtomicProjectionTextAreaSubtree",
            "RecordedRetainedFocusedAtomicProjectionTextAreaHost",
            "RecordedRetainedFocusedAtomicProjectionTextAreaSubtree",
            "RecordedRetainedInteractiveTextAreaSubtree",
            "RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle",
            "RetainedAtomicProjectionTextAreaLiveRasterOracle",
            "ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority",
        ]
        .map(str::to_string)
        .to_vec(),
        "legacy_recording.rs is a closed set awaiting Stage C deletion; do not add items to it",
    );
}

/// The legacy live raster oracles own host/local parity. Nothing outside their
/// own module may reach their raw fields — a `pub(super)` data field there would
/// let another module forge or desynchronise a typed proof.
#[test]
fn legacy_oracle_data_fields_stay_private() {
    let offenders: Vec<&str> = include_str!("../legacy_recording.rs")
        .lines()
        .map(str::trim_end)
        .filter(|line| {
            line.starts_with("    pub(super) ") || line.starts_with("    pub(crate) ")
        })
        .filter(|line| !line.contains("fn "))
        .collect();
    assert_eq!(
        offenders,
        Vec::<&str>::new(),
        "expose a narrow method on the owning module instead of the raw field",
    );
}
