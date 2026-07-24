use super::*;
use glam::Vec2;
use slotmap::SlotMap;

use crate::view::base_component::{
    Rect, RetainedSurfaceBounds, ScrollAxisSnapshot, ScrollContentsClipWitness,
    ScrollbarInteractionWitness, ScrollbarOverlayWitness, ScrollbarPaintStateWitness, Size,
    persistent_target_texture_descriptors, scroll_content_layer_stable_key,
    texture_desc_for_logical_bounds, transformed_layer_stable_key,
};
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ScrollNodeId, TransformNodeId,
};
use crate::view::paint::{
    PaintChunkId, PaintChunkRole, PaintNodePhase, PaintOwnerSnapshot, PaintPayloadIdentity,
    PaintPropertyScope,
};

fn target(
    bounds: RetainedSurfaceBounds,
    color_key: crate::view::frame_graph::PersistentTextureKey,
) -> RetainedSurfaceRasterInputs {
    let color =
        texture_desc_for_logical_bounds(bounds, 1.0, None, wgpu::TextureFormat::Bgra8Unorm);
    let (color, depth) = persistent_target_texture_descriptors(color, color_key);
    RetainedSurfaceRasterInputs {
        color,
        depth,
        scale_factor_bits: 1.0_f32.to_bits(),
        source_bounds_bits: [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits),
    }
}

fn content_stamp(
    content_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
) -> RetainedSurfaceRasterStamp {
    let bounds = RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        corner_radii: [0.0; 4],
    };
    let chunk = RetainedSurfaceChunkStamp {
        id: PaintChunkId {
            owner: content_root,
            scope: PaintPropertyScope::SelfPaint,
            phase: PaintNodePhase::BeforeChildren,
            slot: 0,
            role: PaintChunkRole::SelfDecoration,
        },
        owner: content_root,
        bounds_bits: [0.0, 0.0, 100.0, 100.0].map(f32::to_bits),
        clip: None,
        non_boundary_self_paint_revision: None,
        topology_revision: 1,
        non_boundary_composite_revision: None,
        payload_identity: PaintPayloadIdentity::None,
        op_count: 1,
    };
    let artifact = RetainedSurfaceArtifactSpanStamp {
        step_index: 0,
        owner_topology: vec![PaintOwnerSnapshot {
            owner: content_root,
            parent: None,
        }],
        clip_nodes: Vec::new(),
        chunks: vec![chunk],
        op_count: 1,
        opaque_order_span: 0..1,
        scroll_placement_normalized_owners: Vec::new(),
    };
    validated_scroll_content_raster_stamp(
        content_root,
        stable_id,
        target(bounds, scroll_content_layer_stable_key(stable_id)),
        artifact,
        0..1,
    )
    .expect("canonical scroll-content stamp")
}

fn empty_boundary_artifact(
    boundary_root: crate::view::node_arena::NodeKey,
    step_index: usize,
) -> RetainedSurfaceArtifactSpanStamp {
    RetainedSurfaceArtifactSpanStamp {
        step_index,
        owner_topology: vec![PaintOwnerSnapshot {
            owner: boundary_root,
            parent: None,
        }],
        clip_nodes: Vec::new(),
        chunks: Vec::new(),
        op_count: 0,
        opaque_order_span: 0..0,
        scroll_placement_normalized_owners: Vec::new(),
    }
}

