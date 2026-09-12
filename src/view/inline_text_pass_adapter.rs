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
mod tests;
