use super::*;

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
        "exact local selection output must participate in raster identity",
    );
    assert!(baseline.text_area_paint_grammar.is_none());
    assert!(baseline.interactive_text_area_resident.is_none());
    assert!(matches!(
        baseline.atomic_projection_text_area_resident,
        Some(super::super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Selection(_))
    ));
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
fn atomic_projection_selection_raster_stamp_rejects_hybrid_tile_role_and_tamper() {
    let stable_id = 0xc3b4_4302;
    let stamp = atomic_projection_selection_content_stamp_for_test(6, stable_id)
        .expect("canonical live selection stamp");
    let glyph_stamp = atomic_projection_content_stamp_for_test("projected", stable_id)
        .expect("canonical C3a glyph control stamp");

    let mut selection_with_plain = stamp.clone();
    selection_with_plain.text_area_paint_grammar =
        Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &selection_with_plain
    ));

    let mut selection_with_interactive = stamp.clone();
    selection_with_interactive.interactive_text_area_resident =
        Some(super::super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &selection_with_interactive
    ));

    let glyph_dependency = glyph_stamp
        .atomic_projection_text_area_resident
        .clone()
        .expect("C3a glyph dependency");
    let selection_dependency = stamp
        .atomic_projection_text_area_resident
        .clone()
        .expect("selection dependency");
    let mut selection_with_glyph_dependency = stamp.clone();
    selection_with_glyph_dependency.atomic_projection_text_area_resident =
        Some(glyph_dependency);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &selection_with_glyph_dependency
    ));
    let mut glyph_with_selection_dependency = glyph_stamp;
    glyph_with_selection_dependency.atomic_projection_text_area_resident =
        Some(selection_dependency);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &glyph_with_selection_dependency
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
    let mut tile_misuse = stamp.clone();
    tile_misuse.identity.scroll_content_tile = Some(tile);
    assert!(!retained_surface_raster_stamp_is_canonical(&tile_misuse));

    let mut synchronized_tamper = stamp;
    let Some(super::super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Selection(
        resident,
    )) = synchronized_tamper
        .atomic_projection_text_area_resident
        .as_mut()
    else {
        panic!("selection dependency variant")
    };
    let drifted = (f32::from_bits(resident.selection_chunk.bounds_bits[0]) + 1.0).to_bits();
    resident.selection_chunk.bounds_bits[0] = drifted;
    synchronized_tamper.chunks[1].bounds_bits[0] = drifted;
    let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
        synchronized_tamper.ordered_steps.as_mut_slice()
    else {
        panic!("selection stamp must own one exact4 span")
    };
    span.chunks[1].bounds_bits[0] = drifted;
    assert_eq!(synchronized_tamper.chunks, span.chunks);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &synchronized_tamper
    ));
}