fn canonical_dependency() -> TransformScrollBoundaryRasterDependency {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let receiver_root = keys.insert(());
    let boundary_root = keys.insert(());
    let content_root = keys.insert(());
    let receiver_stable_id = 91_001;
    let boundary_stable_id = 91_002;
    let content_stable_id = 91_003;
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    let overlay = ScrollbarOverlayWitness {
        vertical_track: None,
        vertical_thumb: None,
        horizontal_track: None,
        horizontal_thumb: None,
        interaction: ScrollbarInteractionWitness {
            hovered: false,
            dragging_axis: None,
            has_interaction_timestamp: false,
        },
        paint_state: ScrollbarPaintStateWitness::NotPaintable,
        sampled_alpha: 0.0,
        shadow_blur_radius: 0.0,
    };
    let contents_clip = ClipNodeSnapshot {
        id: ClipNodeId {
            owner: boundary_root,
            role: ClipNodeRole::ContentsClip,
        },
        owner: boundary_root,
        parent: None,
        logical_scissor: [0, 0, 100, 100],
        behavior: ClipBehavior::Intersect,
        generation: 13,
    };
    TransformScrollBoundaryRasterDependency {
        step_index: 0,
        scene_root_ordinal: 0,
        receiver_owner: receiver_root,
        receiver_transform_id: TransformNodeId(receiver_root),
        receiver_stable_id,
        scroll_boundary_ordinal: 0,
        boundary_root,
        boundary_stable_id,
        content_root,
        content_stable_id,
        insertion_index: 0,
        receiver_step_count: 1,
        before_span: 0..0,
        after_span: 1..1,
        recorded_receiver_opaque_before: 0,
        recorded_receiver_opaque_after: 0,
        host_parent_span: 0..0,
        content_local_span: 0..1,
        overlay_parent_span: 0..0,
        host_artifact: empty_boundary_artifact(boundary_root, 0),
        // A hidden/not-paintable scrollbar is still a structural O phase.
        // Its empty artifact must remain a legal dependency identity.
        overlay_artifact: empty_boundary_artifact(boundary_root, 2),
        content_stamps: vec![content_stamp(content_root, content_stable_id)],
        scroll: ScrollNodeSnapshot {
            id: ScrollNodeId(boundary_root),
            owner: boundary_root,
            parent: None,
            offset: Vec2::ZERO,
            configured_axis: ScrollAxisSnapshot::Vertical,
            viewport,
            content_size: Size {
                width: 100.0,
                height: 100.0,
            },
            layout_content_bounds_at_zero: viewport,
            scrollbar_overlay: overlay,
            contents_clip: ScrollContentsClipWitness::ExactRect([0, 0, 100, 100]),
            generation: 12,
        },
        contents_clip,
        receiver_local_raster_clips: Vec::new(),
        receiver_ancestor_composite_clips: Vec::new(),
        same_owner_role: None,
    }
}

fn effect_dependency_from(
    dependency: &TransformScrollBoundaryRasterDependency,
) -> EffectScrollBoundaryRasterDependency {
    EffectScrollBoundaryRasterDependency {
        step_index: dependency.step_index,
        scene_root_ordinal: dependency.scene_root_ordinal,
        receiver_owner: dependency.receiver_owner,
        receiver_stable_id: dependency.receiver_stable_id,
        scroll_boundary_ordinal: dependency.scroll_boundary_ordinal,
        boundary_root: dependency.boundary_root,
        boundary_stable_id: dependency.boundary_stable_id,
        content_root: dependency.content_root,
        content_stable_id: dependency.content_stable_id,
        insertion_index: dependency.insertion_index,
        receiver_step_count: dependency.receiver_step_count,
        before_span: dependency.before_span.clone(),
        after_span: dependency.after_span.clone(),
        recorded_receiver_opaque_before: dependency.recorded_receiver_opaque_before,
        recorded_receiver_opaque_after: dependency.recorded_receiver_opaque_after,
        host_parent_span: dependency.host_parent_span.clone(),
        content_local_span: dependency.content_local_span.clone(),
        overlay_parent_span: dependency.overlay_parent_span.clone(),
        host_artifact: dependency.host_artifact.clone(),
        overlay_artifact: dependency.overlay_artifact.clone(),
        content_stamps: dependency.content_stamps.clone(),
        scroll: dependency.scroll,
        contents_clip: dependency.contents_clip,
        receiver_local_raster_clips: dependency.receiver_local_raster_clips.clone(),
        receiver_ancestor_composite_clips: dependency.receiver_ancestor_composite_clips.clone(),
        same_owner_role: None,
    }
}

