use super::*;

fn mutate_stamp_chunks(
    stamp: &mut RetainedSurfaceRasterStamp,
    mutate: impl Fn(&mut Vec<RetainedSurfaceChunkStamp>),
) {
    mutate(&mut stamp.chunks);
    stamp.op_count = stamp.chunks.iter().map(|chunk| chunk.op_count).sum();
    let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
        stamp.ordered_steps.as_mut_slice()
    else {
        panic!("TextArea content stamp must contain one artifact span")
    };
    mutate(&mut span.chunks);
    span.op_count = span.chunks.iter().map(|chunk| chunk.op_count).sum();
}

#[test]
fn atomic_projection_selection_live_authority_builds_exact6_same_key_raster_stamp() {
    let stable_id = 0xc3b4_4301;
    let baseline = atomic_projection_selection_content_stamp_for_test(6, stable_id)
        .expect("live recorded selection authority must build the dedicated stamp");
    let changed = atomic_projection_selection_content_stamp_for_test(5, stable_id)
        .expect("changed live selection output must remain admissible");
    assert!(retained_surface_raster_stamp_is_canonical(&baseline));
    assert!(retained_surface_raster_stamp_is_canonical(&changed));
    assert_eq!(baseline.chunks.len(), 6);
    assert_eq!(changed.chunks.len(), 6);
    assert_eq!(
        baseline.identity.resident_key(),
        changed.identity.resident_key(),
        "selection output changes must keep the same resident allocation key",
    );
    assert_ne!(
        baseline, changed,
        "generic payload identity must retain exact local selection output",
    );
}

#[test]
fn atomic_projection_selection_emission_constructor_requires_full_canonical_stamp() {
    let (plan, stamp) = atomic_projection_selection_emission_fixture_for_test(6, 0xc3b4_4303)
        .expect("canonical selection emission fixture");
    assert!(
        super::super::compiler::prepare_validated_scroll_scene_atomic_projection_selection_text_area_emission(
            plan.clone(),
            &stamp,
        )
        .is_some()
    );

    let mut drifted = stamp;
    drifted.chunks[1].bounds_bits[0] ^= 1;
    assert!(
        super::super::compiler::prepare_validated_scroll_scene_atomic_projection_selection_text_area_emission(
            plan,
            &drifted,
        )
        .is_none()
    );
}

#[test]
fn atomic_projection_selection_raster_stamp_rejects_generic_artifact_tamper() {
    let stable_id = 0xc3b4_4302;
    let stamp = atomic_projection_selection_content_stamp_for_test(6, stable_id)
        .expect("canonical live selection stamp");
    assert!(retained_surface_raster_stamp_is_canonical(&stamp));

    let mut missing_selection_payload = stamp.clone();
    mutate_stamp_chunks(&mut missing_selection_payload, |chunks| {
        let selection = chunks
            .iter_mut()
            .find(|chunk| chunk.id.role == PaintChunkRole::SelectionUnderlay)
            .expect("selection chunk");
        selection.payload_identity = PaintPayloadIdentity::None;
    });
    assert!(!retained_surface_raster_stamp_is_canonical(
        &missing_selection_payload
    ));

    let mut wrong_projection_scope = stamp.clone();
    mutate_stamp_chunks(&mut wrong_projection_scope, |chunks| {
        let projection = chunks
            .iter_mut()
            .rev()
            .find(|chunk| chunk.id.role == PaintChunkRole::TextGlyphs)
            .expect("projection glyph");
        projection.id.scope = PaintPropertyScope::Contents;
    });
    assert!(!retained_surface_raster_stamp_is_canonical(
        &wrong_projection_scope
    ));

    let mut wrong_role = stamp.clone();
    wrong_role.identity.role = RetainedSurfaceRasterRole::Transform;
    assert!(!retained_surface_raster_stamp_is_canonical(&wrong_role));

    let content_bounds = stamp.target.source_bounds_bits.map(|bits| {
        let value = f32::from_bits(bits);
        assert!(value >= 0.0 && value.fract() == 0.0);
        value as u32
    });
    let index = ScrollContentTileIndex { column: 0, row: 0 };
    let tile_edge = content_bounds[2].max(content_bounds[3]);
    let tile_bounds =
        ScrollContentTileBounds::for_index(content_bounds, tile_edge, 0, index).unwrap();
    let tile =
        ScrollContentTileRasterIdentity::new(index, content_bounds, tile_bounds, tile_edge, 0)
            .unwrap();
    let mut tile_misuse = stamp;
    tile_misuse.identity.scroll_content_tile = Some(tile);
    assert!(!retained_surface_raster_stamp_is_canonical(&tile_misuse));
}

