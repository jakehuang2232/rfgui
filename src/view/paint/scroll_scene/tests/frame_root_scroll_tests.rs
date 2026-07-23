use super::*;

#[test]
fn frame_root_scroll_scene_dpr2_freezes_device_descriptors_and_emits() {
    let (arena, roots) = crate::view::paint::tests::window_like_native_showcase_fixture();
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let scene = plan_and_validate_frame_root_scroll_scene(
        &arena,
        &roots,
        &properties,
        &generations,
        2.0,
        [0.0; 2],
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    )
    .expect("DPR2 native frame-root scroll scene");
    assert!(scene.is_canonical());
    assert!(scene.scroll_host_phase_order_and_store_tampering_are_sealed_for_test());

    let mut viewport = Viewport::new();
    let frame_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("retained frame stage");
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(1600, 1200, wgpu::TextureFormat::Bgra8Unorm, 2.0);
    let prepared = prepare_frame_root_scroll_scene(
        &mut viewport,
        scene,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    )
    .expect("DPR2 descriptors and stamps remain canonical");
    let outcome = emit_prepared_frame_root_scroll_scene(prepared);
    let (_, trace) = outcome.into_parts();
    assert_eq!(trace.scroll_group_count, 1);
    assert!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len()
            >= 1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true,));
}

#[test]
fn frame_root_rounded_scrollbar_overlay_freezes_vertical_horizontal_and_both_axes() {
    fn rect_bits(rect: crate::view::base_component::Rect) -> [u32; 4] {
        [
            rect.x.to_bits(),
            rect.y.to_bits(),
            rect.width.to_bits(),
            rect.height.to_bits(),
        ]
    }

    for direction in [
        ScrollDirection::Vertical,
        ScrollDirection::Horizontal,
        ScrollDirection::Both,
    ] {
        let (arena, roots) = crate::view::paint::tests::window_like_native_showcase_fixture();
        let root = roots[0];
        let scroll = arena.children_of(root)[0];
        {
            let mut host =
                crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
            host.set_scroll_direction_for_retained_test(direction);
            host.set_sampled_scrollbar_alpha_for_test(0.75);
        }
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let snapshot = properties
            .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(scroll))
            .expect("native scroll snapshot");
        let overlay = snapshot.scrollbar_overlay;
        assert_eq!(
            snapshot.configured_axis,
            match direction {
                ScrollDirection::Vertical => {
                    crate::view::base_component::ScrollAxisSnapshot::Vertical
                }
                ScrollDirection::Horizontal => {
                    crate::view::base_component::ScrollAxisSnapshot::Horizontal
                }
                ScrollDirection::Both => crate::view::base_component::ScrollAxisSnapshot::Both,
                ScrollDirection::None => unreachable!("test matrix excludes non-scroll hosts"),
            }
        );
        assert_eq!(
            overlay.paint_state,
            ScrollbarPaintStateWitness::TranslucentNow
        );
        let mut expected_axes = Vec::new();
        if let Some((track, thumb)) = overlay.vertical_track.zip(overlay.vertical_thumb) {
            expected_axes.push((rect_bits(track), rect_bits(thumb)));
        }
        if let Some((track, thumb)) = overlay.horizontal_track.zip(overlay.horizontal_thumb) {
            expected_axes.push((rect_bits(track), rect_bits(thumb)));
        }
        assert_eq!(
            expected_axes.len(),
            match direction {
                ScrollDirection::Both => 2,
                ScrollDirection::Vertical | ScrollDirection::Horizontal => 1,
                ScrollDirection::None => unreachable!("test matrix excludes non-scroll hosts"),
            }
        );

        let scene = plan_and_validate_frame_root_scroll_scene(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            wgpu::TextureFormat::Bgra8Unorm,
        )
        .expect("painted rounded native scroll scene");
        assert!(scene.is_canonical());
        assert_eq!(
            scene.scrollbar_overlay_axis_geometry_for_test(),
            vec![expected_axes.clone()]
        );
        assert!(scene.scrollbar_overlay_tampering_is_rejected_for_test());

        let mut viewport = Viewport::new();
        let frame_owner = viewport
            .begin_retained_surface_frame_stage()
            .expect("retained frame stage");
        let mut graph = FrameGraph::new();
        let ctx = UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let prepared = prepare_frame_root_scroll_scene(
            &mut viewport,
            scene,
            &mut graph,
            ctx,
            [0.0, 0.0, 0.0, 1.0],
            frame_owner,
        )
        .expect("painted rounded native scroll descriptors");
        let _ = emit_prepared_frame_root_scroll_scene(prepared);

        let track_color = [0.95, 0.95, 0.95, 0.35 * 0.75].map(f32::to_bits);
        let thumb_color = [0.95, 0.95, 0.95, 0.58 * 0.75].map(f32::to_bits);
        let painted = graph
            .test_rect_pass_snapshots()
            .into_iter()
            .filter(|rect| {
                rect.fill_color_bits == track_color || rect.fill_color_bits == thumb_color
            })
            .map(|rect| {
                (
                    [
                        rect.position_bits[0],
                        rect.position_bits[1],
                        rect.size_bits[0],
                        rect.size_bits[1],
                    ],
                    rect.fill_color_bits,
                )
            })
            .collect::<Vec<_>>();
        let expected_painted = expected_axes
            .into_iter()
            .flat_map(|(track, thumb)| [(track, track_color), (thumb, thumb_color)])
            .collect::<Vec<_>>();
        assert_eq!(painted, expected_painted, "{direction:?}");
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true,)
        );
    }
}

