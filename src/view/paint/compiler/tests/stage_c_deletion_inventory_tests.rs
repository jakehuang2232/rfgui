use super::*;

fn register_stage_c_deletion_type<T>() {}

#[test]
fn stage_c_deletion_inventory_keeps_compiler_types_compile_time_linked() {
    register_stage_c_deletion_type::<ValidatedScrollSceneInteractiveTextAreaContentArtifact>();
    register_stage_c_deletion_type::<ValidatedScrollSceneAtomicProjectionTextAreaContentArtifact>();
    register_stage_c_deletion_type::<ValidatedScrollSceneAtomicProjectionTextAreaHostBeforeArtifact>(
    );
    register_stage_c_deletion_type::<ValidatedScrollSceneAtomicProjectionTextAreaOverlayArtifact>();
    register_stage_c_deletion_type::<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>();
    register_stage_c_deletion_type::<FocusedAtomicProjectionTextAreaPlanIdentity>();
    register_stage_c_deletion_type::<ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts>(
    );
    register_stage_c_deletion_type::<
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostBeforeArtifact,
    >();
    register_stage_c_deletion_type::<
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentArtifact,
    >();
    register_stage_c_deletion_type::<
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayArtifact,
    >();
    register_stage_c_deletion_type::<AtomicProjectionSelectionTextAreaPlanIdentity>();
    register_stage_c_deletion_type::<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>(
    );
    register_stage_c_deletion_type::<
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission,
    >();
    register_stage_c_deletion_type::<
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission,
    >();
    register_stage_c_deletion_type::<
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission,
    >();
    register_stage_c_deletion_type::<ValidatedScrollSceneAtomicProjectionTextAreaHostEmission>();
    register_stage_c_deletion_type::<ValidatedScrollSceneAtomicProjectionTextAreaContentEmission>();
    register_stage_c_deletion_type::<ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission>();
    register_stage_c_deletion_type::<AtomicProjectionTextAreaPlanIdentity>();
    register_stage_c_deletion_type::<RetainedAtomicProjectionTextAreaFrozenResidentRasterIdentity>(
    );
    register_stage_c_deletion_type::<RetainedAtomicProjectionTextAreaResidentRasterSeal>();
    register_stage_c_deletion_type::<RetainedAtomicProjectionTextAreaFrozenRasterDependencyIdentity>(
    );
    register_stage_c_deletion_type::<RetainedAtomicProjectionTextAreaRasterDependencySeal>();
    register_stage_c_deletion_type::<
        RetainedAtomicProjectionSelectionTextAreaFrozenResidentRasterIdentity,
    >();
    register_stage_c_deletion_type::<RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal>();
    register_stage_c_deletion_type::<
        RetainedAtomicProjectionSelectionTextAreaFrozenRasterDependencyIdentity,
    >();
    register_stage_c_deletion_type::<RetainedAtomicProjectionSelectionTextAreaRasterDependencySeal>(
    );
}

#[test]
fn stage_c_deletion_inventory_rejects_unregistered_downstream_text_area_types() {
    let compiler: std::collections::BTreeSet<String> =
        crate::view::paint::tests::declared_top_level_type_names(include_str!("../../compiler.rs"))
            .into_iter()
            .filter(|name| name.contains("TextArea"))
            .collect();
    let expected_compiler = [
        "AtomicProjectionSelectionTextAreaPlanIdentity",
        "AtomicProjectionTextAreaPlanIdentity",
        "FocusedAtomicProjectionTextAreaPlanIdentity",
        "RetainedAtomicProjectionSelectionTextAreaFrozenRasterDependencyIdentity",
        "RetainedAtomicProjectionSelectionTextAreaFrozenResidentRasterIdentity",
        "RetainedAtomicProjectionSelectionTextAreaRasterDependencySeal",
        "RetainedAtomicProjectionSelectionTextAreaResidentRasterSeal",
        "RetainedAtomicProjectionTextAreaFrozenRasterDependencyIdentity",
        "RetainedAtomicProjectionTextAreaFrozenResidentRasterIdentity",
        "RetainedAtomicProjectionTextAreaRasterDependencySeal",
        "RetainedAtomicProjectionTextAreaResidentRasterSeal",
        "ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentArtifact",
        "ValidatedScrollSceneAtomicProjectionSelectionTextAreaContentEmission",
        "ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostBeforeArtifact",
        "ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission",
        "ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayArtifact",
        "ValidatedScrollSceneAtomicProjectionSelectionTextAreaOverlayEmission",
        "ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts",
        "ValidatedScrollSceneAtomicProjectionTextAreaContentArtifact",
        "ValidatedScrollSceneAtomicProjectionTextAreaContentEmission",
        "ValidatedScrollSceneAtomicProjectionTextAreaHostBeforeArtifact",
        "ValidatedScrollSceneAtomicProjectionTextAreaHostEmission",
        "ValidatedScrollSceneAtomicProjectionTextAreaOverlayArtifact",
        "ValidatedScrollSceneAtomicProjectionTextAreaOverlayEmission",
        "ValidatedScrollSceneAtomicProjectionTextAreaPlanParts",
        "ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts",
        "ValidatedScrollSceneInteractiveTextAreaContentArtifact",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    assert_eq!(
        compiler, expected_compiler,
        "adding or removing a compiler.rs *TextArea* downstream type requires updating the Stage C deletion inventory; these types may survive only in the existing retained middle layer",
    );
}