#[test]
fn atomic_projection_text_area_content_raster_stamp_is_generic_and_closed() {
    let stable_id = 0xc3a_4301;
    let stamp = atomic_projection_content_stamp_for_test("projected", stable_id)
        .expect("dedicated atomic content stamp constructor");
    assert!(retained_surface_raster_stamp_is_canonical(&stamp));

    let mut missing_mask = stamp.clone();
    mutate_stamp_chunks(&mut missing_mask, |chunks| {
        chunks.remove(4);
    });
    assert!(!retained_surface_raster_stamp_is_canonical(&missing_mask));

    let mut reordered_mask = stamp.clone();
    mutate_stamp_chunks(&mut reordered_mask, |chunks| {
        chunks.swap(1, 4);
    });
    assert!(!retained_surface_raster_stamp_is_canonical(&reordered_mask));

    let mut wrong_mask_slot = stamp.clone();
    mutate_stamp_chunks(&mut wrong_mask_slot, |chunks| {
        chunks[4].id.slot = 0;
    });
    assert!(!retained_surface_raster_stamp_is_canonical(
        &wrong_mask_slot
    ));

    let RetainedSurfaceRasterStepStamp::ArtifactSpan(atomic_span) = &stamp.ordered_steps[0]
    else {
        panic!("atomic content stamp must have one artifact span")
    };
    let mut legacy_span = atomic_span.clone();
    let projection = legacy_span.chunks.remove(3);
    legacy_span.op_count = legacy_span
        .op_count
        .checked_sub(projection.op_count)
        .unwrap();
    let plain = super::super::compiler::validated_scroll_text_area_content_raster_stamp(
        stamp.identity.boundary_root,
        stable_id,
        stamp.target.clone(),
        legacy_span.clone(),
        stamp.opaque_order_span.clone(),
        PaintTextContentSource::Glyphs,
    )
    .expect("plain TextArea control stamp");
    assert!(retained_surface_raster_stamp_is_canonical(&plain));
    let interactive =
        super::super::compiler::validated_scroll_interactive_text_area_content_raster_stamp(
            stamp.identity.boundary_root,
            stable_id,
            stamp.target.clone(),
            legacy_span,
            stamp.opaque_order_span.clone(),
            super::super::legacy_admission::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs,
        )
        .expect("interactive TextArea control stamp");
    assert!(retained_surface_raster_stamp_is_canonical(&interactive));

    let mut generic_span = atomic_span.clone();
    generic_span.owner_topology.truncate(1);
    generic_span.clip_nodes.clear();
    generic_span.chunks.truncate(1);
    generic_span.op_count = generic_span.chunks[0].op_count;
    let generic = super::super::compiler::validated_scroll_content_raster_stamp(
        stamp.identity.boundary_root,
        stable_id,
        stamp.target.clone(),
        generic_span,
        stamp.opaque_order_span.clone(),
    )
    .expect("generic scroll-content control stamp");
    assert!(retained_surface_raster_stamp_is_canonical(&generic));

    let mut wrong_projection_role = stamp.clone();
    mutate_stamp_chunks(&mut wrong_projection_role, |chunks| {
        let projection = chunks
            .iter_mut()
            .rev()
            .find(|chunk| chunk.id.role == PaintChunkRole::TextGlyphs)
            .expect("projection glyph");
        projection.id.role = PaintChunkRole::SelfDecoration;
    });
    assert!(!retained_surface_raster_stamp_is_canonical(
        &wrong_projection_role
    ));

    let changed = atomic_projection_content_stamp_for_test("projection", stable_id)
        .expect("changed atomic resident stamp");
    assert!(retained_surface_raster_stamp_is_canonical(&changed));
    assert_eq!(
        stamp.identity.resident_key(),
        changed.identity.resident_key()
    );
    assert_ne!(
        stamp, changed,
        "same resident key must retain generic payload changes",
    );
}
