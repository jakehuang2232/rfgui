use super::*;

#[test]
fn width_and_height_emit_layout_transition_requests() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::All,
            200,
        ))),
    );
    el.apply_style(style);

    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(el));

    let c = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    let p = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };

    measure_and_place(&mut arena, key, c, p);
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_visual_transition_requests();

    let mut next_style = Style::new();
    next_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
    next_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(90.0)));
    crate::view::test_support::get_element_mut::<Element>(&arena, key).apply_style(next_style);
    measure_and_place(&mut arena, key, c, p);

    let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();
    assert!(reqs.iter().any(|r| r.field == LayoutField::Width));
    assert!(reqs.iter().any(|r| r.field == LayoutField::Height));
}

#[test]
fn reflow_uses_current_rendered_position_as_layout_transition_start() {
    let mut el = Element::new(50.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Position,
            200,
        ))),
    );
    el.apply_style(style);

    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(el));

    let constraints = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    let placement_at_100 = LayoutPlacement {
        parent_x: 100.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };

    measure_and_place(&mut arena, key, constraints, placement_at_100);
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_visual_transition_requests();

    // Simulate an in-flight visual offset frame: target rel-x=50, offset=30 => abs x = 180.
    crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .set_layout_transition_x(30.0);
    arena.with_element_taken(key, |el, a| el.place(placement_at_100, a));
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();

    // A reflow shifts parent origin and updates target x.
    crate::view::test_support::get_element_mut::<Element>(&arena, key).set_position(120.0, 0.0);
    let reflow_placement = LayoutPlacement {
        parent_x: 130.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    arena.with_element_taken(key, |el, a| el.place(reflow_placement, a));

    let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_visual_transition_requests();
    let x_req = reqs
        .iter()
        .find(|r| r.field == VisualField::X)
        .expect("x transition request should exist");
    // current rendered rel-x(80 = base 50 + offset 30) - new target rel-x(120) => offset = -40
    assert!((x_req.from + 40.0).abs() < 0.01);
    assert!((x_req.to - 0.0).abs() < 0.01);
}

#[test]
fn transition_start_frame_keeps_previous_visual_geometry() {
    let mut el = Element::new(50.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(
            [
                Transition::new(TransitionProperty::Position, 200),
                Transition::new(TransitionProperty::Width, 200),
            ]
            .into(),
        ),
    );
    el.apply_style(style);

    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(el));

    let constraints = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    let placement = LayoutPlacement {
        parent_x: 100.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };

    measure_and_place(&mut arena, key, constraints, placement);
    {
        let mut el_mut = crate::view::test_support::get_element_mut::<Element>(&arena, key);
        let _ = el_mut.take_layout_transition_requests();
        let _ = el_mut.take_visual_transition_requests();
    }

    {
        let mut el_mut = crate::view::test_support::get_element_mut::<Element>(&arena, key);
        el_mut.set_position(120.0, 0.0);
        let mut next_style = Style::new();
        next_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
        el_mut.apply_style(next_style);
    }
    measure_and_place(&mut arena, key, constraints, placement);

    let el_ref = crate::view::test_support::get_element::<Element>(&arena, key);
    let snapshot = el_ref.box_model_snapshot();
    assert!((snapshot.x - 150.0).abs() < 0.01);
    assert!((snapshot.width - 100.0).abs() < 0.01);
    assert!((el_ref.layout_transition_visual_offset_x + 70.0).abs() < 0.01);
    assert_eq!(el_ref.layout_transition_override_width, Some(100.0));
    drop(el_ref);
    let mut el_mut = crate::view::test_support::get_element_mut::<Element>(&arena, key);
    let layout_reqs = el_mut.take_layout_transition_requests();
    let visual_reqs = el_mut.take_visual_transition_requests();
    assert!(visual_reqs.iter().any(|req| req.field == VisualField::X));
    assert!(
        layout_reqs
            .iter()
            .any(|req| req.field == LayoutField::Width)
    );
}

#[test]
fn reflow_uses_current_rendered_width_as_layout_transition_start() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Width,
            200,
        ))),
    );
    el.apply_style(style);

    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(el));

    let constraints = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    let placement = LayoutPlacement {
        parent_x: 100.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };

    measure_and_place(&mut arena, key, constraints, placement);
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();

    // Simulate in-flight width frame.
    crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .set_layout_transition_width(140.0);
    arena.with_element_taken(key, |el, a| el.place(placement, a));
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();

    // Reflow updates target width while parent origin also changes.
    {
        let mut next_style = Style::new();
        next_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        crate::view::test_support::get_element_mut::<Element>(&arena, key)
            .apply_style(next_style);
    }
    let reflow_placement = LayoutPlacement {
        parent_x: 130.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    measure_and_place(&mut arena, key, constraints, reflow_placement);

    let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();
    let w_req = reqs
        .iter()
        .find(|r| r.field == LayoutField::Width)
        .expect("width transition request should exist");
    assert!((w_req.from - 140.0).abs() < 0.01, "{w_req:?}");
    assert!((w_req.to - 220.0).abs() < 0.01, "{w_req:?}");
}