fn transform_effect_scroll_outer_fixture() -> (
    RetainedSurfaceRasterStamp,
    TransformNodeId,
    EffectPropertySurfaceArtifactContract,
) {
    let scroll = canonical_dependency();
    let mut effect_dependency = effect_dependency_from(&scroll);
    effect_dependency.receiver_step_count = 2;
    effect_dependency.after_span = 1..2;
    effect_dependency.recorded_receiver_opaque_after = 1;
    let effect = EffectNodeSnapshot {
        id: EffectNodeId(scroll.receiver_owner),
        owner: scroll.receiver_owner,
        parent: None,
        opacity: 0.5,
        generation: 17,
    };
    let contract = EffectPropertySurfaceArtifactContract::new(
        effect.owner,
        scroll.receiver_stable_id,
        effect,
        vec![effect],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![super::super::EffectPropertyContentWitness {
            owner: effect.owner,
            stable_id: scroll.receiver_stable_id,
            parent: None,
            self_paint_revision: 19,
            topology_revision: 23,
        }],
    )
    .expect("canonical E authority");
    let bounds = RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        corner_radii: [0.0; 4],
    };
    let mut local_artifact = match &scroll.content_stamps[0].ordered_steps[0] {
        RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => span.clone(),
        _ => unreachable!(),
    };
    local_artifact.step_index = 1;
    let effect_stamp = validated_effect_scroll_receiver_raster_stamp(
        &contract,
        target(
            bounds,
            crate::view::base_component::isolation_layer_stable_key(contract.stable_id()),
        ),
        vec![
            RetainedSurfaceRasterStepStamp::EffectScrollBoundary(effect_dependency),
            RetainedSurfaceRasterStepStamp::ArtifactSpan(local_artifact),
        ],
        0..1,
    )
    .expect("canonical E -> Scroll child stamp");
    let mut outer_keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let _first = outer_keys.insert(());
    let outer_owner = outer_keys.insert(());
    assert_ne!(outer_owner, effect.owner);
    let outer_transform = TransformNodeId(outer_owner);
    let child = TransformEffectScrollChildRasterDependency {
        step_index: 0,
        child_source_bounds_bits: effect_stamp.target.source_bounds_bits,
        child_opacity_bits: effect.opacity.to_bits(),
        child_effect_generation: effect.generation,
        local_basis: outer_transform,
        parent_opaque_order_before: 0,
        parent_opaque_order_after: 0,
        same_owner_role: None,
        child_stamp: Box::new(effect_stamp),
    };
    let outer_stable_id = 92_001;
    let outer = validated_transform_effect_scroll_outer_raster_stamp(
        outer_transform,
        outer_stable_id,
        &contract,
        target(bounds, transformed_layer_stable_key(outer_stable_id)),
        vec![RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(
            child,
        )],
        0..0,
    )
    .expect("dedicated T -> E -> Scroll outer stamp");
    (outer, outer_transform, contract)
}

#[test]
fn transform_effect_scroll_outer_stamp_is_dedicated_and_matrix_neutral() {
    let (outer, transform, contract) = transform_effect_scroll_outer_fixture();
    assert!(
        transform_effect_scroll_outer_raster_stamp_validates_contract(
            &outer, transform, &contract
        )
    );
    let [RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency)] =
        outer.ordered_steps.as_slice()
    else {
        panic!("one typed child dependency")
    };
    assert_eq!(dependency.local_basis, transform);
    assert_eq!(
        dependency.child_source_bounds_bits,
        outer.target.source_bounds_bits
    );
    // The dedicated identity exposes only the local transform id. There
    // is no viewport matrix or transform generation field to drift.
    assert_eq!(dependency.child_effect_generation, 17);
    assert_eq!(dependency.child_stamp.opaque_order_span, 0..1);
    assert_eq!(dependency.parent_opaque_order_before, 0);
    assert_eq!(dependency.parent_opaque_order_after, 0);

    assert!(!retained_surface_raster_stamp_is_canonical(&outer));
    assert!(!retained_surface_raster_stamp_is_canonical_at_depth(
        &outer, 0
    ));
    assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
        &outer, 0
    ));
    assert!(!transform_scroll_receiver_raster_stamp_is_canonical(&outer));
}

