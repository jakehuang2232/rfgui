use super::*;

#[test]
fn opacity_style_sample_updates_arena_paint_dirty_cache() {
    let (mut arena, root_key, child_key, child_id) = clean_style_sample_arena();

    assert!(set_style_field_by_id(
        &mut arena,
        root_key,
        child_id,
        crate::transition::StyleField::Opacity,
        crate::transition::StyleValue::Scalar(0.42),
    ));

    let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
    assert!((child.debug_render_state().opacity - 0.42).abs() < 0.001);
    assert_style_sample_dirty_flags(
        &arena,
        root_key,
        child_key,
        DirtyFlags::PAINT.union(DirtyFlags::COMPOSITE),
    );
}

macro_rules! color_style_sample_dirty_cache_test {
    ($name:ident, $style_field:ident, $debug_field:ident, $color:expr) => {
        #[test]
        fn $name() {
            let (mut arena, root_key, child_key, child_id) = clean_style_sample_arena();
            let color = $color;

            assert!(set_style_field_by_id(
                &mut arena,
                root_key,
                child_id,
                crate::transition::StyleField::$style_field,
                crate::transition::StyleValue::Color(color),
            ));

            let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
            assert_eq!(child.debug_render_state().$debug_field, color.to_rgba_u8());
            assert_style_sample_paint_dirty(&arena, root_key, child_key);
        }
    };
}

color_style_sample_dirty_cache_test!(
    background_color_style_sample_updates_arena_paint_dirty_cache,
    BackgroundColor,
    background_rgba,
    Color::rgb(249, 115, 22)
);
color_style_sample_dirty_cache_test!(
    foreground_color_style_sample_updates_arena_paint_dirty_cache,
    Color,
    foreground_rgba,
    Color::rgb(90, 80, 70)
);
color_style_sample_dirty_cache_test!(
    border_top_color_style_sample_updates_arena_paint_dirty_cache,
    BorderTopColor,
    border_top_rgba,
    Color::rgba(11, 22, 33, 210)
);
color_style_sample_dirty_cache_test!(
    border_right_color_style_sample_updates_arena_paint_dirty_cache,
    BorderRightColor,
    border_right_rgba,
    Color::rgba(44, 55, 66, 220)
);
color_style_sample_dirty_cache_test!(
    border_bottom_color_style_sample_updates_arena_paint_dirty_cache,
    BorderBottomColor,
    border_bottom_rgba,
    Color::rgba(77, 88, 99, 230)
);
color_style_sample_dirty_cache_test!(
    border_left_color_style_sample_updates_arena_paint_dirty_cache,
    BorderLeftColor,
    border_left_rgba,
    Color::rgba(101, 112, 123, 240)
);

#[test]
fn border_radius_style_sample_updates_arena_paint_dirty_cache() {
    let (mut arena, root_key, child_key, child_id) = clean_style_sample_arena();

    assert!(set_style_field_by_id(
        &mut arena,
        root_key,
        child_id,
        crate::transition::StyleField::BorderRadius,
        crate::transition::StyleValue::Scalar(8.0),
    ));

    assert_style_sample_paint_dirty(&arena, root_key, child_key);
}

#[test]
fn box_shadow_style_sample_updates_arena_paint_dirty_cache() {
    let (mut arena, root_key, child_key, child_id) = clean_style_sample_arena();
    let shadows = vec![
        BoxShadow::new()
            .color(Color::hex("#223344"))
            .offset_x(6.0)
            .offset_y(8.0)
            .blur(12.0)
            .spread(4.0)
            .inset(true),
    ];

    assert!(set_style_field_by_id(
        &mut arena,
        root_key,
        child_id,
        crate::transition::StyleField::BoxShadow,
        crate::transition::StyleValue::BoxShadow(shadows.clone()),
    ));

    let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
    assert_eq!(child.box_shadows, shadows);
    assert_style_sample_paint_dirty(&arena, root_key, child_key);
}

#[test]
fn transform_style_sample_updates_arena_place_dirty_cache() {
    let (mut arena, root_key, child_key, child_id) = clean_style_sample_arena();
    let transform = Transform::new([Translate::xy(Length::px(12.0), Length::px(18.0))]);

    assert!(set_style_field_by_id(
        &mut arena,
        root_key,
        child_id,
        crate::transition::StyleField::Transform,
        crate::transition::StyleValue::Transform(transform.clone()),
    ));

    let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
    assert_eq!(child.transform, transform);
    assert!(child.resolved_transform.is_some());
    assert_style_sample_dirty_flags(
        &arena,
        root_key,
        child_key,
        style_sample_place_dirty_flags(),
    );
}

#[test]
fn transform_origin_style_sample_updates_arena_place_dirty_cache() {
    let (mut arena, root_key, child_key, child_id) = clean_style_sample_arena();

    assert!(set_style_field_by_id(
        &mut arena,
        root_key,
        child_id,
        crate::transition::StyleField::TransformOrigin,
        crate::transition::StyleValue::TransformOriginProgress {
            from: TransformOrigin::percent(50.0, 50.0),
            to: TransformOrigin::px(10.0, 20.0),
            progress: 0.5,
        },
    ));

    let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
    assert!(child.resolved_transform.is_none());
    assert!(
        (child
            .transform_origin
            .x()
            .resolve_without_percent_base(0.0, 0.0)
            - 25.0)
            .abs()
            < 0.0001
    );
    assert!(
        (child
            .transform_origin
            .y()
            .resolve_without_percent_base(0.0, 0.0)
            - 20.0)
            .abs()
            < 0.0001
    );
    assert_style_sample_dirty_flags(
        &arena,
        root_key,
        child_key,
        style_sample_place_dirty_flags(),
    );
}

