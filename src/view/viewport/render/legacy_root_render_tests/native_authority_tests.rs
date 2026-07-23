use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn public_native_hosts_close_retained_auto_metadata_and_full_recording() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (text_area_arena, text_area_roots, _) = prepared_auto_text_area(0.0, false);
    let fixtures: [(&str, (NodeArena, Vec<NodeKey>)); 5] = [
        ("Element", prepared_safe_leaf()),
        ("Text", prepared_native_text()),
        ("TextArea", (text_area_arena, text_area_roots)),
        ("Image", prepared_native_image()),
        ("Svg", prepared_native_svg()),
    ];

    for (host, (arena, roots)) in fixtures {
        assert_native_host_retained_closure(host, &arena, &roots, &ctx);
    }
}

#[test]
fn retained_auto_treats_empty_text_as_a_transparent_native_leaf() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_empty_native_text();
    assert_native_host_retained_closure("empty Text", &arena, &roots, &ctx);
}

#[test]
fn transparent_native_text_root_uses_host_generic_root_effect_artifact() {
    let (arena, roots, root) = prepared_transparent_native_text();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let record = |mode| {
        crate::view::paint::record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            mode,
            &properties,
            &generations,
        )
    };
    let metadata = record(crate::view::paint::CoverageRecordingMode::MetadataOnly);
    let full = record(crate::view::paint::CoverageRecordingMode::FullArtifact);
    assert!(matches!(
        metadata.items.as_slice(),
        [crate::view::paint::PaintCoverageItem::TransparentNode { owner, .. }]
            if *owner == root
    ));
    assert!(crate::view::paint::canonical_manifest_matches_for_test(
        &metadata, &full
    ));

    let selection_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let AutoAuthorityDecision::Artifact { candidate, trace } = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &selection_ctx,
        true,
    ) else {
        panic!("transparent native Text root must bypass Element-only effect geometry")
    };
    assert!(candidate.eligibility.eligible);
    assert!(trace.rejections.is_empty());
    assert!(matches!(
        candidate.artifact.target,
        crate::view::paint::PaintArtifactTarget::RootOpacityGroup { root: owner, .. }
            if owner == root
    ));

    let mut graph = FrameGraph::new();
    let mut compile_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = compile_ctx.allocate_target(&mut graph);
    compile_ctx.set_current_target(target);
    let key = crate::view::base_component::root_effect_stable_key(root);
    let desc = compile_ctx.persistent_full_viewport_target_desc(key);
    let root_effect_plan = RootEffectBuildPlan {
        committed: RootEffectRetainedState::Invalid,
        key,
        target: crate::view::paint::RootEffectRasterInputs {
            width: desc.width(),
            height: desc.height(),
            format: desc.format(),
            sample_count: desc.sample_count(),
            scale_factor_bits: compile_ctx.viewport().scale_factor().to_bits(),
        },
        pair_resident: false,
    };
    assert!(matches!(
        try_compile_recorded_artifact_frame(
            &mut graph,
            candidate,
            &compile_ctx,
            Some(&root_effect_plan),
        ),
        PropertyNeutralArtifactAttempt::Compiled { .. }
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn retained_auto_final_authority_covers_native_transform_effect_and_root_opacity() {
    let mut emitted_transform = false;
    for host in ["Image", "Svg"] {
        for state in ["ready", "loading", "error"] {
            let (arena, roots) = prepared_native_media_transform(host, state);
            let emit = !emitted_transform && host == "Image" && state == "ready";
            assert_native_property_scene_authority(
                &format!("direct {host} transform {state}"),
                &arena,
                &roots,
                emit,
            );
            emitted_transform |= emit;
        }
    }
    assert!(emitted_transform);

    let mut emitted_effect = false;
    for (host, states) in [
        ("Text", &["ready"][..]),
        ("Image", &["ready", "loading", "error"][..]),
        ("Svg", &["ready", "loading", "error"][..]),
    ] {
        for state in states {
            let (arena, roots, _child) = prepared_nested_native_effect(host, state);
            let emit = !emitted_effect && host == "Text";
            assert_native_property_scene_authority(
                &format!("nested {host} effect {state}"),
                &arena,
                &roots,
                emit,
            );
            emitted_effect |= emit;
        }
    }
    assert!(emitted_effect);

    let (arena, roots) = prepared_native_text_with_opacity(0.5);
    assert_native_root_opacity_artifact("root Text opacity", &arena, &roots, 0.5);
    let (arena, roots) = prepared_native_image_with_opacity(0.5);
    assert_native_root_opacity_artifact("root Image opacity", &arena, &roots, 0.5);
    let (arena, roots) = prepared_native_svg_with_opacity(0.5);
    assert_native_root_opacity_artifact("root Svg opacity", &arena, &roots, 0.5);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn retained_auto_native_property_malformed_snapshots_still_select_legacy() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);

    let (arena, roots) = prepared_native_media_transform("Image", "ready");
    let root = roots[0];
    let (mut properties, generations) = synced_paint_state(&arena, &roots);
    properties
        .transforms
        .get_mut(&crate::view::compositor::property_tree::TransformNodeId(
            root,
        ))
        .expect("direct Image transform snapshot")
        .generation = 0;
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));

    let (arena, roots, child) = prepared_nested_native_effect("Text", "ready");
    let (mut properties, generations) = synced_paint_state(&arena, &roots);
    properties
        .effects
        .get_mut(&crate::view::compositor::property_tree::EffectNodeId(child))
        .expect("nested Text effect snapshot")
        .opacity = f32::NAN;
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));

    let (arena, roots) = prepared_native_media_transform("Image", "ready");
    let root = roots[0];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    arena
        .get_mut(root)
        .expect("ready Image")
        .element
        .as_any_mut()
        .downcast_mut::<Image>()
        .expect("Image host")
        .set_source(ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([9_u8, 8, 7, 255]),
        });
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));

    let (mut arena, roots, child) = prepared_nested_native_effect("Svg", "loading");
    let (properties, generations) = synced_paint_state(&arena, &roots);
    arena.set_parent(child, None);
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));
}

