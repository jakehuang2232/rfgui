use super::*;

#[test]
fn scroll_state_applies_to_descendants_not_owner_paint() {
    let mut arena = NodeArena::new();
    let (root, child) = make_vertical_scroll_fixture(&mut arena, 1, 2);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    trees.sync(&arena, &[root]);

    arena
        .get_mut(root)
        .expect("root exists")
        .element
        .set_scroll_offset((0.0, 24.0));
    // A newly written offset invalidates placement; property sync must not
    // combine it with the old placed geometry.
    trees.sync(&arena, &[root]);
    assert!(!trees.scrolls.contains_key(&ScrollNodeId(root)));
    assert!(trees.validation_errors.contains(
        &PropertyTreeValidationError::ScrollContractUnavailable(root)
    ));
    clear_layout_dirty_for_subtree(&arena, root);
    trees.sync(&arena, &[root]);

    assert_eq!(trees.states[&root].paint.scroll, None);
    assert_eq!(
        trees.states[&root].descendants.scroll,
        Some(ScrollNodeId(root))
    );
    assert_eq!(trees.states[&child].paint.scroll, Some(ScrollNodeId(root)));
    assert!(
        trees
            .changes_for(root)
            .contains(PropertyChangeFlags::SCROLL)
    );
    assert_eq!(
        trees.scrolls[&ScrollNodeId(root)].offset,
        Vec2::new(0.0, 24.0)
    );
    let scroll = trees.scrolls[&ScrollNodeId(root)];
    assert_eq!(scroll.configured_axis, ScrollAxisSnapshot::Vertical);
    assert!(rect_bits_equal(
        scroll.viewport,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        }
    ));
    assert_eq!(
        [scroll.content_size.width, scroll.content_size.height],
        [100.0, 300.0]
    );
}

#[test]
fn canonical_scroll_geometry_and_live_observation_preserve_full_2d_state_for_all_axes() {
    let cases = [
        (
            ScrollDirection::Vertical,
            ScrollAxisSnapshot::Vertical,
            false,
            true,
        ),
        (
            ScrollDirection::Horizontal,
            ScrollAxisSnapshot::Horizontal,
            true,
            false,
        ),
        (ScrollDirection::Both, ScrollAxisSnapshot::Both, true, true),
    ];
    let mut both = None;
    for (index, (direction, expected_axis, expect_horizontal, expect_vertical)) in
        cases.into_iter().enumerate()
    {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 20_000 + index as u64 * 2);
        let child = insert_element(&mut arena, 20_001 + index as u64 * 2);
        append_child(&mut arena, root, child);
        set_scroll_direction(&arena, root, direction);
        for owner in [root, child] {
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            arena
                .get_mut(owner)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .apply_style(style);
        }
        arena
            .get_mut(child)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_background_color_value(crate::style::Color::rgb(24, 48, 72));
        let viewport = Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        };
        let content_size = [280.0, 260.0];
        let offset = [37.0, 41.0];
        install_scroll_layout_geometry(&arena, root, viewport, content_size);
        install_scroll_layout_geometry(
            &arena,
            child,
            Rect {
                x: viewport.x - offset[0],
                y: viewport.y - offset[1],
                width: content_size[0],
                height: content_size[1],
            },
            content_size,
        );
        arena
            .get_mut(root)
            .unwrap()
            .element
            .set_scroll_offset((offset[0], offset[1]));
        clear_layout_dirty_for_subtree(&arena, root);

        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        assert!(trees.validation_errors.is_empty(), "{direction:?}");
        let snapshot = trees.scroll_snapshot_for(ScrollNodeId(root)).unwrap();
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let clip = trees.clip_snapshot_for(Some(clip_id)).unwrap()[0];
        assert_eq!(snapshot.configured_axis, expected_axis);
        assert_eq!(snapshot.offset, Vec2::new(offset[0], offset[1]));
        assert_eq!(
            snapshot.scrollbar_overlay.horizontal_track.is_some(),
            expect_horizontal
        );
        assert_eq!(
            snapshot.scrollbar_overlay.vertical_track.is_some(),
            expect_vertical
        );
        assert!(snapshot.has_canonical_geometry_with_contents_clip(clip));
        assert!(snapshot.has_canonical_vertical_geometry_with_contents_clip(clip));

        let crate::view::base_component::ScrollGeometryObservation::Exact(live_scroll) = arena
            .get(root)
            .unwrap()
            .element
            .scroll_geometry_observation(root, &arena)
        else {
            panic!("{direction:?} must supply exact live geometry");
        };
        assert_eq!(live_scroll.configured_axis, expected_axis);
        assert_eq!(
            live_scroll.offset.map(f32::to_bits),
            offset.map(f32::to_bits)
        );
        let child_bounds = arena.get(child).unwrap().element.box_model_snapshot();
        assert_eq!(
            (child_bounds.x + live_scroll.offset[0]).to_bits(),
            live_scroll.layout_content_bounds_at_zero.x.to_bits()
        );
        assert_eq!(
            (child_bounds.y + live_scroll.offset[1]).to_bits(),
            live_scroll.layout_content_bounds_at_zero.y.to_bits()
        );

        if expected_axis == ScrollAxisSnapshot::Both {
            both = Some((snapshot, clip));
        }
    }

    let (both, clip) = both.expect("Both fixture");
    let mut offset_tampered = both;
    offset_tampered.offset.x += 1.0;
    assert!(!offset_tampered.has_canonical_geometry_with_contents_clip(clip));

    let mut bounds_tampered = both;
    bounds_tampered.layout_content_bounds_at_zero.width += 1.0;
    assert!(!bounds_tampered.has_canonical_geometry_with_contents_clip(clip));

    let mut axis_tampered = both;
    axis_tampered.configured_axis = ScrollAxisSnapshot::Vertical;
    assert!(!axis_tampered.has_canonical_geometry_with_contents_clip(clip));

    let mut overlay_tampered = both;
    overlay_tampered
        .scrollbar_overlay
        .horizontal_thumb
        .as_mut()
        .unwrap()
        .x += 1.0;
    assert!(!overlay_tampered.has_canonical_geometry_with_contents_clip(clip));
}