#[test]
fn atomic_projection_text_area_content_raster_stamp_is_closed_and_one_hot() {
    let stable_id = 0xc3a_4301;
    let stamp = atomic_projection_content_stamp_for_test("projected", stable_id)
        .expect("dedicated atomic content stamp constructor");
    assert!(retained_surface_raster_stamp_is_canonical(&stamp));
    let synchronized_chunk_tamper =
        |stamp: &mut RetainedSurfaceRasterStamp,
         tamper: fn(&mut Vec<RetainedSurfaceChunkStamp>)| {
            tamper(&mut stamp.chunks);
            stamp.op_count = stamp.chunks.iter().map(|chunk| chunk.op_count).sum();
            let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
                stamp.ordered_steps.as_mut_slice()
            else {
                panic!("atomic stamp must contain one artifact span")
            };
            tamper(&mut span.chunks);
            span.op_count = span.chunks.iter().map(|chunk| chunk.op_count).sum();
        };
    let mut missing_mask = stamp.clone();
    synchronized_chunk_tamper(&mut missing_mask, |chunks| {
        chunks.remove(4);
    });
    assert!(!retained_surface_raster_stamp_is_canonical(&missing_mask));
    let mut reordered_mask = stamp.clone();
    synchronized_chunk_tamper(&mut reordered_mask, |chunks| {
        chunks.swap(1, 4);
    });
    assert!(!retained_surface_raster_stamp_is_canonical(&reordered_mask));
    let mut wrong_mask_slot = stamp.clone();
    synchronized_chunk_tamper(&mut wrong_mask_slot, |chunks| {
        chunks[4].id.slot = 0;
    });
    assert!(!retained_surface_raster_stamp_is_canonical(
        &wrong_mask_slot
    ));
    let mut wrong_mask_pair = stamp.clone();
    synchronized_chunk_tamper(&mut wrong_mask_pair, |chunks| {
        chunks[4].payload_identity = PaintPayloadIdentity::None;
    });
    assert!(!retained_surface_raster_stamp_is_canonical(
        &wrong_mask_pair
    ));
    assert!(stamp.text_area_paint_grammar.is_none());
    assert!(stamp.interactive_text_area_resident.is_none());
    assert!(stamp.atomic_projection_text_area_resident.is_some());

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
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly,
    )
    .expect("C1 plain TextArea control stamp");
    assert!(retained_surface_raster_stamp_is_canonical(&plain));
    let interactive =
        super::super::compiler::validated_scroll_interactive_text_area_content_raster_stamp(
            stamp.identity.boundary_root,
            stable_id,
            stamp.target.clone(),
            legacy_span,
            stamp.opaque_order_span.clone(),
            super::super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs,
        )
        .expect("C2 interactive TextArea control stamp");
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
    let atomic_resident = stamp
        .atomic_projection_text_area_resident
        .as_ref()
        .unwrap()
        .clone();
    let mut generic_with_atomic = generic;
    generic_with_atomic.atomic_projection_text_area_resident = Some(atomic_resident.clone());
    assert!(!retained_surface_raster_stamp_is_canonical(
        &generic_with_atomic
    ));
    let mut plain_with_interactive = plain.clone();
    plain_with_interactive.interactive_text_area_resident =
        Some(super::super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &plain_with_interactive
    ));
    let mut plain_with_atomic = plain;
    plain_with_atomic.atomic_projection_text_area_resident = Some(atomic_resident.clone());
    assert!(!retained_surface_raster_stamp_is_canonical(
        &plain_with_atomic
    ));
    let mut interactive_with_plain = interactive.clone();
    interactive_with_plain.text_area_paint_grammar =
        Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &interactive_with_plain
    ));
    let mut interactive_with_atomic = interactive;
    interactive_with_atomic.atomic_projection_text_area_resident = Some(atomic_resident);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &interactive_with_atomic
    ));

    let mut plain_atomic_hybrid = stamp.clone();
    plain_atomic_hybrid.text_area_paint_grammar =
        Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &plain_atomic_hybrid
    ));

    let mut interactive_atomic_hybrid = stamp.clone();
    interactive_atomic_hybrid.interactive_text_area_resident =
        Some(super::super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs);
    assert!(!retained_surface_raster_stamp_is_canonical(
        &interactive_atomic_hybrid
    ));

    let mut synchronized_public_tamper = stamp.clone();
    let Some(super::super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Glyph(
        atomic_resident,
    )) = synchronized_public_tamper
        .atomic_projection_text_area_resident
        .as_ref()
    else {
        panic!("C3a stamp must carry the glyph dependency")
    };
    let original_x = atomic_resident.wrapper_chunk.bounds_bits[0];
    let drifted_x = (f32::from_bits(original_x) + 1.0).to_bits();
    let Some(super::super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Glyph(
        atomic_resident,
    )) = synchronized_public_tamper
        .atomic_projection_text_area_resident
        .as_mut()
    else {
        panic!("C3a stamp must carry the glyph dependency")
    };
    atomic_resident.wrapper_chunk.bounds_bits[0] = drifted_x;
    synchronized_public_tamper.chunks[0].bounds_bits[0] = drifted_x;
    synchronized_public_tamper.target.source_bounds_bits[0] = drifted_x;
    let [target_x, target_y, target_width, target_height] = synchronized_public_tamper
        .target
        .source_bounds_bits
        .map(f32::from_bits);
    let rebuilt_color = crate::view::base_component::texture_desc_for_logical_bounds(
        crate::view::base_component::RetainedSurfaceBounds {
            x: target_x,
            y: target_y,
            width: target_width,
            height: target_height,
            corner_radii: [0.0; 4],
        },
        1.0,
        None,
        synchronized_public_tamper.target.color.format(),
    );
    let (rebuilt_color, rebuilt_depth) =
        crate::view::base_component::persistent_target_texture_descriptors(
            rebuilt_color,
            synchronized_public_tamper.identity.color_key,
        );
    synchronized_public_tamper.target.color = rebuilt_color;
    synchronized_public_tamper.target.depth = rebuilt_depth;
    let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
        synchronized_public_tamper.ordered_steps.as_mut_slice()
    else {
        panic!("atomic content stamp must have one artifact span")
    };
    span.chunks[0].bounds_bits[0] = drifted_x;
    assert_eq!(synchronized_public_tamper.chunks, span.chunks);
    assert_eq!(synchronized_public_tamper.op_count, span.op_count);
    let Some(super::super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Glyph(
        atomic_resident,
    )) = synchronized_public_tamper
        .atomic_projection_text_area_resident
        .as_ref()
    else {
        panic!("C3a stamp must carry the glyph dependency")
    };
    assert_eq!(
        synchronized_public_tamper.target.source_bounds_bits,
        atomic_resident.wrapper_chunk.bounds_bits,
    );
    assert!(
        synchronized_public_tamper
            .target
            .has_canonical_descriptor_pair_for(synchronized_public_tamper.identity),
        "synchronized source-position drift must pass public target structure",
    );
    assert!(!retained_surface_raster_stamp_is_canonical(
        &synchronized_public_tamper
    ));

    let mut role_misuse = stamp.clone();
    role_misuse.identity.role = RetainedSurfaceRasterRole::Transform;
    assert!(!retained_surface_raster_stamp_is_canonical(&role_misuse));

    let mut executor_transform = stamp.clone();
    executor_transform.text_area_paint_grammar = None;
    executor_transform.interactive_text_area_resident = None;
    executor_transform.atomic_projection_text_area_resident = None;
    executor_transform.identity.role = RetainedSurfaceRasterRole::Transform;
    executor_transform.identity.color_key =
        crate::view::base_component::transformed_layer_stable_key(stable_id);
    let [x, y, width, height] = executor_transform
        .target
        .source_bounds_bits
        .map(f32::from_bits);
    let transform_color = crate::view::base_component::texture_desc_for_logical_bounds(
        crate::view::base_component::RetainedSurfaceBounds {
            x,
            y,
            width,
            height,
            corner_radii: [0.0; 4],
        },
        1.0,
        None,
        executor_transform.target.color.format(),
    );
    let (transform_color, transform_depth) =
        crate::view::base_component::persistent_target_texture_descriptors(
            transform_color,
            executor_transform.identity.color_key,
        );
    executor_transform.target.color = transform_color;
    executor_transform.target.depth = transform_depth;
    assert!(
        !super::super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
            &executor_transform,
        ),
        "legacy property Transform control must reach its private canonicalizer",
    );
    let mut executor_plain = executor_transform.clone();
    executor_plain.text_area_paint_grammar =
        Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
    assert!(
        super::super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
            &executor_plain,
        )
    );
    let mut executor_interactive = executor_transform.clone();
    executor_interactive.interactive_text_area_resident =
        Some(super::super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs);
    assert!(
        super::super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
            &executor_interactive,
        )
    );
    let mut executor_atomic = executor_transform;
    executor_atomic.atomic_projection_text_area_resident =
        stamp.atomic_projection_text_area_resident.clone();
    assert!(
        super::super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
            &executor_atomic,
        )
    );

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
    let mut tile_misuse = stamp.clone();
    tile_misuse.identity.scroll_content_tile = Some(tile);
    tile_misuse.identity.color_key =
        crate::view::base_component::scroll_content_tile_layer_stable_key(
            stable_id,
            index.column,
            index.row,
        )
        .unwrap();
    let [tile_x, tile_y, tile_width, tile_height] =
        tile.bounds.raster.map(|value| value as f32);
    let tile_target_bounds = crate::view::base_component::RetainedSurfaceBounds {
        x: tile_x,
        y: tile_y,
        width: tile_width,
        height: tile_height,
        corner_radii: [0.0; 4],
    };
    let tile_color = crate::view::base_component::texture_desc_for_logical_bounds(
        tile_target_bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let (tile_color, tile_depth) =
        crate::view::base_component::persistent_target_texture_descriptors(
            tile_color,
            tile_misuse.identity.color_key,
        );
    tile_misuse.target = RetainedSurfaceRasterInputs {
        color: tile_color,
        depth: tile_depth,
        scale_factor_bits: 1.0_f32.to_bits(),
        source_bounds_bits: tile.bounds.raster.map(|value| (value as f32).to_bits()),
    };
    assert!(
        tile_misuse
            .target
            .has_canonical_descriptor_pair_for(tile_misuse.identity),
        "tile misuse must reach the TextArea dependency prohibition",
    );
    assert!(!retained_surface_raster_stamp_is_canonical(&tile_misuse));

    let changed = atomic_projection_content_stamp_for_test("projection", stable_id)
        .expect("changed atomic resident stamp");
    assert!(retained_surface_raster_stamp_is_canonical(&changed));
    assert_eq!(
        stamp.identity.resident_key(),
        changed.identity.resident_key()
    );
    assert_ne!(
        stamp, changed,
        "same key must retain resident stamp changes"
    );
}
