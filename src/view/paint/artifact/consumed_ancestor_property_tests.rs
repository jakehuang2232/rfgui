use super::*;
use super::super::recording_context::PaintRecordingContext;
use crate::view::compositor::property_tree::{LayoutPositionNodeId, VisualOffsetNodeId};
use crate::view::base_component::Element;
use crate::view::test_support::{commit_child, commit_element, new_test_arena};
use slotmap::SlotMap;

fn keys() -> (NodeKey, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let parent = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xd1_1000, 0.0, 0.0, 10.0, 10.0)),
    );
    let child = commit_child(
        &mut arena,
        parent,
        Box::new(Element::new_with_id(0xd1_1001, 0.0, 0.0, 8.0, 8.0)),
    );
    let descendant = commit_child(
        &mut arena,
        child,
        Box::new(Element::new_with_id(0xd1_1002, 0.0, 0.0, 4.0, 4.0)),
    );
    (parent, child, descendant)
}

#[test]
fn property_effect_contract_detaches_the_local_clip_root_parent() {
    let (ancestor, boundary, _) = keys();
    let effect = EffectNodeSnapshot {
        id: EffectNodeId(boundary),
        owner: boundary,
        parent: None,
        opacity: 0.5,
        generation: 7,
    };
    let ancestor_clip = ClipNodeSnapshot {
        id: ClipNodeId {
            owner: ancestor,
            role: ClipNodeRole::ContentsClip,
        },
        owner: ancestor,
        parent: None,
        behavior: crate::view::compositor::property_tree::ClipBehavior::Intersect,
        logical_scissor: [0, 0, 20, 20],
        generation: 8,
    };
    let local_clip = ClipNodeSnapshot {
        id: ClipNodeId {
            owner: boundary,
            role: ClipNodeRole::SelfClip,
        },
        owner: boundary,
        parent: Some(ancestor_clip.id),
        behavior: crate::view::compositor::property_tree::ClipBehavior::Replace,
        logical_scissor: [2, 2, 10, 10],
        generation: 9,
    };
    let contract = EffectPropertySurfaceArtifactContract::new(
        boundary,
        0xd1_2000,
        effect,
        vec![effect],
        Vec::new(),
        vec![local_clip],
        vec![ancestor_clip],
        vec![EffectPropertyContentWitness {
            owner: boundary,
            stable_id: 0xd1_2000,
            parent: None,
            self_paint_revision: 10,
            topology_revision: 11,
        }],
    )
    .expect("canonical clipped effect contract");
    let detached = contract
        .detach_clip_snapshot(&[local_clip, ancestor_clip])
        .expect("exact ancestor suffix detaches");
    assert_eq!(detached.len(), 1);
    assert_eq!(detached[0].id, local_clip.id);
    assert_eq!(detached[0].parent, None);
    assert_eq!(contract.isolated_local_raster_clips(), detached);
}

#[test]
fn consumed_transform_projection_is_owner_bound_and_preserves_other_properties() {
    let (parent, child, descendant) = keys();
    let transform = TransformNodeId(parent);
    let witness = ConsumedAncestorTransformWitness::new(parent, child, transform)
        .expect("canonical direct-boundary identity");
    let effect = EffectNodeId(child);
    let live = PropertyTreeState {
        transform: Some(transform),
        effect: Some(effect),
        ..Default::default()
    };
    let context = PaintRecordingContext {
        recording_owner: Some(descendant),
        consumed_ancestor_property: Some(ConsumedAncestorProperty::Transform(
            witness.for_target(descendant),
        )),
        ..Default::default()
    };
    assert_eq!(
        context.project_consumed_ancestor_property(live),
        Some(PropertyTreeState {
            transform: None,
            effect: Some(effect),
            ..Default::default()
        })
    );
}

