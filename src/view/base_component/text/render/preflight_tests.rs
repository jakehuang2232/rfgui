use super::*;
use crate::view::base_component::{LayoutConstraints, LayoutPlacement, Layoutable};
use crate::view::inline_formatting_context::InlineIfcTextPassPaintInput;
use crate::view::paint::PreparedTextOp;
use std::sync::Arc;

fn placed_text(content: &str, owned: bool) -> Text {
    let mut text = Text::new_with_id(123, 0.0, 0.0, 160.0, 80.0, content);
    let mut arena = NodeArena::new();
    text.measure(
        LayoutConstraints {
            max_width: 160.0,
            max_height: 80.0,
            viewport_width: 160.0,
            viewport_height: 80.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(80.0),
        },
        &mut arena,
    );
    text.place(
        LayoutPlacement {
            parent_x: 3.25,
            parent_y: 5.5,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 80.0,
            viewport_width: 160.0,
            viewport_height: 80.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(80.0),
        },
        &mut arena,
    );
    if owned {
        let input = text
            .shaped_context
            .as_ref()
            .unwrap()
            .text_pass_paint_input();
        text.install_inline_ifc_owned_geometry(
            Vec::new(),
            Arc::new(input),
            crate::ui::Rect {
                x: 7.25,
                y: 9.5,
                width: 130.0,
                height: 50.0,
            },
        );
    }
    text
}

#[test]
fn text_preflight_stream_matches_recording_and_legacy_adapter_without_building_ops() {
    for owned in [false, true] {
        let mut text = placed_text("預檢 hello e\u{301} שלום", owned);
        text.set_color(crate::style::Color::rgb(170, 40, 80));
        for (offset, opacity) in [([0.0, 0.0], 1.0), ([-1.25, 8.5], 0.25)] {
            let before = PreparedTextOp::construction_count_for_test();
            for _ in 0..3 {
                assert_eq!(text.validate_shadow_text_payload(offset, opacity), Ok(()));
            }
            assert_eq!(PreparedTextOp::construction_count_for_test(), before);
            let payload = text.prepared_shadow_text_payload(offset, opacity).unwrap();
            let op = payload.op.as_ref().unwrap();
            assert_eq!(PreparedTextOp::construction_count_for_test(), before + 1);
            let (input, bounds, color_override) = if owned {
                (
                    text.inline_ifc_owned_paint_input().unwrap(),
                    text.inline_ifc_owned_paint_bounds().unwrap(),
                    None,
                )
            } else {
                let bounds = text.standalone_paint_bounds();
                (
                    text.shaped_context
                        .as_ref()
                        .unwrap()
                        .prepared_text_pass_paint_input_ref()
                        .unwrap(),
                    crate::ui::Rect {
                        x: bounds.x,
                        y: bounds.y,
                        width: bounds.width,
                        height: bounds.height,
                    },
                    Some(text.color.to_rgba_f32()),
                )
            };
            let origin = [bounds.x + offset[0], bounds.y + offset[1]];
            let expected = inline_ifc_paint_input_to_text_pass_staging_input_with_color(
                input,
                origin,
                opacity,
                0,
                1.0,
                color_override,
            );
            assert_eq!(op.params.staging_input, expected);
            assert_eq!(
                op.params.fragments,
                vec![TextPassPreparedFragment {
                    origin,
                    size: [bounds.width, bounds.height]
                }]
            );
            assert!(op.has_canonical_identity());
        }
    }
}

