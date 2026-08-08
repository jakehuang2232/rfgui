use super::*;

fn text_area_owner(arena: &NodeArena, root: NodeKey) -> NodeKey {
    let root_children = arena.children_of(root);
    let [wrapper] = root_children.as_slice() else {
        panic!("Stage A fixture root must own one content wrapper")
    };
    let wrapper_children = arena.children_of(*wrapper);
    let [text_area] = wrapper_children.as_slice() else {
        panic!("Stage A fixture wrapper must own one TextArea")
    };
    *text_area
}

fn sync_scene(arena: &NodeArena, roots: &[NodeKey]) -> (PropertyTrees, PaintGenerationTracker) {
    synced_paint_state(arena, roots)
}

fn interactive_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, roots, _, _) = prepared_scroll_text_area_scene();
    let text_area = text_area_owner(&arena, roots[0]);
    {
        let mut node = arena.get_mut(text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = true;
        text_area.caret_visible = true;
        text_area.cursor_char = 3;
    }
    let (properties, generations) = sync_scene(&arena, &roots);
    (arena, roots, properties, generations)
}

fn projection_scene(
    focused: bool,
    selection: bool,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, roots, _, _) = prepared_focused_atomic_projection_scroll_text_area_scene();
    let text_area = text_area_owner(&arena, roots[0]);
    {
        let mut node = arena.get_mut(text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = focused;
        text_area.caret_visible = focused;
        if selection {
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(6);
        }
    }
    let (properties, generations) = sync_scene(&arena, &roots);
    (arena, roots, properties, generations)
}

fn assert_property_scroll_authority_and_typed_rejection(
    name: &str,
    mut arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
) {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let accepted =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let AutoAuthorityDecision::PropertyScrollScene { scene, .. } = accepted else {
        panic!("{name}: Stage A must preserve PropertyScrollScene authority")
    };
    assert_eq!(
        scene.receiver_roots_for_test().as_slice(),
        roots.as_slice(),
        "{name}: Stage A must preserve the accepted authority owner",
    );

    let text_area = text_area_owner(&arena, roots[0]);
    let replacement = *arena
        .children_of(text_area)
        .first()
        .expect("Stage A TextArea fixture must own generated paint content");
    let bounds = arena
        .get(replacement)
        .expect("generated TextArea child")
        .element
        .box_model_snapshot();
    *arena.get_mut(replacement).unwrap().element = Box::new(UnknownOverlayHost {
        id: bounds.node_id,
        bounds,
    });
    arena.refresh_stable_id_index();
    arena.refresh_subtree_dirty_cache(roots[0]);
    let (rejected_properties, rejected_generations) = sync_scene(&arena, &roots);
    let rejected = select_retained_auto_authority(
        &arena,
        &roots,
        &rejected_properties,
        &rejected_generations,
        &ctx,
        true,
    );
    let AutoAuthorityDecision::Legacy { trace } = rejected else {
        panic!("{name}: incomplete scroll snapshot must preserve whole-frame Legacy fallback")
    };
    let owner_attributed_reasons = trace.rejections.iter().find_map(|rejection| {
        let AutoAuthorityRejection::PropertyBoundaryDagPlan {
            error: crate::view::paint::PropertyScrollScenePlanError::Frame(error),
        } = rejection
        else {
            return None;
        };
        Some(error.reasons.as_slice())
    });
    assert_eq!(
        owner_attributed_reasons,
        Some(
            [
                crate::view::paint::FramePaintPlanRejection::PropertyTree(
                    crate::view::compositor::property_tree::PropertyTreeValidationError::ScrollContractUnavailable(
                        roots[0],
                    ),
                ),
                crate::view::paint::FramePaintPlanRejection::InvalidPropertyScene(
                    "unreferenced-clip-coverage",
                ),
            ]
            .as_slice()
        ),
        "{name}: Stage A rejection owner and typed reason are part of authority parity; trace={trace:?}",
    );
}

#[test]
fn stage_a_text_area_authority_selection_and_rejection_parity() {
    let plain = prepared_scroll_text_area_scene();
    let interactive = interactive_scene();
    let atomic = projection_scene(false, false);
    let focused_atomic = prepared_focused_atomic_projection_scroll_text_area_scene();
    let atomic_selection = projection_scene(false, true);

    for (name, (arena, roots, properties, generations)) in [
        ("plain", plain),
        ("interactive", interactive),
        ("atomic-projection", atomic),
        ("focused-atomic-projection", focused_atomic),
        ("atomic-projection-selection", atomic_selection),
    ] {
        assert_property_scroll_authority_and_typed_rejection(
            name,
            arena,
            roots,
            properties,
            generations,
        );
    }
}