#[test]
fn transform_effect_scroll_outer_stamp_rejects_typed_dependency_drift() {
    let (outer, transform, contract) = transform_effect_scroll_outer_fixture();
    let rejects = |stamp: &RetainedSurfaceRasterStamp| {
        !transform_effect_scroll_outer_raster_stamp_validates_contract(
            stamp, transform, &contract,
        )
    };

    let mut source = outer.clone();
    let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
        &mut source.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.child_source_bounds_bits[2] = 99.0_f32.to_bits();
    assert!(rejects(&source));

    let mut opacity = outer.clone();
    let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
        &mut opacity.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.child_opacity_bits = 0.75_f32.to_bits();
    assert!(rejects(&opacity));

    let mut generation = outer.clone();
    let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
        &mut generation.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.child_effect_generation += 1;
    assert!(rejects(&generation));

    let mut basis = outer.clone();
    let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
        &mut basis.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.local_basis = TransformNodeId(contract.boundary_root());
    assert!(rejects(&basis));

    let mut span = outer;
    let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) =
        &mut span.ordered_steps[0]
    else {
        unreachable!()
    };
    dependency.parent_opaque_order_after = 1;
    assert!(rejects(&span));
}

#[test]
fn transform_effect_scroll_child_is_rejected_by_every_legacy_gate() {
    let (outer, _transform, contract) = transform_effect_scroll_outer_fixture();
    let typed_step = outer.ordered_steps[0].clone();
    let bounds_values = outer.target.source_bounds_bits.map(f32::from_bits);
    let bounds = RetainedSurfaceBounds {
        x: bounds_values[0],
        y: bounds_values[1],
        width: bounds_values[2],
        height: bounds_values[3],
        corner_radii: [0.0; 4],
    };

    assert!(
        validated_retained_surface_tree_raster_stamp(
            outer.identity.boundary_root,
            outer.identity.stable_id,
            outer.identity.color_key,
            RetainedSurfaceRasterRole::Transform,
            0,
            outer.target.clone(),
            vec![typed_step.clone()],
            outer.opaque_order_span.clone(),
        )
        .is_none()
    );
    assert!(
        validated_property_scene_surface_raster_stamp(
            outer.identity.boundary_root,
            outer.identity.stable_id,
            outer.identity.color_key,
            0,
            outer.target.clone(),
            vec![typed_step.clone()],
            outer.opaque_order_span.clone(),
        )
        .is_none()
    );

    let effect_target = target(
        bounds,
        crate::view::base_component::isolation_layer_stable_key(contract.stable_id()),
    );
    assert!(
        validated_effect_scroll_receiver_raster_stamp(
            &contract,
            effect_target.clone(),
            vec![typed_step.clone()],
            0..0,
        )
        .is_none()
    );
    assert!(
        validated_property_effect_surface_raster_stamp(
            &contract,
            0,
            effect_target,
            vec![typed_step.clone()],
            0..0,
        )
        .is_none()
    );

    let RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency) = &typed_step
    else {
        unreachable!()
    };
    let mut effect_like = dependency.child_stamp.as_ref().clone();
    effect_like.ordered_steps = vec![typed_step];
    assert!(!effect_scroll_receiver_raster_stamp_validates_contract(
        &effect_like,
        &contract,
    ));
    assert!(
        !property_effect_surface_raster_stamp_validates_contract_at_depth(
            &effect_like,
            &contract,
            0,
        )
    );
    assert!(
        super::super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
            &outer,
        )
    );
}

#[test]
fn transform_scroll_boundary_accepts_direct_translation_and_zero_op_overlay() {
    let dependency = canonical_dependency();
    assert!(dependency.overlay_artifact.chunks.is_empty());
    assert_eq!(dependency.overlay_artifact.op_count, 0);
    assert!(transform_scroll_boundary_dependency_is_canonical(
        &dependency
    ));
}

