use super::*;

#[test]
fn ambient_or_wrong_owner_transform_witness_cannot_escape_surface_policy() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let root_stable_id = arena.get(root).expect("root").element.stable_id();
    let ambient = super::super::PaintRecordingContext {
        recording_owner: Some(root),
        recording_owner_stable_id: Some(root_stable_id),
        transform_surface: Some(PaintTransformSurfaceWitness::canonical_root(root)),
        ..Default::default()
    };
    let manifest = super::super::coverage_manifest::record_coverage_manifest_with_context(
        &arena,
        &[root],
        false,
        true,
        super::super::CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
        ambient,
        None,
        &Default::default(),
    );
    assert!(matches!(
        manifest.items.as_slice(),
        [super::super::PaintCoverageItem::LegacyBoundary {
            reason: super::super::LegacyPaintReason::Transform,
            ..
        }]
    ));

    let child = arena
        .get(root)
        .expect("root")
        .element
        .children()
        .first()
        .copied()
        .expect("child");
    let wrong_owner = super::super::PaintRecordingContext {
        recording_owner: Some(root),
        recording_owner_stable_id: Some(root_stable_id),
        transform_surface: Some(
            PaintTransformSurfaceWitness::canonical_root(root).for_target(child),
        ),
        ..Default::default()
    };
    assert!(!wrong_owner.authorizes_transform_surface_root(root_stable_id));
    assert!(!wrong_owner.authorizes_transform_surface_owner(Some(TransformNodeId(root))));
}

use crate::view::base_component::{DirtyPassMask, EventTarget, Size};
fn legacy_recording_scroll_fixture(
    hovered: bool,
    shadow_blur_radius: f32,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        81_001, 0.0, 0.0, 100.0, 80.0,
    ))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(
        81_002, 0.0, -20.0, 100.0, 300.0,
    ))));
    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        root_element.apply_style(style);
        root_element.layout_state.content_size = Size {
            width: 100.0,
            height: 300.0,
        };
        root_element.set_scroll_offset((0.0, 20.0));
        root_element.set_scrollbar_shadow_blur_radius(shadow_blur_radius);
        root_element.set_hovered(hovered);
        root_element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena
        .get_mut(child)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "unexpected offset fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, properties, generations)
}

#[test]
fn general_recorder_still_rejects_scroll_host_without_owned_witness() {
    let (arena, root, _child, properties, generations) =
        legacy_recording_scroll_fixture(false, 3.0);
    let error = record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::StrictPlan,
    )
    .unwrap_err();
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        FrameArtifactFallbackReason::LegacyBoundary(
            LegacyPaintReason::ScrollContainer | LegacyPaintReason::ChildClip
        )
    )));
}
