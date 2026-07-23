use super::*;

#[test]
fn fixed_inline_root_with_text_uses_the_owning_ifc_artifact_path() {
    let (arena, roots, root, text) = prepared_fixed_owning_inline_text_root();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (text, PaintChunkRole::TextGlyphs),
        ]
    );

    let (legacy_arena, legacy_roots, ..) = prepared_fixed_owning_inline_text_root();
    assert_eq!(
        compiled_whole_frame_graph(&artifact).pass_descriptors(),
        legacy_roots_graph(legacy_arena, &legacy_roots).pass_descriptors()
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn standalone_inline_element_image_and_svg_do_not_require_an_owning_ifc_install() {
    fn assert_eligible(arena: &NodeArena, root: NodeKey, label: &str) {
        let roots = [root];
        let (properties, generations) = sync_identity(arena, &roots);
        let (_, eligibility) = whole_frame_artifact(arena, &roots, &properties, &generations);
        assert!(eligibility.eligible, "{label}: {eligibility:?}");
    }

    let mut element_arena = new_test_arena();
    let mut element = Element::new_with_id(0x7d10, 0.0, 0.0, 24.0, 18.0);
    let mut element_style = Style::new();
    element_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    element_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
    element_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    element_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(59, 130, 246)),
    );
    element.apply_style(element_style);
    let element_root = commit_element(&mut element_arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut element_arena, element_root, measure, place);
    element_arena
        .get_mut(element_root)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyFlags::ALL);
    element_arena.clear_arena_dirty_subtree(element_root, DirtyFlags::ALL);
    assert_eligible(&element_arena, element_root, "Element");

    let mut image_arena = new_test_arena();
    let mut image = Image::new_with_id(
        0x7d11,
        ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255, 255, 255, 255]),
        },
    );
    let mut image_style = Style::new();
    image_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    image_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
    image_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    image.apply_style(image_style);
    let image_root = commit_element(&mut image_arena, Box::new(image));
    measure_and_place(&mut image_arena, image_root, measure, place);
    image_arena
        .get_mut(image_root)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyFlags::ALL);
    image_arena.clear_arena_dirty_subtree(image_root, DirtyFlags::ALL);
    assert_eligible(&image_arena, image_root, "Image");

    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='18'><rect width='24' height='18' fill='#22c55e'/></svg>";
    let mut svg_arena = new_test_arena();
    let mut svg = Svg::new_with_id(0x7d12, SvgSource::Content(SVG.into()));
    let mut svg_style = Style::new();
    svg_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    svg_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
    svg_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    svg.apply_style(svg_style);
    let svg_root = commit_element(&mut svg_arena, Box::new(svg));
    measure_and_place(&mut svg_arena, svg_root, measure, place);
    {
        let mut node = svg_arena.get_mut(svg_root).unwrap();
        let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.prepare_content_paint_for_test(SVG, (24.0, 18.0), 1.0)
            .unwrap();
        svg.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    svg_arena.set_children(svg_root, Vec::new());
    svg_arena.clear_arena_dirty_subtree(svg_root, DirtyFlags::ALL);
    assert_eligible(&svg_arena, svg_root, "Svg");
}

#[test]
fn fixed_inline_root_missing_install_falls_back_before_full_hooks() {
    let (arena, roots, root, _) = prepared_fixed_owning_inline_text_root();
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .damage_owning_inline_ifc_root_witness_for_test(
            OwningInlineIfcRootWitnessDamage::MissingCurrent,
        );
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("fixed owning IFC root without a live install must fail closed")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineRoot
            ))
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn owning_inline_root_with_text_records_dom_order_and_matches_legacy() {
    let (arena, roots, root, text) = prepared_owning_inline_text_root();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 2);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (text, PaintChunkRole::TextGlyphs),
        ]
    );
    let artifact_graph = compiled_whole_frame_graph(&artifact);
    let artifact_passes = artifact_graph.pass_descriptors();

    let (legacy_arena, legacy_roots, ..) = prepared_owning_inline_text_root();
    let legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
    let legacy_passes = legacy_graph.pass_descriptors();
    assert_eq!(artifact_passes, legacy_passes);
}

