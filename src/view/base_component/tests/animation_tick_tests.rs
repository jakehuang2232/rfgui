use super::*;

#[test]
fn animation_tick_visits_duplicate_and_cyclic_topology_once() {
    let root_ticks = Rc::new(Cell::new(0));
    let child_ticks = Rc::new(Cell::new(0));
    let root_wants_checks = Rc::new(Cell::new(0));
    let child_wants_checks = Rc::new(Cell::new(0));
    let root_tick_now = Rc::new(Cell::new(None));
    let root_post_tick_now = Rc::new(Cell::new(None));
    let root_resource_now = Rc::new(Cell::new(None));
    let child_tick_now = Rc::new(Cell::new(None));
    let child_post_tick_now = Rc::new(Cell::new(None));
    let child_resource_now = Rc::new(Cell::new(None));
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(AnimationTickProbe {
            id: 0xa001,
            children: Vec::new(),
            ticks: root_ticks.clone(),
            wants_checks: root_wants_checks.clone(),
            tick_now: root_tick_now.clone(),
            post_tick_now: root_post_tick_now.clone(),
            resource_now: root_resource_now.clone(),
        }),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(AnimationTickProbe {
            id: 0xa002,
            children: Vec::new(),
            ticks: child_ticks.clone(),
            wants_checks: child_wants_checks.clone(),
            tick_now: child_tick_now.clone(),
            post_tick_now: child_post_tick_now.clone(),
            resource_now: child_resource_now.clone(),
        }),
    );
    arena.set_children(root, vec![child, child]);
    arena.set_children(child, vec![root]);

    let semantic_now = crate::time::Instant::now();
    assert!(!tick_animation_frames(
        &mut arena,
        &[root, root],
        semantic_now
    ));
    assert!(!tick_post_layout_animation_frames(
        &mut arena,
        &[root, root],
        semantic_now,
    ));
    arena.prepare_registered_paint_resources(PaintResourcePreparationContext {
        frame_number: 7,
        device_scale: 1.0,
        now: semantic_now,
    });
    assert_eq!(root_ticks.get(), 1);
    assert_eq!(child_ticks.get(), 1);
    assert_eq!(root_tick_now.get(), Some(semantic_now));
    assert_eq!(root_post_tick_now.get(), Some(semantic_now));
    assert_eq!(root_resource_now.get(), Some(semantic_now));
    assert_eq!(child_tick_now.get(), Some(semantic_now));
    assert_eq!(child_post_tick_now.get(), Some(semantic_now));
    assert_eq!(child_resource_now.get(), Some(semantic_now));
    assert!(!super::super::has_animation_frame_request(&arena, root));
    assert_eq!(root_wants_checks.get(), 1);
    assert_eq!(child_wants_checks.get(), 1);
}
