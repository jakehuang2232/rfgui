// TODO(session3-todo3): port tests to arena API. 5000+ lines of tests
// using legacy Box-tree add_child, single-arg measure/place/build,
// children().expect(...). Gated pending port.
//
// Prior agent attempts: imports already updated to the arena API (super
// exports, test_support helpers). Remaining work: ~117 `add_child`, ~300
// `measure`/`place` call-sites, ~69 `children()` accessors, 5 `build`
// calls to rewrite via `commit_child` / `with_element_taken` /
// `arena.children_of`. ~404 rustc errors when un-gated.
#[cfg(test)]
mod tests {
    use super::{
        expand_corner_radii_for_spread, main_axis_start_and_gap, normalize_corner_radii,
        resolve_px_with_base, resolve_signed_px_with_base, Element, ElementTrait, EventTarget,
        LayoutConstraints, LayoutPlacement, Layoutable, UiBuildContext,
    };
    use super::super::core::Position as LayoutPosition;
    use rustc_hash::{FxHashMap, FxHashSet};
    use crate::view::base_component::Text;
    use crate::style::{ParsedValue, PropertyId, Transition, TransitionProperty, Transitions};
    use crate::transition::{LayoutField, VisualField};
    use super::{reset_test_promoted_build_counts, test_promoted_build_count};
    use crate::view::base_component::set_style_field_by_id;
    use crate::view::test_support::{
        child_key, child_snapshot, commit_child, commit_element, measure_and_place,
        new_test_arena, nth_child_snapshot,
    };
    use crate::view::frame_graph::FrameGraph;
    use crate::Layout;
    use crate::{
        Align, AnchorName, Angle, Border, BorderRadius, BoxShadow, ClipMode, Collision,
        CollisionBoundary, Color, CrossSize, JustifyContent, Length, Opacity, Operator,
        Origin, Position, Rotate, Transform, TransformOrigin, Translate, Style,
    };
    use glam::{Mat4, Vec3};
    
    use std::sync::Arc;

    #[test]
    fn justify_content_space_evenly_distributes_free_space() {
        let (start, gap) =
            main_axis_start_and_gap(100.0, 40.0, 0.0, 3, JustifyContent::SpaceEvenly);
        assert!((start - 15.0).abs() < 0.001);
        assert!((gap - 15.0).abs() < 0.001);
    }

    #[test]
    fn child_layout_uses_parent_inner_box_with_padding() {
        let mut root = Element::new(10.0, 20.0, 200.0, 120.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        root.apply_style(root_style);
        root.set_padding_left(8.0);
        root.set_padding_top(12.0);
        root.set_padding_right(16.0);
        root.set_padding_bottom(10.0);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let child = Element::new(4.0, 6.0, 300.0, 300.0);
        let _child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            crate::view::base_component::LayoutPlacement {
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
            },
        );
        let snapshot = nth_child_snapshot(&arena, root_key, 0);

        assert_eq!(snapshot.x, 22.0);
        assert_eq!(snapshot.y, 38.0);
        assert_eq!(snapshot.width, 300.0);
        assert_eq!(snapshot.height, 300.0);
    }

    #[test]
    fn box_shadow_spread_keeps_per_corner_radii() {
        let base = normalize_corner_radii(
            super::CornerRadii {
                top_left: 4.0,
                top_right: 12.0,
                bottom_right: 20.0,
                bottom_left: 8.0,
            },
            120.0,
            80.0,
        );
        let spread = 6.0;
        let shadow = expand_corner_radii_for_spread(base, spread, 120.0, 80.0);

        assert!((shadow.top_left - 10.0).abs() < 0.001);
        assert!((shadow.top_right - 18.0).abs() < 0.001);
        assert!((shadow.bottom_right - 26.0).abs() < 0.001);
        assert!((shadow.bottom_left - 14.0).abs() < 0.001);
    }