#[test]
fn scroll_snapshot_generations_track_axis_viewport_content_clip_and_overlay_fields() {
    let mut arena = NodeArena::new();
    let (root, _child) = make_vertical_scroll_fixture(&mut arena, 10, 11);
    let scroll_id = ScrollNodeId(root);
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let stable_scroll = trees.scrolls[&scroll_id].generation;
    let stable_clip = trees.clips[&clip_id].generation;

    trees.sync(&arena, &[root]);
    assert_eq!(trees.scrolls[&scroll_id].generation, stable_scroll);
    assert_eq!(trees.clips[&clip_id].generation, stable_clip);
    assert_eq!(trees.changes_for(root), PropertyChangeFlags::NONE);

    install_scroll_layout_geometry(
        &arena,
        root,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        },
        [100.0, 360.0],
    );
    trees.sync(&arena, &[root]);
    let content_generation = trees.scrolls[&scroll_id].generation;
    assert!(content_generation > stable_scroll);
    assert_eq!(trees.clips[&clip_id].generation, stable_clip);
    assert!(
        trees
            .changes_for(root)
            .contains(PropertyChangeFlags::SCROLL)
    );
    assert!(!trees.changes_for(root).contains(PropertyChangeFlags::CLIP));

    install_scroll_layout_geometry(
        &arena,
        root,
        Rect {
            x: 11.25,
            y: 21.5,
            width: 96.5,
            height: 76.25,
        },
        [180.0, 360.0],
    );
    set_scroll_direction(&arena, root, ScrollDirection::Both);
    clear_layout_dirty_for_subtree(&arena, root);
    trees.sync(&arena, &[root]);
    let geometry_generation = trees.scrolls[&scroll_id].generation;
    assert!(geometry_generation > content_generation);
    assert_eq!(
        trees.scrolls[&scroll_id].configured_axis,
        ScrollAxisSnapshot::Both
    );
    assert!(trees.clips[&clip_id].generation > stable_clip);
    assert!(matches!(
        trees.clips[&clip_id].geometry,
        ClipGeometry::LogicalScissor([11, 21, 97, 77])
    ));
    assert!(
        trees
            .changes_for(root)
            .contains(PropertyChangeFlags::SCROLL)
    );
    assert!(trees.changes_for(root).contains(PropertyChangeFlags::CLIP));

    arena
        .get_mut(root)
        .expect("root")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element")
        .set_scrollbar_shadow_blur_radius_for_test(7.0);
    trees.sync(&arena, &[root]);
    assert!(trees.scrolls[&scroll_id].generation > geometry_generation);
    assert_eq!(
        trees.scrolls[&scroll_id]
            .scrollbar_overlay
            .shadow_blur_radius,
        7.0
    );
}

