use super::*;

#[test]
fn planner_rejects_negative_origin_known_legacy_crop_before_execution() {
    for (root_id, child_id, root_x, root_y, expected_source) in [
        (0xc2_a001, 0xc2_a002, -4.25, 3.5, [-8.5, 7.0]),
        (0xc2_a003, 0xc2_a004, 4.25, -3.5, [8.5, -7.0]),
    ] {
        let (arena, root, properties, generations) =
            exact_transform_fixture_at_origin_with_ids(root_id, child_id, root_x, root_y);
        let geometry = arena
            .get(root)
            .expect("root")
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("Element root")
            .transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
            .expect("finite negative-origin geometry remains representable");
        assert_eq!(
            [
                geometry.source_bounds.x.to_bits(),
                geometry.source_bounds.y.to_bits(),
            ],
            expected_source.map(f32::to_bits)
        );

        let error =
            plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
                .expect_err("known legacy crop must not reach C2 target declaration");
        assert_eq!(
            error.reasons,
            vec![FramePaintPlanRejection::NegativeSurfaceOrigin(root)]
        );
    }
}

#[test]
fn exact_single_root_transform_builds_one_planning_only_surface_step() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("exact single-root transform subtree must be plan-eligible");

    let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
        panic!("M10C1 must produce exactly one retained surface step")
    };
    assert_eq!(surface.boundary_root, root);
    assert_eq!(surface.transform(), TransformNodeId(root));
    assert_eq!(surface.parent_surface, None);
    assert!(surface.geometry().outer_scissor_rect.is_none());
    let span = only_span(surface);
    assert!(!span.artifact.chunks.is_empty());
    assert!(span.artifact.chunks.iter().all(|chunk| {
        chunk.properties.transform == Some(TransformNodeId(root))
            && chunk.properties.clip.is_none()
            && chunk.properties.effect.is_none()
            && chunk.properties.scroll.is_none()
    }));
    assert!(
        super::super::super::compiler::validate_transform_surface_artifact_for_plan(
            &span.artifact,
            root,
            TransformNodeId(root),
        )
    );
}

#[test]
fn transform_child_isolation_recording_projects_only_inherited_transform_and_partitions_ownership()
 {
    let (arena, root, before, child, descendant, after, properties, generations) =
        exact_transform_child_isolation_fixture();
    let effect = crate::view::compositor::property_tree::EffectNodeId(child);
    let boundary = super::super::super::PlannedBoundary {
        root: child,
        stable_id: arena.get(child).unwrap().element.stable_id(),
        kind: super::super::super::PlannedBoundaryKind::Isolation(effect),
    };
    let cutouts = super::super::super::PlannedBoundaryCutoutSet::from_iter([(child, boundary)]);
    let parent_steps = super::super::super::frame_recorder::record_transform_surface_steps_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        PaintTransformSurfaceWitness::canonical_root(root),
        [0.0, 0.0],
        &cutouts,
    )
    .expect("typed isolation cutout keeps parent transform stream recordable");
    let [
        super::super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(before_artifact),
        super::super::super::frame_recorder::RecordedTransformSurfaceStep::Boundary(actual),
        super::super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(after_artifact),
    ] = parent_steps.as_slice()
    else {
        panic!("parent stream must flush before and after exactly one isolation marker")
    };
    assert_eq!(*actual, boundary);

    let child_artifact =
        super::super::super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
            &arena,
            root,
            child,
            &properties,
            &generations,
        )
        .expect("exact child isolation must record with consumed parent transform");
    assert!(matches!(
        child_artifact.target,
        super::super::super::PaintArtifactTarget::RootOpacityGroup { root, effect: actual }
            if root == child && actual == effect
    ));
    assert_eq!(child_artifact.effect_nodes.len(), 1);
    assert_eq!(child_artifact.effect_nodes[0].id, effect);
    assert!(child_artifact.chunks.iter().all(|chunk| {
        chunk.properties.transform.is_none()
            && chunk.properties.effect == Some(effect)
            && chunk.properties.clip.is_none()
            && chunk.properties.scroll.is_none()
    }));
    child_artifact.ops.iter().for_each(|op| match op {
        PaintOp::DrawRect(op) => assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits()),
        PaintOp::PreparedInlineIfcDecoration(op) => {
            assert_eq!(op.fill.opacity.to_bits(), 1.0_f32.to_bits());
            if let Some(border) = &op.border {
                assert_eq!(border.opacity.to_bits(), 1.0_f32.to_bits());
            }
        }
        PaintOp::PreparedShadow(op) => {
            assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
        }
        PaintOp::PreparedScrollbarOverlay(op) => {
            assert!(op.has_baked_opacity(1.0_f32.to_bits()))
        }
        PaintOp::PreparedText(op) => assert!(
            op.params
                .staging_input
                .glyphs
                .iter()
                .all(|glyph| glyph.paint.opacity.to_bits() == 1.0_f32.to_bits())
        ),
        PaintOp::PreparedImage(op) => {
            assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
        }
        PaintOp::PreparedSvg(op) => {
            assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
        }
    });

    let parent_chunk_owners = before_artifact
        .chunks
        .iter()
        .chain(&after_artifact.chunks)
        .map(|chunk| chunk.owner)
        .collect::<FxHashSet<_>>();
    let child_chunk_owners = child_artifact
        .chunks
        .iter()
        .map(|chunk| chunk.owner)
        .collect::<FxHashSet<_>>();
    assert!(parent_chunk_owners.is_disjoint(&child_chunk_owners));
    assert_eq!(
        parent_chunk_owners,
        FxHashSet::from_iter([root, before, after])
    );
    assert_eq!(
        child_chunk_owners,
        FxHashSet::from_iter([child, descendant])
    );
    let all = parent_chunk_owners
        .union(&child_chunk_owners)
        .copied()
        .collect::<FxHashSet<_>>();
    assert_eq!(
        all,
        FxHashSet::from_iter([root, before, child, descendant, after]),
        "parent spans plus child artifact must exhaust canonical paint ownership"
    );
}

