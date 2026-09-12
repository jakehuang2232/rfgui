use super::*;
use crate::style::{Border, Color, FontSize, Length, Padding, VerticalAlign};
use crate::view::base_component::text_area::inline_ifc::TextAreaUnifiedIfcSourceKind;
use crate::view::test_support::{commit_element, measure_and_place, new_test_arena};

const CONTENT: &str = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";

fn projected_area(
    align: VerticalAlign,
    width: f32,
    wrap: bool,
    preedit: bool,
) -> (crate::view::NodeArena, crate::view::NodeKey) {
    let mut area = TextArea::new();
    area.content = CONTENT.into();
    area.font_size = 14.0;
    area.line_height = 1.25;
    area.vertical_align = align;
    area.auto_wrap = wrap;
    if preedit {
        area.is_focused = true;
        area.cursor_char = CONTENT.find("enabled.").unwrap();
        area.ime_preedit = "compose".into();
        area.ime_preedit_cursor = Some((7, 7));
    }
    area.on_render_handler = Some(crate::ui::on_text_area_render(|render| {
        for token in ["{{API_HOST}}", "{{USER_ID}}"] {
            let start = CONTENT.find(token).unwrap();
            render.range(start..start + token.len(), move |_| {
                crate::ui::rsx! {
                    <crate::view::Element style={{
                        font_size: FontSize::Px(24.0),
                        padding: Padding::uniform(Length::px(0.0)).x(Length::px(20.0)),
                        border: Border::uniform(Length::px(1.0), &Color::hex("#42566f")),
                    }}>
                        <crate::view::Text>{token}</crate::view::Text>
                    </crate::view::Element>
                }
            });
        }
    }));
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(area));
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .set_self_node_key(root)
    });
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: width,
            max_height: 300.0,
            viewport_width: width,
            viewport_height: 300.0,
            percent_base_width: Some(width),
            percent_base_height: Some(300.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 300.0,
            viewport_width: width,
            viewport_height: 300.0,
            percent_base_width: Some(width),
            percent_base_height: Some(300.0),
        },
    );
    (arena, root)
}

#[test]
fn projected_text_aligns_once_within_each_visual_line() {
    for align in [
        VerticalAlign::Top,
        VerticalAlign::Middle,
        VerticalAlign::Bottom,
        VerticalAlign::Baseline,
    ] {
        for width in [342.0, 420.0] {
            for wrap in [true, false] {
                let (arena, root) = projected_area(align, width, wrap, false);
                let node = arena.get(root).unwrap();
                let area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
                let package = area.unified_inline_ifc_render_package(&arena).unwrap();
                let lines = package.visual_line_rects();
                let snapshot = package.ifc.text_layout_snapshot_ref();
                let top = snapshot.lines.iter().map(|l| l.y).fold(0.0_f32, f32::min);
                let context = format!("{align:?} width={width} wrap={wrap}");
                for segment in package
                    .source_segments
                    .iter()
                    .filter(|s| s.kind == TextAreaUnifiedIfcSourceKind::TextRun)
                {
                    let fragments = package.child_fragment_rects(segment.child_key);
                    let shaped = package.ifc.source_text_line_rects(segment.source);
                    assert_eq!(fragments.len(), shaped.len(), "{context}");
                    let selection =
                        package.selection_rects_for_char_range(segment.char_range.clone());
                    assert_eq!(selection.len(), fragments.len(), "{context}");
                    for (selection, fragment) in selection.iter().zip(&fragments) {
                        assert!(
                            (selection.y - fragment.y).abs() < 0.05
                                && (selection.height - fragment.height).abs() < 0.05,
                            "{context}: selection and painted text must share their vertical band"
                        );
                    }
                    for (fragment, (line_index, rect)) in fragments.iter().zip(shaped) {
                        let line = lines[line_index];
                        let expected_y = match align {
                            VerticalAlign::Top | VerticalAlign::Baseline => line.y,
                            VerticalAlign::Middle => line.y + (line.height - rect.height) / 2.0,
                            VerticalAlign::Bottom => line.y + line.height - rect.height,
                        };
                        assert!(
                            (fragment.y - expected_y).abs() < 0.05,
                            "{context} line={line_index}: text top {} must be {expected_y}; line={line:?}",
                            fragment.y
                        );
                        assert!(
                            fragment.y >= line.y - 0.05
                                && fragment.y + fragment.height <= line.y + line.height + 0.05,
                            "{context}: fragment escaped its visual line: {fragment:?}, {line:?}"
                        );
                    }
                }
                let staged = package.text_pass_staging_input([0.0, 0.0], 1.0, 0, 1.0);
                let painted = package.ifc.text_pass_paint_input_ref();
                for (glyph, input) in staged.glyphs.iter().zip(&painted.glyphs) {
                    assert!(
                        (glyph.paint.local_pos[1] - (input.baseline_y + input.glyph_y - top)).abs()
                            < 0.05,
                        "{context}: already-aligned glyph was displaced again"
                    );
                }
                let caret_lines = package.visual_caret_lines_ref();
                assert_eq!(caret_lines.len(), lines.len(), "{context}");
                for (caret, line) in caret_lines.iter().zip(&lines) {
                    assert!(
                        (caret.y_top - line.y).abs() < 0.05
                            && (caret.y_bottom - line.y - line.height).abs() < 0.05,
                        "{context}: caret band escaped its line: {caret:?}, {line:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn projected_preedit_underline_tracks_the_aligned_glyph_baseline() {
    for align in [
        VerticalAlign::Top,
        VerticalAlign::Middle,
        VerticalAlign::Bottom,
        VerticalAlign::Baseline,
    ] {
        let (arena, root) = projected_area(align, 342.0, true, true);
        let node = arena.get(root).unwrap();
        let area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        let package = area.unified_inline_ifc_render_package(&arena).unwrap();
        let source = package
            .source_segments
            .iter()
            .find(|s| s.preedit_backing_byte_range.is_some())
            .unwrap();
        let range = source.preedit_backing_byte_range.as_ref().unwrap();
        let paint = package.ifc.text_pass_paint_input_ref();
        let top = package
            .ifc
            .text_layout_snapshot_ref()
            .lines
            .iter()
            .map(|l| l.y)
            .fold(0.0_f32, f32::min);
        let underlines = package.preedit_underline_rects();
        assert!(!underlines.is_empty());
        for underline in underlines {
            assert!(
                paint
                    .glyphs
                    .iter()
                    .filter(
                        |g| g.cluster_range.start < range.end && range.start < g.cluster_range.end
                    )
                    .any(|g| {
                        let baseline = g.baseline_y + g.glyph_y - top;
                        underline.y >= baseline - 0.5 && underline.y <= baseline + g.font_size * 0.5
                    }),
                "{align:?}: preedit underline must stay below its aligned glyphs: {underline:?}"
            );
        }
        let caret = package
            .preedit_caret_geometry_for_char(area.cursor_char)
            .expect("preedit caret");
        assert!(
            package
                .visual_line_rects()
                .iter()
                .any(|line| (line.y - caret.y_top).abs() < 0.05
                    && (line.height - caret.height).abs() < 0.05),
            "{align:?}: preedit caret must stay within its visual line: {caret:?}"
        );
    }
}