#[test]
fn nested_scroll_geometry_validator_binds_scroll_and_clip_parent_edges() {
    let mut arena = NodeArena::new();
    let (outer, inner, leaf) =
        make_nested_vertical_scroll_fixture(&mut arena, 12_200, 12_201, 12_202);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[outer]);
    assert!(trees.validation_errors.is_empty());

    let outer_scroll = trees.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
    let inner_scroll = trees.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
    let outer_clip_id = ClipNodeId {
        owner: outer,
        role: ClipNodeRole::ContentsClip,
    };
    let inner_clip_id = ClipNodeId {
        owner: inner,
        role: ClipNodeRole::ContentsClip,
    };
    let outer_clip = trees.clip_snapshot_for(Some(outer_clip_id)).unwrap()[0];
    let inner_clips = trees.clip_snapshot_for(Some(inner_clip_id)).unwrap();
    assert_eq!(inner_clips.len(), 2);
    let inner_clip = inner_clips[0];
    assert_eq!(inner_scroll.parent, Some(outer_scroll.id));
    assert_eq!(inner_clip.parent, Some(outer_clip.id));
    assert!(
        inner_scroll.has_canonical_nested_vertical_geometry_with_contents_clip(
            inner_clip,
            outer_scroll,
            outer_clip,
        )
    );
    assert!(
        !inner_scroll.has_canonical_vertical_geometry_with_contents_clip(inner_clip),
        "the parentless B0 validator must remain unchanged"
    );

    let outer_state = PropertyTreeState {
        clip: Some(outer_clip.id),
        scroll: Some(outer_scroll.id),
        ..Default::default()
    };
    let inner_state = PropertyTreeState {
        clip: Some(inner_clip.id),
        scroll: Some(inner_scroll.id),
        ..Default::default()
    };
    assert_eq!(trees.states[&outer].paint, PropertyTreeState::default());
    assert_eq!(trees.states[&outer].descendants, outer_state);
    assert_eq!(trees.states[&inner].paint, outer_state);
    assert_eq!(trees.states[&inner].descendants, inner_state);
    assert_eq!(trees.states[&leaf].paint, inner_state);

    let mut wrong_scroll_parent = inner_scroll;
    wrong_scroll_parent.parent = None;
    assert!(
        !wrong_scroll_parent.has_canonical_nested_vertical_geometry_with_contents_clip(
            inner_clip,
            outer_scroll,
            outer_clip,
        )
    );
    let mut wrong_clip_parent = inner_clip;
    wrong_clip_parent.parent = None;
    assert!(
        !inner_scroll.has_canonical_nested_vertical_geometry_with_contents_clip(
            wrong_clip_parent,
            outer_scroll,
            outer_clip,
        )
    );
    let mut non_root_parent = outer_scroll;
    non_root_parent.parent = Some(inner_scroll.id);
    assert!(
        !inner_scroll.has_canonical_nested_vertical_geometry_with_contents_clip(
            inner_clip,
            non_root_parent,
            outer_clip,
        )
    );
}

#[test]
fn scroll_contents_clip_is_owned_by_scroll_host_and_inherits_parent_clip() {
    let mut arena = NodeArena::new();
    let parent = insert_contents_clip_host(&mut arena, 20, Some([2, 3, 140, 120]));
    let (root, child) = make_vertical_scroll_fixture(&mut arena, 21, 22);
    append_child(&mut arena, parent, root);
    clear_layout_dirty_for_subtree(&arena, root);
    let parent_clip = ClipNodeId {
        owner: parent,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll_clip = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[parent]);

    assert_eq!(trees.states[&root].paint.clip, Some(parent_clip));
    assert_eq!(trees.states[&root].descendants.clip, Some(scroll_clip));
    assert_eq!(trees.states[&child].paint.clip, Some(scroll_clip));
    let clip = trees.clips[&scroll_clip];
    assert_eq!(clip.owner, root);
    assert_eq!(clip.parent, Some(parent_clip));
    assert_eq!(clip.behavior, ClipBehavior::Intersect);
    assert!(matches!(
        clip.geometry,
        ClipGeometry::LogicalScissor([10, 20, 100, 80])
    ));
}

