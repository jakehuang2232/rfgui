use slotmap::Key;

use super::*;

fn selection_op() -> DrawRectOp {
    DrawRectOp {
        params: RectPassParams {
            position: [4.0, 6.0],
            size: [20.0, 12.0],
            fill_color: [0.2, 0.4, 0.8, 1.0],
            opacity: 1.0,
            ..RectPassParams::default()
        },
        mode: RectRenderMode::FillOnly,
    }
}

fn selection_identity(op: &DrawRectOp) -> TextSelectionPayloadIdentity {
    PaintPayloadIdentity::prepared_text_selection(
        2,
        5,
        [0.2, 0.4, 0.8, 1.0].map(f32::to_bits),
        [op],
    )
    .and_then(|payload| payload.text_selection_identity())
    .expect("selection fixture must be canonical")
}

#[test]
fn selection_payload_field_tampers_return_typed_owner_rejections() {
    let owner = NodeKey::null();
    let op = selection_op();
    let baseline = selection_identity(&op);

    let mut start = baseline.clone();
    start.start_char = start.end_char;
    assert_eq!(
        start.validate_for_owner(owner),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::SelectionRange,
        })
    );

    let mut end = baseline.clone();
    end.end_char = end.start_char;
    assert_eq!(
        end.validate_for_owner(owner),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::SelectionRange,
        })
    );

    let mut color = baseline.clone();
    color.color_rgba_bits[0] = f32::NAN.to_bits();
    assert_eq!(
        color.validate_for_owner(owner),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::SelectionColor,
        })
    );

    let mut rect = baseline.clone();
    Arc::make_mut(&mut rect.rects)[0].params.position_bits[0] ^= 1;
    assert_eq!(
        rect.validate_exact_ops_for_owner(owner, &[op]),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::SelectionRectIdentity,
        })
    );
}

#[test]
fn selection_payload_synchronized_public_fields_still_require_live_source_parity() {
    let owner = NodeKey::null();
    let op = selection_op();
    let baseline = selection_identity(&op);
    let expected_source = PaintTextSelectionSource {
        start_char: baseline.start_char,
        end_char: baseline.end_char,
        color_rgba_bits: baseline.color_rgba_bits,
    };
    let mut synchronized = baseline.clone();
    synchronized.end_char += 1;
    let synchronized_source = PaintTextSelectionSource {
        end_char: synchronized.end_char,
        ..expected_source
    };

    assert!(synchronized
        .validate_exact_ops_for_owner(owner, &[op])
        .is_ok());
    assert!(synchronized_source.matches_payload(&synchronized));
    assert_eq!(
        expected_source.validate_payload_for_owner(owner, &synchronized),
        Err(PaintArtifactContractRejection {
            owner,
            violation: PaintArtifactContractViolation::SelectionSourceParity,
        }),
        "the independently frozen live source must reject a synchronized payload/source drift",
    );
}
