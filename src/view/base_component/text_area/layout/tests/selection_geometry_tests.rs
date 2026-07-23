use super::*;

#[test]
fn unified_selection_rects_align_with_painted_text_band() {
    use crate::view::base_component::ElementTrait;
    let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.auto_wrap = true;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        let ranges = [(69..81), (91..102)];
        for range in ranges {
            let slice: String = content
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect();
            render.range(range.clone(), move |_node| {
                let slice = slice.clone();
                crate::ui::RsxNode::tagged(
                    "Element",
                    crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop(
                    "style",
                    crate::view::ElementStylePropSchema {
                        font_size: Some(crate::style::FontSize::Px(24.0)),
                        ..Default::default()
                    },
                )
                .with_child(
                    crate::ui::RsxNode::tagged(
                        "Text",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(crate::ui::RsxNode::text(slice)),
                )
            });
        }
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
            max_width: 342.0,
            max_height: 176.0,
            viewport_width: 342.0,
            viewport_height: 176.0,
            percent_base_width: Some(342.0),
            percent_base_height: Some(176.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 342.0,
            available_height: 176.0,
            viewport_width: 342.0,
            viewport_height: 176.0,
            percent_base_width: Some(342.0),
            percent_base_height: Some(176.0),
        },
    );

    arena.with_element_taken_ref(root, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        let package = text_area
            .unified_inline_ifc_render_package(arena)
            .expect("unified package");

        // Selection band for the leading committed text must overlap
        // the painted text band of the run's first fragment.
        let needle = "First line";
        let rects =
            package.selection_rects_for_char_range(0..needle.chars().count());
        assert!(!rects.is_empty(), "selection rects for leading text");
        let selection = rects[0];
        drop(package);

        let mut first_fragment: Option<crate::ui::Rect> = None;
        for &child in &text_area.children {
            let Some(node) = arena.get(child) else {
                continue;
            };
            let Some(run) = node
                .element
                .as_any()
                .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
            else {
                continue;
            };
            if run.text.starts_with("First line") {
                first_fragment = run.inline_paint_fragments.first().copied();
            }
        }
        let fragment = first_fragment.expect("first run fragment");
        // Selection rects are in content coords; fragments are
        // absolute (origin 0,0 here, so directly comparable).
        let sel_top = selection.y;
        let sel_bottom = selection.y + selection.height;
        let frag_top = fragment.y;
        let frag_bottom = fragment.y + fragment.height;
        let overlap =
            sel_bottom.min(frag_bottom) - sel_top.max(frag_top);
        assert!(
            overlap >= fragment.height * 0.6,
            "selection band must cover the painted text band: selection=({sel_top}, {sel_bottom}) fragment=({frag_top}, {frag_bottom})"
        );
    });
}