#[test]
fn inactive_unsupported_and_invalid_scroll_observations_are_distinct_and_fail_closed() {
    let mut arena = NodeArena::new();
    let inactive = insert_element(&mut arena, 30);
    set_scroll_direction(&arena, inactive, ScrollDirection::Vertical);
    clear_layout_dirty_for_subtree(&arena, inactive);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[inactive]);
    assert!(!trees.scrolls.contains_key(&ScrollNodeId(inactive)));
    assert!(trees.validation_errors.is_empty());

    let missing = insert_missing_scroll_contract_host(&mut arena, 33);
    trees.sync(&arena, &[missing]);
    assert!(!trees.scrolls.contains_key(&ScrollNodeId(missing)));
    assert!(!trees.clips.contains_key(&ClipNodeId {
        owner: missing,
        role: ClipNodeRole::ContentsClip,
    }));
    assert!(trees.validation_errors.contains(
        &PropertyTreeValidationError::ScrollContractUnavailable(missing)
    ));

    let (unsupported, child) = make_vertical_scroll_fixture(&mut arena, 31, 32);
    arena
        .get_mut(child)
        .expect("child")
        .element
        .set_layout_width(120.0);
    arena.refresh_subtree_dirty_cache(unsupported);
    trees.sync(&arena, &[unsupported]);
    assert!(!trees.scrolls.contains_key(&ScrollNodeId(unsupported)));
    assert!(trees.validation_errors.contains(
        &PropertyTreeValidationError::ScrollContractUnavailable(unsupported)
    ));

    clear_layout_dirty_for_subtree(&arena, unsupported);
    arena
        .get_mut(unsupported)
        .expect("root")
        .element
        .set_scroll_offset((f32::NAN, 0.0));
    clear_layout_dirty_for_subtree(&arena, unsupported);
    trees.sync(&arena, &[unsupported]);
    assert!(!trees.scrolls.contains_key(&ScrollNodeId(unsupported)));
    assert!(trees.validation_errors.contains(
        &PropertyTreeValidationError::InvalidScrollGeometrySnapshot(unsupported)
    ));
    assert!(!trees.clips.contains_key(&ClipNodeId {
        owner: unsupported,
        role: ClipNodeRole::ContentsClip,
    }));
}

#[test]
fn scroll_generation_stays_monotonic_across_off_and_on_state() {
    let mut arena = NodeArena::new();
    let (root, _child) = make_vertical_scroll_fixture(&mut arena, 1, 2);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first_generation = trees.scrolls[&ScrollNodeId(root)].generation;

    set_scroll_direction(&arena, root, ScrollDirection::None);
    clear_layout_dirty_for_subtree(&arena, root);
    trees.sync(&arena, &[root]);
    assert!(!trees.scrolls.contains_key(&ScrollNodeId(root)));

    set_scroll_direction(&arena, root, ScrollDirection::Vertical);
    clear_layout_dirty_for_subtree(&arena, root);
    trees.sync(&arena, &[root]);
    assert!(trees.scrolls[&ScrollNodeId(root)].generation > first_generation);
}

#[test]
fn scroll_snapshot_equality_owns_exact_scrollbar_paint_state_and_sampled_alpha() {
    let mut arena = NodeArena::new();
    let (root, _child) = make_vertical_scroll_fixture(&mut arena, 31, 32);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let baseline = trees.scroll_snapshot_for(ScrollNodeId(root)).unwrap();

    let mut changed_state = baseline;
    changed_state.scrollbar_overlay.paint_state = ScrollbarPaintStateWitness::OpaqueNow;
    assert_ne!(baseline, changed_state);

    let mut changed_alpha = baseline;
    changed_alpha.scrollbar_overlay.sampled_alpha = 0.5;
    assert_ne!(baseline, changed_alpha);
}

#[test]
fn scroll_generation_survives_temporary_removal_from_active_roots() {
    let mut arena = NodeArena::new();
    let (root, _child) = make_vertical_scroll_fixture(&mut arena, 1, 2);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first_generation = trees.scrolls[&ScrollNodeId(root)].generation;

    trees.sync(&arena, &[]);
    assert!(!trees.scrolls.contains_key(&ScrollNodeId(root)));
    assert!(!trees.states.contains_key(&root));

    trees.sync(&arena, &[root]);
    assert!(trees.scrolls[&ScrollNodeId(root)].generation > first_generation);
}