#[test]
fn text_preflight_stream_rejects_damaged_owned_payloads_and_coordinates() {
    type Damage = (&'static str, fn(&mut InlineIfcTextPassPaintInput));
    let damages: &[Damage] = &[
        ("missing glyphs", |input| input.glyphs.clear()),
        ("missing font", |input| input.glyphs[0].font_data = None),
        ("font id", |input| input.glyphs[0].font_data_id ^= 1),
        ("font index", |input| input.glyphs[0].font_index ^= 1),
        ("invalid size", |input| input.glyphs[0].font_size = f32::NAN),
        ("invalid x", |input| input.glyphs[0].x = f32::INFINITY),
        ("invalid color", |input| input.glyphs[0].color[0] = f32::NAN),
    ];
    for &(label, damage) in damages {
        let mut text = placed_text("payload", true);
        damage(Arc::make_mut(
            &mut text.inline_ifc_owned.as_mut().unwrap().paint_input,
        ));
        assert!(
            text.validate_shadow_text_payload([0.0, 0.0], 1.0).is_err(),
            "{label}"
        );
        assert!(
            text.prepared_shadow_text_payload([0.0, 0.0], 1.0).is_err(),
            "{label}"
        );
    }
    for owned in [false, true] {
        let text = placed_text("payload", owned);
        assert!(
            text.validate_shadow_text_payload([f32::NAN, 0.0], 1.0)
                .is_err()
        );
        assert!(
            text.prepared_shadow_text_payload([f32::NAN, 0.0], 1.0)
                .is_err()
        );
    }
}

#[test]
fn text_preflight_stream_preserves_empty_and_hidden_paint() {
    for owned in [false, true] {
        for content in ["", "   ", "visible"] {
            let text = placed_text(content, owned);
            for opacity in [0.0, 1.0, f32::NAN] {
                let full = text.prepared_shadow_text_payload([0.0, 0.0], opacity);
                let validation = text.validate_shadow_text_payload([0.0, 0.0], opacity);
                assert_eq!(validation.is_ok(), full.is_ok());
                if content.is_empty() || !opacity.is_finite() || opacity == 0.0 {
                    assert!(full.unwrap().op.is_none());
                }
            }
        }
    }
}

#[test]
fn text_paint_memo_reuses_only_identical_source_and_complete_paint_parameters() {
    for owned in [false, true] {
        let mut text = placed_text("memo hello", owned);
        let initial = text.prepared_shadow_text_payload([0.0, 0.0], 1.0).unwrap();
        let count = PreparedTextOp::construction_count_for_test();
        for _ in 0..3 {
            assert_eq!(text.validate_shadow_text_payload([0.0, 0.0], 1.0), Ok(()));
            assert!(Arc::ptr_eq(
                &initial,
                &text.prepared_shadow_text_payload([0.0, 0.0], 1.0).unwrap()
            ));
        }
        assert_eq!(PreparedTextOp::construction_count_for_test(), count);
        // Compare each miss to an independently rebuilt payload using the same
        // established conversion, not merely a cache hit/miss counter.
        for step in 0..6 {
            // Reset the cached offset/opacity before each isolated change.
            let baseline = text.prepared_shadow_text_payload([0.0, 0.0], 1.0).unwrap();
            let offset = if step == 0 { [2.5, -1.0] } else { [0.0, 0.0] };
            let opacity = if step == 1 { 0.25 } else { 1.0 };
            if step == 2 {
                text.set_color(crate::style::Color::rgb(40, 90, 170));
            }
            if step == 3 {
                if owned {
                    text.inline_ifc_owned.as_mut().unwrap().paint_bounds.width += 3.0;
                } else {
                    text.layout_state.layout_size.width =
                        text.standalone_paint_bounds().width + 3.0;
                }
            }
            if step == 4 {
                if owned {
                    text.inline_ifc_owned.as_mut().unwrap().paint_bounds.x += 4.0;
                } else {
                    text.layout_state.layout_position.x += 4.0;
                }
            }
            if step == 5 {
                if owned {
                    let input = &mut text.inline_ifc_owned.as_mut().unwrap().paint_input;
                    Arc::make_mut(input).glyphs[0].x += 1.0;
                } else {
                    text.shaped_context = placed_text("different content", false).shaped_context;
                }
            }
            let cached = text.prepared_shadow_text_payload(offset, opacity).unwrap();
            if !owned || step != 2 {
                assert!(
                    !Arc::ptr_eq(&baseline, &cached),
                    "input change must miss: owned={owned}, step={step}"
                );
            } else {
                // IFC-owned glyph color comes from its immutable input.
                assert!(Arc::ptr_eq(&baseline, &cached));
            }
            *text.paint_memo.borrow_mut() = None;
            let rebuilt = text.prepared_shadow_text_payload(offset, opacity).unwrap();
            assert_eq!(
                [
                    cached.bounds.x,
                    cached.bounds.y,
                    cached.bounds.width,
                    cached.bounds.height
                ],
                [
                    rebuilt.bounds.x,
                    rebuilt.bounds.y,
                    rebuilt.bounds.width,
                    rebuilt.bounds.height
                ],
                "owned={owned}, step={step}"
            );
            assert_eq!(
                cached.op.as_ref().unwrap().params,
                rebuilt.op.as_ref().unwrap().params,
                "owned={owned}, step={step}"
            );
            assert_eq!(
                cached.op.as_ref().unwrap().frozen_identity(),
                rebuilt.op.as_ref().unwrap().frozen_identity()
            );
        }
    }
}

#[test]
fn text_paint_memo_cannot_hide_invalid_or_missing_inputs_after_warming() {
    for damage in 0..4 {
        let mut text = placed_text("memo", true);
        let old = text.prepared_shadow_text_payload([0.0, 0.0], 1.0).unwrap();
        let input = Arc::make_mut(&mut text.inline_ifc_owned.as_mut().unwrap().paint_input);
        match damage {
            0 => input.glyphs[0].font_data_id ^= 1,
            1 => input.glyphs[0].x = f32::NAN,
            2 => input.glyphs[0].color[0] = f32::INFINITY,
            _ => input.glyphs.clear(),
        }
        assert!(text.validate_shadow_text_payload([0.0, 0.0], 1.0).is_err());
        assert!(text.prepared_shadow_text_payload([0.0, 0.0], 1.0).is_err());
        assert!(old.op.as_ref().unwrap().has_canonical_identity());
    }
    let mut text = placed_text("memo", false);
    text.prepared_shadow_text_payload([0.0, 0.0], 1.0).unwrap();
    text.set_text("changed");
    assert!(text.validate_shadow_text_payload([0.0, 0.0], 1.0).is_err());
    assert!(text.prepared_shadow_text_payload([0.0, 0.0], 1.0).is_err());
}

#[test]
fn text_install_memo_preserves_structural_equality_and_rechecks_copy_on_write() {
    use super::super::paint_cache::TextInstallMemo;
    let text = placed_text("memo", true);
    let mut installed = text.inline_ifc_owned.as_ref().unwrap().paint_input.clone();
    let expected = Arc::new(installed.as_ref().clone());
    let slot = Default::default();
    for _ in 0..3 {
        assert!(TextInstallMemo::matches(&slot, &installed, &expected));
    }
    Arc::make_mut(&mut installed).glyphs[0].x += 1.0;
    assert!(!TextInstallMemo::matches(&slot, &installed, &expected));
    Arc::make_mut(&mut installed).glyphs[0].x = f32::NAN;
    for _ in 0..3 {
        assert!(
            !TextInstallMemo::matches(&slot, &installed, &installed),
            "same Arc with NaN must remain unequal"
        );
    }
}