#[test]
fn sampled_layout_transition_frame_root_scroll_is_retained_for_all_axes_and_dpr() {
    for direction in [
        ScrollDirection::Vertical,
        ScrollDirection::Horizontal,
        ScrollDirection::Both,
    ] {
        for scale_factor in [1.0, 2.0] {
            let (arena, roots, scroll) = sampled_window_scroll_fixture(direction, 400.0);
            let sampled = arena
                .get(scroll)
                .unwrap()
                .element
                .retained_sampled_layout_transition_snapshot()
                .expect("clean sampled layout-transition witness");
            assert_eq!(sampled.bounds_bits[2], 400.0_f32.to_bits());
            let mut properties = PropertyTrees::default();
            properties.sync(&arena, &roots);
            assert!(
                properties.validation_errors.is_empty(),
                "sampled {direction:?} DPR{scale_factor}: {:?}",
                properties.validation_errors
            );
            let scroll_snapshot = properties
                .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(
                    scroll,
                ))
                .expect("sampled scroll property snapshot");
            assert_eq!(
                scroll_snapshot.viewport.width.to_bits(),
                400.0_f32.to_bits()
            );
            assert_eq!(
                scroll_snapshot
                    .layout_content_bounds_at_zero
                    .width
                    .to_bits(),
                760.0_f32.to_bits()
            );
            let mut generations = PaintGenerationTracker::default();
            generations.sync(&arena, &roots, &properties);
            let scene = plan_and_validate_frame_root_scroll_scene(
                &arena,
                &roots,
                &properties,
                &generations,
                scale_factor,
                [0.0; 2],
                None,
                wgpu::TextureFormat::Bgra8Unorm,
            )
            .unwrap_or_else(|error| {
                panic!("sampled {direction:?} DPR{scale_factor}: {error:?}")
            });
            assert!(scene.is_canonical());
            assert!(scene.scroll_host_phase_order_and_store_tampering_are_sealed_for_test());
            assert_eq!(
                scene.scrollbar_overlay_axis_geometry_for_test()[0].len(),
                usize::from(direction == ScrollDirection::Both) + 1
            );

            let mut viewport = Viewport::new();
            let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut graph = FrameGraph::new();
            let ctx = UiBuildContext::new(
                (800.0 * scale_factor) as u32,
                (600.0 * scale_factor) as u32,
                wgpu::TextureFormat::Bgra8Unorm,
                scale_factor,
            );
            let prepared = prepare_frame_root_scroll_scene(
                &mut viewport,
                scene,
                &mut graph,
                ctx,
                [0.0, 0.0, 0.0, 1.0],
                frame_owner,
            )
            .expect("sampled scroll descriptors/stamps remain exact");
            let _ = emit_prepared_frame_root_scroll_scene(prepared);
            assert!(!graph.pass_descriptors().is_empty());
            assert!(!graph.test_rect_pass_snapshots().is_empty());
            assert!(
                viewport
                    .finish_retained_surface_transaction_for_frame(Some(frame_owner), true,)
            );
        }
    }
}

