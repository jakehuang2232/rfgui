use super::*;

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

const EMPTY: FrozenPropertyState = state(None, None, None, None, None, None);