#[test]
fn transform_scroll_boundary_rejects_clip_and_identity_tampering() {
    let dependency = canonical_dependency();

    let mut receiver_clipped = dependency.clone();
    receiver_clipped
        .receiver_local_raster_clips
        .push(receiver_clipped.contents_clip);
    assert!(!transform_scroll_boundary_dependency_is_canonical(
        &receiver_clipped
    ));

    let mut receiver_composite_clipped = dependency.clone();
    receiver_composite_clipped
        .receiver_ancestor_composite_clips
        .push(receiver_composite_clipped.contents_clip);
    assert!(!transform_scroll_boundary_dependency_is_canonical(
        &receiver_composite_clipped
    ));

    let mut receiver_identity_drift = dependency.clone();
    receiver_identity_drift.receiver_transform_id = TransformNodeId(dependency.boundary_root);
    assert!(!transform_scroll_boundary_dependency_is_canonical(
        &receiver_identity_drift
    ));

    let mut identity_drift = dependency;
    identity_drift.content_stable_id += 1;
    assert!(!transform_scroll_boundary_dependency_is_canonical(
        &identity_drift
    ));
}

#[test]
fn same_owner_effect_scroll_role_stamp_is_required_and_tamper_evident() {
    let scroll_dependency = canonical_dependency();
    let mut dependency = effect_dependency_from(&scroll_dependency);
    dependency.receiver_owner = dependency.boundary_root;
    dependency.receiver_stable_id = dependency.boundary_stable_id;
    dependency.same_owner_role = Some(SameOwnerEffectScrollRasterRoleStamp {
        owner: dependency.boundary_root,
        stable_id: dependency.boundary_stable_id,
        effect: EffectNodeId(dependency.boundary_root),
        scroll: dependency.scroll.id,
        contents_clip: dependency.contents_clip.id,
        content_root: dependency.content_root,
        content_stable_id: dependency.content_stable_id,
    });
    assert!(effect_scroll_boundary_dependency_is_canonical(&dependency));

    let mut missing = dependency.clone();
    missing.same_owner_role = None;
    assert!(!effect_scroll_boundary_dependency_is_canonical(&missing));

    let mut effect = dependency.clone();
    effect.same_owner_role.as_mut().unwrap().effect = EffectNodeId(effect.content_root);
    assert!(!effect_scroll_boundary_dependency_is_canonical(&effect));

    let mut stable = dependency.clone();
    stable.same_owner_role.as_mut().unwrap().stable_id ^= 1;
    assert!(!effect_scroll_boundary_dependency_is_canonical(&stable));

    let mut content = dependency;
    content.same_owner_role.as_mut().unwrap().content_stable_id ^= 1;
    assert!(!effect_scroll_boundary_dependency_is_canonical(&content));
}

