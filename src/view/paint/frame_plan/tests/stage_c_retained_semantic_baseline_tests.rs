use super::*;
use crate::view::paint::{
    ArtifactTransitionRequest, ClassifiedTransitionEvent, classify_artifact_transition_sequence,
};

const ROOT: u64 = 0xb4_0001;
const INNER_A: u64 = 0xb4_0002;
const INNER_B: u64 = 0xb4_0003;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrozenClipId {
    owner: u64,
    role: ClipNodeRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrozenPropertyState {
    transform: Option<u64>,
    clip: Option<FrozenClipId>,
    effect: Option<u64>,
    scroll: Option<u64>,
    layout_position: Option<u64>,
    visual_offset: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrozenDimensionTransition<Id> {
    from: Option<Id>,
    to: Option<Id>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrozenPropertyTransition {
    transform: FrozenDimensionTransition<u64>,
    clip: FrozenDimensionTransition<FrozenClipId>,
    effect: FrozenDimensionTransition<u64>,
    scroll: FrozenDimensionTransition<u64>,
    layout_position: FrozenDimensionTransition<u64>,
    visual_offset: FrozenDimensionTransition<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrozenTransitionEvent {
    scene_root_ordinal: u32,
    // The old planner has no artifact-local cursor. C1 must additionally
    // validate its ArtifactCursor against artifact traversal while preserving
    // this planner sequence exactly; the two domains must not be conflated.
    planner_sequence_ordinal: u32,
    owner: u64,
    transition: FrozenPropertyTransition,
}

const fn contents_clip(owner: u64) -> FrozenClipId {
    FrozenClipId {
        owner,
        role: ClipNodeRole::ContentsClip,
    }
}

const fn state(
    transform: Option<u64>,
    clip: Option<FrozenClipId>,
    effect: Option<u64>,
    scroll: Option<u64>,
    layout_position: Option<u64>,
    visual_offset: Option<u64>,
) -> FrozenPropertyState {
    FrozenPropertyState {
        transform,
        clip,
        effect,
        scroll,
        layout_position,
        visual_offset,
    }
}

const fn transition(
    from: FrozenPropertyState,
    to: FrozenPropertyState,
) -> FrozenPropertyTransition {
    FrozenPropertyTransition {
        transform: FrozenDimensionTransition {
            from: from.transform,
            to: to.transform,
        },
        clip: FrozenDimensionTransition {
            from: from.clip,
            to: to.clip,
        },
        effect: FrozenDimensionTransition {
            from: from.effect,
            to: to.effect,
        },
        scroll: FrozenDimensionTransition {
            from: from.scroll,
            to: to.scroll,
        },
        layout_position: FrozenDimensionTransition {
            from: from.layout_position,
            to: to.layout_position,
        },
        visual_offset: FrozenDimensionTransition {
            from: from.visual_offset,
            to: to.visual_offset,
        },
    }
}

const fn event(
    planner_sequence_ordinal: u32,
    owner: u64,
    from: FrozenPropertyState,
    to: FrozenPropertyState,
) -> FrozenTransitionEvent {
    FrozenTransitionEvent {
        scene_root_ordinal: 0,
        planner_sequence_ordinal,
        owner,
        transition: transition(from, to),
    }
}

fn freeze_state(arena: &NodeArena, state: PropertyTreeState) -> FrozenPropertyState {
    let stable_id = |owner: NodeKey| {
        arena
            .get(owner)
            .expect("property-state owner must remain reachable")
            .element
            .stable_id()
    };
    FrozenPropertyState {
        transform: state.transform.map(|id| stable_id(id.0)),
        clip: state.clip.map(|id| FrozenClipId {
            owner: stable_id(id.owner),
            role: id.role,
        }),
        effect: state.effect.map(|id| stable_id(id.0)),
        scroll: state.scroll.map(|id| stable_id(id.0)),
        layout_position: state.layout_position.map(|id| stable_id(id.0)),
        visual_offset: state.visual_offset.map(|id| stable_id(id.0)),
    }
}

fn collect_state_owners(state: PropertyTreeState, owners: &mut FxHashSet<NodeKey>) {
    owners.extend(state.transform.map(|id| id.0));
    owners.extend(state.clip.map(|id| id.owner));
    owners.extend(state.effect.map(|id| id.0));
    owners.extend(state.scroll.map(|id| id.0));
    owners.extend(state.layout_position.map(|id| id.0));
    owners.extend(state.visual_offset.map(|id| id.0));
}

fn assert_oracle_stable_ids_are_injective(arena: &NodeArena, dag: &PropertyBoundaryDag) {
    let mut owners = FxHashSet::default();
    for node in &dag.nodes {
        owners.insert(node.owner);
        collect_state_owners(node.consumption.expected_before, &mut owners);
        collect_state_owners(node.consumption.projected_after, &mut owners);
    }
    let stable_ids = owners
        .iter()
        .map(|owner| {
            arena
                .get(*owner)
                .expect("oracle owner must remain reachable")
                .element
                .stable_id()
        })
        .collect::<FxHashSet<_>>();
    assert_eq!(
        owners.len(),
        stable_ids.len(),
        "distinct fixture owners must have distinct oracle stable ids",
    );
}

fn freeze_transition_events(
    arena: &NodeArena,
    dag: &PropertyBoundaryDag,
) -> Vec<FrozenTransitionEvent> {
    dag.nodes
        .iter()
        .enumerate()
        .map(|(ordinal, node)| FrozenTransitionEvent {
            scene_root_ordinal: node.scene_root_ordinal,
            planner_sequence_ordinal: u32::try_from(ordinal).expect("test sequence ordinal"),
            owner: arena
                .get(node.owner)
                .expect("boundary owner must remain reachable")
                .element
                .stable_id(),
            transition: transition(
                freeze_state(arena, node.consumption.expected_before),
                freeze_state(arena, node.consumption.projected_after),
            ),
        })
        .collect()
}

fn freeze_classified_transition_event(
    arena: &NodeArena,
    planner_sequence_ordinal: u32,
    event: ClassifiedTransitionEvent,
) -> FrozenTransitionEvent {
    let classified = event.transition();
    let from = PropertyTreeState {
        transform: classified.transform.from,
        clip: classified.clip.from,
        effect: classified.effect.from,
        scroll: classified.scroll.from,
        layout_position: classified.layout_position.from,
        visual_offset: classified.visual_offset.from,
    };
    let to = PropertyTreeState {
        transform: classified.transform.to,
        clip: classified.clip.to,
        effect: classified.effect.to,
        scroll: classified.scroll.to,
        layout_position: classified.layout_position.to,
        visual_offset: classified.visual_offset.to,
    };
    FrozenTransitionEvent {
        scene_root_ordinal: event.scene_root_ordinal(),
        planner_sequence_ordinal,
        owner: arena
            .get(event.target())
            .expect("classified target must remain reachable")
            .element
            .stable_id(),
        transition: transition(freeze_state(arena, from), freeze_state(arena, to)),
    }
}

fn owner_is_within(arena: &NodeArena, owner: NodeKey, target: NodeKey) -> bool {
    let mut cursor = Some(owner);
    while let Some(owner) = cursor {
        if owner == target {
            return true;
        }
        cursor = arena.parent_of(owner);
    }
    false
}

fn assert_classified_cursors_match_artifact_traversal(
    arena: &NodeArena,
    artifact: &PaintArtifact,
    dag: &PropertyBoundaryDag,
    events: &[ClassifiedTransitionEvent],
) {
    assert_eq!(events.len(), dag.nodes.len());
    for (node, event) in dag.nodes.iter().zip(events) {
        let expected_chunk = artifact
            .chunks
            .iter()
            .position(|chunk| owner_is_within(arena, chunk.owner, node.owner))
            .expect("every classified boundary owns an artifact chunk subtree");
        assert_eq!(event.cursor().chunk_index(), expected_chunk);
        assert_eq!(
            event.cursor().op_index(),
            artifact.chunks[expected_chunk].op_range.start,
        );
    }
}

fn production_fixture_context() -> TransformSurfacePlanContext {
    let ui_context = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let context = TransformSurfacePlanContext::new(
        ui_context.paint_offset(),
        ui_context.graphics_pass_context().scissor_rect,
    );
    assert_eq!(context, TransformSurfacePlanContext::default());
    context
}

const EMPTY: FrozenPropertyState = state(None, None, None, None, None, None);

/// Test-only old-planner oracle frozen at the start of Stage C.
///
/// A terminal grammar label is deliberately insufficient: two pairs of these
/// fixtures share a label. The ordered event stream additionally freezes the
/// boundary owner and all six property dimensions, so co-location remains
/// distinguishable while neutral wrappers remain transparent. Production
/// builds this planner context from the live paint offset and scissor. This
/// corpus uses the zero-offset, unclipped production test context under which
/// its receiver insertion seals were recorded; a different context is not the
/// same frozen input and may legitimately fail closed.
#[test]
fn stage_c_nine_scroll_interleave_semantics_are_frozen_before_v2() {
    let cases: &[(
        ScrollInterleaveFixtureShape,
        PropertyBoundaryDagGrammar,
        &[FrozenTransitionEvent],
    )] = &[
        (
            ScrollInterleaveFixtureShape::FrameRootScroll,
            PropertyBoundaryDagGrammar::FrameRootScroll,
            &[event(
                0,
                ROOT,
                state(
                    None,
                    Some(contents_clip(ROOT)),
                    None,
                    Some(ROOT),
                    None,
                    None,
                ),
                EMPTY,
            )],
        ),
        (
            ScrollInterleaveFixtureShape::TransformScroll,
            PropertyBoundaryDagGrammar::TransformScroll,
            &[
                event(
                    0,
                    ROOT,
                    state(
                        Some(ROOT),
                        Some(contents_clip(INNER_A)),
                        None,
                        Some(INNER_A),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                    state(
                        None,
                        Some(contents_clip(INNER_A)),
                        None,
                        Some(INNER_A),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                ),
                event(
                    1,
                    INNER_A,
                    state(
                        None,
                        Some(contents_clip(INNER_A)),
                        None,
                        Some(INNER_A),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                    state(None, None, None, None, Some(ROOT), Some(ROOT)),
                ),
            ],
        ),
        (
            ScrollInterleaveFixtureShape::EffectScroll,
            PropertyBoundaryDagGrammar::EffectScroll,
            &[
                event(
                    0,
                    ROOT,
                    state(
                        None,
                        Some(contents_clip(INNER_A)),
                        Some(ROOT),
                        Some(INNER_A),
                        None,
                        None,
                    ),
                    state(
                        None,
                        Some(contents_clip(INNER_A)),
                        None,
                        Some(INNER_A),
                        None,
                        None,
                    ),
                ),
                event(
                    1,
                    INNER_A,
                    state(
                        None,
                        Some(contents_clip(INNER_A)),
                        None,
                        Some(INNER_A),
                        None,
                        None,
                    ),
                    EMPTY,
                ),
            ],
        ),
        (
            ScrollInterleaveFixtureShape::TransformEffectScroll,
            PropertyBoundaryDagGrammar::TransformEffectScroll,
            &[
                event(
                    0,
                    ROOT,
                    state(
                        Some(ROOT),
                        Some(contents_clip(INNER_B)),
                        Some(INNER_A),
                        Some(INNER_B),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        Some(INNER_A),
                        Some(INNER_B),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                ),
                event(
                    1,
                    INNER_A,
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        Some(INNER_A),
                        Some(INNER_B),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                ),
                event(
                    2,
                    INNER_B,
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                    state(None, None, None, None, Some(ROOT), Some(ROOT)),
                ),
            ],
        ),
        (
            ScrollInterleaveFixtureShape::EffectTransformScroll,
            PropertyBoundaryDagGrammar::EffectTransformScroll,
            &[
                event(
                    0,
                    ROOT,
                    state(
                        Some(INNER_A),
                        Some(contents_clip(INNER_B)),
                        Some(ROOT),
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                    state(
                        Some(INNER_A),
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                ),
                event(
                    1,
                    INNER_A,
                    state(
                        Some(INNER_A),
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                ),
                event(
                    2,
                    INNER_B,
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                    state(None, None, None, None, Some(INNER_A), Some(INNER_A)),
                ),
            ],
        ),
        (
            ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll,
            PropertyBoundaryDagGrammar::EffectTransformScroll,
            &[
                event(
                    0,
                    ROOT,
                    state(
                        Some(INNER_A),
                        Some(contents_clip(INNER_B)),
                        Some(ROOT),
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                    state(
                        Some(INNER_A),
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                ),
                event(
                    1,
                    INNER_A,
                    state(
                        Some(INNER_A),
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                ),
                event(
                    2,
                    INNER_B,
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        Some(INNER_A),
                        Some(INNER_A),
                    ),
                    state(None, None, None, None, Some(INNER_A), Some(INNER_A)),
                ),
            ],
        ),
        (
            ScrollInterleaveFixtureShape::CoLocatedTransformScroll,
            PropertyBoundaryDagGrammar::TransformScroll,
            &[
                event(
                    0,
                    ROOT,
                    state(
                        Some(ROOT),
                        Some(contents_clip(ROOT)),
                        None,
                        Some(ROOT),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                    state(
                        None,
                        Some(contents_clip(ROOT)),
                        None,
                        Some(ROOT),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                ),
                event(
                    1,
                    ROOT,
                    state(
                        None,
                        Some(contents_clip(ROOT)),
                        None,
                        Some(ROOT),
                        Some(ROOT),
                        Some(ROOT),
                    ),
                    state(None, None, None, None, Some(ROOT), Some(ROOT)),
                ),
            ],
        ),
        (
            ScrollInterleaveFixtureShape::NestedScroll,
            PropertyBoundaryDagGrammar::NestedScrollChain,
            &[
                event(
                    0,
                    INNER_A,
                    state(
                        None,
                        Some(contents_clip(INNER_A)),
                        None,
                        Some(INNER_A),
                        None,
                        None,
                    ),
                    EMPTY,
                ),
                event(
                    1,
                    INNER_B,
                    state(
                        None,
                        Some(contents_clip(INNER_B)),
                        None,
                        Some(INNER_B),
                        None,
                        None,
                    ),
                    state(None, Some(contents_clip(INNER_A)), None, None, None, None),
                ),
            ],
        ),
    ];

    let production_fixture_context = production_fixture_context();
    let mut transform_scroll_events = None;
    let mut co_located_transform_scroll_events = None;
    let mut effect_transform_scroll_events = None;
    let mut neutral_effect_transform_scroll_events = None;

    for &(shape, expected_grammar, expected_events) in cases {
        let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
        let plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            production_fixture_context,
        )
        .expect("Stage C retained semantic baseline must remain plannable");
        let scaffold = plan
            .property_scroll_planning_scaffold()
            .expect("property-scroll planning scaffold");
        assert_eq!(
            scaffold.boundary_dag.existing_grammar(),
            Some(expected_grammar),
        );
        assert_oracle_stable_ids_are_injective(&arena, &scaffold.boundary_dag);
        let planner_events = freeze_transition_events(&arena, &scaffold.boundary_dag);
        assert_eq!(planner_events, expected_events);

        let artifact =
            stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
                .expect("C1 structural artifact must close over the synced property trees");
        let requests = scaffold
            .boundary_dag
            .nodes
            .iter()
            .map(|node| {
                ArtifactTransitionRequest::new(
                    node.owner,
                    node.consumption.expected_before,
                    node.consumption.projected_after,
                )
            })
            .collect::<Vec<_>>();
        let classified = classify_artifact_transition_sequence(&artifact, &requests)
            .expect("C1 artifact transition classification");
        assert_classified_cursors_match_artifact_traversal(
            &arena,
            &artifact,
            &scaffold.boundary_dag,
            &classified,
        );
        let actual_events = classified
            .into_iter()
            .enumerate()
            .map(|(ordinal, event)| {
                freeze_classified_transition_event(
                    &arena,
                    u32::try_from(ordinal).expect("classified sequence ordinal"),
                    event,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual_events, planner_events);
        match shape {
            ScrollInterleaveFixtureShape::TransformScroll => {
                transform_scroll_events = Some(actual_events)
            }
            ScrollInterleaveFixtureShape::CoLocatedTransformScroll => {
                co_located_transform_scroll_events = Some(actual_events)
            }
            ScrollInterleaveFixtureShape::EffectTransformScroll => {
                effect_transform_scroll_events = Some(actual_events)
            }
            ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll => {
                neutral_effect_transform_scroll_events = Some(actual_events)
            }
            ScrollInterleaveFixtureShape::FrameRootScroll
            | ScrollInterleaveFixtureShape::EffectScroll
            | ScrollInterleaveFixtureShape::TransformEffectScroll
            | ScrollInterleaveFixtureShape::ScrollTransform
            | ScrollInterleaveFixtureShape::NestedScroll => {}
        }
    }
    assert_eq!(
        effect_transform_scroll_events.expect("effect-transform-scroll fixture must be measured"),
        neutral_effect_transform_scroll_events
            .expect("neutral effect-transform-scroll fixture must be measured"),
        "neutral wrappers are transparent",
    );
    assert_ne!(
        transform_scroll_events.expect("transform-scroll fixture must be measured"),
        co_located_transform_scroll_events
            .expect("co-located transform-scroll fixture must be measured"),
        "co-location is a distinct topology",
    );
}

/// `ScrollTransform` is handled by a separate exact retained authority. The
/// old property-boundary planner's string-bearing rejection is frozen only as
/// a legacy differential baseline; C1 must define a closed typed taxonomy.
#[test]
fn stage_c_scroll_transform_typed_rejection_is_frozen_before_v2() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let transform = arena.children_of(root)[0];
    let error = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        production_fixture_context(),
    )
    .expect_err("the old property-boundary planner must reject S -> T");
    assert_eq!(
        error.reasons,
        [
            FramePaintPlanRejection::UnsupportedPropertyInterleave(
                transform,
                "transform-only-under-scroll-ancestor",
            ),
            FramePaintPlanRejection::UnsupportedPropertyInterleave(
                root,
                "root-boundary-schedule-unsupported",
            ),
        ],
    );
}