#[test]
fn sampled_frame_root_scroll_stale_uninstalled_nonfinite_and_revision_drift_fail_closed() {
    let assert_layout_transition_rejection =
        |arena: &NodeArena, roots: &[NodeKey], scroll: NodeKey| {
            let mut properties = PropertyTrees::default();
            properties.sync(arena, roots);
            let mut generations = PaintGenerationTracker::default();
            generations.sync(arena, roots, &properties);
            let error = super::super::super::frame_plan::
                plan_property_scroll_interleave_scaffold_with_context(
                    arena,
                    roots,
                    &properties,
                    &generations,
                    super::super::super::TransformSurfacePlanContext::default(),
                )
                .expect_err("non-exact sampled scroll must fail closed");
            assert!(
                error
                    .reasons
                    .contains(&FramePaintPlanRejection::LayoutTransition(scroll)),
                "{error:?}"
            );
        };

    // A newly changed transition sample is stale until layout installs it.
    {
        let (arena, roots, scroll) =
            sampled_window_scroll_fixture(ScrollDirection::Vertical, 400.0);
        crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
            .set_layout_transition_width(392.0);
        assert!(
            arena
                .get(scroll)
                .unwrap()
                .element
                .retained_sampled_layout_transition_snapshot()
                .is_none()
        );
        assert_layout_transition_rejection(&arena, &roots, scroll);
    }

    // Even maliciously clearing dirty flags cannot turn non-finite runtime
    // transition state into an exact witness.
    {
        let (arena, roots, scroll) =
            sampled_window_scroll_fixture(ScrollDirection::Horizontal, 400.0);
        crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
            .set_layout_transition_x(f32::NAN);
        arena
            .get_mut(scroll)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
        arena.clear_arena_dirty_subtree(roots[0], DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(roots[0]);
        assert!(
            arena
                .get(scroll)
                .unwrap()
                .element
                .retained_sampled_layout_transition_snapshot()
                .is_none()
        );
        assert_layout_transition_rejection(&arena, &roots, scroll);
    }

    // An installed new sample cannot be planned against stale property or
    // paint-generation revisions.
    {
        let (mut arena, roots, scroll) =
            sampled_window_scroll_fixture(ScrollDirection::Both, 400.0);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let before = arena
            .get(scroll)
            .unwrap()
            .element
            .retained_sampled_layout_transition_snapshot()
            .unwrap();
        crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
            .set_layout_transition_width(392.0);
        let (constraints, placement) = window_layout_inputs();
        measure_and_place(&mut arena, roots[0], constraints, placement);
        {
            let mut host =
                crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
            host.layout_state.content_size = Size {
                width: 760.0,
                height: 520.0,
            };
            host.set_scroll_offset((18.0, 22.0));
            host.set_sampled_scrollbar_alpha_for_test(0.75);
        }
        let mut pending = roots.clone();
        while let Some(owner) = pending.pop() {
            pending.extend(arena.children_of(owner));
            arena
                .get_mut(owner)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        for root in &roots {
            arena.clear_arena_dirty_subtree(*root, DirtyFlags::ALL);
            arena.refresh_subtree_dirty_cache(*root);
        }
        let after = arena
            .get(scroll)
            .unwrap()
            .element
            .retained_sampled_layout_transition_snapshot()
            .expect("new clean sample is installed");
        assert_ne!(before, after);
        assert!(
            plan_and_validate_frame_root_scroll_scene(
                &arena,
                &roots,
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
                wgpu::TextureFormat::Bgra8Unorm,
            )
            .is_err()
        );
    }

    // Scroll offset belongs to the same exact property/revision snapshot.
    {
        let (arena, roots, scroll) =
            sampled_window_scroll_fixture(ScrollDirection::Both, 400.0);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        crate::view::test_support::get_element_mut::<Element>(&arena, scroll)
            .set_scroll_offset((19.0, 22.0));
        assert!(
            plan_and_validate_frame_root_scroll_scene(
                &arena,
                &roots,
                &properties,
                &generations,
                2.0,
                [0.0; 2],
                None,
                wgpu::TextureFormat::Bgra8Unorm,
            )
            .is_err()
        );
    }
}
