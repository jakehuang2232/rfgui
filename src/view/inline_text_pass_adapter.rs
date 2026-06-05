use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcInput, InlineIfcItem, InlineIfcLayoutOptions,
    InlineIfcSourceId, InlineIfcStyle,
};
use crate::view::inline_formatting_context::{
    InlineIfcTextPassGlyphInput, InlineIfcTextPassPaintInput,
};
use crate::view::render_pass::text_pass::{
    TextPassGlyphPaintInput, TextPassPreparedStagingGlyphInput, TextPassPreparedStagingInput,
    TextPassRasterGlyphInput, TextRasterKey, text_raster_key_for_raster_input,
};

#[cfg(test)]
use crate::view::render_pass::text_pass::{CachedRasterImage, rasterize_text_pass_glyph_input};
#[cfg(test)]
use rustc_hash::FxHashMap;
#[cfg(test)]
use swash::scale::ScaleContext as SwashScaleContext;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassBridgeInput {
    pub(crate) glyphs: Vec<InlineTextPassBridgeGlyph>,
}

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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassBridgeGlyph {
    pub(crate) raster: TextPassRasterGlyphInput,
    pub(crate) paint: TextPassGlyphPaintInput,
}

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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InlineTextPassBridgeBatchKey {
    pub(crate) color_bits: [u32; 4],
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) font_size_bits: u32,
    pub(crate) normalized_coords_hash: u64,
}

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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassBridgeBatch {
    pub(crate) key: InlineTextPassBridgeBatchKey,
    pub(crate) glyph_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassBridgePackage {
    pub(crate) glyphs: Vec<InlineTextPassBridgeGlyph>,
    pub(crate) batches: Vec<InlineTextPassBridgeBatch>,
}

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

/// Staging input for the first Text read-only IFC bridge call-site.
///
/// `origin` is the fragment paint origin from the current Text render path.
/// The bridge package still stores glyph-local paint positions; callers can
/// derive final paint positions with `final_paint_pos()`.
///
/// `width_constraint` and `allow_wrap` are converted into IFC layout options by
/// the staging helper. `layout_size` is retained as call-site metadata for the
/// later `TextPassParams` insertion step.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextReadOnlyIfcBridgeInput {
    pub(crate) content: String,
    pub(crate) style: InlineIfcStyle,
    pub(crate) text_color: [f32; 4],
    pub(crate) opacity: f32,
    pub(crate) fragment_index: u32,
    pub(crate) origin: [f32; 2],
    pub(crate) layout_size: [f32; 2],
    pub(crate) width_constraint: Option<f32>,
    pub(crate) allow_wrap: bool,
}

impl TextReadOnlyIfcBridgeInput {
    pub(crate) fn new(
        content: impl Into<String>,
        style: InlineIfcStyle,
        opacity: f32,
        fragment_index: u32,
    ) -> Self {
        let text_color = brush_to_text_color_for_legacy_bridge(style.brush);
        Self {
            content: content.into(),
            style,
            text_color,
            opacity,
            fragment_index,
            origin: [0.0, 0.0],
            layout_size: [0.0, 0.0],
            width_constraint: None,
            allow_wrap: false,
        }
    }

    pub(crate) fn with_text_color(mut self, text_color: [f32; 4]) -> Self {
        self.text_color = text_color;
        self
    }