#[test]
fn paint_only_style_sample_rejects_mismatched_value_type() {
    let (mut arena, root_key, child_key, child_id) = clean_style_sample_arena();

    assert!(!set_style_field_by_id(
        &mut arena,
        root_key,
        child_id,
        crate::transition::StyleField::Opacity,
        crate::transition::StyleValue::Color(Color::rgb(1, 2, 3)),
    ));
    assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
    assert!(
        !arena
            .cached_subtree_dirty(root_key)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn paint_only_style_sample_rejects_wrong_root() {
    let (mut arena, root_key, child_key, child_id) = clean_style_sample_arena();
    let other_root = commit_element(&mut arena, Box::new(Element::new(0.0, 0.0, 10.0, 10.0)));

    assert!(!set_style_field_by_id(
        &mut arena,
        other_root,
        child_id,
        crate::transition::StyleField::BackgroundColor,
        crate::transition::StyleValue::Color(Color::rgb(1, 2, 3)),
    ));
    assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
    assert!(
        !arena
            .cached_subtree_dirty(root_key)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn paint_only_style_sample_rejects_missing_stable_id() {
    let (mut arena, root_key, child_key, _child_id) = clean_style_sample_arena();

    assert!(!set_style_field_by_id(
        &mut arena,
        root_key,
        u64::MAX,
        crate::transition::StyleField::BorderTopColor,
        crate::transition::StyleValue::Color(Color::rgb(1, 2, 3)),
    ));
    assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
    assert!(
        !arena
            .cached_subtree_dirty(root_key)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn transform_style_sample_updates_element_transform_matrix() {
    let mut arena = new_test_arena();
    let el = Element::new(0.0, 0.0, 200.0, 150.0);
    let node_id = el.stable_id();
    let transform = Transform::new([Translate::xy(Length::px(12.0), Length::px(18.0))]);
    let key = commit_element(&mut arena, Box::new(el));

    assert!(set_style_field_by_id(
        &mut arena,
        key,
        node_id,
        crate::transition::StyleField::Transform,
        crate::transition::StyleValue::Transform(transform.clone()),
    ));

    let el = crate::view::test_support::get_element::<Element>(&arena, key);
    assert_eq!(el.transform, transform);
    assert!(el.resolved_transform.is_some());
}

#[test]
fn box_shadow_style_sample_updates_element_shadows() {
    let mut arena = new_test_arena();
    let el = Element::new(0.0, 0.0, 200.0, 150.0);
    let node_id = el.stable_id();
    let shadows = vec![
        BoxShadow::new()
            .color(Color::hex("#223344"))
            .offset_x(6.0)
            .offset_y(8.0)
            .blur(12.0)
            .spread(4.0)
            .inset(true),
    ];
    let key = commit_element(&mut arena, Box::new(el));

    assert!(set_style_field_by_id(
        &mut arena,
        key,
        node_id,
        crate::transition::StyleField::BoxShadow,
        crate::transition::StyleValue::BoxShadow(shadows.clone()),
    ));

    let el = crate::view::test_support::get_element::<Element>(&arena, key);
    assert_eq!(el.box_shadows, shadows);
}

#[test]
fn transform_origin_style_sample_updates_element_transform_matrix() {
    let mut arena = new_test_arena();
    let el = Element::new(0.0, 0.0, 200.0, 150.0);
    let node_id = el.stable_id();
    let key = commit_element(&mut arena, Box::new(el));

    assert!(set_style_field_by_id(
        &mut arena,
        key,
        node_id,
        crate::transition::StyleField::TransformOrigin,
        crate::transition::StyleValue::TransformOriginProgress {
            from: TransformOrigin::percent(50.0, 50.0),
            to: TransformOrigin::px(10.0, 20.0),
            progress: 0.5,
        },
    ));

    let el = crate::view::test_support::get_element::<Element>(&arena, key);
    assert!(el.resolved_transform.is_none());
    assert!(
        (el.transform_origin
            .x()
            .resolve_without_percent_base(0.0, 0.0)
            - 55.0)
            .abs()
            < 0.0001
    );
    assert!(
        (el.transform_origin
            .y()
            .resolve_without_percent_base(0.0, 0.0)
            - 47.5)
            .abs()
            < 0.0001
    );
}

#[test]
fn transform_transition_baseline_preserves_start_then_progress_updates_live_transform() {
    let mut arena = new_test_arena();
    let mut el = Element::new(0.0, 0.0, 200.0, 150.0);
    let from = Transform::new([Translate::x(Length::px(0.0))]);
    let to = Transform::new([Translate::x(Length::px(40.0))]);
    let mut style = Style::new();
    style.set_transform(from.clone());
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::from(vec![Transition::new(
            TransitionProperty::Transform,
            180,
        )])),
    );
    let mut hover_style = Style::new();
    hover_style.set_transform(to.clone());
    style.set_hover(hover_style);
    el.apply_style(style);

    let _ = el.set_hovered(true);
    assert_eq!(el.transform, from);

    let node_id = el.stable_id();
    let key = commit_element(&mut arena, Box::new(el));

    assert!(set_style_field_by_id(
        &mut arena,
        key,
        node_id,
        crate::transition::StyleField::Transform,
        crate::transition::StyleValue::TransformProgress {
            from: from.clone(),
            to: to.clone(),
            progress: 0.5,
        },
    ));

    let el = crate::view::test_support::get_element::<Element>(&arena, key);
    assert_ne!(el.transform, from);
    assert_ne!(el.transform, to);
}