#[test]
fn consumed_scroll_contents_projection_is_atomic_owner_bound_and_preserves_other_properties() {
    let (parent, child, descendant) = keys();
    let scroll = ScrollNodeId(parent);
    let contents_clip = ClipNodeId {
        owner: parent,
        role: ClipNodeRole::ContentsClip,
    };
    let witness =
        ConsumedAncestorScrollContentsWitness::new(parent, child, scroll, contents_clip).unwrap();
    let effect = EffectNodeId(child);
    let transform = TransformNodeId(child);
    let layout_position = LayoutPositionNodeId(child);
    let visual_offset = VisualOffsetNodeId(child);
    let live = PropertyTreeState {
        transform: Some(transform),
        clip: Some(contents_clip),
        effect: Some(effect),
        scroll: Some(scroll),
        layout_position: Some(layout_position),
        visual_offset: Some(visual_offset),
    };
    let context = PaintRecordingContext {
        recording_owner: Some(descendant),
        consumed_ancestor_property: Some(ConsumedAncestorProperty::ScrollContents(
            witness.for_target(descendant),
        )),
        ..Default::default()
    };
    assert_eq!(
        context.project_consumed_ancestor_property(live),
        Some(PropertyTreeState {
            transform: Some(transform),
            effect: Some(effect),
            layout_position: Some(layout_position),
            visual_offset: Some(visual_offset),
            ..Default::default()
        })
    );

    for mismatch in [
        PropertyTreeState {
            scroll: None,
            ..live
        },
        PropertyTreeState { clip: None, ..live },
    ] {
        assert_eq!(context.project_consumed_ancestor_property(mismatch), None);
    }
    let wrong_target = PaintRecordingContext {
        recording_owner: Some(child),
        consumed_ancestor_property: context.consumed_ancestor_property,
        ..Default::default()
    };
    assert_eq!(wrong_target.project_consumed_ancestor_property(live), None);
    assert!(
        ConsumedAncestorScrollContentsWitness::new(parent, parent, scroll, contents_clip).is_none()
    );
    assert!(
        ConsumedAncestorScrollContentsWitness::new(
            parent,
            child,
            ScrollNodeId(child),
            contents_clip,
        )
        .is_none()
    );
}

#[test]
fn consumed_property_stack_projects_transform_then_scroll_atomically() {
    let (transform_owner, scroll_owner, content_owner) = keys();
    let transform = TransformNodeId(transform_owner);
    let scroll = ScrollNodeId(scroll_owner);
    let contents_clip = ClipNodeId {
        owner: scroll_owner,
        role: ClipNodeRole::ContentsClip,
    };
    let transform_witness =
        ConsumedAncestorTransformWitness::new(transform_owner, scroll_owner, transform).unwrap();
    let scroll_witness = ConsumedAncestorScrollContentsWitness::new(
        scroll_owner,
        content_owner,
        scroll,
        contents_clip,
    )
    .unwrap();
    let stack = ConsumedAncestorPropertyStackWitness::new(
        content_owner,
        &[
            ConsumedAncestorProperty::Transform(transform_witness),
            ConsumedAncestorProperty::ScrollContents(scroll_witness),
        ],
    )
    .unwrap();
    let live = PropertyTreeState {
        transform: Some(transform),
        clip: Some(contents_clip),
        scroll: Some(scroll),
        ..Default::default()
    };
    let context = PaintRecordingContext {
        recording_owner: Some(content_owner),
        consumed_ancestor_property_stack: Some(stack),
        ..Default::default()
    };
    assert_eq!(
        context.project_consumed_ancestor_property(live),
        Some(PropertyTreeState::default())
    );
    assert_eq!(
        context.project_consumed_ancestor_property(PropertyTreeState {
            transform: None,
            ..live
        }),
        None
    );
    let retargeted = PaintRecordingContext {
        recording_owner: Some(scroll_owner),
        consumed_ancestor_property_stack: Some(stack),
        ..Default::default()
    };
    assert_eq!(retargeted.project_consumed_ancestor_property(live), None);
    let reversed = ConsumedAncestorPropertyStackWitness::new(
        content_owner,
        &[
            ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ConsumedAncestorProperty::Transform(transform_witness),
        ],
    );
    assert!(
        reversed.is_none(),
        "planner order is part of the capability"
    );
}

