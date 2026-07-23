use super::*;

#[test]
fn property_scroll_b0_seals_phase_order_and_structural_zero_op_overlay() {
    let (arena, root, _child, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let plan = property_scroll_plan_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    )
    .unwrap();

    assert!(plan.is_canonical());
    assert!(plan.matches_live_inputs(&arena, &[root], &properties, &generations, sampled_at,));
    let [
        ScrollBoundaryStep::HostBefore {
            artifact: host,
            parent_span: host_span,
            ..
        },
        ScrollBoundaryStep::ContentComposite {
            artifact: content,
            parent_before,
            parent_after,
            ..
        },
        ScrollBoundaryStep::OverlayAfter {
            artifact: overlay,
            parent_span: overlay_span,
            ..
        },
    ] = plan.steps.as_slice()
    else {
        panic!("B0 phase shape must be host/content/overlay");
    };
    assert_eq!(*parent_before, host_span.end);
    assert_eq!(*parent_after, *parent_before);
    assert_eq!(overlay_span.start, *parent_after);
    assert!(!host.ops.is_empty());
    assert!(!content.ops.is_empty());
    assert!(overlay.ops.is_empty());
    assert_eq!(plan.seal.joint_transaction.roots.len(), 1);
    assert_eq!(plan.seal.joint_transaction.scroll_groups.len(), 1);
    assert!(plan.seal.joint_transaction.generic_full_set.is_empty());
}

#[test]
fn property_scroll_b0_content_identity_excludes_offset_and_overlay_alpha() {
    let sampled_at = crate::time::Instant::now();
    let (arena_a, root_a, _, properties_a, generations_a) = fixture_with_geometry_and_scrollbar(
        [0.0, 0.0],
        [100.0, 80.0],
        [300.0, 300.0],
        ScrollbarCase::Hidden,
        0.0,
    );
    let offset_a = property_scroll_plan_from_fixture(
        &arena_a,
        root_a,
        &properties_a,
        &generations_a,
        sampled_at,
        generous_budget(),
    )
    .unwrap();
    let (arena_b, root_b, _, properties_b, generations_b) = fixture_with_geometry_and_scrollbar(
        [0.0, 47.25],
        [100.0, 80.0],
        [300.0, 300.0],
        ScrollbarCase::Hidden,
        0.0,
    );
    let offset_b = property_scroll_plan_from_fixture(
        &arena_b,
        root_b,
        &properties_b,
        &generations_b,
        sampled_at,
        generous_budget(),
    )
    .unwrap();
    assert_eq!(offset_a.content_identity(), offset_b.content_identity());
    assert_ne!(
        offset_a.composite_dependency(),
        offset_b.composite_dependency()
    );

    let (arena_c, root_c, _, properties_c, generations_c) = fixture_with_geometry_and_scrollbar(
        [0.0, 0.0],
        [100.0, 80.0],
        [300.0, 300.0],
        ScrollbarCase::Opaque,
        0.0,
    );
    let alpha = property_scroll_plan_from_fixture(
        &arena_c,
        root_c,
        &properties_c,
        &generations_c,
        sampled_at,
        generous_budget(),
    )
    .unwrap();
    assert_eq!(offset_a.content_identity(), alpha.content_identity());
    assert_ne!(offset_a.overlay_identity(), alpha.overlay_identity());
}

#[test]
fn property_scroll_b0_alpha_only_changes_overlay_not_content_identity() {
    let (early_arena, early_root, early_properties, early_generations, early_time) =
        translucent_fixture_at(950);
    let (late_arena, late_root, late_properties, late_generations, late_time) =
        translucent_fixture_at(1_100);
    let early = property_scroll_plan_from_fixture(
        &early_arena,
        early_root,
        &early_properties,
        &early_generations,
        early_time,
        generous_budget(),
    )
    .unwrap();
    let late = property_scroll_plan_from_fixture(
        &late_arena,
        late_root,
        &late_properties,
        &late_generations,
        late_time,
        generous_budget(),
    )
    .unwrap();
    assert_eq!(
        early.seal.semantic.paint_state,
        ScrollbarPaintStateWitness::TranslucentNow
    );
    assert_eq!(
        early.seal.semantic.paint_state,
        late.seal.semantic.paint_state
    );
    assert_ne!(
        early.seal.semantic.sampled_alpha_bits,
        late.seal.semantic.sampled_alpha_bits
    );
    assert_eq!(early.content_identity(), late.content_identity());
    assert_eq!(early.composite_dependency(), late.composite_dependency());
    assert_ne!(early.overlay_identity(), late.overlay_identity());
}

