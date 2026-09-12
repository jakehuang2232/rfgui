use super::*;

fn rects_match<'a>(
    expected: &[PreparedDrawRectIdentity],
    rects: impl IntoIterator<Item = &'a DrawRectOp>,
) -> bool {
    let mut rects = rects.into_iter();
    expected.iter().all(|identity| {
        rects
            .next()
            .and_then(PreparedDrawRectIdentity::from_op)
            .as_ref()
            == Some(identity)
    }) && rects.next().is_none()
}

impl PaintPayloadIdentity {
    // These are the streaming counterparts of the identity constructors.
    // Compiler grammar and op canonicality checks remain separate. In
    // particular, a frozen identity alone does not validate a changed op.
    pub(crate) fn matches_rects<'a>(
        &self,
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> bool {
        matches!(self, Self::PreparedRects(expected) if rects_match(expected, rects))
    }

    pub(crate) fn matches_shadows_with_decoration<'a, 'b>(
        &self,
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        rects: impl IntoIterator<Item = &'b DrawRectOp>,
    ) -> bool {
        let Self::PreparedShadows(expected_shadows, expected_rects) = self else {
            return false;
        };
        shadows
            .into_iter()
            .map(|op| &op.identity)
            .eq(expected_shadows.iter())
            && rects_match(expected_rects, rects)
    }

    pub(crate) fn matches_texts<'a>(
        &self,
        texts: impl IntoIterator<Item = &'a PreparedTextOp>,
    ) -> bool {
        let Self::PreparedTexts(expected) = self else {
            return false;
        };
        texts.into_iter().map(|op| &op.identity).eq(expected.iter())
    }

    pub(crate) fn matches_inline_decorations<'a, 'b>(
        &self,
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decorations: impl IntoIterator<Item = &'b PreparedInlineIfcDecorationOp>,
    ) -> bool {
        let Self::InlineIfcDecorations(expected_shadows, expected_decorations) = self else {
            return false;
        };
        shadows
            .into_iter()
            .map(|op| &op.identity)
            .eq(expected_shadows.iter())
            && decorations
                .into_iter()
                .map(|op| &op.identity)
                .eq(expected_decorations.iter())
    }
}

#[cfg(test)]
mod tests;
