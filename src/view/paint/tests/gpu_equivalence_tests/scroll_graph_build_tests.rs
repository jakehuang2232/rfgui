use super::*;

#[test]
fn scroll_scene_gpu_gradient_fixtures_build_without_adapter() -> Result<(), String> {
    for case in [
        ScrollSceneGpuCase {
            name: "single-offset-fractional",
            offset_y: 47.25,
            content_height: 300.0,
            backing: ScrollSceneBackingKind::Single,
            max_dimension_2d: 8192,
            transition_local_y: 67.25,
        },
        ScrollSceneGpuCase {
            name: "tiled-cross-seam-fractional",
            offset_y: 1000.25,
            content_height: 3000.0,
            backing: ScrollSceneBackingKind::Tiled,
            max_dimension_2d: 2048,
            transition_local_y: 1024.0,
        },
    ] {
        for scrollbar in GpuScrollbarCase::ALL {
            let mut viewport = Viewport::new();
            let (retained, trace) = retained_scroll_scene_graph(&mut viewport, case, scrollbar)?;
            if trace.backing != case.backing
                || trace.action != RetainedSurfaceCompileAction::Reraster
            {
                return Err(format!(
                    "{}/{scrollbar:?}: CPU fixture selected the wrong first-frame authority: {trace:?}",
                    case.name
                ));
            }
            if retained.pass_descriptors().is_empty()
                || legacy_scroll_scene_graph(case, scrollbar)?
                    .pass_descriptors()
                    .is_empty()
            {
                return Err(format!(
                    "{}/{scrollbar:?}: CPU fixture emitted an empty graph",
                    case.name
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn production_multi_root_scroll_forest_builds_without_adapter() -> Result<(), String> {
    let mut viewport = Viewport::new();
    let semantic_frame_time = crate::time::Instant::now();
    let (graph, trace, owner, residents) = production_scroll_forest_graph(
        &mut viewport,
        ScrollForestContentVersion::Baseline,
        semantic_frame_time,
    )?;
    validate_scroll_forest_graph_shape(&graph, &trace, trace.tile_count, 0)?;
    if residents.len() != trace.tile_count {
        return Err(format!(
            "cold scroll-forest resident count differs from tile count: residents={}, trace={trace:?}",
            residents.len()
        ));
    }
    if viewport.retained_surface_transaction_shape_for_test() != (0, Some(trace.tile_count)) {
        return Err(format!(
            "cold scroll-forest did not stage one joint transaction: {:?}",
            viewport.retained_surface_transaction_shape_for_test()
        ));
    }
    if residents
        .iter()
        .any(|(key, desc)| viewport.has_compatible_persistent_render_target_pair(*key, desc))
    {
        return Err(
            "nonadapter scroll-forest unexpectedly established physical residency".to_string(),
        );
    }
    if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), false)
        || viewport.retained_surface_transaction_shape_for_test() != (0, None)
    {
        return Err("nonadapter scroll-forest transaction did not roll back exactly".to_string());
    }
    if legacy_scroll_forest_graph(ScrollForestContentVersion::Baseline)?
        .pass_descriptors()
        .is_empty()
    {
        return Err("legacy scroll-forest fixture emitted an empty graph".to_string());
    }
    Ok(())
}

#[test]
fn focused_atomic_projection_scroll_forest_builds_without_adapter() -> Result<(), String> {
    for (case, caret_visible, preedit) in [
        ("caret-visible", true, None),
        ("caret-hidden", false, None),
        (
            "preedit-caret-visible",
            true,
            Some(("中", Some((0, "中".len())))),
        ),
    ] {
        let mut viewport = Viewport::new();
        let (graph, trace, owner, residents) =
            retained_focused_atomic_projection_scroll_graph(&mut viewport, caret_visible, preedit)?;
        if trace.root_count != 1
            || trace.scroll_group_count != 1
            || trace.backing != ScrollSceneBackingKind::Single
            || trace.tile_count != 1
            || trace.reraster_count != 1
            || trace.reuse_count != 0
            || residents.len() != 1
        {
            return Err(format!(
                "focused atomic projection CPU graph selected the wrong cold authority for {case}: trace={trace:?}, residents={residents:?}"
            ));
        }
        if graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len()
            != 1
        {
            return Err(format!(
                "focused atomic projection CPU graph must composite exactly one retained content target: {case}"
            ));
        }
        if viewport.retained_surface_transaction_shape_for_test() != (0, Some(1)) {
            return Err(format!(
                "focused atomic projection CPU graph did not stage one scroll transaction for {case}: {:?}",
                viewport.retained_surface_transaction_shape_for_test()
            ));
        }
        if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), false) {
            return Err(format!(
                "focused atomic projection CPU graph did not roll back its transaction for {case}"
            ));
        }
        if legacy_focused_atomic_projection_scroll_graph(caret_visible, preedit)?
            .pass_descriptors()
            .is_empty()
        {
            return Err(format!(
                "legacy focused atomic projection graph emitted no passes for {case}"
            ));
        }
    }
    Ok(())
}

#[test]
fn production_nested_scroll_graph_builds_without_adapter() -> Result<(), String> {
    let mut viewport = Viewport::new();
    let (graph, trace, owner, leaf_key, leaf_desc) =
        production_nested_scroll_graph(&mut viewport, 13.0, 9.0, None)?;
    let legacy = legacy_nested_scroll_graph(13.0, 9.0, None)?;
    assert_eq!(trace.reraster_count, 1);
    assert_eq!(trace.reuse_count, 0);
    assert!(!viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc));
    let clears = graph.test_graphics_passes::<crate::view::frame_graph::ClearPass>();
    assert_eq!(clears.len(), 3, "root + transient A0 + cold R1");
    assert_eq!(
        clears[0].test_snapshot().color_bits,
        [0.0_f32.to_bits(); 4],
        "production nested root clear matches the legacy prelude"
    );
    assert_eq!(
        legacy.test_graphics_passes::<crate::view::frame_graph::ClearPass>()[0]
            .test_snapshot()
            .color_bits,
        clears[0].test_snapshot().color_bits,
        "legacy and production use the same transparent root clear"
    );
    let composites = graph.test_graphics_passes::<
        crate::view::render_pass::texture_composite_pass::TextureCompositePass,
    >();
    assert_eq!(composites.len(), 2, "R1 -> A0 -> root");
    assert!(
        composites
            .iter()
            .all(|pass| pass.test_snapshot().effective_scissor_rect.is_some()),
        "the exact fixture's internal C0/C1 clips remain active without an external scissor"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
    Ok(())
}

#[test]
fn production_nested_scroll_image_svg_text_graphs_build_without_adapter() -> Result<(), String> {
    let outer_offset_y = 2.0;
    let inner_offset_y = 3.0;
    for kind in NestedScrollGpuLeafKind::GPU_CLOSURE {
        let mut viewport = Viewport::new();
        let (graph, trace, owner, leaf_key, leaf_desc) = production_nested_scroll_leaf_graph(
            &mut viewport,
            kind,
            outer_offset_y,
            inner_offset_y,
            None,
        )?;
        if trace.reraster_count != 1 || trace.reuse_count != 0 {
            return Err(format!(
                "cold nested-scroll {} graph did not select R: {trace:?}",
                kind.label()
            ));
        }
        if viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
            return Err(format!(
                "fresh nested-scroll {} graph unexpectedly has a resident R1",
                kind.label()
            ));
        }
        validate_nested_scroll_leaf_graph_shape(&graph, kind, true)?;
        let legacy = legacy_nested_scroll_leaf_graph(kind, outer_offset_y, inner_offset_y, None)?;
        validate_nested_scroll_legacy_leaf_graph_shape(&legacy, kind)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), false) {
            return Err(format!(
                "nested-scroll {} graph-build transaction did not roll back",
                kind.label()
            ));
        }
    }
    Ok(())
}

