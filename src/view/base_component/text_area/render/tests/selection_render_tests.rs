use super::*;

#[test]
fn projection_selection_uses_text_rects_instead_of_projection_bounds() {
    let mut text_area = TextArea::new();
    text_area.content = "ab/activity/with/a/very/long/pathcd".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.select_range(19, 28);
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..34, |_text_area_node| {
            let style = ElementStylePropSchema {
                width: Some(Length::px(120.0)),
                height: Some(Length::px(80.0)),
                ..Default::default()
            };
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop("style", style)
            .with_child(
                RsxNode::tagged(
                    "Text",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                )
                .with_child(RsxNode::text("/activity/with/a/very/long/path")),
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

    let projection_snap = projection_snapshot(&arena, root);
    let root_el = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .projection_selection_context_for_child(1, projection_key(&arena, root), arena)
        })
        .expect("root exists");

    let context = root_el.expect("expected projection selection render context");
    assert_eq!(context.start, 17);
    assert_eq!(context.end, 26);
    let text_key = first_text_descendant(&arena, projection_key(&arena, root));
    let rects = arena
        .with_element_taken_ref(text_key, |el, _| {
            el.as_any()
                .downcast_ref::<Text>()
                .expect("projection Text")
                .local_selection_screen_rects(context.start, context.end)
        })
        .expect("text exists");

    assert!(
        !rects.is_empty(),
        "expected projection text selection rects"
    );
    assert!(
        rects
            .iter()
            .all(|rect| rect.height < projection_snap.height - 1.0),
        "selection should use visual text-line rects, not projection bounds: rects={rects:?}, projection={projection_snap:?}"
    );
    assert!(
        rects
            .iter()
            .any(|rect| rect.width < projection_snap.width - 1.0),
        "selection should be narrower than the projection union bounds: rects={rects:?}, projection={projection_snap:?}"
    );
}

#[test]
fn projection_selection_underlay_renders_for_ifc_owned_inner_text() {
    use crate::view::base_component::UiBuildContext;
    use crate::view::frame_graph::FrameGraph;

    // Select-all must paint selection rects inside the chip: the inner
    // Text is IFC-owned (glyphs come from the chip root's unified
    // pass), but the selection underlay still belongs to the Text.
    let build_pass_names = |selected: bool| -> Vec<&'static str> {
        let (mut arena, root) = projection_fixture(0, true);
        arena.with_element_taken(root, |el, _| {
            let ta = el
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root");
            if selected {
                // Select exactly the projection range (2..5): the
                // committed-text selection layer contributes nothing
                // here, so any added rect is the chip's underlay.
                ta.selection_anchor_char = Some(2);
                ta.selection_focus_char = Some(5);
            }
        });
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(400, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        arena
            .with_element_taken(root, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("build result");
        graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect()
    };

    let unselected_rects = build_pass_names(false)
        .iter()
        .filter(|name| name.contains("DrawRectPass"))
        .count();
    let selected_rects = build_pass_names(true)
        .iter()
        .filter(|name| name.contains("DrawRectPass"))
        .count();
    assert!(
        selected_rects > unselected_rects,
        "selecting across a projection must add selection underlay rects inside the chip: unselected={unselected_rects} selected={selected_rects}"
    );
}
