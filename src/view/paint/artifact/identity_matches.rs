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
    // The streaming counterpart of the identity constructor.
    pub(crate) fn matches_rects<'a>(
        &self,
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> bool {
        matches!(self, Self::PreparedRects(expected) if rects_match(expected, rects))
    }
}