#[test]
fn owning_inline_root_with_decorated_span_records_dom_dfs_and_matches_legacy() {
    let (arena, roots, root, span, text, fragment_count) =
        prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (span, PaintChunkRole::SelfDecoration),
            (text, PaintChunkRole::TextGlyphs),
        ],
        "coverage DOM DFS, not IFC plan order, is paint order"
    );
    assert_eq!(artifact.chunks[1].op_range.len(), fragment_count);
    assert!(matches!(
        artifact.chunks[1].payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(_, _)
    ));
    let artifact_graph = compiled_whole_frame_graph(&artifact);
    let artifact_passes = artifact_graph.pass_descriptors();

    let (legacy_arena, legacy_roots, ..) =
        prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
    let legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
    assert_eq!(artifact_passes, legacy_graph.pass_descriptors());
}

#[test]
fn owning_inline_root_witness_drift_falls_back_before_full_hooks() {
    for damage in [
        OwningInlineIfcRootWitnessDamage::MissingCurrent,
        OwningInlineIfcRootWitnessDamage::Pending,
        OwningInlineIfcRootWitnessDamage::ChildrenSnapshot,
        OwningInlineIfcRootWitnessDamage::PlanMissing,
        OwningInlineIfcRootWitnessDamage::PlanDuplicate,
        OwningInlineIfcRootWitnessDamage::InstalledMissing,
        OwningInlineIfcRootWitnessDamage::InstalledDuplicate,
        OwningInlineIfcRootWitnessDamage::CacheKey,
        OwningInlineIfcRootWitnessDamage::WrongKind,
        OwningInlineIfcRootWitnessDamage::LayoutDirty,
        OwningInlineIfcRootWitnessDamage::PlacementDirty,
    ] {
        let (arena, roots, root, _) = prepared_owning_inline_text_root();
        let mut node = arena.get_mut(root).unwrap();
        node.element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .damage_owning_inline_ifc_root_witness_for_test(damage);
        drop(node);

        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("owning IFC root witness drift must fail closed: {damage:?}")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                )),
            "{damage:?}: {eligibility:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0, "{damage:?}");
    }
}

#[test]
fn owning_inline_root_atomic_witness_field_drift_falls_back_before_full_hooks() {
    for damage in [
        OwningInlineIfcRootWitnessDamage::AtomicStableId,
        OwningInlineIfcRootWitnessDamage::AtomicSource,
        OwningInlineIfcRootWitnessDamage::AtomicInlineBoxId,
        OwningInlineIfcRootWitnessDamage::AtomicInsertionByte,
        OwningInlineIfcRootWitnessDamage::AtomicLineIndex,
        OwningInlineIfcRootWitnessDamage::AtomicMeasurementMaxWidth,
        OwningInlineIfcRootWitnessDamage::AtomicMeasurementAvailableHeight,
        OwningInlineIfcRootWitnessDamage::AtomicMeasurementViewport,
        OwningInlineIfcRootWitnessDamage::AtomicMeasurementPercentBase,
        OwningInlineIfcRootWitnessDamage::AtomicMeasurementSizing,
        OwningInlineIfcRootWitnessDamage::AtomicMeasurementSize,
        OwningInlineIfcRootWitnessDamage::AtomicRawRect,
        OwningInlineIfcRootWitnessDamage::AtomicAlignedRect,
        OwningInlineIfcRootWitnessDamage::AtomicVerticalAlign,
    ] {
        let (arena, roots, root, ..) = prepared_owning_inline_root_with_atomic();
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .damage_owning_inline_ifc_root_witness_for_test(damage);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("atomic witness field drift must fail closed: {damage:?}")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                )),
            "{damage:?}: {eligibility:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0, "{damage:?}");
    }
}

