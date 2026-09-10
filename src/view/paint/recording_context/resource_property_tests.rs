use super::*;
use crate::view::base_component::Element;
use crate::view::node_arena::{Node, NodeArena};

#[test]
fn generic_resource_property_authority_requires_exact_owner_and_state() {
    let mut arena = NodeArena::new();
    let owner = arena.insert(Node::new(Box::new(Element::new_with_id(
        41, 0.0, 0.0, 1.0, 1.0,
    ))));
    let other = arena.insert(Node::new(Box::new(Element::new_with_id(
        42, 0.0, 0.0, 1.0, 1.0,
    ))));
    let properties = PropertyTreeState {
        scroll: Some(ScrollNodeId(other)),
        ..Default::default()
    };
    let context = PaintRecordingContext {
        recording_owner: Some(owner),
        recording_owner_stable_id: Some(41),
        surface_dag: true,
        surface_dag_paint_state: Some(properties),
        ..Default::default()
    };
    assert!(context.authorizes_surface_dag_paint_properties(owner, 41, properties));
    assert!(!context.authorizes_surface_dag_paint_properties(other, 41, properties));
    assert!(!context.authorizes_surface_dag_paint_properties(owner, 42, properties));
    assert!(!context.authorizes_surface_dag_paint_properties(owner, 41, Default::default()));
    assert!(
        !PaintRecordingContext {
            surface_dag: false,
            ..context
        }
        .authorizes_surface_dag_paint_properties(owner, 41, properties)
    );
    assert!(
        !PaintRecordingContext {
            surface_dag_paint_state: None,
            ..context
        }
        .authorizes_surface_dag_paint_properties(owner, 41, properties)
    );
}
