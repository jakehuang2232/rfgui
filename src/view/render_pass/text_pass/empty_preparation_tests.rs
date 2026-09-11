use super::*;
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcInput, InlineIfcItem, InlineIfcLayoutOptions,
    InlineIfcSourceId, InlineIfcStyle,
};

#[test]
fn empty_text_raster_requires_valid_frozen_font_and_fragment_sources() {
    let ifc = InlineFormattingContext::build_with_options(
        InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(1),
            text: "   ".into(),
            style: Some(InlineIfcStyle {
                font_size: 17.0,
                line_height: 1.2,
                font_weight: 400,
                brush: [255, 0, 0, 255],
                font_families: vec!["sans-serif".into()].into(),
                vertical_align: crate::style::VerticalAlign::Baseline,
            }),
        }]),
        InlineIfcLayoutOptions::new(Some(200.0), true),
    );
    let staging_input =
        crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input(
            &ifc.text_pass_paint_input(),
            [0.0, 0.0],
            1.0,
            0,
            1.0,
        );
    assert!(
        !staging_input.glyphs.is_empty(),
        "spaces retain shaped glyphs"
    );
    let params = TextPassPreparedParams {
        staging_input,
        fragments: vec![TextPassPreparedFragment {
            origin: [0.0, 0.0],
            size: [200.0, 40.0],
        }],
        scissor_rect: None,
        stencil_clip_id: None,
    };
    assert!(prepared_text_raster_sources_are_valid(&params));
    let mut damaged = params.clone();
    damaged.staging_input.glyphs[0].raster.font_data = None;
    assert!(!prepared_text_raster_sources_are_valid(&damaged));
    damaged = params.clone();
    damaged.staging_input.glyphs[0].raster.font_data_id ^= 1;
    assert!(!prepared_text_raster_sources_are_valid(&damaged));
    damaged = params.clone();
    damaged.staging_input.glyphs[0].paint.fragment_index = 1;
    assert!(!prepared_text_raster_sources_are_valid(&damaged));
    damaged = params;
    damaged.staging_input.glyphs[0].raster.glyph_id = u32::MAX;
    assert!(!prepared_text_raster_sources_are_valid(&damaged));
}
