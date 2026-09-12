use super::*;

#[test]
fn text_area_inline_ifc_selection_rects_follow_unified_text_render() {
    let content = concat!(
        "First line with a long value that can wrap when auto wrap is enabled.",
        "{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\n",
        "Tail line"
    );
    let api_host = char_range_of(content, "{{API_HOST}}");
    let user_id = char_range_of(content, "{{USER_ID}}");

    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.multiline = true;
    text_area.auto_wrap = true;
    text_area.vertical_align = crate::style::VerticalAlign::Bottom;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(api_host.clone(), |_text_area_node| {
            auto_projection_chip_node("{{API_HOST}}")
        });
        render.range(user_id.clone(), |_text_area_node| {
            auto_projection_chip_node("{{USER_ID}}")
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
            max_width: 360.0,
            max_height: 176.0,
            viewport_width: 360.0,
            viewport_height: 176.0,
            percent_base_width: Some(360.0),
            percent_base_height: Some(176.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 360.0,
            available_height: 176.0,
            viewport_width: 360.0,
            viewport_height: 176.0,
            percent_base_width: Some(360.0),
            percent_base_height: Some(176.0),
        },
    );

    let package = arena
        .with_element_taken_ref(root, |el, _| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .unified_inline_ifc_root_package(&arena)
        })
        .flatten()
        .expect("TextArea should expose a unified IFC root package");
    let first_run = package
        .source_segments
        .iter()
        .find(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::TextRun)
        .expect("first text source");
    let selected_range = first_run.char_range.start..first_run.char_range.start + 10;
    let selection_rect = package
        .selection_rects_for_char_range(selected_range)
        .into_iter()
        .next()
        .expect("unified selection rect");
    let first_glyph = package
        .text_pass_staging_input([0.0, 0.0], 1.0, 0, 1.0)
        .glyphs
        .into_iter()
        .find(|glyph| glyph.final_paint_pos[0] >= selection_rect.x - 0.5)
        .expect("selected staged glyph");

    assert!(
        first_glyph.final_paint_pos[0] >= selection_rect.x - 0.5
            && first_glyph.final_paint_pos[0] <= selection_rect.x + selection_rect.width + 0.5,
        "selection rect must track unified glyph render x, selection=({}, {}, {}, {}), glyph_x={}",
        selection_rect.x,
        selection_rect.y,
        selection_rect.width,
        selection_rect.height,
        first_glyph.final_paint_pos[0],
    );
    assert!(
        first_glyph.final_paint_pos[1] >= selection_rect.y - 0.5
            && first_glyph.final_paint_pos[1] <= selection_rect.y + selection_rect.height + 0.5,
        "selection rect must track unified glyph render y, selection=({}, {}, {}, {}), glyph_y={}",
        selection_rect.x,
        selection_rect.y,
        selection_rect.width,
        selection_rect.height,
        first_glyph.final_paint_pos[1],
    );
}