    pub(crate) fn final_paint_pos(&self, local_pos: [f32; 2]) -> [f32; 2] {
        [self.origin[0] + local_pos[0], self.origin[1] + local_pos[1]]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassPreparedInput {
    pub(crate) scale_factor: f32,
    pub(crate) batches: Vec<InlineTextPassPrepareComparableBatch>,
    pub(crate) glyphs: Vec<InlineTextPassPreparedGlyph>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassPreparedGlyph {
    pub(crate) glyph_index: usize,
    pub(crate) batch_index: Option<usize>,
    pub(crate) raster_key: Option<TextRasterKey>,
    pub(crate) paint: TextPassGlyphPaintInput,
    pub(crate) raster: TextPassRasterGlyphInput,
    pub(crate) final_paint_pos: [f32; 2],
}

pub(crate) fn build_inline_text_pass_prepared_input(
    input: &TextReadOnlyIfcBridgeInput,
    package: &InlineTextPassBridgePackage,
    scale_factor: f32,
) -> InlineTextPassPreparedInput {
    let mut batch_index_for_glyph = vec![None; package.glyphs.len()];
    for (batch_index, batch) in package.batches.iter().enumerate() {
        for &glyph_index in &batch.glyph_indices {
            if let Some(slot) = batch_index_for_glyph.get_mut(glyph_index) {
                *slot = Some(batch_index);
            }
        }
    }

    InlineTextPassPreparedInput {
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
            .map(|(glyph_index, glyph)| InlineTextPassPreparedGlyph {
                glyph_index,
                batch_index: batch_index_for_glyph[glyph_index],
                raster_key: text_raster_key_for_raster_input(&glyph.raster, scale_factor),
                paint: glyph.paint,
                raster: glyph.raster.clone(),
                final_paint_pos: input.final_paint_pos(glyph.paint.local_pos),
            })
            .collect(),
    }
}

pub(crate) fn inline_prepared_input_to_text_pass_staging_input(
    input: &InlineTextPassPreparedInput,
) -> TextPassPreparedStagingInput {
    TextPassPreparedStagingInput {
        scale_factor: input.scale_factor,
        glyphs: input
            .glyphs
            .iter()
            .map(|glyph| TextPassPreparedStagingGlyphInput {
                raster: glyph.raster.clone(),
                paint: glyph.paint,
                final_paint_pos: glyph.final_paint_pos,
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
pub(crate) struct InlineTextPassPreparedEquivalentProbe {
    pub(crate) prepared_input: InlineTextPassPreparedInput,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineTextPassPrepareComparablePackage {
    pub(crate) scale_factor: f32,
    pub(crate) batches: Vec<InlineTextPassPrepareComparableBatch>,
    pub(crate) glyphs: Vec<InlineTextPassPrepareComparableGlyph>,
}

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

#[cfg(test)]
pub(crate) fn inline_text_pass_prepared_equivalent_probe_for_test(
    input: &TextReadOnlyIfcBridgeInput,
    package: &InlineTextPassBridgePackage,
    scale_factor: f32,
) -> InlineTextPassPreparedEquivalentProbe {
    InlineTextPassPreparedEquivalentProbe {
        prepared_input: build_inline_text_pass_prepared_input(input, package, scale_factor),
    }
}

/// Builds the IFC -> TextPass bridge payload for the Text read-only staging path.
///
/// This intentionally models only the glyph-local package. Fragment origin is
/// carried by `TextReadOnlyIfcBridgeInput`, but final position application,
/// clipping, and final TextPass insertion remain owned by the current Text
/// render path until the formal call site switch.
pub(crate) fn build_text_read_only_ifc_bridge_package_from_input(
    input: &TextReadOnlyIfcBridgeInput,
) -> InlineTextPassBridgePackage {
    let ifc_input = InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
        source: InlineIfcSourceId(131),
        text: input.content.clone(),
        style: Some(input.style.clone()),
    }]);
    let layout_options = InlineIfcLayoutOptions::new(input.width_constraint, input.allow_wrap);
    let paint_input = InlineFormattingContext::build_with_options(ifc_input, layout_options)
        .text_pass_paint_input();
    let mut bridge = InlineTextPassBridgeInput::from_ifc_paint_input(
        &paint_input,
        input.opacity,
        input.fragment_index,
    );
    for glyph in &mut bridge.glyphs {
        glyph.paint.color = input.text_color;
    }
    InlineTextPassBridgePackage::from_bridge_input(bridge)
}

/// Compatibility wrapper for the initial Text read-only IFC helper.
#[cfg(test)]
pub(crate) fn build_text_read_only_ifc_bridge_package(
    content: &str,
    style: InlineIfcStyle,
    opacity: f32,
    fragment_index: u32,
) -> InlineTextPassBridgePackage {
    let input = TextReadOnlyIfcBridgeInput::new(content, style, opacity, fragment_index);
    build_text_read_only_ifc_bridge_package_from_input(&input)
}

#[cfg(test)]
pub(crate) fn text_read_only_ifc_prepare_comparable_package_for_test(
    content: &str,
    style: InlineIfcStyle,
    opacity: f32,
    fragment_index: u32,
    scale_factor: f32,
) -> InlineTextPassPrepareComparablePackage {
    let package = build_text_read_only_ifc_bridge_package(content, style, opacity, fragment_index);
    inline_text_pass_prepare_comparable_package_for_test(&package, scale_factor)
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

fn brush_to_text_color_for_legacy_bridge(brush: [u8; 4]) -> [f32; 4] {
    [
        brush[0] as f32 / 255.0,
        brush[1] as f32 / 255.0,
        brush[2] as f32 / 255.0,
        brush[3] as f32 / 255.0,
    ]
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
        text_raster_key_for_raster_input, text_raster_key_for_text_glyph,
    };
    use crate::view::text_layout::{TextGlyph, TextLayoutAlignment, build_text_layout};
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
                font_families: vec!["sans-serif".to_string()],
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
            font_families: vec!["sans-serif".to_string()],
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

    fn first_text_glyph_for_text(content: &str, style: &InlineIfcStyle) -> TextGlyph {
        build_text_layout(
            content,
            None,
            true,
            style.font_size,
            style.line_height,
            style.font_weight,
            TextLayoutAlignment::Left,
            &style.font_families,
        )
        .layout
        .lines()
        .into_iter()
        .flat_map(|line| line.glyphs)
        .find(|glyph| glyph.font_data.is_some())
        .expect("Text layout fixture should produce a renderable glyph")
    }

    fn paint_input_for_items(items: Vec<InlineIfcItem>) -> InlineIfcTextPassPaintInput {
        InlineFormattingContext::build(InlineIfcInput::new(items)).text_pass_paint_input()
    }

    fn brush_to_expected_text_color(brush: [u8; 4]) -> [f32; 4] {
        [
            brush[0] as f32 / 255.0,
            brush[1] as f32 / 255.0,
            brush[2] as f32 / 255.0,
            brush[3] as f32 / 255.0,
        ]
    }

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.001,
            "expected {actual} to be close to {expected}"
        );
    }

    fn assert_vec2_close(actual: [f32; 2], expected: [f32; 2]) {
        assert_f32_close(actual[0], expected[0]);
        assert_f32_close(actual[1], expected[1]);
    }

    fn text_read_only_existing_path_glyphs(
        content: &str,
        style: &InlineIfcStyle,
    ) -> Vec<(TextPassRasterGlyphInput, [f32; 2])> {
        text_read_only_existing_path_glyphs_with_layout(content, style, None, true)
    }

    fn text_read_only_existing_path_glyphs_with_layout(
        content: &str,
        style: &InlineIfcStyle,
        width: Option<f32>,
        allow_wrap: bool,
    ) -> Vec<(TextPassRasterGlyphInput, [f32; 2])> {
        build_text_layout(
            content,
            width,
            allow_wrap,
            style.font_size,
            style.line_height,
            style.font_weight,
            TextLayoutAlignment::Left,
            &style.font_families,
        )
        .layout
        .lines()
        .into_iter()
        .flat_map(|line| {
            let baseline_y = line.y + line.baseline;
            line.glyphs.into_iter().filter_map(move |glyph| {
                let raster = TextPassRasterGlyphInput::from_text_glyph(&glyph)?;
                let local_pos = [line.x + glyph.x, baseline_y + glyph.y];
                Some((raster, local_pos))
            })
        })
        .collect()
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
    fn bridge_raster_payload_matches_existing_text_glyph_path_for_same_text() {
        let content = "Parity";
        let style = parity_style();
        let bridged = first_ifc_bridge_glyph_for_text(content, &style);
        let text_glyph = first_text_glyph_for_text(content, &style);
        let text_raster = TextPassRasterGlyphInput::from_text_glyph(&text_glyph)
            .expect("TextGlyph should carry a complete font handle");
        let scale_factor = 1.25;

        assert_eq!(bridged.raster.glyph_id, text_raster.glyph_id);
        assert_eq!(bridged.raster.font_size, text_raster.font_size);
        assert_eq!(bridged.raster.font_data_id, text_raster.font_data_id);
        assert_eq!(bridged.raster.font_index, text_raster.font_index);
        assert_eq!(
            bridged.raster.normalized_coords_hash,
            text_raster.normalized_coords_hash
        );
        assert_eq!(
            bridged
                .raster
                .font_data
                .as_ref()
                .map(|font| (font.data.id(), font.index)),
            text_raster
                .font_data
                .as_ref()
                .map(|font| (font.data.id(), font.index))
        );

        let text_key = text_raster_key_for_text_glyph(&text_glyph, scale_factor)
            .expect("TextGlyph should produce a raster key");
        let text_input_key = text_raster_key_for_raster_input(&text_raster, scale_factor)
            .expect("neutral TextGlyph input should produce a raster key");
        let bridge_key = text_raster_key_for_raster_input(&bridged.raster, scale_factor)
            .expect("bridged IFC input should produce a raster key");
        assert_eq!(text_input_key, text_key);
        assert_eq!(bridge_key, text_key);

        let mut scale_context = SwashScaleContext::new();
        let mut raster_cache = FxHashMap::<TextRasterKey, CachedRasterImage>::default();
        let text_image = rasterize_text_pass_glyph_input(
            &mut scale_context,
            &mut raster_cache,
            201,
            &text_raster,
            scale_factor,
        )
        .expect("existing TextGlyph-neutral input should rasterize");
        let bridged_image = rasterize_text_pass_glyph_input(
            &mut scale_context,
            &mut raster_cache,
            202,
            &bridged.raster,
            scale_factor,
        )
        .expect("bridged IFC input should rasterize with the same key");

        assert!(!text_image.data.is_empty());
        assert!(!bridged_image.data.is_empty());
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
                    font_families: vec!["sans-serif".to_string()],
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

    #[test]
    fn text_read_only_opt_in_comparable_matches_existing_text_pass_semantics() {
        let content = "ReadOnly";
        let mut style = parity_style();
        style.brush = [12, 80, 200, 204];
        style.font_size = 19.0;
        style.line_height = 24.0;
        style.font_weight = 600;
        let opacity = 0.375;
        let fragment_index = 23;
        let scale_factor = 1.25;

        let comparable = text_read_only_ifc_prepare_comparable_package_for_test(
            content,
            style.clone(),
            opacity,
            fragment_index,
            scale_factor,
        );
        let existing = text_read_only_existing_path_glyphs(content, &style);

        assert_eq!(comparable.glyphs.len(), existing.len());
        assert_eq!(comparable.batches.len(), 1);
        assert_eq!(
            comparable.batches[0].glyph_indices,
            (0..comparable.glyphs.len()).collect::<Vec<_>>()
        );

        let expected_color = brush_to_expected_text_color(style.brush);
        for (comparable_glyph, (text_raster, text_local_pos)) in
            comparable.glyphs.iter().zip(existing.iter())
        {
            assert_eq!(comparable_glyph.raster.glyph_id, text_raster.glyph_id);
            assert_eq!(comparable_glyph.raster.font_size, text_raster.font_size);
            assert_eq!(
                comparable_glyph.raster.font_data_id,
                text_raster.font_data_id
            );
            assert_eq!(comparable_glyph.raster.font_index, text_raster.font_index);
            assert_eq!(
                comparable_glyph.raster.normalized_coords_hash,
                text_raster.normalized_coords_hash
            );
            assert_eq!(
                comparable_glyph.raster_key,
                text_raster_key_for_raster_input(text_raster, scale_factor)
            );

            assert_eq!(comparable_glyph.paint.color, expected_color);
            assert_eq!(comparable_glyph.paint.opacity, opacity);
            assert_eq!(comparable_glyph.paint.fragment_index, fragment_index);
            assert_vec2_close(comparable_glyph.paint.local_pos, *text_local_pos);
            assert!(comparable_glyph.batch_index.is_some());
            assert!(comparable_glyph.raster_key.is_some());
        }
    }

    #[test]
    fn text_read_only_staging_input_matches_legacy_wrapper_package() {
        let content = "Staging";
        let style = parity_style();
        let opacity = 0.6875;
        let fragment_index = 31;

        let input =
            TextReadOnlyIfcBridgeInput::new(content, style.clone(), opacity, fragment_index);
        let from_input = build_text_read_only_ifc_bridge_package_from_input(&input);
        let from_wrapper =
            build_text_read_only_ifc_bridge_package(content, style, opacity, fragment_index);

        assert_eq!(from_input, from_wrapper);
        assert!(
            from_input
                .glyphs
                .iter()
                .all(|glyph| glyph.paint.opacity == opacity
                    && glyph.paint.fragment_index == fragment_index)
        );
    }

    #[test]
    fn text_read_only_staging_origin_derives_final_paint_position() {
        let mut input = TextReadOnlyIfcBridgeInput::new("Origin", parity_style(), 0.5, 32);
        input.origin = [42.0, 18.5];
        input.layout_size = [160.0, 24.0];
        input.width_constraint = Some(160.0);
        input.allow_wrap = true;

        let mut zero_origin = input.clone();
        zero_origin.origin = [0.0, 0.0];
        let zero_origin_package = build_text_read_only_ifc_bridge_package_from_input(&zero_origin);
        let package = build_text_read_only_ifc_bridge_package_from_input(&input);
        let first_glyph = package
            .glyphs
            .iter()
            .find(|glyph| glyph.raster.font_data.is_some())
            .expect("Text staging input should produce a renderable glyph");
        let zero_origin_glyph = zero_origin_package
            .glyphs
            .iter()
            .find(|glyph| glyph.raster.font_data.is_some())
            .expect("zero-origin staging input should produce a renderable glyph");
        let final_pos = input.final_paint_pos(first_glyph.paint.local_pos);

        assert_vec2_close(
            first_glyph.paint.local_pos,
            zero_origin_glyph.paint.local_pos,
        );
        assert_vec2_close(
            final_pos,
            [
                input.origin[0] + first_glyph.paint.local_pos[0],
                input.origin[1] + first_glyph.paint.local_pos[1],
            ],
        );
        assert_eq!(first_glyph.paint.opacity, input.opacity);
        assert_eq!(first_glyph.paint.fragment_index, input.fragment_index);
    }

    #[test]
    fn text_read_only_staging_no_width_constraint_keeps_existing_single_flow() {
        let input = TextReadOnlyIfcBridgeInput::new("No width constraint", parity_style(), 0.8, 33);
        let package = build_text_read_only_ifc_bridge_package_from_input(&input);
        let existing = text_read_only_existing_path_glyphs_with_layout(
            &input.content,
            &input.style,
            None,
            false,
        );

        assert_eq!(package.glyphs.len(), existing.len());
        for (glyph, (text_raster, text_local_pos)) in package.glyphs.iter().zip(existing.iter()) {
            assert_eq!(glyph.raster.glyph_id, text_raster.glyph_id);
            assert_eq!(glyph.raster.font_size, text_raster.font_size);
            assert_vec2_close(glyph.paint.local_pos, *text_local_pos);
        }
    }

    #[test]
    fn text_read_only_staging_wraps_when_enabled_with_narrow_width() {
        let mut narrow = TextReadOnlyIfcBridgeInput::new(
            "Wrap staging text across lines",
            parity_style(),
            0.8,
            33,
        );
        narrow.origin = [10.0, 20.0];
        narrow.layout_size = [54.0, 96.0];
        narrow.width_constraint = Some(54.0);
        narrow.allow_wrap = true;
        let scale_factor = 1.0;

        let mut wide = narrow.clone();
        wide.origin = [80.0, 120.0];
        wide.layout_size = [320.0, 24.0];
        wide.width_constraint = Some(320.0);
        wide.allow_wrap = false;

        let narrow_package = build_text_read_only_ifc_bridge_package_from_input(&narrow);
        let wide_package = build_text_read_only_ifc_bridge_package_from_input(&wide);
        let narrow_existing = text_read_only_existing_path_glyphs_with_layout(
            &narrow.content,
            &narrow.style,
            narrow.width_constraint,
            true,
        );
        let comparable =
            inline_text_pass_prepare_comparable_package_for_test(&narrow_package, scale_factor);

        assert_eq!(narrow_package.glyphs.len(), wide_package.glyphs.len());
        assert_eq!(comparable.glyphs.len(), narrow_existing.len());
        assert!(
            narrow_package
                .glyphs
                .iter()
                .any(|glyph| glyph.paint.local_pos[1] > wide_package.glyphs[0].paint.local_pos[1])
        );
        for (comparable_glyph, (text_raster, text_local_pos)) in
            comparable.glyphs.iter().zip(narrow_existing.iter())
        {
            assert_eq!(
                comparable_glyph.paint.color,
                brush_to_expected_text_color(narrow.style.brush)
            );
            assert_eq!(comparable_glyph.paint.opacity, narrow.opacity);
            assert_eq!(comparable_glyph.paint.fragment_index, narrow.fragment_index);
            assert_eq!(comparable_glyph.raster.glyph_id, text_raster.glyph_id);
            assert_eq!(
                comparable_glyph.raster_key,
                text_raster_key_for_raster_input(text_raster, scale_factor)
            );
            assert_vec2_close(comparable_glyph.paint.local_pos, *text_local_pos);
        }
    }

    #[test]
    fn text_read_only_staging_does_not_wrap_when_disabled_with_narrow_width() {
        let mut nowrap = TextReadOnlyIfcBridgeInput::new(
            "No wrap staging text across lines",
            parity_style(),
            0.75,
            35,
        );
        nowrap.width_constraint = Some(48.0);
        nowrap.allow_wrap = false;

        let mut unconstrained = nowrap.clone();
        unconstrained.width_constraint = None;

        let nowrap_package = build_text_read_only_ifc_bridge_package_from_input(&nowrap);
        let unconstrained_package =
            build_text_read_only_ifc_bridge_package_from_input(&unconstrained);
        let existing = text_read_only_existing_path_glyphs_with_layout(
            &nowrap.content,
            &nowrap.style,
            nowrap.width_constraint,
            false,
        );

        assert_eq!(nowrap_package, unconstrained_package);
        assert_eq!(nowrap_package.glyphs.len(), existing.len());
        for (glyph, (text_raster, text_local_pos)) in nowrap_package.glyphs.iter().zip(existing) {
            assert_eq!(glyph.raster.glyph_id, text_raster.glyph_id);
            assert_vec2_close(glyph.paint.local_pos, text_local_pos);
        }
    }

    #[test]
    fn text_read_only_staging_preserves_color_opacity_fragment_and_raster_key() {
        let mut style = parity_style();
        style.brush = [180, 30, 90, 230];
        let mut input = TextReadOnlyIfcBridgeInput::new("Payload", style.clone(), 0.25, 34);
        input.origin = [5.0, 7.0];
        input.layout_size = [140.0, 26.0];
        input.width_constraint = Some(140.0);
        input.allow_wrap = false;
        let scale_factor = 1.75;

        let package = build_text_read_only_ifc_bridge_package_from_input(&input);
        let comparable =
            inline_text_pass_prepare_comparable_package_for_test(&package, scale_factor);
        let existing = text_read_only_existing_path_glyphs(&input.content, &style);
        let expected_color = brush_to_expected_text_color(style.brush);

        assert_eq!(comparable.glyphs.len(), existing.len());
        for (comparable_glyph, (text_raster, _)) in comparable.glyphs.iter().zip(existing.iter()) {
            assert_eq!(comparable_glyph.paint.color, expected_color);
            assert_eq!(comparable_glyph.paint.opacity, input.opacity);
            assert_eq!(comparable_glyph.paint.fragment_index, input.fragment_index);
            assert_eq!(
                comparable_glyph.raster_key,
                text_raster_key_for_raster_input(text_raster, scale_factor)
            );
            assert_eq!(
                input.final_paint_pos(comparable_glyph.paint.local_pos),
                [
                    input.origin[0] + comparable_glyph.paint.local_pos[0],
                    input.origin[1] + comparable_glyph.paint.local_pos[1],
                ]
            );
        }
    }

    #[test]
    fn text_read_only_opt_in_package_keeps_atomic_inline_box_out_of_glyph_path() {
        const TEXT_SOURCE: InlineIfcSourceId = InlineIfcSourceId(141);
        const BOX_SOURCE: InlineIfcSourceId = InlineIfcSourceId(142);

        let ifc = InlineFormattingContext::build(InlineIfcInput::new(vec![
            InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "T".to_string(),
                style: Some(parity_style()),
            },
            InlineIfcItem::AtomicInlineBox {
                source: BOX_SOURCE,
                measurement: measured_box(21.0, 11.0),
            },
            InlineIfcItem::TextSpan {
                source: TEXT_SOURCE,
                text: "X".to_string(),
                style: Some(parity_style()),
            },
        ]));
        let snapshot = ifc.text_layout_snapshot();
        let paint_input = snapshot.text_pass_paint_input();
        let package = build_inline_text_pass_bridge_package_for_test(&paint_input, 0.75, 3);
        let comparable = inline_text_pass_prepare_comparable_package_for_test(&package, 1.0);

        assert_eq!(snapshot.inline_boxes.len(), 1);
        assert_eq!(snapshot.inline_boxes[0].source, BOX_SOURCE);
        assert!(
            paint_input
                .glyphs
                .iter()
                .all(|glyph| glyph.source != BOX_SOURCE)
        );
        assert_eq!(package.glyphs.len(), paint_input.glyphs.len());
        assert_eq!(comparable.glyphs.len(), paint_input.glyphs.len());
        assert!(
            comparable
                .glyphs
                .iter()
                .all(|glyph| glyph.batch_index.is_some() && glyph.raster_key.is_some())
        );
    }
}
