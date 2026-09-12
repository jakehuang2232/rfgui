use super::*;
use crate::view::base_component::Element;
use crate::view::node_arena::Node;

fn sample_capture() -> (DebugCapture, DebugNodeId, DebugNodeId) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        10, 0.0, 0.0, 100.0, 80.0,
    ))));
    let child = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(20, 10.0, 12.0, 20.0, 16.0)),
        Some(root),
    ));
    arena.push_child(root, child);
    arena.set_roots(vec![root]);
    arena.refresh_subtree_dirty_cache(root);

    let capture = DebugCapture::from_arena(
        DebugCaptureOptions::default(),
        &arena,
        &[root],
        DebugViewportCaptureInput {
            logical_size: (320.0, 240.0),
            scale_factor: 2.0,
            focused_node: Some(child),
            hovered_node: Some(root),
            pointer_capture_node: None,
            keyboard_capture_node: Some(child),
            pointer_position: Some((12.0, 14.0)),
            pressed_pointer_buttons: Vec::new(),
        },
    );
    let root_id = capture.document.roots[0].clone();
    let child_id = match capture
        .query(DebugQuery::GetChildren {
            node: root_id.clone(),
        })
        .unwrap()
    {
        DebugResponse::Nodes(nodes) => nodes[0].id.clone(),
        response => panic!("unexpected response: {response:?}"),
    };
    (capture, root_id, child_id)
}

#[test]
fn document_capture_keeps_document_and_arena_ids_separate() {
    let (capture, root_id, child_id) = sample_capture();

    assert_eq!(capture.document.roots, vec![root_id.clone()]);
    assert_eq!(capture.document.node_count, 2);
    assert_ne!(root_id.as_str(), child_id.as_str());

    let root = match capture
        .query(DebugQuery::GetNode {
            node: root_id.clone(),
        })
        .unwrap()
    {
        DebugResponse::Node(node) => node,
        response => panic!("unexpected response: {response:?}"),
    };
    assert_eq!(root.id, root_id);
    assert_eq!(root.stable_id, Some(10));
    assert!(root.arena_id.is_some());
    assert_eq!(root.children, vec![child_id]);
}

#[test]
fn capture_answers_interactive_state_queries_consistently() {
    let (capture, root_id, child_id) = sample_capture();

    let child_state = match capture
        .query(DebugQuery::GetElementState {
            node: child_id.clone(),
        })
        .unwrap()
    {
        DebugResponse::ElementState(state) => state,
        response => panic!("unexpected response: {response:?}"),
    };
    assert_eq!(child_state.identity.stable_id, Some(20));
    assert_eq!(child_state.tree.parent, Some(root_id.clone()));
    assert!(child_state.interaction.as_ref().unwrap().focused);
    assert!(child_state.interaction.as_ref().unwrap().keyboard_captured);

    let ancestors = match capture
        .query(DebugQuery::GetAncestors { node: child_id })
        .unwrap()
    {
        DebugResponse::Nodes(nodes) => nodes,
        response => panic!("unexpected response: {response:?}"),
    };
    assert_eq!(ancestors.len(), 1);
    assert_eq!(ancestors[0].id, root_id);
}

#[test]
fn pick_node_uses_captured_layout_without_live_arena_borrow() {
    let (capture, _root_id, child_id) = sample_capture();

    let picked = match capture
        .query(DebugQuery::PickNode { x: 12.0, y: 14.0 })
        .unwrap()
    {
        DebugResponse::Pick(node) => node,
        response => panic!("unexpected response: {response:?}"),
    };
    assert_eq!(picked, Some(child_id));
}

#[test]
fn missing_node_returns_stable_error() {
    let (capture, _, _) = sample_capture();
    let missing = DebugNodeId::from("missing".to_string());

    assert_eq!(
        capture
            .query(DebugQuery::GetNode {
                node: missing.clone()
            })
            .unwrap_err(),
        DebugError::UnknownNode(missing)
    );
}