#[test]
fn reflow_uses_current_rendered_height_as_layout_transition_start() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Height,
            200,
        ))),
    );
    el.apply_style(style);

    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(el));

    let constraints = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    let placement = LayoutPlacement {
        parent_x: 100.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };

    measure_and_place(&mut arena, key, constraints, placement);
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();

    // Simulate in-flight height frame.
    crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .set_layout_transition_height(70.0);
    arena.with_element_taken(key, |el, a| el.place(placement, a));
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();

    // Reflow updates target height while parent origin also changes.
    {
        let mut next_style = Style::new();
        next_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(160.0)));
        crate::view::test_support::get_element_mut::<Element>(&arena, key)
            .apply_style(next_style);
    }
    let reflow_placement = LayoutPlacement {
        parent_x: 130.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    measure_and_place(&mut arena, key, constraints, reflow_placement);

    let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();
    let h_req = reqs
        .iter()
        .find(|r| r.field == LayoutField::Height)
        .expect("height transition request should exist");
    assert!((h_req.from - 70.0).abs() < 0.01, "{h_req:?}");
    assert!((h_req.to - 160.0).abs() < 0.01, "{h_req:?}");
}

#[test]
fn height_transition_retargets_to_latest_assigned_height_midflight() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Height,
            200,
        ))),
    );
    el.apply_style(style);

    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(el));

    let constraints = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };

    measure_and_place(&mut arena, key, constraints, placement);
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();

    {
        let mut expanded_style = Style::new();
        expanded_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(160.0)));
        crate::view::test_support::get_element_mut::<Element>(&arena, key)
            .apply_style(expanded_style);
    }
    measure_and_place(&mut arena, key, constraints, placement);
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();

    crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .set_layout_transition_height(70.0);
    arena.with_element_taken(key, |el, a| el.place(placement, a));
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();

    {
        let mut collapsed_style = Style::new();
        collapsed_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        crate::view::test_support::get_element_mut::<Element>(&arena, key)
            .apply_style(collapsed_style);
    }
    measure_and_place(&mut arena, key, constraints, placement);

    let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, key)
        .take_layout_transition_requests();
    let h_req = reqs
        .iter()
        .find(|r| r.field == LayoutField::Height)
        .expect("height transition request should retarget");
    assert!((h_req.from - 70.0).abs() < 0.01);
    assert!((h_req.to - 20.0).abs() < 0.01);
}

#[test]
fn seed_layout_snapshot_keeps_flow_and_visual_positions_separate() {
    let mut old = Element::new_with_id(42, 50.0, 0.0, 100.0, 40.0);
    old.has_layout_snapshot = true;
    old.last_layout_placement = Some(LayoutPlacement {
        parent_x: 100.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 100.0,
        available_height: 40.0,
        viewport_width: 100.0,
        viewport_height: 40.0,
        percent_base_width: Some(100.0),
        percent_base_height: Some(40.0),
    });
    old.last_parent_layout_x = 100.0;
    old.last_parent_layout_y = 0.0;
    old.layout_state.layout_flow_position = LayoutPosition { x: 170.0, y: 0.0 };
    old.layout_state.layout_position = LayoutPosition { x: 150.0, y: 0.0 };
    old.layout_transition_visual_offset_x = -20.0;
    old.layout_transition_target_x = Some(70.0);

    let mut arena_old = new_test_arena();
    let old_key = commit_element(&mut arena_old, Box::new(old));
    let layout_snapshots =
        crate::view::viewport::transitions_tick::collect_layout_transition_snapshots(
            &arena_old,
            &[old_key],
        );

    let mut rebuilt = Element::new_with_id(42, 50.0, 0.0, 100.0, 40.0);
    rebuilt.has_layout_snapshot = true;
    rebuilt.layout_transition_visual_offset_x = -20.0;
    rebuilt.layout_transition_target_x = Some(70.0);
    let mut arena = new_test_arena();
    let rebuilt_key = commit_element(&mut arena, Box::new(rebuilt));
    crate::view::viewport::transitions_tick::seed_layout_transition_snapshots(
        &mut arena,
        &[rebuilt_key],
        &layout_snapshots,
    );

    {
        let rebuilt_ref =
            crate::view::test_support::get_element::<Element>(&arena, rebuilt_key);
        assert_eq!(rebuilt_ref.layout_state.layout_position.x, 150.0);
        assert_eq!(rebuilt_ref.layout_state.layout_flow_position.x, 170.0);
    }

    arena.with_element_taken(rebuilt_key, |el, a| {
        el.place(
            LayoutPlacement {
                parent_x: 100.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            a,
        );
    });

    let rebuilt_ref = crate::view::test_support::get_element::<Element>(&arena, rebuilt_key);
    assert!((rebuilt_ref.layout_state.layout_position.x - 150.0).abs() < 0.01);
}
