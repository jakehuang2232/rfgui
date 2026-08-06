use super::*;

use super::super::legacy_admission::{
    RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    RetainedAtomicProjectionTextAreaLiveRasterOracle,
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

/// `artifact.rs` is the artifact data model the V2 layerizer consumes. It has
/// passed the Stage A producer gate: no component-specific paint type, and no
/// dependency on `legacy_admission`. A new `*TextArea*` declaration or a
/// `legacy_admission` reference here is a regression, not a registration
/// opportunity — express the fact as a generic chunk, payload identity,
/// composite edge, or artifact-space transition instead.
#[test]
fn artifact_model_is_component_independent() {
    assert_eq!(
        declared_text_area_types(include_str!("../artifact.rs")),
        Vec::<String>::new(),
        "artifact.rs must not declare a component-specific paint type",
    );
    assert!(
        !include_str!("../artifact.rs").contains("legacy_admission"),
        "artifact.rs must not depend on legacy_admission; the dependency has to point the other way",
    );
}

/// NOT a gate — an inventory of work still owed by A4.
///
/// These eight recorded host/subtree wrappers each encode one exact component
/// shape. A4 is complete only when they have collapsed into generic recorded
/// boundary segments and this list is empty, at which point this test is
/// replaced by the same zero assertion `artifact.rs` already passes.
///
/// The list is a ratchet: it may shrink, never grow.
#[test]
fn frame_recorder_component_specific_wrapper_inventory_only_shrinks() {
    let recorder_pending = [
        "RecordedRetainedAtomicProjectionSelectionTextAreaHost",
        "RecordedRetainedAtomicProjectionSelectionTextAreaSubtree",
        "RecordedRetainedAtomicProjectionTextAreaHost",
        "RecordedRetainedAtomicProjectionTextAreaSubtree",
        "RecordedRetainedFocusedAtomicProjectionTextAreaHost",
        "RecordedRetainedFocusedAtomicProjectionTextAreaSubtree",
        "RecordedRetainedInteractiveTextAreaSubtree",
        "ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority",
    ]
    .map(str::to_string)
    .to_vec();
    assert_eq!(
        declared_text_area_types(include_str!("../frame_recorder.rs")),
        recorder_pending,
        "the recorder may not learn which component grammar it is recording. Shrink this list as A4 collapses the wrappers — never extend it",
    );
}

/// `legacy_admission` is deleted whole in the Stage C hard cutover. Linking the
/// types here makes that batch fail to compile if any survive.
#[test]
fn stage_c_deletion_inventory_keeps_legacy_admission_types_compile_time_linked() {
    register_stage_c_deletion_type::<PaintScrollTextAreaSubtreeWitness>();
    register_stage_c_deletion_type::<PaintScrollInteractiveTextAreaSubtreeWitness>();
    register_stage_c_deletion_type::<PaintScrollAtomicProjectionTextAreaRecorderWitness>();
    register_stage_c_deletion_type::<PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness>();
    register_stage_c_deletion_type::<PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness>();
    register_stage_c_deletion_type::<RetainedInteractiveTextAreaResidentRasterSeal>();
    register_stage_c_deletion_type::<RetainedAtomicProjectionTextAreaLiveRasterOracle>();
    register_stage_c_deletion_type::<RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle>();
}

#[test]
fn stage_c_deletion_inventory_rejects_unregistered_legacy_admission_types() {
    let expected = [
        "PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness",
        "PaintScrollAtomicProjectionTextAreaRecorderWitness",
        "PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness",
        "PaintScrollInteractiveTextAreaSubtreeWitness",
        "PaintScrollTextAreaSubtreeWitness",
        "RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle",
        "RetainedAtomicProjectionTextAreaLiveRasterOracle",
        "RetainedInteractiveTextAreaResidentRasterSeal",
    ]
    .map(str::to_string)
    .to_vec();

    assert_eq!(
        declared_text_area_types(include_str!("../legacy_admission.rs")),
        expected,
        "legacy_admission.rs is a closed set awaiting Stage C deletion; do not add items to it",
    );
}