#[test]
fn owning_inline_root_requires_exactly_one_live_atomic_package_placement() {
    for (damage, fixture) in [
        (
            OwningInlineIfcRootWitnessDamage::AtomicPackageZeroPlacements,
            prepared_owning_inline_root_with_atomic
                as fn() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey),
        ),
        (
            OwningInlineIfcRootWitnessDamage::AtomicPackageDuplicatePlacements,
            || {
                let (arena, roots, root) = prepared_owning_inline_root_with_two_atomics();
                (arena, roots, root, root, root, root)
            },
        ),
    ] {
        let (arena, roots, root, ..) = fixture();
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .damage_owning_inline_ifc_root_witness_for_test(damage);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("non-exact atomic package cardinality must fail closed: {damage:?}")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                )),
            "{damage:?}: {eligibility:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0, "{damage:?}");
    }
}

#[test]
fn owning_inline_root_rejects_live_atomic_last_placement_drift() {
    let (mut arena, roots, _, _, atomic, _) = prepared_owning_inline_root_with_atomic();
    let mut drifted = arena
        .get(atomic)
        .unwrap()
        .element
        .last_placement()
        .expect("atomic fixture must retain its IFC placement");
    drifted.parent_x += 1.0;
    arena.with_element_taken(atomic, |child, arena| child.place(drifted, arena));
    arena
        .get_mut(atomic)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyFlags::ALL);
    arena.clear_arena_dirty_subtree(roots[0], DirtyFlags::ALL);

    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("live atomic placement drift must fail closed")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineRoot
            ))
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn owning_inline_root_requires_atomic_subtree_layout_placement_cleanliness() {
    let (arena, roots, root, before, atomic, grandchild, after) =
        prepared_owning_inline_root_with_atomic_subtree();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, PaintChunkRole::SelfDecoration),
            (before, PaintChunkRole::TextGlyphs),
            (atomic, PaintChunkRole::SelfDecoration),
            (grandchild, PaintChunkRole::SelfDecoration),
            (after, PaintChunkRole::TextGlyphs),
        ],
        "normal atomic subtrees remain coverage-DOM-DFS recordable"
    );
    let (legacy_arena, legacy_roots, ..) = prepared_owning_inline_root_with_atomic_subtree();
    assert_eq!(
        compiled_whole_frame_graph(&artifact).pass_descriptors(),
        legacy_roots_graph(legacy_arena, &legacy_roots).pass_descriptors()
    );

    arena
        .get_mut(grandchild)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .damage_owning_inline_ifc_root_witness_for_test(
            OwningInlineIfcRootWitnessDamage::LayoutDirty,
        );
    let mask = DirtyFlags::LAYOUT.union(DirtyFlags::PLACE);
    assert!(
        !arena
            .get(root)
            .unwrap()
            .element
            .local_dirty_flags()
            .intersects(mask)
    );
    assert!(
        !arena
            .get(atomic)
            .unwrap()
            .element
            .local_dirty_flags()
            .intersects(mask)
    );
    assert_eq!(arena.arena_local_dirty(root), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(atomic), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(grandchild), DirtyFlags::NONE);

    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("dirty atomic grandchild must fail closed before recording")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineRoot
            )),
        "{eligibility:?}"
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn owning_inline_root_text_live_shift_drift_falls_back_before_full_hooks() {
    let (arena, roots, _, text) = prepared_owning_inline_text_root();
    arena
        .get_mut(text)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Text>()
        .unwrap()
        .shift_inline_ifc_owned_geometry(1.0, 0.0);
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("live Text geometry not bound to the current plan must fail closed")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineRoot
            ))
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn owning_inline_root_same_kind_text_plan_swap_falls_back_before_full_hooks() {
    let (arena, roots, root) = prepared_owning_inline_two_text_root();
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .damage_owning_inline_ifc_root_witness_for_test(
            OwningInlineIfcRootWitnessDamage::TextPlanPayloadSwap,
        );
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("same-kind Text plan payload swap must fail closed")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineRoot
            ))
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}
