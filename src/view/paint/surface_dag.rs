#![allow(dead_code)] // C2a type seam; C2b becomes the first DAG constructor.

use rustc_hash::FxHashMap;

use crate::view::{
    compositor::property_tree::{
        ClipNodeId, ClipNodeRole, EffectNodeId, ScrollNodeId, TransformNodeId,
    },
    node_arena::NodeKey,
};

use super::{
    ArtifactCursor, ArtifactOwnerGraph, ArtifactSceneTarget, PaintArtifact, PropertySnapshotGraph,
    TransitionError, artifact_cursors,
};

/// Explicit C2 layerization input. The first policy preserves every authored
/// property boundary; adding another policy is an enum extension rather than
/// a hidden global behavior change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerizationPolicy {
    PreservePropertyBoundaries,
}

/// Durable surface-producing property families.
///
/// Clip, layout-position, and visual-offset identities remain coordinate-space
/// obligations and never become independent surface kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SurfaceDagNodeKind {
    Transform(TransformNodeId),
    Effect(EffectNodeId),
    ScrollContent {
        scroll: ScrollNodeId,
        contents_clip: ClipNodeId,
    },
}

/// Dense artifact-local surface identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceDagNodeId(u32);

/// Scene-root identity derived from one validated artifact owner graph.
/// The ordinal cannot be constructed by a consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceDagSceneRootId(u32);

impl SurfaceDagSceneRootId {
    fn from_scene_target(target: ArtifactSceneTarget) -> Self {
        Self(target.scene_root_ordinal())
    }
}

/// Generic receiver identity for every surface kind.
///
/// There is deliberately no scroll-content receiver grammar variant. A
/// detached scroll-content surface receives another generic surface id just
/// like transform and effect surfaces do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SurfaceDagTargetId {
    SceneRoot(SurfaceDagSceneRootId),
    Surface(SurfaceDagNodeId),
}

/// The clip-space obligation carried by a detached scroll-content surface.
/// `outer_clip` is the contents clip's immediate clip-forest parent;
/// `local_clip` is rebased into the detached content's coordinate space in
/// C2b. Receiver-space equivalence is not implied by this C2a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagClipRebase {
    outer_clip: Option<ClipNodeId>,
    local_clip: ClipNodeId,
}

impl SurfaceDagClipRebase {
    pub(crate) fn outer_clip(self) -> Option<ClipNodeId> {
        self.outer_clip
    }

    pub(crate) fn local_clip(self) -> ClipNodeId {
        self.local_clip
    }
}

/// Final C2 node shape. C2a freezes the type seam; C2b supplies receiver
/// reconstruction and the node sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagNode {
    id: SurfaceDagNodeId,
    target: NodeKey,
    cursor: ArtifactCursor,
    kind: SurfaceDagNodeKind,
    receiver: SurfaceDagTargetId,
    clip_rebase: Option<SurfaceDagClipRebase>,
}

/// One artifact-derived surface boundary before receiver reconstruction.
///
/// `target` is always the generic artifact owner identity. Candidate order is
/// owner-store order, then Transform -> Effect -> ScrollContent for boundaries
/// co-located on one owner. No arena lookup or planner grammar is available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSurfaceCandidate {
    scene_target: ArtifactSceneTarget,
    cursor: ArtifactCursor,
    kind: SurfaceDagNodeKind,
    clip_rebase: Option<SurfaceDagClipRebase>,
}

impl ArtifactSurfaceCandidate {
    pub(crate) fn target(self) -> NodeKey {
        self.scene_target.target()
    }

    pub(crate) fn cursor(self) -> ArtifactCursor {
        self.cursor
    }

    pub(crate) fn kind(self) -> SurfaceDagNodeKind {
        self.kind
    }

    pub(crate) fn clip_rebase(self) -> Option<SurfaceDagClipRebase> {
        self.clip_rebase
    }

    pub(crate) fn scene_root_receiver(self) -> SurfaceDagTargetId {
        SurfaceDagTargetId::SceneRoot(SurfaceDagSceneRootId::from_scene_target(self.scene_target))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceDagError {
    Transition(TransitionError),
    MissingScrollContentsClip {
        scroll: ScrollNodeId,
        expected: ClipNodeId,
    },
}

impl From<TransitionError> for SurfaceDagError {
    fn from(error: TransitionError) -> Self {
        Self::Transition(error)
    }
}

/// Derives surface-producing candidates exclusively from a closed artifact.
///
/// This is not receiver reconstruction. It freezes the C2 mapping and the
/// clip-rebase obligation while leaving the C0a/C0c nested-scroll projected
/// state decision explicit for C2b.
pub(crate) fn derive_artifact_surface_candidates(
    artifact: &PaintArtifact,
    policy: LayerizationPolicy,
) -> Result<Vec<ArtifactSurfaceCandidate>, SurfaceDagError> {
    match policy {
        LayerizationPolicy::PreservePropertyBoundaries => {}
    }

    let snapshots = PropertySnapshotGraph::try_from_artifact(artifact)?;
    let owners = ArtifactOwnerGraph::try_from_artifact(artifact)?;
    let cursors = artifact_cursors(artifact)?;

    let transforms = artifact
        .transform_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.id))
        .collect::<FxHashMap<_, _>>();
    let effects = artifact
        .effect_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.id))
        .collect::<FxHashMap<_, _>>();
    let scrolls = artifact
        .scroll_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.id))
        .collect::<FxHashMap<_, _>>();

    let mut candidates = Vec::new();
    for owner in artifact.owner_nodes.iter().map(|snapshot| snapshot.owner) {
        let scene_target = owners.scene_target(owner)?;
        let cursor = owners.cursor_for_target(owner, &cursors)?;
        if let Some(transform) = transforms.get(&owner).copied() {
            candidates.push(ArtifactSurfaceCandidate {
                scene_target,
                cursor,
                kind: SurfaceDagNodeKind::Transform(transform),
                clip_rebase: None,
            });
        }
        if let Some(effect) = effects.get(&owner).copied() {
            candidates.push(ArtifactSurfaceCandidate {
                scene_target,
                cursor,
                kind: SurfaceDagNodeKind::Effect(effect),
                clip_rebase: None,
            });
        }
        if let Some(scroll) = scrolls.get(&owner).copied() {
            // Scroll snapshots do not carry their contents-clip id. The
            // artifact contract derives it from the canonical owner/role
            // convention, then fails closed if that exact clip is absent.
            let contents_clip = ClipNodeId {
                owner,
                role: ClipNodeRole::ContentsClip,
            };
            let outer_clip = snapshots.clip_parent(contents_clip).map_err(|_| {
                SurfaceDagError::MissingScrollContentsClip {
                    scroll,
                    expected: contents_clip,
                }
            })?;
            candidates.push(ArtifactSurfaceCandidate {
                scene_target,
                cursor,
                kind: SurfaceDagNodeKind::ScrollContent {
                    scroll,
                    contents_clip,
                },
                clip_rebase: Some(SurfaceDagClipRebase {
                    outer_clip,
                    local_clip: contents_clip,
                }),
            });
        }
    }
    Ok(candidates)
}
