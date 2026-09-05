use super::*;
use crate::view::paint::{
    ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceRasterContext,
    FrameArtifactRecordOutcome, RendererMode, RetainedSurfaceRasterRole,
    SurfaceMaterializationOutcome, prepare_artifact_surface_raster_plan,
    record_surface_dag_frame_artifact, seal_prepared_artifact_surface_frame,
};

const MATERIALIZATION_BUDGET_BYTES: u64 = 128 * 1024 * 1024;

fn recorded_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    dpr: f32,
) -> crate::view::paint::PreparedArtifactSurfaceRasterPlan {
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        arena,
        roots,
        properties,
        generations,
        RendererMode::ForcedForTests,
    )
    .expect("materialization artifact must record") else {
        panic!("materialization artifact must not fall back")
    };
    prepare_artifact_surface_raster_plan(
        artifact,
        ArtifactSurfaceRasterContext::new(
            dpr,
            wgpu::TextureFormat::Bgra8Unorm,
            [0.0, 0.0],
            None,
            4096,
            MATERIALIZATION_BUDGET_BYTES,
        )
        .expect("materialization raster context"),
    )
    .expect("materialization raster plan")
}

fn direct_scroll_transform_artifact(
    scroll_offset_y: f32,
    translation: [f32; 2],
    host_has_background: bool,
) -> crate::view::paint::PaintArtifact {
    let mut root = Element::new_with_id(0xb4_7a01, 0.0, 0.0, 100.0, 80.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(root_style);
    root.layout_state.content_size = Size {
        width: 100.0,
        height: 300.0,
    };
    root.set_scroll_offset((0.0, scroll_offset_y));
    if host_has_background {
        root.set_background_color_value(Color::rgb(17, 31, 47));
    }
    root.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));

    let mut content = Element::new_with_id(0xb4_7a02, 0.0, -scroll_offset_y, 100.0, 300.0);
    content.set_background_color_value(Color::rgb(24, 48, 72));
    content.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
        translation[0],
        translation[1],
        0.0,
    ))));
    content.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));

    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(root)));
    let content = arena.insert(Node::new(Box::new(content)));
    arena.set_parent(content, Some(root));
    arena.push_child(root, content);
    arena.refresh_subtree_dirty_cache(root);
    let roots = [root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("direct scroll/transform artifact must record") else {
        panic!("direct scroll/transform artifact must not fall back")
    };
    artifact
}

fn scroll_effect_artifact() -> crate::view::paint::PaintArtifact {
    let mut root = Element::new_with_id(0xb4_7b01, 0.0, 0.0, 100.0, 80.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(root_style);
    root.layout_state.content_size = Size {
        width: 100.0,
        height: 300.0,
    };
    root.set_scroll_offset((0.0, 20.0));
    root.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));

    let mut content = Element::new_with_id(0xb4_7b02, 0.0, -20.0, 100.0, 300.0);
    content.set_background_color_value(Color::rgb(28, 52, 76));
    content.set_opacity(0.625);
    content.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));

    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(root)));
    let content = arena.insert(Node::new(Box::new(content)));
    arena.set_parent(content, Some(root));
    arena.push_child(root, content);
    arena.refresh_subtree_dirty_cache(root);
    let roots = [root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("scroll/effect artifact must record") else {
        panic!("scroll/effect artifact must not fall back")
    };
    artifact
}

