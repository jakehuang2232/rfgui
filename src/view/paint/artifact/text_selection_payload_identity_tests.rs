use super::*;

fn selection_rect(color: [f32; 4], opacity: f32) -> DrawRectOp {
    DrawRectOp {
        params: RectPassParams {
            position: [10.0, 20.0],
            size: [30.0, 12.0],
            fill_color: color,
            opacity,
            ..Default::default()
        },
        mode: RectRenderMode::FillOnly,
    }
}

#[test]
fn text_selection_payload_identity_is_self_canonical() {
    let color = [0.1, 0.2, 0.3, 1.0];
    let color_rgba_bits = color.map(f32::to_bits);
    let rect = selection_rect(color, 1.0);
    let payload = PaintPayloadIdentity::prepared_text_selection(2, 5, color_rgba_bits, [&rect])
        .expect("a finite ordered selection with exact fill rects is canonical");
    let identity = payload.text_selection_identity().unwrap();

    assert!(identity.is_canonical());
    assert!(identity.matches_source(2, 5, color_rgba_bits));
    assert!(payload.matches_text_selection_source(2, 5, color_rgba_bits));
    assert!(payload.matches_exact_text_selection_ops([&rect]));
}

#[test]
fn text_selection_payload_identity_rejects_invalid_source_or_rects() {
    let color = [0.1, 0.2, 0.3, 1.0];
    let color_rgba_bits = color.map(f32::to_bits);
    let rect = selection_rect(color, 1.0);
    assert!(
        PaintPayloadIdentity::prepared_text_selection(5, 5, color_rgba_bits, [&rect],).is_none()
    );
    assert!(PaintPayloadIdentity::prepared_text_selection(
        2,
        5,
        [
            f32::NAN.to_bits(),
            color_rgba_bits[1],
            color_rgba_bits[2],
            color_rgba_bits[3]
        ],
        [&rect],
    )
    .is_none());

    let translucent = selection_rect(color, 0.5);
    assert!(
        PaintPayloadIdentity::prepared_text_selection(2, 5, color_rgba_bits, [&translucent],)
            .is_none()
    );
    assert!(PaintPayloadIdentity::prepared_text_selection(
        2,
        5,
        color_rgba_bits,
        std::iter::empty(),
    )
    .is_none());
}
