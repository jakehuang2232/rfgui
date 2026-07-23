use super::*;

#[test]
fn unified_ifc_staging_applies_vertical_align_to_paint_local_pos() {
    let mut text_area = TextArea::new();
    text_area.content = "abXYZcd".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.vertical_align = crate::style::VerticalAlign::Bottom;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..5, |_text_area_node| {
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop(
                "style",
                ElementStylePropSchema {
                    width: Some(Length::px(90.0)),
                    height: Some(Length::px(42.0)),
                    ..Default::default()
                },
            )
        });
    }));

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 300.0,
            max_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 10.0,
            parent_y: 20.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );

    arena
        .with_element_taken_ref(root, |el, arena| {
            let text_area = el
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            let package = text_area
                .unified_inline_ifc_render_package(arena)
                .expect("unified render package");
            let origin = [7.0_f32, 11.0_f32];
            let paint_input = package.ifc.text_pass_paint_input();
            let staging = package.text_pass_staging_input(origin, 1.0, 0, 1.0);
            assert!(!staging.glyphs.is_empty(), "fixture should stage glyphs");
            let mut saw_vertical_shift = false;
            for (staged, raw) in staging.glyphs.iter().zip(paint_input.glyphs.iter()) {
                let raw_local_y = raw.baseline_y + raw.glyph_y;
                if (staged.paint.local_pos[1] - raw_local_y).abs() > 0.5 {
                    saw_vertical_shift = true;
                }
                assert!(
                    (staged.final_paint_pos[0] - (origin[0] + staged.paint.local_pos[0]))
                        .abs()
                        < 1e-3
                        && (staged.final_paint_pos[1]
                            - (origin[1] + staged.paint.local_pos[1]))
                            .abs()
                            < 1e-3,
                    "prepared pass consumes paint.local_pos; it must stay in sync with final_paint_pos",
                );
            }
            assert!(
                saw_vertical_shift,
                "fixture should exercise a nonzero vertical-align delta"
            );
        })
        .expect("root exists");
}

#[test]
fn text_area_inline_ifc_projection_unified_render_skips_per_run_text_passes() {
    let (mut arena, root) = projection_fixture(3, false);

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    arena
        .with_element_taken(root, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("TextArea build returns state");

    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|desc| desc.name.to_string())
        .collect::<Vec<_>>();
    let prepared_text_pass_count = pass_names
        .iter()
        .filter(|name| name.ends_with("render_pass::text_pass::TextPreparedInputPass"))
        .count();
    assert_eq!(
        prepared_text_pass_count, 1,
        "projection TextArea should render plain glyphs once from the TextArea-level unified IFC package, got {pass_names:?}"
    );
}

#[test]
fn text_area_inline_ifc_plain_unified_render_skips_per_run_text_passes() {
    let (mut arena, root) = wrapped_plain_fixture("plain root render", 240.0);

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    arena
        .with_element_taken(root, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("TextArea build returns state");

    let pass_names = graph
        .pass_descriptors()
        .into_iter()
        .map(|desc| desc.name.to_string())
        .collect::<Vec<_>>();
    let prepared_text_pass_count = pass_names
        .iter()
        .filter(|name| name.ends_with("render_pass::text_pass::TextPreparedInputPass"))
        .count();
    assert_eq!(
        prepared_text_pass_count, 1,
        "plain TextArea should render glyphs once from the TextArea-level unified IFC package, got {pass_names:?}"
    );
}
