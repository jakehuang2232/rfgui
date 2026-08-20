//! Contract tests for the coverage-side legacy detached-subtree authority.
//!
//! The six per-shape admission capabilities that used to sit on
//! `PaintRecordingContext` were split by behavior: the property projection and
//! the clip rebasing stayed with the legacy witness that proves them, and the
//! three component-facing capabilities became recorder-derived behavior flags.
//! These tests pin the two halves of that split — the projection must
//! short-circuit the generic one, and a flag must never outlive the node it was
//! derived for.

use slotmap::SlotMap;

use crate::view::base_component::{
    Rect, ScrollAxisSnapshot, ScrollContentsClipWitness, ScrollbarInteractionWitness,
    ScrollbarOverlayWitness, ScrollbarPaintStateWitness, Size,
};
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, PropertyTreeState, ScrollNodeId,
    ScrollNodeSnapshot,
};
use crate::view::node_arena::NodeKey;
use glam::Vec2;

use super::super::coverage_manifest::{
    project_recorded_node_properties, rebind_legacy_behavior_flags,
};
use super::super::legacy_admission::{
    LegacyTextAreaProjection, PaintLegacyTextAreaCoverageAuthority,
    PaintScrollAtomicProjectionTextAreaRecorderWitness,
    PaintScrollDetachedProjectionSubtreeWitness,
    PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness,
    PaintScrollInteractiveTextAreaSubtreeWitness, PaintScrollTextAreaSubtreeWitness,
};
use super::super::{
    ConsumedAncestorProperty, PaintRecordingContext, PaintScrollContentWitness,
    PaintTextContentSource, PaintTextSelectionSource,
};

const LOCAL_SCISSOR: [u32; 4] = [4, 4, 40, 40];

struct Scene {
    boundary_root: NodeKey,
    content_root: NodeKey,
    text_area_root: NodeKey,
    sibling: NodeKey,
    outer: PaintScrollContentWitness,
    live_contents_clip: ClipNodeSnapshot,
}

