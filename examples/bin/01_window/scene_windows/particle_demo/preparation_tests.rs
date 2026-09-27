use super::*;
#[test]
fn particle_preparation_freezes_once_per_frame_and_clears_geometry_dirty() {
    let now = Instant::now();
    PARTICLE_SYSTEM.with(|s| {
        let mut s = s.borrow_mut();
        *s = ParticleSystemInner::new();
        s.last_update = now;
    });
    let mut canvas = ParticleCanvas::new(77);
    canvas.layout_w = 80.;
    canvas.layout_h = 64.;
    canvas.dirty = DirtyFlags::NONE;
    let context = PaintResourcePreparationContext {
        frame_number: 1,
        device_scale: 2.,
        now: now + std::time::Duration::from_millis(32),
    };
    canvas.prepare_paint_resources(context);
    let first = canvas.source.clone().unwrap();
    assert_eq!(first.extent(), [160, 128]);
    assert_eq!(first.revision(), 1);
    let elapsed = PARTICLE_SYSTEM.with(|s| s.borrow().elapsed);
    assert!(elapsed > 0.);
    canvas.prepare_paint_resources(PaintResourcePreparationContext {
        now: context.now + std::time::Duration::from_secs(1),
        ..context
    });
    assert_eq!(canvas.source.as_ref(), Some(&first));
    assert_eq!(PARTICLE_SYSTEM.with(|s| s.borrow().elapsed), elapsed);
    assert_eq!(canvas.local_dirty_flags(), DirtyFlags::PAINT);
    canvas.clear_local_dirty_flags(DirtyFlags::PAINT);
    assert_eq!(canvas.local_dirty_flags(), DirtyFlags::NONE);
    canvas.prepare_paint_resources(PaintResourcePreparationContext {
        frame_number: 2,
        now: context.now + std::time::Duration::from_millis(16),
        ..context
    });
    assert_eq!(canvas.source.as_ref().unwrap().revision(), 2);
    assert_ne!(canvas.source.as_ref(), Some(&first));
    canvas.should_render = false;
    canvas.prepare_paint_resources(PaintResourcePreparationContext {
        frame_number: 3,
        ..context
    });
    assert!(canvas.source.is_none());
}

#[test]
fn animation_switch_freezes_particles_resizes_and_resumes_without_catching_up() {
    let now = Instant::now();
    PARTICLE_SYSTEM.with(|system| {
        let mut system = system.borrow_mut();
        *system = ParticleSystemInner::new();
        system.last_update = now;
    });
    let animation_on = Binding::new(true);
    let mut canvas = ParticleCanvas::new(78);
    canvas.animation_on = Some(animation_on.clone());
    canvas.layout_w = 80.;
    canvas.layout_h = 64.;
    let mut context = PaintResourcePreparationContext {
        frame_number: 1,
        device_scale: 1.,
        now: now + std::time::Duration::from_millis(32),
    };
    canvas.prepare_paint_resources(context);
    let first = canvas.source.clone().unwrap();
    let frozen = PARTICLE_SYSTEM.with(|s| (s.borrow().elapsed, s.borrow().particles.len()));
    crate::rfgui::ui::batch_state_updates(|| animation_on.set(false));
    context.frame_number += 1;
    context.now += std::time::Duration::from_secs(10);
    canvas.prepare_paint_resources(context);
    assert_eq!(canvas.source.as_ref(), Some(&first));
    assert_eq!(
        PARTICLE_SYSTEM.with(|s| (s.borrow().elapsed, s.borrow().particles.len())),
        frozen
    );
    canvas.layout_w = 100.;
    context.frame_number += 1;
    canvas.prepare_paint_resources(context);
    assert_eq!(canvas.source.as_ref().unwrap().extent(), [100, 64]);
    assert_eq!(
        PARTICLE_SYSTEM.with(|s| (s.borrow().elapsed, s.borrow().particles.len())),
        frozen
    );
    let before_scale = canvas.source.as_ref().unwrap().revision();
    canvas.layout_w = 50.;
    canvas.layout_h = 32.;
    context.device_scale = 2.;
    context.frame_number += 1;
    canvas.prepare_paint_resources(context);
    assert_eq!(canvas.source.as_ref().unwrap().extent(), [100, 64]);
    assert!(canvas.source.as_ref().unwrap().revision() > before_scale);
    crate::rfgui::ui::batch_state_updates(|| animation_on.set(true));
    context.frame_number += 1;
    context.now += std::time::Duration::from_secs(60);
    canvas.prepare_paint_resources(context);
    assert_eq!(PARTICLE_SYSTEM.with(|s| s.borrow().elapsed), frozen.0);
    context.frame_number += 1;
    context.now += std::time::Duration::from_millis(16);
    canvas.prepare_paint_resources(context);
    let elapsed = PARTICLE_SYSTEM.with(|s| s.borrow().elapsed);
    assert!((elapsed - frozen.0 - 0.016).abs() < 0.00001);
}

#[test]
fn particle_animation_request_respects_visibility_and_pause() {
    let now = Instant::now();
    let animation_on = Binding::new(true);
    let mut canvas = ParticleCanvas::new(79);
    canvas.animation_on = Some(animation_on.clone());
    assert_eq!(
        canvas.animation_frame_request(now),
        AnimationFrameRequest::NextFrame
    );
    canvas.should_render = false;
    assert_eq!(
        canvas.animation_frame_request(now),
        AnimationFrameRequest::None
    );
    canvas.should_render = true;
    rfgui::ui::batch_state_updates(|| animation_on.set(false));
    assert_eq!(
        canvas.animation_frame_request(now),
        AnimationFrameRequest::None
    );
    rfgui::ui::batch_state_updates(|| animation_on.set(true));
    assert_eq!(
        canvas.animation_frame_request(now),
        AnimationFrameRequest::NextFrame
    );
}
