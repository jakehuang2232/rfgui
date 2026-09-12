//! One native self-paint input capsule per Element. Live layout/property/child
//! capability checks remain outside it. Colors are sampled on every call and
//! compared as values, so a mutable custom ColorLike cannot hide behind a style
//! pointer or dirty flag. The last capsule is replaced on input change and
//! released with Element; hidden owners may retain this one CPU capsule.
use super::*;
use crate::view::paint::{DrawRectOp, PaintPayloadIdentity, PreparedShadowOp};
use crate::view::render_pass::draw_rect_pass::GradientPaint;
use std::sync::Arc;

pub(super) struct NativeSelfPaintInputs {
    decoration: SelfDecorationPaintOps,
    shadow_rects: Vec<[u32; 4]>,
    shadow_radii: [u32; 4],
    shadow_params: Vec<([u32; 8], bool)>,
    shadow_enabled: bool,
    offset: [u32; 2],
    opacity: u32,
    shadows: Arc<[PreparedShadowOp]>,
    payload: PaintPayloadIdentity,
}
fn shadow_params(shadow: &BoxShadow) -> ([u32; 8], bool) {
    let color = shadow.color.to_rgba_f32();
    (
        [
            shadow.offset_x,
            shadow.offset_y,
            shadow.blur,
            shadow.spread,
            color[0],
            color[1],
            color[2],
            color[3],
        ]
        .map(f32::to_bits),
        shadow.inset,
    )
}
fn shadow_rects(element: &Element) -> impl Iterator<Item = [u32; 4]> + '_ {
    let fragmented =
        element.is_fragmentable_inline_element() && !element.inline_paint_fragments.is_empty();
    let rects = if fragmented {
        element.inline_paint_fragments.as_slice()
    } else {
        &[]
    };
    rects
        .iter()
        .map(|r| [r.x, r.y, r.width, r.height].map(f32::to_bits))
        .chain(
            std::iter::once(
                [
                    element.layout_state.layout_position.x,
                    element.layout_state.layout_position.y,
                    element.layout_state.layout_size.width,
                    element.layout_state.layout_size.height,
                ]
                .map(f32::to_bits),
            )
            .take(usize::from(!fragmented)),
        )
}
fn gradient_eq(a: &Option<GradientPaint>, b: &Option<GradientPaint>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let GradientPaint {
                kind,
                axis,
                repeating,
                stops,
            } = a;
            kind == &b.kind
                && axis.map(f32::to_bits) == b.axis.map(f32::to_bits)
                && repeating == &b.repeating
                && stops.len() == b.stops.len()
                && (Arc::ptr_eq(stops, &b.stops)
                    || stops.iter().zip(b.stops.iter()).all(|(a, b)| {
                        a.color.map(f32::to_bits) == b.color.map(f32::to_bits)
                            && a.pos.map(f32::to_bits) == b.pos.map(f32::to_bits)
                    }))
        }
        _ => false,
    }
}
fn rect_eq(a: &Option<DrawRectOp>, b: &Option<DrawRectOp>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            // Exhaustive fields make new rectangle inputs an explicit cache
            // dependency review. No epsilon, hashing, or padding-byte comparison.
            let RectPassParams {
                position,
                size,
                fill_color,
                opacity,
                border_widths,
                border_radii,
                border_color,
                border_side_colors,
                use_border_side_colors,
                depth,
                gradient,
                border_gradient,
            } = &a.params;
            let q = &b.params;
            a.mode == b.mode
                && position.map(f32::to_bits) == q.position.map(f32::to_bits)
                && size.map(f32::to_bits) == q.size.map(f32::to_bits)
                && fill_color.map(f32::to_bits) == q.fill_color.map(f32::to_bits)
                && opacity.to_bits() == q.opacity.to_bits()
                && border_widths.map(f32::to_bits) == q.border_widths.map(f32::to_bits)
                && border_radii.map(|r| r.map(f32::to_bits))
                    == q.border_radii.map(|r| r.map(f32::to_bits))
                && border_color.map(f32::to_bits) == q.border_color.map(f32::to_bits)
                && border_side_colors.map(|c| c.map(f32::to_bits))
                    == q.border_side_colors.map(|c| c.map(f32::to_bits))
                && use_border_side_colors == &q.use_border_side_colors
                && depth.to_bits() == q.depth.to_bits()
                && gradient_eq(gradient, &q.gradient)
                && gradient_eq(border_gradient, &q.border_gradient)
        }
        _ => false,
    }
}
impl NativeSelfPaintInputs {
    fn matches_shadow_inputs(
        &self,
        element: &Element,
        context: &crate::view::paint::PaintRecordingContext,
    ) -> bool {
        self.shadow_enabled
            && element.core.should_paint
            && !element.box_shadows.is_empty()
            && self.offset == context.paint_offset.map(f32::to_bits)
            && self.opacity == context.paint_opacity(element.opacity).to_bits()
            && self.shadow_radii == element.border_radii.to_array().map(f32::to_bits)
            && self.shadow_rects.iter().copied().eq(shadow_rects(element))
            && self
                .shadow_params
                .iter()
                .copied()
                .eq(element.box_shadows.iter().map(shadow_params))
    }
}
impl Element {
    pub(super) fn replay_prepared_outer_shadow_ops(
        &self,
        context: &crate::view::paint::PaintRecordingContext,
    ) -> Option<Vec<PreparedShadowOp>> {
        let memo = self.paint_recording_inputs.borrow();
        let entry = memo
            .as_ref()
            .filter(|entry| entry.matches_shadow_inputs(self, context))?;
        Some(entry.shadows.iter().cloned().collect())
    }