#[test]
fn materialization_eliminates_only_the_pass_through_target_across_host_paint_cases() {
    let cases = [
        ("baseline", 8.0, [3.0, 0.0]),
        ("scroll-only", 16.0, [3.0, 0.0]),
        ("transform-only", 16.0, [9.0, 4.0]),
    ];
    for (label, scroll_offset_y, translation) in cases {
        for dpr in [1.0, 2.0] {
            for host_has_background in [false, true] {
                let plan = prepare_artifact_surface_raster_plan(
                    direct_scroll_transform_artifact(
                        scroll_offset_y,
                        translation,
                        host_has_background,
                    ),
                    ArtifactSurfaceRasterContext::new(
                        dpr,
                        wgpu::TextureFormat::Bgra8Unorm,
                        [0.0, 0.0],
                        None,
                        4096,
                        MATERIALIZATION_BUDGET_BYTES,
                    )
                    .expect("materialization raster context"),
                )
                .unwrap_or_else(|error| {
                    panic!("{label} dpr={dpr} host_background={host_has_background}: {error:?}")
                });
                assert_eq!(plan.nodes().len(), 1, "{label} dpr={dpr}");
                assert_eq!(
                    plan.nodes()[0].identity().role,
                    RetainedSurfaceRasterRole::Transform,
                    "{label} dpr={dpr}"
                );
                let decisions = plan.materialization_decisions();
                assert_eq!(decisions.len(), 2, "{label} dpr={dpr}");
                assert_eq!(
                    decisions
                        .iter()
                        .map(|decision| decision.source().index())
                        .collect::<Vec<_>>(),
                    vec![0, 1],
                    "{label} dpr={dpr}: decision identities stay artifact-local and stable"
                );
                assert_ne!(
                    decisions[0].target(),
                    decisions[1].target(),
                    "{label} dpr={dpr}: host and retained content owners remain distinct"
                );
                assert_eq!(
                    decisions
                        .iter()
                        .filter(|decision| {
                            decision.outcome()
                                == SurfaceMaterializationOutcome::EliminatedPassThrough
                        })
                        .count(),
                    1,
                    "{label} dpr={dpr}"
                );
                let retained = decisions
                    .iter()
                    .find(|decision| {
                        decision.outcome()
                            == SurfaceMaterializationOutcome::RetainedOwnRasterContent
                    })
                    .expect("content target must be retained by its real paint");
                assert!(retained.has_own_raster_content());
                assert_eq!(retained.nested_target_count(), 0);
                let target = plan.nodes()[0].target();
                assert_eq!(
                    (target.color.width(), target.color.height()),
                    if dpr == 1.0 { (100, 300) } else { (200, 600) },
                    "{label} dpr={dpr}"
                );
                let pair_bytes =
                    u64::from(target.color.width()) * u64::from(target.color.height()) * 12;
                assert_eq!(
                    pair_bytes,
                    if dpr == 1.0 { 360_000 } else { 1_440_000 },
                    "{label} dpr={dpr}"
                );
                let ArtifactSurfaceCompositeGeometryStamp::Transform {
                    destination_bounds_bits,
                    ..
                } = plan.nodes()[0].geometry()
                else {
                    panic!("{label} dpr={dpr}: retained target must composite as Transform")
                };
                let destination = destination_bounds_bits.map(f32::from_bits);
                assert_eq!(
                    destination.map(f32::to_bits),
                    [
                        translation[0],
                        translation[1] - scroll_offset_y,
                        100.0,
                        300.0,
                    ]
                    .map(f32::to_bits),
                    "{label} dpr={dpr}: eliminated receiver placement must be consumed exactly once"
                );
                assert_eq!(
                    seal_prepared_artifact_surface_frame(plan)
                        .expect("materialized direct scroll/transform resident set")
                        .residents()
                        .len(),
                    1
                );
            }
        }
    }
}

#[test]
fn materialization_rule_is_not_tied_to_the_retiring_scroll_transform_authority() {
    for dpr in [1.0, 2.0] {
        let plan = prepare_artifact_surface_raster_plan(
            scroll_effect_artifact(),
            ArtifactSurfaceRasterContext::new(
                dpr,
                wgpu::TextureFormat::Bgra8Unorm,
                [0.0, 0.0],
                None,
                4096,
                MATERIALIZATION_BUDGET_BYTES,
            )
            .expect("materialization raster context"),
        )
        .expect("generic scroll/effect materialization");
        assert_eq!(plan.nodes().len(), 1);
        assert_eq!(
            plan.nodes()[0].identity().role,
            RetainedSurfaceRasterRole::PropertyEffect
        );
        assert_eq!(
            plan.materialization_decisions()
                .iter()
                .filter(|decision| {
                    decision.outcome() == SurfaceMaterializationOutcome::EliminatedPassThrough
                })
                .count(),
            1
        );
        let target = plan.nodes()[0].target();
        assert_eq!(
            (target.color.width(), target.color.height()),
            if dpr == 1.0 { (100, 300) } else { (200, 600) }
        );
        assert_eq!(
            u64::from(target.color.width()) * u64::from(target.color.height()) * 12,
            if dpr == 1.0 { 360_000 } else { 1_440_000 }
        );
        assert_eq!(
            seal_prepared_artifact_surface_frame(plan)
                .expect("materialized scroll/effect resident set")
                .residents()
                .len(),
            1
        );
    }
}