    #[test]
    fn content_box_subtracts_border_and_padding() {
        let mut root = Element::new(10.0, 20.0, 200.0, 120.0);
        let mut style = Style::new();
        style.set_border(Border::uniform(Length::px(5.0), &Color::hex("#000000")));
        root.apply_style(style);
        root.set_padding_left(8.0);
        root.set_padding_top(12.0);
        root.set_padding_right(16.0);
        root.set_padding_bottom(10.0);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));
        let child = Element::new(0.0, 0.0, 300.0, 300.0);
        let _child_key = commit_child(&mut arena, root_key, Box::new(child));

        measure_and_place(
            &mut arena,
            root_key,
            crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            crate::view::base_component::LayoutPlacement {
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
            },
        );
        let snapshot = nth_child_snapshot(&arena, root_key, 0);

        assert_eq!(snapshot.x, 23.0);
        assert_eq!(snapshot.y, 37.0);
        assert_eq!(snapshot.width, 300.0);
        assert_eq!(snapshot.height, 300.0);
    }

    #[test]
    fn element_layout_preserves_fractional_box_metrics() {
        let mut root = Element::new(1.2, 2.4, 100.5, 50.5);
        let mut style = Style::new();
        style.set_padding(crate::Padding::new().xy(Length::px(3.25), Length::px(2.5)));
        root.apply_style(style);

        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(root));

        measure_and_place(
            &mut arena,
            root_key,
            crate::view::base_component::LayoutConstraints {
                max_width: 200.0,
                max_height: 200.0,
                viewport_width: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
                viewport_height: 200.0,
            },
            crate::view::base_component::LayoutPlacement {
                parent_x: 4.1,
                parent_y: 5.3,
                visual_offset_x: 0.2,
                visual_offset_y: -0.1,
                available_width: 200.0,
                available_height: 200.0,
                viewport_width: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
                viewport_height: 200.0,
            },
        );

        let root_el = crate::view::test_support::get_element::<Element>(&arena, root_key);
        let snapshot = root_el.box_model_snapshot();
        assert!((snapshot.x - 5.5).abs() < 0.01);
        assert!((snapshot.y - 7.6).abs() < 0.01);
        assert!((snapshot.width - 100.5).abs() < 0.01);
        assert!((snapshot.height - 50.5).abs() < 0.01);
        assert!((root_el.layout_state.layout_inner_position.x - 8.75).abs() < 0.01);
        assert!((root_el.layout_state.layout_inner_position.y - 10.1).abs() < 0.01);
        assert!((root_el.layout_state.layout_inner_size.width - 94.0).abs() < 0.01);
        assert!((root_el.layout_state.layout_inner_size.height - 45.5).abs() < 0.01);
    }

    #[test]
    fn percent_child_size_works_with_definite_containing_size() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 123.0, 77.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child_style.insert(
            PropertyId::Height,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            crate::view::base_component::LayoutPlacement {
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
            },
        );
        let snapshot_unknown = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot_unknown.width, 400.0);
        assert_eq!(snapshot_unknown.height, 300.0);

        let mut known_parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut known_parent_style = Style::new();
        known_parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        known_parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        known_parent.apply_style(known_parent_style);

        let mut child2 = Element::new(0.0, 0.0, 123.0, 77.0);
        let mut child2_style = Style::new();
        child2_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child2_style.insert(
            PropertyId::Height,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child2.apply_style(child2_style);

        let known_parent_key = commit_element(&mut arena, Box::new(known_parent));
        let _child2_key = commit_child(&mut arena, known_parent_key, Box::new(child2));

        measure_and_place(
            &mut arena,
            known_parent_key,
            crate::view::base_component::LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            crate::view::base_component::LayoutPlacement {
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
            },
        );
        let snapshot_known = nth_child_snapshot(&arena, known_parent_key, 0);
        assert_eq!(snapshot_known.width, 120.0);
        assert_eq!(snapshot_known.height, 60.0);
    }

    #[test]
    fn calc_percent_and_px_resolves_against_parent_content_size() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 50.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::calc(
                Length::percent(100.0),
                Operator::subtract,
                Length::px(20.0),
            )),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.width, 220.0);
    }

    #[test]
    fn calc_with_percent_resolves_when_containing_size_is_definite() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 77.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::calc(
                Length::percent(100.0),
                Operator::subtract,
                Length::px(20.0),
            )),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.width, 780.0);
    }

    #[test]
    fn calc_with_percent_falls_back_to_auto_when_containing_size_is_indefinite() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 77.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::calc(
                Length::percent(100.0),
                Operator::subtract,
                Length::px(20.0),
            )),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.width, 77.0);
    }

    #[test]
    fn calc_nested_with_multiply_and_add_is_supported() {
        let mut el = Element::new(0.0, 0.0, 10.0, 10.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::calc(
                Length::percent(100.0),
                Operator::plus,
                Length::calc(Length::px(10.0), Operator::multiply, 5),
            )),
        );
        el.apply_style(style);

        let mut arena = new_test_arena();
        let key = commit_element(&mut arena, Box::new(el));
        measure_and_place(
            &mut arena,
            key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        assert_eq!(child_snapshot(&arena, key).width, 850.0);
    }

    #[test]
    fn vh_child_size_resolves_against_viewport_height() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 10.0, 10.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Width, ParsedValue::Length(Length::vh(50.0)));
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::vh(50.0)));
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );
        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.width, 300.0);
        assert_eq!(snapshot.height, 300.0);
    }

    #[test]
    fn inner_clip_rect_uses_flex_assigned_width() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 40.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 0.0, 18.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        child_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().grow(1.0).shrink(1.0)),
        );
        child_style.set_border_radius(BorderRadius::uniform(Length::px(4.0)));
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            crate::view::base_component::LayoutConstraints {
                max_width: 240.0,
                max_height: 40.0,
                viewport_width: 240.0,
                viewport_height: 40.0,
                percent_base_width: Some(240.0),
                percent_base_height: Some(40.0),
            },
            crate::view::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 240.0,
                available_height: 40.0,
                viewport_width: 240.0,
                viewport_height: 40.0,
                percent_base_width: Some(240.0),
                percent_base_height: Some(40.0),
            },
        );

        let child_el = crate::view::test_support::get_element::<Element>(&arena, child_k);
        let snapshot = child_el.box_model_snapshot();
        let inner = child_el.inner_clip_rect();

        assert!((snapshot.width - 240.0).abs() < 0.01);
        assert!((inner.width - 240.0).abs() < 0.01);
    }

    #[test]
    fn vw_child_size_resolves_against_viewport_width() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 10.0, 10.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Width, ParsedValue::Length(Length::vw(50.0)));
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::vw(50.0)));
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );
        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.width, 400.0);
        assert_eq!(snapshot.height, 400.0);
    }

    #[test]
    fn vh_falls_back_to_zero_when_viewport_is_unknown() {
        assert_eq!(
            resolve_px_with_base(Length::vh(50.0), None, 0.0, 0.0),
            Some(0.0)
        );
        assert_eq!(
            resolve_signed_px_with_base(Length::vh(-20.0), None, 0.0, 0.0),
            Some(0.0)
        );
        assert_eq!(
            resolve_px_with_base(Length::vw(50.0), None, 0.0, 0.0),
            Some(0.0)
        );
        assert_eq!(
            resolve_signed_px_with_base(Length::vw(-20.0), None, 0.0, 0.0),
            Some(0.0)
        );
    }

    #[test]
    fn absolute_child_does_not_affect_auto_parent_size() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let normal_child = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut absolute_child = Element::new(0.0, 0.0, 300.0, 200.0);
        let mut absolute_style = Style::new();
        absolute_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute()),
        );
        absolute_child.apply_style(absolute_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(normal_child));
        let _ = commit_child(&mut arena, parent_key, Box::new(absolute_child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = child_snapshot(&arena, parent_key);
        assert_eq!(snapshot.width, 80.0);
        assert_eq!(snapshot.height, 40.0);
    }

    #[test]
    fn column_flow_auto_size_uses_cross_for_width_and_main_for_height() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().column().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(Element::new(0.0, 0.0, 80.0, 30.0)));
        let _ = commit_child(&mut arena, parent_key, Box::new(Element::new(0.0, 0.0, 120.0, 10.0)));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = child_snapshot(&arena, parent_key);
        assert_eq!(snapshot.width, 120.0);
        assert_eq!(snapshot.height, 40.0);
    }

    #[test]
    fn flow_align_centers_children_on_cross_axis() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().align(Align::Center).into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(Element::new(0.0, 0.0, 80.0, 40.0)));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.x, 0.0);
        assert_eq!(snapshot.y, 40.0);
    }

    #[test]
    fn flow_cross_size_stretch_skips_children_with_explicit_cross_size() {
        let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(
                Layout::flow()
                    .row()
                    .no_wrap()
                    .cross_size(CrossSize::Stretch)
                    .into(),
            ),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut explicit_child = Element::new(0.0, 0.0, 80.0, 10.0);
        let mut explicit_child_style = Style::new();
        explicit_child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        explicit_child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(10.0)));
        explicit_child.apply_style(explicit_child_style);

        let mut auto_child = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut auto_child_style = Style::new();
        auto_child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        auto_child.apply_style(auto_child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(explicit_child));
        let _ = commit_child(&mut arena, parent_key, Box::new(auto_child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let explicit_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let auto_snapshot = nth_child_snapshot(&arena, parent_key, 1);

        assert_eq!(explicit_snapshot.height, 10.0);
        assert_eq!(auto_snapshot.height, 40.0);
    }

    #[test]
    fn flex_row_grow_distributes_remaining_space_to_children() {
        let mut parent = Element::new(0.0, 0.0, 300.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().align(Align::Center).into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(300.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);

        let mut first = Element::new(0.0, 0.0, 40.0, 20.0);
        let mut first_style = Style::new();
        first_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().basis(Length::px(40.0)).grow(1.0)),
        );
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 40.0, 30.0);
        let mut second_style = Style::new();
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().basis(Length::px(40.0)).grow(2.0)),
        );
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(first));
        let _ = commit_child(&mut arena, parent_key, Box::new(second));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert_eq!(first_snapshot.width, 113.333336);
        assert_eq!(second_snapshot.width, 186.66667);
        assert_eq!(first_snapshot.y, 50.0);
        assert_eq!(second_snapshot.y, 45.0);
    }

    #[test]
    fn flex_row_shrink_uses_basis_when_content_overflows() {
        let mut parent = Element::new(0.0, 0.0, 150.0, 80.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(150.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(80.0)));
        parent.apply_style(parent_style);

        let mut first = Element::new(0.0, 0.0, 80.0, 20.0);
        let mut first_style = Style::new();
        first_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().basis(Length::px(100.0)).shrink(1.0)),
        );
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 80.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().basis(Length::px(100.0)).shrink(1.0)),
        );
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(first));
        let _ = commit_child(&mut arena, parent_key, Box::new(second));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);

        assert!((first_snapshot.width - 75.0).abs() < 0.01);
        assert!((second_snapshot.width - 75.0).abs() < 0.01);
        assert!((second_snapshot.x - 75.0).abs() < 0.01);
    }

    #[test]
    fn absolute_defaults_to_parent_anchor_and_zero_insets() {
        let parent = Element::new(40.0, 60.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute()),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.x, 40.0);
        assert_eq!(snapshot.y, 60.0);
    }

    #[test]
    fn absolute_stretch_with_left_right_top_bottom() {
        let parent = Element::new(10.0, 20.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.0))
                    .right(Length::px(20.0))
                    .top(Length::px(5.0))
                    .bottom(Length::px(15.0)),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.x, 20.0);
        assert_eq!(snapshot.y, 25.0);
        assert_eq!(snapshot.width, 170.0);
        assert_eq!(snapshot.height, 100.0);
    }

    #[test]
    fn absolute_negative_insets_are_preserved() {
        let parent = Element::new(10.0, 20.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(-10.0))
                    .right(Length::px(20.0))
                    .top(Length::px(-5.0))
                    .bottom(Length::px(15.0)),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.x, 0.0);
        assert_eq!(snapshot.y, 15.0);
        assert_eq!(snapshot.width, 190.0);
        assert_eq!(snapshot.height, 110.0);
    }

    #[test]
    fn absolute_self_origin_center_centers_on_inset_point() {
        let parent = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut child = Element::new(0.0, 0.0, 40.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(100.0))
                    .top(Length::px(100.0))
                    .origin(Origin::center()),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.x, 80.0);
        assert_eq!(snapshot.y, 80.0);
        assert_eq!(snapshot.width, 40.0);
        assert_eq!(snapshot.height, 40.0);
    }

    #[test]
    fn absolute_self_origin_bottom_right_aligns_to_anchor_corner() {
        let parent = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut child = Element::new(0.0, 0.0, 50.0, 30.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(0.0))
                    .top(Length::px(0.0))
                    .origin(Origin::bottom_right()),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.x, -50.0);
        assert_eq!(snapshot.y, -30.0);
    }

    #[test]
    fn absolute_self_origin_px_offset() {
        let parent = Element::new(0.0, 0.0, 400.0, 300.0);
        let mut child = Element::new(0.0, 0.0, 60.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(100.0))
                    .top(Length::px(100.0))
                    .origin(Origin::px(20.0, 30.0)),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.x, 80.0);
        assert_eq!(snapshot.y, 70.0);
    }

    #[test]
    fn absolute_self_origin_top_center_for_popover_pattern() {
        // Popover anchored to parent bottom-center: top: 100%, left: 50%,
        // origin: top_center → self top edge centered at parent's bottom-center.
        let parent = Element::new(0.0, 0.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 80.0, 50.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::percent(50.0))
                    .top(Length::percent(100.0))
                    .origin(Origin::top_center()),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        // placement point = (50%, 100%) of parent = (100, 120)
        // self top-left = placement - (50%, 0%) of self = (100-40, 120-0) = (60, 120)
        assert_eq!(snapshot.x, 60.0);
        assert_eq!(snapshot.y, 120.0);
    }

    #[test]
    fn absolute_self_origin_with_auto_size_via_child() {
        // Mirror tooltip pattern: absolute element with Auto width/height,
        // size determined by a fixed-size child after measure pass. Origin
        // shift must use the post-measure auto-size, not 0.
        let parent = Element::new(0.0, 0.0, 200.0, 60.0);
        let mut tooltip_box = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut tooltip_style = Style::new();
        tooltip_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
        );
        tooltip_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::calc(Length::percent(100.0), Operator::plus, Length::px(6.0)))
                    .top(Length::percent(50.0))
                    .origin(Origin::center_left()),
            ),
        );
        tooltip_box.apply_style(tooltip_style);

        // Fixed-size grand-child standing in for the tooltip's text.
        let text_child = Element::new(0.0, 0.0, 80.0, 20.0);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let tooltip_key = commit_child(&mut arena, parent_key, Box::new(tooltip_box));
        let _text_key = commit_child(&mut arena, tooltip_key, Box::new(text_child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        // parent 200x60. tooltip auto-sized to ~80x20 from text child.
        // left = 100% + 6 → tooltip.x = 200 + 6 = 206
        // top = 50% → 30; minus origin y (50% of 20 = 10) → tooltip.y = 20
        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.x, 206.0);
        assert_eq!(snapshot.width, 80.0);
        assert_eq!(snapshot.height, 20.0);
        assert_eq!(snapshot.y, 20.0);
    }

    #[test]
    fn absolute_self_origin_left_placement_with_right_inset() {
        // Tooltip Left placement: right inset + origin center_left.
        // Right inset already shifts by self_w; origin x=0 leaves x alone,
        // origin y=50% centers vertically.
        let parent = Element::new(100.0, 100.0, 60.0, 30.0);
        let mut tooltip_box = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut tooltip_style = Style::new();
        tooltip_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
        );
        tooltip_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .right(Length::calc(
                        Length::percent(100.0),
                        Operator::plus,
                        Length::px(6.0),
                    ))
                    .top(Length::percent(50.0))
                    .origin(Origin::center_left()),
            ),
        );
        tooltip_box.apply_style(tooltip_style);
        let text_child = Element::new(0.0, 0.0, 80.0, 20.0);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let tooltip_key = commit_child(&mut arena, parent_key, Box::new(tooltip_box));
        let _text_key = commit_child(&mut arena, tooltip_key, Box::new(text_child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.width, 80.0);
        assert_eq!(snapshot.height, 20.0);
        // anchor (parent) = 100x100..160x130.
        // right inset = 60+6 = 66 → target_rel_x = (100-0) + (60 - 66 - 80) = 100 - 86 = 14
        // tooltip right edge = 14 + 80 = 94 → anchor.left (100) - tooltip.right (94) = 6 = gap ✓
        assert_eq!(snapshot.x, 14.0);
        // top = 50% → target_rel_y = 100 + 15 = 115. origin oy = 10 → 105.
        // tooltip vertical center = 105 + 10 = 115 = anchor.y (100) + 0.5*30 = 115 ✓
        assert_eq!(snapshot.y, 105.0);
    }

    #[test]
    fn relative_mode_ignores_self_origin() {
        let parent = Element::new(0.0, 0.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 40.0, 30.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::relative().origin(Origin::center())),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        // Relative element follows flow layout; origin must not shift it.
        assert_eq!(snapshot.x, 0.0);
        assert_eq!(snapshot.y, 0.0);
    }

    #[test]
    fn absolute_collision_fit_viewport_clamps_into_view() {
        let mut el = Element::new(0.0, 0.0, 50.0, 30.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(390.0))
                    .top(Length::px(295.0))
                    .collision(Collision::Fit, CollisionBoundary::Viewport),
            ),
        );
        el.apply_style(style);

        let mut arena = new_test_arena();
        let key = commit_element(&mut arena, Box::new(el));
        measure_and_place(
            &mut arena,
            key,
            LayoutConstraints {
                max_width: 400.0,
                max_height: 300.0,
                viewport_width: 400.0,
                percent_base_width: Some(400.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 400.0,
                available_height: 300.0,
                viewport_width: 400.0,
                percent_base_width: Some(400.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
        );
        let snapshot = child_snapshot(&arena, key);
        assert_eq!(snapshot.x, 350.0);
        assert_eq!(snapshot.y, 270.0);
    }

    #[test]
    fn absolute_clip_viewport_allows_render_outside_parent_bounds() {
        let parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 400.0,
                max_height: 300.0,
                viewport_width: 400.0,
                percent_base_width: Some(400.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 400.0,
                available_height: 300.0,
                viewport_width: 400.0,
                percent_base_width: Some(400.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
        );

        let rendered = crate::view::test_support::get_element::<Element>(&arena, child_k)
            .layout_state
            .should_render;
        assert!(rendered);
    }

    #[test]
    fn viewport_clipped_absolute_descendant_is_deferred_even_if_parent_is_not_rendered() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);
        parent.layout_state.should_render = false;

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        // Mirror `Viewport::render_rsx`: seed the ctx defer list once
        // from the arena.
        let mut popup_stack = crate::view::popup_stack::PopupStack::new();
        arena.seed_defer_render_with_stack(&mut popup_stack, &mut ctx);
        let ctx_for_build =
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("parent build returns state");
        ctx.set_state(next_state);

        let deferred = ctx.take_deferred_node_ids();
        let child_id = arena.get(child_k).unwrap().element.stable_id();
        assert!(deferred.contains(&child_id));
    }

    #[test]
    fn absolute_clip_anchor_parent_falls_back_to_grandparent_without_anchor() {
        let parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 400.0,
                max_height: 300.0,
                viewport_width: 400.0,
                percent_base_width: Some(400.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 400.0,
                available_height: 300.0,
                viewport_width: 400.0,
                percent_base_width: Some(400.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
        );

        // AnchorParent without explicit anchor uses grandparent (= proposal/viewport
        // 400x300) as the clip rect. Child at x=130, y=10, size 30x20 fits inside
        // the grandparent clip even though it overflows the immediate parent (100x80).
        let rendered = crate::view::test_support::get_element::<Element>(&arena, child_k)
            .layout_state
            .should_render;
        assert!(rendered);
    }

    #[test]
    fn absolute_clip_anchor_parent_uses_anchor_parent_bounds() {
        let parent = Element::new(0.0, 0.0, 500.0, 200.0);
        let mut anchor = Element::new(300.0, 20.0, 40.0, 40.0);
        anchor.set_anchor_name(Some(AnchorName::new("menu_button")));

        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor("menu_button")
                    .left(Length::px(50.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _anchor_k = commit_child(&mut arena, parent_key, Box::new(anchor));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 600.0,
                max_height: 300.0,
                viewport_width: 600.0,
                percent_base_width: Some(600.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 600.0,
                available_height: 300.0,
                viewport_width: 600.0,
                percent_base_width: Some(600.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
        );

        let rendered = crate::view::test_support::get_element::<Element>(&arena, child_k)
            .layout_state
            .should_render;
        assert!(rendered);
    }

    #[test]
    fn absolute_clip_anchor_parent_scissor_uses_anchor_parent_bounds() {
        let parent = Element::new(0.0, 0.0, 500.0, 200.0);
        let mut anchor = Element::new(300.0, 20.0, 40.0, 40.0);
        anchor.set_anchor_name(Some(AnchorName::new("menu_button")));

        let mut child = Element::new(0.0, 0.0, 150.0, 22.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor("menu_button")
                    .left(Length::px(38.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _anchor_k = commit_child(&mut arena, parent_key, Box::new(anchor));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 600.0,
                max_height: 300.0,
                viewport_width: 600.0,
                percent_base_width: Some(600.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 600.0,
                available_height: 300.0,
                viewport_width: 600.0,
                percent_base_width: Some(600.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
        );

        let child_el = crate::view::test_support::get_element::<Element>(&arena, child_k);
        assert_eq!(child_el.absolute_clip_scissor_rect(), Some([0, 0, 500, 200]));
    }

    #[test]
    fn absolute_clip_anchor_parent_scissor_falls_back_to_grandparent_without_anchor() {
        let parent = Element::new(0.0, 0.0, 100.0, 80.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(130.0))
                    .top(Length::px(10.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 400.0,
                max_height: 300.0,
                viewport_width: 400.0,
                percent_base_width: Some(400.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 400.0,
                available_height: 300.0,
                viewport_width: 400.0,
                percent_base_width: Some(400.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
        );

        // AnchorParent without anchor → grandparent's clip. Root parent's
        // grandparent clip falls back to the proposal viewport (400x300).
        let child_el = crate::view::test_support::get_element::<Element>(&arena, child_k);
        assert_eq!(child_el.absolute_clip_scissor_rect(), Some([0, 0, 400, 300]));
    }

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
        assert!(layout_reqs.iter().any(|req| req.field == LayoutField::Width));
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
        let layout_snapshots = crate::view::base_component::collect_layout_transition_snapshots(
            &arena_old,
            &[old_key],
        );

        let mut rebuilt = Element::new_with_id(42, 50.0, 0.0, 100.0, 40.0);
        rebuilt.has_layout_snapshot = true;
        rebuilt.layout_transition_visual_offset_x = -20.0;
        rebuilt.layout_transition_target_x = Some(70.0);
        let mut arena = new_test_arena();
        let rebuilt_key = commit_element(&mut arena, Box::new(rebuilt));
        crate::view::base_component::seed_layout_transition_snapshots(
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

    #[test]
    fn axis_layout_measure_uses_target_size_not_transition_override_for_distribution() {
        let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
        parent.apply_style(parent_style);
        parent.layout_transition_override_width = Some(320.0);
        parent.layout_transition_override_height = Some(180.0);

        let mut first = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut first_style = Style::new();
        first_style.set_flex(crate::flex().grow(1.0).basis(Length::px(50.0)));
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut second_style = Style::new();
        second_style.set_flex(crate::flex().grow(1.0).basis(Length::px(50.0)));
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _first_k = commit_child(&mut arena, parent_key, Box::new(first));
        let _second_k = commit_child(&mut arena, parent_key, Box::new(second));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert_eq!(first_snapshot.width, 100.0);
        assert_eq!(second_snapshot.width, 100.0);
    }

    #[test]
    fn flow_measure_uses_target_size_not_transition_override_for_percent_children() {
        let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
        parent.apply_style(parent_style);
        parent.layout_transition_override_width = Some(320.0);
        parent.layout_transition_override_height = Some(180.0);

        let mut child = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child_style.insert(
            PropertyId::Height,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _child_k = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snapshot.width, 100.0);
        assert_eq!(snapshot.height, 50.0);
    }

    #[test]
    fn flex_measure_does_not_feed_distributed_main_size_back_into_auto_basis() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
        parent.apply_style(parent_style);

        let mut first = Element::new(0.0, 0.0, 10.0, 20.0);
        let mut first_style = Style::new();
        first_style.insert(PropertyId::Width, ParsedValue::Auto);
        first_style.insert(PropertyId::Height, ParsedValue::Auto);
        first_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().shrink(1.0)));
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 120.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().shrink(1.0)));
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let first_key = commit_child(&mut arena, parent_key, Box::new(first));
        let _first_leaf = commit_child(&mut arena, first_key, Box::new(Element::new(0.0, 0.0, 20.0, 20.0)));
        let _second_key = commit_child(&mut arena, parent_key, Box::new(second));

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

        measure_and_place(&mut arena, parent_key, constraints, placement);
        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert!((first_snapshot.width - 20.0).abs() < 0.01);
        assert!((second_snapshot.width - 80.0).abs() < 0.01);

        crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
            .mark_layout_dirty();
        measure_and_place(&mut arena, parent_key, constraints, placement);
        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert!((first_snapshot.width - 20.0).abs() < 0.01);
        assert!((second_snapshot.width - 80.0).abs() < 0.01);
    }

    #[test]
    fn flex_grow_redistributes_remaining_space_after_max_width_clamp() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        parent.apply_style(parent_style);

        let mut first = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut first_style = Style::new();
        first_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().grow(1.0)));
        first_style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(30.0)));
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().grow(1.0)));
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(first));
        let _ = commit_child(&mut arena, parent_key, Box::new(second));
        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert!((first_snapshot.width - 30.0).abs() < 0.01);
        assert!((second_snapshot.width - 70.0).abs() < 0.01);
    }

    #[test]
    fn flex_shrink_redistributes_remaining_space_after_min_width_clamp() {
        let mut parent = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        parent.apply_style(parent_style);

        let mut first = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut first_style = Style::new();
        first_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().shrink(1.0)));
        first_style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(50.0)));
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().shrink(1.0)));
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(first));
        let _ = commit_child(&mut arena, parent_key, Box::new(second));
        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert!((first_snapshot.width - 50.0).abs() < 0.01);
        assert!((second_snapshot.width - 30.0).abs() < 0.01);
    }

    #[test]
    fn flex_auto_min_main_size_uses_measured_size_for_auto_main_axis_items() {
        let mut parent = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        parent.apply_style(parent_style);

        let mut first = Element::new(0.0, 0.0, 10.0, 20.0);
        let mut first_style = Style::new();
        first_style.insert(PropertyId::Width, ParsedValue::Auto);
        first_style.insert(PropertyId::Height, ParsedValue::Auto);
        first_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().shrink(1.0)));
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(60.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().shrink(1.0)));
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let first_key = commit_child(&mut arena, parent_key, Box::new(first));
        let _ = commit_child(&mut arena, first_key, Box::new(Element::new(0.0, 0.0, 60.0, 20.0)));
        let _ = commit_child(&mut arena, parent_key, Box::new(second));
        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert!((first_snapshot.width - 60.0).abs() < 0.01);
        assert!((second_snapshot.width - 20.0).abs() < 0.01);
    }

    #[test]
    fn flex_basis_auto_uses_zero_when_child_main_size_is_indefinite() {
        let mut parent = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        parent.apply_style(parent_style);

        let mut first = Element::new(0.0, 0.0, 10.0, 20.0);
        let mut first_style = Style::new();
        first_style.insert(PropertyId::Width, ParsedValue::Auto);
        first_style.insert(PropertyId::Height, ParsedValue::Auto);
        first_style.insert(
            PropertyId::MinWidth,
            ParsedValue::Length(Length::Zero),
        );
        first_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().shrink(1.0)),
        );
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(60.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().shrink(1.0)),
        );
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let first_key = commit_child(&mut arena, parent_key, Box::new(first));
        let _ = commit_child(&mut arena, first_key, Box::new(Element::new(0.0, 0.0, 60.0, 20.0)));
        let _ = commit_child(&mut arena, parent_key, Box::new(second));
        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );

        let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert!((first_snapshot.width - 0.0).abs() < 0.01);
        assert!((second_snapshot.width - 60.0).abs() < 0.01);
    }

    #[test]
    fn width_transition_on_flow_child_repositions_following_sibling() {
        let mut parent = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        parent.apply_style(parent_style);

        let mut spacer = Element::new_with_id(1, 0.0, 0.0, 0.0, 20.0);
        let mut spacer_style = Style::new();
        spacer_style.insert(PropertyId::Width, ParsedValue::Length(Length::Zero));
        spacer_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        spacer_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(
                Transitions::single(Transition::new(TransitionProperty::Width, 180)),
            ),
        );
        spacer.apply_style(spacer_style);

        let mut thumb = Element::new_with_id(2, 0.0, 0.0, 20.0, 20.0);
        let mut thumb_style = Style::new();
        thumb_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        thumb_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        thumb.apply_style(thumb_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let spacer_key = commit_child(&mut arena, parent_key, Box::new(spacer));
        let _ = commit_child(&mut arena, parent_key, Box::new(thumb));

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

        measure_and_place(&mut arena, parent_key, constraints, placement);
        let _ = crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
            .take_layout_transition_requests();

        let mut next_spacer_style = Style::new();
        next_spacer_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        crate::view::test_support::get_element_mut::<Element>(&arena, spacer_key)
            .apply_style(next_spacer_style);

        crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
            .mark_layout_dirty();
        measure_and_place(&mut arena, parent_key, constraints, placement);

        let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, spacer_key)
            .take_layout_transition_requests();
        assert!(reqs.iter().any(|req| req.field == LayoutField::Width));

        crate::view::test_support::get_element_mut::<Element>(&arena, spacer_key)
            .set_layout_transition_width(10.0);
        crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
            .mark_layout_dirty();
        measure_and_place(&mut arena, parent_key, constraints, placement);

        let thumb_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert!((thumb_snapshot.x - 10.0).abs() < 0.01);
    }


    #[test]
    fn min_and_max_size_clamp_explicit_width_and_height() {
        let mut el = Element::new(0.0, 0.0, 320.0, 20.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(320.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(120.0)));
        style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(180.0)));
        style.insert(PropertyId::MinHeight, ParsedValue::Length(Length::px(40.0)));
        style.insert(PropertyId::MaxHeight, ParsedValue::Length(Length::px(60.0)));
        el.apply_style(style);

        let mut arena = new_test_arena();
        let key = commit_element(&mut arena, Box::new(el));
        measure_and_place(
            &mut arena,
            key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let snapshot = child_snapshot(&arena, key);
        assert_eq!(snapshot.width, 180.0);
        assert_eq!(snapshot.height, 40.0);
    }

    #[test]
    fn percent_min_and_max_size_resolve_against_parent_inner_size() {
        let parent = Element::new(0.0, 0.0, 300.0, 200.0);
        let mut child = Element::new(0.0, 0.0, 500.0, 10.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(500.0)));
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(10.0)));
        child_style.insert(
            PropertyId::MinWidth,
            ParsedValue::Length(Length::percent(50.0)),
        );
        child_style.insert(
            PropertyId::MaxWidth,
            ParsedValue::Length(Length::percent(60.0)),
        );
        child_style.insert(
            PropertyId::MinHeight,
            ParsedValue::Length(Length::percent(40.0)),
        );
        child_style.insert(
            PropertyId::MaxHeight,
            ParsedValue::Length(Length::percent(45.0)),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let snap = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snap.width, 180.0);
        assert_eq!(snap.height, 80.0);
    }

    #[test]
    fn percent_min_and_max_size_apply_when_parent_auto_has_resolved_percent_base() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut child = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        child_style.insert(
            PropertyId::MinWidth,
            ParsedValue::Length(Length::percent(60.0)),
        );
        child_style.insert(
            PropertyId::MinHeight,
            ParsedValue::Length(Length::percent(70.0)),
        );
        child_style.insert(
            PropertyId::MaxWidth,
            ParsedValue::Length(Length::percent(10.0)),
        );
        child_style.insert(
            PropertyId::MaxHeight,
            ParsedValue::Length(Length::percent(10.0)),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let snap = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(snap.width, 480.0);
        assert_eq!(snap.height, 420.0);
    }

    #[test]
    fn min_greater_than_max_uses_min_as_effective_max() {
        let mut el = Element::new(0.0, 0.0, 10.0, 10.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(30.0)));
        style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(120.0)));
        style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(90.0)));
        style.insert(PropertyId::MinHeight, ParsedValue::Length(Length::px(50.0)));
        style.insert(PropertyId::MaxHeight, ParsedValue::Length(Length::px(40.0)));
        el.apply_style(style);

        let mut arena = new_test_arena();
        let key = commit_element(&mut arena, Box::new(el));
        measure_and_place(
            &mut arena,
            key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let snapshot = child_snapshot(&arena, key);
        assert_eq!(snapshot.width, 120.0);
        assert_eq!(snapshot.height, 50.0);
    }

    #[test]
    fn apply_style_syncs_box_shadow_into_element_state() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.set_box_shadow(vec![
            BoxShadow::new()
                .color(Color::hex("#223344"))
                .offset_x(3.0)
                .offset_y(5.0)
                .blur(8.0)
                .spread(2.0),
            BoxShadow::new().offset(-1.0),
        ]);
        el.apply_style(style);

        assert_eq!(el.computed_style.box_shadow.len(), 2);
        assert_eq!(el.box_shadows.len(), 2);
        assert_eq!(el.box_shadows[0].offset_x, 3.0);
        assert_eq!(el.box_shadows[0].offset_y, 5.0);
        assert_eq!(el.box_shadows[0].blur, 8.0);
        assert_eq!(el.box_shadows[0].spread, 2.0);
        assert!(!el.box_shadows[0].inset);
        assert_eq!(el.box_shadows[1].offset_x, -1.0);
        assert_eq!(el.box_shadows[1].offset_y, -1.0);
        assert!(!el.box_shadows[1].inset);
    }

    #[test]
    fn child_clip_scope_is_skipped_when_children_are_fully_inside_inner_rect() {
        let parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let child = Element::new(20.0, 20.0, 40.0, 40.0);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
        );

        let child_count = arena.children_of(parent_key).len();
        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        let inner_radii = parent_ref.inner_clip_radii(normalize_corner_radii(
            parent_ref.border_radii,
            parent_ref.layout_state.layout_size.width.max(0.0),
            parent_ref.layout_state.layout_size.height.max(0.0),
        ));
        let overflow_child_indices: Vec<bool> = (0..child_count)
            .map(|idx| parent_ref.child_renders_outside_inner_clip(idx, &arena))
            .collect();
        assert!(!parent_ref.should_clip_children(&overflow_child_indices, inner_radii, &arena));
    }

    #[test]
    fn child_clip_scope_is_required_when_child_overflows_inner_rect() {
        let parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut child = Element::new(0.0, 0.0, 140.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
        child.apply_style(style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
        );

        let child_count = arena.children_of(parent_key).len();
        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        let inner_radii = parent_ref.inner_clip_radii(normalize_corner_radii(
            parent_ref.border_radii,
            parent_ref.layout_state.layout_size.width.max(0.0),
            parent_ref.layout_state.layout_size.height.max(0.0),
        ));
        let overflow_child_indices: Vec<bool> = (0..child_count)
            .map(|idx| parent_ref.child_renders_outside_inner_clip(idx, &arena))
            .collect();
        assert!(parent_ref.should_clip_children(&overflow_child_indices, inner_radii, &arena));
    }

    #[test]
    fn child_clip_scope_uses_stencil_without_rounding() {
        let parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut child = Element::new(0.0, 0.0, 140.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
        child.apply_style(style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);

        let inner_radii = {
            let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_ref.inner_clip_radii(normalize_corner_radii(
                parent_ref.border_radii,
                parent_ref.layout_state.layout_size.width.max(0.0),
                parent_ref.layout_state.layout_size.height.max(0.0),
            ))
        };
        assert!(!inner_radii.has_any_rounding());

        let mut parent_mut = crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
        let scope = parent_mut.begin_child_clip_scope(&mut graph, &mut ctx, inner_radii);
        assert!(scope.is_some());
        assert!(scope.as_ref().is_some_and(|scope| scope.child_clip_id != 0));
    }

    #[test]
    fn child_clip_scope_is_skipped_when_inner_scissor_is_outside_ancestor_scissor() {
        let parent = Element::new(100.0, 100.0, 50.0, 50.0);
        let mut child = Element::new(0.0, 0.0, 80.0, 20.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        child.apply_style(style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 100.0,
                parent_y: 100.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        ctx.push_scissor_rect(Some([0, 0, 20, 20]));

        let inner_radii = {
            let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_ref.inner_clip_radii(normalize_corner_radii(
                parent_ref.border_radii,
                parent_ref.layout_state.layout_size.width.max(0.0),
                parent_ref.layout_state.layout_size.height.max(0.0),
            ))
        };

        let mut parent_mut = crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
        let scope = parent_mut.begin_child_clip_scope(&mut graph, &mut ctx, inner_radii);

        assert!(scope.is_none());
        assert_eq!(ctx.current_clip_id(), 0);
        assert_eq!(ctx.scissor_rect(), Some([0, 0, 20, 20]));
    }

    #[test]
    fn promoted_child_is_skipped_when_required_inner_clip_is_outside_ancestor_scissor() {
        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(Element::new(0.0, 0.0, 200.0, 200.0)));

        let mut container = Element::new(100.0, 100.0, 50.0, 50.0);
        let mut container_style = Style::new();
        container_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(100.0))
                    .top(Length::px(100.0)),
            ),
        );
        container.apply_style(container_style);
        let container_key = commit_child(&mut arena, root_key, Box::new(container));

        let mut promoted_child = Element::new(0.0, 0.0, 80.0, 20.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
        promoted_child.apply_style(style);
        let promoted_child_id = promoted_child.stable_id();
        let _ = commit_child(&mut arena, container_key, Box::new(promoted_child));

        measure_and_place(
            &mut arena,
            root_key,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        ctx.push_scissor_rect(Some([0, 0, 20, 20]));
        ctx.set_promoted_runtime(
            Arc::new(FxHashSet::from_iter([promoted_child_id])),
            Arc::new(FxHashMap::default()),
            Arc::new(FxHashMap::default()),
        );
        reset_test_promoted_build_counts();

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(root_key, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("root build returns state");
        ctx.set_state(next_state);

        assert_eq!(test_promoted_build_count(promoted_child_id, "promoted-child"), 0);
        assert_eq!(test_promoted_build_count(promoted_child_id, "promoted-layer"), 0);
        assert_eq!(ctx.scissor_rect(), Some([0, 0, 20, 20]));
    }

    #[test]
    fn scrollbar_renders_with_promoted_child() {
        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#101010")),
        );
        parent_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(crate::ScrollDirection::Vertical),
        );
        parent.apply_style(parent_style);
        let _ = parent.set_hovered(true);
        parent.set_scrollbar_shadow_blur_radius(0.0);

        let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let child_key_k = commit_child(&mut arena, parent_key, Box::new(child));
        let child_id = arena.get(child_key_k).unwrap().element.stable_id();

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        ctx.set_promoted_runtime(
            Arc::new(FxHashSet::from_iter([child_id])),
            Arc::new(FxHashMap::default()),
            Arc::new(FxHashMap::default()),
        );

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("parent build returns state");
        ctx.set_state(next_state);

        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect::<Vec<_>>();

        assert!(
            pass_names
                .iter()
                .any(|name| name.contains("draw_rect_pass::DrawRectPass")),
            "expected scrollbar draw rect with promoted child, passes: {pass_names:?}"
        );
    }

    #[test]
    fn promoted_scroll_container_without_promoted_descendants_still_renders_scrollbar() {
        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(crate::ScrollDirection::Vertical),
        );
        parent.apply_style(parent_style);
        let _ = parent.set_hovered(true);
        parent.set_scrollbar_shadow_blur_radius(0.0);
        let parent_id = parent.stable_id();

        let child = Element::new(0.0, 0.0, 120.0, 360.0);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        ctx.set_promoted_runtime(
            Arc::new(FxHashSet::from_iter([parent_id])),
            Arc::new(FxHashMap::default()),
            Arc::new(FxHashMap::default()),
        );

        let promotion_bounds = crate::view::test_support::get_element::<Element>(&arena, parent_key)
            .promotion_composite_bounds();
        let mut layer_ctx = UiBuildContext::from_parts(
            ctx.viewport(),
            super::BuildState::for_layer_subtree_with_ancestor_clip(ctx.ancestor_clip_context()),
        );
        let layer_target = layer_ctx.allocate_promoted_layer_target(
            &mut graph,
            parent_id,
            promotion_bounds,
        );
        layer_ctx.set_current_target(layer_target);
        let next_state = arena
            .with_element_taken(parent_key, |el, a| {
                el.as_any_mut()
                    .downcast_mut::<Element>()
                    .unwrap()
                    .build_promoted_layer(
                        &mut graph,
                        a,
                        layer_ctx,
                        crate::view::promotion::PromotedLayerUpdateKind::Reraster,
                        false,
                        crate::view::viewport::DebugReusePathContext::Root,
                    )
            })
            .expect("build_promoted_layer returns state");
        ctx.set_state(next_state);

        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect::<Vec<_>>();

        assert!(
            pass_names
                .iter()
                .any(|name| name.contains("draw_rect_pass::DrawRectPass")),
            "expected scrollbar draw rect in promoted root base path, passes: {pass_names:?}"
        );
    }

    #[test]
    fn scroll_container_build_restores_scissor_and_clip_state() {
        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(crate::ScrollDirection::Vertical),
        );
        parent.apply_style(parent_style);
        let child = Element::new(0.0, 0.0, 120.0, 360.0);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("parent build returns state");

        assert_eq!(
            next_state.scissor_rect, None,
            "scroll container build should not leak scissor rect to sibling roots"
        );
        assert!(
            next_state.clip_id_stack.is_empty(),
            "scroll container build should not leak clip ids to sibling roots"
        );
    }

    #[test]
    fn vertical_scroll_container_does_not_expand_auto_height_flex_row_child() {
        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(
                Layout::flow()
                    .column()
                    .no_wrap()
                    .cross_size(CrossSize::Stretch)
                    .into(),
            ),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
        parent_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(crate::ScrollDirection::Vertical),
        );
        parent.apply_style(parent_style);

        let mut row_child = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut row_style = Style::new();
        row_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        row_style.insert(PropertyId::Width, ParsedValue::Length(Length::percent(100.0)));
        row_child.apply_style(row_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let row_key = commit_child(&mut arena, parent_key, Box::new(row_child));
        let _ = commit_child(&mut arena, row_key, Box::new(Element::new(0.0, 0.0, 40.0, 24.0)));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
        );

        let row_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert!((row_snapshot.height - 24.0).abs() < 0.01);
        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        assert!((parent_ref.layout_state.content_size.height - 24.0).abs() < 0.01);
    }

    #[test]
    fn flow_cross_size_stretch_aligns_using_current_then_final_cross_size() {
        for align in [Align::Center, Align::End] {
            let mut parent = Element::new(0.0, 0.0, 320.0, 140.0);
            let mut parent_style = Style::new();
            parent_style.insert(
                PropertyId::Layout,
                ParsedValue::Layout(
                    Layout::flow()
                        .row()
                        .no_wrap()
                        .align(Align::Start)
                        .cross_size(CrossSize::Fit)
                        .into(),
                ),
            );
            parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(320.0)));
            parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(140.0)));
            parent.apply_style(parent_style);

            let mut tall = Element::new(0.0, 0.0, 120.0, 100.0);
            let mut tall_style = Style::new();
            tall_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
            tall_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
            tall.apply_style(tall_style);

            let mut stretched = Element::new(0.0, 0.0, 120.0, 0.0);
            let mut stretched_style = Style::new();
            stretched_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
            stretched_style.insert(
                PropertyId::Transition,
                ParsedValue::Transition(Transition::new(TransitionProperty::Height, 180).into()),
            );
            stretched.apply_style(stretched_style);

            let mut arena = new_test_arena();
            let parent_key = commit_element(&mut arena, Box::new(parent));
            let _ = commit_child(&mut arena, parent_key, Box::new(tall));
            let stretched_key = commit_child(&mut arena, parent_key, Box::new(stretched));
            let _ = commit_child(&mut arena, stretched_key, Box::new(Element::new(0.0, 0.0, 120.0, 40.0)));

            let constraints = LayoutConstraints {
                max_width: 320.0,
                max_height: 140.0,
                viewport_width: 320.0,
                viewport_height: 140.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(140.0),
            };
            let placement = LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 320.0,
                available_height: 140.0,
                viewport_width: 320.0,
                viewport_height: 140.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(140.0),
            };
            measure_and_place(&mut arena, parent_key, constraints, placement);
            let _ = crate::view::test_support::get_element_mut::<Element>(&arena, stretched_key)
                .take_layout_transition_requests();

            let mut next_parent_style = Style::new();
            next_parent_style.insert(
                PropertyId::Layout,
                ParsedValue::Layout(
                    Layout::flow()
                        .row()
                        .no_wrap()
                        .align(align)
                        .cross_size(CrossSize::Stretch)
                        .into(),
                ),
            );
            next_parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(320.0)));
            next_parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(140.0)));
            crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
                .apply_style(next_parent_style);
            arena.with_element_taken(parent_key, |el, a| el.measure(constraints, a));
            {
                let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
                assert_eq!(parent_ref.computed_style.layout_axis_cross_size(), CrossSize::Stretch);
            }
            {
                let stretched_ref = crate::view::test_support::get_element::<Element>(&arena, stretched_key);
                assert!(stretched_ref.flex_props().allows_cross_stretch(true));
            }
            arena.with_element_taken(parent_key, |el, a| el.place(placement, a));

            let stretched_snapshot = child_snapshot(&arena, stretched_key);
            let expected_animated_y = match align {
                Align::Start => 0.0,
                Align::Center => 50.0,
                Align::End => 100.0,
            };

            assert!(
                (stretched_snapshot.y - expected_animated_y).abs() < 0.01,
                "stretched child should align using current animated height for {align:?}, got y={}, expected {}",
                stretched_snapshot.y,
                expected_animated_y
            );
            assert!(
                (stretched_snapshot.height - 40.0).abs() < 0.01,
                "stretched child should still render previous height during animation for {align:?}, got h={}",
                stretched_snapshot.height
            );

            crate::view::test_support::get_element_mut::<Element>(&arena, stretched_key)
                .set_layout_transition_height(100.0);

            arena.with_element_taken(parent_key, |el, a| el.place(placement, a));

            {
                let mut stretched_mut = crate::view::test_support::get_element_mut::<Element>(&arena, stretched_key);
                stretched_mut.layout_transition_override_height = None;
                stretched_mut.layout_transition_target_height = None;
            }

            arena.with_element_taken(parent_key, |el, a| el.place(placement, a));

            let stretched_snapshot = child_snapshot(&arena, stretched_key);
            let expected_final_y = match align {
                Align::Start => 0.0,
                Align::Center => 20.0,
                Align::End => 40.0,
            };
            assert!(
                (stretched_snapshot.y - expected_final_y).abs() < 0.01,
                "stretched child should align using final cross size after animation for {align:?}, got y={}, expected {}",
                stretched_snapshot.y,
                expected_final_y
            );
        }
    }

    #[test]
    fn texture_desc_for_logical_bounds_keeps_logical_scale_mapping() {
        let bounds = super::PromotionCompositeBounds {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
            corner_radii: [0.0; 4],
        };

        let unscaled = super::texture_desc_for_logical_bounds(
            bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8Unorm,
        );
        let scaled = super::texture_desc_for_logical_bounds(
            bounds,
            1.0,
            Some(Mat4::from_scale(Vec3::new(2.0, 2.0, 1.0))),
            wgpu::TextureFormat::Bgra8Unorm,
        );

        assert_eq!(unscaled.width(), 30);
        assert_eq!(unscaled.height(), 40);
        assert_eq!(scaled.width(), 30);
        assert_eq!(scaled.height(), 40);
    }

    #[test]
    fn build_context_render_transform_propagates_to_child_without_leaking_back() {
        let mut parent_ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let parent_transform = Mat4::from_scale(Vec3::new(2.0, 1.5, 1.0));
        parent_ctx.set_current_render_transform(Some(parent_transform));

        let parent_viewport = parent_ctx.viewport();
        let mut child_ctx =
            UiBuildContext::from_parts(parent_viewport.clone(), parent_ctx.state_clone());
        assert_eq!(child_ctx.current_render_transform(), Some(parent_transform));

        let child_transform = Mat4::from_scale(Vec3::new(3.0, 3.0, 1.0));
        child_ctx.set_current_render_transform(Some(child_transform));

        let restored_parent = UiBuildContext::from_parts(parent_viewport, child_ctx.into_state());
        assert_eq!(
            restored_parent.current_render_transform(),
            Some(parent_transform)
        );
    }

    #[test]
    fn layer_subtree_does_not_inherit_ancestor_stencil_clip_id() {
        let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        assert_eq!(
            ctx.graphics_pass_context().stencil_clip_id,
            None,
            "fresh build context should not start with an active clip"
        );

        let pushed = ctx.push_clip_id();
        assert_eq!(pushed, Some(1), "first pushed clip id should be 1");

        let ancestor_clip = ctx.ancestor_clip_context();
        let layer_state = super::BuildState::for_layer_subtree_with_ancestor_clip(ancestor_clip);
        let layer_ctx = UiBuildContext::from_parts(ctx.viewport(), layer_state);

        assert_eq!(
            layer_ctx.graphics_pass_context().stencil_clip_id,
            None,
            "offscreen promoted layer subtree should not inherit ancestor stencil clip id"
        );
    }

    #[test]
    fn transformed_layer_subtree_starts_without_ancestor_scissor_rect() {
        let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let previous = ctx.push_scissor_rect(Some([10, 10, 40, 40]));
        assert_eq!(previous, None);
        assert_eq!(ctx.graphics_pass_context().scissor_rect, Some([10, 10, 40, 40]));

        let layer_state =
            super::BuildState::for_layer_subtree_with_ancestor_clip(super::AncestorClipContext::default());
        let layer_ctx = UiBuildContext::from_parts(ctx.viewport(), layer_state);

        assert_eq!(
            layer_ctx.graphics_pass_context().scissor_rect,
            None,
            "transformed offscreen subtree should rasterize from viewport clip, not ancestor scissor"
        );
    }

    #[test]
    fn non_promoted_container_with_promoted_child_is_not_built_twice_during_compose() {
        let mut arena = new_test_arena();
        let mut root = Element::new(0.0, 0.0, 200.0, 200.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#202020")),
        );
        root.apply_style(root_style);
        let root_key = commit_element(&mut arena, Box::new(root));

        let mut container = Element::new(0.0, 0.0, 120.0, 120.0);
        let container_id = container.stable_id();
        let mut container_style = Style::new();
        container_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#101010")),
        );
        container_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(crate::ScrollDirection::Vertical),
        );
        container.apply_style(container_style);
        let container_key = commit_child(&mut arena, root_key, Box::new(container));

        let mut promoted_child = Element::new(0.0, 0.0, 120.0, 240.0);
        let mut promoted_child_style = Style::new();
        promoted_child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        promoted_child.apply_style(promoted_child_style);
        let promoted_child_id = promoted_child.stable_id();
        let _ = commit_child(&mut arena, container_key, Box::new(promoted_child));

        measure_and_place(
            &mut arena,
            root_key,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 200.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        ctx.set_promoted_runtime(
            Arc::new(FxHashSet::from_iter([promoted_child_id])),
            Arc::new(FxHashMap::default()),
            Arc::new(FxHashMap::default()),
        );
        reset_test_promoted_build_counts();

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(root_key, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("root build returns state");
        ctx.set_state(next_state);

        assert_eq!(
            test_promoted_build_count(container_id, "base"),
            1,
            "expected non-promoted container base path to run only once"
        );
    }

    #[test]
    fn zero_opacity_sets_should_paint_false_but_keeps_render() {
        let mut arena = new_test_arena();
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#112233")),
        );
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
        el.apply_style(style);
        let key = commit_element(&mut arena, Box::new(el));

        measure_and_place(
            &mut arena,
            key,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 40.0,
                viewport_width: 100.0,
                viewport_height: 40.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(40.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 40.0,
                viewport_width: 100.0,
                viewport_height: 40.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(40.0),
            },
        );

        let el = crate::view::test_support::get_element::<Element>(&arena, key);
        assert!(el.layout_state.should_render);
        assert!(!el.core.should_paint);
    }

    #[test]
    fn transformed_bounds_are_used_for_clip_culling() {
        let mut arena = new_test_arena();
        let mut el = Element::new(120.0, 0.0, 40.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(40.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        style.set_transform(Transform::new([Translate::x(Length::px(-80.0))]));
        style.set_transform_origin(TransformOrigin::center());
        el.apply_style(style);
        let key = commit_element(&mut arena, Box::new(el));

        measure_and_place(
            &mut arena,
            key,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 100.0,
                viewport_width: 200.0,
                viewport_height: 100.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(100.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 200.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
        );

        let el = crate::view::test_support::get_element::<Element>(&arena, key);
        let transformed = el.transformed_frame_bounding_rect(super::LayoutFrame {
            x: el.layout_state.layout_position.x,
            y: el.layout_state.layout_position.y,
            width: el.layout_state.layout_size.width,
            height: el.layout_state.layout_size.height,
        });
        assert!((transformed.x - 40.0).abs() < 0.01, "{transformed:?}");
        assert!((transformed.width - 40.0).abs() < 0.01, "{transformed:?}");
        assert!(
            el.layout_state.should_render,
            "translate 後的 bounding box 已進入 parent clip，不應被提前剔除"
        );
    }

    #[test]
    fn promotion_composite_bounds_follow_transformed_bounding_box() {
        let mut arena = new_test_arena();
        let mut el = Element::new(40.0, 20.0, 30.0, 20.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(30.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        style.set_transform(Transform::new([Rotate::z(Angle::deg(90.0))]));
        style.set_transform_origin(TransformOrigin::center());
        el.apply_style(style);
        let key = commit_element(&mut arena, Box::new(el));

        measure_and_place(
            &mut arena,
            key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
        );

        let el = crate::view::test_support::get_element::<Element>(&arena, key);
        let bounds = el.promotion_composite_bounds();
        assert!((bounds.x - 45.0).abs() < 0.01);
        assert!((bounds.y - 15.0).abs() < 0.01);
        assert!((bounds.width - 20.0).abs() < 0.01);
        assert!((bounds.height - 30.0).abs() < 0.01);
    }

    #[test]
    fn transparent_borderless_shadowless_element_does_not_paint_even_with_child() {
        let mut arena = new_test_arena();
        let parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut child = Element::new(0.0, 0.0, 60.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child.apply_style(child_style);
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
        );

        let parent = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        assert!(parent.layout_state.should_render);
        assert!(!parent.core.should_paint);
    }

    #[test]
    fn zero_inner_area_sets_should_paint_false() {
        let mut arena = new_test_arena();
        let mut el = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#112233")),
        );
        style.insert(PropertyId::PaddingLeft, ParsedValue::Length(Length::px(10.0)));
        style.insert(PropertyId::PaddingRight, ParsedValue::Length(Length::px(10.0)));
        style.insert(PropertyId::PaddingTop, ParsedValue::Length(Length::px(10.0)));
        style.insert(PropertyId::PaddingBottom, ParsedValue::Length(Length::px(10.0)));
        el.apply_style(style);
        let key = commit_element(&mut arena, Box::new(el));

        measure_and_place(
            &mut arena,
            key,
            LayoutConstraints {
                max_width: 20.0,
                max_height: 20.0,
                viewport_width: 20.0,
                viewport_height: 20.0,
                percent_base_width: Some(20.0),
                percent_base_height: Some(20.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 20.0,
                available_height: 20.0,
                viewport_width: 20.0,
                viewport_height: 20.0,
                percent_base_width: Some(20.0),
                percent_base_height: Some(20.0),
            },
        );

        let el = crate::view::test_support::get_element::<Element>(&arena, key);
        assert_eq!(el.layout_state.layout_inner_size.width, 0.0);
        assert_eq!(el.layout_state.layout_inner_size.height, 0.0);
        assert!(el.layout_state.should_render);
        assert!(!el.core.should_paint);
    }

    #[test]
    fn transition_override_keeps_inner_render_area_available() {
        let mut el = Element::new(0.0, 0.0, 20.0, 20.0);
        el.layout_state.layout_position = LayoutPosition { x: 0.0, y: 0.0 };
        el.layout_state.layout_size.width = 0.0;
        el.layout_state.layout_size.height = 0.0;
        el.layout_state.layout_inner_position = LayoutPosition { x: 0.0, y: 0.0 };
        el.layout_state.layout_inner_size.width = 0.0;
        el.layout_state.layout_inner_size.height = 0.0;
        el.layout_transition_override_width = Some(40.0);
        el.layout_transition_override_height = Some(30.0);

        assert!(el.has_inner_render_area());
        let transition_inner = el.transition_inner_rect();
        assert_eq!(transition_inner.width, 40.0);
        assert_eq!(transition_inner.height, 30.0);
        let inner = el.inner_clip_rect();
        assert_eq!(inner.width, 40.0);
        assert_eq!(inner.height, 30.0);
    }

    #[test]
    fn child_hit_test_clip_uses_parent_transition_inner_size() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
        parent_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(
                [
                    Transition::new(TransitionProperty::Width, 200),
                    Transition::new(TransitionProperty::Height, 200),
                ]
                .into(),
            ),
        );
        parent_style.set_border(Border::uniform(Length::px(10.0), &Color::hex("#000000")));
        parent.apply_style(parent_style);
        parent.set_padding_left(5.0);
        parent.set_padding_right(15.0);
        parent.set_padding_top(7.0);
        parent.set_padding_bottom(13.0);
        parent.layout_transition_override_width = Some(320.0);
        parent.layout_transition_override_height = Some(180.0);
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(Element::new(0.0, 0.0, 40.0, 40.0)));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let child_key = child_key(&arena, parent_key, 0);
        let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
        let clip = child.hit_test_clip_rect.expect("hit-test clip");

        assert_eq!(clip.x, 15.0);
        assert_eq!(clip.y, 17.0);
        assert_eq!(clip.width, 280.0);
        assert_eq!(clip.height, 140.0);
    }

    #[test]
    fn absolute_parent_clip_uses_parent_transition_inner_size() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
        parent_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(
                [
                    Transition::new(TransitionProperty::Width, 200),
                    Transition::new(TransitionProperty::Height, 200),
                ]
                .into(),
            ),
        );
        parent_style.set_border(Border::uniform(Length::px(10.0), &Color::hex("#000000")));
        parent.apply_style(parent_style);
        parent.set_padding_left(5.0);
        parent.set_padding_right(15.0);
        parent.set_padding_top(7.0);
        parent.set_padding_bottom(13.0);
        parent.layout_transition_override_width = Some(320.0);
        parent.layout_transition_override_height = Some(180.0);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut child = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute().clip(ClipMode::Parent)),
        );
        child.apply_style(child_style);
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let ck = child_key(&arena, parent_key, 0);
        let child = crate::view::test_support::get_element::<Element>(&arena, ck);
        let clip = child.absolute_clip_rect.expect("absolute clip");

        assert_eq!(clip.x, 15.0);
        assert_eq!(clip.y, 17.0);
        assert_eq!(clip.width, 280.0);
        assert_eq!(clip.height, 140.0);
    }

    #[test]
    fn anchor_parent_clip_uses_transitioning_parent_inner_size() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
        parent_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(
                [
                    Transition::new(TransitionProperty::Width, 200),
                    Transition::new(TransitionProperty::Height, 200),
                ]
                .into(),
            ),
        );
        parent_style.set_border(Border::uniform(Length::px(10.0), &Color::hex("#000000")));
        parent.apply_style(parent_style);
        parent.set_padding_left(5.0);
        parent.set_padding_right(15.0);
        parent.set_padding_top(7.0);
        parent.set_padding_bottom(13.0);
        parent.layout_transition_override_width = Some(320.0);
        parent.layout_transition_override_height = Some(180.0);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut anchor = Element::new(30.0, 20.0, 40.0, 20.0);
        anchor.set_anchor_name(Some(AnchorName::new("menu_button")));
        let _ = commit_child(&mut arena, parent_key, Box::new(anchor));

        let mut child = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor("menu_button")
                    .left(Length::px(10.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child.apply_style(child_style);
        let _ = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        let ck = child_key(&arena, parent_key, 1);
        let child = crate::view::test_support::get_element::<Element>(&arena, ck);
        let clip = child.absolute_clip_rect.expect("absolute clip");

        assert_eq!(clip.x, 15.0);
        assert_eq!(clip.y, 17.0);
        assert_eq!(clip.width, 280.0);
        assert_eq!(clip.height, 140.0);
    }

    #[test]
    fn anchored_absolute_child_uses_anchor_visual_position_during_transition() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 500.0, 200.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(500.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(200.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut anchor = Element::new(300.0, 20.0, 40.0, 20.0);
        let mut anchor_style = Style::new();
        anchor_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(300.0))
                    .top(Length::px(20.0)),
            ),
        );
        anchor_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(
                Transitions::single(Transition::new(TransitionProperty::Position, 200)),
            ),
        );
        anchor.apply_style(anchor_style);
        anchor.set_anchor_name(Some(AnchorName::new("menu_button")));
        let anchor_key = commit_child(&mut arena, parent_key, Box::new(anchor));

        let mut child = Element::new(0.0, 0.0, 80.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor("menu_button")
                    .left(Length::px(10.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child.apply_style(child_style);
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        let constraints = LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        };

        measure_and_place(&mut arena, parent_key, constraints, placement);
        arena.with_element_taken(parent_key, |el, _a| {
            let p = el.as_any_mut().downcast_mut::<Element>().unwrap();
            let _ = p.take_layout_transition_requests();
            let _ = p.take_visual_transition_requests();
        });

        arena.with_element_taken(anchor_key, |el, _a| {
            let anchor = el.as_any_mut().downcast_mut::<Element>().unwrap();
            let mut next_anchor_style = Style::new();
            next_anchor_style.insert(
                PropertyId::Position,
                ParsedValue::Position(
                    Position::absolute()
                        .left(Length::px(340.0))
                        .top(Length::px(20.0)),
                ),
            );
            next_anchor_style.insert(
                PropertyId::Transition,
                ParsedValue::Transition(
                    Transitions::single(Transition::new(TransitionProperty::Position, 200)),
                ),
            );
            anchor.apply_style(next_anchor_style);
            anchor.layout_transition_visual_offset_x = -40.0;
            anchor.layout_transition_target_x = Some(340.0);
        });

        arena.with_element_taken(parent_key, |el, _a| {
            el.as_any_mut().downcast_mut::<Element>().unwrap().mark_layout_dirty();
        });
        measure_and_place(&mut arena, parent_key, constraints, placement);

        let anchor = crate::view::test_support::get_element::<Element>(&arena, anchor_key);
        let child = crate::view::test_support::get_element::<Element>(&arena, child_k);
        assert!(
            (anchor.layout_state.layout_position.x - 300.0).abs() < 0.01,
            "anchor_x={}, child_x={}",
            anchor.layout_state.layout_position.x,
            child.layout_state.layout_position.x
        );
        assert!(
            (child.layout_state.layout_position.x - 310.0).abs() < 0.01,
            "anchor_x={}, child_x={}",
            anchor.layout_state.layout_position.x,
            child.layout_state.layout_position.x
        );
    }

    #[test]
    fn setting_border_radius_does_not_mark_layout_dirty() {
        let mut el = Element::new(0.0, 0.0, 20.0, 20.0);
        el.layout_dirty = false;

        el.set_border_radius(12.0);

        assert_eq!(el.border_radius, 12.0);
        assert!(!el.layout_dirty);
    }

    #[test]
    fn border_radius_style_sample_preserves_resolved_corner_ratios() {
        let mut arena = new_test_arena();
        let mut el = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut style = Style::new();
        style.set_border_radius(
            BorderRadius::uniform(Length::px(10.0))
                .top_right(Length::px(32.0))
                .bottom_left(Length::percent(90.0)),
        );
        el.apply_style(style);
        let node_id = el.stable_id();
        let key = commit_element(&mut arena, Box::new(el));

        assert!(set_style_field_by_id(
            &mut arena,
            key,
            node_id,
            crate::transition::StyleField::BorderRadius,
            crate::transition::StyleValue::Scalar(50.0),
        ));

        let el = crate::view::test_support::get_element::<Element>(&arena, key);
        assert!((el.border_radii.top_left - 3.7037036).abs() < 0.001);
        assert!((el.border_radii.top_right - 11.851851).abs() < 0.001);
        assert!((el.border_radii.bottom_right - 3.7037036).abs() < 0.001);
        assert!((el.border_radii.bottom_left - 50.0).abs() < 0.001);
        assert!((el.border_radius - 50.0).abs() < 0.001);
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
        assert!((el.transform_origin.x().resolve_without_percent_base(0.0, 0.0) - 55.0).abs() < 0.0001);
        assert!((el.transform_origin.y().resolve_without_percent_base(0.0, 0.0) - 47.5).abs() < 0.0001);
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

    #[test]
    fn inline_layout_wraps_children_into_multiple_line_boxes() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 100.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        commit_child(&mut arena, parent_key, Box::new(Element::new(0.0, 0.0, 60.0, 10.0)));
        commit_child(&mut arena, parent_key, Box::new(Element::new(0.0, 0.0, 50.0, 20.0)));
        commit_child(&mut arena, parent_key, Box::new(Element::new(0.0, 0.0, 40.0, 15.0)));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 200.0,
                viewport_width: 100.0,
                viewport_height: 200.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 200.0,
                viewport_width: 100.0,
                viewport_height: 200.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(200.0),
            },
        );

        let first = nth_child_snapshot(&arena, parent_key, 0);
        let second = nth_child_snapshot(&arena, parent_key, 1);
        let third = nth_child_snapshot(&arena, parent_key, 2);

        assert_eq!(first.x, 0.0);
        assert_eq!(first.y, 0.0);
        assert_eq!(second.x, 0.0);
        assert_eq!(second.y, 10.0);
        assert_eq!(third.x, 50.0);
        assert_eq!(third.y, 10.0);
        let parent_el = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        assert!((parent_el.box_model_snapshot().height - 30.0).abs() < 0.01);
        assert!((parent_el.layout_state.content_size.height - 30.0).abs() < 0.01);
    }

    #[test]
    fn inline_layout_keeps_trailing_text_on_same_line_after_inline_element() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        commit_child(&mut arena, parent_key, Box::new(Text::from_content("lead")));
        commit_child(&mut arena, parent_key, Box::new(Element::new(0.0, 0.0, 50.0, 20.0)));
        commit_child(&mut arena, parent_key, Box::new(Text::from_content(" trailing text continues after the badge.")));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 200.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 200.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
        );

        let badge = nth_child_snapshot(&arena, parent_key, 1);
        let trailing_key = child_key(&arena, parent_key, 2);
        let trailing = crate::view::test_support::get_element::<Text>(&arena, trailing_key);
        let trailing_snapshot = trailing.box_model_snapshot();
        let trailing_fragments = trailing.inline_fragment_positions();
        let first_fragment = trailing_fragments.first().expect("first inline fragment");

        assert_eq!(badge.y, 0.0);
        assert_eq!(trailing_snapshot.y, 0.0);
        assert!(first_fragment.1.x >= badge.x + badge.width);
        assert_eq!(first_fragment.1.y, 0.0);
    }

    #[test]
    fn inline_text_ignores_explicit_size_and_still_fragments() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut text = Text::from_content("fragmented text should wrap across multiple inline lines");
        text.set_size(300.0, 300.0);
        commit_child(&mut arena, parent_key, Box::new(text));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 240.0,
                viewport_width: 160.0,
                viewport_height: 240.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 240.0,
                viewport_width: 160.0,
                viewport_height: 240.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(240.0),
            },
        );

        let text_key = child_key(&arena, parent_key, 0);
        let text = crate::view::test_support::get_element::<Text>(&arena, text_key);
        let fragments = text.inline_fragment_positions();
        let snapshot = text.box_model_snapshot();

        assert!(fragments.len() > 1);
        assert!(snapshot.width < 300.0);
        assert!(snapshot.height < 300.0);
    }

    #[test]
    fn inline_gap_does_not_apply_between_text_fragments_of_same_text_node() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 120.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(24.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut text = Text::from_content("alpha beta gamma");
        text.set_size(300.0, 80.0);
        commit_child(&mut arena, parent_key, Box::new(text));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 120.0,
                viewport_width: 120.0,
                viewport_height: 120.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(120.0),
            },
        );

        let text_key = child_key(&arena, parent_key, 0);
        let text = crate::view::test_support::get_element::<Text>(&arena, text_key);
        let fragments = text.inline_fragment_positions();
        assert!(fragments.len() >= 2);
        assert!(fragments[1].1.x < 120.0);
    }

    #[test]
    fn inline_cjk_text_fragments_follow_wrapped_lines() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        commit_child(&mut arena, parent_key, Box::new(Text::from_content("最後接一段中文，確認混排時也能一起換行。")));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 120.0,
                viewport_width: 220.0,
                viewport_height: 120.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 120.0,
                viewport_width: 220.0,
                viewport_height: 120.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(120.0),
            },
        );

        let text_key = child_key(&arena, parent_key, 0);
        let text = crate::view::test_support::get_element::<Text>(&arena, text_key);
        let fragments = text.inline_fragment_positions();

        assert!(fragments.len() > 1);
        assert!(fragments[0].0.starts_with("最後"));
        assert!(fragments.iter().all(|(_, position)| position.x >= 0.0));
    }

    #[test]
    fn inline_auto_sized_element_expands_into_child_fragments() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("nested")));
        commit_child(&mut arena, wrapper_key, Box::new(Element::new(0.0, 0.0, 44.0, 20.0)));

        commit_child(&mut arena, parent_key, Box::new(Text::from_content("tail")));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 120.0,
                viewport_width: 220.0,
                viewport_height: 120.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 120.0,
                viewport_width: 220.0,
                viewport_height: 120.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(120.0),
            },
        );

        let wrapper_snap = child_snapshot(&arena, wrapper_key);
        let tail_key = child_key(&arena, parent_key, 1);
        let tail_snap = child_snapshot(&arena, tail_key);
        let second_wrapper_child_snap = nth_child_snapshot(&arena, wrapper_key, 1);

        assert!(wrapper_snap.width > 44.0);
        assert_eq!(second_wrapper_child_snap.y, 0.0);
        assert!(tail_snap.x >= wrapper_snap.x + 44.0);
    }

    #[test]
    fn inline_fragmentable_element_builds_multiple_draw_rect_passes() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#93c5fd")),
        );
        wrapper_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#2563eb")));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content(
            "inline wrapper background should wrap across lines",
        )));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 160.0,
                viewport_width: 160.0,
                viewport_height: 160.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(160.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 160.0,
                viewport_width: 160.0,
                viewport_height: 160.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(160.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 160, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("build result");
        ctx.set_state(next_state);

        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect::<Vec<_>>();
        let rect_like_count = pass_names
            .iter()
            .filter(|name| name.contains("draw_rect_pass::DrawRectPass") || name.contains("draw_rect_pass::OpaqueRectPass"))
            .count();
        let border_count = pass_names
            .iter()
            .filter(|name| name.contains("draw_rect_pass::DrawRectPass"))
            .count();

        assert!(rect_like_count >= 4, "expected multiple fragment rect passes, got {pass_names:?}");
        assert!(border_count >= 2, "expected multiple border rect passes, got {pass_names:?}");
    }

    #[test]
    fn inline_fragmentable_element_keeps_all_nested_text_fragments() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));
        commit_child(&mut arena, parent_key, Box::new(Text::from_content("Inline text starts here,")));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#93c5fd")),
        );
        wrapper_style.set_padding(crate::Padding::uniform(Length::px(8.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content(
            "badge test test test test test test test",
        )));
        commit_child(&mut arena, parent_key, Box::new(Text::from_content(
            "then more text continues after the badge,",
        )));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 200.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 200.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
        );

        let nested_text_key = child_key(&arena, wrapper_key, 0);
        let nested_text = crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
        let fragments = nested_text.inline_fragment_positions();
        assert!(fragments.len() >= 2, "fragments={fragments:?}");
    }

    #[test]
    fn inline_fragmentable_element_uses_slice_padding_across_fragments() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.set_padding(crate::Padding::uniform(Length::px(8.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content(
            "badge test test test test",
        )));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 160.0,
                viewport_width: 160.0,
                viewport_height: 160.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(160.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 160.0,
                viewport_width: 160.0,
                viewport_height: 160.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(160.0),
            },
        );

        let nested_text_key = child_key(&arena, wrapper_key, 0);
        let (first, last, fragments) = {
            let wrapper_el = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
            assert!(wrapper_el.inline_paint_fragments.len() >= 2);
            let first = wrapper_el.inline_paint_fragments[0];
            let last = *wrapper_el.inline_paint_fragments.last().expect("last fragment");
            let nested_text = crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
            let fragments = nested_text.inline_fragment_positions();
            (first, last, fragments)
        };
        assert!(fragments.len() >= 2, "fragments={fragments:?}");

        let first_line_y = fragments[0].1.y;
        let first_line_left = fragments
            .iter()
            .filter(|(_, position)| (position.y - first_line_y).abs() < 0.5)
            .map(|(_, position)| position.x)
            .fold(f32::INFINITY, f32::min);
        assert!((first_line_left - first.x - 8.0).abs() < 0.5);

        let last_line_y = fragments.last().expect("last fragment").1.y;
        let last_line_right = fragments
            .iter()
            .filter(|(_, position)| (position.y - last_line_y).abs() < 0.5)
            .map(|(content, position)| {
                let mut text = Text::from_content(content.as_str());
                text.measure(LayoutConstraints {
                    max_width: 200.0,
                    max_height: 80.0,
                    viewport_width: 200.0,
                    viewport_height: 80.0,
                    percent_base_width: Some(200.0),
                    percent_base_height: Some(80.0),
                }, &mut arena);
                let (width, _) = text.measured_size();
                position.x + width
            })
            .fold(0.0_f32, f32::max);
        assert!((last.x + last.width - last_line_right - 8.0).abs() < 0.5);
    }

    #[test]
    fn inline_fragmentable_wrapper_respects_remaining_width_on_first_line() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut badge = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut badge_style = Style::new();
        badge_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
        badge_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        badge.apply_style(badge_style);
        commit_child(&mut arena, parent_key, Box::new(badge));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("alpha beta gamma delta")));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 200.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 200.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
        );

        let text_key = child_key(&arena, wrapper_key, 0);
        let text = crate::view::test_support::get_element::<Text>(&arena, text_key);
        let fragments = text.inline_fragment_positions();
        let first_fragment = fragments.first().expect("first fragment");

        assert_eq!(first_fragment.1.y, 0.0, "fragments={fragments:?}");
        assert!(first_fragment.1.x >= 140.0, "fragments={fragments:?}");
    }

    #[test]
    fn inline_fragmentable_wrapper_padding_reduces_first_line_content_width() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut leading = Element::new(0.0, 0.0, 180.0, 20.0);
        let mut leading_style = Style::new();
        leading_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
        leading_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        leading.apply_style(leading_style);
        commit_child(&mut arena, parent_key, Box::new(leading));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.set_padding(crate::Padding::uniform(Length::px(8.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content(
            "Permission is hereby granted, free of charge, to any person obtaining a copy",
        )));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 200.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 200.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
        );

        let text_key = child_key(&arena, wrapper_key, 0);
        let text = crate::view::test_support::get_element::<Text>(&arena, text_key);
        let fragments = text.inline_fragment_positions();
        let first_fragment = fragments.first().expect("first fragment");

        assert_eq!(first_fragment.1.y, 0.0, "fragments={fragments:?}");
        assert!(first_fragment.1.x >= 188.0, "fragments={fragments:?}");
    }

    #[test]
    fn inline_fragmentable_element_vertical_padding_does_not_shift_inline_content_y() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 280.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(280.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.set_padding(crate::Padding::new().xy(
            Length::px(8.0),
            Length::px(12.0),
        ));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("badge")));
        commit_child(&mut arena, parent_key, Box::new(Text::from_content("trailing")));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 280.0,
                max_height: 120.0,
                viewport_width: 280.0,
                viewport_height: 120.0,
                percent_base_width: Some(280.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 280.0,
                available_height: 120.0,
                viewport_width: 280.0,
                viewport_height: 120.0,
                percent_base_width: Some(280.0),
                percent_base_height: Some(120.0),
            },
        );

        let nested_text_key = child_key(&arena, wrapper_key, 0);
        let trailing_key = child_key(&arena, parent_key, 1);

        let (badge_y, text_height) = {
            let nested_text = crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
            let by = nested_text.inline_fragment_positions()[0].1.y;
            let (_, th) = nested_text.measured_size();
            (by, th)
        };
        let trailing_y = {
            let trailing = crate::view::test_support::get_element::<Text>(&arena, trailing_key);
            trailing.inline_fragment_positions()[0].1.y
        };
        let (paint_top, paint_height) = {
            let wrapper_el = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
            (wrapper_el.inline_paint_fragments[0].y, wrapper_el.inline_paint_fragments[0].height)
        };
        let inline_node_height = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a)[0].height)
            .expect("inline node size");
        assert!((badge_y - trailing_y).abs() < 0.5);
        assert!((badge_y - paint_top - 12.0).abs() < 0.5);
        assert!((inline_node_height - text_height).abs() < 0.5);
        assert!((paint_height - (text_height + 24.0)).abs() < 0.5);
    }

    #[test]
    fn inline_fragmentable_element_positions_all_nested_text_fragments_across_widths() {
        for width in 140..=240 {
            let width = width as f32;
            let mut arena = new_test_arena();
            let mut parent = Element::new(0.0, 0.0, width, 0.0);
            let mut parent_style = Style::new();
            parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
            parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
            parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
            parent.apply_style(parent_style);
            let parent_key = commit_element(&mut arena, Box::new(parent));
            commit_child(&mut arena, parent_key, Box::new(Text::from_content("Inline text starts here,")));

            let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
            let mut wrapper_style = Style::new();
            wrapper_style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(Color::hex("#93c5fd")));
            wrapper_style.set_padding(crate::Padding::uniform(Length::px(8.0)));
            wrapper.apply_style(wrapper_style);
            let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
            commit_child(&mut arena, wrapper_key, Box::new(Text::from_content(
                "badge test test test test test test test",
            )));
            commit_child(&mut arena, parent_key, Box::new(Text::from_content(
                "then more text continues after the badge,",
            )));

            measure_and_place(
                &mut arena,
                parent_key,
                LayoutConstraints {
                    max_width: width,
                    max_height: 240.0,
                    viewport_width: width,
                    viewport_height: 240.0,
                    percent_base_width: Some(width),
                    percent_base_height: Some(240.0),
                },
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: width,
                    available_height: 240.0,
                    viewport_width: width,
                    viewport_height: 240.0,
                    percent_base_width: Some(width),
                    percent_base_height: Some(240.0),
                },
            );

            let nested_text_key = child_key(&arena, wrapper_key, 0);
            let expected = arena
                .with_element_taken(nested_text_key, |el, a| el.get_inline_nodes_size(a).len())
                .expect("inline nodes size");
            let nested_text = crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
            let actual = nested_text.inline_fragment_positions().len();
            assert_eq!(actual, expected, "width={width}, actual={actual}, expected={expected}, fragments={:?}", nested_text.inline_fragment_positions());
        }
    }

    #[test]
    fn inline_fragmentable_element_places_all_nested_inline_children_across_wrapped_lines() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 180.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));
        commit_child(&mut arena, parent_key, Box::new(Text::from_content("lead-in text")));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.set_padding(crate::Padding::uniform(Length::px(6.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("first child text that wraps")));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("second child text also wraps")));
        commit_child(&mut arena, parent_key, Box::new(Text::from_content("tail")));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 180.0,
                max_height: 240.0,
                viewport_width: 180.0,
                viewport_height: 240.0,
                percent_base_width: Some(180.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 180.0,
                available_height: 240.0,
                viewport_width: 180.0,
                viewport_height: 240.0,
                percent_base_width: Some(180.0),
                percent_base_height: Some(240.0),
            },
        );

        {
            let wrapper_el = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
            assert!(
                wrapper_el.inline_paint_fragments.len() >= 2,
                "paint_fragments={:?}",
                wrapper_el.inline_paint_fragments
            );
        }

        let first_key = child_key(&arena, wrapper_key, 0);
        let second_key = child_key(&arena, wrapper_key, 1);

        let first_expected = arena
            .with_element_taken(first_key, |el, a| el.get_inline_nodes_size(a).len())
            .expect("first inline nodes size");
        let second_expected = arena
            .with_element_taken(second_key, |el, a| el.get_inline_nodes_size(a).len())
            .expect("second inline nodes size");

        let first = crate::view::test_support::get_element::<Text>(&arena, first_key);
        let second = crate::view::test_support::get_element::<Text>(&arena, second_key);
        let first_actual = first.inline_fragment_positions().len();
        let second_actual = second.inline_fragment_positions().len();

        assert_eq!(
            first_actual,
            first_expected,
            "first fragments={:?}",
            first.inline_fragment_positions()
        );
        assert_eq!(
            second_actual,
            second_expected,
            "second fragments={:?}",
            second.inline_fragment_positions()
        );
    }

    #[test]
    fn measure_recomputes_when_child_layout_dirty_under_same_proposal() {
        let constraints = LayoutConstraints {
            max_width: 240.0,
            max_height: 120.0,
            viewport_width: 240.0,
            viewport_height: 120.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(120.0),
        };

        let mut arena = new_test_arena();
        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
        );
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_element(&mut arena, Box::new(wrapper));

        let child_key_val = commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("a")));
        arena.with_element_taken(wrapper_key, |el, a| el.measure(constraints, a));
        let before_width = {
            let w = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
            w.measured_size().0
        };

        {
            let mut child = crate::view::test_support::get_element_mut::<Text>(&arena, child_key_val);
            child.set_text("a much longer child");
        }

        arena.with_element_taken(wrapper_key, |el, a| el.measure(constraints, a));
        let after_width = {
            let w = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
            w.measured_size().0
        };
        assert!(after_width > before_width + 1.0);
    }

    #[test]
    fn fragmentable_inline_element_remeasures_when_first_available_width_changes() {
        let mut arena = new_test_arena();
        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_element(&mut arena, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content(
            "Permission is hereby granted, free of charge, to any person obtaining a copy",
        )));

        arena.with_element_taken(wrapper_key, |el, a| {
            el.measure_inline(super::InlineMeasureContext {
                first_available_width: 200.0,
                full_available_width: 200.0,
                viewport_width: 200.0,
                viewport_height: 120.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(120.0),
            }, a);
        });
        let wide_nodes = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a))
            .expect("wide inline nodes");

        arena.with_element_taken(wrapper_key, |el, a| {
            el.measure_inline(super::InlineMeasureContext {
                first_available_width: 40.0,
                full_available_width: 200.0,
                viewport_width: 200.0,
                viewport_height: 120.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(120.0),
            }, a);
        });
        let narrow_first_line_nodes = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a))
            .expect("narrow inline nodes");

        assert_ne!(
            wide_nodes,
            narrow_first_line_nodes,
            "fragmentable inline element should remeasure when only first_available_width changes"
        );
    }

    #[test]
    fn inline_fragmentable_element_does_not_overlap_trailing_text_across_widths() {
        for width in 140..=240 {
            let width = width as f32;
            let mut arena = new_test_arena();
            let mut parent = Element::new(0.0, 0.0, width, 0.0);
            let mut parent_style = Style::new();
            parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
            parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
            parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
            parent.apply_style(parent_style);
            let parent_key = commit_element(&mut arena, Box::new(parent));
            commit_child(&mut arena, parent_key, Box::new(Text::from_content("Inline text starts here,")));

            let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
            let mut wrapper_style = Style::new();
            wrapper_style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(Color::hex("#93c5fd")));
            wrapper_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#ffffff")));
            wrapper_style.set_padding(crate::Padding::uniform(Length::px(8.0)));
            wrapper.apply_style(wrapper_style);
            let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
            commit_child(&mut arena, wrapper_key, Box::new(Text::from_content(
                "badge test test test test test test test",
            )));
            commit_child(&mut arena, parent_key, Box::new(Text::from_content(
                "then more text continues after the badge,",
            )));

            measure_and_place(
                &mut arena,
                parent_key,
                LayoutConstraints {
                    max_width: width,
                    max_height: 240.0,
                    viewport_width: width,
                    viewport_height: 240.0,
                    percent_base_width: Some(width),
                    percent_base_height: Some(240.0),
                },
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: width,
                    available_height: 240.0,
                    viewport_width: width,
                    viewport_height: 240.0,
                    percent_base_width: Some(width),
                    percent_base_height: Some(240.0),
                },
            );

            let nested_text_key = child_key(&arena, wrapper_key, 0);
            let trailing_key = child_key(&arena, parent_key, 2);

            let nested_fragments = {
                let nested_text = crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
                nested_text.inline_fragment_positions()
            };
            let trailing_fragments = {
                let trailing = crate::view::test_support::get_element::<Text>(&arena, trailing_key);
                trailing.inline_fragment_positions()
            };
            for (_, trailing_position) in &trailing_fragments {
                let same_line_right = nested_fragments
                    .iter()
                    .filter(|(_, nested_position)| (nested_position.y - trailing_position.y).abs() < 0.5)
                    .map(|(content, nested_position)| {
                        let mut text = Text::from_content(content.as_str());
                        text.measure(LayoutConstraints {
                            max_width: 200.0,
                            max_height: 80.0,
                            viewport_width: 200.0,
                            viewport_height: 80.0,
                            percent_base_width: Some(200.0),
                            percent_base_height: Some(80.0),
                        }, &mut arena);
                        let (fragment_width, _) = text.measured_size();
                        nested_position.x + fragment_width
                    })
                    .fold(None, |acc: Option<f32>, value| Some(acc.map_or(value, |max| max.max(value))));
                if let Some(nested_right) = same_line_right {
                    assert!(
                        nested_right <= trailing_position.x + 0.5,
                        "width={width}, nested_right={nested_right}, trailing_x={}, nested={nested_fragments:?}, trailing={trailing_fragments:?}",
                        trailing_position.x
                    );
                }
            }
        }
    }

    fn place_grandparent_parent_child(
        parent_box: (f32, f32, f32, f32),
        child_anchor: crate::Anchor,
        child_left: f32,
        child_top: f32,
    ) -> (
        crate::view::node_arena::NodeArena,
        crate::view::node_arena::NodeKey,
    ) {
        // grandparent (root) > parent (absolute @ parent_box) > child (absolute, anchor=...)
        let grandparent = Element::new(0.0, 0.0, 800.0, 600.0);
        let mut parent = Element::new(0.0, 0.0, parent_box.2, parent_box.3);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(parent_box.0))
                    .top(Length::px(parent_box.1)),
            ),
        );
        parent.apply_style(parent_style);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor(child_anchor)
                    .left(Length::px(child_left))
                    .top(Length::px(child_top)),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let gp_key = commit_element(&mut arena, Box::new(grandparent));
        let parent_key = commit_child(&mut arena, gp_key, Box::new(parent));
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            gp_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
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
            },
        );
        (arena, child_key)
    }

    #[test]
    fn anchor_parent_resolves_to_immediate_parent_box() {
        let (arena, child_k) = place_grandparent_parent_child(
            (100.0, 50.0, 200.0, 120.0),
            crate::Anchor::Parent,
            10.0,
            5.0,
        );
        let snap = child_snapshot(&arena, child_k);
        // child positioned at parent.x + left, parent.y + top
        assert!((snap.x - (100.0 + 10.0)).abs() < 0.01, "layout_x = {}", snap.x);
        assert!((snap.y - (50.0 + 5.0)).abs() < 0.01, "layout_y = {}", snap.y);
    }

    #[test]
    fn anchor_root_resolves_to_root_box() {
        // root is grandparent at (0,0,800,600). left=12, top=8 → child at (12,8).
        let (arena, child_k) = place_grandparent_parent_child(
            (100.0, 50.0, 200.0, 120.0),
            crate::Anchor::Viewport,
            12.0,
            8.0,
        );
        let snap = child_snapshot(&arena, child_k);
        assert!((snap.x - 12.0).abs() < 0.01, "layout_x = {}", snap.x);
        assert!((snap.y - 8.0).abs() < 0.01, "layout_y = {}", snap.y);
    }

    #[test]
    fn anchor_ancestor_n_walks_up_n_levels() {
        // Ancestor(1) == Parent.
        let (arena, child_k) = place_grandparent_parent_child(
            (100.0, 50.0, 200.0, 120.0),
            crate::Anchor::Ancestor(1),
            10.0,
            5.0,
        );
        let snap = child_snapshot(&arena, child_k);
        assert!((snap.x - 110.0).abs() < 0.01);
        assert!((snap.y - 55.0).abs() < 0.01);

        // Ancestor(2) == grandparent (root) at (0,0).
        let (arena2, child_k2) = place_grandparent_parent_child(
            (100.0, 50.0, 200.0, 120.0),
            crate::Anchor::Ancestor(2),
            12.0,
            8.0,
        );
        let snap2 = child_snapshot(&arena2, child_k2);
        assert!((snap2.x - 12.0).abs() < 0.01, "layout_x = {}", snap2.x);
        assert!((snap2.y - 8.0).abs() < 0.01, "layout_y = {}", snap2.y);
    }

    #[test]
    fn anchor_str_still_resolves_via_named_map() {
        // Regression: From<&str> for Anchor still flows through the named-anchor map.
        let parent = Element::new(0.0, 0.0, 500.0, 200.0);
        let mut anchor = Element::new(0.0, 0.0, 40.0, 40.0);
        let mut anchor_style = Style::new();
        anchor_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(300.0))
                    .top(Length::px(20.0)),
            ),
        );
        anchor.apply_style(anchor_style);
        anchor.set_anchor_name(Some(AnchorName::new("menu_button")));
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor("menu_button")
                    .left(Length::px(5.0))
                    .top(Length::px(0.0)),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _anchor_k = commit_child(&mut arena, parent_key, Box::new(anchor));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 600.0,
                max_height: 300.0,
                viewport_width: 600.0,
                percent_base_width: Some(600.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 600.0,
                available_height: 300.0,
                viewport_width: 600.0,
                percent_base_width: Some(600.0),
                percent_base_height: Some(300.0),
                viewport_height: 300.0,
            },
        );

        let snap = child_snapshot(&arena, child_k);
        // Anchored to menu_button at (300,20). left=5, top=0 → child at (305, 20).
        assert!((snap.x - 305.0).abs() < 0.01, "layout_x = {}", snap.x);
        assert!((snap.y - 20.0).abs() < 0.01, "layout_y = {}", snap.y);
    }

    /// Repro for the user-reported bug: when an ancestor extends beyond the
    /// viewport, an `absolute + Anchor::Viewport + clip:Viewport` descendant
    /// (e.g. a snackbar pinned to the viewport bottom) gets clipped /
    /// culled by the offscreen ancestor. Expected: the descendant should
    /// render at its viewport-anchored position, full viewport scissor,
    /// `should_render = true`, regardless of ancestor geometry.
    #[test]
    fn viewport_anchored_child_renders_when_ancestor_partly_offscreen() {
        // Window-like parent: clip:Viewport, dragged so left half is
        // offscreen — frame at (-200, 100), size 460x380.
        let mut parent = Element::new(0.0, 0.0, 460.0, 380.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(-200.0))
                    .top(Length::px(100.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        parent.apply_style(parent_style);

        // Snackbar-like child: absolute, Anchor::Viewport, clip:Viewport,
        // bottom=16 left=16 right=16 (spans width minus gaps), height=40.
        let mut child = Element::new(0.0, 0.0, 0.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor(crate::Anchor::Viewport)
                    .left(Length::px(16.0))
                    .right(Length::px(16.0))
                    .bottom(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let child_k = commit_child(&mut arena, parent_key, Box::new(child));

        // Viewport 1280x800.
        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 1280.0,
                max_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 1280.0,
                available_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
        );

        // Expected: child anchored to viewport, NOT to parent.
        // x = 16 (viewport left + 16), width = 1280 - 16 - 16 = 1248.
        // y = 800 - 16 - 40 = 744.
        let snap = child_snapshot(&arena, child_k);
        eprintln!(
            "[viewport-anchored snap] x={} y={} w={} h={}",
            snap.x, snap.y, snap.width, snap.height
        );
        assert!(
            (snap.x - 16.0).abs() < 0.5,
            "child x should pin to viewport+16, got {}",
            snap.x
        );
        assert!(
            (snap.y - 744.0).abs() < 0.5,
            "child y should pin to viewport bottom-16-40, got {}",
            snap.y
        );
        assert!(
            (snap.width - 1248.0).abs() < 0.5,
            "child width should span viewport minus 2*16, got {}",
            snap.width
        );

        // Should render — frame is fully inside viewport.
        let child_el = crate::view::test_support::get_element::<Element>(&arena, child_k);
        assert!(
            child_el.layout_state.should_render,
            "viewport-anchored child should render despite ancestor offscreen"
        );

        // absolute_clip_rect should be the full viewport rect (escape clip).
        let abs_clip = child_el.absolute_clip_rect.expect("clip_rect set for absolute");
        assert!(
            (abs_clip.x - 0.0).abs() < 0.01
                && (abs_clip.y - 0.0).abs() < 0.01
                && (abs_clip.width - 1280.0).abs() < 0.01
                && (abs_clip.height - 800.0).abs() < 0.01,
            "absolute_clip_rect should be viewport rect, got {:?}",
            abs_clip
        );
    }

    /// Deeper repro: ancestor chain (Window > content > section > snackbar)
    /// where Window is dragged so most of it sits offscreen. Verify the
    /// viewport-anchored snackbar still computes correct viewport-aligned
    /// frame and `should_render`.
    #[test]
    fn viewport_anchored_child_through_deep_offscreen_chain() {
        // Window outer: clip:Viewport, position absolute at (-300, 50),
        // size 460x380.
        let mut window = Element::new(0.0, 0.0, 460.0, 380.0);
        let mut window_style = Style::new();
        window_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(-300.0))
                    .top(Length::px(50.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        window.apply_style(window_style);

        // Window content (column flow, 100% width/height of window, with
        // padding to mimic the title bar etc).
        let mut content = Element::new(0.0, 0.0, 460.0, 350.0);
        let mut content_style = Style::new();
        content_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
        );
        content.apply_style(content_style);

        // Section wrapper inside content.
        let section = Element::new(0.0, 0.0, 460.0, 200.0);

        // Snackbar wrapper: absolute, Anchor::Viewport, clip:Viewport.
        let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
        let mut snackbar_style = Style::new();
        snackbar_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor(crate::Anchor::Viewport)
                    .left(Length::px(16.0))
                    .right(Length::px(16.0))
                    .bottom(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        snackbar.apply_style(snackbar_style);

        let mut arena = new_test_arena();
        let win_k = commit_element(&mut arena, Box::new(window));
        let content_k = commit_child(&mut arena, win_k, Box::new(content));
        let section_k = commit_child(&mut arena, content_k, Box::new(section));
        let snackbar_k = commit_child(&mut arena, section_k, Box::new(snackbar));

        // Viewport 1280x800.
        measure_and_place(
            &mut arena,
            win_k,
            LayoutConstraints {
                max_width: 1280.0,
                max_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 1280.0,
                available_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
        );

        let snap = child_snapshot(&arena, snackbar_k);
        assert!(
            (snap.x - 16.0).abs() < 0.5,
            "deep child x should pin to viewport, got {}",
            snap.x
        );
        assert!(
            (snap.y - 744.0).abs() < 0.5,
            "deep child y should pin to viewport bottom-16-40, got {}",
            snap.y
        );

        let snackbar_el = crate::view::test_support::get_element::<Element>(&arena, snackbar_k);
        assert!(
            snackbar_el.layout_state.should_render,
            "deep viewport-anchored child should render"
        );
    }

    /// Render-level repro: drive Element::build through the deep-offscreen
    /// ancestor chain, then run the deferred build for the snackbar's
    /// node id. Inspect the recorded FrameGraph pass count to confirm the
    /// snackbar actually emitted draw passes.
    #[test]
    fn viewport_anchored_child_renders_passes_through_offscreen_ancestor() {
        // Same scene as the layout-only test, but exercise the build/defer
        // pipeline.
        let mut window = Element::new(0.0, 0.0, 460.0, 380.0);
        let mut window_style = Style::new();
        window_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#222222")),
        );
        window_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(-300.0))
                    .top(Length::px(50.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        window.apply_style(window_style);

        let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
        let mut snackbar_style = Style::new();
        snackbar_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        snackbar_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor(crate::Anchor::Viewport)
                    .left(Length::px(16.0))
                    .right(Length::px(16.0))
                    .bottom(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        snackbar.apply_style(snackbar_style);

        let mut arena = new_test_arena();
        let win_k = commit_element(&mut arena, Box::new(window));
        let snackbar_k = commit_child(&mut arena, win_k, Box::new(snackbar));
        let snackbar_id = arena.get(snackbar_k).unwrap().element.stable_id();

        measure_and_place(
            &mut arena,
            win_k,
            LayoutConstraints {
                max_width: 1280.0,
                max_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 1280.0,
                available_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(1280, 800, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);

        // Main walk.
        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(win_k, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("window build returns state");
        ctx.set_state(next_state);

        let pass_count_before_defer = graph.pass_descriptors().len();

        // Defer pass.
        let deferred = ctx.take_deferred_node_ids();
        assert!(
            deferred.contains(&snackbar_id),
            "snackbar should be in deferred list, got {:?}",
            deferred
        );
        for id in &deferred {
            let _ = arena.with_element_taken(win_k, |root, arena| {
                crate::view::base_component::build_node_by_id(
                    root.as_mut(),
                    *id,
                    &mut graph,
                    arena,
                    &mut ctx,
                )
            });
        }

        let pass_count_after_defer = graph.pass_descriptors().len();
        assert!(
            pass_count_after_defer > pass_count_before_defer,
            "deferred snackbar should emit at least one render pass (before={}, after={})",
            pass_count_before_defer,
            pass_count_after_defer
        );
    }

    /// Even when ancestor is FULLY offscreen (intersects viewport = false),
    /// a viewport-anchored descendant must still render.
    #[test]
    fn viewport_anchored_child_renders_when_ancestor_fully_offscreen() {
        let mut window = Element::new(0.0, 0.0, 460.0, 380.0);
        let mut window_style = Style::new();
        window_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(-2000.0)) // way off the left edge
                    .top(Length::px(-2000.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        window.apply_style(window_style);

        let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
        let mut snackbar_style = Style::new();
        snackbar_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#00ff00")),
        );
        snackbar_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor(crate::Anchor::Viewport)
                    .left(Length::px(16.0))
                    .right(Length::px(16.0))
                    .bottom(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        snackbar.apply_style(snackbar_style);

        let mut arena = new_test_arena();
        let win_k = commit_element(&mut arena, Box::new(window));
        let snackbar_k = commit_child(&mut arena, win_k, Box::new(snackbar));
        let snackbar_id = arena.get(snackbar_k).unwrap().element.stable_id();

        measure_and_place(
            &mut arena,
            win_k,
            LayoutConstraints {
                max_width: 1280.0,
                max_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 1280.0,
                available_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
        );

        // Window should NOT render (fully offscreen).
        let window_el = crate::view::test_support::get_element::<Element>(&arena, win_k);
        assert!(
            !window_el.layout_state.should_render,
            "fully offscreen window should NOT render"
        );
        drop(window_el);

        // Snackbar should still render — it's anchored to viewport.
        let snackbar_el = crate::view::test_support::get_element::<Element>(&arena, snackbar_k);
        assert!(
            snackbar_el.layout_state.should_render,
            "viewport-anchored snackbar should render even when window is fully offscreen"
        );
        drop(snackbar_el);

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(1280, 800, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        // Mirror `Viewport::render_rsx`: seed the ctx defer list once
        // from the arena.
        let mut popup_stack = crate::view::popup_stack::PopupStack::new();
        arena.seed_defer_render_with_stack(&mut popup_stack, &mut ctx);

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(win_k, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("window build returns state");
        ctx.set_state(next_state);

        // Window's build with should_render=false should still collect
        // viewport-anchored descendants into the deferred list.
        let deferred = ctx.take_deferred_node_ids();
        assert!(
            deferred.contains(&snackbar_id),
            "snackbar should be deferred even when window not rendered, got {:?}",
            deferred
        );

        let pass_count_before_defer = graph.pass_descriptors().len();
        for id in &deferred {
            let _ = arena.with_element_taken(win_k, |root, arena| {
                crate::view::base_component::build_node_by_id(
                    root.as_mut(),
                    *id,
                    &mut graph,
                    arena,
                    &mut ctx,
                )
            });
        }
        let pass_count_after_defer = graph.pass_descriptors().len();
        assert!(
            pass_count_after_defer > pass_count_before_defer,
            "snackbar should emit passes even when ancestor is offscreen (before={}, after={})",
            pass_count_before_defer,
            pass_count_after_defer
        );
    }

    /// Repro for the user's video bug: when a tree-ancestor's inner area
    /// lies entirely outside the viewport (e.g. a Window dragged so its
    /// content area sits below viewport), a viewport-anchored
    /// `clip:Viewport` descendant gets dropped from the deferred list and
    /// never rendered. The ancestor itself still passes
    /// `should_render` (its frame intersects viewport at the top edge),
    /// but `has_visible_inner_render_area` returns false because the
    /// inner rect's intersection with the current scissor is empty —
    /// the overflow loop is skipped and the descendant is never appended
    /// via `append_to_defer`.
    #[test]
    fn viewport_anchored_descendant_collected_when_ancestor_inner_below_viewport() {
        // Window: clip:Viewport, top in viewport, content stretches below.
        let mut window = Element::new(0.0, 0.0, 460.0, 1500.0);
        let mut window_style = Style::new();
        window_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(100.0))
                    .top(Length::px(700.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        window.apply_style(window_style);

        // Section: positioned far down inside Window so its frame sits
        // entirely below viewport y=800.
        let section = Element::new(0.0, 1000.0, 460.0, 200.0);

        // Snackbar wrapper: viewport-anchored + clip:Viewport, bottom-left.
        let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
        let mut snackbar_style = Style::new();
        snackbar_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#00ff00")),
        );
        snackbar_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor(crate::Anchor::Viewport)
                    .left(Length::px(16.0))
                    .right(Length::px(16.0))
                    .bottom(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        snackbar.apply_style(snackbar_style);

        let mut arena = new_test_arena();
        let win_k = commit_element(&mut arena, Box::new(window));
        let section_k = commit_child(&mut arena, win_k, Box::new(section));
        let snackbar_k = commit_child(&mut arena, section_k, Box::new(snackbar));
        let snackbar_id = arena.get(snackbar_k).unwrap().element.stable_id();

        // Viewport 1280x800.
        measure_and_place(
            &mut arena,
            win_k,
            LayoutConstraints {
                max_width: 1280.0,
                max_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 1280.0,
                available_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
        );

        // Sanity: snackbar layout still anchored to viewport.
        let snap = child_snapshot(&arena, snackbar_k);
        assert!(
            (snap.x - 16.0).abs() < 0.5 && (snap.y - 744.0).abs() < 0.5,
            "snackbar should still be anchored to viewport, got ({}, {})",
            snap.x,
            snap.y
        );

        // Build via FrameGraph.
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(1280, 800, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        // Mirror `Viewport::render_rsx`: seed the ctx defer list once
        // from the arena.
        let mut popup_stack = crate::view::popup_stack::PopupStack::new();
        arena.seed_defer_render_with_stack(&mut popup_stack, &mut ctx);

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(win_k, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("window build returns state");
        ctx.set_state(next_state);

        let deferred = ctx.take_deferred_node_ids();
        assert!(
            deferred.contains(&snackbar_id),
            "BUG: snackbar should be in deferred list when ancestor inner is below viewport, got {:?}",
            deferred
        );
    }

    /// Closer repro of the user's video: Window with column flow content,
    /// multiple sections in the column, snackbar nested inside one of the
    /// later sections (Accordion-style). Window dragged down so the
    /// section that holds the snackbar sits entirely below viewport.
    /// Expected: snackbar (viewport-anchored) still rendered.
    #[test]
    fn viewport_anchored_snackbar_through_flow_column_below_viewport() {
        // Window outer at y=700, height=1500 (extends well below viewport).
        let mut window = Element::new(0.0, 0.0, 460.0, 1500.0);
        let mut window_style = Style::new();
        window_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(100.0))
                    .top(Length::px(700.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        window_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
        );
        window.apply_style(window_style);

        // 3 sections in the column (each 200 tall). With Window at y=700,
        // section heights: 200/200/200 → bottoms at 900/1100/1300 — all
        // below viewport=800.
        let section1 = Element::new(0.0, 0.0, 460.0, 200.0);
        let section2 = Element::new(0.0, 0.0, 460.0, 200.0);
        let section3 = Element::new(0.0, 0.0, 460.0, 200.0);

        let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
        let mut snackbar_style = Style::new();
        snackbar_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#00ff00")),
        );
        snackbar_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor(crate::Anchor::Viewport)
                    .left(Length::px(16.0))
                    .right(Length::px(16.0))
                    .bottom(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        snackbar.apply_style(snackbar_style);

        let mut arena = new_test_arena();
        let win_k = commit_element(&mut arena, Box::new(window));
        let _s1_k = commit_child(&mut arena, win_k, Box::new(section1));
        let _s2_k = commit_child(&mut arena, win_k, Box::new(section2));
        let s3_k = commit_child(&mut arena, win_k, Box::new(section3));
        let snackbar_k = commit_child(&mut arena, s3_k, Box::new(snackbar));
        let snackbar_id = arena.get(snackbar_k).unwrap().element.stable_id();

        measure_and_place(
            &mut arena,
            win_k,
            LayoutConstraints {
                max_width: 1280.0,
                max_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 1280.0,
                available_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
        );

        // Section 3 should be below viewport (its parent Window is at y=700,
        // section3 follows after section1+section2 = +400 → y=1100).
        let s3_snap = child_snapshot(&arena, s3_k);
        eprintln!(
            "[s3] x={} y={} w={} h={} should_render? -- need internal access",
            s3_snap.x, s3_snap.y, s3_snap.width, s3_snap.height
        );

        let snap = child_snapshot(&arena, snackbar_k);
        eprintln!(
            "[snackbar] x={} y={} w={} h={}",
            snap.x, snap.y, snap.width, snap.height
        );

        // Snackbar must still anchor to viewport.
        assert!(
            (snap.x - 16.0).abs() < 0.5 && (snap.y - 744.0).abs() < 0.5,
            "snackbar anchored to viewport, got ({}, {})",
            snap.x,
            snap.y
        );

        // Check render path: build then defer.
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(1280, 800, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        // Mirror `Viewport::render_rsx`: seed the ctx defer list once
        // from the arena.
        let mut popup_stack = crate::view::popup_stack::PopupStack::new();
        arena.seed_defer_render_with_stack(&mut popup_stack, &mut ctx);

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(win_k, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("window build returns state");
        ctx.set_state(next_state);

        let deferred = ctx.take_deferred_node_ids();
        eprintln!("[deferred ids] {:?}", deferred);
        eprintln!("[snackbar id] {}", snackbar_id);
        assert!(
            deferred.contains(&snackbar_id),
            "BUG: snackbar must be deferred when its tree-ancestor section is below viewport"
        );
    }

    /// Even deeper: viewport-clip element nested 4+ levels under a section
    /// that's below viewport (mimics `Window > content > Section >
    /// Accordion > AccordionContent > Snackbar wrapper`). Each intermediate
    /// ancestor's visibility gate fails because its inner is below viewport,
    /// so we need RECURSIVE defer collection (collect_root walks subtree).
    #[test]
    fn viewport_anchored_snackbar_deeply_nested_under_offscreen_section() {
        // Window outer.
        let mut window = Element::new(0.0, 0.0, 460.0, 1500.0);
        let mut window_style = Style::new();
        window_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(100.0))
                    .top(Length::px(700.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        window_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
        );
        window.apply_style(window_style);

        // Section1 (placeholder, fills first 200).
        let section1 = Element::new(0.0, 0.0, 460.0, 200.0);
        // Section2 = Snackbar Section (after section1, so y=900, well below
        // viewport=800).
        let section2 = Element::new(0.0, 0.0, 460.0, 200.0);
        // Inside section2: Accordion wrapper (~190 tall).
        let accordion = Element::new(0.0, 0.0, 460.0, 190.0);
        // Accordion content (after header).
        let accordion_content = Element::new(0.0, 0.0, 460.0, 150.0);

        let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
        let mut snackbar_style = Style::new();
        snackbar_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#00ff00")),
        );
        snackbar_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .anchor(crate::Anchor::Viewport)
                    .left(Length::px(16.0))
                    .right(Length::px(16.0))
                    .bottom(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        snackbar.apply_style(snackbar_style);

        let mut arena = new_test_arena();
        let win_k = commit_element(&mut arena, Box::new(window));
        let _s1_k = commit_child(&mut arena, win_k, Box::new(section1));
        let s2_k = commit_child(&mut arena, win_k, Box::new(section2));
        let acc_k = commit_child(&mut arena, s2_k, Box::new(accordion));
        let acc_content_k = commit_child(&mut arena, acc_k, Box::new(accordion_content));
        let snackbar_k =
            commit_child(&mut arena, acc_content_k, Box::new(snackbar));
        let snackbar_id = arena.get(snackbar_k).unwrap().element.stable_id();

        measure_and_place(
            &mut arena,
            win_k,
            LayoutConstraints {
                max_width: 1280.0,
                max_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 1280.0,
                available_height: 800.0,
                viewport_width: 1280.0,
                viewport_height: 800.0,
                percent_base_width: Some(1280.0),
                percent_base_height: Some(800.0),
            },
        );

        let snap = child_snapshot(&arena, snackbar_k);
        assert!(
            (snap.x - 16.0).abs() < 0.5 && (snap.y - 744.0).abs() < 0.5,
            "snackbar still anchored to viewport, got ({}, {})",
            snap.x,
            snap.y
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(1280, 800, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        // Mirror `Viewport::render_rsx`: seed the ctx defer list once
        // from the arena.
        let mut popup_stack = crate::view::popup_stack::PopupStack::new();
        arena.seed_defer_render_with_stack(&mut popup_stack, &mut ctx);

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(win_k, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("window build returns state");
        ctx.set_state(next_state);

        let deferred = ctx.take_deferred_node_ids();
        assert!(
            deferred.contains(&snackbar_id),
            "BUG: deeply nested snackbar must still be deferred. defer={:?} snackbar_id={}",
            deferred,
            snackbar_id
        );
    }
}
