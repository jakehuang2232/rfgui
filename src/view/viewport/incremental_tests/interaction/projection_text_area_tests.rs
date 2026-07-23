use super::*;

#[test]
fn projection_text_area_caret_stays_at_insertion_point_across_frames() {
    // Chip ranges in the fixture: {{API_HOST}}=69..81, {{USER_ID}}=91..102.
    for cursor in [10_usize, 68, 69, 81, 82, 90, 91, 102, 103, 130] {
        projection_caret_probe_at(cursor);
    }
}

#[test]
fn projection_text_area_auto_wrap_toggle_keeps_all_rows_visible() {
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea as TextAreaHost;

    let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
    let projection_renderer = crate::ui::on_text_area_render(move |render| {
        for range in [69..81, 91..102] {
            let slice: String = render
                .content()
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect();
            render.range(range, move |_node| {
                let slice = slice.clone();
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop(
                    "style",
                    crate::view::ElementStylePropSchema {
                        padding: Some(Padding::uniform(Length::px(0.0)).x(Length::px(20.0))),
                        font_size: Some(crate::style::FontSize::Px(24.0)),
                        ..Default::default()
                    },
                )
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text(slice)),
                )
            });
        }
    });
    let tree = |wrap: bool| {
        let renderer = projection_renderer.clone();
        rsx! {
            <HostElement style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                padding: Padding::uniform(Length::px(8.0)),
                }}>
                <HostElement style={{
                    width: Length::percent(100.0),
                    padding: Padding::uniform(Length::px(8.0)),
                }}>
                    <HostTextArea
                        content={content}
                        multiline={true}
                        auto_wrap={wrap}
                        font_size={14}
                        on_render={renderer}
                    />
                </HostElement>
            </HostElement>
        }
    };

    let mut viewport = Viewport::new();
    // The reported screenshot is @2x; its logical viewport is 557x345.
    viewport.set_size(557, 345);
    viewport
        .render_rsx(&tree(false))
        .expect("initial nowrap render");
    run_layout_for_test(&mut viewport, 557.0, 345.0);

    viewport.render_rsx(&tree(true)).expect("wrapped rerender");
    run_layout_for_test(&mut viewport, 557.0, 345.0);

    fn find_text_area(
        arena: &crate::view::node_arena::NodeArena,
        key: crate::view::node_arena::NodeKey,
    ) -> Option<crate::view::node_arena::NodeKey> {
        let node = arena.get(key)?;
        if node.element.as_any().is::<TextAreaHost>() {
            return Some(key);
        }
        let children = node.children.clone();
        drop(node);
        children
            .into_iter()
            .find_map(|child| find_text_area(arena, child))
    }

    let root = viewport.scene.ui_root_keys[0];
    let text_area_key = find_text_area(&viewport.scene.node_arena, root).expect("TextArea");
    viewport
        .scene
        .node_arena
        .with_element_taken_ref(text_area_key, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextAreaHost>().unwrap();
            let package = text_area
                .unified_inline_ifc_render_package(arena)
                .expect("wrapped unified package");
            let tail_key = text_area
                .children
                .iter()
                .copied()
                .find(|key| {
                    arena.get(*key).is_some_and(|node| {
                        node.element
                            .as_any()
                            .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                            .is_some_and(|run| run.text == "Tail line")
                    })
                })
                .expect("Tail line run");
            let tail = arena.get(tail_key).unwrap().element.box_model_snapshot();
            let viewport_bottom =
                text_area.layout_state.layout_position.y + text_area.viewport_size.height;
            assert!(package.content_size().height > 35.0, "fixture must wrap");
            assert!(
                tail.y + tail.height <= viewport_bottom + 0.5,
                "Tail line must remain inside the TextArea scissor after nowrap -> wrap: tail={tail:?} viewport_bottom={viewport_bottom}",
            );
        });
}