#[test]
fn consumed_effect_scroll_stack_requires_exact_chain_and_neutral_authority() {
    let (effect_owner, scroll_owner, content_owner) = keys();
    let effect = EffectNodeSnapshot {
        id: EffectNodeId(effect_owner),
        owner: effect_owner,
        parent: None,
        opacity: 0.5,
        generation: 7,
    };
    let effect_witness = ConsumedAncestorEffectWitness::new(
        effect_owner,
        scroll_owner,
        effect,
        Some(effect.id),
        None,
    )
    .unwrap();
    let contents_clip = ClipNodeId {
        owner: scroll_owner,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll_witness = ConsumedAncestorScrollContentsWitness::new(
        scroll_owner,
        content_owner,
        ScrollNodeId(scroll_owner),
        contents_clip,
    )
    .unwrap();
    let stack = ConsumedAncestorPropertyStackWitness::new(
        content_owner,
        &[
            ConsumedAncestorProperty::Effect(effect_witness),
            ConsumedAncestorProperty::ScrollContents(scroll_witness),
        ],
    )
    .unwrap();
    let live = PropertyTreeState {
        clip: Some(contents_clip),
        effect: Some(effect.id),
        scroll: Some(ScrollNodeId(scroll_owner)),
        ..Default::default()
    };
    let neutral = PaintRecordingContext {
        recording_owner: Some(content_owner),
        consumed_ancestor_property_stack: Some(stack),
        opacity_authority: PaintOpacityAuthority::NeutralRootEffect(effect.id),
        ..Default::default()
    };
    assert_eq!(
        neutral.project_consumed_ancestor_property(live),
        Some(PropertyTreeState::default())
    );
    assert!(neutral.authorizes_scroll_content_local_owner(content_owner));

    let baked = PaintRecordingContext {
        opacity_authority: PaintOpacityAuthority::Baked,
        ..neutral
    };
    assert_eq!(baked.project_consumed_ancestor_property(live), None);
    assert!(!baked.authorizes_scroll_content_local_owner(content_owner));

    let mut wrong_chain = effect_witness;
    wrong_chain.projected_after = Some(EffectNodeId(scroll_owner));
    assert!(
        ConsumedAncestorPropertyStackWitness::new(
            content_owner,
            &[
                ConsumedAncestorProperty::Effect(wrong_chain),
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ],
        )
        .is_none()
    );
    assert!(
        ConsumedAncestorPropertyStackWitness::new(
            content_owner,
            &[
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
                ConsumedAncestorProperty::Effect(effect_witness),
            ],
        )
        .is_none(),
        "projection capability is sealed outer-to-inner"
    );
}

#[test]
fn consumed_transform_effect_scroll_stack_projects_all_three_layers_exactly() {
    let mut keys = SlotMap::<NodeKey, ()>::with_key();
    let transform_owner = keys.insert(());
    let effect_owner = keys.insert(());
    let scroll_owner = keys.insert(());
    let content_owner = keys.insert(());
    let transform = TransformNodeId(transform_owner);
    let effect = EffectNodeSnapshot {
        id: EffectNodeId(effect_owner),
        owner: effect_owner,
        parent: None,
        opacity: 0.5,
        generation: 9,
    };
    let transform_witness =
        ConsumedAncestorTransformWitness::new(transform_owner, effect_owner, transform).unwrap();
    let effect_witness = ConsumedAncestorEffectWitness::new(
        effect_owner,
        scroll_owner,
        effect,
        Some(effect.id),
        None,
    )
    .unwrap();
    let clip = ClipNodeId {
        owner: scroll_owner,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll_witness = ConsumedAncestorScrollContentsWitness::new(
        scroll_owner,
        content_owner,
        ScrollNodeId(scroll_owner),
        clip,
    )
    .unwrap();
    let stack = ConsumedAncestorPropertyStackWitness::new(
        content_owner,
        &[
            ConsumedAncestorProperty::Transform(transform_witness),
            ConsumedAncestorProperty::Effect(effect_witness),
            ConsumedAncestorProperty::ScrollContents(scroll_witness),
        ],
    )
    .unwrap();
    let live = PropertyTreeState {
        transform: Some(transform),
        effect: Some(effect.id),
        scroll: Some(ScrollNodeId(scroll_owner)),
        clip: Some(clip),
        layout_position: Some(LayoutPositionNodeId(content_owner)),
        visual_offset: Some(VisualOffsetNodeId(content_owner)),
    };
    let context = PaintRecordingContext {
        recording_owner: Some(content_owner),
        consumed_ancestor_property_stack: Some(stack),
        opacity_authority: PaintOpacityAuthority::NeutralRootEffect(effect.id),
        ..Default::default()
    };
    assert_eq!(
        context.project_consumed_ancestor_property(live),
        Some(PropertyTreeState {
            layout_position: live.layout_position,
            visual_offset: live.visual_offset,
            ..Default::default()
        })
    );
    assert!(context.authorizes_scroll_content_local_owner(content_owner));
    let effect_transform_stack = ConsumedAncestorPropertyStackWitness::new(
        content_owner,
        &[
            ConsumedAncestorProperty::Effect(effect_witness),
            ConsumedAncestorProperty::Transform(transform_witness),
            ConsumedAncestorProperty::ScrollContents(scroll_witness),
        ],
    )
    .expect("the typed DAG also admits exact E->T->S order");
    let effect_transform_context = PaintRecordingContext {
        recording_owner: Some(content_owner),
        consumed_ancestor_property_stack: Some(effect_transform_stack),
        opacity_authority: PaintOpacityAuthority::NeutralRootEffect(effect.id),
        ..Default::default()
    };
    assert_eq!(
        effect_transform_context.project_consumed_ancestor_property(live),
        Some(PropertyTreeState {
            layout_position: live.layout_position,
            visual_offset: live.visual_offset,
            ..Default::default()
        })
    );
    for invalid in [
        [
            ConsumedAncestorProperty::Transform(transform_witness),
            ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ConsumedAncestorProperty::Effect(effect_witness),
        ],
        [
            ConsumedAncestorProperty::Effect(effect_witness),
            ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ConsumedAncestorProperty::Transform(transform_witness),
        ],
    ] {
        assert!(ConsumedAncestorPropertyStackWitness::new(content_owner, &invalid).is_none());
    }
}

#[test]
fn scroll_content_local_authority_accepts_only_exact_canonical_stack() {
    let (transform_owner, scroll_owner, content_owner) = keys();
    let transform_witness = ConsumedAncestorTransformWitness::new(
        transform_owner,
        scroll_owner,
        TransformNodeId(transform_owner),
    )
    .unwrap();
    let scroll_witness = ConsumedAncestorScrollContentsWitness::new(
        scroll_owner,
        content_owner,
        ScrollNodeId(scroll_owner),
        ClipNodeId {
            owner: scroll_owner,
            role: ClipNodeRole::ContentsClip,
        },
    )
    .unwrap();
    let stack = ConsumedAncestorPropertyStackWitness::new(
        content_owner,
        &[
            ConsumedAncestorProperty::Transform(transform_witness),
            ConsumedAncestorProperty::ScrollContents(scroll_witness),
        ],
    )
    .unwrap();
    let context = PaintRecordingContext {
        recording_owner: Some(content_owner),
        consumed_ancestor_property_stack: Some(stack),
        ..Default::default()
    };
    assert!(context.authorizes_scroll_content_local_owner(content_owner));

    let wrong_owner = PaintRecordingContext {
        recording_owner: Some(scroll_owner),
        ..context
    };
    assert!(!wrong_owner.authorizes_scroll_content_local_owner(scroll_owner));

    let transform_only = ConsumedAncestorPropertyStackWitness::new(
        content_owner,
        &[ConsumedAncestorProperty::Transform(transform_witness)],
    )
    .unwrap();
    assert!(
        !PaintRecordingContext {
            recording_owner: Some(content_owner),
            consumed_ancestor_property_stack: Some(transform_only),
            ..Default::default()
        }
        .authorizes_scroll_content_local_owner(content_owner)
    );

    let duplicate_scroll = ConsumedAncestorPropertyStackWitness {
        entries: [
            Some(ConsumedAncestorProperty::ScrollContents(scroll_witness)),
            Some(ConsumedAncestorProperty::ScrollContents(scroll_witness)),
            None,
        ],
        len: 2,
        target_owner: content_owner,
    };
    assert!(
        !PaintRecordingContext {
            recording_owner: Some(content_owner),
            consumed_ancestor_property_stack: Some(duplicate_scroll),
            ..Default::default()
        }
        .authorizes_scroll_content_local_owner(content_owner)
    );

    let mut noncanonical = stack;
    noncanonical.entries[1] = Some(ConsumedAncestorProperty::ScrollContents(
        scroll_witness.for_target(scroll_owner),
    ));
    assert!(
        !PaintRecordingContext {
            recording_owner: Some(content_owner),
            consumed_ancestor_property_stack: Some(noncanonical),
            ..Default::default()
        }
        .authorizes_scroll_content_local_owner(content_owner)
    );
}

#[test]
fn wrong_child_boundary_retarget_or_live_transform_cannot_project() {
    let (parent, child, descendant) = keys();
    let transform = TransformNodeId(parent);
    assert!(ConsumedAncestorTransformWitness::new(parent, parent, transform).is_none());
    let witness = ConsumedAncestorTransformWitness::new(parent, child, transform).unwrap();
    let live = PropertyTreeState {
        transform: Some(transform),
        ..Default::default()
    };
    let wrong_target = PaintRecordingContext {
        recording_owner: Some(child),
        consumed_ancestor_property: Some(ConsumedAncestorProperty::Transform(
            witness.for_target(descendant),
        )),
        ..Default::default()
    };
    assert_eq!(wrong_target.project_consumed_ancestor_property(live), None);

    let mismatch = PaintRecordingContext {
        recording_owner: Some(child),
        consumed_ancestor_property: Some(ConsumedAncestorProperty::Transform(
            witness.for_target(child),
        )),
        ..Default::default()
    };
    assert_eq!(
        mismatch.project_consumed_ancestor_property(PropertyTreeState::default()),
        None
    );
}

#[test]
fn same_owner_transform_boundary_is_typed_and_cannot_masquerade_as_ancestor() {
    let (owner, child, descendant) = keys();
    let transform = TransformNodeId(owner);
    assert!(ConsumedAncestorTransformWitness::new(owner, owner, transform).is_none());
    assert!(
        ConsumedSameOwnerTransformBoundaryWitness::new(owner, TransformNodeId(child)).is_none()
    );
    let witness = ConsumedSameOwnerTransformBoundaryWitness::new(owner, transform).unwrap();
    assert!(
        ConsumedAncestorPropertyStackWitness::new(
            child,
            &[ConsumedAncestorProperty::SameOwnerTransformBoundary(
                witness
            )]
        )
        .is_none()
    );

    let live = PropertyTreeState {
        transform: Some(transform),
        ..Default::default()
    };
    let wrong_target = PaintRecordingContext {
        recording_owner: Some(child),
        consumed_ancestor_property: Some(ConsumedAncestorProperty::SameOwnerTransformBoundary(
            witness.for_target(descendant),
        )),
        ..Default::default()
    };
    assert_eq!(wrong_target.project_consumed_ancestor_property(live), None);

    let exact = PaintRecordingContext {
        recording_owner: Some(child),
        consumed_ancestor_property: Some(ConsumedAncestorProperty::SameOwnerTransformBoundary(
            witness.for_target(child),
        )),
        ..Default::default()
    };
    assert_eq!(
        exact.project_consumed_ancestor_property(live),
        Some(PropertyTreeState::default())
    );
}

#[test]
fn deferred_effect_authority_requires_late_phase_clip_and_exact_effect() {
    let (owner, other, _) = keys();
    let stable_id = 0xde_fe01;
    let clip = ClipNodeSnapshot {
        id: ClipNodeId {
            owner,
            role: ClipNodeRole::SelfClip,
        },
        owner,
        parent: None,
        behavior: ClipBehavior::Replace,
        logical_scissor: [0, 0, 40, 30],
        generation: 7,
    };
    let clip_witness =
        PaintDeferredViewportSelfClipWitness::new(owner, stable_id, clip, clip.logical_scissor)
            .unwrap();
    let effect = EffectNodeSnapshot {
        id: EffectNodeId(owner),
        owner,
        parent: None,
        opacity: 0.5,
        generation: 8,
    };
    let witness = PaintDeferredViewportEffectWitness::new(clip_witness, effect).unwrap();
    assert!(
        PaintDeferredViewportEffectWitness::new(
            clip_witness,
            EffectNodeSnapshot {
                id: EffectNodeId(other),
                owner: other,
                ..effect
            }
        )
        .is_none()
    );

    let normal_phase = PaintRecordingContext {
        recording_owner: Some(owner),
        recording_owner_stable_id: Some(stable_id),
        authoritative_self_clip: Some(clip.id),
        deferred_viewport_self_clip: Some(clip_witness),
        opacity_authority: PaintOpacityAuthority::NeutralRootEffect(effect.id),
        ..Default::default()
    };
    assert!(!normal_phase.authorizes_deferred_viewport_effect_for(stable_id, effect.id));

    let late_phase = PaintRecordingContext {
        deferred_viewport_effect: Some(witness),
        ..normal_phase
    };
    assert!(late_phase.authorizes_deferred_viewport_effect_for(stable_id, effect.id));
    assert!(!late_phase.authorizes_deferred_viewport_effect_for(stable_id, EffectNodeId(other)));
}
