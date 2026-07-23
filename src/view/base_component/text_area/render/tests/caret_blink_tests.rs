use super::*;

#[test]
fn retained_caret_blink_has_deterministic_boundaries_and_paint_only_dirty() {
    let mut text_area = TextArea::new();
    text_area.layout_state.should_render = true;
    assert!(!text_area.caret_visible);
    assert!(text_area.caret_blink_epoch.is_none());

    assert!(text_area.set_focused(true));
    assert!(text_area.caret_visible);
    assert!(text_area.caret_blink_epoch.is_none());
    assert!(
        <TextArea as crate::view::base_component::EventTarget>::wants_animation_frame(
            &text_area
        )
    );

    let t0 = crate::time::Instant::now();
    text_area.dirty_flags = DirtyFlags::NONE;
    assert_eq!(text_area.tick_caret_blink(t0), DirtyFlags::NONE);
    assert_eq!(text_area.caret_blink_epoch, Some(t0));
    assert!(text_area.caret_visible);

    assert_eq!(
        text_area.tick_caret_blink(t0 + Duration::from_millis(529)),
        DirtyFlags::NONE
    );
    assert!(text_area.caret_visible);
    assert!(text_area.dirty_flags.is_empty());

    assert_eq!(
        text_area.tick_caret_blink(t0 + Duration::from_millis(530)),
        DirtyFlags::PAINT
    );
    assert!(!text_area.caret_visible);
    assert_eq!(text_area.dirty_flags, DirtyFlags::PAINT);
    assert!(
        <TextArea as crate::view::base_component::EventTarget>::wants_animation_frame(
            &text_area
        ),
        "the invisible blink phase must keep requesting frames"
    );

    text_area.dirty_flags = DirtyFlags::NONE;
    assert_eq!(
        text_area.tick_caret_blink(t0 + Duration::from_millis(1059)),
        DirtyFlags::NONE
    );
    assert!(!text_area.caret_visible);
    assert!(text_area.dirty_flags.is_empty());
    assert_eq!(
        text_area.tick_caret_blink(t0 + Duration::from_millis(1060)),
        DirtyFlags::PAINT
    );
    assert!(text_area.caret_visible);
    assert_eq!(text_area.dirty_flags, DirtyFlags::PAINT);
    assert!(
        !text_area.dirty_flags.intersects(
            DirtyFlags::LAYOUT
                .union(DirtyFlags::PLACE)
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST)
                .union(DirtyFlags::COMPOSITE)
        )
    );
}

#[test]
fn retained_caret_focus_reset_blur_and_unrender_restart_without_clock_reads() {
    let mut text_area = TextArea::new();
    text_area.layout_state.should_render = true;
    text_area.set_focused(true);
    let t0 = crate::time::Instant::now();
    assert_eq!(text_area.tick_caret_blink(t0), DirtyFlags::NONE);
    assert_eq!(
        text_area.tick_caret_blink(t0 + Duration::from_millis(530)),
        DirtyFlags::PAINT
    );
    assert!(!text_area.caret_visible);

    text_area.dirty_flags = DirtyFlags::NONE;
    text_area.reset_caret_blink();
    assert!(text_area.caret_visible);
    assert!(text_area.caret_blink_epoch.is_none());
    assert_eq!(text_area.dirty_flags, DirtyFlags::PAINT);

    text_area.caret_visible = false;
    text_area.caret_blink_epoch = Some(t0);
    assert!(text_area.insert_text("x"));
    assert!(text_area.caret_visible);
    assert!(text_area.caret_blink_epoch.is_none());

    text_area.dirty_flags = DirtyFlags::NONE;
    text_area.layout_state.should_render = false;
    assert_eq!(
        text_area.tick_caret_blink(t0 + Duration::from_millis(600)),
        DirtyFlags::PAINT
    );
    assert!(!text_area.caret_visible);
    assert!(text_area.caret_blink_epoch.is_none());
    assert!(
        !<TextArea as crate::view::base_component::EventTarget>::wants_animation_frame(
            &text_area
        )
    );

    text_area.dirty_flags = DirtyFlags::NONE;
    text_area.layout_state.should_render = true;
    assert_eq!(
        text_area.tick_caret_blink(t0 + Duration::from_millis(700)),
        DirtyFlags::PAINT
    );
    assert!(text_area.caret_visible);
    assert_eq!(
        text_area.caret_blink_epoch,
        Some(t0 + Duration::from_millis(700))
    );

    text_area.dirty_flags = DirtyFlags::NONE;
    assert!(text_area.set_focused(false));
    assert!(!text_area.caret_visible);
    assert!(text_area.caret_blink_epoch.is_none());
    assert_eq!(text_area.dirty_flags, DirtyFlags::PAINT);
    assert!(
        !<TextArea as crate::view::base_component::EventTarget>::wants_animation_frame(
            &text_area
        )
    );
}
