#[cfg(test)]
mod tests {
    use super::{
        expand_corner_radii_for_spread, main_axis_start_and_gap, normalize_corner_radii,
        resolve_px_with_base, resolve_signed_px_with_base, Element, ElementTrait, EventTarget,
        LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
    };
    use super::super::core::Position as LayoutPosition;
    use crate::view::base_component::Text;
    use crate::style::{ParsedValue, PropertyId, Transition, TransitionProperty, Transitions};
    use crate::transition::{LayoutField, VisualField};
    use crate::view::base_component::{
        reset_test_promoted_build_counts, set_style_field_by_id, test_promoted_build_count,
    };
    use crate::view::frame_graph::FrameGraph;
    use crate::Layout;
    use crate::{
        Align, AnchorName, Angle, Border, BorderRadius, BoxShadow, ClipMode, Collision,
        CollisionBoundary, Color, CrossSize, JustifyContent, Length, Opacity, Operator,
        Position, Rotate, Transform, TransformOrigin, Translate, Style,
    };
    use glam::{Mat4, Vec3};
    use std::collections::{HashMap, HashSet};
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

        let child = Element::new(4.0, 6.0, 300.0, 300.0);
        root.add_child(Box::new(child));

        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });
        let children = root.children().expect("element has children");
        let snapshot = children[0].box_model_snapshot();

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

        let child = Element::new(0.0, 0.0, 300.0, 300.0);
        root.add_child(Box::new(child));

        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });
        let children = root.children().expect("element has children");
        let snapshot = children[0].box_model_snapshot();

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

        root.measure(crate::view::base_component::LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        });
        root.place(crate::view::base_component::LayoutPlacement {
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
        });

        let snapshot = root.box_model_snapshot();
        assert!((snapshot.x - 5.5).abs() < 0.01);
        assert!((snapshot.y - 7.6).abs() < 0.01);
        assert!((snapshot.width - 100.5).abs() < 0.01);
        assert!((snapshot.height - 50.5).abs() < 0.01);
        assert!((root.layout_inner_position.x - 8.75).abs() < 0.01);
        assert!((root.layout_inner_position.y - 10.1).abs() < 0.01);
        assert!((root.layout_inner_size.width - 94.0).abs() < 0.01);
        assert!((root.layout_inner_size.height - 45.5).abs() < 0.01);
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
        parent.add_child(Box::new(child));

        parent.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(crate::view::base_component::LayoutPlacement {
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
        });
        let snapshot_unknown = parent.children().expect("child")[0].box_model_snapshot();
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
        known_parent.add_child(Box::new(child2));

        known_parent.measure(crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        known_parent.place(crate::view::base_component::LayoutPlacement {
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
        });
        let snapshot_known = known_parent.children().expect("child")[0].box_model_snapshot();
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: None,
            percent_base_height: None,
        });
        parent.place(LayoutPlacement {
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
        });

        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
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

        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        el.place(LayoutPlacement {
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
        });

        assert_eq!(el.box_model_snapshot().width, 850.0);
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });
        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
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
        parent.add_child(Box::new(child));

        parent.measure(crate::view::base_component::LayoutConstraints {
            max_width: 240.0,
            max_height: 40.0,
            viewport_width: 240.0,
            viewport_height: 40.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(40.0),
        });
        parent.place(crate::view::base_component::LayoutPlacement {
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
        });

        let child = parent.children().expect("child")[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("element child");
        let snapshot = child.box_model_snapshot();
        let inner = child.inner_clip_rect();

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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });
        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
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

        parent.add_child(Box::new(normal_child));
        parent.add_child(Box::new(absolute_child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let snapshot = parent.box_model_snapshot();
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

        parent.add_child(Box::new(Element::new(0.0, 0.0, 80.0, 30.0)));
        parent.add_child(Box::new(Element::new(0.0, 0.0, 120.0, 10.0)));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let snapshot = parent.box_model_snapshot();
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

        parent.add_child(Box::new(Element::new(0.0, 0.0, 80.0, 40.0)));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
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

        parent.add_child(Box::new(explicit_child));
        parent.add_child(Box::new(auto_child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let explicit_snapshot = children[0].box_model_snapshot();
        let auto_snapshot = children[1].box_model_snapshot();

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

        parent.add_child(Box::new(first));
        parent.add_child(Box::new(second));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();
        assert_eq!(first_snapshot.width, 113.0);
        assert_eq!(second_snapshot.width, 187.0);
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

        parent.add_child(Box::new(first));
        parent.add_child(Box::new(second));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();

        assert!((first_snapshot.width - 75.0).abs() < 0.01);
        assert!((second_snapshot.width - 75.0).abs() < 0.01);
        assert!((second_snapshot.x - 75.0).abs() < 0.01);
    }

    #[test]
    fn absolute_defaults_to_parent_anchor_and_zero_insets() {
        let mut parent = Element::new(40.0, 60.0, 200.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute()),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("has child");
        let snapshot = children[0].box_model_snapshot();
        assert_eq!(snapshot.x, 40.0);
        assert_eq!(snapshot.y, 60.0);
    }

    #[test]
    fn absolute_stretch_with_left_right_top_bottom() {
        let mut parent = Element::new(10.0, 20.0, 200.0, 120.0);
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("has child");
        let snapshot = children[0].box_model_snapshot();
        assert_eq!(snapshot.x, 20.0);
        assert_eq!(snapshot.y, 25.0);
        assert_eq!(snapshot.width, 170.0);
        assert_eq!(snapshot.height, 100.0);
    }

    #[test]
    fn absolute_negative_insets_are_preserved() {
        let mut parent = Element::new(10.0, 20.0, 200.0, 120.0);
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("has child");
        let snapshot = children[0].box_model_snapshot();
        assert_eq!(snapshot.x, 0.0);
        assert_eq!(snapshot.y, 15.0);
        assert_eq!(snapshot.width, 190.0);
        assert_eq!(snapshot.height, 110.0);
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

        el.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        el.place(LayoutPlacement {
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
        });
        let snapshot = el.box_model_snapshot();
        assert_eq!(snapshot.x, 350.0);
        assert_eq!(snapshot.y, 270.0);
    }

    #[test]
    fn absolute_clip_viewport_allows_render_outside_parent_bounds() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("has child");
        let rendered = children[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("downcast child")
            .core
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
        parent.add_child(Box::new(child));
        parent.core.should_render = false;

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let next_state = parent.build(
            &mut graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
        );
        ctx.set_state(next_state);

        let deferred = ctx.take_deferred_node_ids();
        let child_id = parent.children().expect("has child")[0].id();
        assert!(deferred.contains(&child_id));
    }

    #[test]
    fn absolute_clip_anchor_parent_falls_back_to_parent_without_anchor() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("has child");
        let rendered = children[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("downcast child")
            .core
            .should_render;
        assert!(!rendered);
    }

    #[test]
    fn absolute_clip_anchor_parent_uses_anchor_parent_bounds() {
        let mut parent = Element::new(0.0, 0.0, 500.0, 200.0);
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
        parent.add_child(Box::new(anchor));
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 600.0,
            max_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("has children");
        let rendered = children[1]
            .as_any()
            .downcast_ref::<Element>()
            .expect("downcast child")
            .core
            .should_render;
        assert!(rendered);
    }

    #[test]
    fn absolute_clip_anchor_parent_scissor_uses_anchor_parent_bounds() {
        let mut parent = Element::new(0.0, 0.0, 500.0, 200.0);
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
        parent.add_child(Box::new(anchor));
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 600.0,
            max_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("has children");
        let child = children[1]
            .as_any()
            .downcast_ref::<Element>()
            .expect("downcast child");
        assert_eq!(child.absolute_clip_scissor_rect(), Some([0, 0, 500, 200]));
    }

    #[test]
    fn absolute_clip_anchor_parent_scissor_falls_back_to_parent_without_anchor() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("has child");
        let child = children[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("downcast child");
        assert_eq!(child.absolute_clip_scissor_rect(), Some([0, 0, 100, 80]));
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

        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        el.place(LayoutPlacement {
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
        });
        let _ = el.take_visual_transition_requests();

        let mut next_style = Style::new();
        next_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
        next_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(90.0)));
        el.apply_style(next_style);
        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        el.place(LayoutPlacement {
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
        });

        let reqs = el.take_layout_transition_requests();
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

        el.measure(constraints);
        el.place(placement_at_100);
        let _ = el.take_visual_transition_requests();

        // Simulate an in-flight visual offset frame: target rel-x=50, offset=30 => abs x = 180.
        el.set_layout_transition_x(30.0);
        el.place(placement_at_100);
        let _ = el.take_layout_transition_requests();

        // A reflow shifts parent origin and updates target x.
        el.set_position(120.0, 0.0);
        el.place(LayoutPlacement {
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
        });

        let reqs = el.take_visual_transition_requests();
        let x_req = reqs
            .iter()
            .find(|r| r.field == VisualField::X)
            .expect("x transition request should exist");
        // current abs(180) - new parent_x(130) = 50, target rel-x=120 => offset = -70
        assert!((x_req.from + 70.0).abs() < 0.01);
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

        el.measure(constraints);
        el.place(placement);
        let _ = el.take_layout_transition_requests();
        let _ = el.take_visual_transition_requests();

        el.set_position(120.0, 0.0);
        let mut next_style = Style::new();
        next_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
        el.apply_style(next_style);
        el.measure(constraints);
        el.place(placement);

        let snapshot = el.box_model_snapshot();
        let layout_reqs = el.take_layout_transition_requests();
        let visual_reqs = el.take_visual_transition_requests();
        assert!((snapshot.x - 150.0).abs() < 0.01);
        assert!((snapshot.width - 100.0).abs() < 0.01);
        assert!((el.layout_transition_visual_offset_x + 70.0).abs() < 0.01);
        assert_eq!(el.layout_transition_override_width, Some(100.0));
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

        el.measure(constraints);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        // Simulate in-flight width frame.
        el.set_layout_transition_width(140.0);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        // Reflow updates target width while parent origin also changes.
        let mut next_style = Style::new();
        next_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        el.apply_style(next_style);
        el.measure(constraints);
        el.place(LayoutPlacement {
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
        });

        let reqs = el.take_layout_transition_requests();
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

        el.measure(constraints);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        // Simulate in-flight height frame.
        el.set_layout_transition_height(70.0);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        // Reflow updates target height while parent origin also changes.
        let mut next_style = Style::new();
        next_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(160.0)));
        el.apply_style(next_style);
        el.measure(constraints);
        el.place(LayoutPlacement {
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
        });

        let reqs = el.take_layout_transition_requests();
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

        el.measure(constraints);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        let mut expanded_style = Style::new();
        expanded_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(160.0)));
        el.apply_style(expanded_style);
        el.measure(constraints);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        el.set_layout_transition_height(70.0);
        el.place(placement);
        let _ = el.take_layout_transition_requests();

        let mut collapsed_style = Style::new();
        collapsed_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        el.apply_style(collapsed_style);
        el.measure(constraints);
        el.place(placement);

        let reqs = el.take_layout_transition_requests();
        let h_req = reqs
            .iter()
            .find(|r| r.field == LayoutField::Height)
            .expect("height transition request should retarget");
        assert!((h_req.from - 70.0).abs() < 0.01);
        assert!((h_req.to - 20.0).abs() < 0.01);
    }

    #[test]
    fn snapshot_restore_keeps_layout_transition_inflight_state() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        el.has_layout_snapshot = true;
        el.layout_transition_visual_offset_x = 12.5;
        el.layout_transition_visual_offset_y = -3.0;
        el.layout_transition_override_width = Some(140.0);
        el.layout_transition_override_height = Some(55.0);
        el.layout_transition_target_x = Some(30.0);
        el.layout_transition_target_y = Some(8.0);
        el.layout_transition_target_width = Some(180.0);
        el.layout_transition_target_height = Some(80.0);
        el.last_parent_layout_x = 21.0;
        el.last_parent_layout_y = 34.0;

        let snapshot = el.snapshot_state().expect("snapshot should exist");

        let mut restored = Element::new(0.0, 0.0, 100.0, 40.0);
        let ok = restored.restore_state(snapshot.as_ref());
        assert!(ok);

        assert!(restored.has_layout_snapshot);
        assert!((restored.layout_transition_visual_offset_x - 12.5).abs() < 0.001);
        assert!((restored.layout_transition_visual_offset_y + 3.0).abs() < 0.001);
        assert_eq!(restored.layout_transition_override_width, Some(140.0));
        assert_eq!(restored.layout_transition_override_height, Some(55.0));
        assert_eq!(restored.layout_transition_target_x, Some(30.0));
        assert_eq!(restored.layout_transition_target_y, Some(8.0));
        assert_eq!(restored.layout_transition_target_width, Some(180.0));
        assert_eq!(restored.layout_transition_target_height, Some(80.0));
        assert!((restored.last_parent_layout_x - 21.0).abs() < 0.001);
        assert!((restored.last_parent_layout_y - 34.0).abs() < 0.001);
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
        old.layout_flow_position = LayoutPosition { x: 170.0, y: 0.0 };
        old.core.layout_position = LayoutPosition { x: 150.0, y: 0.0 };
        old.layout_transition_visual_offset_x = -20.0;
        old.layout_transition_target_x = Some(70.0);

        let layout_snapshots =
            crate::view::base_component::collect_layout_transition_snapshots(&[Box::new(old)]);

        let mut rebuilt = Element::new_with_id(42, 50.0, 0.0, 100.0, 40.0);
        rebuilt.has_layout_snapshot = true;
        rebuilt.layout_transition_visual_offset_x = -20.0;
        rebuilt.layout_transition_target_x = Some(70.0);
        let mut roots: Vec<Box<dyn ElementTrait>> = vec![Box::new(rebuilt)];
        crate::view::base_component::seed_layout_transition_snapshots(&mut roots, &layout_snapshots);

        let rebuilt = roots[0]
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("downcast rebuilt element");
        assert_eq!(rebuilt.core.layout_position.x, 150.0);
        assert_eq!(rebuilt.layout_flow_position.x, 170.0);

        rebuilt.place(LayoutPlacement {
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
        });

        assert!((rebuilt.core.layout_position.x - 150.0).abs() < 0.01);
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

        parent.add_child(Box::new(first));
        parent.add_child(Box::new(second));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let snapshot = parent.children().expect("child")[0].box_model_snapshot();
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
        first.add_child(Box::new(Element::new(0.0, 0.0, 20.0, 20.0)));

        let mut second = Element::new(0.0, 0.0, 120.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().shrink(1.0)));
        second.apply_style(second_style);

        parent.add_child(Box::new(first));
        parent.add_child(Box::new(second));

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

        parent.measure(constraints);
        parent.place(placement);
        let children = parent.children().expect("children after first layout");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();
        assert!((first_snapshot.width - 20.0).abs() < 0.01);
        assert!((second_snapshot.width - 80.0).abs() < 0.01);

        parent.mark_layout_dirty();
        parent.measure(constraints);
        parent.place(placement);
        let children = parent.children().expect("children after second layout");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();
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

        parent.add_child(Box::new(first));
        parent.add_child(Box::new(second));
        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();
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

        parent.add_child(Box::new(first));
        parent.add_child(Box::new(second));
        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();
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
        first.add_child(Box::new(Element::new(0.0, 0.0, 60.0, 20.0)));

        let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(60.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(PropertyId::Flex, ParsedValue::Flex(crate::flex().shrink(1.0)));
        second.apply_style(second_style);

        parent.add_child(Box::new(first));
        parent.add_child(Box::new(second));
        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();
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
        first.add_child(Box::new(Element::new(0.0, 0.0, 60.0, 20.0)));

        let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(60.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().shrink(1.0)),
        );
        second.apply_style(second_style);

        parent.add_child(Box::new(first));
        parent.add_child(Box::new(second));
        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let first_snapshot = children[0].box_model_snapshot();
        let second_snapshot = children[1].box_model_snapshot();
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

        parent.add_child(Box::new(spacer));
        parent.add_child(Box::new(thumb));

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

        parent.measure(constraints);
        parent.place(placement);
        let _ = parent.take_layout_transition_requests();

        let mut next_spacer_style = Style::new();
        next_spacer_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        parent
            .children_mut()
            .expect("children")[0]
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("spacer")
            .apply_style(next_spacer_style);

        parent.mark_layout_dirty();
        parent.measure(constraints);
        parent.place(placement);

        let reqs = parent
            .children_mut()
            .expect("children")[0]
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("spacer")
            .take_layout_transition_requests();
        assert!(reqs.iter().any(|req| req.field == LayoutField::Width));

        parent
            .children_mut()
            .expect("children")[0]
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("spacer")
            .set_layout_transition_width(10.0);
        parent.mark_layout_dirty();
        parent.measure(constraints);
        parent.place(placement);

        let children = parent.children().expect("children");
        let thumb_snapshot = children[1].box_model_snapshot();
        assert!((thumb_snapshot.x - 10.0).abs() < 0.01);
    }

    #[test]
    fn snapshot_restore_preserves_hover_style_state() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#112233")),
        );
        let mut hover_style = Style::new();
        hover_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#aabbcc")),
        );
        style.set_hover(hover_style);
        el.apply_style(style);
        let _ = el.set_hovered(true);

        let snapshot = el.snapshot_state().expect("snapshot should exist");

        let mut restored = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut restored_style = Style::new();
        restored_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#112233")),
        );
        let mut restored_hover_style = Style::new();
        restored_hover_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#aabbcc")),
        );
        restored_style.set_hover(restored_hover_style);
        restored.apply_style(restored_style);

        let ok = restored.restore_state(snapshot.as_ref());
        assert!(ok);
        assert!(restored.is_hovered);
        assert_eq!(
            restored.background_color.as_ref().to_rgba_u8(),
            [170, 187, 204, 255]
        );
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

        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        el.place(LayoutPlacement {
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
        });

        let snapshot = el.box_model_snapshot();
        assert_eq!(snapshot.width, 180.0);
        assert_eq!(snapshot.height, 40.0);
    }

    #[test]
    fn percent_min_and_max_size_resolve_against_parent_inner_size() {
        let mut parent = Element::new(0.0, 0.0, 300.0, 200.0);
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let child_snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(child_snapshot.width, 180.0);
        assert_eq!(child_snapshot.height, 80.0);
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let child_snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert_eq!(child_snapshot.width, 480.0);
        assert_eq!(child_snapshot.height, 420.0);
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

        el.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        el.place(LayoutPlacement {
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
        });

        let snapshot = el.box_model_snapshot();
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
        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let child = Element::new(20.0, 20.0, 40.0, 40.0);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let inner_radii = parent.inner_clip_radii(normalize_corner_radii(
            parent.border_radii,
            parent.core.layout_size.width.max(0.0),
            parent.core.layout_size.height.max(0.0),
        ));
        let overflow_child_indices: Vec<bool> = (0..parent.children.len())
            .map(|idx| parent.child_renders_outside_inner_clip(idx))
            .collect();
        assert!(!parent.should_clip_children(&overflow_child_indices, inner_radii));
    }

    #[test]
    fn child_clip_scope_is_required_when_child_overflows_inner_rect() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut child = Element::new(0.0, 0.0, 140.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
        child.apply_style(style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let inner_radii = parent.inner_clip_radii(normalize_corner_radii(
            parent.border_radii,
            parent.core.layout_size.width.max(0.0),
            parent.core.layout_size.height.max(0.0),
        ));
        let overflow_child_indices: Vec<bool> = (0..parent.children.len())
            .map(|idx| parent.child_renders_outside_inner_clip(idx))
            .collect();
        assert!(parent.should_clip_children(&overflow_child_indices, inner_radii));
    }

    #[test]
    fn child_clip_scope_uses_stencil_without_rounding() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut child = Element::new(0.0, 0.0, 140.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
        child.apply_style(style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);

        let inner_radii = parent.inner_clip_radii(normalize_corner_radii(
            parent.border_radii,
            parent.core.layout_size.width.max(0.0),
            parent.core.layout_size.height.max(0.0),
        ));
        assert!(!inner_radii.has_any_rounding());

        let scope = parent.begin_child_clip_scope(&mut graph, &mut ctx, inner_radii);
        assert!(scope.is_some());
        assert!(scope.as_ref().is_some_and(|scope| scope.child_clip_id != 0));
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
        parent.add_child(Box::new(child));
        let child_id = parent.children().expect("has child")[0].id();

        parent.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        ctx.set_promoted_runtime(
            Arc::new(HashSet::from([child_id])),
            Arc::new(HashMap::new()),
            Arc::new(HashMap::new()),
        );

        let next_state = parent.build(
            &mut graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
        );
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

        let child = Element::new(0.0, 0.0, 120.0, 360.0);
        parent.add_child(Box::new(child));
        let parent_id = parent.id();

        parent.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        ctx.set_promoted_runtime(
            Arc::new(HashSet::from([parent_id])),
            Arc::new(HashMap::new()),
            Arc::new(HashMap::new()),
        );

        let mut layer_ctx = UiBuildContext::from_parts(
            ctx.viewport(),
            super::BuildState::for_layer_subtree_with_ancestor_clip(ctx.ancestor_clip_context()),
        );
        let layer_target = layer_ctx.allocate_promoted_layer_target(
            &mut graph,
            parent_id,
            parent.promotion_composite_bounds(),
        );
        layer_ctx.set_current_target(layer_target);
        let next_state = parent.build_promoted_layer(
            &mut graph,
            layer_ctx,
            crate::view::promotion::PromotedLayerUpdateKind::Reraster,
            false,
            crate::view::viewport::DebugReusePathContext::Root,
        );
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
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);

        let next_state = parent.build(
            &mut graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
        );

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
        row_child.add_child(Box::new(Element::new(0.0, 0.0, 40.0, 24.0)));
        parent.add_child(Box::new(row_child));

        parent.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let row_snapshot = parent.children().expect("child")[0].box_model_snapshot();
        assert!((row_snapshot.height - 24.0).abs() < 0.01);
        assert!((parent.content_size.height - 24.0).abs() < 0.01);
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
            stretched.add_child(Box::new(Element::new(0.0, 0.0, 120.0, 40.0)));

            parent.add_child(Box::new(tall));
            parent.add_child(Box::new(stretched));

            parent.measure(LayoutConstraints {
                max_width: 320.0,
                max_height: 140.0,
                viewport_width: 320.0,
                viewport_height: 140.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(140.0),
            });
            parent.place(LayoutPlacement {
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
            });
            let _ = parent.children[1]
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("stretched child element")
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
            parent.apply_style(next_parent_style);
            parent.measure(LayoutConstraints {
                max_width: 320.0,
                max_height: 140.0,
                viewport_width: 320.0,
                viewport_height: 140.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(140.0),
            });
            assert_eq!(parent.computed_style.layout_axis_cross_size(), CrossSize::Stretch);
            assert!(parent.children[1].allows_cross_stretch(true));
            parent.place(LayoutPlacement {
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
            });

            let children = parent.children().expect("children");
            let stretched_snapshot = children[1].box_model_snapshot();
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

            let stretched = parent.children[1]
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("stretched child element");
            stretched.set_layout_transition_height(100.0);

            parent.place(LayoutPlacement {
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
            });

            let stretched = parent.children[1]
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("stretched child element");
            stretched.layout_transition_override_height = None;
            stretched.layout_transition_target_height = None;

            parent.place(LayoutPlacement {
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
            });

            let children = parent.children().expect("children");
            let stretched_snapshot = children[1].box_model_snapshot();
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
        let mut root = Element::new(0.0, 0.0, 200.0, 200.0);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#202020")),
        );
        root.apply_style(root_style);
        let mut container = Element::new(0.0, 0.0, 120.0, 120.0);
        let container_id = container.id();
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

        let mut promoted_child = Element::new(0.0, 0.0, 120.0, 240.0);
        let mut promoted_child_style = Style::new();
        promoted_child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        promoted_child.apply_style(promoted_child_style);
        let promoted_child_id = promoted_child.id();
        container.add_child(Box::new(promoted_child));
        root.add_child(Box::new(container));

        root.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        });
        root.place(LayoutPlacement {
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
        });

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        ctx.set_promoted_runtime(
            Arc::new(HashSet::from([promoted_child_id])),
            Arc::new(HashMap::new()),
            Arc::new(HashMap::new()),
        );
        reset_test_promoted_build_counts();

        let next_state = root.build(
            &mut graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
        );
        ctx.set_state(next_state);

        assert_eq!(
            test_promoted_build_count(container_id, "base"),
            1,
            "expected non-promoted container base path to run only once"
        );
    }

    #[test]
    fn zero_opacity_sets_should_paint_false_but_keeps_render() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#112233")),
        );
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
        el.apply_style(style);

        el.measure(LayoutConstraints {
            max_width: 100.0,
            max_height: 40.0,
            viewport_width: 100.0,
            viewport_height: 40.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(40.0),
        });
        el.place(LayoutPlacement {
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
        });

        assert!(el.core.should_render);
        assert!(!el.core.should_paint);
    }

    #[test]
    fn transformed_bounds_are_used_for_clip_culling() {
        let mut el = Element::new(120.0, 0.0, 40.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(40.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        style.set_transform(Transform::new([Translate::x(Length::px(-80.0))]));
        style.set_transform_origin(TransformOrigin::center());
        el.apply_style(style);

        el.measure(LayoutConstraints {
            max_width: 200.0,
            max_height: 100.0,
            viewport_width: 200.0,
            viewport_height: 100.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(100.0),
        });
        el.place(LayoutPlacement {
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
        });

        let transformed = el.transformed_frame_bounding_rect(super::LayoutFrame {
            x: el.core.layout_position.x,
            y: el.core.layout_position.y,
            width: el.core.layout_size.width,
            height: el.core.layout_size.height,
        });
        assert!((transformed.x - 40.0).abs() < 0.01, "{transformed:?}");
        assert!((transformed.width - 40.0).abs() < 0.01, "{transformed:?}");
        assert!(
            el.core.should_render,
            "translate 後的 bounding box 已進入 parent clip，不應被提前剔除"
        );
    }

    #[test]
    fn promotion_composite_bounds_follow_transformed_bounding_box() {
        let mut el = Element::new(40.0, 20.0, 30.0, 20.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(30.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        style.set_transform(Transform::new([Rotate::z(Angle::deg(90.0))]));
        style.set_transform_origin(TransformOrigin::center());
        el.apply_style(style);

        el.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        });
        el.place(LayoutPlacement {
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
        });

        let bounds = el.promotion_composite_bounds();
        assert!((bounds.x - 45.0).abs() < 0.01);
        assert!((bounds.y - 15.0).abs() < 0.01);
        assert!((bounds.width - 20.0).abs() < 0.01);
        assert!((bounds.height - 30.0).abs() < 0.01);
    }

    #[test]
    fn transparent_borderless_shadowless_element_does_not_paint_even_with_child() {
        let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
        let mut child = Element::new(0.0, 0.0, 60.0, 40.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#ff0000")),
        );
        child.apply_style(child_style);
        parent.add_child(Box::new(child));

        parent.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        assert!(parent.core.should_render);
        assert!(!parent.core.should_paint);
    }

    #[test]
    fn zero_inner_area_sets_should_paint_false() {
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

        el.measure(LayoutConstraints {
            max_width: 20.0,
            max_height: 20.0,
            viewport_width: 20.0,
            viewport_height: 20.0,
            percent_base_width: Some(20.0),
            percent_base_height: Some(20.0),
        });
        el.place(LayoutPlacement {
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
        });

        assert_eq!(el.layout_inner_size.width, 0.0);
        assert_eq!(el.layout_inner_size.height, 0.0);
        assert!(el.core.should_render);
        assert!(!el.core.should_paint);
    }

    #[test]
    fn transition_override_keeps_inner_render_area_available() {
        let mut el = Element::new(0.0, 0.0, 20.0, 20.0);
        el.core.layout_position = LayoutPosition { x: 0.0, y: 0.0 };
        el.core.layout_size.width = 0.0;
        el.core.layout_size.height = 0.0;
        el.layout_inner_position = LayoutPosition { x: 0.0, y: 0.0 };
        el.layout_inner_size.width = 0.0;
        el.layout_inner_size.height = 0.0;
        el.layout_transition_override_width = Some(40.0);
        el.layout_transition_override_height = Some(30.0);

        assert!(el.has_inner_render_area());
        let transition_inner = el.transition_inner_rect();
        assert_eq!(transition_inner.width, 40.0);
        assert_eq!(transition_inner.height, 30.0);
        let inner = el.inner_clip_rect();
        assert_eq!(inner.width, 20.0);
        assert_eq!(inner.height, 20.0);
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
        let mut el = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut style = Style::new();
        style.set_border_radius(
            BorderRadius::uniform(Length::px(10.0))
                .top_right(Length::px(32.0))
                .bottom_left(Length::percent(90.0)),
        );
        el.apply_style(style);
        let node_id = el.id();

        assert!(set_style_field_by_id(
            &mut el,
            node_id,
            crate::transition::StyleField::BorderRadius,
            crate::transition::StyleValue::Scalar(50.0),
        ));

        assert!((el.border_radii.top_left - 3.7037036).abs() < 0.001);
        assert!((el.border_radii.top_right - 11.851851).abs() < 0.001);
        assert!((el.border_radii.bottom_right - 3.7037036).abs() < 0.001);
        assert!((el.border_radii.bottom_left - 50.0).abs() < 0.001);
        assert!((el.border_radius - 50.0).abs() < 0.001);
    }

    #[test]
    fn snapshot_restore_with_same_style_does_not_emit_spurious_border_radius_transition() {
        let mut original = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut style = Style::new();
        style.set_border_radius(
            BorderRadius::uniform(Length::px(10.0))
                .top_right(Length::px(32.0))
                .bottom_left(Length::percent(90.0)),
        );
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::from(vec![Transition::new(
                TransitionProperty::All,
                180,
            )])),
        );
        original.apply_style(style.clone());

        let snapshot = original.snapshot_state().expect("snapshot should exist");

        let mut rebuilt = Element::new(0.0, 0.0, 0.0, 0.0);
        rebuilt.apply_style(style);
        assert!(rebuilt.restore_state(snapshot.as_ref()));

        let requests = std::mem::take(&mut rebuilt.pending_style_transition_requests);
        assert!(
            requests.is_empty(),
            "restore of identical style should not enqueue style transitions: {requests:?}"
        );
    }

    #[test]
    fn snapshot_restore_emits_background_transition_when_current_style_differs() {
        let mut original = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut original_style = Style::new();
        original_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#112233")),
        );
        original.apply_style(original_style);
        let snapshot = original.snapshot_state().expect("snapshot should exist");

        let mut rebuilt = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut rebuilt_style = Style::new();
        rebuilt_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#445566")),
        );
        rebuilt_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::from(vec![Transition::new(
                TransitionProperty::BackgroundColor,
                180,
            )])),
        );
        rebuilt.apply_style(rebuilt_style);

        assert!(rebuilt.restore_state(snapshot.as_ref()));

        let requests = std::mem::take(&mut rebuilt.pending_style_transition_requests);
        assert_eq!(requests.len(), 1, "expected one background transition request");
        assert_eq!(requests[0].field, crate::transition::StyleField::BackgroundColor);
        assert_eq!(
            requests[0].from,
            crate::transition::StyleValue::Color(Color::rgba(0x11, 0x22, 0x33, 0xff))
        );
        assert_eq!(
            requests[0].to,
            crate::transition::StyleValue::Color(Color::rgba(0x44, 0x55, 0x66, 0xff))
        );
    }

    #[test]
    fn snapshot_restore_emits_transform_transition_when_current_style_differs() {
        let mut original = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut original_style = Style::new();
        original_style.set_transform(Transform::new([Translate::x(Length::px(10.0))]));
        original.apply_style(original_style);
        let snapshot = original.snapshot_state().expect("snapshot should exist");

        let mut rebuilt = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut rebuilt_style = Style::new();
        rebuilt_style.set_transform(Transform::new([Translate::x(Length::px(40.0))]));
        rebuilt_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::from(vec![Transition::new(
                TransitionProperty::Transform,
                180,
            )])),
        );
        rebuilt.apply_style(rebuilt_style);

        assert!(rebuilt.restore_state(snapshot.as_ref()));

        let requests = std::mem::take(&mut rebuilt.pending_style_transition_requests);
        assert_eq!(requests.len(), 1, "expected one transform transition request");
        assert_eq!(requests[0].field, crate::transition::StyleField::Transform);
        assert_eq!(
            requests[0].from,
            crate::transition::StyleValue::Transform(Transform::new([Translate::x(
                Length::px(10.0)
            )]))
        );
        assert_eq!(
            requests[0].to,
            crate::transition::StyleValue::Transform(Transform::new([Translate::x(
                Length::px(40.0)
            )]))
        );
    }

    #[test]
    fn snapshot_restore_reconciles_stale_layout_override_without_runtime_track() {
        let mut original = Element::new(0.0, 0.0, 180.0, 58.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Width,
                180,
            ))),
        );
        original.apply_style(style.clone());
        original.layout_transition_override_width = Some(34.0);
        original.layout_transition_target_width = Some(180.0);
        original.layout_assigned_width = Some(180.0);

        let snapshot = original.snapshot_state().expect("snapshot should exist");

        let mut rebuilt = Element::new(0.0, 0.0, 180.0, 58.0);
        rebuilt.apply_style(style);
        assert!(rebuilt.restore_state(snapshot.as_ref()));

        let mut roots: Vec<Box<dyn ElementTrait>> = vec![Box::new(rebuilt)];
        assert!(crate::view::base_component::reconcile_transition_runtime_state(
            &mut roots,
            &HashMap::new(),
        ));
        let rebuilt = roots[0]
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("rebuilt element");

        rebuilt.measure(LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        });
        rebuilt.place(LayoutPlacement {
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
        });

        assert_eq!(rebuilt.box_model_snapshot().width, 180.0);
        assert_eq!(rebuilt.layout_transition_override_width, None);
        assert_eq!(rebuilt.layout_transition_target_width, None);
    }

    #[test]
    fn transform_style_sample_updates_element_transform_matrix() {
        let mut el = Element::new(0.0, 0.0, 200.0, 150.0);
        let node_id = el.id();
        let transform = Transform::new([Translate::xy(Length::px(12.0), Length::px(18.0))]);

        assert!(set_style_field_by_id(
            &mut el,
            node_id,
            crate::transition::StyleField::Transform,
            crate::transition::StyleValue::Transform(transform.clone()),
        ));

        assert_eq!(el.transform, transform);
        assert!(el.resolved_transform.is_some());
    }

    #[test]
    fn snapshot_restore_emits_transform_origin_transition_when_current_style_differs() {
        let mut original = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut original_style = Style::new();
        original_style.set_transform_origin(TransformOrigin::percent(50.0, 50.0));
        original.apply_style(original_style);
        let snapshot = original.snapshot_state().expect("snapshot should exist");

        let mut rebuilt = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut rebuilt_style = Style::new();
        rebuilt_style.set_transform_origin(TransformOrigin::px(10.0, 20.0));
        rebuilt_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::from(vec![Transition::new(
                TransitionProperty::TransformOrigin,
                180,
            )])),
        );
        rebuilt.apply_style(rebuilt_style);

        assert!(rebuilt.restore_state(snapshot.as_ref()));

        let requests = std::mem::take(&mut rebuilt.pending_style_transition_requests);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].field, crate::transition::StyleField::TransformOrigin);
    }

    #[test]
    fn snapshot_restore_emits_box_shadow_transition_when_current_style_differs() {
        let mut original = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut original_style = Style::new();
        original_style.set_box_shadow(vec![BoxShadow::new().offset_x(2.0).blur(4.0)]);
        original.apply_style(original_style);
        let snapshot = original.snapshot_state().expect("snapshot should exist");

        let mut rebuilt = Element::new(0.0, 0.0, 200.0, 150.0);
        let mut rebuilt_style = Style::new();
        rebuilt_style.set_box_shadow(vec![BoxShadow::new().offset_x(12.0).blur(10.0)]);
        rebuilt_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::from(vec![Transition::new(
                TransitionProperty::BoxShadow,
                180,
            )])),
        );
        rebuilt.apply_style(rebuilt_style);

        assert!(rebuilt.restore_state(snapshot.as_ref()));

        let requests = std::mem::take(&mut rebuilt.pending_style_transition_requests);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].field, crate::transition::StyleField::BoxShadow);
    }

    #[test]
    fn box_shadow_style_sample_updates_element_shadows() {
        let mut el = Element::new(0.0, 0.0, 200.0, 150.0);
        let node_id = el.id();
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
            &mut el,
            node_id,
            crate::transition::StyleField::BoxShadow,
            crate::transition::StyleValue::BoxShadow(shadows.clone()),
        ));

        assert_eq!(el.box_shadows, shadows);
    }

    #[test]
    fn transform_origin_style_sample_updates_element_transform_matrix() {
        let mut el = Element::new(0.0, 0.0, 200.0, 150.0);
        let node_id = el.id();

        assert!(set_style_field_by_id(
            &mut el,
            node_id,
            crate::transition::StyleField::TransformOrigin,
            crate::transition::StyleValue::TransformOriginProgress {
                from: TransformOrigin::percent(50.0, 50.0),
                to: TransformOrigin::px(10.0, 20.0),
                progress: 0.5,
            },
        ));

        assert!(el.resolved_transform.is_none());
        assert!((el.transform_origin.x().resolve_without_percent_base(0.0, 0.0) - 55.0).abs() < 0.0001);
        assert!((el.transform_origin.y().resolve_without_percent_base(0.0, 0.0) - 47.5).abs() < 0.0001);
    }

    #[test]
    fn transform_transition_baseline_preserves_start_then_progress_updates_live_transform() {
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

        let node_id = el.id();
        assert!(set_style_field_by_id(
            &mut el,
            node_id,
            crate::transition::StyleField::Transform,
            crate::transition::StyleValue::TransformProgress {
                from: from.clone(),
                to: to.clone(),
                progress: 0.5,
            },
        ));

        assert_ne!(el.transform, from);
        assert_ne!(el.transform, to);
    }

    #[test]
    fn inline_layout_wraps_children_into_multiple_line_boxes() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
        parent.apply_style(parent_style);

        parent.add_child(Box::new(Element::new(0.0, 0.0, 60.0, 10.0)));
        parent.add_child(Box::new(Element::new(0.0, 0.0, 50.0, 20.0)));
        parent.add_child(Box::new(Element::new(0.0, 0.0, 40.0, 15.0)));

        parent.measure(LayoutConstraints {
            max_width: 100.0,
            max_height: 200.0,
            viewport_width: 100.0,
            viewport_height: 200.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(200.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let first = children[0].box_model_snapshot();
        let second = children[1].box_model_snapshot();
        let third = children[2].box_model_snapshot();

        assert_eq!(first.x, 0.0);
        assert_eq!(first.y, 0.0);
        assert_eq!(second.x, 0.0);
        assert_eq!(second.y, 10.0);
        assert_eq!(third.x, 50.0);
        assert_eq!(third.y, 10.0);
        assert!((parent.box_model_snapshot().height - 30.0).abs() < 0.01);
        assert!((parent.content_size.height - 30.0).abs() < 0.01);
    }

    #[test]
    fn inline_layout_keeps_trailing_text_on_same_line_after_inline_element() {
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);

        parent.add_child(Box::new(Text::from_content("lead")));
        parent.add_child(Box::new(Element::new(0.0, 0.0, 50.0, 20.0)));
        parent.add_child(Box::new(Text::from_content(" trailing text continues after the badge.")));

        parent.measure(LayoutConstraints {
            max_width: 220.0,
            max_height: 200.0,
            viewport_width: 220.0,
            viewport_height: 200.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(200.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let children = parent.children().expect("children");
        let badge = children[1].box_model_snapshot();
        let trailing = children[2]
            .as_any()
            .downcast_ref::<Text>()
            .expect("text child");
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
        let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        parent.apply_style(parent_style);

        let mut text = Text::from_content("fragmented text should wrap across multiple inline lines");
        text.set_size(300.0, 300.0);
        parent.add_child(Box::new(text));

        parent.measure(LayoutConstraints {
            max_width: 160.0,
            max_height: 240.0,
            viewport_width: 160.0,
            viewport_height: 240.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(240.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let text = parent.children().expect("children")[0]
            .as_any()
            .downcast_ref::<Text>()
            .expect("text child");
        let fragments = text.inline_fragment_positions();
        let snapshot = text.box_model_snapshot();

        assert!(fragments.len() > 1);
        assert!(snapshot.width < 300.0);
        assert!(snapshot.height < 300.0);
    }

    #[test]
    fn inline_gap_does_not_apply_between_text_fragments_of_same_text_node() {
        let mut parent = Element::new(0.0, 0.0, 120.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(24.0)));
        parent.apply_style(parent_style);

        let mut text = Text::from_content("alpha beta gamma");
        text.set_size(300.0, 80.0);
        parent.add_child(Box::new(text));

        parent.measure(LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let text = parent.children().expect("children")[0]
            .as_any()
            .downcast_ref::<Text>()
            .expect("text child");
        let fragments = text.inline_fragment_positions();
        assert!(fragments.len() >= 2);
        assert!(fragments[1].1.x < 120.0);
    }

    #[test]
    fn inline_cjk_text_fragments_follow_wrapped_lines() {
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);

        parent.add_child(Box::new(Text::from_content("最後接一段中文，確認混排時也能一起換行。")));

        parent.measure(LayoutConstraints {
            max_width: 220.0,
            max_height: 120.0,
            viewport_width: 220.0,
            viewport_height: 120.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let text = parent.children().expect("children")[0]
            .as_any()
            .downcast_ref::<Text>()
            .expect("text child");
        let fragments = text.inline_fragment_positions();

        assert!(fragments.len() > 1);
        assert!(fragments[0].0.starts_with("最後"));
        assert!(fragments.iter().all(|(_, position)| position.x >= 0.0));
    }

    #[test]
    fn inline_auto_sized_element_expands_into_child_fragments() {
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        wrapper.add_child(Box::new(Text::from_content("nested")));
        wrapper.add_child(Box::new(Element::new(0.0, 0.0, 44.0, 20.0)));

        parent.add_child(Box::new(wrapper));
        parent.add_child(Box::new(Text::from_content("tail")));

        parent.measure(LayoutConstraints {
            max_width: 220.0,
            max_height: 120.0,
            viewport_width: 220.0,
            viewport_height: 120.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let wrapper = parent.children().expect("children")[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("wrapper element");
        let tail = parent.children().expect("children")[1]
            .as_any()
            .downcast_ref::<Text>()
            .expect("tail text");
        let wrapper_children = wrapper.children().expect("wrapper children");

        assert!(wrapper.box_model_snapshot().width > 44.0);
        assert_eq!(wrapper_children[1].box_model_snapshot().y, 0.0);
        assert!(tail.box_model_snapshot().x >= wrapper.box_model_snapshot().x + 44.0);
    }

    #[test]
    fn inline_fragmentable_element_builds_multiple_draw_rect_passes() {
        let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        parent.apply_style(parent_style);

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
        wrapper.add_child(Box::new(Text::from_content(
            "inline wrapper background should wrap across lines",
        )));
        parent.add_child(Box::new(wrapper));

        parent.measure(LayoutConstraints {
            max_width: 160.0,
            max_height: 160.0,
            viewport_width: 160.0,
            viewport_height: 160.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(160.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 160, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let next_state = parent.build(
            &mut graph,
            UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone()),
        );
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
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
        parent.apply_style(parent_style);
        parent.add_child(Box::new(Text::from_content("Inline text starts here,")));

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
        wrapper.add_child(Box::new(Text::from_content(
            "badge test test test test test test test",
        )));
        parent.add_child(Box::new(wrapper));
        parent.add_child(Box::new(Text::from_content(
            "then more text continues after the badge,",
        )));

        parent.measure(LayoutConstraints {
            max_width: 220.0,
            max_height: 200.0,
            viewport_width: 220.0,
            viewport_height: 200.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(200.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let wrapper = parent.children().expect("children")[1]
            .as_any()
            .downcast_ref::<Element>()
            .expect("wrapper");
        let nested_text = wrapper.children().expect("wrapper children")[0]
            .as_any()
            .downcast_ref::<Text>()
            .expect("nested text");
        let fragments = nested_text.inline_fragment_positions();
        assert!(fragments.len() >= 2, "fragments={fragments:?}");
    }

    #[test]
    fn inline_fragmentable_element_uses_slice_padding_across_fragments() {
        let mut parent = Element::new(0.0, 0.0, 160.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        parent.apply_style(parent_style);

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.set_padding(crate::Padding::uniform(Length::px(8.0)));
        wrapper.apply_style(wrapper_style);
        wrapper.add_child(Box::new(Text::from_content(
            "badge test test test test",
        )));
        parent.add_child(Box::new(wrapper));

        parent.measure(LayoutConstraints {
            max_width: 160.0,
            max_height: 160.0,
            viewport_width: 160.0,
            viewport_height: 160.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(160.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let wrapper = parent.children().expect("children")[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("wrapper");
        let nested_text = wrapper.children().expect("wrapper children")[0]
            .as_any()
            .downcast_ref::<Text>()
            .expect("nested text");
        assert!(wrapper.inline_paint_fragments.len() >= 2);
        let first = wrapper.inline_paint_fragments[0];
        let last = *wrapper.inline_paint_fragments.last().expect("last fragment");
        let fragments = nested_text.inline_fragment_positions();
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
                });
                let (width, _) = text.measured_size();
                position.x + width
            })
            .fold(0.0_f32, f32::max);
        assert!((last.x + last.width - last_line_right - 8.0).abs() < 0.5);
    }

    #[test]
    fn inline_fragmentable_wrapper_respects_remaining_width_on_first_line() {
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(parent_style);

        let mut badge = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut badge_style = Style::new();
        badge_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
        badge_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        badge.apply_style(badge_style);
        parent.add_child(Box::new(badge));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        wrapper.add_child(Box::new(Text::from_content("alpha beta gamma delta")));
        parent.add_child(Box::new(wrapper));

        parent.measure(LayoutConstraints {
            max_width: 220.0,
            max_height: 200.0,
            viewport_width: 220.0,
            viewport_height: 200.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(200.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let wrapper = parent.children().expect("children")[1]
            .as_any()
            .downcast_ref::<Element>()
            .expect("wrapper");
        let text = wrapper.children().expect("wrapper children")[0]
            .as_any()
            .downcast_ref::<Text>()
            .expect("text child");
        let fragments = text.inline_fragment_positions();
        let first_fragment = fragments.first().expect("first fragment");

        assert_eq!(first_fragment.1.y, 0.0, "fragments={fragments:?}");
        assert!(first_fragment.1.x >= 140.0, "fragments={fragments:?}");
    }

    #[test]
    fn inline_fragmentable_element_vertical_padding_does_not_shift_inline_content_y() {
        let mut parent = Element::new(0.0, 0.0, 280.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(280.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
        parent.apply_style(parent_style);

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
        wrapper.add_child(Box::new(Text::from_content("badge")));
        parent.add_child(Box::new(wrapper));
        parent.add_child(Box::new(Text::from_content("trailing")));

        parent.measure(LayoutConstraints {
            max_width: 280.0,
            max_height: 120.0,
            viewport_width: 280.0,
            viewport_height: 120.0,
            percent_base_width: Some(280.0),
            percent_base_height: Some(120.0),
        });
        parent.place(LayoutPlacement {
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
        });

        let wrapper = parent.children().expect("children")[0]
            .as_any()
            .downcast_ref::<Element>()
            .expect("wrapper");
        let nested_text = wrapper.children().expect("wrapper children")[0]
            .as_any()
            .downcast_ref::<Text>()
            .expect("nested text");
        let trailing = parent.children().expect("children")[1]
            .as_any()
            .downcast_ref::<Text>()
            .expect("trailing text");

        let badge_y = nested_text.inline_fragment_positions()[0].1.y;
        let trailing_y = trailing.inline_fragment_positions()[0].1.y;
        let paint_top = wrapper.inline_paint_fragments[0].y;
        let (_, text_height) = nested_text.measured_size();
        let paint_height = wrapper.inline_paint_fragments[0].height;
        assert!((badge_y - trailing_y).abs() < 0.5);
        assert!((badge_y - paint_top - 12.0).abs() < 0.5);
        assert!((paint_height - (text_height + 24.0)).abs() < 0.5);
    }

    #[test]
    fn inline_fragmentable_element_positions_all_nested_text_fragments_across_widths() {
        for width in 140..=240 {
            let width = width as f32;
            let mut parent = Element::new(0.0, 0.0, width, 0.0);
            let mut parent_style = Style::new();
            parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
            parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
            parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
            parent.apply_style(parent_style);
            parent.add_child(Box::new(Text::from_content("Inline text starts here,")));

            let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
            let mut wrapper_style = Style::new();
            wrapper_style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(Color::hex("#93c5fd")));
            wrapper_style.set_padding(crate::Padding::uniform(Length::px(8.0)));
            wrapper.apply_style(wrapper_style);
            wrapper.add_child(Box::new(Text::from_content(
                "badge test test test test test test test",
            )));
            parent.add_child(Box::new(wrapper));
            parent.add_child(Box::new(Text::from_content(
                "then more text continues after the badge,",
            )));

            parent.measure(LayoutConstraints {
                max_width: width,
                max_height: 240.0,
                viewport_width: width,
                viewport_height: 240.0,
                percent_base_width: Some(width),
                percent_base_height: Some(240.0),
            });
            parent.place(LayoutPlacement {
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
            });

            let wrapper = parent.children().expect("children")[1]
                .as_any()
                .downcast_ref::<Element>()
                .expect("wrapper");
            let nested_text = wrapper.children().expect("wrapper children")[0]
                .as_any()
                .downcast_ref::<Text>()
                .expect("nested text");
            let expected = nested_text.get_inline_nodes_size().len();
            let actual = nested_text.inline_fragment_positions().len();
            assert_eq!(actual, expected, "width={width}, actual={actual}, expected={expected}, fragments={:?}", nested_text.inline_fragment_positions());
        }
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

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
        );
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);

        wrapper.add_child(Box::new(Text::from_content("a")));
        wrapper.measure(constraints);
        let (before_width, _) = wrapper.measured_size();

        let child = wrapper.children_mut().expect("children")[0]
            .as_any_mut()
            .downcast_mut::<Text>()
            .expect("text child");
        child.set_text("a much longer child");

        wrapper.measure(constraints);
        let (after_width, _) = wrapper.measured_size();
        assert!(after_width > before_width + 1.0);
    }

    #[test]
    fn inline_fragmentable_element_does_not_overlap_trailing_text_across_widths() {
        for width in 140..=240 {
            let width = width as f32;
            let mut parent = Element::new(0.0, 0.0, width, 0.0);
            let mut parent_style = Style::new();
            parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
            parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
            parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
            parent.apply_style(parent_style);
            parent.add_child(Box::new(Text::from_content("Inline text starts here,")));

            let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
            let mut wrapper_style = Style::new();
            wrapper_style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(Color::hex("#93c5fd")));
            wrapper_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#ffffff")));
            wrapper_style.set_padding(crate::Padding::uniform(Length::px(8.0)));
            wrapper.apply_style(wrapper_style);
            wrapper.add_child(Box::new(Text::from_content(
                "badge test test test test test test test",
            )));
            parent.add_child(Box::new(wrapper));
            parent.add_child(Box::new(Text::from_content(
                "then more text continues after the badge,",
            )));

            parent.measure(LayoutConstraints {
                max_width: width,
                max_height: 240.0,
                viewport_width: width,
                viewport_height: 240.0,
                percent_base_width: Some(width),
                percent_base_height: Some(240.0),
            });
            parent.place(LayoutPlacement {
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
            });

            let wrapper = parent.children().expect("children")[1]
                .as_any()
                .downcast_ref::<Element>()
                .expect("wrapper");
            let nested_text = wrapper.children().expect("wrapper children")[0]
                .as_any()
                .downcast_ref::<Text>()
                .expect("nested text");
            let trailing = parent.children().expect("children")[2]
                .as_any()
                .downcast_ref::<Text>()
                .expect("trailing text");

            let nested_fragments = nested_text.inline_fragment_positions();
            let trailing_fragments = trailing.inline_fragment_positions();
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
                        });
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

}