#[test]
fn property_scroll_b0_scroll_generation_only_does_not_enter_content_identity() {
    let (arena, root, _, mut properties, mut generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let before = property_scroll_plan_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    )
    .unwrap();

    {
        let mut root_node = arena.get_mut(root).unwrap();
        let element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        assert!(element.set_hovered(true));
        let _ = element.tick_post_layout_animation_frame(sampled_at);
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);

    let leave = sampled_at + crate::time::Duration::from_millis(10);
    let hidden_time = leave + crate::time::Duration::from_millis(1_250);
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        assert!(element.set_hovered(false));
        let _ = element.tick_post_layout_animation_frame(leave);
        let _ = element.tick_post_layout_animation_frame(hidden_time);
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let after = property_scroll_plan_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        hidden_time,
        generous_budget(),
    )
    .unwrap();
    assert_ne!(before.seal.scroll.generation, after.seal.scroll.generation);
    let mut normalized_after = after.seal.scroll;
    normalized_after.generation = before.seal.scroll.generation;
    assert_eq!(before.seal.scroll, normalized_after);
    assert_eq!(before.content_identity(), after.content_identity());
    assert_eq!(before.composite_dependency(), after.composite_dependency());
}

#[test]
fn property_scroll_b0_tiled_plan_seals_order_gutter_and_budget() {
    let (arena, root, _, properties, generations) =
        fixture_with_geometry([0.0, 1000.0], [100.0, 80.0], [300.0, 3000.0]);
    let plan = property_scroll_plan_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
        tiled_budget(),
    )
    .unwrap();
    let ScrollBoundaryStep::ContentComposite {
        backing: PropertyScrollBackingPlan::Tiled(tiled),
        ..
    } = &plan.steps[1]
    else {
        panic!("oversized content must select tiled backing");
    };
    assert_eq!(tiled.gutter, 1);
    assert!(
        tiled
            .tiles
            .windows(2)
            .all(|pair| pair[0].index < pair[1].index)
    );
    assert_eq!(
        tiled.total_pair_bytes,
        tiled.tiles.iter().map(|tile| tile.pair_bytes).sum::<u64>()
    );
    assert!(tiled.total_pair_bytes <= tiled.budget.max_active_pair_bytes);

    let with_tiled =
        |plan: &mut PropertyScrollScenePlan,
         tamper: fn(&mut PropertyScrollTiledBackingPlan)| {
            let PropertyScrollBackingPlan::Tiled(tiled) = property_scroll_backing_mut(plan)
            else {
                unreachable!();
            };
            tamper(tiled);
        };
    for tamper in [
        (|tiled: &mut PropertyScrollTiledBackingPlan| tiled.tiles.swap(0, 1))
            as fn(&mut PropertyScrollTiledBackingPlan),
        |tiled| tiled.tiles[0].bounds.interior[0] += 1,
        |tiled| tiled.tiles[0].index.row += 1,
        |tiled| tiled.tiles[0].color_key = PersistentTextureKey::Generic(0xb0),
        |tiled| {
            let width = tiled.tiles[0].color_desc.width();
            let height = tiled.tiles[0].color_desc.height();
            tiled.tiles[0].color_desc = tiled.tiles[0]
                .color_desc
                .clone()
                .with_size(width + 1, height);
        },
        |tiled| {
            let width = tiled.tiles[0].depth_desc.width();
            let height = tiled.tiles[0].depth_desc.height();
            tiled.tiles[0].depth_desc = tiled.tiles[0]
                .depth_desc
                .clone()
                .with_size(width + 1, height);
        },
        |tiled| tiled.tiles[0].pair_bytes += 1,
        |tiled| tiled.total_pair_bytes += 1,
        |tiled| tiled.gutter = 0,
        |tiled| tiled.overscan += 1,
        |tiled| tiled.tile_edge -= 1,
    ] {
        assert_property_scroll_plan_tamper_rejected(&plan, |plan| with_tiled(plan, tamper));
    }
    assert_property_scroll_plan_tamper_rejected(&plan, |plan| {
        plan.seal.budget.max_active_pair_bytes = 1;
    });
}