#[test]
fn generic_retained_surface_canonicalizers_reject_scroll_boundary_steps() {
    let dependency = canonical_dependency();
    let bounds = RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 16.0,
        height: 12.0,
        corner_radii: [0.0; 4],
    };
    let stable_id = dependency.receiver_stable_id;
    let mut receiver = validated_property_scene_surface_raster_stamp(
        dependency.receiver_owner,
        stable_id,
        transformed_layer_stable_key(stable_id),
        0,
        target(bounds, transformed_layer_stable_key(stable_id)),
        Vec::new(),
        0..0,
    )
    .expect("canonical empty receiver stamp");
    let effect_dependency = EffectScrollBoundaryRasterDependency {
        step_index: dependency.step_index,
        scene_root_ordinal: dependency.scene_root_ordinal,
        receiver_owner: dependency.receiver_owner,
        receiver_stable_id: dependency.receiver_stable_id,
        scroll_boundary_ordinal: dependency.scroll_boundary_ordinal,
        boundary_root: dependency.boundary_root,
        boundary_stable_id: dependency.boundary_stable_id,
        content_root: dependency.content_root,
        content_stable_id: dependency.content_stable_id,
        insertion_index: dependency.insertion_index,
        receiver_step_count: dependency.receiver_step_count,
        before_span: dependency.before_span.clone(),
        after_span: dependency.after_span.clone(),
        recorded_receiver_opaque_before: dependency.recorded_receiver_opaque_before,
        recorded_receiver_opaque_after: dependency.recorded_receiver_opaque_after,
        host_parent_span: dependency.host_parent_span.clone(),
        content_local_span: dependency.content_local_span.clone(),
        overlay_parent_span: dependency.overlay_parent_span.clone(),
        host_artifact: dependency.host_artifact.clone(),
        overlay_artifact: dependency.overlay_artifact.clone(),
        content_stamps: dependency.content_stamps.clone(),
        scroll: dependency.scroll,
        contents_clip: dependency.contents_clip,
        receiver_local_raster_clips: dependency.receiver_local_raster_clips.clone(),
        receiver_ancestor_composite_clips: dependency.receiver_ancestor_composite_clips.clone(),
        same_owner_role: None,
    };
    receiver.ordered_steps = vec![RetainedSurfaceRasterStepStamp::ScrollBoundary(dependency)];

    assert!(!retained_surface_raster_stamp_is_canonical(&receiver));
    assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
        &receiver, 0
    ));

    receiver.ordered_steps = vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
        effect_dependency.clone(),
    )];
    assert!(!retained_surface_raster_stamp_is_canonical(&receiver));
    assert!(!retained_surface_raster_stamp_is_canonical_at_depth(
        &receiver, 0
    ));
    assert!(!transform_scroll_receiver_raster_stamp_is_canonical(
        &receiver
    ));
    assert!(!property_scene_surface_raster_stamp_is_canonical_at_depth(
        &receiver, 0
    ));
    assert!(
        super::super::retained_surface_executor::legacy_property_executor_rejects_effect_scroll_boundary_for_test(
            &receiver,
        )
    );

    assert!(
        validated_property_scene_surface_raster_stamp(
            receiver.identity.boundary_root,
            receiver.identity.stable_id,
            receiver.identity.color_key,
            0,
            receiver.target.clone(),
            vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                effect_dependency.clone(),
            )],
            0..0,
        )
        .is_none()
    );
    assert!(
        validated_retained_surface_tree_raster_stamp(
            receiver.identity.boundary_root,
            receiver.identity.stable_id,
            receiver.identity.color_key,
            RetainedSurfaceRasterRole::Transform,
            0,
            receiver.target.clone(),
            vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                effect_dependency.clone(),
            )],
            0..0,
        )
        .is_none()
    );

    let scroll_key = scroll_content_layer_stable_key(effect_dependency.content_stable_id);
    let scroll_bounds = RetainedSurfaceBounds {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        corner_radii: [0.0; 4],
    };
    assert!(
        validated_retained_surface_tree_raster_stamp_with_scroll(
            effect_dependency.content_root,
            effect_dependency.content_stable_id,
            scroll_key,
            RetainedSurfaceRasterRole::ScrollContent,
            0,
            target(scroll_bounds, scroll_key),
            vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                effect_dependency.clone(),
            )],
            0..0,
            None,
            None,
            None,
            None,
            None,
        )
        .is_none()
    );

    let effect = EffectNodeSnapshot {
        id: crate::view::compositor::property_tree::EffectNodeId(
            effect_dependency.receiver_owner,
        ),
        owner: effect_dependency.receiver_owner,
        parent: None,
        opacity: 0.5,
        generation: 1,
    };
    let contract = EffectPropertySurfaceArtifactContract::new(
        effect_dependency.receiver_owner,
        effect_dependency.receiver_stable_id,
        effect,
        vec![effect],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![super::super::EffectPropertyContentWitness {
            owner: effect_dependency.receiver_owner,
            stable_id: effect_dependency.receiver_stable_id,
            parent: None,
            self_paint_revision: 1,
            topology_revision: 1,
        }],
    )
    .expect("canonical effect authority for isolation regression");
    let effect_key =
        crate::view::base_component::isolation_layer_stable_key(contract.stable_id());
    assert!(
        validated_property_effect_surface_raster_stamp(
            &contract,
            0,
            target(bounds, effect_key),
            vec![RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                effect_dependency,
            )],
            0..0,
        )
        .is_none()
    );
}
