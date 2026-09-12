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