    pub(super) fn prepared_self_paint_from_inputs(
        &self,
        geometry: SelfPaintRecordingGeometry,
        context: &crate::view::paint::PaintRecordingContext,
    ) -> Result<PreparedSelfPaintRecord, crate::view::paint::LegacyPaintReason> {
        use crate::view::paint::LegacyPaintReason;
        let opacity = context.paint_opacity(self.opacity);
        let decoration = self.self_decoration_paint_ops(opacity, context.paint_offset);
        let enabled = self.core.should_paint && !self.box_shadows.is_empty();
        let offset = context.paint_offset.map(f32::to_bits);
        let radii = self.border_radii.to_array().map(f32::to_bits);
        let memo = self.paint_recording_inputs.borrow();
        if let Some(old) = memo.as_ref().filter(|old| {
            rect_eq(&old.decoration.fill, &decoration.fill)
                && rect_eq(&old.decoration.border, &decoration.border)
                && old.shadow_enabled == enabled
                && (!enabled
                    || (old.offset == offset
                        && old.opacity == opacity.to_bits()
                        && old.shadow_radii == radii
                        && old.shadow_rects.iter().copied().eq(shadow_rects(self))
                        && old
                            .shadow_params
                            .iter()
                            .copied()
                            .eq(self.box_shadows.iter().map(shadow_params))))
        }) {
            #[cfg(test)]
            tests::native_paint_input_tests::note_replay();
            return Ok(PreparedSelfPaintRecord {
                geometry,
                shadows: old.shadows.clone(),
                decoration,
                payload_identity: old.payload.clone(),
            });
        }
        drop(memo);
        let shadows: Arc<[PreparedShadowOp]> = self
            .prepared_outer_shadow_ops(context)
            .ok_or(LegacyPaintReason::BoxShadow)?
            .into();
        let payload = PaintPayloadIdentity::prepared_shadows_with_decoration(
            shadows.iter(),
            decoration.fill.iter().chain(&decoration.border),
        )
        .ok_or(LegacyPaintReason::StatefulPaint)?;
        *self.paint_recording_inputs.borrow_mut() = Some(NativeSelfPaintInputs {
            decoration: decoration.clone(),
            shadow_rects: if enabled {
                shadow_rects(self).collect()
            } else {
                Vec::new()
            },
            shadow_radii: radii,
            shadow_params: if enabled {
                self.box_shadows.iter().map(shadow_params).collect()
            } else {
                Vec::new()
            },
            shadow_enabled: enabled,
            offset,
            opacity: opacity.to_bits(),
            shadows: shadows.clone(),
            payload: payload.clone(),
        });
        Ok(PreparedSelfPaintRecord {
            geometry,
            shadows,
            decoration,
            payload_identity: payload,
        })
    }
}