#[test]
fn native_text_image_svg_root_opacity_selects_and_compiles_retained_auto() {
    for opacity in [0.0, 0.5, 1.0] {
        let (arena, roots) = prepared_native_text_with_opacity(opacity);
        assert_native_root_opacity_artifact("Text", &arena, &roots, opacity);

        let (arena, roots) = prepared_native_image_with_opacity(opacity);
        assert_native_root_opacity_artifact("Image ready", &arena, &roots, opacity);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (arena, roots) = prepared_native_svg_with_opacity(opacity);
            assert_native_root_opacity_artifact("Svg ready", &arena, &roots, opacity);
        }
    }
}

#[test]
fn native_image_svg_loading_error_root_opacity_stays_retained_auto() {
    for error in [false, true] {
        let state = if error { "error" } else { "loading" };
        let (arena, roots) =
            prepared_native_image_path_state(&format!("image-{state}"), 0.5, error);
        assert_native_root_opacity_artifact(&format!("Image {state}"), &arena, &roots, 0.5);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (arena, roots) =
                prepared_native_svg_path_state(&format!("svg-{state}"), 0.5, error);
            assert_native_root_opacity_artifact(&format!("Svg {state}"), &arena, &roots, 0.5);
        }
    }
}

#[test]
fn native_root_opacity_contract_rejects_property_resource_and_topology_drift() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_native_text_with_opacity(0.5);
    let root = roots[0];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let effect = crate::view::compositor::property_tree::EffectNodeId(root);
    let AutoAuthorityDecision::Artifact { candidate, .. } =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true)
    else {
        panic!("baseline native root opacity must select artifact")
    };
    let compile_tampered = |candidate: RecordedArtifactCandidate| {
        let mut graph = FrameGraph::new();
        let mut compile_ctx =
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = compile_ctx.allocate_target(&mut graph);
        compile_ctx.set_current_target(target);
        let key = crate::view::base_component::root_effect_stable_key(root);
        let desc = compile_ctx.persistent_full_viewport_target_desc(key);
        let plan = RootEffectBuildPlan {
            committed: RootEffectRetainedState::Invalid,
            key,
            target: crate::view::paint::RootEffectRasterInputs {
                width: desc.width(),
                height: desc.height(),
                format: desc.format(),
                sample_count: desc.sample_count(),
                scale_factor_bits: compile_ctx.viewport().scale_factor().to_bits(),
            },
            pair_resident: false,
        };
        try_compile_recorded_artifact_frame(&mut graph, candidate, &compile_ctx, Some(&plan))
    };
    let mut generation_tamper = candidate.clone();
    generation_tamper
        .artifact
        .effect_nodes
        .iter_mut()
        .find(|snapshot| snapshot.id == effect)
        .unwrap()
        .generation = 0;
    assert!(matches!(
        compile_tampered(generation_tamper),
        PropertyNeutralArtifactAttempt::CompileRejected(
            crate::view::paint::ArtifactCompileErrorKind::InvalidStore
        )
    ));
    let mut opacity_tamper = candidate;
    opacity_tamper
        .artifact
        .effect_nodes
        .iter_mut()
        .find(|snapshot| snapshot.id == effect)
        .unwrap()
        .opacity = f32::NAN;
    assert!(matches!(
        compile_tampered(opacity_tamper),
        PropertyNeutralArtifactAttempt::CompileRejected(
            crate::view::paint::ArtifactCompileErrorKind::InvalidStore
        )
    ));

    let (mut properties, generations) = synced_paint_state(&arena, &roots);
    properties.effects.get_mut(&effect).unwrap().generation = 0;
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));

    let (mut properties, generations) = synced_paint_state(&arena, &roots);
    properties.effects.get_mut(&effect).unwrap().opacity = f32::NAN;
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));

    let (arena, roots) = prepared_native_image_with_opacity(0.5);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    arena
        .get_mut(roots[0])
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Image>()
        .unwrap()
        .set_source(ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([9_u8, 8, 7, 255]),
        });
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));

    let (mut arena, roots) = prepared_native_text_with_opacity(0.5);
    let unrelated = commit_element(
        &mut arena,
        Box::new(colored_element(0xd3_a012, 0.0, Color::rgb(1, 2, 3))),
    );
    arena.set_arena_children_without_mirror_for_test(roots[0], vec![unrelated]);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));
}

