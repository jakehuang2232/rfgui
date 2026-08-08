use super::*;

fn register_stage_c_deletion_type<T>() {}

#[test]
fn stage_c_deletion_inventory_keeps_scroll_scene_types_compile_time_linked() {
    register_stage_c_deletion_type::<PropertyScrollInteractiveTextAreaCaretSeal>();
}

#[test]
fn stage_c_deletion_inventory_rejects_unregistered_scroll_scene_text_area_types() {
    let declared: std::collections::BTreeSet<String> =
        crate::view::paint::tests::declared_top_level_type_names(include_str!(
            "../../scroll_scene.rs"
        ))
        .into_iter()
        .filter(|name| name.contains("TextArea"))
        .collect();
    let expected = ["PropertyScrollInteractiveTextAreaCaretSeal"]
        .map(str::to_string)
        .into_iter()
        .collect();
    assert_eq!(
        declared, expected,
        "adding or removing a scroll_scene.rs *TextArea* downstream type requires updating the Stage C deletion inventory; these types may survive only in the existing retained middle layer",
    );
}
