use super::*;

fn placed(assignments: [Option<f32>; 2]) -> Element {
    let mut e = Element::new(0., 0., 100., 24.);
    e.last_layout_placement = Some(LayoutPlacement {
        parent_x: 0.,
        parent_y: 0.,
        visual_offset_x: 0.,
        visual_offset_y: 0.,
        available_width: 400.,
        available_height: 400.,
        viewport_width: 640.,
        viewport_height: 480.,
        percent_base_width: Some(400.),
        percent_base_height: Some(400.),
    });
    e.layout_assigned_width = assignments[0];
    e.layout_assigned_height = assignments[1];
    e.layout_dirty = false;
    e.dirty_flags = DirtyFlags::NONE;
    e
}

#[test]
fn exact_reassignment_preserves_existing_paint_damage_and_is_consumed() {
    for paint in [DirtyFlags::NONE, DirtyFlags::PAINT] {
        let mut e = placed([None, Some(24.)]);
        e.dirty_flags = paint;
        e.clear_reusable_measure_assignment();
        assert_eq!(e.layout_assigned_height, None);
        e.restore_reusable_axis_assignment(false, 24., 100., false);
        e.set_layout_height(24.);
        assert_eq!(e.dirty_flags, paint);
        assert!(e.reusable_measure_assignment.is_none());
        // An unrelated later assignment still invokes the normal dirty path.
        e.set_layout_height(25.);
        assert!(e.dirty_flags.intersects(DirtyPassMask::PLACEMENT));
    }
}

#[test]
fn removed_cross_assignment_changed_axis_and_changed_size_reject_restore() {
    for (old, row, main) in [
        ([Some(100.), Some(24.)], false, 24.),
        ([None, Some(24.)], true, 100.),
        ([None, Some(24.)], false, 25.),
    ] {
        let mut e = placed(old);
        e.clear_reusable_measure_assignment();
        e.restore_reusable_axis_assignment(row, main, 100., false);
        assert_eq!(
            [e.layout_assigned_width, e.layout_assigned_height],
            [None, None]
        );
        if row {
            e.set_layout_width(main)
        } else {
            e.set_layout_height(main)
        }
        assert!(e.dirty_flags.intersects(DirtyPassMask::PLACEMENT));
    }
}

#[test]
fn transition_or_new_layout_damage_rejects_reassignment_proof() {
    for late in [false, true] {
        let mut e = placed([None, Some(24.)]);
        if !late {
            e.layout_transition_override_height = Some(12.);
        }
        e.clear_reusable_measure_assignment();
        if late {
            e.layout_transition_override_height = Some(12.);
        }
        e.restore_reusable_axis_assignment(false, 24., 100., false);
        assert_eq!(e.layout_assigned_height, None);
        e.set_layout_height(24.);
        assert!(e.dirty_flags.intersects(DirtyPassMask::PLACEMENT));
    }
    let mut e = placed([None, Some(24.)]);
    e.clear_reusable_measure_assignment();
    e.mark_layout_dirty();
    e.restore_reusable_axis_assignment(false, 24., 100., false);
    assert_eq!(e.layout_assigned_height, None);
    assert!(e.dirty_flags.intersects(DirtyPassMask::LAYOUT));
}

#[test]
fn repeated_reusable_measure_keeps_proof_but_real_measure_discards_it() {
    let mut e = placed([None, Some(24.)]);
    e.clear_reusable_measure_assignment();
    e.clear_reusable_measure_assignment();
    e.restore_reusable_axis_assignment(false, 24., 100., false);
    assert_eq!(e.layout_assigned_height, Some(24.));
    e.clear_reusable_measure_assignment();
    e.measure(
        LayoutConstraints {
            max_width: 400.,
            max_height: 400.,
            viewport_width: 640.,
            viewport_height: 480.,
            percent_base_width: Some(400.),
            percent_base_height: Some(400.),
        },
        &mut NodeArena::new(),
    );
    assert!(e.reusable_measure_assignment.is_none());
}
