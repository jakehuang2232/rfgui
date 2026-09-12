use super::*;
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcInput, InlineIfcItem, InlineIfcLayoutOptions,
    InlineIfcSourceId, InlineIfcStyle,
};
use crate::view::inline_text_pass_adapter::inline_ifc_glyph_to_text_pass_raster_input;

#[test]
fn snap_text_local_pos_snaps_absolute_pixel_position() {
    let fragment_origin = [10.25, 4.75];
    let local = snap_text_local_pos(fragment_origin, [3.90, 2.40]);

    assert_eq!(fragment_origin[0] + local[0], 14.0);
    assert_eq!(fragment_origin[1] + local[1], 7.0);
}

#[test]
fn text_render_trunc_moves_toward_zero() {
    assert_eq!(text_render_trunc(4.9), 4.0);
    assert_eq!(text_render_trunc(-4.9), -4.0);
}

#[test]
fn text_glyph_instance_layout_matches_shader_locations() {
    assert_eq!(std::mem::size_of::<TextGlyphInstance>(), 56);
    let attrs = text_glyph_vertex_layout();
    assert_eq!(attrs.array_stride, 56);
    assert_eq!(attrs.attributes.len(), 7);
    assert_eq!(attrs.attributes[0].offset, 0);
    assert_eq!(attrs.attributes[4].offset, 32);
    assert_eq!(attrs.attributes[6].offset, 52);
}

fn first_renderable_raster_input() -> TextPassRasterGlyphInput {
    let ifc = InlineFormattingContext::build_with_options(
        InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(1),
            text: "Raster key".to_string(),
            style: Some(InlineIfcStyle {
                font_size: 17.0,
                line_height: 1.2,
                font_weight: 500,
                brush: [0, 0, 0, 255],
                font_families: vec!["sans-serif".to_string()].into(),
                vertical_align: crate::style::VerticalAlign::Baseline,
            }),
        }]),
        InlineIfcLayoutOptions::new(Some(200.0), true),
    );
    let glyph = ifc
        .text_pass_paint_input()
        .glyphs
        .into_iter()
        .find(|glyph| glyph.font_data.is_some())
        .expect("test layout should produce a glyph with font data");
    inline_ifc_glyph_to_text_pass_raster_input(&glyph)
}

#[test]
fn raster_input_key_matches_existing_text_glyph_key_fields() {
    let input = first_renderable_raster_input();
    let scale_factor = 1.75;

    let input_key = text_raster_key_for_raster_input(&input, scale_factor)
        .expect("neutral input should produce a raster key");

    assert_eq!(input_key.glyph_id, input.glyph_id);
    assert_eq!(input_key.font_size_bits, input.font_size.to_bits());
    assert_eq!(input_key.font_blob_id, input.font_data_id);
    assert_eq!(input_key.font_index, input.font_index);
    assert_eq!(
        input_key.normalized_coords_hash,
        input.normalized_coords_hash
    );
    assert_eq!(input_key.scale_factor_bits, scale_factor.to_bits());
}

#[test]
fn raster_input_rejects_stale_font_handle_identity() {
    let mut input = first_renderable_raster_input();
    input.font_data_id = input.font_data_id.wrapping_add(1);

    assert!(text_raster_key_for_raster_input(&input, 1.0).is_none());
}

#[test]
fn raster_input_uses_existing_rasterize_path() {
    let input = first_renderable_raster_input();
    let scale_factor = 1.0;
    let key = text_raster_key_for_raster_input(&input, scale_factor)
        .expect("neutral input should produce a raster key");
    let mut scale_context = SwashScaleContext::new();
    let mut raster_cache = FxHashMap::default();

    let image = rasterize_text_pass_glyph_input(
        &mut scale_context,
        &mut raster_cache,
        42,
        &input,
        scale_factor,
    )
    .expect("neutral input should rasterize through the existing glyph path");

    assert!(!image.data.is_empty());
    assert!(raster_cache.contains_key(&key));
}

#[test]
fn paint_input_is_separate_from_raster_key_fields() {
    let input = first_renderable_raster_input();
    let first_paint = TextPassGlyphPaintInput {
        local_pos: [1.0, 2.0],
        color: [1.0, 0.0, 0.0, 1.0],
        opacity: 0.25,
        fragment_index: 3,
    };
    let second_paint = TextPassGlyphPaintInput {
        local_pos: [8.0, 13.0],
        color: [0.0, 0.0, 1.0, 1.0],
        opacity: 0.95,
        fragment_index: 7,
    };

    let before = text_raster_key_for_raster_input(&input, 2.0)
        .expect("neutral input should produce a raster key");
    let after = text_raster_key_for_raster_input(&input, 2.0)
        .expect("paint changes are not part of raster key input");

    assert_ne!(first_paint, second_paint);
    assert_eq!(before, after);
}

#[test]
fn prepared_staging_probe_uses_existing_raster_and_instance_metadata() {
    let raster = first_renderable_raster_input();
    let scale_factor = 1.5;
    let paint = TextPassGlyphPaintInput {
        local_pos: [2.25, 7.5],
        color: [0.2, 0.4, 0.6, 1.0],
        opacity: 0.5,
        fragment_index: 11,
    };
    let input = TextPassPreparedStagingInput {
        scale_factor,
        glyphs: vec![TextPassPreparedStagingGlyphInput {
            raster: raster.clone(),
            paint,
            final_paint_pos: [23.0 + paint.local_pos[0], 29.0 + paint.local_pos[1]],
        }],
    };

    let probe = build_text_pass_prepared_staging_probe(&input);

    assert_eq!(probe.scale_factor, scale_factor);
    assert_eq!(probe.glyphs.len(), 1);
    let staged = &probe.glyphs[0];
    assert_eq!(staged.glyph_index, 0);
    assert_eq!(
        staged.raster_key,
        text_raster_key_for_raster_input(&raster, scale_factor)
    );
    assert_eq!(staged.paint, paint);
    assert_eq!(staged.final_paint_pos, input.glyphs[0].final_paint_pos);
    assert!(staged.instance_size[0] >= 1.0);
    assert!(staged.instance_size[1] >= 1.0);
    assert!(matches!(
        staged.atlas_kind,
        TextPassPreparedStagingAtlasKind::Mask | TextPassPreparedStagingAtlasKind::Color
    ));
}
