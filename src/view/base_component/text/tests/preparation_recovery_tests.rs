use super::*;

#[test]
fn missing_standalone_preparation_recovers_without_an_edit_or_constraint_change() {
    let mut text = Text::new_with_id(0xc001, 0.0, 0.0, 180.0, 32.0, "unchanged text");
    place_text_for_read_only_ifc_test(&mut text, 200.0, 80.0);
    text.clear_local_dirty_flags(DirtyFlags::ALL);
    let original = text.shaped_context.clone().unwrap();
    let signature = text.retained_paint_signature();
    let size = text.measured_size();
    text.clear_prepared_standalone_text_for_test();
    assert!(text.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
    // Clearing explicit flags cannot conceal the missing preparation.
    text.clear_local_dirty_flags(DirtyFlags::ALL);
    assert!(text.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
    place_text_for_read_only_ifc_test(&mut text, 200.0, 80.0);
    assert!(std::sync::Arc::ptr_eq(
        &original,
        text.shaped_context.as_ref().unwrap()
    ));
    assert_eq!(text.measured_size(), size);
    assert_eq!(text.retained_paint_signature(), signature);
    assert!(!text.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
    assert!(!build_text_for_read_only_ifc_test(&mut text).is_empty());
}

#[test]
fn prepared_text_noop_keeps_context_and_does_not_request_measurement() {
    let mut text = Text::new_with_id(0xc002, 0.0, 0.0, 180.0, 32.0, "warm");
    place_text_for_read_only_ifc_test(&mut text, 200.0, 80.0);
    text.clear_local_dirty_flags(DirtyFlags::ALL);
    let context = text.shaped_context.clone().unwrap();
    place_text_for_read_only_ifc_test(&mut text, 200.0, 80.0);
    assert!(std::sync::Arc::ptr_eq(
        &context,
        text.shaped_context.as_ref().unwrap()
    ));
    assert!(text.local_dirty_flags().is_empty());
}