fn retained_auto_input(root: NodeKey, child: NodeKey) -> DebugRetainedAutoCaptureInput {
    let child_bounds = DebugRect {
        x: 10.0,
        y: 12.0,
        width: 20.0,
        height: 16.0,
    };
    let fallback = DebugRetainedAutoFallbackCaptureInput {
        stage: DebugFallbackStage::Planning,
        category: DebugFallbackCategory::PropertyTopology,
        detail: DebugFallbackDetail::Boundary {
            reason: "nested-effect",
        },
        owner: Some(child),
        stable_id: Some(20),
        element_type: Some("Element"),
        bounds: Some(child_bounds),
    };
    DebugRetainedAutoCaptureInput {
        frame: DebugRetainedAutoFrameCaptureInput {
            attempt_id: 42,
            requested_mode: DebugPaintRequestedMode::RetainedAuto,
            selected_authority: DebugFramePaintAuthority::Legacy,
            disposition: DebugFrameDisposition::FellBackToLegacy,
            fallback_stages: vec![fallback.clone()],
            statistics: DebugRetainedAutoStatistics {
                reachable_nodes: 2,
                covered_nodes: 1,
                legacy_nodes: 1,
                fallback_count: 1,
                ..Default::default()
            },
        },
        nodes: vec![DebugRetainedAutoNodeCaptureInput {
            owner: Some(child),
            stable_id: Some(20),
            element_type: "Element",
            bounds: Some(child_bounds),
            coverage: vec![DebugCoverageKind::LegacyBoundary],
            resident_action: Some(DebugResidentAction::None),
            fallbacks: vec![fallback],
        }],
        surfaces: vec![DebugRetainedAutoSurfaceCaptureInput {
            owner: Some(root),
            stable_id: Some(10),
            element_type: "Element",
            bounds: Some(DebugRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            }),
            kind: DebugSurfaceKind::Effect,
            coverage: DebugCoverageKind::PropertySurface,
            resident_action: DebugResidentAction::Reuse,
        }],
    }
}

#[test]
fn retained_auto_capture_maps_existing_node_ids_and_structured_reasons() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        10, 0.0, 0.0, 100.0, 80.0,
    ))));
    let child = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(20, 10.0, 12.0, 20.0, 16.0)),
        Some(root),
    ));
    arena.push_child(root, child);
    arena.set_roots(vec![root]);
    let mut options = DebugCaptureOptions::default();
    options.include_retained_auto = true;
    let capture = DebugCapture::from_arena_with_retained_auto(
        options,
        &arena,
        &[root],
        DebugViewportCaptureInput {
            logical_size: (320.0, 240.0),
            scale_factor: 2.0,
            focused_node: None,
            hovered_node: None,
            pointer_capture_node: None,
            keyboard_capture_node: None,
            pointer_position: None,
            pressed_pointer_buttons: Vec::new(),
        },
        Some(retained_auto_input(root, child)),
    );

    let retained = capture
        .document()
        .viewport
        .retained_auto
        .as_ref()
        .expect("last attempt is captured");
    assert_eq!(retained.frame.attempt_id, 42);
    assert_eq!(retained.frame.fallback_stages.len(), 1);
    assert!(matches!(
        retained.frame.fallback_stages[0].detail,
        DebugFallbackDetail::Boundary {
            reason: "nested-effect"
        }
    ));
    assert_eq!(retained.nodes[0].stable_id, Some(20));
    assert!(retained.nodes[0].node.is_some());
    assert_eq!(
        retained.surfaces[0].resident_action,
        DebugResidentAction::Reuse
    );

    let child_id = retained.nodes[0].node.clone().unwrap();
    let DebugResponse::RenderState(Some(render)) = capture
        .query(DebugQuery::GetRenderState { node: child_id })
        .unwrap()
    else {
        panic!("node render state must exist")
    };
    assert_eq!(
        render.retained_auto.unwrap().coverage,
        vec![DebugCoverageKind::LegacyBoundary]
    );
}

#[test]
fn retained_auto_input_is_ignored_when_capture_option_is_off() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        10, 0.0, 0.0, 100.0, 80.0,
    ))));
    let child = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(20, 10.0, 12.0, 20.0, 16.0)),
        Some(root),
    ));
    arena.push_child(root, child);
    let capture = DebugCapture::from_arena_with_retained_auto(
        DebugCaptureOptions::default(),
        &arena,
        &[root],
        DebugViewportCaptureInput {
            logical_size: (320.0, 240.0),
            scale_factor: 1.0,
            focused_node: None,
            hovered_node: None,
            pointer_capture_node: None,
            keyboard_capture_node: None,
            pointer_position: None,
            pressed_pointer_buttons: Vec::new(),
        },
        Some(retained_auto_input(root, child)),
    );
    assert!(capture.document().viewport.retained_auto.is_none());
}
