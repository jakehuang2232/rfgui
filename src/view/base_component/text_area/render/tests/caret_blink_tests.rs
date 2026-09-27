use super::*;
use crate::view::base_component::AnimationFrameRequest;

#[test]
fn caret_deadlines_follow_phase_without_rounding_or_catchup_frames() {
    let mut text = TextArea::new();
    let now = crate::time::Instant::now();
    text.layout_state.should_render = true;
    text.set_focused(true);
    assert_eq!(
        text.animation_frame_request(now),
        AnimationFrameRequest::NextFrame
    );
    text.tick_caret_blink(now);
    for offset in [
        0,
        1,
        529_999_999,
        530_000_000,
        1_059_999_999,
        1_060_000_000,
        5_829_000_000,
    ] {
        let sample = now + Duration::from_nanos(offset);
        text.tick_caret_blink(sample);
        let next = (offset / 530_000_000 + 1) * 530_000_000;
        assert_eq!(
            text.animation_frame_request(sample),
            AnimationFrameRequest::At(now + Duration::from_nanos(next))
        );
    }
    text.reset_caret_blink();
    assert_eq!(
        text.animation_frame_request(now),
        AnimationFrameRequest::NextFrame
    );
    text.set_focused(false);
    assert_eq!(
        text.animation_frame_request(now),
        AnimationFrameRequest::None
    );
    text.set_focused(true);
    text.layout_state.should_render = false;
    assert_eq!(
        text.animation_frame_request(now),
        AnimationFrameRequest::None
    );
}

#[test]
fn retained_caret_blink_has_deterministic_boundaries_and_paint_only_dirty() {
    let mut text_area = TextArea::new();
    text_area.layout_state.should_render = true;
    assert!(!text_area.caret_visible);
    assert!(text_area.caret_blink_epoch.is_none());

    assert!(text_area.set_focused(true));
    assert!(text_area.caret_visible);
    assert!(text_area.caret_blink_epoch.is_none());
    let t0 = crate::time::Instant::now();
    assert_eq!(
        text_area.animation_frame_request(t0),
        AnimationFrameRequest::NextFrame
    );
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
    assert_eq!(
        text_area.animation_frame_request(t0 + Duration::from_millis(530)),
        AnimationFrameRequest::At(t0 + Duration::from_millis(1060)),
        "the invisible blink phase waits until the next visible phase"
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
fn active_preedit_reaches_the_hidden_caret_blink_phase() {
    let mut text_area = TextArea::new();
    text_area.layout_state.should_render = true;
    assert!(text_area.set_focused(true));
    assert!(text_area.set_preedit("中".to_string(), Some((0, "中".len()))));

    let t0 = crate::time::Instant::now();
    assert_eq!(text_area.tick_caret_blink(t0), DirtyFlags::NONE);
    assert_eq!(
        text_area.tick_caret_blink(t0 + Duration::from_millis(530)),
        DirtyFlags::PAINT,
    );

    assert!(!text_area.caret_visible);
    assert_eq!(text_area.ime_preedit, "中");
    assert_eq!(text_area.ime_preedit_cursor, Some((0, "中".len())));
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
    assert_eq!(
        text_area.animation_frame_request(t0 + Duration::from_millis(600)),
        AnimationFrameRequest::None
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
    assert_eq!(
        text_area.animation_frame_request(t0 + Duration::from_millis(700)),
        AnimationFrameRequest::None
    );
}