#[test]
fn property_scroll_b0_single_plan_seals_keys_descriptors_and_pair_bytes() {
    let (arena, root, _, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let plan = property_scroll_plan_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        crate::time::Instant::now(),
        generous_budget(),
    )
    .unwrap();
    assert!(matches!(
        property_scroll_backing_mut(&mut plan.clone()),
        PropertyScrollBackingPlan::Single(_)
    ));
    for tamper in [
        (|single: &mut PropertyScrollSingleBackingPlan| {
            single.color_key = PersistentTextureKey::Generic(0xb0)
        }) as fn(&mut PropertyScrollSingleBackingPlan),
        |single| {
            single.color_desc = single
                .color_desc
                .clone()
                .with_size(single.color_desc.width() + 1, single.color_desc.height());
        },
        |single| {
            single.depth_desc = single
                .depth_desc
                .clone()
                .with_size(single.depth_desc.width() + 1, single.depth_desc.height());
        },
        |single| single.pair_bytes += 1,
    ] {
        assert_property_scroll_plan_tamper_rejected(&plan, |plan| {
            let PropertyScrollBackingPlan::Single(single) = property_scroll_backing_mut(plan)
            else {
                unreachable!();
            };
            tamper(single);
        });
    }
}

#[test]
fn property_scroll_b0_rejects_clip_cursor_and_semantic_time_tampering() {
    let (arena, root, child, mut properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let plan = property_scroll_plan_from_fixture(
        &arena,
        root,
        &properties,
        &generations,
        sampled_at,
        generous_budget(),
    )
    .unwrap();

    let mut bad_clip = plan.clone();
    let ScrollBoundaryStep::ContentComposite { clip_split, .. } = &mut bad_clip.steps[1] else {
        unreachable!();
    };
    clip_split
        .local_raster_clips
        .push(clip_split.own_contents_clip);
    assert!(!bad_clip.is_canonical());

    let mut bad_cursor = plan.clone();
    let ScrollBoundaryStep::ContentComposite { parent_after, .. } = &mut bad_cursor.steps[1]
    else {
        unreachable!();
    };
    *parent_after += 1;
    assert!(!bad_cursor.is_canonical());

    for tamper in [
        (|plan: &mut PropertyScrollScenePlan| {
            let ScrollBoundaryStep::ContentComposite { clip_split, .. } = &mut plan.steps[1]
            else {
                unreachable!();
            };
            clip_split.own_contents_clip.generation += 1;
        }) as fn(&mut PropertyScrollScenePlan),
        |plan| {
            let ScrollBoundaryStep::ContentComposite { clip_split, .. } = &mut plan.steps[1]
            else {
                unreachable!();
            };
            clip_split.own_contents_clip.logical_scissor[0] += 1;
        },
        |plan| {
            let ScrollBoundaryStep::ContentComposite { clip_split, .. } = &mut plan.steps[1]
            else {
                unreachable!();
            };
            clip_split
                .ancestor_composite_clips
                .push(clip_split.own_contents_clip);
        },
        |plan| {
            let ScrollBoundaryStep::ContentComposite { parent_before, .. } = &mut plan.steps[1]
            else {
                unreachable!();
            };
            *parent_before += 1;
        },
        |plan| {
            let ScrollBoundaryStep::HostBefore { parent_span, .. } = &mut plan.steps[0] else {
                unreachable!();
            };
            parent_span.end += 1;
        },
        |plan| {
            let ScrollBoundaryStep::OverlayAfter { parent_span, .. } = &mut plan.steps[2]
            else {
                unreachable!();
            };
            parent_span.start += 1;
        },
    ] {
        assert_property_scroll_plan_tamper_rejected(&plan, tamper);
    }

    assert!(!plan.matches_live_inputs(
        &arena,
        &[root],
        &properties,
        &generations,
        sampled_at + crate::time::Duration::from_millis(1),
    ));

    let live_scroll = properties
        .scrolls
        .get_mut(&crate::view::compositor::property_tree::ScrollNodeId(root))
        .unwrap();
    live_scroll.generation += 1;
    assert!(!plan.matches_live_inputs(&arena, &[root], &properties, &generations, sampled_at,));
    properties
        .scrolls
        .get_mut(&crate::view::compositor::property_tree::ScrollNodeId(root))
        .unwrap()
        .generation -= 1;

    arena
        .get_mut(child)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_background_color_value(Color::rgb(72, 48, 24));
    arena.refresh_subtree_dirty_cache(root);
    assert!(!plan.matches_live_inputs(&arena, &[root], &properties, &generations, sampled_at,));
}

#[test]
fn property_scroll_b0_rejects_unsupported_scene_contracts() {
    let (arena, root, _child, properties, generations) = fixture_at_offset([0.0, 20.0]);
    let sampled_at = crate::time::Instant::now();
    let budget = generous_budget();
    for result in [
        plan_property_scroll_scene_scaffold(
            &arena,
            &[root, root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        ),
        plan_property_scroll_scene_scaffold(
            &arena,
            &[root],
            &properties,
            &generations,
            2.0,
            [0.0; 2],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        ),
        plan_property_scroll_scene_scaffold(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.25, 0.0],
            None,
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        ),
        plan_property_scroll_scene_scaffold(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            Some([0, 0, 100, 80]),
            sampled_at,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        ),
    ] {
        assert_eq!(
            result.err(),
            Some(PropertyScrollScenePlanError::InvalidContract)
        );
    }
}

#[test]
fn property_scroll_b0_rejects_transform_effect_nested_scroll_and_colocation() {
    let reject = |arena: &NodeArena,
                  root,
                  properties: &PropertyTrees,
                  generations: &PaintGenerationTracker| {
        assert!(
            property_scroll_plan_from_fixture(
                arena,
                root,
                properties,
                generations,
                crate::time::Instant::now(),
                generous_budget(),
            )
            .is_err()
        );
    };

    for transform_owner_is_root in [true, false] {
        let (arena, root, child, mut properties, mut generations) =
            fixture_at_offset([0.0, 20.0]);
        let owner = if transform_owner_is_root { root } else { child };
        arena
            .get_mut(owner)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(3.0, 4.0, 0.0),
            )));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        assert!(properties.transforms.contains_key(
            &crate::view::compositor::property_tree::TransformNodeId(owner)
        ));
        if transform_owner_is_root {
            assert!(
                properties
                    .scrolls
                    .contains_key(&crate::view::compositor::property_tree::ScrollNodeId(root))
            );
        }
        reject(&arena, root, &properties, &generations);
    }

    for effect_owner_is_root in [true, false] {
        let (arena, root, child, mut properties, mut generations) =
            fixture_at_offset([0.0, 20.0]);
        let owner = if effect_owner_is_root { root } else { child };
        arena
            .get_mut(owner)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_opacity(0.5);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        assert!(
            properties
                .effects
                .contains_key(&crate::view::compositor::property_tree::EffectNodeId(owner))
        );
        if effect_owner_is_root {
            assert!(
                properties
                    .scrolls
                    .contains_key(&crate::view::compositor::property_tree::ScrollNodeId(root))
            );
        }
        reject(&arena, root, &properties, &generations);
    }

    let (mut arena, root, child, mut properties, mut generations) =
        fixture_at_offset([0.0, 20.0]);
    let grandchild = arena.insert(Node::new(Box::new(Element::new_with_id(
        82_003, 0.0, 0.0, 300.0, 600.0,
    ))));
    arena.set_parent(grandchild, Some(child));
    arena.push_child(child, grandchild);
    let mut child_node = arena.get_mut(child).unwrap();
    let child_element = child_node
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap();
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    child_element.apply_style(style);
    child_element.layout_state.content_size = Size {
        width: 300.0,
        height: 600.0,
    };
    child_element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    drop(child_node);
    arena
        .get_mut(grandchild)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let child_scroll = properties
        .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(child))
        .unwrap();
    assert_eq!(
        child_scroll.parent,
        Some(crate::view::compositor::property_tree::ScrollNodeId(root))
    );
    assert_eq!(properties.scrolls.len(), 2);
    reject(&arena, root, &properties, &generations);
}