#[test]
fn transform_child_isolation_recording_rejects_wrong_boundary_and_live_projection_mismatch() {
    let (arena, root, before, child, _, _, mut properties, generations) =
        exact_transform_child_isolation_fixture();
    assert!(
        super::super::super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
            &arena,
            before,
            child,
            &properties,
            &generations,
        )
        .is_err(),
        "a non-parent boundary cannot mint consumed-transform authority"
    );

    properties.states.get_mut(&child).unwrap().paint.transform = None;
    assert!(
        super::super::super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
            &arena,
            root,
            child,
            &properties,
            &generations,
        )
        .is_err(),
        "live property mismatch must reject before projected recording"
    );

    let (arena, root, _, child, descendant, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    let mut deferred_style = Style::new();
    deferred_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            crate::style::Position::absolute()
                .left(crate::style::Length::px(0.0))
                .clip(crate::style::ClipMode::Viewport),
        ),
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, descendant)
        .apply_style(deferred_style);
    let _ = super::super::super::take_full_artifact_record_count();
    let error =
        super::super::super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
            &arena,
            root,
            child,
            &properties,
            &generations,
        )
        .expect_err("deferred descendants must fail before either artifact recording pass");
    assert_eq!(
        error,
        vec![super::super::super::FrameArtifactFallbackReason::DeferredBoundary(
            descendant
        )]
    );
    assert_eq!(
        super::super::super::take_full_artifact_record_count(),
        0,
        "deferred preflight must reject before the full artifact pass"
    );
}