#[test]
fn transparent_deferred_element_root_uses_property_scene_and_tamper_fails_closed() {
    let mut element = colored_element(0x6d2b, 10.0, Color::rgb(40, 80, 160));
    let mut style = Style::new();
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(0.0)),
    );
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(4.0))
                .top(Length::px(5.0))
                .clip(ClipMode::Viewport),
        ),
    );
    element.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let unrelated = commit_element(
        &mut arena,
        Box::new(colored_element(0x6d2c, 80.0, Color::rgb(20, 180, 40))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let roots = [root];
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (mut properties, generations) = synced_paint_state(&arena, &roots);
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    if !matches!(decision, AutoAuthorityDecision::PropertyScene { .. }) {
        panic!(
            "expected property scene, got {:?}: {:?}",
            auto_authority_kind(&decision),
            auto_authority_trace(&decision).rejections
        );
    }

    properties
        .clips
        .values_mut()
        .next()
        .expect("deferred root owns one clip witness")
        .owner = unrelated;
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));
}

#[test]
fn retained_auto_selects_one_exact_authority_by_property_topology() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();

    let (neutral_arena, neutral_roots) = prepared_safe_leaf();
    assert!(matches!(
        auto_decision(&neutral_arena, &neutral_roots, &ctx),
        AutoAuthorityDecision::Artifact { .. }
    ));

    let (clip_arena, clip_roots) = prepared_contents_clipped_leaf();
    assert!(matches!(
        auto_decision(&clip_arena, &clip_roots, &ctx),
        AutoAuthorityDecision::Artifact { .. }
    ));

    let (transform_arena, transform_roots) = prepared_transform_leaf();
    assert!(matches!(
        auto_decision(&transform_arena, &transform_roots, &ctx),
        AutoAuthorityDecision::PropertyScene { .. }
    ));

    let (tree_arena, tree_roots, _) = prepared_nested_transform_tree();
    assert!(matches!(
        auto_decision(&tree_arena, &tree_roots, &ctx),
        AutoAuthorityDecision::PropertyScene { .. }
    ));

    let (general_arena, general_roots) = prepared_general_transform_scene();
    match auto_decision(&general_arena, &general_roots, &ctx) {
        AutoAuthorityDecision::PropertyScene { .. } => {}
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "general transform scene rejected: {:?}",
            trace
                .rejections
                .iter()
                .map(AutoAuthorityRejection::debug_label)
                .collect::<Vec<_>>()
        ),
        _ => panic!("general transform scene selected the wrong authority"),
    }

    let (effect_tree_arena, effect_tree_roots, _, _, _) =
        prepared_transform_child_isolation_tree();
    assert!(matches!(
        auto_decision(&effect_tree_arena, &effect_tree_roots, &ctx),
        AutoAuthorityDecision::PropertyScene { .. }
    ));

    let (isolation_arena, isolation_roots) = prepared_safe_leaf();
    crate::view::test_support::get_element_mut::<Element>(&isolation_arena, isolation_roots[0])
        .set_opacity(0.5);
    assert!(matches!(
        auto_decision(&isolation_arena, &isolation_roots, &ctx),
        AutoAuthorityDecision::PropertyScene { .. }
    ));

    assert_eq!(
        graph.build_state_snapshot_for_test(),
        graph_before,
        "automatic selection cannot mutate the frame graph"
    );
}
