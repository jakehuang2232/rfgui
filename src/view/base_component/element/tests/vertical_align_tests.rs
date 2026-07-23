use super::*;

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
/// element bottom-aligns (line baseline - height).
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
/// element keeps top (baseline = height = line baseline), text drops
/// to align glyph baseline. (Specific px audit — text ascent is
/// font-dependent.)
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
/// through the computed style used by inline layout.
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
