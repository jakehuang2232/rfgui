use super::*;
use crate::time::Duration;
use crate::ui::{next_timer_deadline, run_due_timers};
use crate::view::base_component::{AnimationFrameRequest, ElementTrait, EventTarget, TextArea};
use crate::view::test_support::commit_element;

fn caret(epoch: Instant) -> TextArea {
    let mut caret = TextArea::new();
    caret.is_focused = true;
    caret.layout_state.should_render = true;
    caret.tick_animation_frame(epoch);
    caret
}

#[test]
fn deadline_aggregation_consumes_one_wakeup_and_clears_removed_nodes() {
    let now = Instant::now();
    let mut viewport = Viewport::new();
    let first = commit_element(&mut viewport.scene.node_arena, Box::new(caret(now)));
    let second = commit_element(
        &mut viewport.scene.node_arena,
        Box::new(caret(now - Duration::from_millis(200))),
    );
    // Duplicate roots and a cycle must not change the earliest request.
    viewport
        .scene
        .node_arena
        .set_children(first, vec![second, second]);
    viewport.scene.node_arena.set_children(second, vec![first]);
    viewport.scene.ui_root_keys = vec![first, first];
    viewport.take_redraw_request();
    viewport.update_animation_frame_schedule(now);
    let due = now + Duration::from_millis(330);
    assert_eq!(next_timer_deadline(), Some(due));
    assert!(!viewport.take_redraw_request());
    run_due_timers(due - Duration::from_nanos(1));
    assert!(!viewport.redraw_requested());
    run_due_timers(due);
    assert!(viewport.take_redraw_request());
    run_due_timers(due + Duration::from_secs(1));
    assert!(!viewport.take_redraw_request());

    viewport.update_animation_frame_schedule(now);
    viewport.scene.ui_root_keys.clear();
    viewport.update_animation_frame_schedule(now);
    assert_eq!(next_timer_deadline(), None);
}

#[test]
fn unsampled_caret_requests_immediate_frame_instead_of_future_deadline() {
    let now = Instant::now();
    let mut viewport = Viewport::new();
    let mut text = caret(now);
    text.caret_blink_epoch = None;
    let root = commit_element(&mut viewport.scene.node_arena, Box::new(text));
    viewport.scene.ui_root_keys = vec![root];
    viewport.take_redraw_request();
    viewport.update_animation_frame_schedule(now);
    assert_eq!(next_timer_deadline(), None);
    assert!(viewport.take_redraw_request());
}

#[test]
fn custom_animation_hook_explicitly_requests_frames_and_default_is_idle() {
    struct CustomAnimation(bool);
    impl EventTarget for CustomAnimation {
        fn animation_frame_request(&self, _now: Instant) -> AnimationFrameRequest {
            if self.0 {
                AnimationFrameRequest::NextFrame
            } else {
                AnimationFrameRequest::None
            }
        }
    }
    struct Idle;
    impl EventTarget for Idle {}
    let now = Instant::now();
    assert_eq!(
        Idle.animation_frame_request(now),
        AnimationFrameRequest::None
    );
    assert_eq!(
        CustomAnimation(true).animation_frame_request(now),
        AnimationFrameRequest::NextFrame
    );
    assert_eq!(
        CustomAnimation(false).animation_frame_request(now),
        AnimationFrameRequest::None
    );
}

#[test]
fn viewport_drop_and_hook_unmount_cancel_only_their_own_timers() {
    let now = Instant::now();
    let mut viewport = Viewport::new();
    let root = commit_element(&mut viewport.scene.node_arena, Box::new(caret(now)));
    viewport.scene.ui_root_keys = vec![root];
    viewport.take_redraw_request();
    viewport.update_animation_frame_schedule(now);
    let fired = std::rc::Rc::new(std::cell::Cell::new(0));
    let callback_count = fired.clone();
    crate::ui::build_scope(|| {
        crate::ui::render_component::<u128, _>(|| {
            crate::ui::use_timeout(true, Duration::from_secs(10), move || {
                callback_count.set(callback_count.get() + 1);
            });
        });
    });
    assert_eq!(
        next_timer_deadline(),
        Some(now + Duration::from_millis(530))
    );
    // Component GC must not remove the independently owned viewport deadline.
    crate::ui::build_scope(|| {
        crate::ui::render_component::<u8, _>(|| {});
    });
    assert_eq!(
        next_timer_deadline(),
        Some(now + Duration::from_millis(530))
    );
    run_due_timers(now + Duration::from_secs(20));
    assert!(viewport.drain_platform_requests().request_redraw);
    assert_eq!(fired.get(), 0);

    viewport.update_animation_frame_schedule(now);
    let mut other = Viewport::new();
    let root = commit_element(
        &mut other.scene.node_arena,
        Box::new(caret(now + Duration::from_secs(1))),
    );
    other.scene.ui_root_keys = vec![root];
    other.take_redraw_request();
    other.update_animation_frame_schedule(now + Duration::from_secs(1));
    drop(viewport);
    assert_eq!(
        next_timer_deadline(),
        Some(now + Duration::from_millis(1530))
    );
    run_due_timers(now + Duration::from_millis(1530));
    assert!(other.drain_platform_requests().request_redraw);
    assert!(!other.drain_platform_requests().request_redraw);
    drop(other);
    assert_eq!(next_timer_deadline(), None);
}

#[test]
fn animation_deadline_uses_existing_wakeup_without_dirtying_component_state() {
    let _ = crate::ui::take_state_dirty();
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let notified = count.clone();
    crate::ui::set_redraw_callback(move || notified.set(notified.get() + 1));
    let now = Instant::now();
    let mut viewport = Viewport::new();
    let root = commit_element(&mut viewport.scene.node_arena, Box::new(caret(now)));
    viewport.scene.ui_root_keys = vec![root];
    viewport.take_redraw_request();
    viewport.update_animation_frame_schedule(now);
    run_due_timers(now + Duration::from_millis(530));
    assert_eq!(count.get(), 1);
    assert!(!crate::ui::peek_state_dirty().has_any());
    assert!(viewport.drain_platform_requests().request_redraw);
    assert_eq!(next_timer_deadline(), None);
    run_due_timers(now + Duration::from_secs(1));
    assert_eq!(count.get(), 1);
    crate::ui::clear_redraw_callback();
}

#[test]
fn failed_acquisition_preserves_consumed_frontend_changes_for_retry() {
    let root = crate::ui::rsx! { <crate::view::Element /> };
    let mut viewport = Viewport::new();
    let now = Instant::now();
    viewport.render_rsx_at(&root, now, None).unwrap();
    assert!(viewport.frame.render_required);
    assert!(viewport.drain_platform_requests().request_redraw);
    let first_attempt = viewport.frame.frame_number;
    viewport.render_rsx_at(&root, now, None).unwrap();
    assert_eq!(viewport.frame.frame_number, first_attempt + 1);
    assert!(viewport.frame.render_required);
    assert!(viewport.drain_platform_requests().request_redraw);
    assert_eq!(viewport.frame.completion_counts.submits, 0);
}