#[test]
fn production_direct_scroll_transform_graphs_build_without_adapter() -> Result<(), String> {
    for case in DirectScrollTransformGpuCase::GRAPH_BUILD_CASES {
        let mut viewport = Viewport::new();
        let (graph, trace, owner, resident, _) =
            production_direct_scroll_transform_graph(&mut viewport, case)?;
        validate_direct_scroll_transform_graph_shape(&graph, trace, true, case.label)?;
        if viewport.has_compatible_persistent_render_target_pair(resident.0, &resident.1) {
            return Err(format!(
                "fresh direct S->T {} viewport unexpectedly has physical residency",
                case.label
            ));
        }
        if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), false) {
            return Err(format!(
                "direct S->T {} graph-build transaction did not roll back",
                case.label
            ));
        }
    }
    Ok(())
}

#[test]
fn production_transform_and_effect_scroll_graphs_build_without_adapter() -> Result<(), String> {
    let sampled_at = crate::time::Instant::now();
    for grammar in [
        DirectPropertyScrollGpuGrammar::Transform {
            translation: [7.0, 5.0],
        },
        DirectPropertyScrollGpuGrammar::Effect { opacity: 0.625 },
    ] {
        let mut viewport = Viewport::new();
        let (graph, trace, owner, residents) =
            production_direct_property_scroll_graph(&mut viewport, grammar, sampled_at)?;
        if trace.reraster_count != 2 || trace.reuse_count != 0 {
            return Err(format!(
                "cold {} graph did not naturally select R/R: {trace:?}",
                grammar.label()
            ));
        }
        validate_direct_property_scroll_graph_shape(&graph, grammar, true)?;
        if residents
            .iter()
            .any(|(key, desc)| viewport.has_compatible_persistent_render_target_pair(*key, desc))
        {
            return Err(format!(
                "fresh {} viewport unexpectedly has physical residency",
                grammar.label()
            ));
        }
        if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), false) {
            return Err(format!(
                "{} graph-build transaction did not roll back",
                grammar.label()
            ));
        }
    }

    // A warm receiver-reuse graph declares only the receiver pair. Detached
    // content stays physically resident in the viewport pool and is not
    // redundantly declared by the graph, so the collector must accept one
    // complete pair instead of assuming every frame declares both pairs.
    let mut one_pair_graph = FrameGraph::new();
    let color_key = crate::view::frame_graph::PersistentTextureKey::retained(
        crate::view::frame_graph::RetainedTextureRole::TransformedColor,
        0xb4_2fff,
    );
    let depth_key = color_key
        .depth_stencil()
        .expect("synthetic warm receiver key has a depth pair");
    let color_desc =
        crate::view::frame_graph::TextureDesc::new(8, 8, FORMAT, wgpu::TextureDimension::D2);
    let depth_desc = crate::view::frame_graph::TextureDesc::new(
        8,
        8,
        wgpu::TextureFormat::Depth24PlusStencil8,
        wgpu::TextureDimension::D2,
    );
    let _ = one_pair_graph.declare_persistent_texture_internal::<()>(color_desc.clone(), color_key);
    let _ = one_pair_graph.declare_persistent_texture_internal::<()>(depth_desc, depth_key);
    assert_eq!(
        direct_property_scroll_residents(&one_pair_graph)?,
        vec![(color_key, color_desc)]
    );
    Ok(())
}

