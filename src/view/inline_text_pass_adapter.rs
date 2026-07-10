use crate::view::inline_formatting_context::{
    InlineIfcTextPassGlyphInput, InlineIfcTextPassPaintInput,
};
use crate::view::render_pass::text_pass::{
    TextPassGlyphPaintInput, TextPassPreparedStagingGlyphInput, TextPassPreparedStagingInput,
    TextPassRasterGlyphInput,
};
#[cfg(test)]
use crate::view::render_pass::text_pass::{TextRasterKey, text_raster_key_for_raster_input};

#[cfg(test)]
use crate::view::render_pass::text_pass::{CachedRasterImage, rasterize_text_pass_glyph_input};
#[cfg(test)]
use rustc_hash::FxHashMap;
#[cfg(test)]
use swash::scale::ScaleContext as SwashScaleContext;

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassBridgeInput {
    pub(crate) glyphs: Vec<InlineTextPassBridgeGlyph>,
}

#[cfg(test)]
impl InlineTextPassBridgeInput {
    pub(crate) fn from_ifc_paint_input(
        input: &InlineIfcTextPassPaintInput,
        opacity: f32,
        fragment_index: u32,
    ) -> Self {
        Self {
            glyphs: input
                .glyphs
                .iter()
                .map(|glyph| {
                    InlineTextPassBridgeGlyph::from_ifc_glyph(glyph, opacity, fragment_index)
                })
                .collect(),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassBridgeGlyph {
    pub(crate) raster: TextPassRasterGlyphInput,
    pub(crate) paint: TextPassGlyphPaintInput,
}

#[cfg(test)]
impl InlineTextPassBridgeGlyph {
    pub(crate) fn from_ifc_glyph(
        glyph: &InlineIfcTextPassGlyphInput,
        opacity: f32,
        fragment_index: u32,
    ) -> Self {
        Self {
            raster: inline_ifc_glyph_to_text_pass_raster_input(glyph),
            paint: inline_ifc_glyph_to_text_pass_paint_input(glyph, opacity, fragment_index),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineTextPassBridgeBatchKey {
    pub(crate) color_bits: [u32; 4],
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) font_size_bits: u32,
    pub(crate) normalized_coords_hash: u64,
}

#[cfg(test)]
impl InlineTextPassBridgeBatchKey {
    fn from_glyph(glyph: &InlineTextPassBridgeGlyph) -> Self {
        Self {
            color_bits: glyph.paint.color.map(f32::to_bits),
            font_data_id: glyph.raster.font_data_id,
            font_index: glyph.raster.font_index,
            font_size_bits: glyph.raster.font_size.to_bits(),
            normalized_coords_hash: glyph.raster.normalized_coords_hash,
        }
    }

    #[cfg(test)]
    fn font_size(self) -> f32 {
        f32::from_bits(self.font_size_bits)
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassBridgeBatch {
    pub(crate) key: InlineTextPassBridgeBatchKey,
    pub(crate) glyph_indices: Vec<usize>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassBridgePackage {
    pub(crate) glyphs: Vec<InlineTextPassBridgeGlyph>,
    pub(crate) batches: Vec<InlineTextPassBridgeBatch>,
}

#[cfg(test)]
impl InlineTextPassBridgePackage {
    #[cfg(test)]
    pub(crate) fn from_ifc_paint_input(
        input: &InlineIfcTextPassPaintInput,
        opacity: f32,
        fragment_index: u32,
    ) -> Self {
        let bridge =
            InlineTextPassBridgeInput::from_ifc_paint_input(input, opacity, fragment_index);
        Self::from_bridge_input(bridge)
    }

    pub(crate) fn from_bridge_input(input: InlineTextPassBridgeInput) -> Self {
        let mut batches = Vec::<InlineTextPassBridgeBatch>::new();
        for (glyph_index, glyph) in input.glyphs.iter().enumerate() {
            let key = InlineTextPassBridgeBatchKey::from_glyph(glyph);
            if let Some(batch) = batches.last_mut() {
                if batch.key == key {
                    batch.glyph_indices.push(glyph_index);
                    continue;
                }
            }

            batches.push(InlineTextPassBridgeBatch {
                key,
                glyph_indices: vec![glyph_index],
            });
        }

        Self {
            glyphs: input.glyphs,
            batches,
        }
    }
}

pub(crate) fn inline_ifc_paint_input_to_text_pass_staging_input(
    input: &InlineIfcTextPassPaintInput,
    origin: [f32; 2],
    opacity: f32,
    fragment_index: u32,
    scale_factor: f32,
) -> TextPassPreparedStagingInput {
    inline_ifc_paint_input_to_text_pass_staging_input_with_color(
        input,
        origin,
        opacity,
        fragment_index,
        scale_factor,
        None,
    )
}

/// Like [`inline_ifc_paint_input_to_text_pass_staging_input`], overriding
/// every glyph's paint color. Standalone Text keeps its brush out of the
/// shaping cache key and injects the live color here instead.
pub(crate) fn inline_ifc_paint_input_to_text_pass_staging_input_with_color(
    input: &InlineIfcTextPassPaintInput,
    origin: [f32; 2],
    opacity: f32,
    fragment_index: u32,
    scale_factor: f32,
    color_override: Option<[f32; 4]>,
) -> TextPassPreparedStagingInput {
    TextPassPreparedStagingInput {
        scale_factor,
        glyphs: input
            .glyphs
            .iter()
            .map(|glyph| {
                let raster = inline_ifc_glyph_to_text_pass_raster_input(glyph);
                let mut paint =
                    inline_ifc_glyph_to_text_pass_paint_input(glyph, opacity, fragment_index);
                if let Some(color) = color_override {
                    paint.color = color;
                }
                TextPassPreparedStagingGlyphInput {
                    raster,
                    paint,
                    final_paint_pos: [
                        origin[0] + paint.local_pos[0],
                        origin[1] + paint.local_pos[1],
                    ],
                }
            })
            .collect(),
    }
}

#[cfg(test)]
pub(crate) fn build_inline_text_pass_bridge_package_for_test(
    input: &InlineIfcTextPassPaintInput,
    opacity: f32,
    fragment_index: u32,
) -> InlineTextPassBridgePackage {
    InlineTextPassBridgePackage::from_ifc_paint_input(input, opacity, fragment_index)
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassPrepareComparablePackage {
    pub(crate) scale_factor: f32,
    pub(crate) batches: Vec<InlineTextPassPrepareComparableBatch>,
    pub(crate) glyphs: Vec<InlineTextPassPrepareComparableGlyph>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassPrepareComparableBatch {
    pub(crate) key: InlineTextPassBridgeBatchKey,
    pub(crate) glyph_indices: Vec<usize>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassPrepareComparableGlyph {
    pub(crate) glyph_index: usize,
    pub(crate) batch_index: Option<usize>,
    pub(crate) raster_key: Option<TextRasterKey>,
    pub(crate) paint: TextPassGlyphPaintInput,
    pub(crate) raster: TextPassRasterGlyphInput,
}

#[cfg(test)]
pub(crate) fn inline_text_pass_prepare_comparable_package_for_test(
    package: &InlineTextPassBridgePackage,
    scale_factor: f32,
) -> InlineTextPassPrepareComparablePackage {
    let mut batch_index_for_glyph = vec![None; package.glyphs.len()];
    for (batch_index, batch) in package.batches.iter().enumerate() {
        for &glyph_index in &batch.glyph_indices {
            if let Some(slot) = batch_index_for_glyph.get_mut(glyph_index) {
                *slot = Some(batch_index);
            }
        }
    }

    InlineTextPassPrepareComparablePackage {
        scale_factor,
        batches: package
            .batches
            .iter()
            .map(|batch| InlineTextPassPrepareComparableBatch {
                key: batch.key,
                glyph_indices: batch.glyph_indices.clone(),
            })
            .collect(),
        glyphs: package
            .glyphs
            .iter()
            .enumerate()
            .map(
                |(glyph_index, glyph)| InlineTextPassPrepareComparableGlyph {
                    glyph_index,
                    batch_index: batch_index_for_glyph[glyph_index],
                    raster_key: text_raster_key_for_raster_input(&glyph.raster, scale_factor),
                    paint: glyph.paint,
                    raster: glyph.raster.clone(),
                },
            )
            .collect(),
    }
}

pub(crate) fn inline_ifc_glyph_to_text_pass_raster_input(
    glyph: &InlineIfcTextPassGlyphInput,
) -> TextPassRasterGlyphInput {
    TextPassRasterGlyphInput {
        glyph_id: glyph.glyph_id,
        font_size: glyph.font_size,
        font_data: glyph.font_data.clone(),
        font_data_id: glyph.font_data_id,
        font_index: glyph.font_index,
        normalized_coords_hash: glyph.normalized_coords_hash,
    }
}

pub(crate) fn inline_ifc_glyph_to_text_pass_paint_input(
    glyph: &InlineIfcTextPassGlyphInput,
    opacity: f32,
    fragment_index: u32,
) -> TextPassGlyphPaintInput {
    TextPassGlyphPaintInput {
        local_pos: [glyph.x, glyph.baseline_y + glyph.glyph_y],
        color: glyph.color,
        opacity,
        fragment_index,
    }
}

#[cfg(test)]
pub(crate) fn rasterize_first_bridged_glyph_for_test(
    input: &InlineIfcTextPassPaintInput,
    opacity: f32,
    fragment_index: u32,
    scale_context: &mut SwashScaleContext,
    raster_cache: &mut FxHashMap<TextRasterKey, CachedRasterImage>,
) -> Option<(InlineTextPassBridgeGlyph, usize)> {
    let bridge = InlineTextPassBridgeInput::from_ifc_paint_input(input, opacity, fragment_index);
    let glyph = bridge
        .glyphs
        .into_iter()
        .find(|glyph| glyph.raster.font_data.is_some())?;
    let image =
        rasterize_text_pass_glyph_input(scale_context, raster_cache, 1, &glyph.raster, 1.0)?;
    Some((glyph, image.data.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::inline_formatting_context::{
        InlineFormattingContext, InlineIfcAtomicMeasureConstraints, InlineIfcInput, InlineIfcItem,
        InlineIfcMeasuredAtomicBox, InlineIfcSize, InlineIfcSourceId, InlineIfcStyle,
    };
    use crate::view::render_pass::text_pass::{
        CachedRasterImage, TextRasterKey, rasterize_text_pass_glyph_input,
        text_raster_key_for_raster_input,
    };
    use rustc_hash::FxHashMap;
    use swash::scale::ScaleContext as SwashScaleContext;

    fn first_ifc_glyph() -> InlineIfcTextPassGlyphInput {
        let input = InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(7),
            text: "Bridge".to_string(),
            style: Some(InlineIfcStyle {
                brush: [64, 128, 191, 255],
                font_size: 18.0,
                font_weight: 600,
                line_height: 22.0,
                font_families: vec!["sans-serif".to_string()].into(),
                vertical_align: crate::style::VerticalAlign::Baseline,
            }),
        }]);
        InlineFormattingContext::build(input)
            .text_pass_paint_input()
            .glyphs
            .into_iter()
            .find(|glyph| glyph.font_data.is_some())
            .expect("test IFC should produce a renderable glyph")
    }

    fn measured_box(width: f32, height: f32) -> InlineIfcMeasuredAtomicBox {
        InlineIfcMeasuredAtomicBox::new(
            InlineIfcSize::new(width, height),
            InlineIfcAtomicMeasureConstraints::new(Some(width)),
        )
    }

    fn parity_style() -> InlineIfcStyle {
        InlineIfcStyle {
            brush: [24, 96, 192, 255],
            font_size: 21.0,
            font_weight: 500,
            line_height: 26.0,
            font_families: vec!["sans-serif".to_string()].into(),
            vertical_align: crate::style::VerticalAlign::Baseline,
        }
    }

    fn first_ifc_bridge_glyph_for_text(
        content: &str,
        style: &InlineIfcStyle,
    ) -> InlineTextPassBridgeGlyph {
        let input = InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(31),
            text: content.to_string(),
            style: Some(style.clone()),
        }]);
        let paint_input = InlineFormattingContext::build(input).text_pass_paint_input();
        InlineTextPassBridgeInput::from_ifc_paint_input(&paint_input, 0.5, 7)
            .glyphs
            .into_iter()
            .find(|glyph| glyph.raster.font_data.is_some())
            .expect("IFC bridge fixture should produce a renderable glyph")
    }

    fn paint_input_for_items(items: Vec<InlineIfcItem>) -> InlineIfcTextPassPaintInput {
        InlineFormattingContext::build(InlineIfcInput::new(items)).text_pass_paint_input()
    }

    #[test]
    fn bridge_preserves_raster_key_fields_from_ifc_glyph() {
        let glyph = first_ifc_glyph();
        let raster = inline_ifc_glyph_to_text_pass_raster_input(&glyph);

        assert_eq!(raster.glyph_id, glyph.glyph_id);
        assert_eq!(raster.font_size, glyph.font_size);
        assert_eq!(raster.font_data_id, glyph.font_data_id);
        assert_eq!(raster.font_index, glyph.font_index);
        assert_eq!(raster.normalized_coords_hash, glyph.normalized_coords_hash);
        assert_eq!(
            raster
                .font_data
                .as_ref()
                .map(|font| (font.data.id(), font.index)),
            glyph
                .font_data
                .as_ref()
                .map(|font| (font.data.id(), font.index))
        );
    }

    #[test]
    fn bridge_keeps_paint_fields_out_of_raster_input() {
        let glyph = first_ifc_glyph();
        let raster_before = inline_ifc_glyph_to_text_pass_raster_input(&glyph);
        let paint = inline_ifc_glyph_to_text_pass_paint_input(&glyph, 0.625, 11);
        let raster_after = inline_ifc_glyph_to_text_pass_raster_input(&glyph);

        assert_eq!(paint.local_pos, [glyph.x, glyph.baseline_y + glyph.glyph_y]);
        assert_eq!(paint.color, glyph.color);
        assert_eq!(paint.opacity, 0.625);
        assert_eq!(paint.fragment_index, 11);
        assert_eq!(raster_before, raster_after);
    }

    #[test]
    fn bridge_payload_converts_all_ifc_glyphs() {
        let input = InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(12),
            text: "abc".to_string(),
            style: None,
        }]);
        let adapter = InlineFormattingContext::build(input).text_pass_paint_input();
        let bridge = InlineTextPassBridgeInput::from_ifc_paint_input(&adapter, 0.5, 3);

        assert_eq!(bridge.glyphs.len(), adapter.glyphs.len());
        for (bridged, glyph) in bridge.glyphs.iter().zip(adapter.glyphs.iter()) {
            assert_eq!(bridged.raster.glyph_id, glyph.glyph_id);
            assert_eq!(bridged.raster.font_data_id, glyph.font_data_id);
            assert_eq!(bridged.paint.color, glyph.color);
            assert_eq!(bridged.paint.opacity, 0.5);
            assert_eq!(bridged.paint.fragment_index, 3);
        }
    }

    #[test]
    fn bridged_raster_input_uses_existing_text_pass_raster_helper() {
        let glyph = first_ifc_glyph();
        let raster = inline_ifc_glyph_to_text_pass_raster_input(&glyph);
        let mut scale_context = SwashScaleContext::new();
        let mut raster_cache = FxHashMap::<TextRasterKey, CachedRasterImage>::default();

        let image = rasterize_text_pass_glyph_input(
            &mut scale_context,
            &mut raster_cache,
            101,
            &raster,
            1.0,
        )
        .expect("bridged IFC glyph should rasterize through the existing TextPass helper");

        assert!(!image.data.is_empty());
        assert_eq!(raster_cache.len(), 1);
    }

    #[test]
    fn bridge_paint_payload_changes_do_not_change_raster_key() {
        let content = "Paint";
        let style = parity_style();
        let first = first_ifc_bridge_glyph_for_text(content, &style);

        let paint_input =
            InlineFormattingContext::build(InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
                source: InlineIfcSourceId(41),
                text: content.to_string(),
                style: Some(style),
            }]))
            .text_pass_paint_input();
        let low_opacity = InlineTextPassBridgeInput::from_ifc_paint_input(&paint_input, 0.2, 1)
            .glyphs
            .into_iter()
            .find(|glyph| glyph.raster.font_data.is_some())
            .expect("fixture should produce a renderable low-opacity glyph");
        let high_opacity = InlineTextPassBridgeInput::from_ifc_paint_input(&paint_input, 0.9, 99)
            .glyphs
            .into_iter()
            .find(|glyph| glyph.raster.font_data.is_some())
            .expect("fixture should produce a renderable high-opacity glyph");
        let scale_factor = 1.0;

        assert_eq!(low_opacity.raster, high_opacity.raster);
        assert_eq!(
            text_raster_key_for_raster_input(&low_opacity.raster, scale_factor),
            text_raster_key_for_raster_input(&high_opacity.raster, scale_factor)
        );
        assert_eq!(
            text_raster_key_for_raster_input(&first.raster, scale_factor),
            text_raster_key_for_raster_input(&low_opacity.raster, scale_factor)
        );
        assert_ne!(low_opacity.paint.opacity, high_opacity.paint.opacity);
        assert_ne!(
            low_opacity.paint.fragment_index,
            high_opacity.paint.fragment_index
        );
        assert_eq!(low_opacity.paint.color, high_opacity.paint.color);
        assert_eq!(low_opacity.paint.local_pos, high_opacity.paint.local_pos);
    }

    #[test]
    fn ifc_snapshot_bridge_smoke_rasterizes_first_glyph_without_inline_boxes() {
        const TEXT_SOURCE: InlineIfcSourceId = InlineIfcSourceId(21);
        const BOX_SOURCE: InlineIfcSourceId = InlineIfcSourceId(22);

        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "A".to_string(),
                style: Some(InlineIfcStyle {
                    brush: [10, 120, 220, 255],
                    font_size: 19.0,
                    font_weight: 500,
                    line_height: 24.0,
                    font_families: vec!["sans-serif".to_string()].into(),
                    vertical_align: crate::style::VerticalAlign::Baseline,
                }),
            },
            InlineIfcItem::AtomicInlineBox {
                source: BOX_SOURCE,
                measurement: measured_box(28.0, 14.0),
            },
            InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "Z".to_string(),
                style: None,
            },
        ]);
        let ifc = InlineFormattingContext::build(input);
        let snapshot = ifc.text_layout_snapshot();
        let paint_input = snapshot.text_pass_paint_input();
        let bridge = InlineTextPassBridgeInput::from_ifc_paint_input(&paint_input, 0.75, 9);

        assert_eq!(snapshot.inline_boxes.len(), 1);
        assert_eq!(snapshot.inline_boxes[0].source, BOX_SOURCE);
        assert!(
            paint_input
                .glyphs
                .iter()
                .all(|glyph| glyph.source != BOX_SOURCE)
        );
        assert!(bridge.glyphs.iter().all(|glyph| {
            glyph.raster.font_data.is_some()
                && glyph.raster.font_data_id != 0
                && glyph.raster.font_size > 0.0
                && glyph.raster.glyph_id != 0
        }));

        let first_ifc_glyph = paint_input
            .glyphs
            .iter()
            .find(|glyph| glyph.font_data.is_some())
            .expect("fixture should produce a renderable IFC glyph");
        let first_bridged = bridge
            .glyphs
            .iter()
            .find(|glyph| glyph.raster.font_data.is_some())
            .expect("bridge should keep the renderable IFC glyph");

        assert_eq!(first_bridged.raster.glyph_id, first_ifc_glyph.glyph_id);
        assert_eq!(first_bridged.raster.font_size, first_ifc_glyph.font_size);
        assert_eq!(
            first_bridged.raster.font_data_id,
            first_ifc_glyph.font_data_id
        );
        assert_eq!(first_bridged.raster.font_index, first_ifc_glyph.font_index);
        assert_eq!(
            first_bridged.raster.normalized_coords_hash,
            first_ifc_glyph.normalized_coords_hash
        );
        assert_eq!(first_bridged.paint.color, first_ifc_glyph.color);
        assert_eq!(
            first_bridged.paint.local_pos,
            [
                first_ifc_glyph.x,
                first_ifc_glyph.baseline_y + first_ifc_glyph.glyph_y
            ]
        );
        assert_eq!(first_bridged.paint.opacity, 0.75);
        assert_eq!(first_bridged.paint.fragment_index, 9);

        let mut scale_context = SwashScaleContext::new();
        let mut raster_cache = FxHashMap::<TextRasterKey, CachedRasterImage>::default();
        let (rasterized, raster_data_len) = rasterize_first_bridged_glyph_for_test(
            &paint_input,
            0.75,
            9,
            &mut scale_context,
            &mut raster_cache,
        )
        .expect("bridged IFC glyph should rasterize through TextPass helper");

        assert!(raster_data_len > 0);
        assert_eq!(rasterized.raster.glyph_id, first_ifc_glyph.glyph_id);
        assert_eq!(rasterized.paint.fragment_index, 9);
        assert_eq!(raster_cache.len(), 1);
    }

    #[test]
    fn bridge_package_batches_consecutive_same_style_glyphs_together() {
        let style = InlineIfcStyle {
            brush: [40, 120, 200, 255],
            ..parity_style()
        };
        let paint_input = paint_input_for_items(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(51),
            text: "Same".to_string(),
            style: Some(style),
        }]);
        let package = build_inline_text_pass_bridge_package_for_test(&paint_input, 0.8, 12);

        assert_eq!(package.glyphs.len(), paint_input.glyphs.len());
        assert_eq!(package.batches.len(), 1);
        assert_eq!(
            package.batches[0].glyph_indices,
            (0..package.glyphs.len()).collect::<Vec<_>>()
        );

        let first = &package.glyphs[0];
        let key = package.batches[0].key;
        assert_eq!(key.color_bits, first.paint.color.map(f32::to_bits));
        assert_eq!(key.font_data_id, first.raster.font_data_id);
        assert_eq!(key.font_index, first.raster.font_index);
        assert_eq!(key.font_size(), first.raster.font_size);
        assert_eq!(
            key.normalized_coords_hash,
            first.raster.normalized_coords_hash
        );
    }

    #[test]
    fn bridge_package_splits_batches_for_different_color() {
        let mut blue = parity_style();
        blue.brush = [24, 96, 192, 255];
        let mut red = parity_style();
        red.brush = [220, 60, 40, 255];
        let paint_input = paint_input_for_items(vec![
            InlineIfcItem::TextSpan {
                source: InlineIfcSourceId(61),
                text: "A".to_string(),
                style: Some(blue),
            },
            InlineIfcItem::TextSpan {
                source: InlineIfcSourceId(62),
                text: "B".to_string(),
                style: Some(red),
            },
        ]);
        let package = build_inline_text_pass_bridge_package_for_test(&paint_input, 1.0, 4);

        assert_eq!(package.glyphs.len(), 2);
        assert_eq!(package.batches.len(), 2);
        assert_ne!(
            package.batches[0].key.color_bits,
            package.batches[1].key.color_bits
        );
        assert_eq!(package.batches[0].glyph_indices, vec![0]);
        assert_eq!(package.batches[1].glyph_indices, vec![1]);
    }

    #[test]
    fn bridge_package_splits_batches_for_different_font_render_identity_or_hash() {
        let paint_input = paint_input_for_items(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(71),
            text: "AA".to_string(),
            style: Some(parity_style()),
        }]);
        let mut bridge = InlineTextPassBridgeInput::from_ifc_paint_input(&paint_input, 1.0, 5);
        assert!(
            bridge.glyphs.len() >= 2,
            "fixture should produce at least two glyphs"
        );

        bridge.glyphs[1].raster.font_data_id += 1;
        bridge.glyphs[1].raster.normalized_coords_hash = bridge.glyphs[1]
            .raster
            .normalized_coords_hash
            .wrapping_add(1);
        let package = InlineTextPassBridgePackage::from_bridge_input(bridge);

        assert_eq!(package.batches.len(), 2);
        assert_ne!(
            package.batches[0].key.font_data_id,
            package.batches[1].key.font_data_id
        );
        assert_ne!(
            package.batches[0].key.normalized_coords_hash,
            package.batches[1].key.normalized_coords_hash
        );
    }

    #[test]
    fn bridge_package_does_not_include_atomic_inline_box_glyphs() {
        const TEXT_SOURCE: InlineIfcSourceId = InlineIfcSourceId(81);
        const BOX_SOURCE: InlineIfcSourceId = InlineIfcSourceId(82);

        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "L".to_string(),
                style: Some(parity_style()),
            },
            InlineIfcItem::AtomicInlineBox {
                source: BOX_SOURCE,
                measurement: measured_box(20.0, 10.0),
            },
            InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "R".to_string(),
                style: Some(parity_style()),
            },
        ]);
        let ifc = InlineFormattingContext::build(input);
        let snapshot = ifc.text_layout_snapshot();
        let paint_input = snapshot.text_pass_paint_input();
        let package = build_inline_text_pass_bridge_package_for_test(&paint_input, 1.0, 6);

        assert_eq!(snapshot.inline_boxes.len(), 1);
        assert_eq!(snapshot.inline_boxes[0].source, BOX_SOURCE);
        assert!(
            paint_input
                .glyphs
                .iter()
                .all(|glyph| glyph.source != BOX_SOURCE)
        );
        assert_eq!(package.glyphs.len(), paint_input.glyphs.len());
        assert_eq!(
            package
                .batches
                .iter()
                .map(|batch| batch.glyph_indices.len())
                .sum::<usize>(),
            package.glyphs.len()
        );
    }

    #[test]
    fn bridge_package_glyphs_rasterize_through_existing_path_and_share_cache() {
        let paint_input = paint_input_for_items(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(91),
            text: "AAA".to_string(),
            style: Some(parity_style()),
        }]);
        let package = build_inline_text_pass_bridge_package_for_test(&paint_input, 1.0, 8);
        let mut scale_context = SwashScaleContext::new();
        let mut raster_cache = FxHashMap::<TextRasterKey, CachedRasterImage>::default();
        let scale_factor = 1.0;
        let mut expected_keys = Vec::new();

        for glyph in &package.glyphs {
            let key = text_raster_key_for_raster_input(&glyph.raster, scale_factor)
                .expect("packaged glyph should produce a raster key");
            expected_keys.push(key);
            let image = rasterize_text_pass_glyph_input(
                &mut scale_context,
                &mut raster_cache,
                301,
                &glyph.raster,
                scale_factor,
            )
            .expect("packaged glyph should rasterize through the existing TextPass path");
            assert!(!image.data.is_empty());
        }

        expected_keys.sort_by_key(|key| format!("{key:?}"));
        expected_keys.dedup();
        assert_eq!(expected_keys.len(), 1);
        assert_eq!(raster_cache.len(), expected_keys.len());
    }

    #[test]
    fn bridge_package_prepare_comparable_preserves_batches_and_glyph_indices() {
        let mut blue = parity_style();
        blue.brush = [24, 96, 192, 255];
        let mut red = parity_style();
        red.brush = [220, 60, 40, 255];
        let paint_input = paint_input_for_items(vec![
            InlineIfcItem::TextSpan {
                source: InlineIfcSourceId(101),
                text: "A".to_string(),
                style: Some(blue),
            },
            InlineIfcItem::AtomicInlineBox {
                source: InlineIfcSourceId(102),
                measurement: measured_box(24.0, 12.0),
            },
            InlineIfcItem::TextSpan {
                source: InlineIfcSourceId(103),
                text: "B".to_string(),
                style: Some(red),
            },
        ]);
        let package = build_inline_text_pass_bridge_package_for_test(&paint_input, 0.625, 14);
        let comparable = inline_text_pass_prepare_comparable_package_for_test(&package, 1.5);

        assert_eq!(comparable.scale_factor, 1.5);
        assert_eq!(comparable.batches.len(), package.batches.len());
        assert_eq!(comparable.glyphs.len(), package.glyphs.len());

        for (batch_index, (expected, comparable_batch)) in package
            .batches
            .iter()
            .zip(comparable.batches.iter())
            .enumerate()
        {
            assert_eq!(comparable_batch.key, expected.key);
            assert_eq!(comparable_batch.glyph_indices, expected.glyph_indices);
            for glyph_index in &comparable_batch.glyph_indices {
                assert_eq!(
                    comparable.glyphs[*glyph_index].batch_index,
                    Some(batch_index)
                );
            }
        }

        for comparable_glyph in &comparable.glyphs {
            let package_glyph = &package.glyphs[comparable_glyph.glyph_index];
            assert_eq!(comparable_glyph.paint, package_glyph.paint);
            assert_eq!(comparable_glyph.raster, package_glyph.raster);
            assert_eq!(
                comparable_glyph.raster_key,
                text_raster_key_for_raster_input(&package_glyph.raster, 1.5)
            );
            assert_eq!(comparable_glyph.paint.opacity, 0.625);
            assert_eq!(comparable_glyph.paint.fragment_index, 14);
            assert!(comparable_glyph.raster_key.is_some());
        }
    }

    #[test]
    fn bridge_package_prepare_comparable_excludes_atomic_inline_box_glyphs() {
        const TEXT_SOURCE: InlineIfcSourceId = InlineIfcSourceId(111);
        const BOX_SOURCE: InlineIfcSourceId = InlineIfcSourceId(112);

        let input = InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "Left".to_string(),
                style: Some(parity_style()),
            },
            InlineIfcItem::AtomicInlineBox {
                source: BOX_SOURCE,
                measurement: measured_box(18.0, 9.0),
            },
            InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "Right".to_string(),
                style: Some(parity_style()),
            },
        ]);
        let ifc = InlineFormattingContext::build(input);
        let snapshot = ifc.text_layout_snapshot();
        let paint_input = snapshot.text_pass_paint_input();
        let package = build_inline_text_pass_bridge_package_for_test(&paint_input, 1.0, 2);
        let comparable = inline_text_pass_prepare_comparable_package_for_test(&package, 1.0);

        assert_eq!(snapshot.inline_boxes.len(), 1);
        assert_eq!(snapshot.inline_boxes[0].source, BOX_SOURCE);
        assert!(
            paint_input
                .glyphs
                .iter()
                .all(|glyph| glyph.source != BOX_SOURCE)
        );
        assert_eq!(comparable.glyphs.len(), paint_input.glyphs.len());
        assert_eq!(
            comparable
                .batches
                .iter()
                .map(|batch| batch.glyph_indices.len())
                .sum::<usize>(),
            comparable.glyphs.len()
        );
        assert!(
            comparable
                .glyphs
                .iter()
                .all(|glyph| glyph.batch_index.is_some() && glyph.raster_key.is_some())
        );
    }
}
