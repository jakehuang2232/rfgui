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

/// The Stage A producer gate.
///
/// `artifact.rs` is the artifact data model the V2 layerizer consumes, and
/// `frame_recorder.rs` is what produces it. Neither may declare a
/// component-specific paint type, and the artifact model may not depend on the
/// legacy modules at all — the dependency has to point the other way.
///
/// A new `*TextArea*` declaration in either file is a regression, not a
/// registration opportunity: express the fact as a generic chunk, payload
/// identity, composite edge, or artifact-space transition instead.
#[test]
fn stage_a_producers_declare_no_component_specific_paint_types() {
    assert_eq!(
        declared_text_area_types(include_str!("../artifact.rs")),
        Vec::<String>::new(),
        "artifact.rs must not declare a component-specific paint type",
    );
    assert_eq!(
        declared_text_area_types(include_str!("../frame_recorder.rs")),
        Vec::<String>::new(),
        "the recorder may not learn which component grammar it is recording",
    );
    let artifact_source = include_str!("../artifact.rs");
    assert!(
        !artifact_source.contains("legacy_admission")
            && !artifact_source.contains("legacy_recording"),
        "artifact.rs must not depend on the legacy modules",
    );
}

/// `recording_context.rs` still carries per-shape legacy admission capability.
/// It is the remaining A4 producer debt: each field either moves into the legacy
/// capability/bridge or decomposes into a generic property witness, payload
/// source, or composite edge. It must never become a permanent hiding place that
/// bypasses the gate above.
///
/// The list is a ratchet: it may shrink, never grow.
#[test]
fn recording_context_legacy_capability_inventory_only_shrinks() {
    let pending = [
        "baked_scroll_atomic_projection_text_area_subtree",
        "baked_scroll_interactive_text_area_subtree",
        "baked_scroll_text_area_subtree",
        "scroll_atomic_projection_text_area_subtree",
        "scroll_interactive_text_area_subtree",
        "scroll_text_area_subtree",
    ];
    let source = include_str!("../recording_context.rs");
    let declared: Vec<&str> = pending
        .into_iter()
        .filter(|field| source.contains(&format!("{field}:")))
        .collect();
    assert_eq!(
        declared,
        pending.to_vec(),
        "shrink this list as A4 relocates or decomposes each legacy capability — never extend it",
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