#[test]
fn transform_child_isolation_planner_freezes_exact_fractional_geometry_and_cursors() {
    let (arena, root, _, child, _, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    let parent_snapped_offset = arena
        .get(root)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap()
        .retained_child_paint_offset([0.0, 0.0])
        .unwrap();
    let exact_child_bounds = arena
        .get(child)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap()
        .exact_nested_isolation_render_output_bounds(&arena, parent_snapped_offset)
        .unwrap();
    assert!(exact_child_bounds.x > 0.0 && exact_child_bounds.y > 0.0);
    assert!(
        exact_child_bounds.x.fract() != 0.0 || exact_child_bounds.y.fract() != 0.0,
        "fixture must retain a positive fractional nonzero child-local origin"
    );

    let plan = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .expect("exact Transform -> direct child Isolation plan");
    let root_surface = only_surface(&plan);
    assert!(matches!(root_surface.kind(), SurfaceKind::Transform(_)));
    let [
        PaintPlanStep::ArtifactSpan(before),
        PaintPlanStep::RetainedSurface(child_surface),
        PaintPlanStep::ArtifactSpan(after),
    ] = root_surface.raster_steps()
    else {
        panic!("parent recorder must preserve before-marker-after order")
    };
    let SurfaceKind::NestedIsolation(nested) = child_surface.kind() else {
        panic!("typed marker must become a nested-isolation role")
    };
    assert_eq!(child_surface.boundary_root(), child);
    assert_eq!(child_surface.parent_surface(), Some(root));
    assert!(nested.geometry.bitwise_eq(nested.planned_geometry_witness));
    assert_eq!(
        [
            nested.geometry.source_bounds.x,
            nested.geometry.source_bounds.y,
            nested.geometry.source_bounds.width,
            nested.geometry.source_bounds.height,
        ]
        .map(f32::to_bits),
        [
            exact_child_bounds.x,
            exact_child_bounds.y,
            exact_child_bounds.width,
            exact_child_bounds.height,
        ]
        .map(f32::to_bits)
    );
    assert_eq!(
        nested.geometry.logical_size(),
        [exact_child_bounds.width, exact_child_bounds.height,]
    );
    assert_eq!(nested.geometry.source_bounds.corner_radii, [0.0; 4]);

    let [PaintPlanStep::ArtifactSpan(child_span)] = child_surface.raster_steps() else {
        panic!("nested isolation owns exactly one projected artifact")
    };
    let child_terminal = opaque_order_count(child_span.artifact());
    assert_eq!(child_span.opaque_order_span(), &(0..child_terminal));
    assert_eq!(
        child_surface.aggregate_opaque_order_span(),
        &(0..child_terminal)
    );
    let before_end = opaque_order_count(before.artifact());
    assert_eq!(before.opaque_order_span(), &(0..before_end));
    let expected_after_start = before_end.max(child_terminal);
    assert_eq!(after.opaque_order_span().start, expected_after_start);
    assert_eq!(
        after.opaque_order_span().end,
        expected_after_start + opaque_order_count(after.artifact())
    );
    assert_eq!(
        root_surface.aggregate_opaque_order_span(),
        &(0..after.opaque_order_span().end)
    );
}

#[test]
fn transform_child_isolation_planner_hard_gates_shape_and_extra_properties() {
    let (arena, root, _, _, _, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    let baseline = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .expect("retained-compatible transform child isolation baseline");
    let _ = only_surface(&baseline);

    let (arena, root, _, child, _, _, mut properties, generations) =
        exact_transform_child_isolation_fixture();
    properties.transforms.insert(
        TransformNodeId(child),
        crate::view::compositor::property_tree::TransformNode {
            owner: child,
            parent: Some(TransformNodeId(root)),
            viewport_matrix: glam::Mat4::IDENTITY,
            generation: 1,
        },
    );
    let error = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .expect_err("a second transform is an extra property boundary");
    assert!(
        error
            .reasons
            .contains(&FramePaintPlanRejection::TransformNodeCount(2))
    );

    let (arena, root, _, child, _, _, mut properties, generations) =
        exact_transform_child_isolation_fixture();
    properties.states.get_mut(&child).unwrap().paint.clip =
        Some(crate::view::compositor::property_tree::ClipNodeId {
            owner: child,
            role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
        });
    let error = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .expect_err("clip state is outside the mixed exact slice");
    assert!(
        error
            .reasons
            .contains(&FramePaintPlanRejection::ClipBoundary(child))
    );

    let (arena, root, _, child, _, _, mut properties, generations) =
        exact_transform_child_isolation_fixture();
    properties.effects.remove(&EffectNodeId(child));
    let error = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .expect_err("missing child effect snapshot cannot mint a nested isolation");
    assert!(
        error
            .reasons
            .iter()
            .any(|reason| matches!(reason, FramePaintPlanRejection::InvalidIsolationEffect(_)))
    );

    let (arena, root, _, child, descendant, _, properties, generations) =
        exact_transform_child_isolation_fixture();
    crate::view::test_support::get_element_mut::<Element>(&arena, descendant)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 2.0, 0.0,
        ))));
    let error = plan_single_root_transform_child_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
    )
    .expect_err(
        "a live descendant transform added after property sync must not produce a plan",
    );
    assert_eq!(
        error.reasons,
        vec![FramePaintPlanRejection::InvalidSurfaceGeometry(child)],
        "child-local geometry must reject stale property trees before artifact recording"
    );
}