#[test]
fn frozen_scroll_corpus_evaluates_and_retains_every_non_pass_through_target() {
    let cases = [
        {
            let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
            ("scroll", 1, arena, roots, properties, generations)
        },
        {
            let (arena, roots, properties, generations) =
                prepared_same_owner_transform_scroll_scene();
            ("transform-scroll", 2, arena, roots, properties, generations)
        },
        {
            let (arena, roots, properties, generations) = prepared_same_owner_effect_scroll_scene();
            ("effect-scroll", 2, arena, roots, properties, generations)
        },
        {
            let (arena, roots, properties, generations) = prepared_transform_effect_scroll_scene();
            (
                "transform-effect-scroll",
                3,
                arena,
                roots,
                properties,
                generations,
            )
        },
        {
            let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
            ("nested-scroll", 2, arena, roots, properties, generations)
        },
    ];
    for (label, expected_targets, arena, roots, properties, generations) in cases {
        for dpr in [1.0, 2.0] {
            let plan = recorded_plan(&arena, &roots, &properties, &generations, dpr);
            let decisions = plan.materialization_decisions();
            assert_eq!(decisions.len(), expected_targets, "{label} dpr={dpr}");
            assert_eq!(
                decisions
                    .iter()
                    .map(|decision| decision.source().index())
                    .collect::<Vec<_>>(),
                (0..expected_targets).collect::<Vec<_>>(),
                "{label} dpr={dpr}: every logical target must be evaluated once"
            );
            assert!(
                decisions.iter().all(|decision| {
                    decision.outcome() != SurfaceMaterializationOutcome::EliminatedPassThrough
                }),
                "{label} dpr={dpr} must evaluate and reject target elimination: {decisions:?}"
            );
            assert!(decisions.iter().all(|decision| {
                decision.outcome() == SurfaceMaterializationOutcome::RetainedOwnRasterContent
            }));
            assert!(decisions.iter().all(|decision| match decision.outcome() {
                SurfaceMaterializationOutcome::RetainedOwnRasterContent => {
                    decision.has_own_raster_content()
                }
                SurfaceMaterializationOutcome::RetainedNestedTargetCount => {
                    decision.nested_target_count() != 1
                }
                SurfaceMaterializationOutcome::RetainedUncomposedBoundary
                | SurfaceMaterializationOutcome::RetainedIsolation
                | SurfaceMaterializationOutcome::RetainedNonTranslation
                | SurfaceMaterializationOutcome::RetainedClipTransfer => {
                    !decision.has_own_raster_content() && decision.nested_target_count() == 1
                }
                SurfaceMaterializationOutcome::EliminatedPassThrough => false,
            }));
        }
    }
}

#[test]
fn materialization_eliminates_unclipped_translation_without_a_scroll_boundary() {
    for dpr in [1.0, 2.0] {
        for (matrix, eliminated) in [
            (
                glam::Mat4::from_translation(glam::Vec3::new(9.0, 4.0, 0.0)),
                true,
            ),
            (
                glam::Mat4::from_scale(glam::Vec3::new(2.0, 1.0, 1.0)),
                false,
            ),
        ] {
            let mut root = Element::new_with_id(0xb4_7c01, 0.0, 0.0, 40.0, 30.0);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            root.apply_style(style);
            root.set_resolved_transform_for_test(Some(matrix));
            root.set_background_color_value(Color::rgb(24, 48, 72));
            root.set_opacity(0.625);
            root.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
            let mut arena = NodeArena::new();
            let root = arena.insert(Node::new(Box::new(root)));
            arena.refresh_subtree_dirty_cache(root);
            let (properties, generations) = synced_paint_state(&arena, &[root]);
            let plan = recorded_plan(&arena, &[root], &properties, &generations, dpr);
            assert_eq!(plan.materialization_decisions().len(), 2);
            assert_eq!(
                plan.nodes().len(),
                if eliminated { 1 } else { 2 },
                "{:?}",
                plan.materialization_decisions()
            );
            assert_eq!(
                plan.materialization_decisions()[0].outcome(),
                if eliminated {
                    SurfaceMaterializationOutcome::EliminatedPassThrough
                } else {
                    SurfaceMaterializationOutcome::RetainedNonTranslation
                }
            );
            if eliminated {
                let ArtifactSurfaceCompositeGeometryStamp::Effect {
                    destination_bounds_bits,
                    ..
                } = plan.nodes()[0].geometry()
                else {
                    panic!("expected retained effect");
                };
                assert_eq!(
                    destination_bounds_bits.map(f32::from_bits),
                    [9.0, 4.0, 40.0, 30.0]
                );
            }
            assert_eq!(
                seal_prepared_artifact_surface_frame(plan)
                    .unwrap()
                    .residents()
                    .len(),
                if eliminated { 1 } else { 2 }
            );
        }
    }
}

#[test]
fn materialization_preserves_parent_child_mask_raster_obligations() {
    let mut root = Element::new_with_id(0xb4_7e01, 0.0, 0.0, 100.0, 80.0);
    let mut child = Element::new_with_id(0xb4_7e02, 0.0, 0.0, 40.0, 30.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root.apply_style(style.clone());
    child.apply_style(style);
    root.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
        9.0, 4.0, 0.0,
    ))));
    child.set_background_color_value(Color::rgb(24, 48, 72));
    child.set_opacity(0.625);
    root.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    child.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(root)));
    let child = arena.insert(Node::new(Box::new(child)));
    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &[root]);
    let plan = recorded_plan(&arena, &[root], &properties, &generations, 1.0);
    assert_eq!(plan.nodes().len(), 2);
    assert!(
        plan.materialization_decisions()
            .iter()
            .all(|decision| decision.outcome()
                == SurfaceMaterializationOutcome::RetainedOwnRasterContent)
    );
}