#[test]
fn production_transform_effect_scroll_graph_builds_without_adapter() -> Result<(), String> {
    let frame = TransformEffectScrollGpuFrame {
        translation: [7.0, 3.0],
    };
    let mut viewport = Viewport::new();
    let (graph, trace, owner, residents) = production_transform_effect_scroll_graph(
        &mut viewport,
        frame,
        crate::time::Instant::now(),
    )?;
    if trace.reraster_count != 3 || trace.reuse_count != 0 {
        return Err(format!(
            "cold T->E->S graph did not naturally select R/R/R: {trace:?}"
        ));
    }
    if !transform_effect_scroll_resident_roles_are_exact(&residents, true) {
        return Err(format!(
            "cold T->E->S graph must declare exactly one T, E, and S content pair: {residents:?}"
        ));
    }
    validate_transform_effect_scroll_graph_shape(&graph, true)?;
    if residents
        .iter()
        .any(|(key, desc)| viewport.has_compatible_persistent_render_target_pair(*key, desc))
    {
        return Err("fresh T->E->S viewport unexpectedly has physical residency".to_string());
    }
    if !viewport.finish_retained_surface_transaction_for_frame(Some(owner), false) {
        return Err("T->E->S graph-build transaction did not roll back".to_string());
    }

    // A U/U/U frame declares only T and E. Exercise the dynamic collector
    // with two complete pairs without pretending a non-GPU viewport has
    // physical residency or seeding it through a forced test-pool path.
    let mut warm_shape = FrameGraph::new();
    for (role, stable_id) in [
        (
            crate::view::frame_graph::RetainedTextureRole::TransformedColor,
            0xb4_3ff1,
        ),
        (
            crate::view::frame_graph::RetainedTextureRole::IsolationColor,
            0xb4_3ff2,
        ),
    ] {
        let color_key = crate::view::frame_graph::PersistentTextureKey::retained(role, stable_id);
        let depth_key = color_key
            .depth_stencil()
            .expect("synthetic T/E color key has a depth pair");
        let _ = warm_shape.declare_persistent_texture_internal::<()>(
            crate::view::frame_graph::TextureDesc::new(8, 8, FORMAT, wgpu::TextureDimension::D2),
            color_key,
        );
        let _ = warm_shape.declare_persistent_texture_internal::<()>(
            crate::view::frame_graph::TextureDesc::new(
                8,
                8,
                wgpu::TextureFormat::Depth24PlusStencil8,
                wgpu::TextureDimension::D2,
            ),
            depth_key,
        );
    }
    let warm_residents = direct_property_scroll_residents(&warm_shape)?;
    assert!(
        transform_effect_scroll_resident_roles_are_exact(&warm_residents, false),
        "warm T->E->S declarations must contain exactly one T and E pair: {warm_residents:?}"
    );
    Ok(())
}
