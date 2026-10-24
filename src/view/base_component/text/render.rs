//! Text paint recording: the prepared glyph payload built from the shaped
//! context measure produced. Text draws no selection.

use crate::style::ColorLike;
use crate::view::render_pass::text_pass::{TextPassPreparedFragment, TextPassPreparedParams};

use super::Text;
use super::paint_cache::{TextPaintKey, TextPaintMemo};
use std::sync::Arc;

impl Text {
    /// Exact visibility gate for retained recording.
    /// Callers supply the effective opacity. Neutral root-effect recording
    /// uses 1.0, while baked paths use the clamped local opacity.
    pub(super) fn is_paint_visible(&self, effective_opacity: f32) -> bool {
        self.layout_state.should_render
            && !self.backface_culled
            && !self.content.is_empty()
            && effective_opacity.is_finite()
            && effective_opacity > 0.0
    }

    pub(super) fn standalone_paint_bounds(&self) -> super::super::Rect {
        super::super::Rect {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.size.width.max(self.layout_state.layout_size.width),
            height: self.size.height.max(self.layout_state.layout_size.height),
        }
    }

    /// The immutable source recording reads: owned by an inline formatting
    /// context, or shaped by this Text itself. `None` without one.
    ///
    /// Either way the glyphs take the drawn color: an IFC shapes them with
    /// this Text's color as their brush, and an animating color changes the
    /// drawn one without reshaping.
    fn shadow_text_paint_source(&self, opacity: f32) -> Option<ShadowTextPaintSource<'_>> {
        if let Some(input) = self.inline_ifc_owned_paint_input() {
            let installed = self.inline_ifc_owned_paint_bounds()?;
            let bounds = super::super::Rect {
                x: installed.x,
                y: installed.y,
                width: installed.width,
                height: installed.height,
            };
            return Some(ShadowTextPaintSource {
                bounds,
                input: self.is_paint_visible(opacity).then_some(input),
                color_override: Some(self.drawn_color().to_rgba_f32()),
            });
        }
        let bounds = self.standalone_paint_bounds();
        if !self.is_paint_visible(opacity) {
            return Some(ShadowTextPaintSource {
                bounds,
                input: None,
                color_override: None,
            });
        }
        let input = self
            .shaped_context
            .as_ref()?
            .prepared_text_pass_paint_input_ref()?;
        Some(ShadowTextPaintSource {
            bounds,
            input: Some(input),
            color_override: Some(self.drawn_color().to_rgba_f32()),
        })
    }

    /// Recording materializes the glyph vector and frozen identity on a cache miss.
    pub(super) fn prepared_shadow_text_payload(
        &self,
        paint_offset: [f32; 2],
        opacity: f32,
    ) -> Option<Arc<PreparedShadowTextPayload>> {
        let source = self.shadow_text_paint_source(opacity)?;
        let Some(input) = source.input.filter(|input| !input.glyphs.is_empty()) else {
            return Some(Arc::new(PreparedShadowTextPayload {
                bounds: source.bounds,
                op: None,
            }));
        };
        let key = TextPaintKey::new(self, &source, paint_offset, opacity);
        if let Some(memo) = self
            .paint_memo
            .borrow()
            .as_ref()
            .filter(|memo| memo.key.matches(&key))
        {
            return Some(memo.payload.clone());
        }
        let fragment = source.fragment(paint_offset);
        let params = TextPassPreparedParams {
            staging_input: crate::view::render_pass::text_pass::TextPassPreparedStagingInput {
                scale_factor: 1.0,
                glyphs: source
                    .staged_glyphs(input, fragment.origin, opacity)
                    .collect(),
            },
            fragments: vec![fragment],
            scissor_rect: None,
            stencil_clip_id: None,
        };
        let op = crate::view::paint::PreparedTextOp::new(params)?;
        let payload = Arc::new(PreparedShadowTextPayload {
            bounds: source.bounds,
            op: Some(op),
        });
        *self.paint_memo.borrow_mut() = Some(TextPaintMemo {
            key,
            payload: payload.clone(),
        });
        Some(payload)
    }

    /// Staging input for the prepared glyph pass, built from the shaped
    /// context measure installed. Live color is injected here so color
    /// changes repaint without reshaping.
    #[cfg(test)]
    fn shaped_staging_input(
        &self,
        origin: [f32; 2],
        opacity: f32,
    ) -> Option<crate::view::render_pass::text_pass::TextPassPreparedStagingInput> {
        let context = self.shaped_context.as_ref()?;
        let paint_input = context.text_pass_paint_input_ref();
        Some(
            crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input_with_color(
                &paint_input,
                origin,
                opacity,
                0,
                1.0,
                Some(self.drawn_color().to_rgba_f32()),
            ),
        )
    }

    #[cfg(test)]
    pub(crate) fn shaped_staging_input_for_test(
        &self,
        origin: [f32; 2],
    ) -> Option<crate::view::render_pass::text_pass::TextPassPreparedStagingInput> {
        self.shaped_staging_input(origin, self.opacity.clamp(0.0, 1.0))
    }

    #[cfg(test)]
    pub(crate) fn shaped_context_for_test(
        &self,
    ) -> Option<&std::sync::Arc<crate::view::inline_formatting_context::InlineFormattingContext>>
    {
        self.shaped_context.as_ref()
    }
}

pub(super) struct PreparedShadowTextPayload {
    pub(super) bounds: super::super::Rect,
    pub(super) op: Option<crate::view::paint::PreparedTextOp>,
}

pub(super) struct ShadowTextPaintSource<'a> {
    pub(super) bounds: super::super::Rect,
    input: Option<&'a crate::view::inline_formatting_context::InlineIfcTextPassPaintInput>,
    pub(super) color_override: Option<[f32; 4]>,
}

impl ShadowTextPaintSource<'_> {
    fn fragment(&self, paint_offset: [f32; 2]) -> TextPassPreparedFragment {
        TextPassPreparedFragment {
            origin: [
                self.bounds.x + paint_offset[0],
                self.bounds.y + paint_offset[1],
            ],
            size: [self.bounds.width, self.bounds.height],
        }
    }

    fn staged_glyphs<'a>(
        &self,
        input: &'a crate::view::inline_formatting_context::InlineIfcTextPassPaintInput,
        origin: [f32; 2],
        opacity: f32,
    ) -> impl ExactSizeIterator<
        Item = crate::view::render_pass::text_pass::TextPassPreparedStagingGlyphInput,
    > + 'a {
        let color_override = self.color_override;
        input.glyphs.iter().map(move |glyph| {
            let raster =
                crate::view::inline_text_pass_adapter::inline_ifc_glyph_to_text_pass_raster_input(
                    input, glyph,
                );
            let mut paint =
                crate::view::inline_text_pass_adapter::inline_ifc_glyph_to_text_pass_paint_input(
                    input, glyph, opacity, 0,
                );
            if let Some(color) = color_override {
                paint.color = color;
            }
            crate::view::render_pass::text_pass::TextPassPreparedStagingGlyphInput {
                raster,
                paint,
                final_paint_pos: [
                    origin[0] + paint.local_pos[0],
                    origin[1] + paint.local_pos[1],
                ],
            }
        })
    }
}
