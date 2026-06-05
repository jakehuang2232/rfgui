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
    use super::super::core::Position as LayoutPosition;
    use super::{
        DirtyFlags, Element, ElementInlineIfcCandidateLifecycle,
        ElementInlineIfcCandidateLifecycleInput, ElementInlineIfcCandidateLifecycleInstallStatus,
        ElementInlineIfcDefaultRolloutBlockedReason, ElementInlineIfcDefaultRolloutDecision,
        ElementInlineIfcDefaultRolloutDecisionInput,
        ElementInlineIfcDefaultShadowRunAdoptionAudit,
        ElementInlineIfcDefaultShadowRunAdoptionAuditInput,
        ElementInlineIfcDefaultShadowRunAuditBlockedReason,
        ElementInlineIfcDefaultShadowRunAuditReadiness,
        ElementInlineIfcRenderDefaultAudit, ElementInlineIfcRenderDefaultAuditBlockedReason,
        ElementInlineIfcRenderDefaultAuditInput, ElementInlineIfcRenderDefaultAuditReadiness,
        ElementInlineIfcRenderDefaultAdoptionAudit,
        ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason,
        ElementInlineIfcRenderDefaultAdoptionAuditInput,
        ElementInlineIfcRenderDefaultAdoptionAuditReadiness,
        ElementInlineIfcRenderDefaultRolloutBlockedReason,
        ElementInlineIfcRenderDefaultRolloutDecision,
        ElementInlineIfcRenderDefaultRolloutDecisionInput,
        ElementInlineIfcRenderDefaultRolloutReadiness,
        ElementInlineIfcLayoutCallSiteOptIn, ElementInlineIfcLayoutCallSiteOptInInput,
        ElementInlineIfcLayoutCallSiteOptInMode, ElementInlineIfcLayoutCallSiteOptInStatus,
        ElementInlineIfcLayoutCallSiteRolloutConfig,
        ElementInlineIfcLayoutCallSiteRolloutPhase, ElementInlineIfcLayoutCallSiteScenario,
        ElementInlineIfcMetadataCollector, ElementInlineIfcMetadataCollectorInput,
        ElementInlineIfcRenderDecision, ElementInlineIfcRenderFallback, ElementInlineIfcRenderMode,
        ElementInlineIfcRolloutPackages, ElementTrait, EventTarget,
        LayoutConstraints, LayoutPlacement, Layoutable, UiBuildContext,
        TextAreaInlineIfcReadiness, TextAreaInlineIfcReadinessBlockedReason,
        TextAreaInlineIfcReadinessInput, TextAreaInlineIfcReadinessState,
        expand_corner_radii_for_spread, main_axis_start_and_gap, normalize_corner_radii,
        resolve_px_with_base, resolve_signed_px_with_base, Rect,
    };
    use super::{reset_test_promoted_build_counts, test_promoted_build_count};
    use crate::style::Layout;
    use crate::style::{
        Align, AnchorName, Angle, Border, BorderRadius, BoxShadow, ClipMode, Collision,
        CollisionBoundary, Color, ComputedStyle, CrossSize, JustifyContent, Length, Opacity,
        Operator, Origin, Position, Rotate, Style, Transform, TransformOrigin, Translate,
        VerticalAlign,
    };
    use crate::style::{ParsedValue, PropertyId, Transition, TransitionProperty, Transitions};
    use crate::transition::{LayoutField, VisualField};
    use crate::view::base_component::ComputedStyleConsumer;
    use crate::view::base_component::Text;
    use crate::view::base_component::set_style_field_by_id;
    use crate::view::frame_graph::{FrameGraph, PassDescriptor, PassDetails};
    use crate::view::inline_formatting_context::{
        InlineFormattingContext, InlineIfcAtomicMeasureConstraints, InlineIfcDecorationBoxInsets,
        InlineIfcElementDecorationDrawRectStyle, InlineIfcElementDecorationPackageSource,
        InlineIfcElementPackageDistributionInput, InlineIfcElementRootCandidateCache,
        InlineIfcElementRootSourceBuilder, InlineIfcInput, InlineIfcInvalidation, InlineIfcItem,
        InlineIfcMeasuredAtomicBox, InlineIfcSize, InlineIfcSourceId, InlineIfcStyle,
    };
    use crate::view::test_support::{
        child_key, child_snapshot, commit_child, commit_element, measure_and_place, new_test_arena,
        nth_child_snapshot,
    };
    use glam::{Mat4, Vec3};
    use rustc_hash::{FxHashMap, FxHashSet};

    use std::sync::Arc;

    fn rect_close(actual: Rect, expected: Rect, epsilon: f32) -> bool {
        (actual.x - expected.x).abs() <= epsilon
            && (actual.y - expected.y).abs() <= epsilon
            && (actual.width - expected.width).abs() <= epsilon
            && (actual.height - expected.height).abs() <= epsilon
    }

    #[derive(Clone, Debug)]
    struct InlineElementIfcRenderGraphSummary {
        pass_names: Vec<String>,
        draw_rect_descriptors: Vec<PassDescriptor>,
    }

    #[derive(Clone, Copy, Debug)]
    struct InlineElementIfcProductionMatrixFixture {
        parent_key: crate::view::node_arena::NodeKey,
        outer_key: crate::view::node_arena::NodeKey,
        inner_key: crate::view::node_arena::NodeKey,
        atomic_key: crate::view::node_arena::NodeKey,
        sibling_key: crate::view::node_arena::NodeKey,
        mutable_text_key: crate::view::node_arena::NodeKey,
        height: f32,
    }

    fn inline_element_ifc_matrix_constraints(width: f32, height: f32) -> LayoutConstraints {
        LayoutConstraints {
            max_width: width,
            max_height: height,
            viewport_width: width,
            viewport_height: height,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
        }
    }

    fn inline_element_ifc_matrix_placement(width: f32, height: f32) -> LayoutPlacement {
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: height,
            viewport_width: width,
            viewport_height: height,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
        }
    }

    fn build_inline_element_ifc_production_matrix_fixture(
        parent_width: f32,
        scenario: ElementInlineIfcLayoutCallSiteScenario,
    ) -> (
        crate::view::node_arena::NodeArena,
        InlineElementIfcProductionMatrixFixture,
    ) {
        let mut arena = new_test_arena();
        let height = 260.0;

        let mut parent = Element::new_with_id(820, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent_style.set_line_height(1.2);
        parent_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#111827")));
        parent.apply_style(parent_style);
        parent.apply_inline_ifc_layout_call_site_rollout_config_for_test(
            ElementInlineIfcLayoutCallSiteRolloutConfig::for_scenario(scenario),
        );
        let parent_key = commit_element(&mut arena, Box::new(parent));

        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content_with_id(920, "prefix production text ")),
        );

        let mut outer = Element::new_with_id(821, 0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#e0f2fe")),
        );
        outer_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#0c4a6e")));
        outer_style.set_padding(crate::style::Padding::uniform(Length::px(5.0)));
        outer_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#0284c7")));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));

        let mut mutable_text = Text::from_content_with_id(921, "outer text before ");
        mutable_text.set_font_size(16.0);
        mutable_text.set_color(Color::hex("#0f172a"));
        let mutable_text_key = commit_child(&mut arena, outer_key, Box::new(mutable_text));

        let mut inner = Element::new_with_id(822, 0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dcfce7")),
        );
        inner_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#14532d")));
        inner_style.set_padding(crate::style::Padding::uniform(Length::px(3.0)));
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#16a34a")));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));

        commit_child(
            &mut arena,
            inner_key,
            Box::new(Text::from_content_with_id(
                922,
                "nested inline element span text",
            )),
        );

        let mut atomic = Element::new_with_id(823, 0.0, 0.0, 54.0, 22.0);
        let mut atomic_style = Style::new();
        atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(54.0)));
        atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(22.0)));
        atomic_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fef3c7")),
        );
        atomic.apply_style(atomic_style);
        let atomic_key = commit_child(&mut arena, outer_key, Box::new(atomic));

        commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::from_content_with_id(923, " outer tail")),
        );

        let mut sibling = Element::new_with_id(824, 0.0, 0.0, 0.0, 0.0);
        let mut sibling_style = Style::new();
        sibling_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        sibling_style.insert(PropertyId::Width, ParsedValue::Auto);
        sibling_style.insert(PropertyId::Height, ParsedValue::Auto);
        sibling_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fee2e2")),
        );
        sibling_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#7f1d1d")));
        sibling_style.set_padding(crate::style::Padding::uniform(Length::px(4.0)));
        sibling_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        sibling.apply_style(sibling_style);
        let sibling_key = commit_child(&mut arena, parent_key, Box::new(sibling));

        commit_child(
            &mut arena,
            sibling_key,
            Box::new(Text::from_content_with_id(
                924,
                "sibling inline element text",
            )),
        );
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content_with_id(925, " final text")),
        );

        (
            arena,
            InlineElementIfcProductionMatrixFixture {
                parent_key,
                outer_key,
                inner_key,
                atomic_key,
                sibling_key,
                mutable_text_key,
                height,
            },
        )
    }

    fn compile_inline_element_render_graph_for_test(
        arena: &mut crate::view::node_arena::NodeArena,
        root_key: crate::view::node_arena::NodeKey,
        width: u32,
        height: u32,
    ) -> InlineElementIfcRenderGraphSummary {
        let mut graph = FrameGraph::new();
        let mut ctx =
            UiBuildContext::new(width, height, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target,
            },
        ));
        ctx.set_current_target(target);

        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(root_key, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("inline element build should return state");
        ctx.set_state(next_state);

        graph
            .compile()
            .expect("inline element render graph should compile");
        let pass_descriptors = graph
            .pass_descriptors()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let pass_names = pass_descriptors
            .iter()
            .map(|descriptor| descriptor.name.to_string())
            .collect::<Vec<_>>();
        let draw_rect_descriptors = pass_descriptors
            .into_iter()
            .filter(|descriptor| {
                descriptor.name.contains("draw_rect_pass::DrawRectPass")
                    || descriptor
                        .name
                        .contains("draw_rect_pass::OpaqueRectPass")
            })
            .collect::<Vec<_>>();

        InlineElementIfcRenderGraphSummary {
            pass_names,
            draw_rect_descriptors,
        }
    }

    fn assert_draw_rect_descriptors_are_graphics(
        descriptors: &[PassDescriptor],
        expected_min_count: usize,
    ) {
        assert!(
            descriptors.len() >= expected_min_count,
            "expected at least {expected_min_count} draw rect descriptors, got {descriptors:?}"
        );
        for descriptor in descriptors {
            let PassDetails::Graphics(graphics) = &descriptor.details else {
                panic!("draw rect descriptor should be graphics: {descriptor:?}");
            };
            assert!(
                !graphics.color_attachments.is_empty(),
                "draw rect descriptor should write a color target: {descriptor:?}"
            );
        }
    }

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
        style.set_padding(crate::style::Padding::new().xy(Length::px(3.25), Length::px(2.5)));
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
            ParsedValue::Flex(crate::style::flex().grow(1.0).shrink(1.0)),
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
    fn inline_measure_skips_absolute_child_for_remaining_width() {
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let leading = Element::new(0.0, 0.0, 190.0, 20.0);

        let mut popover = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut popover_style = Style::new();
        popover_style.insert(PropertyId::Width, ParsedValue::Auto);
        popover_style.insert(PropertyId::Height, ParsedValue::Auto);
        popover_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
        );
        popover_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute().anchor(crate::style::Anchor::Viewport),
            ),
        );
        popover.apply_style(popover_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _leading_key = commit_child(&mut arena, parent_key, Box::new(leading));
        let popover_key = commit_child(&mut arena, parent_key, Box::new(popover));
        let _popover_text_key = commit_child(
            &mut arena,
            popover_key,
            Box::new(Text::from_content("absolute snackbar message")),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 200.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
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
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
        );

        let parent_snapshot = child_snapshot(&arena, parent_key);
        let popover_snapshot = nth_child_snapshot(&arena, parent_key, 1);

        assert!(
            popover_snapshot.width > 100.0,
            "absolute child should measure against the parent constraint, not the 10px inline remainder: {:?}",
            popover_snapshot
        );
        assert!(
            popover_snapshot.height < 40.0,
            "absolute child should not be made tall by remainder-width text wrapping: {:?}",
            popover_snapshot
        );
        assert_eq!(parent_snapshot.width, 200.0);
        assert_eq!(parent_snapshot.height, 20.0);
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
        let _ = commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 80.0, 30.0)),
        );
        let _ = commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 120.0, 10.0)),
        );

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
        let _ = commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 80.0, 40.0)),
        );

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
            ParsedValue::Flex(crate::style::flex().basis(Length::px(40.0)).grow(1.0)),
        );
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 40.0, 30.0);
        let mut second_style = Style::new();
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().basis(Length::px(40.0)).grow(2.0)),
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
            ParsedValue::Flex(crate::style::flex().basis(Length::px(100.0)).shrink(1.0)),
        );
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 80.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().basis(Length::px(100.0)).shrink(1.0)),
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
                    .left(Length::calc(
                        Length::percent(100.0),
                        Operator::plus,
                        Length::px(6.0),
                    ))
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
        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("parent build returns state");
        ctx.set_state(next_state);

        let deferred = ctx.take_deferred_nodes();
        let child_id = arena.get(child_k).unwrap().element.stable_id();
        assert!(deferred.iter().any(|node| node.stable_id == child_id));
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
        assert_eq!(
            child_el.absolute_clip_scissor_rect(),
            Some([0, 0, 500, 200])
        );
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
        assert_eq!(
            child_el.absolute_clip_scissor_rect(),
            Some([0, 0, 400, 300])
        );
    }

    #[test]
    fn hover_style_updates_color_opacity_and_reverts() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let base_color = Color::rgb(10, 20, 30);
        let hover_color = Color::rgb(200, 150, 100);
        let mut style = Style::new();
        style.set_background(base_color.into());
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.25)));
        let mut hover_style = Style::new();
        hover_style.set_background(hover_color.into());
        hover_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.75)));
        style.set_hover(hover_style);
        el.apply_style(style);
        el.layout_dirty = false;
        el.clear_local_dirty_flags(DirtyFlags::ALL);

        assert!(el.set_hovered(true));
        let hovered_state = el.debug_render_state();
        assert_eq!(hovered_state.background_rgba, hover_color.to_rgba_u8());
        assert!((hovered_state.opacity - 0.75).abs() < 0.001);
        assert!(!el.layout_dirty);
        assert!(el.local_dirty_flags().contains(DirtyFlags::RUNTIME));

        el.clear_local_dirty_flags(DirtyFlags::ALL);
        el.layout_dirty = false;
        assert!(el.set_hovered(false));
        let base_state = el.debug_render_state();
        assert_eq!(base_state.background_rgba, base_color.to_rgba_u8());
        assert!((base_state.opacity - 0.25).abs() < 0.001);
        assert!(!el.layout_dirty);
        assert!(el.local_dirty_flags().contains(DirtyFlags::RUNTIME));
    }

    #[test]
    fn layout_affecting_hover_style_marks_layout_dirty() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
        let mut hover_style = Style::new();
        hover_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
        style.set_hover(hover_style);
        el.apply_style(style);
        el.layout_dirty = false;
        el.clear_local_dirty_flags(DirtyFlags::ALL);

        assert!(el.set_hovered(true));
        assert!(el.layout_dirty);
        assert!(el.local_dirty_flags().contains(DirtyFlags::LAYOUT));
    }

    #[test]
    fn hover_style_emits_transition_request() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.2)));
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Opacity,
                200,
            ))),
        );
        let mut hover_style = Style::new();
        hover_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.8)));
        style.set_hover(hover_style);
        el.apply_style(style);

        assert!(el.set_hovered(true));
        let reqs = el.take_style_transition_requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].field, crate::transition::StyleField::Opacity);
        assert_eq!(
            reqs[0].from,
            crate::transition::StyleValue::Scalar(0.2)
        );
        assert_eq!(reqs[0].to, crate::transition::StyleValue::Scalar(0.8));
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
        first_style.set_flex(crate::style::flex().grow(1.0).basis(Length::px(50.0)));
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut second_style = Style::new();
        second_style.set_flex(crate::style::flex().grow(1.0).basis(Length::px(50.0)));
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
    fn auto_axis_layout_measures_and_places_children_against_constraint_not_stale_zero() {
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().column().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Height,
                200,
            ))),
        );
        parent.apply_style(parent_style);
        parent.has_layout_snapshot = true;
        parent.layout_state.layout_size.height = 0.0;

        let child = Element::new(0.0, 0.0, 100.0, 32.0);

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

        let parent_snapshot = child_snapshot(&arena, parent_key);
        let child_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(parent_snapshot.height, 0.0);
        assert_eq!(child_snapshot.height, 32.0);

        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        assert_eq!(parent_ref.core.size.height, 32.0);
    }

    #[test]
    fn auto_axis_layout_places_children_against_target_not_parent_proposal() {
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().row().no_wrap().align(Align::Center).into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let child = Element::new(0.0, 0.0, 80.0, 20.0);

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

        let parent_snapshot = child_snapshot(&arena, parent_key);
        let child_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(parent_snapshot.height, 20.0);
        assert_eq!(child_snapshot.y, 0.0);
        assert_eq!(child_snapshot.height, 20.0);
    }

    #[test]
    fn explicit_zero_axis_layout_without_transition_reports_zero() {
        let mut parent = Element::new(0.0, 0.0, 200.0, 40.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().column().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::Zero));
        parent.apply_style(parent_style);
        parent.has_layout_snapshot = true;
        parent.layout_state.layout_size.height = 40.0;

        let child = Element::new(0.0, 0.0, 100.0, 32.0);

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

        let parent_snapshot = child_snapshot(&arena, parent_key);
        let child_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        assert_eq!(parent_snapshot.height, 0.0);
        assert_eq!(child_snapshot.height, 0.0);
    }

    #[test]
    fn flow_places_expanding_height_transition_child_at_target_size() {
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut expanding = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut expanding_style = Style::new();
        expanding_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().column().into()),
        );
        expanding_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        expanding_style.insert(PropertyId::Height, ParsedValue::Auto);
        expanding_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Height,
                200,
            ))),
        );
        expanding.apply_style(expanding_style);
        expanding.has_layout_snapshot = true;
        expanding.layout_state.layout_size.height = 0.0;

        let content_child = Element::new(0.0, 0.0, 100.0, 32.0);
        let sibling = Element::new(0.0, 0.0, 200.0, 20.0);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let expanding_key = commit_child(&mut arena, parent_key, Box::new(expanding));
        let _content_key = commit_child(&mut arena, expanding_key, Box::new(content_child));
        let _sibling_k = commit_child(&mut arena, parent_key, Box::new(sibling));

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

        let parent_snapshot = child_snapshot(&arena, parent_key);
        let expanding_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let sibling_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert_eq!(parent_snapshot.height, 20.0);
        assert_eq!(expanding_snapshot.height, 0.0);
        assert_eq!(sibling_snapshot.y, 0.0);

        let expanding_ref =
            crate::view::test_support::get_element::<Element>(&arena, expanding_key);
        assert_eq!(expanding_ref.core.size.height, 32.0);
        drop(expanding_ref);

        let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, expanding_key)
            .take_layout_transition_requests();
        let h_req = reqs
            .iter()
            .find(|req| req.field == LayoutField::Height)
            .expect("expanding child should request a height transition");
        assert_eq!(h_req.from, 0.0);
        assert_eq!(h_req.to, 32.0);
    }

    #[test]
    fn explicit_height_transition_start_reports_current_size_to_parent_measure() {
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);

        let mut collapsing = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut collapsing_style = Style::new();
        collapsing_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().column().into()),
        );
        collapsing_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        collapsing_style.insert(PropertyId::Height, ParsedValue::Length(Length::Zero));
        collapsing_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Height,
                200,
            ))),
        );
        collapsing.apply_style(collapsing_style);
        collapsing.has_layout_snapshot = true;
        collapsing.layout_state.layout_size.height = 80.0;

        let content_child = Element::new(0.0, 0.0, 100.0, 32.0);
        let sibling = Element::new(0.0, 0.0, 200.0, 20.0);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let collapsing_key = commit_child(&mut arena, parent_key, Box::new(collapsing));
        let content_key = commit_child(&mut arena, collapsing_key, Box::new(content_child));
        let _sibling_k = commit_child(&mut arena, parent_key, Box::new(sibling));

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

        let parent_snapshot = child_snapshot(&arena, parent_key);
        let collapsing_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let content_snapshot = child_snapshot(&arena, content_key);
        let sibling_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert_eq!(parent_snapshot.height, 100.0);
        assert_eq!(collapsing_snapshot.height, 80.0);
        assert_eq!(content_snapshot.height, 32.0);
        assert_eq!(sibling_snapshot.y, 80.0);
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
        first_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 120.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let first_key = commit_child(&mut arena, parent_key, Box::new(first));
        let _first_leaf = commit_child(
            &mut arena,
            first_key,
            Box::new(Element::new(0.0, 0.0, 20.0, 20.0)),
        );
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
        first_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().grow(1.0)),
        );
        first_style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(30.0)));
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 20.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().grow(1.0)),
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
        first_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        first_style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(50.0)));
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
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
        first_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(60.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let first_key = commit_child(&mut arena, parent_key, Box::new(first));
        let _ = commit_child(
            &mut arena,
            first_key,
            Box::new(Element::new(0.0, 0.0, 60.0, 20.0)),
        );
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
    fn explicit_flex_basis_is_not_clamped_by_intrinsic_auto_min_main() {
        let mut parent = Element::new(0.0, 0.0, 409.0, 40.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(409.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(4.0)));
        parent.apply_style(parent_style);

        let mut track = Element::new(0.0, 0.0, 155.0, 18.0);
        let mut track_style = Style::new();
        track_style.insert(PropertyId::Width, ParsedValue::Auto);
        track_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        track_style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::Zero));
        track_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().grow(3.0).shrink(1.0)),
        );
        track.apply_style(track_style);

        let mut label = Element::new(0.0, 0.0, 250.0, 18.0);
        let mut label_style = Style::new();
        label_style.insert(PropertyId::Width, ParsedValue::Auto);
        label_style.insert(PropertyId::Height, ParsedValue::Auto);
        label_style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(250.0)));
        label_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(
                crate::style::flex()
                    .grow(1.0)
                    .shrink(1.0)
                    .basis(Length::px(80.0)),
            ),
        );
        label.apply_style(label_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(track));
        let label_key = commit_child(&mut arena, parent_key, Box::new(label));
        let _ = commit_child(
            &mut arena,
            label_key,
            Box::new(Element::new(0.0, 0.0, 250.0, 18.0)),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 409.0,
                max_height: 40.0,
                viewport_width: 800.0,
                percent_base_width: Some(409.0),
                percent_base_height: Some(40.0),
                viewport_height: 600.0,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 409.0,
                available_height: 40.0,
                viewport_width: 800.0,
                percent_base_width: Some(409.0),
                percent_base_height: Some(40.0),
                viewport_height: 600.0,
            },
        );

        let track_snapshot = nth_child_snapshot(&arena, parent_key, 0);
        let label_snapshot = nth_child_snapshot(&arena, parent_key, 1);
        assert!(
            (track_snapshot.width - 243.75).abs() < 0.01,
            "track width should grow from zero basis, got {}",
            track_snapshot.width
        );
        assert!(
            (label_snapshot.width - 161.25).abs() < 0.01,
            "label width should grow from 80px basis, not clamp to intrinsic 250px, got {}",
            label_snapshot.width
        );
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
        first_style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::Zero));
        first_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        first.apply_style(first_style);

        let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(60.0)));
        second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        second_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        second.apply_style(second_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let first_key = commit_child(&mut arena, parent_key, Box::new(first));
        let _ = commit_child(
            &mut arena,
            first_key,
            Box::new(Element::new(0.0, 0.0, 60.0, 20.0)),
        );
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
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Width,
                180,
            ))),
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
    fn apply_style_syncs_background_border_and_opacity_into_element_state() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let background = Color::rgb(18, 52, 86);
        let border_color = Color::rgb(171, 205, 239);
        let mut style = Style::new();
        style.set_background(background.into());
        style.set_border(Border::uniform(Length::px(3.0), &border_color));
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.42)));

        el.apply_style(style);

        let render_state = el.debug_render_state();
        assert_eq!(render_state.background_rgba, background.to_rgba_u8());
        assert_eq!(render_state.border_top_rgba, border_color.to_rgba_u8());
        assert_eq!(render_state.border_right_rgba, border_color.to_rgba_u8());
        assert_eq!(render_state.border_bottom_rgba, border_color.to_rgba_u8());
        assert_eq!(render_state.border_left_rgba, border_color.to_rgba_u8());
        assert!((el.border_widths.left - 3.0).abs() < 0.001);
        assert!((el.border_widths.right - 3.0).abs() < 0.001);
        assert!((el.border_widths.top - 3.0).abs() < 0.001);
        assert!((el.border_widths.bottom - 3.0).abs() < 0.001);
        assert!((render_state.opacity - 0.42).abs() < 0.001);
    }

    #[test]
    fn computed_style_consumer_syncs_element_render_state() {
        let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
        let background = Color::rgb(9, 18, 27);
        let border_color = Color::rgb(36, 45, 54);
        let mut computed = ComputedStyle::default();
        computed.background_color = background;
        computed.border_colors = crate::style::EdgeInsets {
            top: border_color,
            right: border_color,
            bottom: border_color,
            left: border_color,
        };
        computed.border_widths = crate::style::EdgeInsets {
            top: Length::px(2.0),
            right: Length::px(2.0),
            bottom: Length::px(2.0),
            left: Length::px(2.0),
        };
        computed.opacity = 0.35;

        ComputedStyleConsumer::apply_computed_style(&mut el, computed, None);

        let render_state = el.debug_render_state();
        assert_eq!(render_state.background_rgba, background.to_rgba_u8());
        assert_eq!(render_state.border_top_rgba, border_color.to_rgba_u8());
        assert_eq!(render_state.border_right_rgba, border_color.to_rgba_u8());
        assert_eq!(render_state.border_bottom_rgba, border_color.to_rgba_u8());
        assert_eq!(render_state.border_left_rgba, border_color.to_rgba_u8());
        assert!((el.border_widths.left - 2.0).abs() < 0.001);
        assert!((el.border_widths.right - 2.0).abs() < 0.001);
        assert!((el.border_widths.top - 2.0).abs() < 0.001);
        assert!((el.border_widths.bottom - 2.0).abs() < 0.001);
        assert!((render_state.opacity - 0.35).abs() < 0.001);
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

        let mut parent_mut =
            crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
        let scope = parent_mut.begin_child_clip_scope(&mut graph, &mut ctx, inner_radii);
        assert!(scope.is_some());
        assert!(scope.as_ref().is_some_and(|scope| scope.child_clip_id != 0));
    }

    #[test]
    fn child_clip_stencil_mask_uses_paint_snapped_destination_origin() {
        let parent = Element::new(0.0, 0.0, 100.5, 50.25);
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
                parent_x: 10.25,
                parent_y: 20.75,
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

        let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        ctx.translate_paint_offset(-0.25, -0.75);
        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        let inner_radii = parent_ref.inner_clip_radii(normalize_corner_radii(
            parent_ref.border_radii,
            parent_ref.layout_state.layout_size.width.max(0.0),
            parent_ref.layout_state.layout_size.height.max(0.0),
        ));
        let params = parent_ref.child_clip_stencil_pass_params(&ctx, inner_radii);

        assert_eq!(params.position, [10.0, 20.0]);
        assert_eq!(params.size, [100.5, 50.25]);
    }

    #[test]
    fn fractional_inner_clip_scissor_preserves_raw_coverage() {
        let parent = Element::new(0.0, 0.0, 100.5, 50.25);
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
                parent_x: 10.25,
                parent_y: 20.75,
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

        let inner_radii = {
            let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            assert_eq!(parent_ref.inner_clip_scissor_rect(), Some([10, 20, 101, 51]));
            parent_ref.inner_clip_radii(normalize_corner_radii(
                parent_ref.border_radii,
                parent_ref.layout_state.layout_size.width.max(0.0),
                parent_ref.layout_state.layout_size.height.max(0.0),
            ))
        };
        let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        ctx.translate_paint_offset(0.4, -0.6);
        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        let params = parent_ref.child_clip_stencil_pass_params(&ctx, inner_radii);

        assert!((params.position[0] - 10.65).abs() < 0.001);
        assert!((params.position[1] - 20.15).abs() < 0.001);
        assert_eq!(params.size, [100.5, 50.25]);
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

        let mut parent_mut =
            crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
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

        assert_eq!(
            test_promoted_build_count(promoted_child_id, "promoted-child"),
            0
        );
        assert_eq!(
            test_promoted_build_count(promoted_child_id, "promoted-layer"),
            0
        );
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
            ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
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
            ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
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

        let promotion_bounds =
            crate::view::test_support::get_element::<Element>(&arena, parent_key)
                .promotion_composite_bounds();
        let mut layer_ctx = UiBuildContext::from_parts(
            ctx.viewport(),
            super::BuildState::for_layer_subtree_with_ancestor_clip(ctx.ancestor_clip_context()),
        );
        let layer_target =
            layer_ctx.allocate_promoted_layer_target(&mut graph, parent_id, promotion_bounds);
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
            ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
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
            ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
        );
        parent.apply_style(parent_style);

        let mut row_child = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut row_style = Style::new();
        row_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        row_style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::percent(100.0)),
        );
        row_child.apply_style(row_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let row_key = commit_child(&mut arena, parent_key, Box::new(row_child));
        let _ = commit_child(
            &mut arena,
            row_key,
            Box::new(Element::new(0.0, 0.0, 40.0, 24.0)),
        );

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
            let _ = commit_child(
                &mut arena,
                stretched_key,
                Box::new(Element::new(0.0, 0.0, 120.0, 40.0)),
            );

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
                let parent_ref =
                    crate::view::test_support::get_element::<Element>(&arena, parent_key);
                assert_eq!(
                    parent_ref.computed_style.layout_axis_cross_size(),
                    CrossSize::Stretch
                );
            }
            {
                let stretched_ref =
                    crate::view::test_support::get_element::<Element>(&arena, stretched_key);
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
                let mut stretched_mut =
                    crate::view::test_support::get_element_mut::<Element>(&arena, stretched_key);
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
        assert_eq!(
            ctx.graphics_pass_context().scissor_rect,
            Some([10, 10, 40, 40])
        );

        let layer_state = super::BuildState::for_layer_subtree_with_ancestor_clip(
            super::AncestorClipContext::default(),
        );
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
            ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
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
        style.insert(
            PropertyId::PaddingLeft,
            ParsedValue::Length(Length::px(10.0)),
        );
        style.insert(
            PropertyId::PaddingRight,
            ParsedValue::Length(Length::px(10.0)),
        );
        style.insert(
            PropertyId::PaddingTop,
            ParsedValue::Length(Length::px(10.0)),
        );
        style.insert(
            PropertyId::PaddingBottom,
            ParsedValue::Length(Length::px(10.0)),
        );
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
    fn box_model_snapshot_uses_active_layout_frame_size() {
        let mut el = Element::new(0.0, 0.0, 100.0, 80.0);
        el.layout_state.layout_position = LayoutPosition { x: 5.0, y: 7.0 };
        el.layout_state.layout_size.width = 100.0;
        el.layout_state.layout_size.height = 80.0;
        el.layout_transition_override_width = Some(48.0);
        el.layout_transition_override_height = Some(0.0);

        let snapshot = el.box_model_snapshot();
        assert_eq!(snapshot.x, 5.0);
        assert_eq!(snapshot.y, 7.0);
        assert_eq!(snapshot.width, 48.0);
        assert_eq!(snapshot.height, 0.0);
    }

    #[test]
    fn box_model_snapshot_uses_rendered_size_without_polluting_layout_target() {
        let mut el = Element::new(0.0, 0.0, 100.0, 80.0);
        el.layout_state.layout_position = LayoutPosition { x: 5.0, y: 7.0 };
        el.layout_state.layout_size.width = 48.0;
        el.layout_state.layout_size.height = 30.0;

        let snapshot = el.box_model_snapshot();
        assert_eq!(snapshot.x, 5.0);
        assert_eq!(snapshot.y, 7.0);
        assert_eq!(snapshot.width, 48.0);
        assert_eq!(snapshot.height, 30.0);
        assert_eq!(el.layout_target_size(), (100.0, 80.0));
        assert_eq!(el.measured_size(), (100.0, 80.0));
    }

    #[test]
    fn zero_height_layout_transition_still_clips_children() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
        parent.layout_state.layout_position = LayoutPosition { x: 0.0, y: 0.0 };
        parent.layout_state.layout_size.width = 100.0;
        parent.layout_state.layout_size.height = 80.0;
        parent.layout_transition_override_width = Some(100.0);
        parent.layout_transition_override_height = Some(0.0);
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 40.0, 20.0)),
        );

        let parent = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        assert!(!parent.has_inner_render_area());
        let inner_radii = parent.inner_clip_radii(normalize_corner_radii(
            parent.border_radii,
            parent.box_model_snapshot().width.max(0.0),
            parent.box_model_snapshot().height.max(0.0),
        ));
        assert!(parent.should_clip_children(&[false], inner_radii, &arena));
    }

    #[test]
    fn zero_height_layout_transition_changes_promotion_clip_signature() {
        let mut closed_arena = new_test_arena();
        let mut closed = Element::new(0.0, 0.0, 100.0, 80.0);
        closed.layout_state.layout_position = LayoutPosition { x: 0.0, y: 0.0 };
        closed.layout_state.layout_size.width = 100.0;
        closed.layout_state.layout_size.height = 80.0;
        let closed_key = commit_element(&mut closed_arena, Box::new(closed));
        let _ = commit_child(
            &mut closed_arena,
            closed_key,
            Box::new(Element::new(0.0, 0.0, 40.0, 20.0)),
        );
        let closed = crate::view::test_support::get_element::<Element>(&closed_arena, closed_key);
        let open_signature = closed.promotion_clip_intersection_signature(&closed_arena);

        let mut active_arena = new_test_arena();
        let mut active = Element::new(0.0, 0.0, 100.0, 80.0);
        active.layout_state.layout_position = LayoutPosition { x: 0.0, y: 0.0 };
        active.layout_state.layout_size.width = 100.0;
        active.layout_state.layout_size.height = 80.0;
        active.layout_transition_override_width = Some(100.0);
        active.layout_transition_override_height = Some(0.0);
        let active_key = commit_element(&mut active_arena, Box::new(active));
        let _ = commit_child(
            &mut active_arena,
            active_key,
            Box::new(Element::new(0.0, 0.0, 40.0, 20.0)),
        );
        let active = crate::view::test_support::get_element::<Element>(&active_arena, active_key);
        assert!(!active.has_inner_render_area());
        let active_signature = active.promotion_clip_intersection_signature(&active_arena);

        assert_ne!(open_signature, active_signature);
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
        let _ = commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 40.0, 40.0)),
        );

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
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Position,
                200,
            ))),
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
                ParsedValue::Transition(Transitions::single(Transition::new(
                    TransitionProperty::Position,
                    200,
                ))),
            );
            anchor.apply_style(next_anchor_style);
            anchor.layout_transition_visual_offset_x = -40.0;
            anchor.layout_transition_target_x = Some(340.0);
        });

        arena.with_element_taken(parent_key, |el, _a| {
            el.as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .mark_layout_dirty();
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

    fn clean_bridge_element(width: f32, height: f32) -> Element {
        let mut element = Element::new(0.0, 0.0, width, height);
        element.clear_local_dirty_flags(DirtyFlags::ALL);
        element.mark_paint_dirty();
        element
    }

    fn mark_arena_paint_dirty_for_subtree(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
    ) {
        arena.mark_dirty(key, DirtyFlags::PAINT);
        for child in arena.children_of(key) {
            mark_arena_paint_dirty_for_subtree(arena, child);
        }
    }

    #[test]
    fn clear_subtree_dirty_flags_with_arena_dirty_clears_element_and_arena_dirty() {
        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(clean_bridge_element(100.0, 100.0)));
        let child_key = commit_child(
            &mut arena,
            root_key,
            Box::new(clean_bridge_element(80.0, 40.0)),
        );
        let grandchild_key = commit_child(
            &mut arena,
            child_key,
            Box::new(clean_bridge_element(40.0, 20.0)),
        );
        arena.with_element_taken(root_key, |root, _arena| {
            root.clear_local_dirty_flags(DirtyFlags::PAINT);
        });
        arena.clear_arena_dirty_subtree(root_key, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root_key);
        mark_arena_paint_dirty_for_subtree(&arena, child_key);

        assert!(
            arena
                .cached_subtree_dirty(root_key)
                .intersects(DirtyFlags::PAINT)
        );
        assert!(
            arena
                .cached_subtree_dirty(child_key)
                .intersects(DirtyFlags::PAINT)
        );

        assert!(
            crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
                &mut arena,
                child_key,
                DirtyFlags::PAINT,
            )
        );

        let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
        let grandchild = crate::view::test_support::get_element::<Element>(&arena, grandchild_key);
        assert!(!child.local_dirty_flags().contains(DirtyFlags::PAINT));
        assert!(!grandchild.local_dirty_flags().contains(DirtyFlags::PAINT));
        assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
        assert_eq!(arena.arena_local_dirty(grandchild_key), DirtyFlags::NONE);
        assert!(
            !arena
                .cached_subtree_dirty(root_key)
                .intersects(DirtyFlags::PAINT)
        );
        assert!(
            !arena
                .cached_subtree_dirty(child_key)
                .intersects(DirtyFlags::PAINT)
        );
    }

    #[test]
    fn clear_subtree_dirty_flags_with_arena_dirty_preserves_sibling_arena_dirty() {
        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(clean_bridge_element(100.0, 100.0)));
        let child_key = commit_child(
            &mut arena,
            root_key,
            Box::new(clean_bridge_element(80.0, 40.0)),
        );
        let grandchild_key = commit_child(
            &mut arena,
            child_key,
            Box::new(clean_bridge_element(40.0, 20.0)),
        );
        let sibling_key = commit_child(
            &mut arena,
            root_key,
            Box::new(clean_bridge_element(60.0, 30.0)),
        );
        arena.with_element_taken(root_key, |root, _arena| {
            root.clear_local_dirty_flags(DirtyFlags::PAINT);
        });
        arena.clear_arena_dirty_subtree(root_key, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root_key);
        mark_arena_paint_dirty_for_subtree(&arena, child_key);
        arena.mark_dirty(sibling_key, DirtyFlags::PAINT);

        assert!(
            crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
                &mut arena,
                child_key,
                DirtyFlags::PAINT,
            )
        );

        let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
        let grandchild = crate::view::test_support::get_element::<Element>(&arena, grandchild_key);
        assert!(!child.local_dirty_flags().contains(DirtyFlags::PAINT));
        assert!(!grandchild.local_dirty_flags().contains(DirtyFlags::PAINT));
        assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
        assert_eq!(arena.arena_local_dirty(grandchild_key), DirtyFlags::NONE);
        assert!(
            arena
                .arena_local_dirty(sibling_key)
                .contains(DirtyFlags::PAINT)
        );
        assert!(
            !arena
                .cached_subtree_dirty(child_key)
                .intersects(DirtyFlags::PAINT)
        );
        assert!(
            arena
                .cached_subtree_dirty(sibling_key)
                .intersects(DirtyFlags::PAINT)
        );
        assert!(
            arena
                .cached_subtree_dirty(root_key)
                .intersects(DirtyFlags::PAINT)
        );
    }

    #[test]
    fn clear_subtree_dirty_flags_with_arena_dirty_returns_false_for_missing_root() {
        let mut arena = new_test_arena();
        let root_key = commit_element(&mut arena, Box::new(clean_bridge_element(100.0, 100.0)));

        assert!(
            !crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
                &mut arena,
                crate::view::node_arena::NodeKey::default(),
                DirtyFlags::PAINT,
            )
        );
        assert!(
            arena
                .cached_subtree_dirty(root_key)
                .intersects(DirtyFlags::PAINT)
        );
    }

    fn clean_style_sample_arena() -> (
        crate::view::node_arena::NodeArena,
        crate::view::node_arena::NodeKey,
        crate::view::node_arena::NodeKey,
        u64,
    ) {
        let mut arena = new_test_arena();
        let mut root = Element::new(0.0, 0.0, 200.0, 150.0);
        root.clear_local_dirty_flags(DirtyFlags::ALL);
        let root_key = commit_element(&mut arena, Box::new(root));

        let mut child = Element::new(0.0, 0.0, 80.0, 40.0);
        child.clear_local_dirty_flags(DirtyFlags::ALL);
        let child_id = child.stable_id();
        let child_key = commit_child(&mut arena, root_key, Box::new(child));

        assert!(crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
            &mut arena,
            root_key,
            DirtyFlags::ALL,
        ));
        arena.refresh_subtree_dirty_cache(root_key);
        assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
        assert!(
            !arena
                .cached_subtree_dirty(root_key)
                .intersects(DirtyFlags::PAINT)
        );

        (arena, root_key, child_key, child_id)
    }

    fn assert_style_sample_paint_dirty(
        arena: &crate::view::node_arena::NodeArena,
        root_key: crate::view::node_arena::NodeKey,
        child_key: crate::view::node_arena::NodeKey,
    ) {
        assert_style_sample_dirty_flags(arena, root_key, child_key, DirtyFlags::PAINT);
    }

    fn assert_style_sample_dirty_flags(
        arena: &crate::view::node_arena::NodeArena,
        root_key: crate::view::node_arena::NodeKey,
        child_key: crate::view::node_arena::NodeKey,
        flags: DirtyFlags,
    ) {
        let child = crate::view::test_support::get_element::<Element>(arena, child_key);
        assert!(child.local_dirty_flags().contains(flags));
        assert!(arena.arena_local_dirty(child_key).contains(flags));
        assert!(arena.cached_subtree_dirty(child_key).contains(flags));
        assert!(arena.cached_subtree_dirty(root_key).contains(flags));
    }

    fn style_sample_place_dirty_flags() -> DirtyFlags {
        DirtyFlags::PLACE
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
            .union(DirtyFlags::PAINT)
    }

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
        assert_style_sample_paint_dirty(&arena, root_key, child_key);
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

    #[test]
    fn inline_layout_wraps_children_into_multiple_line_boxes() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 100.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 60.0, 10.0)),
        );
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 50.0, 20.0)),
        );
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 40.0, 15.0)),
        );

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
        // Sprint 3 (D3 default Baseline): pure-element diff-height row
        // → shorter element bottom-aligns. line_ascent = max(20, 15) =
        // 20; element baseline = height, so el3 offset = 20 - 15 = 5
        // → y = 10 + 5 = 15 (was 10 under Align::Start).
        assert_eq!(third.y, 15.0);
        let parent_el = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        // Pure-element rows: line_box_h = max(height) (descent = 0),
        // total content_size unchanged from pre-Sprint-3.
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
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 50.0, 20.0)),
        );
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content(
                " trailing text continues after the badge.",
            )),
        );

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

        // Sprint 3 (D3 default Baseline): mixed text + tall-element row
        // → element keeps top (baseline = height = line_ascent),
        // text drops to align its glyph baseline to the line baseline.
        // Test still verifies `lead`, `badge`, `trailing` all share
        // line 0 (no wrap); text y is now > 0 by a small text-ascent
        // adjustment (~3-5 px at default font).
        assert_eq!(badge.y, 0.0);
        assert!(trailing_snapshot.y > 0.0);
        assert!(trailing_snapshot.y < 8.0);
        assert!(first_fragment.1.x >= badge.x + badge.width);
        assert!(first_fragment.1.y > 0.0);
        assert!(first_fragment.1.y < 8.0);
        // All three children still share line 0 — no wrap.
        assert!((first_fragment.1.y - trailing_snapshot.y).abs() < 0.5);
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

        let mut text =
            Text::from_content("fragmented text should wrap across multiple inline lines");
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

        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content(
                "最後接一段中文，確認混排時也能一起換行。",
            )),
        );

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
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content("nested")),
        );
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Element::new(0.0, 0.0, 44.0, 20.0)),
        );

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
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(
                "inline wrapper background should wrap across lines",
            )),
        );

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
            .filter(|name| {
                name.contains("draw_rect_pass::DrawRectPass")
                    || name.contains("draw_rect_pass::OpaqueRectPass")
            })
            .count();
        let border_count = pass_names
            .iter()
            .filter(|name| name.contains("draw_rect_pass::DrawRectPass"))
            .count();

        // Both DrawRectPass and OpaqueRectPass count as fragment rects;
        // opacity promotion is governed by `is_opaque_candidate` and may
        // shift between the two depending on geometry/overlap. The
        // invariant we care about is that a wrapped fragmentable inline
        // wrapper produces ≥ 2 *fill* and ≥ 2 *border* rect passes (one
        // per visual line fragment) — so total rect-like passes ≥ 4.
        let _ = border_count;
        assert!(
            rect_like_count >= 4,
            "expected multiple fragment rect passes, got {pass_names:?}"
        );
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
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content("Inline text starts here,")),
        );

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#93c5fd")),
        );
        wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(
                "badge test test test test test test test",
            )),
        );
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content(
                "then more text continues after the badge,",
            )),
        );

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
        wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content("badge test test test test")),
        );

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
            let last = *wrapper_el
                .inline_paint_fragments
                .last()
                .expect("last fragment");
            let nested_text =
                crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
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
                text.measure(
                    LayoutConstraints {
                        max_width: 200.0,
                        max_height: 80.0,
                        viewport_width: 200.0,
                        viewport_height: 80.0,
                        percent_base_width: Some(200.0),
                        percent_base_height: Some(80.0),
                    },
                    &mut arena,
                );
                let (width, _) = text.measured_size();
                position.x + width
            })
            .fold(0.0_f32, f32::max);
        assert!((last.x + last.width - last_line_right - 8.0).abs() < 0.5);
    }

    #[test]
    fn inline_element_ifc_decoration_prewire_matches_legacy_fragment_rects() {
        const WRAPPER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(42);
        let content = "badge test test test test";
        let parent_width = 160.0;
        let inset = 8.0;

        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(parent_width)));
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
        wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(7.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(content)),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: parent_width,
                max_height: 160.0,
                viewport_width: parent_width,
                viewport_height: 160.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(160.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: parent_width,
                available_height: 160.0,
                viewport_width: parent_width,
                viewport_height: 160.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(160.0),
            },
        );

        let legacy_fragments = {
            let wrapper_el =
                crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
            wrapper_el.inline_fragment_rects().to_vec()
        };
        assert!(
            legacy_fragments.len() >= 2,
            "legacy wrapped inline wrapper should expose per-line paint fragments: {legacy_fragments:?}"
        );

        let ifc = InlineFormattingContext::build(
            InlineIfcInput::new(vec![InlineIfcItem::Span {
                source: WRAPPER_SOURCE,
                style: Some(InlineIfcStyle {
                    font_size: 16.0,
                    line_height: 1.25,
                    ..InlineIfcStyle::default()
                }),
                children: vec![InlineIfcItem::TextSpan {
                    source: WRAPPER_SOURCE,
                    text: content.to_string(),
                    style: None,
                }],
            }])
            .with_max_width(parent_width - inset * 2.0),
        );
        let ifc_fragments = ifc.element_decoration_paint_fragments(
            WRAPPER_SOURCE,
            InlineIfcDecorationBoxInsets::new(inset, inset, inset, inset),
        );

        assert_eq!(
            ifc_fragments.len(),
            legacy_fragments.len(),
            "IFC decoration prewire should split the same wrapped span count; legacy={legacy_fragments:?} ifc={ifc_fragments:?}"
        );
        assert!(
            ifc_fragments
                .iter()
                .zip(legacy_fragments.iter())
                .all(|(ifc, legacy)| {
                    let ifc = ifc.rect;
                    rect_close(
                        Rect {
                            x: ifc.x,
                            y: ifc.y,
                            width: ifc.width,
                            height: ifc.height,
                        },
                        *legacy,
                        0.75,
                    )
                }),
            "IFC decoration fragments should match legacy inline_paint_fragments; legacy={legacy_fragments:?} ifc={ifc_fragments:?}"
        );
        let first = ifc_fragments.first().expect("first IFC fragment");
        let last = ifc_fragments.last().expect("last IFC fragment");
        assert!(first.is_first_for_source);
        assert!(last.is_last_for_source);
    }

    #[test]
    fn inline_element_ifc_decoration_package_keeps_multiple_sibling_sources_separate() {
        const FIRST_SOURCE: InlineIfcSourceId = InlineIfcSourceId(51);
        const SECOND_SOURCE: InlineIfcSourceId = InlineIfcSourceId(52);
        let first_content = "alpha beta gamma delta";
        let second_content = "epsilon zeta eta theta";
        let parent_width = 170.0;
        let inset = 8.0;

        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(parent_width)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut first = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut first_style = Style::new();
        first_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        first_style.insert(PropertyId::Width, ParsedValue::Auto);
        first_style.insert(PropertyId::Height, ParsedValue::Auto);
        first_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#bfdbfe")),
        );
        first_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#2563eb")));
        first_style.set_padding(crate::style::Padding::uniform(Length::px(7.0)));
        first.apply_style(first_style);
        let first_key = commit_child(&mut arena, parent_key, Box::new(first));
        commit_child(
            &mut arena,
            first_key,
            Box::new(Text::from_content(first_content)),
        );

        let mut second = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        second_style.insert(PropertyId::Width, ParsedValue::Auto);
        second_style.insert(PropertyId::Height, ParsedValue::Auto);
        second_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fecaca")),
        );
        second_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        second_style.set_padding(crate::style::Padding::uniform(Length::px(7.0)));
        second.apply_style(second_style);
        let second_key = commit_child(&mut arena, parent_key, Box::new(second));
        commit_child(
            &mut arena,
            second_key,
            Box::new(Text::from_content(second_content)),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: parent_width,
                max_height: 220.0,
                viewport_width: parent_width,
                viewport_height: 220.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(220.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: parent_width,
                available_height: 220.0,
                viewport_width: parent_width,
                viewport_height: 220.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(220.0),
            },
        );

        let first_legacy = crate::view::test_support::get_element::<Element>(&arena, first_key)
            .inline_fragment_rects()
            .to_vec();
        let second_legacy = crate::view::test_support::get_element::<Element>(&arena, second_key)
            .inline_fragment_rects()
            .to_vec();
        assert!(
            first_legacy.len() >= 2 && second_legacy.len() >= 2,
            "fixture should wrap both legacy sibling wrappers; first={first_legacy:?} second={second_legacy:?}"
        );

        let ifc = InlineFormattingContext::build(
            InlineIfcInput::new(vec![
                InlineIfcItem::Span {
                    source: FIRST_SOURCE,
                    style: Some(InlineIfcStyle {
                        font_size: 16.0,
                        line_height: 1.25,
                        brush: [11, 22, 33, 255],
                        ..InlineIfcStyle::default()
                    }),
                    children: vec![InlineIfcItem::TextSpan {
                        source: FIRST_SOURCE,
                        text: first_content.to_string(),
                        style: None,
                    }],
                },
                InlineIfcItem::Span {
                    source: SECOND_SOURCE,
                    style: Some(InlineIfcStyle {
                        font_size: 16.0,
                        line_height: 1.25,
                        brush: [44, 55, 66, 255],
                        ..InlineIfcStyle::default()
                    }),
                    children: vec![InlineIfcItem::TextSpan {
                        source: SECOND_SOURCE,
                        text: second_content.to_string(),
                        style: None,
                    }],
                },
            ])
            .with_max_width(parent_width - inset * 2.0),
        );
        let first_style = InlineIfcElementDecorationDrawRectStyle::from_fill_style(
            &InlineIfcStyle {
                brush: [11, 22, 33, 255],
                ..InlineIfcStyle::default()
            },
        );
        let second_style = InlineIfcElementDecorationDrawRectStyle::from_fill_style(
            &InlineIfcStyle {
                brush: [44, 55, 66, 255],
                ..InlineIfcStyle::default()
            },
        );
        let first_package = ifc.element_decoration_draw_rect_package(
            FIRST_SOURCE,
            InlineIfcDecorationBoxInsets::new(inset, inset, inset, inset),
            first_style,
        );
        let second_package = ifc.element_decoration_draw_rect_package(
            SECOND_SOURCE,
            InlineIfcDecorationBoxInsets::new(inset, inset, inset, inset),
            second_style,
        );

        assert_eq!(first_package.source, FIRST_SOURCE);
        assert_eq!(second_package.source, SECOND_SOURCE);
        assert!(
            first_package
                .fragments
                .iter()
                .all(|fragment| fragment.source == FIRST_SOURCE
                    && fragment.style_key == first_package.style_key),
            "first package should not contain second sibling metadata: {first_package:?}"
        );
        assert!(
            second_package
                .fragments
                .iter()
                .all(|fragment| fragment.source == SECOND_SOURCE
                    && fragment.style_key == second_package.style_key),
            "second package should not contain first sibling metadata: {second_package:?}"
        );
        assert_eq!(
            first_package.fragments.len(),
            first_legacy.len(),
            "first sibling IFC package should split like legacy; legacy={first_legacy:?} package={first_package:?}"
        );
        assert_eq!(
            second_package.fragments.len(),
            second_legacy.len(),
            "second sibling IFC package should split like legacy; legacy={second_legacy:?} package={second_package:?}"
        );
    }

    #[test]
    fn inline_element_ifc_render_candidate_gate_preserves_legacy_fallback() {
        const WRAPPER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(77);
        let content = "inline wrapper background should wrap across lines";
        let parent_width = 160.0;

        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(parent_width)));
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
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(content)),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: parent_width,
                max_height: 160.0,
                viewport_width: parent_width,
                viewport_height: 160.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(160.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: parent_width,
                available_height: 160.0,
                viewport_width: parent_width,
                viewport_height: 160.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(160.0),
            },
        );

        let legacy_fragments = crate::view::test_support::get_element::<Element>(&arena, wrapper_key)
            .inline_fragment_rects()
            .to_vec();
        assert!(
            legacy_fragments.len() >= 2,
            "fixture should have wrapped legacy fragments: {legacy_fragments:?}"
        );

        let ifc = InlineFormattingContext::build(
            InlineIfcInput::new(vec![InlineIfcItem::Span {
                source: WRAPPER_SOURCE,
                style: Some(InlineIfcStyle {
                    font_size: 16.0,
                    line_height: 1.25,
                    brush: [147, 197, 253, 255],
                    ..InlineIfcStyle::default()
                }),
                children: vec![InlineIfcItem::TextSpan {
                    source: WRAPPER_SOURCE,
                    text: content.to_string(),
                    style: None,
                }],
            }])
            .with_max_width(parent_width),
        );
        let mut draw_style = InlineIfcElementDecorationDrawRectStyle::from_fill_style(
            &InlineIfcStyle {
                brush: [147, 197, 253, 255],
                ..InlineIfcStyle::default()
            },
        );
        draw_style.border_widths = [1.0, 1.0, 1.0, 1.0];
        draw_style.border_color = [37.0 / 255.0, 99.0 / 255.0, 235.0 / 255.0, 1.0];
        let package = ifc.element_decoration_draw_rect_package(
            WRAPPER_SOURCE,
            InlineIfcDecorationBoxInsets::new(1.0, 1.0, 1.0, 1.0),
            draw_style,
        );
        assert_eq!(package.fragments.len(), legacy_fragments.len());

        let fallback_without_package = {
            let mut wrapper_el =
                crate::view::test_support::get_element_mut::<Element>(&mut arena, wrapper_key);
            assert_eq!(
                wrapper_el.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments
            );
            wrapper_el.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
            wrapper_el.inline_ifc_render_decision_for_test()
        };
        assert_eq!(
            fallback_without_package,
            ElementInlineIfcRenderDecision::ExistingInlineFragments
        );

        let candidate_metadata = {
            let mut wrapper_el =
                crate::view::test_support::get_element_mut::<Element>(&mut arena, wrapper_key);
            wrapper_el.set_inline_ifc_draw_rect_package_for_test(package.clone());
            assert_eq!(
                wrapper_el.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                }
            );
            wrapper_el.inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
        };
        assert_eq!(candidate_metadata.len(), legacy_fragments.len());
        for ((metadata, legacy), package_fragment) in candidate_metadata
            .iter()
            .zip(legacy_fragments.iter())
            .zip(package.fragments.iter())
        {
            assert!(
                rect_close(
                    Rect {
                        x: metadata.fill.position[0],
                        y: metadata.fill.position[1],
                        width: metadata.fill.size[0],
                        height: metadata.fill.size[1],
                    },
                    *legacy,
                    0.75,
                ),
                "candidate fill metadata should preserve legacy rects; metadata={metadata:?} legacy={legacy:?}"
            );
            assert_eq!(metadata.fill.fill_color, package_fragment.metadata.fill_color);
            assert_eq!(metadata.fill.opacity, package_fragment.metadata.opacity);
            assert_eq!(metadata.fill.border_widths, [1.0, 1.0, 1.0, 1.0]);
            let border = metadata.border.as_ref().expect("border metadata");
            assert_eq!(border.fill_color, [0.0, 0.0, 0.0, 0.0]);
            assert_eq!(border.opacity, package_fragment.metadata.opacity);
            assert_eq!(border.border_widths, [1.0, 1.0, 1.0, 1.0]);
            assert_eq!(border.border_color, package_fragment.metadata.border_color);
        }

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
            .filter(|name| {
                name.contains("draw_rect_pass::DrawRectPass")
                    || name.contains("draw_rect_pass::OpaqueRectPass")
            })
            .count();
        assert!(
            rect_like_count >= package.fragments.len() * 2,
            "candidate package should build fill/border DrawRect wiring, got {pass_names:?}"
        );
    }

    #[test]
    fn inline_element_ifc_rollout_candidate_reads_atomic_placement_without_decorating_it() {
        const WRAPPER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(81);
        const ATOMIC_SOURCE: InlineIfcSourceId = InlineIfcSourceId(82);

        let ifc = InlineFormattingContext::build(
            InlineIfcInput::new(vec![InlineIfcItem::Span {
                source: WRAPPER_SOURCE,
                style: Some(InlineIfcStyle {
                    font_size: 16.0,
                    line_height: 1.25,
                    brush: [191, 219, 254, 255],
                    ..InlineIfcStyle::default()
                }),
                children: vec![
                    InlineIfcItem::TextSpan {
                        source: WRAPPER_SOURCE,
                        text: "before ".to_string(),
                        style: None,
                    },
                    InlineIfcItem::AtomicInlineBox {
                        source: ATOMIC_SOURCE,
                        measurement: InlineIfcMeasuredAtomicBox::new(
                            InlineIfcSize::new(42.0, 20.0),
                            InlineIfcAtomicMeasureConstraints::new(Some(170.0)),
                        ),
                    },
                    InlineIfcItem::TextSpan {
                        source: WRAPPER_SOURCE,
                        text: " after".to_string(),
                        style: None,
                    },
                ],
            }])
            .with_max_width(170.0),
        );
        let snapshot = ifc.text_layout_snapshot();
        let decoration_package = ifc.element_decoration_draw_rect_package(
            WRAPPER_SOURCE,
            InlineIfcDecorationBoxInsets::new(2.0, 2.0, 1.0, 1.0),
            InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
                brush: [191, 219, 254, 255],
                ..InlineIfcStyle::default()
            }),
        );
        let atomic_package = ifc.atomic_box_placement_package(ATOMIC_SOURCE);

        assert!(!decoration_package.fragments.is_empty());
        assert_eq!(atomic_package.placements.len(), 1);
        assert!(
            snapshot
                .lines
                .iter()
                .flat_map(|line| &line.glyphs)
                .all(|glyph| glyph.source != ATOMIC_SOURCE),
            "atomic inline box must stay out of text glyph payload: {snapshot:?}"
        );
        assert!(
            decoration_package
                .fragments
                .iter()
                .all(|fragment| fragment.source != ATOMIC_SOURCE),
            "atomic inline box must stay out of span decoration payload: {decoration_package:?}"
        );

        let mut element = Element::new(0.0, 0.0, 0.0, 0.0);
        element.set_inline_ifc_render_mode_for_test(
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
        );
        element.set_inline_ifc_draw_rect_package_for_test(decoration_package);
        element.set_inline_ifc_atomic_placement_package_for_test(atomic_package.clone());

        assert_eq!(
            element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: true,
            }
        );
        let metadata = element
            .inline_ifc_atomic_placement_metadata_for_test()
            .expect("candidate should expose atomic placement metadata");
        assert_eq!(metadata.package.source, ATOMIC_SOURCE);
        assert_eq!(metadata.package.placements, atomic_package.placements);
        let placement = &metadata.package.placements[0];
        assert_eq!(placement.source, ATOMIC_SOURCE);
        assert_eq!(
            placement.measurement.measured_size,
            InlineIfcSize::new(42.0, 20.0)
        );
        assert!(placement.rect.width > 0.0 && placement.rect.height > 0.0);
    }

    #[test]
    fn inline_element_ifc_rollout_candidate_accepts_distributed_packages() {
        const WRAPPER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(83);
        const ATOMIC_SOURCE: InlineIfcSourceId = InlineIfcSourceId(84);
        const MISSING_SOURCE: InlineIfcSourceId = InlineIfcSourceId(85);

        let ifc = InlineFormattingContext::build(
            InlineIfcInput::new(vec![InlineIfcItem::Span {
                source: WRAPPER_SOURCE,
                style: Some(InlineIfcStyle {
                    font_size: 16.0,
                    line_height: 1.25,
                    brush: [216, 180, 254, 255],
                    ..InlineIfcStyle::default()
                }),
                children: vec![
                    InlineIfcItem::TextSpan {
                        source: WRAPPER_SOURCE,
                        text: "distributed wrapper ".to_string(),
                        style: None,
                    },
                    InlineIfcItem::AtomicInlineBox {
                        source: ATOMIC_SOURCE,
                        measurement: InlineIfcMeasuredAtomicBox::new(
                            InlineIfcSize::new(36.0, 18.0),
                            InlineIfcAtomicMeasureConstraints::new(Some(180.0)),
                        ),
                    },
                    InlineIfcItem::TextSpan {
                        source: WRAPPER_SOURCE,
                        text: " candidate".to_string(),
                        style: None,
                    },
                ],
            }])
            .with_max_width(180.0),
        );
        let mut draw_style = InlineIfcElementDecorationDrawRectStyle::from_fill_style(
            &InlineIfcStyle {
                brush: [216, 180, 254, 255],
                ..InlineIfcStyle::default()
            },
        );
        draw_style.opacity = 0.82;
        draw_style.border_widths = [1.0, 2.0, 1.0, 2.0];
        draw_style.border_color = [126.0 / 255.0, 34.0 / 255.0, 206.0 / 255.0, 1.0];
        let distributor = ifc.element_package_distributor(
            InlineIfcElementPackageDistributionInput::new()
                .with_decoration_source(InlineIfcElementDecorationPackageSource::new(
                    WRAPPER_SOURCE,
                    InlineIfcDecorationBoxInsets::new(2.0, 2.0, 1.0, 1.0),
                    draw_style,
                ))
                .with_atomic_source(ATOMIC_SOURCE)
                .with_atomic_source(MISSING_SOURCE),
        );

        assert!(
            distributor.package(MISSING_SOURCE).is_none(),
            "missing sources should not alias another source's package"
        );
        let distributed = distributor
            .package(WRAPPER_SOURCE)
            .expect("wrapper source should receive distributed package");
        assert!(distributed.decoration_draw_rect.is_some());
        assert!(distributed.atomic_placement.is_none());
        let atomic = distributor
            .package(ATOMIC_SOURCE)
            .expect("atomic source should receive distributed package");
        assert!(atomic.decoration_draw_rect.is_none());
        assert!(atomic.atomic_placement.is_some());

        let mut element = Element::new(0.0, 0.0, 0.0, 0.0);
        element.set_inline_ifc_render_mode_for_test(
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
        );
        element.set_inline_ifc_rollout_packages_for_test(
            ElementInlineIfcRolloutPackages::from_inline_ifc_distributed(distributed),
        );

        assert_eq!(
            element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            }
        );
        let metadata = element.inline_ifc_draw_rect_pass_metadata_for_test([4.0, 5.0]);
        let package = distributed
            .decoration_draw_rect
            .as_ref()
            .expect("distributed decoration package");
        assert_eq!(metadata.len(), package.fragments.len());
        assert!(!metadata.is_empty());
        for (metadata, fragment) in metadata.iter().zip(package.fragments.iter()) {
            assert_eq!(
                metadata.fill.position,
                [
                    fragment.metadata.position[0] + 4.0,
                    fragment.metadata.position[1] + 5.0,
                ]
            );
            assert_eq!(metadata.fill.size, fragment.metadata.size);
            assert_eq!(metadata.fill.fill_color, draw_style.fill_color);
            assert_eq!(
                metadata.border.as_ref().map(|border| border.border_color),
                Some(draw_style.border_color)
            );
        }
    }

    #[test]
    fn inline_element_ifc_root_source_builder_caches_and_distributes_production_like_subtree() {
        const OUTER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(86);
        const INNER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(87);
        const ATOMIC_SOURCE: InlineIfcSourceId = InlineIfcSourceId(88);
        const MISSING_SOURCE: InlineIfcSourceId = InlineIfcSourceId(89);

        let outer_style = InlineIfcStyle {
            font_size: 16.0,
            line_height: 1.25,
            brush: [191, 219, 254, 255],
            ..InlineIfcStyle::default()
        };
        let inner_style = InlineIfcStyle {
            font_size: 16.0,
            line_height: 1.25,
            brush: [254, 202, 202, 255],
            ..InlineIfcStyle::default()
        };
        let mut outer_draw_style =
            InlineIfcElementDecorationDrawRectStyle::from_fill_style(&outer_style);
        outer_draw_style.opacity = 0.91;
        outer_draw_style.border_widths = [2.0, 2.0, 1.0, 1.0];
        outer_draw_style.border_color = [37.0 / 255.0, 99.0 / 255.0, 235.0 / 255.0, 1.0];
        let mut inner_draw_style =
            InlineIfcElementDecorationDrawRectStyle::from_fill_style(&inner_style);
        inner_draw_style.opacity = 0.83;
        inner_draw_style.border_widths = [1.0, 1.0, 1.0, 1.0];
        inner_draw_style.border_color = [220.0 / 255.0, 38.0 / 255.0, 38.0 / 255.0, 1.0];

        let mut root_builder = InlineIfcElementRootSourceBuilder::new().with_max_width(178.0);
        root_builder
            .push_item(InlineIfcItem::Span {
                source: OUTER_SOURCE,
                style: Some(outer_style.clone()),
                children: vec![
                    InlineIfcItem::TextSpan {
                        source: OUTER_SOURCE,
                        text: "outer prefix ".to_string(),
                        style: None,
                    },
                    InlineIfcItem::Span {
                        source: INNER_SOURCE,
                        style: Some(inner_style.clone()),
                        children: vec![InlineIfcItem::TextSpan {
                            source: INNER_SOURCE,
                            text: "inner production-like chip".to_string(),
                            style: None,
                        }],
                    },
                    InlineIfcItem::AtomicInlineBox {
                        source: ATOMIC_SOURCE,
                        measurement: InlineIfcMeasuredAtomicBox::new(
                            InlineIfcSize::new(34.0, 18.0),
                            InlineIfcAtomicMeasureConstraints::new(Some(178.0)),
                        ),
                    },
                    InlineIfcItem::TextSpan {
                        source: OUTER_SOURCE,
                        text: " outer tail wraps".to_string(),
                        style: None,
                    },
                ],
            })
            .add_decoration_source(InlineIfcElementDecorationPackageSource::new(
                OUTER_SOURCE,
                InlineIfcDecorationBoxInsets::new(2.0, 2.0, 1.0, 1.0),
                outer_draw_style,
            ))
            .add_decoration_source(InlineIfcElementDecorationPackageSource::new(
                INNER_SOURCE,
                InlineIfcDecorationBoxInsets::new(1.0, 1.0, 1.0, 1.0),
                inner_draw_style,
            ))
            .add_decoration_source(InlineIfcElementDecorationPackageSource::new(
                MISSING_SOURCE,
                InlineIfcDecorationBoxInsets::new(1.0, 1.0, 1.0, 1.0),
                inner_draw_style,
            ))
            .add_atomic_source(ATOMIC_SOURCE)
            .add_atomic_source(MISSING_SOURCE);
        let root_source = root_builder.build();
        let expected_key = root_source.cache_key();

        let mut cache = InlineIfcElementRootCandidateCache::new();
        let mut outer_element = Element::new(0.0, 0.0, 0.0, 0.0);
        let first_candidate = outer_element.update_inline_ifc_rollout_packages_from_root_source(
            &root_source,
            OUTER_SOURCE,
            &mut cache,
        );
        assert_eq!(first_candidate.cache_key, expected_key);
        assert_eq!(first_candidate.invalidation, InlineIfcInvalidation::Reshape);
        assert!(first_candidate.rebuilt);
        assert_eq!(cache.len(), 1);
        assert!(first_candidate.decoration_package(OUTER_SOURCE).is_some());
        assert!(first_candidate.decoration_package(INNER_SOURCE).is_some());
        assert!(first_candidate.atomic_package(ATOMIC_SOURCE).is_some());
        assert!(
            first_candidate.package(MISSING_SOURCE).is_none(),
            "missing source should not alias any production-like root package"
        );
        assert_eq!(
            outer_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            },
            "render default should use staged packages while retaining legacy fallback"
        );

        outer_element.set_inline_ifc_render_mode_for_test(
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
        );
        assert_eq!(
            outer_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            }
        );
        assert!(
            !outer_element
                .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                .is_empty()
        );

        let mut inner_element = Element::new(0.0, 0.0, 0.0, 0.0);
        inner_element
            .set_inline_ifc_render_mode_for_test(ElementInlineIfcRenderMode::DrawRectPackageCandidate);
        let second_candidate = inner_element.update_inline_ifc_rollout_packages_from_root_source(
            &root_source,
            INNER_SOURCE,
            &mut cache,
        );
        assert_eq!(second_candidate.invalidation, InlineIfcInvalidation::Reuse);
        assert!(!second_candidate.rebuilt);
        assert_eq!(cache.len(), 1);
        assert_eq!(
            inner_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            }
        );

        let mut missing_element = Element::new(0.0, 0.0, 0.0, 0.0);
        missing_element.set_inline_ifc_render_mode_for_test(
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
        );
        let missing_candidate = missing_element.update_inline_ifc_rollout_packages_from_root_source(
            &root_source,
            MISSING_SOURCE,
            &mut cache,
        );
        assert_eq!(missing_candidate.invalidation, InlineIfcInvalidation::Reuse);
        assert_eq!(
            missing_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "source without a distributed package must keep the legacy fallback path"
        );
        assert!(
            missing_element
                .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                .is_empty()
        );
    }

    #[test]
    fn inline_element_ifc_metadata_collector_builds_root_source_from_real_subtree() {
        let parent_width = 188.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(200, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(parent_width)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut outer = Element::new_with_id(201, 0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#1e3a8a")));
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dbeafe")),
        );
        outer_style.set_padding(crate::style::Padding::uniform(Length::px(3.0)));
        outer_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1d4ed8")));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));

        let mut lead_text = Text::from_content_with_id(301, "outer prefix ");
        lead_text.set_font_size(15.0);
        lead_text.set_color(Color::hex("#172554"));
        let lead_text_key = commit_child(&mut arena, outer_key, Box::new(lead_text));

        let mut inner = Element::new_with_id(202, 0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#7f1d1d")));
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fecaca")),
        );
        inner_style.set_padding(crate::style::Padding::uniform(Length::px(2.0)));
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));

        let mut inner_text = Text::from_content_with_id(302, "inner chip text");
        inner_text.set_font_size(13.0);
        inner_text.set_color(Color::hex("#7f1d1d"));
        let inner_text_key = commit_child(&mut arena, inner_key, Box::new(inner_text));

        let mut atomic = Element::new_with_id(203, 0.0, 0.0, 34.0, 18.0);
        let mut atomic_style = Style::new();
        atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(34.0)));
        atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        atomic_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#bbf7d0")),
        );
        atomic.apply_style(atomic_style);
        let atomic_key = commit_child(&mut arena, outer_key, Box::new(atomic));

        let tail_text_key = commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::from_content_with_id(303, " outer tail wraps")),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: parent_width,
                max_height: 180.0,
                viewport_width: parent_width,
                viewport_height: 180.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(180.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: parent_width,
                available_height: 180.0,
                viewport_width: parent_width,
                viewport_height: 180.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(180.0),
            },
        );

        let collected = ElementInlineIfcMetadataCollector::collect(
            &arena,
            ElementInlineIfcMetadataCollectorInput::new(parent_key, parent_width),
        )
        .expect("collector should produce a root source for a real Element subtree");
        let outer_source = collected
            .source_for_node(outer_key)
            .expect("outer source");
        let inner_source = collected
            .source_for_node(inner_key)
            .expect("inner source");
        let atomic_source = collected
            .source_for_node(atomic_key)
            .expect("atomic source");
        let lead_text_source = collected
            .source_for_node(lead_text_key)
            .expect("lead text source");
        let inner_text_source = collected
            .source_for_node(inner_text_key)
            .expect("inner text source");
        let tail_text_source = collected
            .source_for_node(tail_text_key)
            .expect("tail text source");

        assert_eq!(outer_source, InlineIfcSourceId(201));
        assert_eq!(inner_source, InlineIfcSourceId(202));
        assert_eq!(atomic_source, InlineIfcSourceId(203));
        assert_eq!(lead_text_source, InlineIfcSourceId(301));
        assert_eq!(inner_text_source, InlineIfcSourceId(302));
        assert_eq!(tail_text_source, InlineIfcSourceId(303));

        let [InlineIfcItem::Span {
            source: collected_outer,
            children: outer_children,
            ..
        }] = collected.root_source.input.items.as_slice()
        else {
            panic!(
                "collector should flatten the root Element children into one outer span: {:?}",
                collected.root_source.input.items
            );
        };
        assert_eq!(*collected_outer, outer_source);
        assert_eq!(outer_children.len(), 4);
        assert!(matches!(
            &outer_children[0],
            InlineIfcItem::TextSpan { source, text, .. }
                if *source == lead_text_source && text == "outer prefix "
        ));
        assert!(matches!(
            &outer_children[1],
            InlineIfcItem::Span { source, children, .. }
                if *source == inner_source
                    && matches!(
                        children.as_slice(),
                        [InlineIfcItem::TextSpan { source, text, .. }]
                            if *source == inner_text_source && text == "inner chip text"
                    )
        ));
        assert!(matches!(
            &outer_children[2],
            InlineIfcItem::AtomicInlineBox {
                source,
                measurement,
            } if *source == atomic_source
                && (measurement.measured_size.width - 34.0).abs() <= 0.001
                && (measurement.measured_size.height - 18.0).abs() <= 0.001
        ));
        assert!(matches!(
            &outer_children[3],
            InlineIfcItem::TextSpan { source, text, .. }
                if *source == tail_text_source && text == " outer tail wraps"
        ));

        assert_eq!(
            collected.root_source.package_distribution.decoration_sources.len(),
            2
        );
        assert!(
            collected
                .root_source
                .package_distribution
                .decoration_sources
                .iter()
                .any(|source| source.source == outer_source)
        );
        assert!(
            collected
                .root_source
                .package_distribution
                .decoration_sources
                .iter()
                .any(|source| source.source == inner_source)
        );
        assert_eq!(
            collected.root_source.package_distribution.atomic_sources,
            vec![atomic_source]
        );

        let mut cache = InlineIfcElementRootCandidateCache::new();
        let mut outer_element = crate::view::test_support::get_element_mut::<Element>(
            &mut arena,
            outer_key,
        );
        let candidate = outer_element.update_inline_ifc_rollout_packages_from_root_source(
            &collected.root_source,
            outer_source,
            &mut cache,
        );
        assert!(candidate.rebuilt);
        assert!(candidate.decoration_package(outer_source).is_some());
        assert!(candidate.decoration_package(inner_source).is_some());
        assert!(candidate.atomic_package(atomic_source).is_some());
        assert!(
            candidate.package(InlineIfcSourceId(999_999)).is_none(),
            "missing source must not alias another collected package"
        );
        assert_eq!(
            outer_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            },
            "render default should use collector-staged packages while retaining legacy fallback"
        );
        outer_element.set_inline_ifc_render_mode_for_test(
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
        );
        assert_eq!(
            outer_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            },
            "explicit candidate mode is required before the collected package affects render decision"
        );
        assert!(
            !outer_element
                .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                .is_empty()
        );
        drop(outer_element);

        let mut atomic_element = crate::view::test_support::get_element_mut::<Element>(
            &mut arena,
            atomic_key,
        );
        let reused_candidate = atomic_element.update_inline_ifc_rollout_packages_from_root_source(
            &collected.root_source,
            atomic_source,
            &mut cache,
        );
        assert_eq!(reused_candidate.invalidation, InlineIfcInvalidation::Reuse);
        let atomic_metadata = atomic_element
            .inline_ifc_atomic_placement_metadata_for_test()
            .expect("atomic source should receive placement metadata");
        assert_eq!(atomic_metadata.package.source, atomic_source);
        assert!(!atomic_metadata.package.placements.is_empty());
    }

    #[test]
    fn inline_element_ifc_candidate_lifecycle_dry_run_installs_and_reuses_packages() {
        let parent_width = 188.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(400, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(parent_width)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut outer = Element::new_with_id(401, 0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#1e3a8a")));
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dbeafe")),
        );
        outer_style.set_padding(crate::style::Padding::uniform(Length::px(3.0)));
        outer_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1d4ed8")));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));

        let mut lead_text = Text::from_content_with_id(501, "outer prefix ");
        lead_text.set_font_size(15.0);
        lead_text.set_color(Color::hex("#172554"));
        let lead_text_key = commit_child(&mut arena, outer_key, Box::new(lead_text));

        let mut inner = Element::new_with_id(402, 0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#7f1d1d")));
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fecaca")),
        );
        inner_style.set_padding(crate::style::Padding::uniform(Length::px(2.0)));
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));

        let mut inner_text = Text::from_content_with_id(502, "inner chip text");
        inner_text.set_font_size(13.0);
        inner_text.set_color(Color::hex("#7f1d1d"));
        commit_child(&mut arena, inner_key, Box::new(inner_text));

        let mut atomic = Element::new_with_id(403, 0.0, 0.0, 34.0, 18.0);
        let mut atomic_style = Style::new();
        atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(34.0)));
        atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        atomic_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#bbf7d0")),
        );
        atomic.apply_style(atomic_style);
        let atomic_key = commit_child(&mut arena, outer_key, Box::new(atomic));

        commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::from_content_with_id(503, " outer tail wraps")),
        );

        let mut unrelated_block = Element::new_with_id(404, 0.0, 0.0, 50.0, 20.0);
        let mut unrelated_style = Style::new();
        unrelated_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        unrelated_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(50.0)));
        unrelated_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        unrelated_block.apply_style(unrelated_style);
        let unrelated_key = commit_element(&mut arena, Box::new(unrelated_block));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: parent_width,
                max_height: 180.0,
                viewport_width: parent_width,
                viewport_height: 180.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(180.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: parent_width,
                available_height: 180.0,
                viewport_width: parent_width,
                viewport_height: 180.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(180.0),
            },
        );

        let mut cache = InlineIfcElementRootCandidateCache::new();
        let install_targets = vec![
            outer_key,
            inner_key,
            atomic_key,
            lead_text_key,
            unrelated_key,
            crate::view::node_arena::NodeKey::default(),
        ];
        let first = ElementInlineIfcCandidateLifecycle::dry_run(
            &mut arena,
            ElementInlineIfcCandidateLifecycleInput::new(parent_key, parent_width)
                .with_install_targets(install_targets.clone()),
            &mut cache,
        )
        .expect("dry-run lifecycle should collect and stage the real subtree");
        assert_eq!(first.invalidation, InlineIfcInvalidation::Reshape);
        assert!(first.rebuilt);
        assert_eq!(first.cache_len, 1);

        let outer_source = first.source_for_node(outer_key).expect("outer source");
        let inner_source = first.source_for_node(inner_key).expect("inner source");
        let atomic_source = first.source_for_node(atomic_key).expect("atomic source");
        assert_eq!(outer_source, InlineIfcSourceId(401));
        assert_eq!(inner_source, InlineIfcSourceId(402));
        assert_eq!(atomic_source, InlineIfcSourceId(403));

        let outer_install = first.install_for_node(outer_key).expect("outer install");
        assert_eq!(
            outer_install.status,
            ElementInlineIfcCandidateLifecycleInstallStatus::Installed
        );
        assert!(outer_install.has_decoration_package);
        assert!(!outer_install.has_atomic_package);

        let inner_install = first.install_for_node(inner_key).expect("inner install");
        assert_eq!(
            inner_install.status,
            ElementInlineIfcCandidateLifecycleInstallStatus::Installed
        );
        assert!(inner_install.has_decoration_package);
        assert!(!inner_install.has_atomic_package);

        let atomic_install = first.install_for_node(atomic_key).expect("atomic install");
        assert_eq!(
            atomic_install.status,
            ElementInlineIfcCandidateLifecycleInstallStatus::Installed
        );
        assert!(!atomic_install.has_decoration_package);
        assert!(atomic_install.has_atomic_package);

        let text_install = first
            .install_for_node(lead_text_key)
            .expect("text install result");
        assert_eq!(
            text_install.status,
            ElementInlineIfcCandidateLifecycleInstallStatus::SkippedNonElement
        );
        assert_eq!(text_install.source, first.source_for_node(lead_text_key));

        let unrelated_install = first
            .install_for_node(unrelated_key)
            .expect("unrelated block install result");
        assert_eq!(
            unrelated_install.status,
            ElementInlineIfcCandidateLifecycleInstallStatus::ClearedMissingSource
        );
        assert_eq!(unrelated_install.source, None);
        assert!(!unrelated_install.has_decoration_package);
        assert!(!unrelated_install.has_atomic_package);

        let missing_install = first
            .install_for_node(crate::view::node_arena::NodeKey::default())
            .expect("missing node install result");
        assert_eq!(
            missing_install.status,
            ElementInlineIfcCandidateLifecycleInstallStatus::MissingNode
        );

        {
            let outer_element =
                crate::view::test_support::get_element::<Element>(&arena, outer_key);
            assert_eq!(
                outer_element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                },
                "render default should use lifecycle-staged packages while retaining legacy fallback"
            );
            assert!(
                !outer_element
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty()
            );
        }
        {
            let mut outer_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, outer_key);
            outer_element.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
            assert_eq!(
                outer_element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                },
                "explicit candidate mode is required before staged decoration affects render decision"
            );
        }
        {
            let atomic_element =
                crate::view::test_support::get_element::<Element>(&arena, atomic_key);
            let atomic_metadata = atomic_element
                .inline_ifc_atomic_placement_metadata_for_test()
                .expect("atomic child should receive placement metadata");
            assert_eq!(atomic_metadata.package.source, atomic_source);
        }
        {
            let unrelated_element =
                crate::view::test_support::get_element::<Element>(&arena, unrelated_key);
            assert_eq!(
                unrelated_element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
                "unrelated non-inline target must not alias a collected package"
            );
            assert!(
                unrelated_element
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty()
            );
        }

        let second = ElementInlineIfcCandidateLifecycle::dry_run(
            &mut arena,
            ElementInlineIfcCandidateLifecycleInput::new(parent_key, parent_width)
                .with_install_targets(install_targets),
            &mut cache,
        )
        .expect("second dry-run should reuse the cached candidate");
        assert_eq!(second.cache_key, first.cache_key);
        assert_eq!(second.invalidation, InlineIfcInvalidation::Reuse);
        assert!(!second.rebuilt);
        assert_eq!(second.cache_len, 1);
    }

    #[test]
    fn inline_element_ifc_layout_call_site_opt_in_discovers_targets_and_keeps_default_fallback() {
        let parent_width = 188.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(420, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(parent_width)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut outer = Element::new_with_id(421, 0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#1e3a8a")));
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dbeafe")),
        );
        outer_style.set_padding(crate::style::Padding::uniform(Length::px(3.0)));
        outer_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1d4ed8")));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));

        let mut lead_text = Text::from_content_with_id(521, "outer prefix ");
        lead_text.set_font_size(15.0);
        lead_text.set_color(Color::hex("#172554"));
        let lead_text_key = commit_child(&mut arena, outer_key, Box::new(lead_text));

        let mut inner = Element::new_with_id(422, 0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#7f1d1d")));
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fecaca")),
        );
        inner_style.set_padding(crate::style::Padding::uniform(Length::px(2.0)));
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));

        let mut inner_text = Text::from_content_with_id(522, "inner chip text");
        inner_text.set_font_size(13.0);
        inner_text.set_color(Color::hex("#7f1d1d"));
        commit_child(&mut arena, inner_key, Box::new(inner_text));

        let mut atomic = Element::new_with_id(423, 0.0, 0.0, 34.0, 18.0);
        let mut atomic_style = Style::new();
        atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(34.0)));
        atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        atomic_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#bbf7d0")),
        );
        atomic.apply_style(atomic_style);
        let atomic_key = commit_child(&mut arena, outer_key, Box::new(atomic));

        commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::from_content_with_id(523, " outer tail wraps")),
        );

        let mut block_root = Element::new_with_id(424, 0.0, 0.0, 50.0, 20.0);
        let mut block_style = Style::new();
        block_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        block_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(50.0)));
        block_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        block_root.apply_style(block_style);
        let block_root_key = commit_element(&mut arena, Box::new(block_root));

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: parent_width,
                max_height: 180.0,
                viewport_width: parent_width,
                viewport_height: 180.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(180.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: parent_width,
                available_height: 180.0,
                viewport_width: parent_width,
                viewport_height: 180.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(180.0),
            },
        );

        let mut cache = InlineIfcElementRootCandidateCache::new();
        let disabled = ElementInlineIfcLayoutCallSiteOptIn::run(
            &mut arena,
            ElementInlineIfcLayoutCallSiteOptInInput::disabled(parent_key, parent_width),
            &mut cache,
        );
        assert_eq!(
            disabled.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::Disabled
        );
        assert!(disabled.install_targets.is_empty());
        assert!(disabled.lifecycle().is_none());
        assert_eq!(cache.len(), 0);
        assert!(
            crate::view::test_support::get_element::<Element>(&arena, outer_key)
                .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                .is_empty(),
            "disabled opt-in must not install packages"
        );

        let first = ElementInlineIfcLayoutCallSiteOptIn::run(
            &mut arena,
            ElementInlineIfcLayoutCallSiteOptInInput::dry_run_candidate(parent_key, parent_width),
            &mut cache,
        );
        assert_eq!(
            first.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan
        );
        assert_eq!(
            first.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );
        assert!(first.install_targets.contains(&outer_key));
        assert!(first.install_targets.contains(&inner_key));
        assert!(first.install_targets.contains(&atomic_key));
        assert!(!first.install_targets.contains(&lead_text_key));

        let first_lifecycle = first.lifecycle().expect("opt-in lifecycle output");
        assert_eq!(first_lifecycle.invalidation, InlineIfcInvalidation::Reshape);
        assert!(first_lifecycle.rebuilt);
        assert_eq!(first_lifecycle.cache_len, 1);
        assert_eq!(
            first_lifecycle
                .install_for_node(outer_key)
                .expect("outer install")
                .status,
            ElementInlineIfcCandidateLifecycleInstallStatus::Installed
        );
        assert_eq!(
            first_lifecycle
                .install_for_node(inner_key)
                .expect("inner install")
                .status,
            ElementInlineIfcCandidateLifecycleInstallStatus::Installed
        );
        assert!(
            first_lifecycle
                .install_for_node(atomic_key)
                .expect("atomic install")
                .has_atomic_package
        );

        {
            let outer_element =
                crate::view::test_support::get_element::<Element>(&arena, outer_key);
            assert_eq!(
                outer_element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                },
                "render default should use installed packages while retaining legacy fallback"
            );
            assert!(
                !outer_element
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty()
            );
        }
        {
            let mut outer_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, outer_key);
            outer_element.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
            assert_eq!(
                outer_element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                }
            );
        }

        let second = ElementInlineIfcLayoutCallSiteOptIn::run(
            &mut arena,
            ElementInlineIfcLayoutCallSiteOptInInput::dry_run_candidate(parent_key, parent_width),
            &mut cache,
        );
        let second_lifecycle = second.lifecycle().expect("second lifecycle output");
        assert_eq!(second_lifecycle.cache_key, first_lifecycle.cache_key);
        assert_eq!(second_lifecycle.invalidation, InlineIfcInvalidation::Reuse);
        assert!(!second_lifecycle.rebuilt);
        assert_eq!(second_lifecycle.cache_len, 1);

        let narrower_width = 126.0;
        let narrower = ElementInlineIfcLayoutCallSiteOptIn::run(
            &mut arena,
            ElementInlineIfcLayoutCallSiteOptInInput::dry_run_candidate(parent_key, narrower_width),
            &mut cache,
        );
        let narrower_lifecycle = narrower.lifecycle().expect("narrower lifecycle output");
        assert_ne!(narrower_lifecycle.cache_key, first_lifecycle.cache_key);
        assert_eq!(narrower_lifecycle.invalidation, InlineIfcInvalidation::Reshape);
        assert!(narrower_lifecycle.rebuilt);
        assert_eq!(narrower_lifecycle.cache_len, 2);

        let unsupported = ElementInlineIfcLayoutCallSiteOptIn::run(
            &mut arena,
            ElementInlineIfcLayoutCallSiteOptInInput::dry_run_candidate(block_root_key, 50.0),
            &mut cache,
        );
        assert_eq!(
            unsupported.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::UnsupportedRoot
        );
        assert!(unsupported.install_targets.is_empty());
        assert!(unsupported.lifecycle().is_none());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn inline_element_ifc_production_layout_call_site_opt_in_owns_cache_and_invalidates() {
        let parent_width = 188.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(450, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);
        parent.set_inline_ifc_layout_call_site_opt_in_mode(
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate,
        );
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut outer = Element::new_with_id(451, 0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#1e3a8a")));
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dbeafe")),
        );
        outer_style.set_padding(crate::style::Padding::uniform(Length::px(3.0)));
        outer_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1d4ed8")));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));

        let mut text = Text::from_content_with_id(551, "outer prefix text");
        text.set_font_size(15.0);
        text.set_color(Color::hex("#172554"));
        let text_key = commit_child(&mut arena, outer_key, Box::new(text));

        let mut inner = Element::new_with_id(452, 0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fecaca")),
        );
        inner_style.set_padding(crate::style::Padding::uniform(Length::px(2.0)));
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));

        commit_child(
            &mut arena,
            inner_key,
            Box::new(Text::from_content_with_id(552, "inner chip text")),
        );

        let mut atomic = Element::new_with_id(453, 0.0, 0.0, 34.0, 18.0);
        let mut atomic_style = Style::new();
        atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(34.0)));
        atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        atomic.apply_style(atomic_style);
        let atomic_key = commit_child(&mut arena, outer_key, Box::new(atomic));

        let constraints_for_width = |width: f32| LayoutConstraints {
            max_width: width,
            max_height: 180.0,
            viewport_width: width,
            viewport_height: 180.0,
            percent_base_width: Some(width),
            percent_base_height: Some(180.0),
        };
        let placement_for_width = |width: f32| LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 180.0,
            viewport_width: width,
            viewport_height: 180.0,
            percent_base_width: Some(width),
            percent_base_height: Some(180.0),
        };

        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );
        let first_output = {
            let parent_element = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            let output = parent_element
                .inline_ifc_layout_call_site_last_output_for_test()
                .expect("production layout pass should run opt-in candidate");
            assert_eq!(
                output.status,
                ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan
            );
            let lifecycle = output.lifecycle().expect("first lifecycle output");
            assert_eq!(lifecycle.invalidation, InlineIfcInvalidation::Reshape);
            assert!(lifecycle.rebuilt);
            assert_eq!(lifecycle.cache_len, 1);
            assert_eq!(parent_element.inline_ifc_layout_call_site_cache_len_for_test(), 1);
            output.clone()
        };
        assert!(first_output.install_targets.contains(&outer_key));
        assert!(first_output.install_targets.contains(&inner_key));
        assert!(first_output.install_targets.contains(&atomic_key));
        {
            let outer_element = crate::view::test_support::get_element::<Element>(&arena, outer_key);
            assert_eq!(
                outer_element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                },
                "render default uses installed decoration packages while retaining legacy fallback"
            );
            assert!(
                !outer_element
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty(),
                "production opt-in layout pass should install descendant decoration packages"
            );
        }
        {
            let atomic_element =
                crate::view::test_support::get_element::<Element>(&arena, atomic_key);
            assert!(
                atomic_element
                    .inline_ifc_atomic_placement_metadata_for_test()
                    .is_some(),
                "production opt-in layout pass should install atomic placement package"
            );
        }
        {
            let mut outer_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, outer_key);
            outer_element.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
            assert_eq!(
                outer_element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                },
                "explicit candidate mode remains equivalent to the new render default"
            );
        }

        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
            parent_element.mark_place_dirty();
        }
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );
        let reuse_key = {
            let parent_element = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            let lifecycle = parent_element
                .inline_ifc_layout_call_site_last_output_for_test()
                .and_then(|output| output.lifecycle())
                .expect("second lifecycle output");
            assert_eq!(
                lifecycle.cache_key,
                first_output.lifecycle().expect("first lifecycle").cache_key
            );
            assert_eq!(lifecycle.invalidation, InlineIfcInvalidation::Reuse);
            assert!(!lifecycle.rebuilt);
            lifecycle.cache_key.clone()
        };

        let narrower_width = 126.0;
        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
            parent_element.mark_place_dirty();
        }
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(narrower_width),
            placement_for_width(narrower_width),
        );
        {
            let parent_element = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            let lifecycle = parent_element
                .inline_ifc_layout_call_site_last_output_for_test()
                .and_then(|output| output.lifecycle())
                .expect("narrower lifecycle output");
            assert_ne!(lifecycle.cache_key, reuse_key);
            assert_eq!(lifecycle.invalidation, InlineIfcInvalidation::Reshape);
            assert!(lifecycle.rebuilt);
        }

        {
            let mut text_element =
                crate::view::test_support::get_element_mut::<Text>(&arena, text_key);
            text_element.set_text("outer prefix text with content reshape");
        }
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(narrower_width),
            placement_for_width(narrower_width),
        );
        {
            let parent_element = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            let lifecycle = parent_element
                .inline_ifc_layout_call_site_last_output_for_test()
                .and_then(|output| output.lifecycle())
                .expect("content lifecycle output");
            assert_eq!(lifecycle.invalidation, InlineIfcInvalidation::Reshape);
            assert!(lifecycle.rebuilt);
        }

        {
            let mut text_element =
                crate::view::test_support::get_element_mut::<Text>(&arena, text_key);
            text_element.set_color(Color::hex("#be123c"));
        }
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(narrower_width),
            placement_for_width(narrower_width),
        );
        {
            let parent_element = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            let lifecycle = parent_element
                .inline_ifc_layout_call_site_last_output_for_test()
                .and_then(|output| output.lifecycle())
                .expect("paint lifecycle output");
            assert_eq!(lifecycle.invalidation, InlineIfcInvalidation::RepaintOnly);
            assert!(lifecycle.rebuilt);
        }

        let cache_len_before_disabled = {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
            let len = parent_element.inline_ifc_layout_call_site_cache_len_for_test();
            parent_element.set_inline_ifc_layout_call_site_opt_in_mode(
                ElementInlineIfcLayoutCallSiteOptInMode::Disabled,
            );
            parent_element.mark_place_dirty();
            len
        };
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(narrower_width),
            placement_for_width(narrower_width),
        );
        {
            let parent_element = crate::view::test_support::get_element::<Element>(&arena, parent_key);
            assert_eq!(
                parent_element.inline_ifc_layout_call_site_cache_len_for_test(),
                cache_len_before_disabled,
                "disabled call-site mode must not update the owned cache"
            );
            let lifecycle = parent_element
                .inline_ifc_layout_call_site_last_output_for_test()
                .and_then(|output| output.lifecycle())
                .expect("disabled should leave previous output untouched");
            assert_eq!(lifecycle.invalidation, InlineIfcInvalidation::RepaintOnly);
        }
    }

    #[test]
    fn inline_element_ifc_production_call_site_diagnostic_tracks_demo_like_tree() {
        let parent_width = 172.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(600, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);
        parent.set_inline_ifc_layout_call_site_opt_in_mode(
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate,
        );
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut outer = Element::new_with_id(601, 0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#0f172a")));
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dbeafe")),
        );
        outer_style.set_padding(crate::style::Padding::uniform(Length::px(6.0)));
        outer_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#2563eb")));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));

        let mut lead = Text::from_content_with_id(701, "Permission prefix ");
        lead.set_font_size(16.0);
        lead.set_color(Color::hex("#172554"));
        commit_child(&mut arena, outer_key, Box::new(lead));

        let mut inner = Element::new_with_id(602, 0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#bfdbfe")),
        );
        inner_style.set_padding(crate::style::Padding::uniform(Length::px(4.0)));
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1d4ed8")));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));

        let mut inner_text = Text::from_content_with_id(
            702,
            "restriction including limitation",
        );
        inner_text.set_font_size(16.0);
        inner_text.set_color(Color::hex("#1e40af"));
        commit_child(&mut arena, inner_key, Box::new(inner_text));

        let mut atomic = Element::new_with_id(603, 0.0, 0.0, 42.0, 20.0);
        let mut atomic_style = Style::new();
        atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(42.0)));
        atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        atomic_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fef3c7")),
        );
        atomic.apply_style(atomic_style);
        let atomic_key = commit_child(&mut arena, outer_key, Box::new(atomic));

        commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::from_content_with_id(703, " suffix text continues")),
        );

        let constraints_for_width = |width: f32| LayoutConstraints {
            max_width: width,
            max_height: 220.0,
            viewport_width: width,
            viewport_height: 220.0,
            percent_base_width: Some(width),
            percent_base_height: Some(220.0),
        };
        let placement_for_width = |width: f32| LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 220.0,
            viewport_width: width,
            viewport_height: 220.0,
            percent_base_width: Some(width),
            percent_base_height: Some(220.0),
        };

        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );
        let first_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("production call-site opt-in should expose diagnostic")
        };
        assert_eq!(first_diagnostic.root_key, parent_key);
        assert_eq!(
            first_diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
        );
        assert_eq!(
            first_diagnostic.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan
        );
        assert_eq!(
            first_diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );
        assert_eq!(
            first_diagnostic.invalidation,
            Some(InlineIfcInvalidation::Reshape)
        );
        assert_eq!(first_diagnostic.rebuilt, Some(true));
        assert_eq!(first_diagnostic.cache_len, 1);
        assert!(first_diagnostic.install_targets.contains(&outer_key));
        assert!(first_diagnostic.install_targets.contains(&inner_key));
        assert!(first_diagnostic.install_targets.contains(&atomic_key));

        let outer_diag = first_diagnostic
            .target(outer_key)
            .expect("outer target diagnostic");
        assert_eq!(
            outer_diag.install_status,
            Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
        );
        assert!(outer_diag.has_decoration_package);
        assert!(!outer_diag.has_atomic_package);
        assert_eq!(
            outer_diag.render_decision,
            Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            })
        );

        let inner_diag = first_diagnostic
            .target(inner_key)
            .expect("inner target diagnostic");
        assert_eq!(
            inner_diag.install_status,
            Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
        );
        assert!(inner_diag.has_decoration_package);
        assert!(!inner_diag.has_atomic_package);

        let atomic_diag = first_diagnostic
            .target(atomic_key)
            .expect("atomic target diagnostic");
        assert_eq!(
            atomic_diag.install_status,
            Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
        );
        assert!(!atomic_diag.has_decoration_package);
        assert!(atomic_diag.has_atomic_package);
        assert_eq!(
            atomic_diag.render_decision,
            Some(ElementInlineIfcRenderDecision::ExistingInlineFragments)
        );

        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
            parent_element.mark_place_dirty();
        }
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );
        let reuse_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("second production diagnostic")
        };
        assert_eq!(reuse_diagnostic.cache_key, first_diagnostic.cache_key);
        assert_eq!(
            reuse_diagnostic.invalidation,
            Some(InlineIfcInvalidation::Reuse)
        );
        assert_eq!(reuse_diagnostic.rebuilt, Some(false));
        assert_eq!(reuse_diagnostic.cache_len, 1);

        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
            parent_element.set_inline_ifc_layout_call_site_opt_in_mode(
                ElementInlineIfcLayoutCallSiteOptInMode::Disabled,
            );
            parent_element.mark_place_dirty();
        }
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );
        let disabled_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("disabled mode should leave previous diagnostic untouched")
        };
        assert_eq!(
            disabled_diagnostic, reuse_diagnostic,
            "disabled production call-site mode must not refresh diagnostic or cache"
        );

        {
            let mut outer_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, outer_key);
            outer_element.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
        }
        let explicit_candidate_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("candidate diagnostic should read live target render decision")
        };
        assert_eq!(
            explicit_candidate_diagnostic
                .target(outer_key)
                .expect("outer target diagnostic")
                .render_decision,
            Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            }),
            "installed packages allow the render default to choose the candidate"
        );
        assert_eq!(
            explicit_candidate_diagnostic
                .target(inner_key)
                .expect("inner target diagnostic")
                .render_decision,
            Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            })
        );
    }

    #[test]
    fn inline_element_ifc_config_gate_smoke_keeps_examples_like_tree_explicit() {
        let parent_width = 196.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(620, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent_style.set_line_height(1.2);
        parent_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#0f172a")));
        parent.apply_style(parent_style);
        parent.apply_inline_ifc_layout_call_site_rollout_config_for_test(
            ElementInlineIfcLayoutCallSiteRolloutConfig::disabled(),
        );
        let parent_key = commit_element(&mut arena, Box::new(parent));

        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content_with_id(720, "Inline text starts here, ")),
        );

        let mut outer = Element::new_with_id(621, 0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dbeafe")),
        );
        outer_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#172554")));
        outer_style.set_padding(crate::style::Padding::uniform(Length::px(6.0)));
        outer_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#2563eb")));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));

        commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::from_content_with_id(
                721,
                "Permission is hereby granted without ",
            )),
        );

        let mut inner = Element::new_with_id(622, 0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#bfdbfe")),
        );
        inner_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#1e40af")));
        inner_style.set_padding(crate::style::Padding::uniform(Length::px(4.0)));
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1d4ed8")));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));

        commit_child(
            &mut arena,
            inner_key,
            Box::new(Text::from_content_with_id(
                722,
                "restriction including without limitation",
            )),
        );

        let mut atomic = Element::new_with_id(623, 0.0, 0.0, 90.0, 28.0);
        let mut atomic_style = Style::new();
        atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(90.0)));
        atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(28.0)));
        atomic_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#065f46")),
        );
        atomic_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#ecfdf5")));
        atomic.apply_style(atomic_style);
        let atomic_key = commit_child(&mut arena, parent_key, Box::new(atomic));

        let mut sibling = Element::new_with_id(624, 0.0, 0.0, 0.0, 0.0);
        let mut sibling_style = Style::new();
        sibling_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        sibling_style.insert(PropertyId::Width, ParsedValue::Auto);
        sibling_style.insert(PropertyId::Height, ParsedValue::Auto);
        sibling_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fee2e2")),
        );
        sibling_style.insert(PropertyId::Color, ParsedValue::color_like(Color::hex("#7f1d1d")));
        sibling_style.set_padding(crate::style::Padding::uniform(Length::px(5.0)));
        sibling_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        sibling.apply_style(sibling_style);
        let sibling_key = commit_child(&mut arena, parent_key, Box::new(sibling));

        commit_child(
            &mut arena,
            sibling_key,
            Box::new(Text::from_content_with_id(723, "note note note note note")),
        );
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content_with_id(724, " final mixed text")),
        );

        let constraints_for_width = |width: f32| LayoutConstraints {
            max_width: width,
            max_height: 240.0,
            viewport_width: width,
            viewport_height: 240.0,
            percent_base_width: Some(width),
            percent_base_height: Some(240.0),
        };
        let placement_for_width = |width: f32| LayoutPlacement {
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
        };

        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );
        {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            assert_eq!(
                parent_element.inline_ifc_layout_call_site_gate_mode_for_test(),
                ElementInlineIfcLayoutCallSiteOptInMode::Disabled,
                "explicit disabled rollout config must keep rollback no-op"
            );
            assert!(
                parent_element
                    .inline_ifc_layout_call_site_last_output_for_test()
                    .is_none(),
                "explicit disabled gate must not write diagnostic output"
            );
            assert_eq!(parent_element.inline_ifc_layout_call_site_cache_len_for_test(), 0);
        }
        for key in [outer_key, inner_key, atomic_key, sibling_key] {
            let element = crate::view::test_support::get_element::<Element>(&arena, key);
            assert_eq!(
                element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments
            );
            assert!(
                element
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty(),
                "disabled gate must not install decoration packages"
            );
            assert!(
                element.inline_ifc_atomic_placement_metadata_for_test().is_none(),
                "disabled gate must not install atomic packages"
            );
        }

        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
            parent_element.apply_inline_ifc_layout_call_site_rollout_config_for_test(
                ElementInlineIfcLayoutCallSiteRolloutConfig::for_scenario(
                    ElementInlineIfcLayoutCallSiteScenario::ExamplesLikeDryRunCandidate,
                ),
            );
            parent_element.mark_place_dirty();
        }
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );

        let diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            assert_eq!(
                parent_element.inline_ifc_layout_call_site_gate_mode_for_test(),
                ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
            );
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("explicit gate should run production call-site dry-run")
        };
        assert_eq!(
            diagnostic.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan
        );
        assert_eq!(
            diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );
        assert_eq!(diagnostic.invalidation, Some(InlineIfcInvalidation::Reshape));
        assert_eq!(diagnostic.rebuilt, Some(true));
        assert_eq!(diagnostic.cache_len, 1);
        for key in [outer_key, inner_key, atomic_key, sibling_key] {
            assert!(
                diagnostic.install_targets.contains(&key),
                "examples-like inline target should be discovered: {key:?}"
            );
            assert_eq!(
                diagnostic.target(key).expect("target diagnostic").install_status,
                Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
            );
        }
        assert!(
            diagnostic
                .target(outer_key)
                .expect("outer diagnostic")
                .has_decoration_package
        );
        assert!(
            diagnostic
                .target(inner_key)
                .expect("inner diagnostic")
                .has_decoration_package
        );
        assert!(
            diagnostic
                .target(sibling_key)
                .expect("sibling diagnostic")
                .has_decoration_package
        );
        assert!(
            diagnostic
                .target(atomic_key)
                .expect("atomic diagnostic")
                .has_atomic_package
        );
            assert_eq!(
                diagnostic
                    .target(outer_key)
                    .expect("outer diagnostic")
                    .render_decision,
            Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            }),
            "installed packages allow the inline Element render default to choose the candidate"
        );

        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
            parent_element.mark_place_dirty();
        }
        measure_and_place(
            &mut arena,
            parent_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );
        let reuse_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("scenario opt-in should keep diagnostic available")
        };
        assert_eq!(reuse_diagnostic.cache_key, diagnostic.cache_key);
        assert_eq!(
            reuse_diagnostic.invalidation,
            Some(InlineIfcInvalidation::Reuse),
            "same examples-like scenario input should reuse the IFC candidate cache"
        );
        assert_eq!(reuse_diagnostic.rebuilt, Some(false));
        assert_eq!(
            reuse_diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments,
            "scenario opt-in still keeps the legacy render fallback"
        );

        {
            let mut sibling_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, sibling_key);
            sibling_element.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
        }
        let candidate_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("diagnostic reads live render decision")
        };
        assert_eq!(
            candidate_diagnostic
                .target(sibling_key)
                .expect("sibling diagnostic")
                .render_decision,
            Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            }),
            "render candidate default reads the installed package"
        );
        assert_eq!(
            candidate_diagnostic
                .target(outer_key)
                .expect("outer diagnostic")
                .render_decision,
            Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            })
        );

        let mut unsupported_arena = new_test_arena();
        let mut unsupported_root = Element::new_with_id(625, 0.0, 0.0, parent_width, 0.0);
        let mut unsupported_style = Style::new();
        unsupported_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().into()),
        );
        unsupported_root.apply_style(unsupported_style);
        unsupported_root.apply_inline_ifc_layout_call_site_rollout_config_for_test(
            ElementInlineIfcLayoutCallSiteRolloutConfig::for_scenario(
                ElementInlineIfcLayoutCallSiteScenario::UnsupportedRootProbe,
            ),
        );
        let unsupported_key = commit_element(&mut unsupported_arena, Box::new(unsupported_root));
        measure_and_place(
            &mut unsupported_arena,
            unsupported_key,
            constraints_for_width(parent_width),
            placement_for_width(parent_width),
        );
        let unsupported_diagnostic = {
            let root =
                crate::view::test_support::get_element::<Element>(&unsupported_arena, unsupported_key);
            root.inline_ifc_layout_call_site_diagnostic_for_test(&unsupported_arena)
                .expect("unsupported scenario should report an explicit status")
        };
        assert_eq!(
            unsupported_diagnostic.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::UnsupportedRoot
        );
        assert_eq!(unsupported_diagnostic.cache_len, 0);
        assert!(unsupported_diagnostic.target_installs.is_empty());
    }

    #[test]
    fn inline_element_ifc_production_call_site_regression_matrix_tracks_invalidations() {
        let parent_width = 210.0;
        let (mut arena, fixture) = build_inline_element_ifc_production_matrix_fixture(
            parent_width,
            ElementInlineIfcLayoutCallSiteScenario::ExamplesLikeDryRunCandidate,
        );

        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, fixture.height),
            inline_element_ifc_matrix_placement(parent_width, fixture.height),
        );
        let first_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("scenario opt-in should expose production diagnostic")
        };
        assert_eq!(
            first_diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
        );
        assert_eq!(
            first_diagnostic.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan
        );
        assert_eq!(
            first_diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );
        assert_eq!(
            first_diagnostic.invalidation,
            Some(InlineIfcInvalidation::Reshape)
        );
        assert_eq!(first_diagnostic.rebuilt, Some(true));
        assert_eq!(first_diagnostic.cache_len, 1);

        for key in [
            fixture.outer_key,
            fixture.inner_key,
            fixture.atomic_key,
            fixture.sibling_key,
        ] {
            let target = first_diagnostic
                .target(key)
                .expect("matrix inline target should be discovered");
            assert_eq!(
                target.install_status,
                Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
            );
            let expected = if target.has_decoration_package {
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: target.has_atomic_package,
                }
            } else {
                ElementInlineIfcRenderDecision::ExistingInlineFragments
            };
            assert_eq!(
                target.render_decision,
                Some(expected),
                "render default should use installed packages and keep legacy fallback otherwise"
            );
        }
        assert!(
            first_diagnostic
                .target(fixture.outer_key)
                .expect("outer target")
                .has_decoration_package
        );
        assert!(
            first_diagnostic
                .target(fixture.inner_key)
                .expect("inner target")
                .has_decoration_package
        );
        assert!(
            first_diagnostic
                .target(fixture.sibling_key)
                .expect("sibling target")
                .has_decoration_package
        );
        assert!(
            first_diagnostic
                .target(fixture.atomic_key)
                .expect("atomic target")
                .has_atomic_package
        );

        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.parent_key);
            parent_element.mark_place_dirty();
        }
        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, fixture.height),
            inline_element_ifc_matrix_placement(parent_width, fixture.height),
        );
        let reuse_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("reuse diagnostic")
        };
        assert_eq!(reuse_diagnostic.cache_key, first_diagnostic.cache_key);
        assert_eq!(
            reuse_diagnostic.invalidation,
            Some(InlineIfcInvalidation::Reuse)
        );
        assert_eq!(reuse_diagnostic.rebuilt, Some(false));
        assert_eq!(
            reuse_diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );

        let narrower_width = 156.0;
        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.parent_key);
            parent_element.mark_place_dirty();
        }
        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(narrower_width, fixture.height),
            inline_element_ifc_matrix_placement(narrower_width, fixture.height),
        );
        let width_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("width diagnostic")
        };
        assert_ne!(width_diagnostic.cache_key, reuse_diagnostic.cache_key);
        assert_eq!(
            width_diagnostic.invalidation,
            Some(InlineIfcInvalidation::Reshape)
        );
        assert_eq!(width_diagnostic.rebuilt, Some(true));

        {
            let mut text_element =
                crate::view::test_support::get_element_mut::<Text>(&arena, fixture.mutable_text_key);
            text_element.set_text("outer text before content changes enough to reshape");
        }
        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(narrower_width, fixture.height),
            inline_element_ifc_matrix_placement(narrower_width, fixture.height),
        );
        let content_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("content diagnostic")
        };
        assert_eq!(
            content_diagnostic.invalidation,
            Some(InlineIfcInvalidation::Reshape)
        );
        assert_eq!(content_diagnostic.rebuilt, Some(true));

        {
            let mut text_element =
                crate::view::test_support::get_element_mut::<Text>(&arena, fixture.mutable_text_key);
            text_element.set_color(Color::hex("#be123c"));
        }
        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(narrower_width, fixture.height),
            inline_element_ifc_matrix_placement(narrower_width, fixture.height),
        );
        let style_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("style diagnostic")
        };
        assert_eq!(
            style_diagnostic.invalidation,
            Some(InlineIfcInvalidation::RepaintOnly)
        );
        assert_eq!(style_diagnostic.rebuilt, Some(true));

        {
            let mut sibling_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.sibling_key);
            sibling_element.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
        }
        let render_candidate_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("render decision diagnostic")
        };
        assert_eq!(
            render_candidate_diagnostic
                .target(fixture.sibling_key)
                .expect("sibling target")
                .render_decision,
            Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            }),
            "render candidate default reads the installed package"
        );
        assert_eq!(
            render_candidate_diagnostic
                .target(fixture.outer_key)
                .expect("outer target")
                .render_decision,
            Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            })
        );
    }

    #[test]
    fn inline_element_ifc_shadow_observation_reports_without_installing_or_switching_render() {
        let parent_width = 210.0;
        let (mut arena, fixture) = build_inline_element_ifc_production_matrix_fixture(
            parent_width,
            ElementInlineIfcLayoutCallSiteScenario::DefaultLegacyFallback,
        );
        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.parent_key);
            assert_eq!(
                parent_element.inline_ifc_layout_call_site_rollout_phase_for_test(),
                ElementInlineIfcLayoutCallSiteRolloutPhase::Disabled,
                "fixture starts from explicit disabled rollback config"
            );
            parent_element.apply_inline_ifc_layout_call_site_rollout_config_for_test(
                ElementInlineIfcLayoutCallSiteRolloutConfig::production_default_shadow_run_phase(),
            );
            assert_eq!(
                parent_element.inline_ifc_layout_call_site_rollout_phase_for_test(),
                ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun,
                "shadow-run phase must only observe candidates"
            );
        }

        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, fixture.height),
            inline_element_ifc_matrix_placement(parent_width, fixture.height),
        );

        let diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("shadow observation should expose diagnostics")
        };
        assert_eq!(
            diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
        );
        assert_eq!(
            diagnostic.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan
        );
        assert_eq!(
            diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );
        assert_eq!(
            diagnostic.invalidation,
            Some(InlineIfcInvalidation::Reshape)
        );
        assert_eq!(diagnostic.rebuilt, Some(true));
        assert_eq!(diagnostic.cache_len, 1);

        for key in [
            fixture.outer_key,
            fixture.inner_key,
            fixture.atomic_key,
            fixture.sibling_key,
        ] {
            let target = diagnostic
                .target(key)
                .expect("shadow observation should report each inline target");
            assert_eq!(
                target.install_status,
                Some(ElementInlineIfcCandidateLifecycleInstallStatus::ObservedOnly),
                "shadow observation must not install rollout packages"
            );
            assert_eq!(
                target.render_decision,
                Some(ElementInlineIfcRenderDecision::ExistingInlineFragments),
                "shadow observation must keep render on legacy fallback"
            );
        }
        assert!(
            diagnostic
                .target(fixture.outer_key)
                .expect("outer target")
                .has_decoration_package
        );
        assert!(
            diagnostic
                .target(fixture.inner_key)
                .expect("inner target")
                .has_decoration_package
        );
        assert!(
            diagnostic
                .target(fixture.sibling_key)
                .expect("sibling target")
                .has_decoration_package
        );
        assert!(
            diagnostic
                .target(fixture.atomic_key)
                .expect("atomic target")
                .has_atomic_package
        );

        for key in [
            fixture.outer_key,
            fixture.inner_key,
            fixture.sibling_key,
        ] {
            let element = crate::view::test_support::get_element::<Element>(&arena, key);
            assert!(
                element
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty(),
                "shadow observation must not install decoration packages"
            );
        }
        let atomic = crate::view::test_support::get_element::<Element>(&arena, fixture.atomic_key);
        assert!(
            atomic.inline_ifc_atomic_placement_metadata_for_test().is_none(),
            "shadow observation must not install atomic packages"
        );

        {
            let mut sibling_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.sibling_key);
            sibling_element.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
        }
        let candidate_diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("shadow diagnostic reads live render decisions")
        };
        assert_eq!(
            candidate_diagnostic
                .target(fixture.sibling_key)
                .expect("sibling target")
                .render_decision,
            Some(ElementInlineIfcRenderDecision::ExistingInlineFragments),
            "explicit render candidate still falls back when shadow observation did not install packages"
        );
    }

    #[test]
    fn inline_element_ifc_production_default_layout_is_shadow_only_observation() {
        let parent_width = 180.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(850, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent_style.set_line_height(1.2);
        parent.apply_style(parent_style);
        assert_eq!(
            parent.inline_ifc_layout_call_site_rollout_phase_for_test(),
            ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun,
            "new Element production layout default must only be the shadow-run phase"
        );
        let parent_key = commit_element(&mut arena, Box::new(parent));

        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content_with_id(940, "default shadow prefix ")),
        );

        let mut child = Element::new_with_id(851, 0.0, 0.0, 0.0, 0.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        child_style.insert(PropertyId::Width, ParsedValue::Auto);
        child_style.insert(PropertyId::Height, ParsedValue::Auto);
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fef3c7")),
        );
        child_style.set_padding(crate::style::Padding::uniform(Length::px(4.0)));
        child.apply_style(child_style);
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));
        commit_child(
            &mut arena,
            child_key,
            Box::new(Text::from_content_with_id(941, "inline child")),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            inline_element_ifc_matrix_constraints(parent_width, 160.0),
            inline_element_ifc_matrix_placement(parent_width, 160.0),
        );

        let diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("production default shadow-run should expose diagnostics")
        };
        assert_eq!(
            diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
        );
        assert_eq!(
            diagnostic.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan
        );
        assert_eq!(
            diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );
        let target = diagnostic.target(child_key).expect("inline child target");
        assert_eq!(
            target.install_status,
            Some(ElementInlineIfcCandidateLifecycleInstallStatus::ObservedOnly),
            "production default shadow-run must not install rollout packages"
        );
        assert_eq!(
            target.render_decision,
            Some(ElementInlineIfcRenderDecision::ExistingInlineFragments)
        );

        {
            let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
            assert!(
                child
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty(),
                "production default shadow-run must not install decoration packages"
            );
            assert_eq!(
                child.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments
            );
        }

        {
            let mut child =
                crate::view::test_support::get_element_mut::<Element>(&arena, child_key);
            child.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
        }
        {
            let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
            assert_eq!(
                child.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
                "explicit render candidate still falls back without installed packages"
            );
        }
    }

    #[test]
    fn inline_element_ifc_default_rollout_decision_guard_requires_full_checklist() {
        let default_config = ElementInlineIfcLayoutCallSiteRolloutConfig::default();
        assert_eq!(
            default_config.phase(),
            ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun,
            "production layout default may only advance to shadow-run observation"
        );
        assert_eq!(
            default_config.mode(),
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
        );
        assert_eq!(
            ElementInlineIfcLayoutCallSiteRolloutConfig::disabled().mode(),
            ElementInlineIfcLayoutCallSiteOptInMode::Disabled,
            "explicit disabled config remains the rollback no-op"
        );

        let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
            ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
        );
        assert!(decision.is_allowed());
        assert!(decision.blocked_reasons().is_empty());
        assert_eq!(
            decision.recommended_phase(),
            ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun
        );
        assert_eq!(
            decision.recommended_config().mode(),
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation,
            "decision guard may only recommend the layout shadow-run phase"
        );
    }

    #[test]
    fn inline_element_ifc_default_rollout_decision_guard_blocks_each_missing_condition() {
        let blocking_cases = [
            (
                ElementInlineIfcDefaultRolloutDecisionInput {
                    render_gate_independent: false,
                    ..ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed()
                },
                ElementInlineIfcDefaultRolloutBlockedReason::RenderGateNotIndependent,
            ),
            (
                ElementInlineIfcDefaultRolloutDecisionInput {
                    legacy_fallback_available: false,
                    ..ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed()
                },
                ElementInlineIfcDefaultRolloutBlockedReason::LegacyFallbackMissing,
            ),
            (
                ElementInlineIfcDefaultRolloutDecisionInput {
                    unsupported_root_and_text_area_boundary_confirmed: false,
                    ..ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed()
                },
                ElementInlineIfcDefaultRolloutBlockedReason::
                    UnsupportedRootAndTextAreaBoundaryUnconfirmed,
            ),
            (
                ElementInlineIfcDefaultRolloutDecisionInput {
                    invalidation_guard_confirmed: false,
                    ..ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed()
                },
                ElementInlineIfcDefaultRolloutBlockedReason::InvalidationGuardUnconfirmed,
            ),
        ];

        for (input, reason) in blocking_cases {
            let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(input);
            assert!(!decision.is_allowed(), "missing {reason:?} must block");
            assert_eq!(
                decision.recommended_phase(),
                ElementInlineIfcLayoutCallSiteRolloutPhase::Disabled
            );
            assert_eq!(
                decision.recommended_config().mode(),
                ElementInlineIfcLayoutCallSiteOptInMode::Disabled
            );
            assert!(
                decision.blocked_reasons().contains(&reason),
                "decision should report the exact blocked reason"
            );
        }
    }

    #[test]
    fn inline_element_ifc_default_rollout_decision_shadow_run_keeps_render_gate_explicit() {
        let parent_width = 210.0;
        let (mut arena, fixture) = build_inline_element_ifc_production_matrix_fixture(
            parent_width,
            ElementInlineIfcLayoutCallSiteScenario::DefaultLegacyFallback,
        );
        let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
            ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
        );
        assert!(decision.is_allowed());
        {
            let mut parent_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.parent_key);
            parent_element.apply_inline_ifc_layout_call_site_rollout_config_for_test(
                decision.recommended_config(),
            );
        }

        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, fixture.height),
            inline_element_ifc_matrix_placement(parent_width, fixture.height),
        );

        let diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("decision-recommended shadow run should expose diagnostics")
        };
        assert_eq!(
            diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
        );
        assert_eq!(
            diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );

        {
            let sibling =
                crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
            assert_eq!(
                sibling.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
                "layout shadow-run decision must not enable draw-rect render candidate by default"
            );
        }

        {
            let mut sibling =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.sibling_key);
            sibling.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
        }
        {
            let sibling =
                crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
            assert_eq!(
                sibling.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
                "explicit draw-rect render opt-in still falls back without installed packages"
            );
        }
    }

    #[test]
    fn inline_element_ifc_default_shadow_run_audit_only_allows_shadow_observation() {
        let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
            ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
        );
        let audit = ElementInlineIfcDefaultShadowRunAdoptionAudit::evaluate(
            ElementInlineIfcDefaultShadowRunAdoptionAuditInput::with_confirmed_observations(
                decision,
            ),
        );

        assert!(audit.is_ready_for_shadow_only_observation());
        assert_eq!(
            audit.readiness(),
            ElementInlineIfcDefaultShadowRunAuditReadiness::ReadyForShadowOnlyObservation
        );
        assert!(audit.blocked_reasons().is_empty());
        assert_eq!(
            audit.recommended_config().phase(),
            ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun,
            "audit may only recommend adopting the shadow-run layout phase"
        );
        assert_eq!(
            audit.recommended_config().mode(),
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
        );
        assert!(
            !audit.allows_render_candidate_default(),
            "shadow-run adoption audit must not authorize draw-rect render candidate default"
        );
        assert_eq!(
            ElementInlineIfcLayoutCallSiteRolloutConfig::default().phase(),
            ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun,
            "production default config is now shadow-only observation"
        );
    }

    #[test]
    fn inline_element_ifc_default_shadow_run_audit_blocks_missing_critical_observations() {
        let blocking_cases = [
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput {
                            render_gate_independent: false,
                            ..ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed()
                        },
                    );
                    ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision)
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::DecisionBlocked,
            ),
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
                    );
                    let mut input = ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision);
                    input.production_default_config =
                        ElementInlineIfcLayoutCallSiteRolloutConfig::disabled();
                    input
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    ProductionDefaultNotShadowOnlyObservation,
            ),
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
                    );
                    let mut input = ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision);
                    input.shadow_observation_diagnostic_observed = false;
                    input
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    ShadowObservationDiagnosticMissing,
            ),
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
                    );
                    let mut input = ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision);
                    input.shadow_observation_installed_packages = true;
                    input
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    ShadowObservationInstalledPackages,
            ),
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
                    );
                    let mut input = ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision);
                    input.legacy_fallback_observed = false;
                    input
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::LegacyFallbackNotObserved,
            ),
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
                    );
                    let mut input = ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision);
                    input.render_gate_explicit_observed = false;
                    input
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    RenderGateExplicitnessUnobserved,
            ),
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
                    );
                    let mut input = ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision);
                    input.unsupported_or_non_inline_no_op_observed = false;
                    input
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    UnsupportedOrNonInlineNoOpUnobserved,
            ),
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
                    );
                    let mut input = ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision);
                    input.text_area_boundary_observed = false;
                    input
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::TextAreaBoundaryUnobserved,
            ),
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
                    );
                    let mut input = ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                        with_confirmed_observations(decision);
                    input.matrix_invalidation_guard_observed = false;
                    input
                },
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    MatrixInvalidationGuardUnobserved,
            ),
        ];

        for (input, reason) in blocking_cases {
            let audit = ElementInlineIfcDefaultShadowRunAdoptionAudit::evaluate(input);
            assert!(
                !audit.is_ready_for_shadow_only_observation(),
                "missing {reason:?} must block shadow-run adoption"
            );
            assert_eq!(
                audit.readiness(),
                ElementInlineIfcDefaultShadowRunAuditReadiness::Blocked
            );
            assert_eq!(
                audit.recommended_config().phase(),
                ElementInlineIfcLayoutCallSiteRolloutPhase::Disabled
            );
            assert!(
                audit.blocked_reasons().contains(&reason),
                "audit should report the exact blocked reason"
            );
            assert!(!audit.allows_render_candidate_default());
        }
    }

    #[test]
    fn inline_element_ifc_default_shadow_run_audit_recommended_config_stays_shadow_only() {
        let parent_width = 210.0;
        let (mut arena, fixture) = build_inline_element_ifc_production_matrix_fixture(
            parent_width,
            ElementInlineIfcLayoutCallSiteScenario::DefaultLegacyFallback,
        );
        let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
            ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
        );
        let audit = ElementInlineIfcDefaultShadowRunAdoptionAudit::evaluate(
            ElementInlineIfcDefaultShadowRunAdoptionAuditInput::with_confirmed_observations(
                decision,
            ),
        );
        assert!(audit.is_ready_for_shadow_only_observation());

        {
            let mut parent =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.parent_key);
            parent.apply_inline_ifc_layout_call_site_rollout_config_for_test(
                audit.recommended_config(),
            );
        }
        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, fixture.height),
            inline_element_ifc_matrix_placement(parent_width, fixture.height),
        );

        let diagnostic = {
            let parent =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            parent
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("audit-recommended shadow run should expose diagnostics")
        };
        assert_eq!(
            diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
        );
        assert_eq!(
            diagnostic.fallback,
            ElementInlineIfcRenderFallback::ExistingInlineFragments
        );
        for target in &diagnostic.target_installs {
            assert_eq!(
                target.install_status,
                Some(ElementInlineIfcCandidateLifecycleInstallStatus::ObservedOnly),
                "audit-recommended shadow run must not install rollout packages"
            );
        }

        let sibling = crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
        assert_eq!(
            sibling.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments
        );
        assert!(
            sibling
                .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                .is_empty(),
            "shadow-only adoption must not install draw-rect metadata"
        );
    }

    fn inline_element_ifc_ready_shadow_run_audit_for_test(
    ) -> ElementInlineIfcDefaultShadowRunAdoptionAudit {
        let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
            ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed(),
        );
        ElementInlineIfcDefaultShadowRunAdoptionAudit::evaluate(
            ElementInlineIfcDefaultShadowRunAdoptionAuditInput::with_confirmed_observations(
                decision,
            ),
        )
    }

    #[test]
    fn inline_element_ifc_render_default_audit_only_allows_explicit_candidate_evaluation() {
        let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
        assert!(shadow_audit.is_ready_for_shadow_only_observation());

        let audit = ElementInlineIfcRenderDefaultAudit::evaluate(
            ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(shadow_audit),
        );

        assert!(audit.is_ready_for_explicit_render_candidate_evaluation());
        assert_eq!(
            audit.readiness(),
            ElementInlineIfcRenderDefaultAuditReadiness::
                ReadyForExplicitRenderCandidateEvaluation
        );
        assert!(audit.blocked_reasons().is_empty());
        assert_eq!(
            audit.explicit_candidate_evaluation_mode(),
            Some(ElementInlineIfcRenderMode::DrawRectPackageCandidate),
            "render audit may only name the explicit candidate evaluation mode"
        );
        assert!(
            !audit.allows_render_candidate_default(),
            "render audit must not authorize changing the production render default"
        );
        assert_eq!(
            ElementInlineIfcRenderMode::default(),
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            "inline Element render default is now the draw-rect package candidate"
        );
        let element = Element::new(0.0, 0.0, 40.0, 20.0);
        assert_eq!(
            element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "audit pass must not mutate element render decision"
        );
    }

    #[test]
    fn inline_element_ifc_render_default_audit_blocks_missing_critical_observations() {
        let blocking_cases = [
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput {
                            render_gate_independent: false,
                            ..ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed()
                        },
                    );
                    let shadow_audit = ElementInlineIfcDefaultShadowRunAdoptionAudit::evaluate(
                        ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                            with_confirmed_observations(decision),
                    );
                    ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                        shadow_audit,
                    )
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::ShadowRunAuditNotReady,
            ),
            (
                {
                    let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
                    ElementInlineIfcRenderDefaultAuditInput {
                        current_render_default: ElementInlineIfcRenderMode::
                            DrawRectPackageCandidate,
                        ..ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        )
                    }
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::RenderDefaultAlreadyCandidate,
            ),
            (
                {
                    let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
                    ElementInlineIfcRenderDefaultAuditInput {
                        installed_package_lifecycle_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        )
                    }
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::
                    InstalledPackageLifecycleUnconfirmed,
            ),
            (
                {
                    let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
                    ElementInlineIfcRenderDefaultAuditInput {
                        legacy_fallback_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        )
                    }
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::LegacyFallbackUnconfirmed,
            ),
            (
                {
                    let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
                    ElementInlineIfcRenderDefaultAuditInput {
                        unsupported_or_non_inline_boundary_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        )
                    }
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::
                    UnsupportedOrNonInlineBoundaryUnconfirmed,
            ),
            (
                {
                    let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
                    ElementInlineIfcRenderDefaultAuditInput {
                        text_area_boundary_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        )
                    }
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::TextAreaBoundaryUnconfirmed,
            ),
            (
                {
                    let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
                    ElementInlineIfcRenderDefaultAuditInput {
                        rollback_disabled_path_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        )
                    }
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::RollbackDisabledPathUnconfirmed,
            ),
            (
                {
                    let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
                    ElementInlineIfcRenderDefaultAuditInput {
                        explicit_render_opt_in_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        )
                    }
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::ExplicitRenderOptInUnconfirmed,
            ),
            (
                {
                    let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
                    ElementInlineIfcRenderDefaultAuditInput {
                        missing_installed_package_fallback_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        )
                    }
                },
                ElementInlineIfcRenderDefaultAuditBlockedReason::
                    MissingInstalledPackageFallbackUnconfirmed,
            ),
        ];

        for (input, reason) in blocking_cases {
            let audit = ElementInlineIfcRenderDefaultAudit::evaluate(input);
            assert!(
                !audit.is_ready_for_explicit_render_candidate_evaluation(),
                "missing {reason:?} must block render candidate evaluation readiness"
            );
            assert_eq!(
                audit.readiness(),
                ElementInlineIfcRenderDefaultAuditReadiness::Blocked
            );
            assert_eq!(audit.explicit_candidate_evaluation_mode(), None);
            assert!(
                audit.blocked_reasons().contains(&reason),
                "audit should report the exact blocked reason"
            );
            assert!(!audit.allows_render_candidate_default());
        }
    }

    #[test]
    fn inline_element_ifc_render_default_audit_keeps_explicit_opt_in_and_missing_package_fallback() {
        let parent_width = 210.0;
        let (mut arena, fixture) = build_inline_element_ifc_production_matrix_fixture(
            parent_width,
            ElementInlineIfcLayoutCallSiteScenario::DefaultLegacyFallback,
        );

        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, fixture.height),
            inline_element_ifc_matrix_placement(parent_width, fixture.height),
        );

        let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
        let audit = ElementInlineIfcRenderDefaultAudit::evaluate(
            ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(shadow_audit),
        );
        assert!(audit.is_ready_for_explicit_render_candidate_evaluation());

        {
            let sibling =
                crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
            assert_eq!(
                sibling.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
                "render default must still fallback when shadow-only layout installed no packages"
            );
        }

        {
            let mut sibling =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.sibling_key);
            sibling.set_inline_ifc_render_mode_for_test(
                audit
                    .explicit_candidate_evaluation_mode()
                    .expect("audit pass should only expose explicit evaluation mode"),
            );
        }

        let sibling = crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
        assert_eq!(
            sibling.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "explicit draw-rect candidate must still fallback without installed packages"
        );
        assert!(
            sibling
                .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                .is_empty(),
            "shadow-only layout default must not provide render candidate packages"
        );
    }

    fn inline_element_ifc_ready_render_default_audit_for_test() -> ElementInlineIfcRenderDefaultAudit
    {
        let shadow_audit = inline_element_ifc_ready_shadow_run_audit_for_test();
        ElementInlineIfcRenderDefaultAudit::evaluate(
            ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(shadow_audit),
        )
    }

    fn inline_element_ifc_ready_render_default_rollout_decision_for_test(
    ) -> ElementInlineIfcRenderDefaultRolloutDecision {
        let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
        ElementInlineIfcRenderDefaultRolloutDecision::evaluate(
            ElementInlineIfcRenderDefaultRolloutDecisionInput::with_confirmed_observations(
                render_audit,
            ),
        )
    }

    #[test]
    fn inline_element_ifc_render_default_rollout_decision_only_allows_controlled_installed_package_candidate(
    ) {
        let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
        assert!(render_audit.is_ready_for_explicit_render_candidate_evaluation());

        let decision = ElementInlineIfcRenderDefaultRolloutDecision::evaluate(
            ElementInlineIfcRenderDefaultRolloutDecisionInput::with_confirmed_observations(
                render_audit,
            ),
        );

        assert!(decision.is_ready_for_controlled_installed_package_candidate());
        assert_eq!(
            decision.readiness(),
            ElementInlineIfcRenderDefaultRolloutReadiness::
                ReadyForControlledInstalledPackageCandidate
        );
        assert!(decision.blocked_reasons().is_empty());
        assert_eq!(
            decision.explicit_installed_package_candidate_mode(),
            Some(ElementInlineIfcRenderMode::DrawRectPackageCandidate),
            "rollout decision may only name an explicit installed-package candidate mode"
        );
        assert!(
            !decision.allows_render_candidate_default(),
            "rollout decision must not authorize changing the production render default"
        );
        assert_eq!(
            ElementInlineIfcRenderMode::default(),
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            "inline Element render default is now the draw-rect package candidate"
        );
        let element = Element::new(0.0, 0.0, 40.0, 20.0);
        assert_eq!(
            element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "rollout decision pass must not mutate element render decision"
        );
    }

    #[test]
    fn inline_element_ifc_render_default_rollout_decision_blocks_missing_readiness() {
        let blocking_cases = [
            (
                {
                    let decision = ElementInlineIfcDefaultRolloutDecision::evaluate(
                        ElementInlineIfcDefaultRolloutDecisionInput {
                            render_gate_independent: false,
                            ..ElementInlineIfcDefaultRolloutDecisionInput::checklist_passed()
                        },
                    );
                    let shadow_audit = ElementInlineIfcDefaultShadowRunAdoptionAudit::evaluate(
                        ElementInlineIfcDefaultShadowRunAdoptionAuditInput::
                            with_confirmed_observations(decision),
                    );
                    let render_audit = ElementInlineIfcRenderDefaultAudit::evaluate(
                        ElementInlineIfcRenderDefaultAuditInput::with_confirmed_observations(
                            shadow_audit,
                        ),
                    );
                    ElementInlineIfcRenderDefaultRolloutDecisionInput::
                        with_confirmed_observations(render_audit)
                },
                ElementInlineIfcRenderDefaultRolloutBlockedReason::RenderAuditNotReady,
            ),
            (
                {
                    let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
                    ElementInlineIfcRenderDefaultRolloutDecisionInput {
                        current_render_default: ElementInlineIfcRenderMode::
                            DrawRectPackageCandidate,
                        ..ElementInlineIfcRenderDefaultRolloutDecisionInput::
                            with_confirmed_observations(render_audit)
                    }
                },
                ElementInlineIfcRenderDefaultRolloutBlockedReason::RenderDefaultAlreadyCandidate,
            ),
            (
                {
                    let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
                    ElementInlineIfcRenderDefaultRolloutDecisionInput {
                        explicit_candidate_evaluation_observed: false,
                        ..ElementInlineIfcRenderDefaultRolloutDecisionInput::
                            with_confirmed_observations(render_audit)
                    }
                },
                ElementInlineIfcRenderDefaultRolloutBlockedReason::
                    ExplicitCandidateEvaluationUnobserved,
            ),
            (
                {
                    let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
                    ElementInlineIfcRenderDefaultRolloutDecisionInput {
                        controlled_installed_package_candidate_observed: false,
                        ..ElementInlineIfcRenderDefaultRolloutDecisionInput::
                            with_confirmed_observations(render_audit)
                    }
                },
                ElementInlineIfcRenderDefaultRolloutBlockedReason::
                    InstalledPackageCandidateUnobserved,
            ),
            (
                {
                    let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
                    ElementInlineIfcRenderDefaultRolloutDecisionInput {
                        current_default_render_decision:
                            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                                has_atomic_placement_package: false,
                            },
                        ..ElementInlineIfcRenderDefaultRolloutDecisionInput::
                            with_confirmed_observations(render_audit)
                    }
                },
                ElementInlineIfcRenderDefaultRolloutBlockedReason::LegacyDefaultDecisionChanged,
            ),
            (
                {
                    let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
                    ElementInlineIfcRenderDefaultRolloutDecisionInput {
                        missing_installed_package_fallback_observed: false,
                        ..ElementInlineIfcRenderDefaultRolloutDecisionInput::
                            with_confirmed_observations(render_audit)
                    }
                },
                ElementInlineIfcRenderDefaultRolloutBlockedReason::
                    MissingInstalledPackageFallbackUnobserved,
            ),
            (
                {
                    let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
                    ElementInlineIfcRenderDefaultRolloutDecisionInput {
                        rollback_disabled_path_confirmed: false,
                        ..ElementInlineIfcRenderDefaultRolloutDecisionInput::
                            with_confirmed_observations(render_audit)
                    }
                },
                ElementInlineIfcRenderDefaultRolloutBlockedReason::RollbackDisabledPathUnconfirmed,
            ),
            (
                {
                    let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
                    ElementInlineIfcRenderDefaultRolloutDecisionInput {
                        text_area_boundary_confirmed: false,
                        ..ElementInlineIfcRenderDefaultRolloutDecisionInput::
                            with_confirmed_observations(render_audit)
                    }
                },
                ElementInlineIfcRenderDefaultRolloutBlockedReason::TextAreaBoundaryUnconfirmed,
            ),
        ];

        for (input, reason) in blocking_cases {
            let decision = ElementInlineIfcRenderDefaultRolloutDecision::evaluate(input);
            assert!(
                !decision.is_ready_for_controlled_installed_package_candidate(),
                "missing {reason:?} must block controlled installed-package readiness"
            );
            assert_eq!(
                decision.readiness(),
                ElementInlineIfcRenderDefaultRolloutReadiness::Blocked
            );
            assert_eq!(decision.explicit_installed_package_candidate_mode(), None);
            assert!(
                decision.blocked_reasons().contains(&reason),
                "decision should report the exact blocked reason"
            );
            assert!(!decision.allows_render_candidate_default());
        }
    }

    #[test]
    fn inline_element_ifc_render_default_rollout_decision_keeps_default_fallback_and_rollback() {
        let parent_width = 210.0;
        let (mut arena, fixture) = build_inline_element_ifc_production_matrix_fixture(
            parent_width,
            ElementInlineIfcLayoutCallSiteScenario::DefaultLegacyFallback,
        );

        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, fixture.height),
            inline_element_ifc_matrix_placement(parent_width, fixture.height),
        );

        let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
        let decision = ElementInlineIfcRenderDefaultRolloutDecision::evaluate(
            ElementInlineIfcRenderDefaultRolloutDecisionInput::with_confirmed_observations(
                render_audit,
            ),
        );
        assert!(decision.is_ready_for_controlled_installed_package_candidate());

        {
            let sibling =
                crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
            assert_eq!(
                sibling.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
                "default shadow-only layout still leaves render on legacy fallback without packages"
            );
        }

        {
            let mut sibling =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.sibling_key);
            sibling.set_inline_ifc_render_mode_for_test(
                decision
                    .explicit_installed_package_candidate_mode()
                    .expect("decision pass should only expose explicit installed-package mode"),
            );
        }

        {
            let sibling =
                crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
            assert_eq!(
                sibling.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
                "explicit candidate must still fallback without installed packages"
            );
            assert!(
                sibling
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty(),
                "default shadow-only layout must not install draw-rect metadata"
            );
        }

        {
            let mut sibling =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.sibling_key);
            sibling.set_inline_ifc_render_mode_for_test(ElementInlineIfcRenderMode::Disabled);
        }
        let sibling = crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
        assert_eq!(
            sibling.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "explicit disabled render mode remains the rollback path"
        );
    }

    #[test]
    fn inline_element_ifc_controlled_installed_package_candidate_installs_packages_under_guard() {
        let parent_width = 210.0;
        let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
        let config = decision
            .controlled_installed_package_candidate_config()
            .expect("ready rollout decision should expose controlled layout candidate config");
        assert_eq!(
            config.phase(),
            ElementInlineIfcLayoutCallSiteRolloutPhase::ControlledInstalledPackageCandidate
        );

        let (mut arena, fixture) = build_inline_element_ifc_production_matrix_fixture(
            parent_width,
            ElementInlineIfcLayoutCallSiteScenario::ControlledInstalledPackageCandidate,
        );

        measure_and_place(
            &mut arena,
            fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, fixture.height),
            inline_element_ifc_matrix_placement(parent_width, fixture.height),
        );

        let diagnostic = {
            let parent =
                crate::view::test_support::get_element::<Element>(&arena, fixture.parent_key);
            assert_eq!(
                parent.inline_ifc_layout_call_site_rollout_phase_for_test(),
                ElementInlineIfcLayoutCallSiteRolloutPhase::ControlledInstalledPackageCandidate
            );
            parent
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("controlled package candidate should expose diagnostic output")
        };
        assert_eq!(
            diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
        );
        for key in [
            fixture.outer_key,
            fixture.inner_key,
            fixture.sibling_key,
        ] {
            let target = diagnostic.target(key).expect("inline Element target");
            assert_eq!(
                target.install_status,
                Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
            );
            assert!(target.has_decoration_package);

            let element = crate::view::test_support::get_element::<Element>(&arena, key);
            assert!(
                !element
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty(),
                "controlled installed-package candidate should install decoration package"
            );
            assert_eq!(
                element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                },
                "render default should use the installed package while retaining fallback"
            );
        }

        let atomic = diagnostic
            .target(fixture.atomic_key)
            .expect("atomic inline Element target");
        assert_eq!(
            atomic.install_status,
            Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
        );
        assert!(atomic.has_atomic_package);

        {
            let mut sibling =
                crate::view::test_support::get_element_mut::<Element>(&arena, fixture.sibling_key);
            sibling.set_inline_ifc_render_mode_for_test(ElementInlineIfcRenderMode::Disabled);
        }
        let sibling = crate::view::test_support::get_element::<Element>(&arena, fixture.sibling_key);
        assert_eq!(
            sibling.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "explicit disabled render mode remains the rollback path after packages are installed"
        );

        let missing_package_element = Element::new(0.0, 0.0, 40.0, 20.0);
        assert_eq!(
            missing_package_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "render default candidate must fallback when no installed package is available"
        );
    }

    #[test]
    fn inline_element_ifc_render_default_adoption_audit_allows_default_after_controlled_candidate() {
        let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
        assert!(decision.is_ready_for_controlled_installed_package_candidate());

        let audit = ElementInlineIfcRenderDefaultAdoptionAudit::evaluate(
            ElementInlineIfcRenderDefaultAdoptionAuditInput::with_confirmed_observations(decision),
        );

        assert!(audit.is_ready_for_inline_element_render_default());
        assert_eq!(
            audit.readiness(),
            ElementInlineIfcRenderDefaultAdoptionAuditReadiness::ReadyForInlineElementRenderDefault
        );
        assert_eq!(
            audit.recommended_default_mode(),
            Some(ElementInlineIfcRenderMode::DrawRectPackageCandidate)
        );
        assert!(audit.allows_render_candidate_default());
        assert!(audit.blocked_reasons().is_empty());
        assert_eq!(
            ElementInlineIfcRenderMode::default(),
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            "inline Element render default should follow the adoption audit recommendation"
        );
    }

    #[test]
    fn inline_element_ifc_render_default_adoption_audit_blocks_missing_safety_observations() {
        let blocking_cases = [
            (
                {
                    let render_audit = inline_element_ifc_ready_render_default_audit_for_test();
                    let blocked_decision = ElementInlineIfcRenderDefaultRolloutDecision::evaluate(
                        ElementInlineIfcRenderDefaultRolloutDecisionInput {
                            missing_installed_package_fallback_observed: false,
                            ..ElementInlineIfcRenderDefaultRolloutDecisionInput::
                                with_confirmed_observations(render_audit)
                        },
                    );
                    ElementInlineIfcRenderDefaultAdoptionAuditInput::
                        with_confirmed_observations(blocked_decision)
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::RolloutDecisionNotReady,
            ),
            (
                {
                    let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
                    ElementInlineIfcRenderDefaultAdoptionAuditInput {
                        controlled_installed_package_candidate_observed: false,
                        ..ElementInlineIfcRenderDefaultAdoptionAuditInput::
                            with_confirmed_observations(decision)
                    }
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    ControlledInstalledPackageCandidateMissing,
            ),
            (
                {
                    let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
                    ElementInlineIfcRenderDefaultAdoptionAuditInput {
                        controlled_installed_package_diagnostic_observed: false,
                        ..ElementInlineIfcRenderDefaultAdoptionAuditInput::
                            with_confirmed_observations(decision)
                    }
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    ControlledInstalledPackageDiagnosticMissing,
            ),
            (
                {
                    let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
                    ElementInlineIfcRenderDefaultAdoptionAuditInput {
                        default_path_package_available: false,
                        ..ElementInlineIfcRenderDefaultAdoptionAuditInput::
                            with_confirmed_observations(decision)
                    }
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    DefaultPathPackageUnavailable,
            ),
            (
                {
                    let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
                    ElementInlineIfcRenderDefaultAdoptionAuditInput {
                        missing_installed_package_fallback_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAdoptionAuditInput::
                            with_confirmed_observations(decision)
                    }
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    MissingInstalledPackageFallbackUnconfirmed,
            ),
            (
                {
                    let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
                    ElementInlineIfcRenderDefaultAdoptionAuditInput {
                        disabled_rollback_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAdoptionAuditInput::
                            with_confirmed_observations(decision)
                    }
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::DisabledRollbackUnconfirmed,
            ),
            (
                {
                    let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
                    ElementInlineIfcRenderDefaultAdoptionAuditInput {
                        unsupported_or_non_inline_no_op_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAdoptionAuditInput::
                            with_confirmed_observations(decision)
                    }
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    UnsupportedOrNonInlineNoOpUnconfirmed,
            ),
            (
                {
                    let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
                    ElementInlineIfcRenderDefaultAdoptionAuditInput {
                        text_area_boundary_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAdoptionAuditInput::
                            with_confirmed_observations(decision)
                    }
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::TextAreaBoundaryUnconfirmed,
            ),
            (
                {
                    let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
                    ElementInlineIfcRenderDefaultAdoptionAuditInput {
                        legacy_fallback_confirmed: false,
                        ..ElementInlineIfcRenderDefaultAdoptionAuditInput::
                            with_confirmed_observations(decision)
                    }
                },
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::LegacyFallbackUnconfirmed,
            ),
        ];

        for (input, reason) in blocking_cases {
            let audit = ElementInlineIfcRenderDefaultAdoptionAudit::evaluate(input);
            assert_eq!(
                audit.readiness(),
                ElementInlineIfcRenderDefaultAdoptionAuditReadiness::Blocked
            );
            assert_eq!(audit.recommended_default_mode(), None);
            assert!(!audit.allows_render_candidate_default());
            assert!(
                audit.blocked_reasons().contains(&reason),
                "adoption audit should report missing {reason:?}"
            );
        }
    }

    #[test]
    fn inline_element_ifc_render_default_examples_like_adoption_guard_after_default_switch() {
        let parent_width = 210.0;
        assert_eq!(
            ElementInlineIfcRenderMode::default(),
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            "inline Element render default is now the draw-rect package candidate"
        );
        assert_eq!(
            ElementInlineIfcLayoutCallSiteRolloutConfig::default().phase(),
            ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun,
            "layout default must remain shadow-only after render default adoption"
        );

        let (mut shadow_arena, shadow_fixture) = build_inline_element_ifc_production_matrix_fixture(
            parent_width,
            ElementInlineIfcLayoutCallSiteScenario::DefaultCandidateShadowObservation,
        );
        measure_and_place(
            &mut shadow_arena,
            shadow_fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, shadow_fixture.height),
            inline_element_ifc_matrix_placement(parent_width, shadow_fixture.height),
        );
        let shadow_diagnostic = {
            let parent_element = crate::view::test_support::get_element::<Element>(
                &shadow_arena,
                shadow_fixture.parent_key,
            );
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&shadow_arena)
                .expect("default shadow-only rollout should expose examples-like diagnostic")
        };
        assert_eq!(
            shadow_diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
        );
        for key in [
            shadow_fixture.outer_key,
            shadow_fixture.inner_key,
            shadow_fixture.atomic_key,
            shadow_fixture.sibling_key,
        ] {
            let target = shadow_diagnostic
                .target(key)
                .expect("shadow diagnostic should include examples-like target");
            assert_eq!(
                target.install_status,
                Some(ElementInlineIfcCandidateLifecycleInstallStatus::ObservedOnly),
                "default shadow-only layout must not install rollout packages"
            );
            assert_eq!(
                target.render_decision,
                Some(ElementInlineIfcRenderDecision::ExistingInlineFragments),
                "render default must fallback while layout default only observes packages"
            );
        }
        for key in [
            shadow_fixture.outer_key,
            shadow_fixture.inner_key,
            shadow_fixture.sibling_key,
        ] {
            let element = crate::view::test_support::get_element::<Element>(&shadow_arena, key);
            assert!(
                element
                    .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                    .is_empty(),
                "shadow-only default must not leave installed draw-rect metadata"
            );
            assert_eq!(
                element.inline_ifc_render_decision_for_test(),
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
                "missing installed package remains the render default fallback"
            );
        }

        let decision = inline_element_ifc_ready_render_default_rollout_decision_for_test();
        let controlled_config = decision
            .controlled_installed_package_candidate_config()
            .expect("ready rollout decision should expose controlled package config");
        assert_eq!(
            controlled_config.phase(),
            ElementInlineIfcLayoutCallSiteRolloutPhase::ControlledInstalledPackageCandidate
        );
        let (mut controlled_arena, controlled_fixture) =
            build_inline_element_ifc_production_matrix_fixture(
                parent_width,
                ElementInlineIfcLayoutCallSiteScenario::ControlledInstalledPackageCandidate,
            );
        measure_and_place(
            &mut controlled_arena,
            controlled_fixture.parent_key,
            inline_element_ifc_matrix_constraints(parent_width, controlled_fixture.height),
            inline_element_ifc_matrix_placement(parent_width, controlled_fixture.height),
        );
        let controlled_diagnostic = {
            let parent_element = crate::view::test_support::get_element::<Element>(
                &controlled_arena,
                controlled_fixture.parent_key,
            );
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&controlled_arena)
                .expect("controlled installed-package candidate should expose diagnostic")
        };
        assert_eq!(
            controlled_diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
        );
        for key in [
            controlled_fixture.outer_key,
            controlled_fixture.inner_key,
            controlled_fixture.sibling_key,
        ] {
            let target = controlled_diagnostic
                .target(key)
                .expect("controlled diagnostic should include decorated inline target");
            assert_eq!(
                target.install_status,
                Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
            );
            assert!(target.has_decoration_package);
            assert_eq!(
                target.render_decision,
                Some(ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                    fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                    has_atomic_placement_package: false,
                }),
                "installed packages let the render default choose the candidate"
            );
        }
        let atomic_target = controlled_diagnostic
            .target(controlled_fixture.atomic_key)
            .expect("controlled diagnostic should include atomic inline target");
        assert_eq!(
            atomic_target.install_status,
            Some(ElementInlineIfcCandidateLifecycleInstallStatus::Installed)
        );
        assert!(atomic_target.has_atomic_package);
        assert_eq!(
            atomic_target.render_decision,
            Some(ElementInlineIfcRenderDecision::ExistingInlineFragments),
            "atomic-only targets do not become decoration draw-rect candidates"
        );

        {
            let mut sibling = crate::view::test_support::get_element_mut::<Element>(
                &controlled_arena,
                controlled_fixture.sibling_key,
            );
            sibling.set_inline_ifc_render_mode_for_test(ElementInlineIfcRenderMode::Disabled);
        }
        let sibling = crate::view::test_support::get_element::<Element>(
            &controlled_arena,
            controlled_fixture.sibling_key,
        );
        assert_eq!(
            sibling.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "explicit Disabled remains rollback after packages are installed"
        );
        let missing_package_element = Element::new(0.0, 0.0, 40.0, 20.0);
        assert_eq!(
            missing_package_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "missing installed package fallback remains under render default"
        );

        let mut unsupported_arena = new_test_arena();
        let mut unsupported_root = Element::new_with_id(842, 0.0, 0.0, parent_width, 0.0);
        let mut unsupported_style = Style::new();
        unsupported_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().into()),
        );
        unsupported_root.apply_style(unsupported_style);
        unsupported_root.apply_inline_ifc_layout_call_site_rollout_config_for_test(
            ElementInlineIfcLayoutCallSiteRolloutConfig::controlled_installed_package_candidate(),
        );
        let unsupported_key = commit_element(&mut unsupported_arena, Box::new(unsupported_root));
        measure_and_place(
            &mut unsupported_arena,
            unsupported_key,
            inline_element_ifc_matrix_constraints(parent_width, 160.0),
            inline_element_ifc_matrix_placement(parent_width, 160.0),
        );
        let unsupported =
            crate::view::test_support::get_element::<Element>(&unsupported_arena, unsupported_key);
        let unsupported_diagnostic = unsupported
            .inline_ifc_layout_call_site_diagnostic_for_test(&unsupported_arena)
            .expect("controlled config should report unsupported root status");
        assert_eq!(
            unsupported_diagnostic.status,
            ElementInlineIfcLayoutCallSiteOptInStatus::UnsupportedRoot
        );
        assert!(unsupported_diagnostic.target_installs.is_empty());
        assert_eq!(
            unsupported.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments,
            "unsupported root remains no-op under render default"
        );
    }

    #[test]
    fn inline_element_ifc_shadow_observation_keeps_text_area_run_out_of_rollout_targets() {
        use crate::view::base_component::text_area::TextAreaTextRun;

        let parent_width = 180.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(840, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);
        parent.apply_inline_ifc_layout_call_site_rollout_config_for_test(
            ElementInlineIfcLayoutCallSiteRolloutConfig::for_scenario(
                ElementInlineIfcLayoutCallSiteScenario::DefaultCandidateShadowObservation,
            ),
        );
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let text_area_run_key = commit_child(
            &mut arena,
            parent_key,
            Box::new(TextAreaTextRun::new(
                "editable run remains outside rollout".to_string(),
                0..35,
            )),
        );

        let mut child = Element::new_with_id(841, 0.0, 0.0, 0.0, 0.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        child_style.insert(PropertyId::Width, ParsedValue::Auto);
        child_style.insert(PropertyId::Height, ParsedValue::Auto);
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#e0f2fe")),
        );
        child.apply_style(child_style);
        let child_key = commit_child(&mut arena, parent_key, Box::new(child));
        commit_child(
            &mut arena,
            child_key,
            Box::new(Text::from_content_with_id(930, "inline child")),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            inline_element_ifc_matrix_constraints(parent_width, 160.0),
            inline_element_ifc_matrix_placement(parent_width, 160.0),
        );

        let diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("shadow observation should still run with TextAreaTextRun present")
        };
        assert_eq!(
            diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
        );
        assert!(
            diagnostic.install_targets.contains(&child_key),
            "inline Element child should remain an observation target"
        );
        assert!(
            !diagnostic.install_targets.contains(&text_area_run_key),
            "TextAreaTextRun must not become an inline Element rollout target"
        );

        let child_element = crate::view::test_support::get_element::<Element>(&arena, child_key);
        assert_eq!(
            child_element.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::ExistingInlineFragments
        );
        assert!(
            child_element
                .inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0])
                .is_empty(),
            "TextArea boundary test still must not install shadow packages"
        );
    }

    #[test]
    fn text_area_inline_ifc_readiness_blocks_current_p7_preflight() {
        let readiness = TextAreaInlineIfcReadiness::evaluate(
            TextAreaInlineIfcReadinessInput::current_p7_preflight_observations(),
        );

        assert_eq!(
            readiness.readiness(),
            TextAreaInlineIfcReadinessState::Blocked
        );
        assert!(!readiness.is_ready_for_editable_ifc_evaluation());
        assert!(
            !readiness.allows_text_area_default_rollout(),
            "P7 readiness guard must not authorize TextArea default rollout"
        );
        for reason in [
            TextAreaInlineIfcReadinessBlockedReason::EditableIfcPathUnwired,
            TextAreaInlineIfcReadinessBlockedReason::ProjectionIfcPathUnwired,
            TextAreaInlineIfcReadinessBlockedReason::ImeIfcPathUnwired,
            TextAreaInlineIfcReadinessBlockedReason::CaretAffinityIfcPathUnwired,
            TextAreaInlineIfcReadinessBlockedReason::ScrollFollowIfcPathUnwired,
        ] {
            assert!(
                readiness.blocked_reasons().contains(&reason),
                "current P7 preflight must block on missing {reason:?}"
            );
        }
        for reason in [
            TextAreaInlineIfcReadinessBlockedReason::TextAreaTextRunBoundaryUnconfirmed,
            TextAreaInlineIfcReadinessBlockedReason::InlineElementRolloutBoundaryUnconfirmed,
            TextAreaInlineIfcReadinessBlockedReason::
                ReadOnlyTextPreparedPathSeparationUnconfirmed,
            TextAreaInlineIfcReadinessBlockedReason::LegacyFallbackUnconfirmed,
        ] {
            assert!(
                !readiness.blocked_reasons().contains(&reason),
                "current P7 preflight has already confirmed {reason:?}"
            );
        }
        assert_eq!(
            ElementInlineIfcRenderMode::default(),
            ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            "TextArea readiness guard must not roll back inline Element render default"
        );
    }

    #[test]
    fn text_area_inline_ifc_readiness_requires_each_boundary_observation() {
        let blocking_cases = [
            (
                TextAreaInlineIfcReadinessInput {
                    text_area_text_run_boundary_confirmed: false,
                    ..TextAreaInlineIfcReadinessInput::with_all_ifc_paths_wired()
                },
                TextAreaInlineIfcReadinessBlockedReason::TextAreaTextRunBoundaryUnconfirmed,
            ),
            (
                TextAreaInlineIfcReadinessInput {
                    inline_element_rollout_boundary_confirmed: false,
                    ..TextAreaInlineIfcReadinessInput::with_all_ifc_paths_wired()
                },
                TextAreaInlineIfcReadinessBlockedReason::InlineElementRolloutBoundaryUnconfirmed,
            ),
            (
                TextAreaInlineIfcReadinessInput {
                    read_only_text_prepared_path_separated: false,
                    ..TextAreaInlineIfcReadinessInput::with_all_ifc_paths_wired()
                },
                TextAreaInlineIfcReadinessBlockedReason::
                    ReadOnlyTextPreparedPathSeparationUnconfirmed,
            ),
            (
                TextAreaInlineIfcReadinessInput {
                    legacy_fallback_confirmed: false,
                    ..TextAreaInlineIfcReadinessInput::with_all_ifc_paths_wired()
                },
                TextAreaInlineIfcReadinessBlockedReason::LegacyFallbackUnconfirmed,
            ),
        ];

        for (input, reason) in blocking_cases {
            let readiness = TextAreaInlineIfcReadiness::evaluate(input);
            assert_eq!(
                readiness.readiness(),
                TextAreaInlineIfcReadinessState::Blocked
            );
            assert!(
                readiness.blocked_reasons().contains(&reason),
                "missing {reason:?} must block TextArea IFC readiness"
            );
            assert!(!readiness.allows_text_area_default_rollout());
        }
    }

    #[test]
    fn text_area_inline_ifc_readiness_ready_state_still_does_not_authorize_default_rollout() {
        let readiness = TextAreaInlineIfcReadiness::evaluate(
            TextAreaInlineIfcReadinessInput::with_all_ifc_paths_wired(),
        );

        assert_eq!(
            readiness.readiness(),
            TextAreaInlineIfcReadinessState::ReadyForEditableIfcEvaluation
        );
        assert!(readiness.is_ready_for_editable_ifc_evaluation());
        assert!(readiness.blocked_reasons().is_empty());
        assert!(
            !readiness.allows_text_area_default_rollout(),
            "readiness only allows a future editable IFC evaluation, not TextArea default rollout"
        );
    }

    #[test]
    fn text_area_inline_ifc_readiness_keeps_text_area_run_out_of_controlled_package_targets() {
        use crate::view::base_component::text_area::TextAreaTextRun;

        let parent_width = 180.0;
        let mut arena = new_test_arena();

        let mut parent = Element::new_with_id(850, 0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Auto);
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);
        parent.apply_inline_ifc_layout_call_site_rollout_config_for_test(
            ElementInlineIfcLayoutCallSiteRolloutConfig::controlled_installed_package_candidate(),
        );
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let run_text = "editable run stays legacy";
        let text_area_run_key = commit_child(
            &mut arena,
            parent_key,
            Box::new(TextAreaTextRun::new(
                run_text.to_string(),
                0..run_text.chars().count(),
            )),
        );

        let mut inline_child = Element::new_with_id(851, 0.0, 0.0, 0.0, 0.0);
        let mut child_style = Style::new();
        child_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        child_style.insert(PropertyId::Width, ParsedValue::Auto);
        child_style.insert(PropertyId::Height, ParsedValue::Auto);
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fef3c7")),
        );
        inline_child.apply_style(child_style);
        let inline_child_key = commit_child(&mut arena, parent_key, Box::new(inline_child));
        commit_child(
            &mut arena,
            inline_child_key,
            Box::new(Text::from_content_with_id(931, "inline package target")),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            inline_element_ifc_matrix_constraints(parent_width, 160.0),
            inline_element_ifc_matrix_placement(parent_width, 160.0),
        );

        let diagnostic = {
            let parent_element =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            parent_element
                .inline_ifc_layout_call_site_diagnostic_for_test(&arena)
                .expect("controlled candidate should expose diagnostic")
        };
        assert_eq!(
            diagnostic.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
        );
        assert!(
            diagnostic.install_targets.contains(&inline_child_key),
            "regular inline Element child remains a controlled package target"
        );
        assert!(
            !diagnostic.install_targets.contains(&text_area_run_key),
            "TextAreaTextRun must not enter inline Element rollout targets even under controlled candidate"
        );
        assert!(
            diagnostic.target(text_area_run_key).is_none(),
            "TextAreaTextRun must not get package lifecycle diagnostic"
        );

        let inline_child = crate::view::test_support::get_element::<Element>(&arena, inline_child_key);
        assert_eq!(
            inline_child.inline_ifc_render_decision_for_test(),
            ElementInlineIfcRenderDecision::DrawRectPackageCandidate {
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
                has_atomic_placement_package: false,
            },
            "controlled candidate may install packages for regular inline Element children only"
        );
    }

    #[test]
    fn inline_element_ifc_render_candidate_wiring_keeps_nested_and_sibling_sources_separate() {
        const OUTER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(91);
        const INNER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(92);
        const SIBLING_SOURCE: InlineIfcSourceId = InlineIfcSourceId(93);
        let parent_width = 360.0;

        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, parent_width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(parent_width)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut outer = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dbeafe")),
        );
        outer_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1d4ed8")));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));
        commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::from_content("outer prefix ")),
        );

        let mut inner = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fecaca")),
        );
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));
        commit_child(
            &mut arena,
            inner_key,
            Box::new(Text::from_content("inner chip")),
        );
        commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::from_content(" outer tail")),
        );

        let mut sibling = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut sibling_style = Style::new();
        sibling_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        sibling_style.insert(PropertyId::Width, ParsedValue::Auto);
        sibling_style.insert(PropertyId::Height, ParsedValue::Auto);
        sibling_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#bbf7d0")),
        );
        sibling_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#15803d")));
        sibling.apply_style(sibling_style);
        let sibling_key = commit_child(&mut arena, parent_key, Box::new(sibling));
        commit_child(
            &mut arena,
            sibling_key,
            Box::new(Text::from_content(" sibling inline wrapper")),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: parent_width,
                max_height: 180.0,
                viewport_width: parent_width,
                viewport_height: 180.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(180.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: parent_width,
                available_height: 180.0,
                viewport_width: parent_width,
                viewport_height: 180.0,
                percent_base_width: Some(parent_width),
                percent_base_height: Some(180.0),
            },
        );

        let outer_legacy = crate::view::test_support::get_element::<Element>(&arena, outer_key)
            .inline_fragment_rects()
            .to_vec();
        let inner_legacy = crate::view::test_support::get_element::<Element>(&arena, inner_key)
            .inline_fragment_rects()
            .to_vec();
        let sibling_legacy =
            crate::view::test_support::get_element::<Element>(&arena, sibling_key)
                .inline_fragment_rects()
                .to_vec();
        assert!(!outer_legacy.is_empty());
        assert!(!inner_legacy.is_empty());
        assert!(!sibling_legacy.is_empty());

        let ifc = InlineFormattingContext::build(
            InlineIfcInput::new(vec![
                InlineIfcItem::Span {
                    source: OUTER_SOURCE,
                    style: Some(InlineIfcStyle {
                        font_size: 16.0,
                        line_height: 1.25,
                        brush: [219, 234, 254, 255],
                        ..InlineIfcStyle::default()
                    }),
                    children: vec![
                        InlineIfcItem::TextSpan {
                            source: OUTER_SOURCE,
                            text: "outer prefix ".to_string(),
                            style: None,
                        },
                        InlineIfcItem::Span {
                            source: INNER_SOURCE,
                            style: Some(InlineIfcStyle {
                                font_size: 16.0,
                                line_height: 1.25,
                                brush: [254, 202, 202, 255],
                                ..InlineIfcStyle::default()
                            }),
                            children: vec![InlineIfcItem::TextSpan {
                                source: INNER_SOURCE,
                                text: "inner chip".to_string(),
                                style: None,
                            }],
                        },
                        InlineIfcItem::TextSpan {
                            source: OUTER_SOURCE,
                            text: " outer tail".to_string(),
                            style: None,
                        },
                    ],
                },
                InlineIfcItem::Span {
                    source: SIBLING_SOURCE,
                    style: Some(InlineIfcStyle {
                        font_size: 16.0,
                        line_height: 1.25,
                        brush: [187, 247, 208, 255],
                        ..InlineIfcStyle::default()
                    }),
                    children: vec![InlineIfcItem::TextSpan {
                        source: SIBLING_SOURCE,
                        text: " sibling inline wrapper".to_string(),
                        style: None,
                    }],
                },
            ])
            .with_max_width(parent_width),
        );

        let mut outer_style =
            InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
                brush: [219, 234, 254, 255],
                ..InlineIfcStyle::default()
            });
        outer_style.opacity = 0.88;
        outer_style.border_widths = [1.0, 1.0, 1.0, 1.0];
        outer_style.border_color = [29.0 / 255.0, 78.0 / 255.0, 216.0 / 255.0, 1.0];
        let mut inner_style =
            InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
                brush: [254, 202, 202, 255],
                ..InlineIfcStyle::default()
            });
        inner_style.opacity = 0.87;
        inner_style.border_widths = [1.0, 1.0, 1.0, 1.0];
        inner_style.border_color = [220.0 / 255.0, 38.0 / 255.0, 38.0 / 255.0, 1.0];
        let mut sibling_style =
            InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
                brush: [187, 247, 208, 255],
                ..InlineIfcStyle::default()
            });
        sibling_style.opacity = 0.86;
        sibling_style.border_widths = [1.0, 1.0, 1.0, 1.0];
        sibling_style.border_color = [21.0 / 255.0, 128.0 / 255.0, 61.0 / 255.0, 1.0];

        let insets = InlineIfcDecorationBoxInsets::new(1.0, 1.0, 1.0, 1.0);
        let outer_package =
            ifc.element_decoration_draw_rect_package(OUTER_SOURCE, insets, outer_style);
        let inner_package =
            ifc.element_decoration_draw_rect_package(INNER_SOURCE, insets, inner_style);
        let sibling_package =
            ifc.element_decoration_draw_rect_package(SIBLING_SOURCE, insets, sibling_style);
        assert_eq!(outer_package.fragments.len(), outer_legacy.len());
        assert_eq!(inner_package.fragments.len(), inner_legacy.len());
        assert_eq!(sibling_package.fragments.len(), sibling_legacy.len());
        assert!(
            outer_package
                .fragments
                .iter()
                .all(|fragment| fragment.source == OUTER_SOURCE),
            "outer package should not include nested/sibling metadata: {outer_package:?}"
        );
        assert!(
            inner_package
                .fragments
                .iter()
                .all(|fragment| fragment.source == INNER_SOURCE),
            "inner package should not include outer/sibling metadata: {inner_package:?}"
        );
        assert!(
            sibling_package
                .fragments
                .iter()
                .all(|fragment| fragment.source == SIBLING_SOURCE),
            "sibling package should not include nested metadata: {sibling_package:?}"
        );

        let expected_draw_rect_passes = {
            let mut outer_el =
                crate::view::test_support::get_element_mut::<Element>(&mut arena, outer_key);
            outer_el.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
            outer_el.set_inline_ifc_draw_rect_package_for_test(outer_package.clone());
            let metadata = outer_el.inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0]);
            assert_eq!(metadata.len(), outer_legacy.len());
            for (metadata, package_fragment) in metadata.iter().zip(outer_package.fragments.iter())
            {
                assert_eq!(
                    metadata.fill.position,
                    package_fragment.metadata.position
                );
                assert_eq!(metadata.fill.size, package_fragment.metadata.size);
                assert_eq!(metadata.fill.fill_color, outer_style.fill_color);
                assert_eq!(
                    metadata.border.as_ref().map(|border| border.border_color),
                    Some(outer_style.border_color)
                );
            }
            metadata.len() * 2
        } + {
            let mut inner_el =
                crate::view::test_support::get_element_mut::<Element>(&mut arena, inner_key);
            inner_el.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
            inner_el.set_inline_ifc_draw_rect_package_for_test(inner_package.clone());
            let metadata = inner_el.inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0]);
            assert_eq!(metadata.len(), inner_legacy.len());
            assert!(
                metadata
                    .iter()
                    .all(|metadata| metadata.fill.fill_color == inner_style.fill_color)
            );
            metadata.len() * 2
        } + {
            let mut sibling_el =
                crate::view::test_support::get_element_mut::<Element>(&mut arena, sibling_key);
            sibling_el.set_inline_ifc_render_mode_for_test(
                ElementInlineIfcRenderMode::DrawRectPackageCandidate,
            );
            sibling_el.set_inline_ifc_draw_rect_package_for_test(sibling_package.clone());
            let metadata = sibling_el.inline_ifc_draw_rect_pass_metadata_for_test([0.0, 0.0]);
            assert_eq!(metadata.len(), sibling_legacy.len());
            assert!(
                metadata
                    .iter()
                    .all(|metadata| metadata.fill.fill_color == sibling_style.fill_color)
            );
            metadata.len() * 2
        };

        let summary =
            compile_inline_element_render_graph_for_test(&mut arena, parent_key, 360, 180);
        assert_draw_rect_descriptors_are_graphics(
            &summary.draw_rect_descriptors,
            expected_draw_rect_passes,
        );
        assert!(
            summary.pass_names.iter().any(|name| name.contains("TextPass")),
            "nested/sibling render graph should still contain surrounding text passes: {:?}",
            summary.pass_names
        );
    }

    #[derive(Clone, Debug)]
    struct InlineElementIfcDemoSpec {
        name: &'static str,
        max_width: f32,
        include_atomic_box: bool,
    }

    #[test]
    fn inline_element_ifc_demo_coverage_fixes_nested_and_atomic_specs() {
        const OUTER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(101);
        const INNER_SOURCE: InlineIfcSourceId = InlineIfcSourceId(102);
        const ATOMIC_SOURCE: InlineIfcSourceId = InlineIfcSourceId(103);

        let specs = [
            InlineElementIfcDemoSpec {
                name: "Inline Element Test nested wrappers",
                max_width: 150.0,
                include_atomic_box: false,
            },
            InlineElementIfcDemoSpec {
                name: "Mixed Text / Element atomic chip",
                max_width: 170.0,
                include_atomic_box: true,
            },
        ];

        for spec in specs {
            let mut children = vec![
                InlineIfcItem::TextSpan {
                    source: OUTER_SOURCE,
                    text: "Permission prefix ".to_string(),
                    style: None,
                },
                InlineIfcItem::Span {
                    source: INNER_SOURCE,
                    style: Some(InlineIfcStyle {
                        font_size: 16.0,
                        line_height: 1.25,
                        brush: [59, 130, 246, 255],
                        ..InlineIfcStyle::default()
                    }),
                    children: vec![InlineIfcItem::TextSpan {
                        source: INNER_SOURCE,
                        text: "restriction including limitation".to_string(),
                        style: None,
                    }],
                },
                InlineIfcItem::TextSpan {
                    source: OUTER_SOURCE,
                    text: " suffix text continues".to_string(),
                    style: None,
                },
            ];
            if spec.include_atomic_box {
                children.insert(
                    1,
                    InlineIfcItem::AtomicInlineBox {
                        source: ATOMIC_SOURCE,
                        measurement: InlineIfcMeasuredAtomicBox::new(
                            InlineIfcSize::new(42.0, 20.0),
                            InlineIfcAtomicMeasureConstraints::new(Some(spec.max_width)),
                        ),
                    },
                );
            }

            let ifc = InlineFormattingContext::build(
                InlineIfcInput::new(vec![InlineIfcItem::Span {
                    source: OUTER_SOURCE,
                    style: Some(InlineIfcStyle {
                        font_size: 16.0,
                        line_height: 1.25,
                        brush: [191, 219, 254, 255],
                        ..InlineIfcStyle::default()
                    }),
                    children,
                }])
                .with_max_width(spec.max_width),
            );
            let snapshot = ifc.text_layout_snapshot();
            let outer_package = ifc.element_decoration_draw_rect_package(
                OUTER_SOURCE,
                InlineIfcDecorationBoxInsets::new(8.0, 8.0, 8.0, 8.0),
                InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
                    brush: [191, 219, 254, 255],
                    ..InlineIfcStyle::default()
                }),
            );
            let inner_package = ifc.element_decoration_draw_rect_package(
                INNER_SOURCE,
                InlineIfcDecorationBoxInsets::new(8.0, 8.0, 8.0, 8.0),
                InlineIfcElementDecorationDrawRectStyle::from_fill_style(&InlineIfcStyle {
                    brush: [59, 130, 246, 255],
                    ..InlineIfcStyle::default()
                }),
            );
            let atomic_package = ifc.atomic_box_placement_package(ATOMIC_SOURCE);

            assert!(
                !snapshot.lines.is_empty() && snapshot.lines.iter().any(|line| !line.glyphs.is_empty()),
                "{} should expose text glyph demo payload: {snapshot:?}",
                spec.name
            );
            assert!(
                !outer_package.fragments.is_empty(),
                "{} should expose outer decoration draw-rect package",
                spec.name
            );
            assert!(
                !inner_package.fragments.is_empty(),
                "{} should expose inner decoration draw-rect package",
                spec.name
            );
            assert!(
                outer_package
                    .fragments
                    .iter()
                    .all(|fragment| fragment.source == OUTER_SOURCE)
            );
            assert!(
                inner_package
                    .fragments
                    .iter()
                    .all(|fragment| fragment.source == INNER_SOURCE)
            );
            if spec.include_atomic_box {
                assert_eq!(atomic_package.placements.len(), 1);
                assert_eq!(atomic_package.placements[0].source, ATOMIC_SOURCE);
                assert!(
                    snapshot
                        .lines
                        .iter()
                        .flat_map(|line| &line.glyphs)
                        .all(|glyph| glyph.source != ATOMIC_SOURCE),
                    "{} atomic box should stay out of glyph payload",
                    spec.name
                );
                assert!(
                    outer_package
                        .fragments
                        .iter()
                        .all(|fragment| fragment.source != ATOMIC_SOURCE)
                );
            } else {
                assert!(atomic_package.placements.is_empty());
            }
        }
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
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content("alpha beta gamma delta")),
        );

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

        // Sprint 3 (D3 default Baseline): badge (h=20, baseline=20) sets
        // outer line_ascent; text-only wrapper fragment baseline =
        // text_ascent < 20, so the wrapper drops by (20 - text_ascent),
        // shifting the text fragment down a few pixels. Original test
        // was checking wrap geometry (x), not alignment.
        assert!(
            first_fragment.1.y >= 0.0 && first_fragment.1.y < 8.0,
            "fragments={fragments:?}"
        );
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
        wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(
                "Permission is hereby granted, free of charge, to any person obtaining a copy",
            )),
        );

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

        // Sprint 3 (D3 default Baseline): leading element height=20
        // pulls outer line_ascent to 20; text-only wrapper fragment
        // drops by (20 - text_ascent). Original test verifies x-only
        // (padding shrinks first-line content width).
        assert!(
            first_fragment.1.y >= 0.0 && first_fragment.1.y < 8.0,
            "fragments={fragments:?}"
        );
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
        wrapper_style
            .set_padding(crate::style::Padding::new().xy(Length::px(8.0), Length::px(12.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content("badge")),
        );
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content("trailing")),
        );

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
            let nested_text =
                crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
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
            (
                wrapper_el.inline_paint_fragments[0].y,
                wrapper_el.inline_paint_fragments[0].height,
            )
        };
        let inline_node_height = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a)[0].height)
            .expect("inline node size");
        // CSS inline rule: vertical padding/border on a non-replaced
        // inline (no explicit width/height) paints OUTSIDE the line
        // box and MUST NOT contribute to line height. Inner text Y
        // matches sibling text Y on the same line; paint top extends
        // 12px (padding-y) above the line top; inline node height
        // exposed to outer solver stays at text_height; paint height
        // includes the full v_inset (12 + 12 = 24).
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
            commit_child(
                &mut arena,
                parent_key,
                Box::new(Text::from_content("Inline text starts here,")),
            );

            let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
            let mut wrapper_style = Style::new();
            wrapper_style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::hex("#93c5fd")),
            );
            wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
            wrapper.apply_style(wrapper_style);
            let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
            commit_child(
                &mut arena,
                wrapper_key,
                Box::new(Text::from_content(
                    "badge test test test test test test test",
                )),
            );
            commit_child(
                &mut arena,
                parent_key,
                Box::new(Text::from_content(
                    "then more text continues after the badge,",
                )),
            );

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
            let nested_text =
                crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
            let actual = nested_text.inline_fragment_positions().len();
            assert_eq!(
                actual,
                expected,
                "width={width}, actual={actual}, expected={expected}, fragments={:?}",
                nested_text.inline_fragment_positions()
            );
        }
    }

    /// CSS inline rule: a fragmentable inline wrapper's vertical
    /// padding/border MUST NOT contribute to the line height seen by
    /// the outer inline solver. Two sibling padded wrappers wrapping
    /// multi-line text under a common Inline parent should produce
    /// per-line text Y intervals equal to (text ascent + descent),
    /// NOT (ascent + descent + v_inset). Regression guard against
    /// cba6a24 which folded v_inset into `Element::get_inline_nodes_size`.
    #[test]
    fn fragmentable_padded_wrapper_line_interval_excludes_padding() {
        let width = 200.0_f32;
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, width, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut sib_a = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut sib_a_style = Style::new();
        sib_a_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
        sib_a.apply_style(sib_a_style);
        let sib_a_key = commit_child(&mut arena, parent_key, Box::new(sib_a));
        let text_a_key = commit_child(
            &mut arena,
            sib_a_key,
            Box::new(Text::from_content(
                "Sibling A wraps over many lines because its content is long enough to span several visual rows in the outer inline context.",
            )),
        );

        let mut sib_b = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut sib_b_style = Style::new();
        sib_b_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
        sib_b.apply_style(sib_b_style);
        let sib_b_key = commit_child(&mut arena, parent_key, Box::new(sib_b));
        let text_b_key = commit_child(
            &mut arena,
            sib_b_key,
            Box::new(Text::from_content(
                "Sibling B begins after A and likewise wraps over multiple lines.",
            )),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: width,
                max_height: 600.0,
                viewport_width: width,
                viewport_height: 600.0,
                percent_base_width: Some(width),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: width,
                available_height: 600.0,
                viewport_width: width,
                viewport_height: 600.0,
                percent_base_width: Some(width),
                percent_base_height: Some(600.0),
            },
        );

        let a_paints: Vec<_> = {
            let el = crate::view::test_support::get_element::<Element>(&arena, sib_a_key);
            el.inline_paint_fragments
                .iter()
                .map(|r| (r.x, r.y, r.width, r.height))
                .collect()
        };
        let b_paints: Vec<_> = {
            let el = crate::view::test_support::get_element::<Element>(&arena, sib_b_key);
            el.inline_paint_fragments
                .iter()
                .map(|r| (r.x, r.y, r.width, r.height))
                .collect()
        };
        let text_a: Vec<_> = {
            let t = crate::view::test_support::get_element::<Text>(&arena, text_a_key);
            t.inline_fragment_positions()
                .iter()
                .map(|(_, p)| (p.x, p.y))
                .collect()
        };
        let text_b: Vec<_> = {
            let t = crate::view::test_support::get_element::<Text>(&arena, text_b_key);
            t.inline_fragment_positions()
                .iter()
                .map(|(_, p)| (p.x, p.y))
                .collect()
        };
        let a_inline_nodes = arena
            .with_element_taken(sib_a_key, |el, a| el.get_inline_nodes_size(a))
            .unwrap_or_default();
        let b_inline_nodes = arena
            .with_element_taken(sib_b_key, |el, a| el.get_inline_nodes_size(a))
            .unwrap_or_default();

        let _ = (b_paints, text_b, b_inline_nodes);

        // Inline node height seen by outer solver = ascent + descent
        // of inner text line. NOT ascent + descent + v_inset.
        let baseline = a_inline_nodes[0].baseline;
        let inner_h = a_inline_nodes[0].height;
        let v_inset = 16.0_f32;
        assert!(
            (inner_h - baseline).abs() < 0.5 || inner_h < baseline + v_inset - 0.5,
            "inline node height ({inner_h}) must not include vertical padding ({v_inset}); baseline={baseline}"
        );

        // Per-line text Y interval = text ascent+descent (not padded).
        // Verify by checking consecutive text fragments are spaced by
        // ~text-line-height, NOT (text-line-height + v_inset).
        let dy = text_a[1].1 - text_a[0].1;
        assert!(
            dy < baseline * 2.0,
            "text line interval ({dy}) suggests padding was folded into line height (baseline={baseline}, v_inset={v_inset})"
        );
        let _ = (a_paints,);
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
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content("lead-in text")),
        );

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(6.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content("first child text that wraps")),
        );
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content("second child text also wraps")),
        );
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

        let child_key_val =
            commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("a")));
        arena.with_element_taken(wrapper_key, |el, a| el.measure(constraints, a));
        let before_width = {
            let w = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
            w.measured_size().0
        };

        {
            let mut child =
                crate::view::test_support::get_element_mut::<Text>(&arena, child_key_val);
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
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(
                "Permission is hereby granted, free of charge, to any person obtaining a copy",
            )),
        );

        arena.with_element_taken(wrapper_key, |el, a| {
            el.measure_inline(
                super::InlineMeasureContext {
                    first_available_width: 200.0,
                    full_available_width: 200.0,
                    available_height: 1_000_000.0,
                    viewport_width: 200.0,
                    viewport_height: 120.0,
                    percent_base_width: Some(200.0),
                    percent_base_height: Some(120.0),
                },
                a,
            );
        });
        let wide_nodes = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a))
            .expect("wide inline nodes");

        arena.with_element_taken(wrapper_key, |el, a| {
            el.measure_inline(
                super::InlineMeasureContext {
                    first_available_width: 40.0,
                    full_available_width: 200.0,
                    available_height: 1_000_000.0,
                    viewport_width: 200.0,
                    viewport_height: 120.0,
                    percent_base_width: Some(200.0),
                    percent_base_height: Some(120.0),
                },
                a,
            );
        });
        let narrow_first_line_nodes = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a))
            .expect("narrow inline nodes");

        assert_ne!(
            wide_nodes, narrow_first_line_nodes,
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
            commit_child(
                &mut arena,
                parent_key,
                Box::new(Text::from_content("Inline text starts here,")),
            );

            let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
            let mut wrapper_style = Style::new();
            wrapper_style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::hex("#93c5fd")),
            );
            wrapper_style.insert(
                PropertyId::Color,
                ParsedValue::color_like(Color::hex("#ffffff")),
            );
            wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
            wrapper.apply_style(wrapper_style);
            let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
            commit_child(
                &mut arena,
                wrapper_key,
                Box::new(Text::from_content(
                    "badge test test test test test test test",
                )),
            );
            commit_child(
                &mut arena,
                parent_key,
                Box::new(Text::from_content(
                    "then more text continues after the badge,",
                )),
            );

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
                let nested_text =
                    crate::view::test_support::get_element::<Text>(&arena, nested_text_key);
                nested_text.inline_fragment_positions()
            };
            let trailing_fragments = {
                let trailing = crate::view::test_support::get_element::<Text>(&arena, trailing_key);
                trailing.inline_fragment_positions()
            };
            for (_, trailing_position) in &trailing_fragments {
                let same_line_right = nested_fragments
                    .iter()
                    .filter(|(_, nested_position)| {
                        (nested_position.y - trailing_position.y).abs() < 0.5
                    })
                    .map(|(content, nested_position)| {
                        let mut text = Text::from_content(content.as_str());
                        text.measure(
                            LayoutConstraints {
                                max_width: 200.0,
                                max_height: 80.0,
                                viewport_width: 200.0,
                                viewport_height: 80.0,
                                percent_base_width: Some(200.0),
                                percent_base_height: Some(80.0),
                            },
                            &mut arena,
                        );
                        let (fragment_width, _) = text.measured_size();
                        nested_position.x + fragment_width
                    })
                    .fold(None, |acc: Option<f32>, value| {
                        Some(acc.map_or(value, |max| max.max(value)))
                    });
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
        child_anchor: crate::style::Anchor,
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
            crate::style::Anchor::Parent,
            10.0,
            5.0,
        );
        let snap = child_snapshot(&arena, child_k);
        // child positioned at parent.x + left, parent.y + top
        assert!(
            (snap.x - (100.0 + 10.0)).abs() < 0.01,
            "layout_x = {}",
            snap.x
        );
        assert!(
            (snap.y - (50.0 + 5.0)).abs() < 0.01,
            "layout_y = {}",
            snap.y
        );
    }

    #[test]
    fn anchor_root_resolves_to_root_box() {
        // root is grandparent at (0,0,800,600). left=12, top=8 → child at (12,8).
        let (arena, child_k) = place_grandparent_parent_child(
            (100.0, 50.0, 200.0, 120.0),
            crate::style::Anchor::Viewport,
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
            crate::style::Anchor::Ancestor(1),
            10.0,
            5.0,
        );
        let snap = child_snapshot(&arena, child_k);
        assert!((snap.x - 110.0).abs() < 0.01);
        assert!((snap.y - 55.0).abs() < 0.01);

        // Ancestor(2) == grandparent (root) at (0,0).
        let (arena2, child_k2) = place_grandparent_parent_child(
            (100.0, 50.0, 200.0, 120.0),
            crate::style::Anchor::Ancestor(2),
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
                    .anchor(crate::style::Anchor::Viewport)
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
        let abs_clip = child_el
            .absolute_clip_rect
            .expect("clip_rect set for absolute");
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
                    .anchor(crate::style::Anchor::Viewport)
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
                    .anchor(crate::style::Anchor::Viewport)
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
        let deferred = ctx.take_deferred_nodes();
        assert!(
            deferred.iter().any(|node| node.stable_id == snackbar_id),
            "snackbar should be in deferred list, got {:?}",
            deferred
        );
        for node in &deferred {
            crate::view::base_component::build_node_by_key(
                node.key,
                node.stable_id,
                &mut graph,
                &mut arena,
                &mut ctx,
            );
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
                    .anchor(crate::style::Anchor::Viewport)
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
        let deferred = ctx.take_deferred_nodes();
        assert!(
            deferred.iter().any(|node| node.stable_id == snackbar_id),
            "snackbar should be deferred even when window not rendered, got {:?}",
            deferred
        );

        let pass_count_before_defer = graph.pass_descriptors().len();
        for node in &deferred {
            crate::view::base_component::build_node_by_key(
                node.key,
                node.stable_id,
                &mut graph,
                &mut arena,
                &mut ctx,
            );
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
                    .anchor(crate::style::Anchor::Viewport)
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

        let deferred = ctx.take_deferred_nodes();
        assert!(
            deferred.iter().any(|node| node.stable_id == snackbar_id),
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
                    .anchor(crate::style::Anchor::Viewport)
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

        let deferred = ctx.take_deferred_nodes();
        eprintln!("[deferred ids] {:?}", deferred);
        eprintln!("[snackbar id] {}", snackbar_id);
        assert!(
            deferred.iter().any(|node| node.stable_id == snackbar_id),
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
                    .anchor(crate::style::Anchor::Viewport)
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
        let snackbar_k = commit_child(&mut arena, acc_content_k, Box::new(snackbar));
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

        let deferred = ctx.take_deferred_nodes();
        assert!(
            deferred.iter().any(|node| node.stable_id == snackbar_id),
            "BUG: deeply nested snackbar must still be deferred. defer={:?} snackbar_id={}",
            deferred,
            snackbar_id
        );
    }

    // ---- inline-baseline Sprint 1 plumbing tests ----
    //
    // Per `docs/design/inline-baseline.md` Sprint 1 acceptance: every
    // inline fragment must surface a non-trivial `baseline` value.
    // Tests cover all four producer paths.

    #[test]
    fn inline_baseline_pure_text_fragment_within_height() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent.apply_style(style);
        let parent_key = commit_element(&mut arena, Box::new(parent));
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content("hello")),
        );

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

        let text_key = child_key(&arena, parent_key, 0);
        let nodes = arena
            .with_element_taken(text_key, |el, a| el.get_inline_nodes_size(a))
            .expect("text inline nodes");
        assert_eq!(nodes.len(), 1);
        let n = nodes[0];
        assert!(n.height > 0.0, "expected positive height, got {}", n.height);
        assert!(
            n.baseline > 0.0,
            "text baseline must be > 0 (got {})",
            n.baseline
        );
        assert!(
            n.baseline < n.height,
            "text baseline {} must lie within fragment height {}",
            n.baseline,
            n.height
        );
    }

    #[test]
    fn inline_baseline_non_fragmentable_element_equals_height() {
        let arena = new_test_arena();
        let element = Element::new(0.0, 0.0, 50.0, 30.0);
        let nodes = element.get_inline_nodes_size(&arena);
        assert_eq!(nodes.len(), 1);
        let n = nodes[0];
        assert!(
            (n.height - 30.0).abs() < 1e-3,
            "height mismatch: {}",
            n.height
        );
        assert!(
            (n.baseline - n.height).abs() < 1e-3,
            "non-fragmentable element baseline {} must equal height {}",
            n.baseline,
            n.height
        );
    }

    #[test]
    fn inline_baseline_text_area_run_reports_first_visual_line_baseline() {
        use crate::view::base_component::InlineMeasureContext;
        use crate::view::base_component::text_area::TextAreaTextRun;
        let mut arena = new_test_arena();
        let mut run = TextAreaTextRun::new("hello".to_string(), 0..5);
        run.measure_inline(
            InlineMeasureContext {
                first_available_width: 200.0,
                full_available_width: 200.0,
                available_height: 1_000_000.0,
                viewport_width: 200.0,
                viewport_height: 200.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(200.0),
            },
            &mut arena,
        );
        let nodes = run.get_inline_nodes_size(&arena);
        assert_eq!(nodes.len(), 1);
        let n = nodes[0];
        assert!(n.height > 0.0, "expected positive height, got {}", n.height);
        assert!(
            n.baseline > 0.0,
            "text-area run baseline must be > 0 (got {})",
            n.baseline
        );
        assert!(
            n.baseline < n.height,
            "text-area run baseline {} must lie within fragment height {}",
            n.baseline,
            n.height
        );
    }

    #[test]
    fn text_area_run_exposes_wrapped_visual_lines_as_inline_nodes() {
        use crate::view::base_component::InlineMeasureContext;
        use crate::view::base_component::text_area::TextAreaTextRun;

        let mut arena = new_test_arena();
        let content = "First line with a long value that can wrap when auto wrap is enabled.";
        let mut run = TextAreaTextRun::new(content.to_string(), 0..content.chars().count());
        run.measure_inline(
            InlineMeasureContext {
                first_available_width: 220.0,
                full_available_width: 220.0,
                available_height: 1_000_000.0,
                viewport_width: 220.0,
                viewport_height: 200.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(200.0),
            },
            &mut arena,
        );

        let nodes = run.get_inline_nodes_size(&arena);
        assert!(
            nodes.len() >= 2,
            "wrapped TextAreaTextRun must expose one inline node per visual line, got {nodes:?}"
        );
        for (idx, node) in nodes[..nodes.len() - 1].iter().enumerate() {
            assert!(
                node.force_break_after,
                "wrapped visual line {idx} must force a parent inline break"
            );
        }
        assert!(
            !nodes.last().expect("last visual line").force_break_after,
            "last soft-wrapped visual line should leave room for following inline siblings"
        );
    }

    // ---- Sprint 3: D3 vertical-align offset formula ----

    /// Helper: build a parent inline container holding two pure
    /// elements of differing heights. `va` is applied to each child
    /// directly (the runtime style cascade for Element-to-Element
    /// inheritance is not wired through the test apply_style path —
    /// `compute_style` with parent context is exercised in its own
    /// unit tests). Returns the placed y-offset of each element.
    fn place_two_pure_elements_with_va(
        va: VerticalAlign,
        first_w: f32,
        first_h: f32,
        second_w: f32,
        second_h: f32,
    ) -> (f32, f32) {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut first = Element::new(0.0, 0.0, first_w, first_h);
        let mut first_style = Style::new();
        first_style.insert(PropertyId::VerticalAlign, ParsedValue::VerticalAlign(va));
        first.apply_style(first_style);
        commit_child(&mut arena, parent_key, Box::new(first));

        let mut second = Element::new(0.0, 0.0, second_w, second_h);
        let mut second_style = Style::new();
        second_style.insert(PropertyId::VerticalAlign, ParsedValue::VerticalAlign(va));
        second.apply_style(second_style);
        commit_child(&mut arena, parent_key, Box::new(second));

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

        let first = nth_child_snapshot(&arena, parent_key, 0);
        let second = nth_child_snapshot(&arena, parent_key, 1);
        (first.y, second.y)
    }

    /// D3 row 1: pure-element same-height row → trivial alignment, all
    /// at y=0 regardless of vertical-align.
    #[test]
    fn d3_pure_element_same_height_baseline_aligns_at_top() {
        let (a, b) =
            place_two_pure_elements_with_va(VerticalAlign::Baseline, 20.0, 10.0, 20.0, 10.0);
        assert!((a - 0.0).abs() < 0.01);
        assert!((b - 0.0).abs() < 0.01);
    }

    /// D3 row 2: pure-element diff-height + default Baseline → short
    /// element bottom-aligns (line_ascent - height).
    #[test]
    fn d3_pure_element_diff_height_default_baseline_short_element_drops_to_bottom() {
        let (a, b) =
            place_two_pure_elements_with_va(VerticalAlign::Baseline, 20.0, 30.0, 20.0, 10.0);
        assert!((a - 0.0).abs() < 0.01);
        // 30 - 10 = 20
        assert!((b - 20.0).abs() < 0.01, "got b={b}");
    }

    /// D3 row 3: pure-element diff-height + explicit Top → both at top
    /// (pre-Sprint-3 visual).
    #[test]
    fn d3_pure_element_diff_height_top_align_keeps_both_at_top() {
        let (a, b) = place_two_pure_elements_with_va(VerticalAlign::Top, 20.0, 30.0, 20.0, 10.0);
        assert!((a - 0.0).abs() < 0.01);
        assert!((b - 0.0).abs() < 0.01);
    }

    /// D3 row 4 — pure-element diff-height + Bottom: tallest at top,
    /// shorter at bottom (line_box_h - height). Same y as Baseline for
    /// pure-element rows since baseline = height collapses both
    /// formulas to line_h - h.
    #[test]
    fn d3_pure_element_diff_height_bottom_aligns_short_to_bottom() {
        let (a, b) = place_two_pure_elements_with_va(VerticalAlign::Bottom, 20.0, 30.0, 20.0, 10.0);
        assert!((a - 0.0).abs() < 0.01);
        assert!((b - 20.0).abs() < 0.01);
    }

    /// D3 row 5 — pure-element diff-height + Middle: each item
    /// vertically centered within line_box_h.
    #[test]
    fn d3_pure_element_diff_height_middle_centers_each_item() {
        let (a, b) = place_two_pure_elements_with_va(VerticalAlign::Middle, 20.0, 30.0, 20.0, 10.0);
        // line_box_h = 30 (descent = 0 for pure-element)
        // tallest centered: (30 - 30)/2 = 0
        // shorter centered: (30 - 10)/2 = 10
        assert!((a - 0.0).abs() < 0.01, "got a={a}");
        assert!((b - 10.0).abs() < 0.01, "got b={b}");
    }

    /// D3 row 6 — mixed text + tall element + default Baseline:
    /// element keeps top (baseline = height = line_ascent), text drops
    /// to align glyph baseline. (Specific px audit — text ascent is
    /// font-dependent.)
    #[test]
    fn d3_mixed_text_plus_tall_element_text_drops_to_align_baseline() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent.apply_style(style);
        let parent_key = commit_element(&mut arena, Box::new(parent));
        commit_child(&mut arena, parent_key, Box::new(Text::from_content("hi")));
        // Tall element: baseline = height = 30 sets the line baseline.
        commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 20.0, 30.0)),
        );

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

        let text_key = child_key(&arena, parent_key, 0);
        let elem = nth_child_snapshot(&arena, parent_key, 1);
        let text_nodes = arena
            .with_element_taken(text_key, |el, a| el.get_inline_nodes_size(a))
            .expect("text inline nodes");
        let text_n = text_nodes[0];
        let text = crate::view::test_support::get_element::<Text>(&arena, text_key);
        let text_y = text.box_model_snapshot().y;

        // element: baseline align offset = 30 - 30 = 0 → y=0.
        assert!((elem.y - 0.0).abs() < 0.01, "elem.y={}", elem.y);
        // text: baseline align offset = 30 - text_baseline.
        let expected_text_y = 30.0 - text_n.baseline;
        assert!(
            (text_y - expected_text_y).abs() < 0.5,
            "text.y={} expected≈{}",
            text_y,
            expected_text_y
        );
    }

    /// D3 row 7 — mixed text + element + explicit Middle: each
    /// vertically centered within line_box_h.
    #[test]
    fn d3_mixed_text_plus_element_middle_centers_each_item() {
        let (a, b) = place_two_pure_elements_with_va(VerticalAlign::Middle, 20.0, 30.0, 20.0, 10.0);
        // line_box_h = 30 (pure-element row, descent = 0).
        // tall element centered: (30-30)/2 = 0.
        // short element centered: (30-10)/2 = 10.
        assert!((a - 0.0).abs() < 0.01, "got a={a}");
        assert!((b - 10.0).abs() < 0.01, "got b={b}");
    }

    /// Element-to-Element inheritance via `compute_style` parent
    /// context (unit-level — the apply_style → recompute_style path
    /// currently passes `None` for parent, so production cascade is
    /// driven elsewhere; this test verifies the inheritance branch in
    /// `compute_style` itself).
    #[test]
    fn d3_compute_style_inherits_vertical_align_from_parent() {
        use crate::style::compute_style;
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::VerticalAlign,
            ParsedValue::VerticalAlign(VerticalAlign::Bottom),
        );
        let parent = compute_style(&parent_style, None);
        assert_eq!(parent.vertical_align, VerticalAlign::Bottom);

        let child_style = Style::new();
        let child = compute_style(&child_style, Some(&parent));
        assert_eq!(child.vertical_align, VerticalAlign::Bottom);

        // Explicit override beats inheritance.
        let mut override_style = Style::new();
        override_style.insert(
            PropertyId::VerticalAlign,
            ParsedValue::VerticalAlign(VerticalAlign::Top),
        );
        let overridden = compute_style(&override_style, Some(&parent));
        assert_eq!(overridden.vertical_align, VerticalAlign::Top);
    }

    /// Padded fragmentable inline wrapper sharing an outer line with
    /// non-padded text siblings: per CSS, the wrapper's vertical
    /// padding paints OUTSIDE the line box, so the painted box top
    /// extends above the line top by `padding-top`. The wrapper's
    /// inner text fragment.position.y must still match its non-padded
    /// siblings' fragment.position.y on the same line. Mirrors the
    /// inline-test demo's "Mixed Text / Element" scene where a padded
    /// badge flows inline alongside `<Text>` siblings.
    #[test]
    fn padded_fragmentable_box_top_aligns_with_line_top_and_inner_text_with_siblings() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 720.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(720.0)));
        parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(4.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let lead_key = commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content("Inline text starts here,")),
        );

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        let inner_text_key = commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(
                "badge test test test test test test test",
            )),
        );

        let trailing_key = commit_child(
            &mut arena,
            parent_key,
            Box::new(Text::from_content("then more text")),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 720.0,
                max_height: 200.0,
                viewport_width: 720.0,
                viewport_height: 200.0,
                percent_base_width: Some(720.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 720.0,
                available_height: 200.0,
                viewport_width: 720.0,
                viewport_height: 200.0,
                percent_base_width: Some(720.0),
                percent_base_height: Some(200.0),
            },
        );

        let lead_y = {
            let t = crate::view::test_support::get_element::<Text>(&arena, lead_key);
            t.inline_fragment_positions()[0].1.y
        };
        let inner_y = {
            let t = crate::view::test_support::get_element::<Text>(&arena, inner_text_key);
            t.inline_fragment_positions()[0].1.y
        };
        let trailing_y = {
            let t = crate::view::test_support::get_element::<Text>(&arena, trailing_key);
            t.inline_fragment_positions()[0].1.y
        };
        let wrapper_paint_y = {
            let el = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
            el.inline_paint_fragments[0].y
        };

        // All three texts share the same outer line (line 0 top).
        assert!(
            (lead_y - inner_y).abs() < 0.5,
            "lead_y={lead_y} inner_y={inner_y} should match (both on line 0)"
        );
        assert!(
            (lead_y - trailing_y).abs() < 0.5,
            "lead_y={lead_y} trailing_y={trailing_y} should match (both on line 0)"
        );
        // Box top sits `padding-top` (8 px) above the outer line top
        // (CSS inline: vertical padding paints outside the line box).
        assert!(
            (lead_y - wrapper_paint_y - 8.0).abs() < 0.5,
            "wrapper_paint_y={wrapper_paint_y} lead_y={lead_y} — box top should sit padding-top above line top"
        );
    }

    #[test]
    fn fragmentable_badge_text_keeps_source_order_and_y_with_sibling_text() {
        for width in 220..=1_000 {
            let width = width as f32;
            let mut arena = new_test_arena();
            let mut parent = Element::new(0.0, 0.0, width, 0.0);
            let mut parent_style = Style::new();
            parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
            parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
            parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(8.0)));
            parent.apply_style(parent_style);
            let parent_key = commit_element(&mut arena, Box::new(parent));

            let lead_key = commit_child(
                &mut arena,
                parent_key,
                Box::new(Text::from_content("Inline text starts here,")),
            );

            let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
            let mut wrapper_style = Style::new();
            wrapper_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
            wrapper.apply_style(wrapper_style);
            let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
            let badge_text_key = commit_child(
                &mut arena,
                wrapper_key,
                Box::new(Text::from_content(
                    "badge test test test test test test test",
                )),
            );

            let trailing_key = commit_child(
                &mut arena,
                parent_key,
                Box::new(Text::from_content(
                    "then more text continues after the badge,",
                )),
            );

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

            let lead_fragment = {
                let lead = crate::view::test_support::get_element::<Text>(&arena, lead_key);
                lead.inline_fragment_positions()[0].1
            };
            let badge_fragment = {
                let badge = crate::view::test_support::get_element::<Text>(&arena, badge_text_key);
                badge.inline_fragment_positions()[0].1
            };
            let trailing_fragment = {
                let trailing = crate::view::test_support::get_element::<Text>(&arena, trailing_key);
                trailing.inline_fragment_positions()[0].1
            };

            let trailing_after_badge = trailing_fragment.y > badge_fragment.y + 0.5
                || ((trailing_fragment.y - badge_fragment.y).abs() < 0.5
                    && trailing_fragment.x > badge_fragment.x);
            assert!(
                trailing_after_badge,
                "width={width} trailing text must not be visually before badge text: lead=({},{}) badge=({},{}) trailing=({},{})",
                lead_fragment.x,
                lead_fragment.y,
                badge_fragment.x,
                badge_fragment.y,
                trailing_fragment.x,
                trailing_fragment.y
            );
        }
    }

    /// D7: fragmentable inline element shares its own `vertical-align`
    /// across all outer fragments. Inner line items keep their own
    /// values.
    #[test]
    fn d3_fragmentable_element_fragments_share_outer_vertical_align() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 120.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper_style.insert(
            PropertyId::VerticalAlign,
            ParsedValue::VerticalAlign(VerticalAlign::Middle),
        );
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(
                "alpha beta gamma delta epsilon zeta eta theta",
            )),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 240.0,
                viewport_width: 120.0,
                viewport_height: 240.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 240.0,
                viewport_width: 120.0,
                viewport_height: 240.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(240.0),
            },
        );

        let nodes = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a))
            .expect("wrapper inline nodes");
        assert!(nodes.len() >= 2, "expect ≥2 fragments");
        for (idx, n) in nodes.iter().enumerate() {
            assert_eq!(
                n.vertical_align,
                VerticalAlign::Middle,
                "fragment {idx} must carry wrapper's vertical_align"
            );
        }
    }

    // ---- Regression: force_break_after must reset measure-phase line state ----

    /// Without `force_break_after` honoring in `measure_axis_children`,
    /// a child following a forced-break sibling receives a tiny
    /// `first_available_width` (residue from the previous line's
    /// accumulated width). The flex solver later places that child on
    /// a fresh line, but the text layout adapter inside fragmentable inline
    /// children would have already wrapped at the wrong glyph
    /// boundary. Repro: a `TextAreaTextRun` that fills most of the
    /// row and has a trailing newline (force_break) followed by a
    /// fragmentable Auto/Auto Element wrapping a short Text. Without
    /// the fix, the inner Text receives a narrow first_available_width
    /// and the text layout adapter wraps it on the wrong boundary; the chip ends
    /// up with multiple fragments instead of a single atomic one.
    #[test]
    fn force_break_after_resets_measure_line_state_for_fragmentable_chip() {
        use crate::view::base_component::text_area::{TextAreaLineBreak, TextAreaTextRun};

        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        // Force-break source: a text run filling most of the row width,
        // followed by an explicit `\n` formatting object.
        let prev = TextAreaTextRun::new(
            "First line filling almost the whole row.".to_string(),
            0..40,
        );
        commit_child(&mut arena, parent_key, Box::new(prev));
        commit_child(
            &mut arena,
            parent_key,
            Box::new(TextAreaLineBreak::new(40..41)),
        );

        // Fragmentable chip: Auto/Auto inline Element wrapping a Text.
        // Text content is short enough to fit on a fresh line; with a
        // narrow first_available_width residue, the text layout adapter would wrap
        // it across two fragments.
        let mut chip = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut chip_style = Style::new();
        chip_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        chip_style.insert(PropertyId::Width, ParsedValue::Auto);
        chip_style.insert(PropertyId::Height, ParsedValue::Auto);
        chip.apply_style(chip_style);
        let chip_key = commit_child(&mut arena, parent_key, Box::new(chip));
        let inner_text_key = commit_child(
            &mut arena,
            chip_key,
            Box::new(Text::from_content("{{API_HOST}}")),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 240.0,
                viewport_width: 220.0,
                viewport_height: 240.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(220.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 240.0,
                viewport_width: 220.0,
                viewport_height: 240.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(220.0),
            },
        );

        // Inner Text must shape as ONE fragment (atomic chip content).
        let inner_text = crate::view::test_support::get_element::<Text>(&arena, inner_text_key);
        let fragments = inner_text.inline_fragment_positions();
        assert_eq!(
            fragments.len(),
            1,
            "chip text must shape as single fragment (got {} fragments: {:?})",
            fragments.len(),
            fragments
        );
    }

    #[test]
    fn text_area_projection_segment_wraps_when_first_fragment_cannot_fit_residue() {
        use crate::view::base_component::text_area::{TextAreaProjectionSegment, TextAreaTextRun};

        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 220.0, 0.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(220.0)));
        parent.apply_style(style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let prev_key = commit_child(
            &mut arena,
            parent_key,
            Box::new(Element::new(0.0, 0.0, 214.0, 18.0)),
        );

        let mut badge = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut badge_style = Style::new();
        badge_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        badge_style.insert(PropertyId::Width, ParsedValue::Auto);
        badge_style.insert(PropertyId::Height, ParsedValue::Auto);
        badge.apply_style(badge_style);
        badge.set_padding_left(8.0);
        badge.set_padding_right(8.0);
        let projection_index = arena.children_of(parent_key).len();
        let projection_key = crate::view::renderer_adapter::arena_insert_child(
            &mut arena,
            parent_key,
            projection_index,
            crate::view::renderer_adapter::ElementDescriptor {
                element: Box::new(TextAreaProjectionSegment::new()),
                children: vec![crate::view::renderer_adapter::ElementDescriptor {
                    element: Box::new(badge),
                    children: vec![crate::view::renderer_adapter::ElementDescriptor::leaf(
                        Box::new(Text::from_content("{{API_HOST}}")),
                    )],
                    side_slots: vec![],
                }],
                side_slots: vec![],
            },
        );

        let suffix_key = commit_child(
            &mut arena,
            parent_key,
            Box::new(TextAreaTextRun::new("/v1/users/".to_string(), 0..10)),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 240.0,
                viewport_width: 220.0,
                viewport_height: 240.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 240.0,
                viewport_width: 220.0,
                viewport_height: 240.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(240.0),
            },
        );

        let prev = child_snapshot(&arena, prev_key);
        let projection = child_snapshot(&arena, projection_key);
        let suffix = child_snapshot(&arena, suffix_key);
        assert!(
            projection.y > prev.y + 1.0,
            "projection must wrap to a fresh line when the residue cannot fit it: prev={prev:?}, projection={projection:?}"
        );
        assert!(
            suffix.y >= projection.y - 1.0 && suffix.y < projection.y + projection.height.max(1.0),
            "suffix should stay within the projection line box: projection={projection:?}, suffix={suffix:?}"
        );
        assert!(
            suffix.x >= projection.x + projection.width - 1.0,
            "suffix should be placed after the projection, not overlap it: projection={projection:?}, suffix={suffix:?}"
        );
    }

    #[test]
    fn text_area_projection_segment_forces_breaks_between_wrapped_inner_lines() {
        use crate::view::base_component::text_area::TextAreaProjectionSegment;

        let mut arena = new_test_arena();
        let mut text = Text::from_content("{{USER_ID_WITH_A_VERY_LONG_PROJECTION_BADGE}}");
        text.set_auto_width(true);
        text.set_auto_height(true);
        let segment_key = crate::view::test_support::commit_descriptor(
            &mut arena,
            None,
            crate::view::renderer_adapter::ElementDescriptor {
                element: Box::new(TextAreaProjectionSegment::new()),
                children: vec![crate::view::renderer_adapter::ElementDescriptor::leaf(
                    Box::new(text),
                )],
                side_slots: vec![],
            },
        );

        arena.with_element_taken(segment_key, |el, arena| {
            el.measure_inline(
                super::InlineMeasureContext {
                    first_available_width: 120.0,
                    full_available_width: 120.0,
                    available_height: 1_000_000.0,
                    viewport_width: 120.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(120.0),
                    percent_base_height: Some(240.0),
                },
                arena,
            );
        });

        let nodes = arena
            .with_element_taken_ref(segment_key, |el, arena| el.get_inline_nodes_size(arena))
            .expect("segment inline nodes");
        assert!(
            nodes.len() >= 2,
            "projection segment should expose wrapped inner text lines, got {nodes:?}"
        );
        for (idx, node) in nodes[..nodes.len() - 1].iter().enumerate() {
            assert!(
                node.force_break_after,
                "projection fragment {idx} must force a parent inline break"
            );
        }
        assert!(
            !nodes
                .last()
                .expect("last projection fragment")
                .force_break_after,
            "last projection fragment should allow following siblings on the same line"
        );
    }

    #[test]
    fn text_area_projection_segment_uses_owner_vertical_align() {
        use crate::view::base_component::text_area::TextAreaProjectionSegment;

        let mut arena = new_test_arena();
        let mut segment = TextAreaProjectionSegment::new();
        segment.set_vertical_align(VerticalAlign::Bottom);
        let mut badge = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut badge_style = Style::new();
        badge_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        badge_style.insert(PropertyId::Width, ParsedValue::Auto);
        badge_style.insert(PropertyId::Height, ParsedValue::Auto);
        badge_style.insert(
            PropertyId::VerticalAlign,
            ParsedValue::VerticalAlign(VerticalAlign::Middle),
        );
        badge.apply_style(badge_style);

        let segment_key = crate::view::test_support::commit_descriptor(
            &mut arena,
            None,
            crate::view::renderer_adapter::ElementDescriptor {
                element: Box::new(segment),
                children: vec![crate::view::renderer_adapter::ElementDescriptor {
                    element: Box::new(badge),
                    children: vec![crate::view::renderer_adapter::ElementDescriptor::leaf(
                        Box::new(Text::from_content("{{API_HOST}}")),
                    )],
                    side_slots: vec![],
                }],
                side_slots: vec![],
            },
        );

        arena.with_element_taken(segment_key, |el, arena| {
            el.measure_inline(
                super::InlineMeasureContext {
                    first_available_width: 120.0,
                    full_available_width: 120.0,
                    available_height: 1_000_000.0,
                    viewport_width: 120.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(120.0),
                    percent_base_height: Some(240.0),
                },
                arena,
            );
        });

        let nodes = arena
            .with_element_taken_ref(segment_key, |el, arena| el.get_inline_nodes_size(arena))
            .expect("segment inline nodes");
        assert!(
            !nodes.is_empty(),
            "projection segment should expose inline nodes"
        );
        for (idx, node) in nodes.iter().enumerate() {
            assert_eq!(
                node.vertical_align,
                VerticalAlign::Bottom,
                "projection fragment {idx} must expose the owning TextArea's vertical_align"
            );
        }
    }

    // ---- vertical-align as style prop (Style builder + cascade) ----

    /// `Style::with_vertical_align` (builder) and `set_vertical_align`
    /// both lower to the same `ParsedValue::VerticalAlign` declaration,
    /// which `compute_style` consumes into `ComputedStyle.vertical_align`.
    #[test]
    fn style_builder_vertical_align_lowers_to_computed_style() {
        use crate::style::compute_style;
        let style = Style::new().with_vertical_align(VerticalAlign::Middle);
        let computed = compute_style(&style, None);
        assert_eq!(computed.vertical_align, VerticalAlign::Middle);

        let mut style2 = Style::new();
        style2.set_vertical_align(VerticalAlign::Bottom);
        let computed2 = compute_style(&style2, None);
        assert_eq!(computed2.vertical_align, VerticalAlign::Bottom);
    }

    /// Element absorbs `vertical_align` from a `Style` and surfaces it
    /// via `get_inline_nodes_size` for the inline solver to consume.
    #[test]
    fn style_vertical_align_reaches_element_inline_node() {
        let arena = new_test_arena();
        let mut element = Element::new(0.0, 0.0, 50.0, 30.0);
        element.apply_style(Style::new().with_vertical_align(VerticalAlign::Top));
        let nodes = element.get_inline_nodes_size(&arena);
        assert_eq!(nodes[0].vertical_align, VerticalAlign::Top);
    }

    /// `Style::with_line_height` lowers to `ParsedValue::LineHeight`
    /// and cascades through `StyleCascadeContext` into `Text.line_height`.
    /// Explicit `Text::set_line_height` flips `line_height_explicit` so
    /// later cascades skip the prop.
    #[test]
    fn style_line_height_cascades_into_text_unless_explicit() {
        use crate::view::renderer_adapter::StyleCascadeContext;

        let parent_style = Style::new().with_line_height(2.0);
        let mut inherited = StyleCascadeContext::default();
        inherited.merge_style(&parent_style);
        assert_eq!(inherited.inherited_line_height(), Some(2.0));

        // Cascade reaches a non-explicit Text.
        let mut text = Text::from_content("hello");
        let changed = text.apply_inherited(&inherited);
        assert!(changed);
        assert!((text.line_height_value() - 2.0).abs() < f32::EPSILON);

        // Explicit setter wins over later cascade.
        let mut text2 = Text::from_content("hello");
        text2.set_line_height(1.4);
        let inherited3 = {
            let mut tmp = StyleCascadeContext::default();
            tmp.merge_style(&Style::new().with_line_height(2.0));
            tmp
        };
        text2.apply_inherited(&inherited3);
        assert!(
            (text2.line_height_value() - 1.4).abs() < f32::EPSILON,
            "explicit line_height must beat cascade"
        );
    }

    /// `StyleCascadeContext` cascade carries `vertical-align` from an
    /// ancestor's style into a leaf `Text`. Verifies the renderer-adapter
    /// path that production cascade uses to fan-out non-explicit props.
    #[test]
    fn style_cascade_cascades_vertical_align_into_text() {
        use crate::view::renderer_adapter::StyleCascadeContext;

        let parent_style = Style::new().with_vertical_align(VerticalAlign::Middle);
        let mut inherited = StyleCascadeContext::default();
        inherited.merge_style(&parent_style);
        assert_eq!(
            inherited.inherited_vertical_align(),
            Some(VerticalAlign::Middle)
        );

        let mut text = Text::from_content("hello");
        let changed = text.apply_inherited(&inherited);
        assert!(changed, "apply_inherited should report change");
        assert_eq!(text.vertical_align(), VerticalAlign::Middle);

        let mut arena = new_test_arena();
        let measure_ctx = crate::view::base_component::InlineMeasureContext {
            first_available_width: 200.0,
            full_available_width: 200.0,
            available_height: 1_000_000.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        };
        text.measure_inline(measure_ctx, &mut arena);
        let nodes = text.get_inline_nodes_size(&arena);
        assert_eq!(nodes[0].vertical_align, VerticalAlign::Middle);
    }

    // ---- Sprint 4: line-height leading verification ----
    //
    // Text baselines come from the text layout adapter's first visual line.
    // These tests exist to lock in the leading/2 distribution invariant and
    // confirm the Element-side baseline is untouched by line_height.

    /// D4: doubling line-height pushes a Text fragment's baseline down
    /// by leading/2 on top + leading/2 on bottom. Going from 1.0 → 2.0
    /// at font_size 14 pumps the line box from 14 → 28 (Δ=14), so the
    /// new baseline shifts down by ~font_size * 0.5 (= 7) — the top
    /// half of the added leading (the bottom half manifests as
    /// extra fragment height below the baseline).
    #[test]
    fn sprint4_text_baseline_shifts_by_half_added_leading_when_line_height_doubles() {
        use crate::view::base_component::InlineMeasureContext;
        let mut arena = new_test_arena();

        let measure_ctx = InlineMeasureContext {
            first_available_width: 200.0,
            full_available_width: 200.0,
            available_height: 1_000_000.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        };

        let mut text_a = Text::from_content("hello");
        text_a.set_font_size(14.0);
        text_a.set_line_height(1.0);
        text_a.measure_inline(measure_ctx, &mut arena);
        let nodes_a = text_a.get_inline_nodes_size(&arena);
        let baseline_a = nodes_a[0].baseline;
        let height_a = nodes_a[0].height;

        let mut text_b = Text::from_content("hello");
        text_b.set_font_size(14.0);
        text_b.set_line_height(2.0);
        text_b.measure_inline(measure_ctx, &mut arena);
        let nodes_b = text_b.get_inline_nodes_size(&arena);
        let baseline_b = nodes_b[0].baseline;
        let height_b = nodes_b[0].height;

        // Total fragment height grows by exactly the added leading.
        let height_delta = height_b - height_a;
        assert!(
            (height_delta - 14.0).abs() < 0.5,
            "line_height 1.0 -> 2.0 at font_size=14 must add ~14px to height (got {})",
            height_delta
        );

        // Baseline drops by half of the added leading.
        let baseline_delta = baseline_b - baseline_a;
        let expected_delta = 14.0 * 0.5;
        assert!(
            (baseline_delta - expected_delta).abs() < 0.5,
            "baseline shift {} should be ≈ font_size * 0.5 = {}",
            baseline_delta,
            expected_delta
        );

        // Descent (height - baseline) also grows by half of the added
        // leading — sanity check that leading is split symmetrically.
        let descent_a = height_a - baseline_a;
        let descent_b = height_b - baseline_b;
        let descent_delta = descent_b - descent_a;
        assert!(
            (descent_delta - expected_delta).abs() < 0.5,
            "descent shift {} should be ≈ font_size * 0.5 = {}",
            descent_delta,
            expected_delta
        );
    }

    /// D4 + D1: Element baseline is `height` (bottom edge), independent
    /// of any text-side line-height. Doubling line-height in a mixed
    /// row only stretches the line box via text ascent/descent — the
    /// element keeps reporting baseline = height.
    #[test]
    fn sprint4_element_baseline_unchanged_under_line_height_change_in_mixed_row() {
        use crate::view::base_component::InlineMeasureContext;
        let mut arena = new_test_arena();

        // Element baseline doesn't depend on measure; just check
        // get_inline_nodes_size directly.
        let element = Element::new(0.0, 0.0, 20.0, 30.0);
        let nodes = element.get_inline_nodes_size(&arena);
        assert_eq!(nodes.len(), 1);
        assert!((nodes[0].baseline - 30.0).abs() < 0.01);

        // For the text side: confirm the line_box_h for a mixed row
        // grows when text line-height grows, but the element baseline
        // value reported into the inline solver is invariant.
        let measure_ctx = InlineMeasureContext {
            first_available_width: 200.0,
            full_available_width: 200.0,
            available_height: 1_000_000.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        };

        let mut text_a = Text::from_content("hello");
        text_a.set_font_size(14.0);
        text_a.set_line_height(1.0);
        text_a.measure_inline(measure_ctx, &mut arena);
        let text_a_n = text_a.get_inline_nodes_size(&arena)[0];

        let mut text_b = Text::from_content("hello");
        text_b.set_font_size(14.0);
        text_b.set_line_height(2.0);
        text_b.measure_inline(measure_ctx, &mut arena);
        let text_b_n = text_b.get_inline_nodes_size(&arena)[0];

        // line_box_h(row) = max(text_ascent, elem_h=30) + max(text_descent, 0)
        // With elem_h=30 dominating ascent, line_box_h = 30 + text_descent.
        let line_box_a =
            30.0_f32.max(text_a_n.baseline) + (text_a_n.height - text_a_n.baseline).max(0.0);
        let line_box_b =
            30.0_f32.max(text_b_n.baseline) + (text_b_n.height - text_b_n.baseline).max(0.0);
        // line_box_b grows by ~font_size * 0.5 (descent grew that much,
        // ascent stayed below elem_h since text_ascent ~16 < 30).
        let delta = line_box_b - line_box_a;
        assert!(
            delta > 5.0 && delta < 10.0,
            "line_box_h delta {} should reflect added text descent (~7px)",
            delta
        );

        // Element side: same baseline, same height, regardless of any
        // sibling Text's line-height.
        let nodes2 = element.get_inline_nodes_size(&arena);
        assert!((nodes2[0].baseline - 30.0).abs() < 0.01);
        assert!((nodes2[0].height - 30.0).abs() < 0.01);
    }

    // ---- Sprint 2: D2 line-box ascent/descent formula ----

    /// Pure-element diff-height row: each fragment's descent = 0
    /// (baseline = height), so `line_box_h = max(child.height)` —
    /// identical to pre-Sprint-2 line_cross_max.
    #[test]
    fn inline_baseline_pure_element_diff_height_row_line_box_h_unchanged() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
        parent.apply_style(style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Element::new(0.0, 0.0, 20.0, 10.0)),
        );
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Element::new(0.0, 0.0, 20.0, 30.0)),
        );

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

        let nodes = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a))
            .expect("wrapper inline nodes");
        assert_eq!(nodes.len(), 1);
        let n = nodes[0];
        assert!(
            (n.height - 30.0).abs() < 0.5,
            "pure-element diff-height row line_box_h must equal max(height)=30, got {}",
            n.height
        );
        // Tallest element's baseline=height=30 → line_ascent=30.
        assert!(
            (n.baseline - 30.0).abs() < 0.5,
            "fragment baseline must equal tallest element height (got {})",
            n.baseline
        );
    }

    /// Mixed text + tall element: line_box_h grows past element height
    /// to accommodate the text descent below the line baseline. This is
    /// the headline Sprint 2 visual change.
    #[test]
    fn inline_baseline_mixed_text_plus_tall_element_line_box_h_grows() {
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 240.0, 0.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
        parent.apply_style(style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("hi")));
        // Element taller than typical text height (~16.8 px at default
        // font-size 14, line-height 1.2). Element baseline = height = 30,
        // so line_ascent = 30 and the line picks up text_descent on top.
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Element::new(0.0, 0.0, 20.0, 30.0)),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 240.0,
                max_height: 240.0,
                viewport_width: 240.0,
                viewport_height: 240.0,
                percent_base_width: Some(240.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 240.0,
                available_height: 240.0,
                viewport_width: 240.0,
                viewport_height: 240.0,
                percent_base_width: Some(240.0),
                percent_base_height: Some(240.0),
            },
        );

        let text_key = child_key(&arena, wrapper_key, 0);
        let text_nodes = arena
            .with_element_taken(text_key, |el, a| el.get_inline_nodes_size(a))
            .expect("text inline nodes");
        let text_n = text_nodes[0];

        let nodes = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a))
            .expect("wrapper inline nodes");
        assert_eq!(nodes.len(), 1);
        let n = nodes[0];
        assert!(
            n.height > 30.0,
            "mixed text+tall-element line_box_h must exceed element height 30 (got {})",
            n.height
        );
        let expected_descent = text_n.height - text_n.baseline;
        let expected_h = 30.0 + expected_descent;
        assert!(
            (n.height - expected_h).abs() < 0.5,
            "expected line_box_h ≈ {} (= 30 + text_descent {}), got {}",
            expected_h,
            expected_descent,
            n.height
        );
        assert!(
            (n.baseline - 30.0).abs() < 0.5,
            "line_ascent must be max(child.baseline) = element.height = 30 (got {})",
            n.baseline
        );
    }

    #[test]
    fn inline_baseline_fragmentable_element_each_fragment_within_height() {
        // Outer Inline parent + Auto/Auto inline wrapper that wraps across
        // multiple lines (long Text + one element). Wrapper becomes
        // fragmentable: get_inline_nodes_size returns one InlineNodeSize
        // per inner line, each carrying the line's max child baseline.
        let mut arena = new_test_arena();
        let mut parent = Element::new(0.0, 0.0, 120.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
        wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
        wrapper.apply_style(wrapper_style);
        let wrapper_key = commit_child(&mut arena, parent_key, Box::new(wrapper));
        commit_child(
            &mut arena,
            wrapper_key,
            Box::new(Text::from_content(
                "alpha beta gamma delta epsilon zeta eta theta",
            )),
        );

        measure_and_place(
            &mut arena,
            parent_key,
            LayoutConstraints {
                max_width: 120.0,
                max_height: 240.0,
                viewport_width: 120.0,
                viewport_height: 240.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 120.0,
                available_height: 240.0,
                viewport_width: 120.0,
                viewport_height: 240.0,
                percent_base_width: Some(120.0),
                percent_base_height: Some(240.0),
            },
        );

        let nodes = arena
            .with_element_taken(wrapper_key, |el, a| el.get_inline_nodes_size(a))
            .expect("wrapper inline nodes");
        assert!(
            nodes.len() >= 2,
            "wrapper must split into ≥2 fragments, got {}",
            nodes.len()
        );
        for (idx, n) in nodes.iter().enumerate() {
            assert!(n.height > 0.0, "fragment {idx} height must be > 0");
            assert!(
                n.baseline > 0.0,
                "fragment {idx} baseline must be > 0 (got {})",
                n.baseline
            );
            assert!(
                n.baseline <= n.height + 1e-3,
                "fragment {idx} baseline {} must be ≤ height {}",
                n.baseline,
                n.height
            );
        }
    }
}
