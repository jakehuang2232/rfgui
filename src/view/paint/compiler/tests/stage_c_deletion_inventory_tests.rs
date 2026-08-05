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

fn declared_text_area_types(source: &str) -> std::collections::BTreeSet<String> {
    source
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
        .collect()
}

#[test]
fn stage_c_deletion_inventory_rejects_unregistered_downstream_text_area_types() {
    let compiler = declared_text_area_types(include_str!("../../compiler.rs"));
    let scroll_scene = declared_text_area_types(include_str!("../../scroll_scene.rs"));
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
    let expected_scroll_scene = ["PropertyScrollInteractiveTextAreaCaretSeal"]
        .into_iter()
        .map(str::to_string)
        .collect();

    assert_eq!(
        compiler, expected_compiler,
        "compiler.rs 新增或移除 *TextArea* downstream 型別時，必須同步更新 Stage C deletion inventory；這些型別只能在既有 retained 中層存活",
    );
    assert_eq!(
        scroll_scene, expected_scroll_scene,
        "scroll_scene.rs 新增或移除 *TextArea* downstream 型別時，必須同步更新 Stage C deletion inventory；這些型別只能在既有 retained 中層存活",
    );
}