fn scene() -> Scene {
    let mut keys = SlotMap::<NodeKey, ()>::with_key();
    let boundary_root = keys.insert(());
    let content_root = keys.insert(());
    let text_area_root = keys.insert(());
    let sibling = keys.insert(());
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    let scroll = ScrollNodeSnapshot {
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
        scrollbar_overlay: ScrollbarOverlayWitness {
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
        },
        contents_clip: ScrollContentsClipWitness::ExactRect([0, 0, 100, 100]),
        generation: 13,
    };
    let outer_clip = ClipNodeSnapshot {
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
    let outer = PaintScrollContentWitness::new(boundary_root, content_root, scroll, outer_clip)
        .expect("canonical scroll-content witness");
    Scene {
        boundary_root,
        content_root,
        text_area_root,
        sibling,
        outer,
        live_contents_clip: ClipNodeSnapshot {
            id: ClipNodeId {
                owner: text_area_root,
                role: ClipNodeRole::ContentsClip,
            },
            owner: text_area_root,
            parent: Some(outer_clip.id),
            logical_scissor: [4, 4, 40, 40],
            behavior: ClipBehavior::Intersect,
            generation: 21,
        },
    }
}

impl Scene {
    fn stable_witness(&self) -> PaintScrollTextAreaSubtreeWitness {
        PaintScrollTextAreaSubtreeWitness::new(
            self.outer,
            self.text_area_root,
            self.live_contents_clip,
            LOCAL_SCISSOR,
            PaintTextContentSource::Glyphs,
        )
        .expect("canonical detached subtree witness")
    }

    fn interactive_witness(&self) -> PaintScrollInteractiveTextAreaSubtreeWitness {
        PaintScrollInteractiveTextAreaSubtreeWitness::new(
            self.outer,
            self.text_area_root,
            self.live_contents_clip,
            LOCAL_SCISSOR,
            PaintTextContentSource::Glyphs,
        )
        .expect("canonical interactive subtree witness")
    }

    fn detached_projection(&self) -> PaintScrollDetachedProjectionSubtreeWitness {
        PaintScrollDetachedProjectionSubtreeWitness::new(
            self.outer,
            self.text_area_root,
            self.live_contents_clip,
            LOCAL_SCISSOR,
        )
        .expect("canonical detached projection witness")
    }

    /// The state a node inside the recorded subtree carries before projection:
    /// the outer scroll pair plus, for the wrapper's descendants, the
    /// TextArea-local contents clip.
    fn live_under_outer_clip(&self) -> PropertyTreeState {
        PropertyTreeState {
            scroll: Some(self.outer.scroll_snapshot().id),
            clip: Some(self.outer.contents_clip_snapshot().id),
            ..Default::default()
        }
    }

    fn live_under_local_clip(&self) -> PropertyTreeState {
        PropertyTreeState {
            scroll: Some(self.outer.scroll_snapshot().id),
            clip: Some(self.live_contents_clip.id),
            ..Default::default()
        }
    }
}

/// The projection dispatch coverage actually performs, exercised through the
/// production seam.
///
/// Every case is set up so that the legacy and the generic projection disagree
/// about the same live state, which is what makes the assertions
/// discriminating: if the seam ever ran both, or ran the wrong one, the
/// returned value would change. Rerouting `Projected` into the generic
/// projection, or letting `Rejected` fall through to it, both break this test.
#[test]
fn projection_dispatch_runs_the_legacy_authority_instead_of_the_generic_one() {
    let scene = scene();
    let witness = scene.stable_witness();
    let authority = PaintLegacyTextAreaCoverageAuthority::Local(witness);
    // The generic capability is genuinely live on this recording: it is what
    // the generalized scroll-content recorder installs alongside the legacy
    // authority.
    let context = PaintRecordingContext {
        recording_owner: Some(scene.content_root),
        consumed_ancestor_property: Some(match scene.outer.consumed_property() {
            ConsumedAncestorProperty::ScrollContents(witness) => {
                ConsumedAncestorProperty::ScrollContents(witness.for_target(scene.content_root))
            }
            other => panic!("scroll content witness expected, got {other:?}"),
        }),
        ..Default::default()
    };

    // `Projected` must not consult the generic projection. Under the
    // TextArea-local clip the generic one rejects outright, so a `Some` result
    // can only have come from the legacy authority.
    let under_local_clip = scene.live_under_local_clip();
    assert_eq!(
        context.project_consumed_ancestor_property(under_local_clip),
        None,
        "the generic projection does not admit the descendant contents clip",
    );
    assert_eq!(
        project_recorded_node_properties(
            &context,
            Some(authority),
            scene.content_root,
            under_local_clip,
        ),
        Some(PropertyTreeState {
            clip: Some(witness.local_contents_clip().id),
            ..Default::default()
        }),
        "a projected node is decided by the legacy authority alone",
    );

    // `Rejected` must not fall through. Here the generic projection *succeeds*
    // — it preserves the transform the exact shape refuses — so a `Some`
    // result would prove a fallback happened.
    let with_transform = PropertyTreeState {
        transform: Some(crate::view::compositor::property_tree::TransformNodeId(
            scene.content_root,
        )),
        ..scene.live_under_outer_clip()
    };
    assert_eq!(
        authority
            .for_target(scene.content_root)
            .project_for(scene.content_root, with_transform),
        LegacyTextAreaProjection::Rejected,
    );
    assert_eq!(
        context.project_consumed_ancestor_property(with_transform),
        Some(PropertyTreeState {
            transform: with_transform.transform,
            ..Default::default()
        }),
        "the generic projection would have admitted this node",
    );
    assert_eq!(
        project_recorded_node_properties(
            &context,
            Some(authority),
            scene.content_root,
            with_transform,
        ),
        None,
        "a rejected exact authority must not be rescued by the generic one",
    );

    // `NoAuthority` is the only path into the generic projection — both when
    // there is no legacy authority at all and when a baked host declines it.
    for authority in [
        None,
        Some(PaintLegacyTextAreaCoverageAuthority::BakedHost(witness)),
    ] {
        assert_eq!(
            project_recorded_node_properties(
                &context,
                authority,
                scene.content_root,
                with_transform,
            ),
            Some(PropertyTreeState {
                transform: with_transform.transform,
                ..Default::default()
            }),
            "without legacy authority the generic projection decides the node",
        );
        assert_eq!(
            project_recorded_node_properties(
                &context,
                authority,
                scene.content_root,
                under_local_clip,
            ),
            None,
            "and it keeps its own exactness when it does",
        );
    }
}

/// Each of these admission inputs is load-bearing, and failing one is a
/// rejection, not an absence of authority.
///
/// Scope: the six canonicality checks the constructor runs on the live contents
/// clip — `id.owner`, `id.role`, `owner`, `parent`, `behavior`, `generation` —
/// plus the paint source, together with retargeting and out-of-shape live
/// state. The constructor's other arguments (`text_area_root`, the outer
/// witness, the local scissor) are not covered here. Neither is the local
/// contents clip: it is *derived* from the live one inside the constructor, so
/// its identity, owner, parent, behavior and generation are not independently
/// reachable without a test-only seam and are guaranteed by the mint.
#[test]
fn tampering_live_clip_or_paint_source_admission_inputs_rejects_without_falling_through_to_generic()
{
    let scene = scene();
    let canonical = scene.live_contents_clip;
    let tampers: [(&str, ClipNodeSnapshot, [u32; 4]); 6] = [
        (
            "clip id owner",
            ClipNodeSnapshot {
                id: ClipNodeId {
                    owner: scene.sibling,
                    role: ClipNodeRole::ContentsClip,
                },
                ..canonical
            },
            LOCAL_SCISSOR,
        ),
        (
            "clip owner",
            ClipNodeSnapshot {
                owner: scene.sibling,
                ..canonical
            },
            LOCAL_SCISSOR,
        ),
        (
            "clip role",
            ClipNodeSnapshot {
                id: ClipNodeId {
                    owner: scene.text_area_root,
                    role: ClipNodeRole::SelfClip,
                },
                ..canonical
            },
            LOCAL_SCISSOR,
        ),
        (
            "clip parent",
            ClipNodeSnapshot {
                parent: None,
                ..canonical
            },
            LOCAL_SCISSOR,
        ),
        (
            "clip behavior",
            ClipNodeSnapshot {
                behavior: ClipBehavior::Replace,
                ..canonical
            },
            LOCAL_SCISSOR,
        ),
        (
            "clip generation",
            ClipNodeSnapshot {
                generation: 0,
                ..canonical
            },
            LOCAL_SCISSOR,
        ),
    ];
    for (field, tampered, local_scissor) in tampers {
        assert!(
            PaintScrollTextAreaSubtreeWitness::new(
                scene.outer,
                scene.text_area_root,
                tampered,
                local_scissor,
                PaintTextContentSource::Glyphs,
            )
            .is_none(),
            "a witness with a tampered {field} must not mint at all",
        );
    }
    assert!(
        PaintScrollTextAreaSubtreeWitness::new(
            scene.outer,
            scene.text_area_root,
            canonical,
            LOCAL_SCISSOR,
            PaintTextContentSource::Selection(PaintTextSelectionSource {
                start_char: 5,
                end_char: 5,
                color_rgba_bits: [1.0f32.to_bits(); 4],
            }),
        )
        .is_none(),
        "a non-canonical paint source must not mint either",
    );

    // A witness that minted canonically and was then retargeted at a node it
    // does not own must reject the projection outright. Returning
    // `NoAuthority` here would hand the node to the generic projection, which
    // is exactly the fall-through the short-circuit forbids.
    let retargeted = PaintLegacyTextAreaCoverageAuthority::Local(scene.stable_witness())
        .for_target(scene.sibling);
    assert_eq!(
        retargeted.project_for(scene.content_root, scene.live_under_outer_clip()),
        LegacyTextAreaProjection::Rejected,
    );

    // Same for live state the exact shape does not admit.
    let authority = PaintLegacyTextAreaCoverageAuthority::Local(scene.stable_witness())
        .for_target(scene.content_root);
    for (field, live) in [
        (
            "transform",
            PropertyTreeState {
                transform: Some(crate::view::compositor::property_tree::TransformNodeId(
                    scene.content_root,
                )),
                ..scene.live_under_outer_clip()
            },
        ),
        (
            "effect",
            PropertyTreeState {
                effect: Some(crate::view::compositor::property_tree::EffectNodeId(
                    scene.content_root,
                )),
                ..scene.live_under_outer_clip()
            },
        ),
        (
            "scroll",
            PropertyTreeState {
                scroll: None,
                ..scene.live_under_outer_clip()
            },
        ),
        (
            "clip",
            PropertyTreeState {
                clip: Some(ClipNodeId {
                    owner: scene.sibling,
                    role: ClipNodeRole::ContentsClip,
                }),
                ..scene.live_under_outer_clip()
            },
        ),
    ] {
        assert_eq!(
            authority.project_for(scene.content_root, live),
            LegacyTextAreaProjection::Rejected,
            "live {field} outside the exact shape must fail closed",
        );
    }
}

/// Tampering two things at once is still one rejection, and a rejected
/// projection leaves both the authority and the live state untouched.
#[test]
fn simultaneous_tamper_rejects_and_rejection_mutates_nothing() {
    let scene = scene();
    let authority = PaintLegacyTextAreaCoverageAuthority::Local(scene.stable_witness())
        .for_target(scene.content_root);
    let live = PropertyTreeState {
        transform: Some(crate::view::compositor::property_tree::TransformNodeId(
            scene.content_root,
        )),
        effect: Some(crate::view::compositor::property_tree::EffectNodeId(
            scene.content_root,
        )),
        scroll: None,
        ..scene.live_under_outer_clip()
    };
    let authority_before = authority;
    let live_before = live;
    assert_eq!(
        authority.project_for(scene.content_root, live),
        LegacyTextAreaProjection::Rejected,
    );
    assert_eq!(authority, authority_before);
    assert_eq!(live, live_before);

    let chain = [
        scene.live_contents_clip,
        scene.outer.contents_clip_snapshot(),
    ];
    let mut wrong_chain = chain;
    wrong_chain.swap(0, 1);
    assert_eq!(
        authority.detach_clip_snapshot(&wrong_chain),
        None,
        "a clip chain the witness did not freeze must not rebase",
    );
    assert_eq!(
        wrong_chain,
        [chain[1], chain[0]],
        "rejection is not a mutation"
    );
    assert_eq!(
        authority.detach_clip_snapshot(&chain),
        Some(vec![scene.stable_witness().local_contents_clip()]),
    );
}

/// A baked host records against unprojected live state. It must report no
/// authority so the generic projection stays in charge, and it must not rebase
/// the recorded clip chain.
#[test]
fn baked_host_authority_leaves_the_generic_projection_in_charge() {
    let scene = scene();
    let authority = PaintLegacyTextAreaCoverageAuthority::BakedHost(scene.stable_witness())
        .for_target(scene.content_root);
    assert_eq!(
        authority.project_for(scene.content_root, scene.live_under_outer_clip()),
        LegacyTextAreaProjection::NoAuthority,
    );
    assert!(!authority.detaches_clip_snapshot());
}

/// The three behavior flags are derived per node and each is bound to a
/// different node of the recorded topology.
#[test]
fn behavior_flags_are_derived_for_exactly_one_node_each() {
    let scene = scene();
    let stable = PaintLegacyTextAreaCoverageAuthority::Local(scene.stable_witness());
    let interactive =
        PaintLegacyTextAreaCoverageAuthority::InteractiveLocal(scene.interactive_witness());
    let focused = PaintLegacyTextAreaCoverageAuthority::AtomicProjectionLocal(
        PaintScrollAtomicProjectionTextAreaRecorderWitness::FocusedAtomicProjectionGlyph(
            PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness::new(
                scene.outer,
                scene.text_area_root,
                scene.live_contents_clip,
                LOCAL_SCISSOR,
            )
            .expect("canonical focused projection witness"),
        ),
    );
    let existing_glyph = PaintLegacyTextAreaCoverageAuthority::AtomicProjectionLocal(
        PaintScrollAtomicProjectionTextAreaRecorderWitness::ExistingAtomicGlyph(
            scene.detached_projection(),
        ),
    );

    for node in [
        scene.boundary_root,
        scene.content_root,
        scene.text_area_root,
        scene.sibling,
    ] {
        let bound = stable.for_target(node);
        assert!(
            bound.authorizes_scroll_content_local_owner(node),
            "the detached local basis applies to every node coverage walks",
        );
        assert_eq!(
            bound.authorizes_descendant_contents_clip(node),
            node == scene.content_root,
            "only the content root may keep a descendant contents clip",
        );
        assert!(
            !bound.suppresses_resident_caret(node),
            "the stable grammar has no resident raster to defer the caret to",
        );

        assert_eq!(
            interactive.for_target(node).suppresses_resident_caret(node),
            node == scene.text_area_root,
            "caret suppression belongs to the TextArea root, not the wrapper",
        );
        assert_eq!(
            focused.for_target(node).suppresses_resident_caret(node),
            node == scene.text_area_root,
            "the focused projection root owns the resident caret",
        );
        assert!(
            !existing_glyph
                .for_target(node)
                .suppresses_resident_caret(node),
            "a non-focused atomic projection has no resident caret to defer to",
        );
    }
}

/// A flag that survived from a previous node cannot authorize the next one.
///
/// Coverage clears all three after the component hook and re-derives them, but
/// the capability predicates do not rely on that alone: each re-checks the
/// recording owner, so a `true` inherited by context copy — or minted by a
/// component hook — is inert for every node except the one coverage is
/// currently recording, and for that node the flag has already been recomputed.
#[test]
fn a_stale_or_hook_minted_flag_cannot_authorize_another_node() {
    let scene = scene();
    let leaked = PaintRecordingContext {
        recording_owner: Some(scene.content_root),
        recording_owner_stable_id: Some(0x5a_0001),
        scroll_content_local_owner: true,
        descendant_contents_clip: true,
        resident_caret_suppressed: true,
        ..Default::default()
    };
    assert!(!leaked.authorizes_scroll_content_local_owner(scene.sibling));
    assert!(!leaked.suppresses_resident_caret(scene.sibling));
    assert!(!leaked.authorizes_descendant_contents_clip(0x5a_0002));

    let hook_minted = PaintRecordingContext {
        scroll_content_local_owner: true,
        descendant_contents_clip: true,
        resident_caret_suppressed: true,
        ..Default::default()
    };
    assert!(!hook_minted.authorizes_scroll_content_local_owner(scene.content_root));
    assert!(!hook_minted.suppresses_resident_caret(scene.content_root));
    assert!(
        !hook_minted.authorizes_descendant_contents_clip(0x5a_0001),
        "a flag without a recording owner is not ambient authority",
    );
}

/// The walker clears all three flags before re-deriving them, so no node
/// inherits another node's authority.
///
/// This is the seam a child context passes through. A child copies its
/// parent's context by value and the walker rebinds `recording_owner` to the
/// child, so the owner recheck inside each capability predicate cannot catch a
/// parent-to-child leak on its own — only the unconditional clear can.
#[test]
fn rebinding_clears_inherited_flags_before_deriving_this_nodes_authority() {
    let scene = scene();
    let authority = PaintLegacyTextAreaCoverageAuthority::Local(scene.stable_witness());
    let inherited = PaintRecordingContext {
        scroll_content_local_owner: true,
        descendant_contents_clip: true,
        resident_caret_suppressed: true,
        ..Default::default()
    };

    // The content root really is authorized for two of the three, so the
    // derivation is not trivially clearing everything.
    let mut at_content_root = inherited;
    at_content_root.recording_owner = Some(scene.content_root);
    rebind_legacy_behavior_flags(&mut at_content_root, Some(authority), scene.content_root);
    assert!(at_content_root.scroll_content_local_owner);
    assert!(at_content_root.descendant_contents_clip);
    assert!(!at_content_root.resident_caret_suppressed);

    // A sibling of the content root inherits `descendant_contents_clip: true`
    // by context copy and must lose it, even though it is the recording owner.
    let mut at_sibling = at_content_root;
    at_sibling.recording_owner = Some(scene.sibling);
    rebind_legacy_behavior_flags(&mut at_sibling, Some(authority), scene.sibling);
    assert!(
        !at_sibling.descendant_contents_clip,
        "only the content root may keep a descendant contents clip",
    );
    assert!(!at_sibling.resident_caret_suppressed);

    // With no authority at all, every inherited or hook-minted flag is cleared.
    let mut without_authority = inherited;
    without_authority.recording_owner = Some(scene.content_root);
    rebind_legacy_behavior_flags(&mut without_authority, None, scene.content_root);
    assert!(!without_authority.scroll_content_local_owner);
    assert!(!without_authority.descendant_contents_clip);
    assert!(!without_authority.resident_caret_suppressed);
}
