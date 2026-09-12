use super::*;
use crate::view::base_component::Element;
use crate::view::test_support::{commit_element, new_test_arena};

#[test]
fn immutable_spatial_prefixes_preserve_rounding_and_query_order() {
    let mut arena = new_test_arena();
    let keys = (0..4)
        .map(|i| {
            commit_element(
                &mut arena,
                Box::new(Element::new_with_id(0xdad0 + i, 0., 0., 1., 1.)),
            )
        })
        .collect::<Vec<_>>();
    // f32 addition is not associative: root-to-leaf yields 3, while summing
    // the last two edges before adding the large prefix would yield 4.
    let values = [100_000_000., 1., -100_000_000., 3.];
    let positions = keys
        .iter()
        .enumerate()
        .map(|(i, &owner)| LayoutPositionNodeSnapshot {
            id: LayoutPositionNodeId(owner),
            owner,
            reference: SpatialPositionReference::LayoutParent(i.checked_sub(1).map(|p| keys[p])),
            reference_scroll: None,
            translation_at_scroll_zero: Vec2::new(values[i], 0.),
            child_reference_offset_at_scroll_zero: Vec2::ZERO,
            generation: 1,
        })
        .collect::<Vec<_>>();
    let visuals = keys
        .iter()
        .enumerate()
        .map(|(i, &owner)| VisualOffsetNodeSnapshot {
            id: VisualOffsetNodeId(owner),
            owner,
            parent: i.checked_sub(1).map(|p| VisualOffsetNodeId(keys[p])),
            offset: Vec2::new(values[i], 0.),
            generation: 1,
        })
        .collect::<Vec<_>>();
    for order in [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2]] {
        let graph = SpatialProjectionGraph::try_new(&[], &positions, &visuals, &[]).unwrap();
        for i in order {
            let expected = values[..=i].iter().fold(0_f32, |sum, v| sum + v);
            assert_eq!(
                graph.layout_flow_position(keys[i]).unwrap().x.to_bits(),
                expected.to_bits()
            );
            assert_eq!(
                graph.cumulative_visual_offset(keys[i]).unwrap().x.to_bits(),
                expected.to_bits()
            );
        }
        assert_eq!(
            graph
                .derive_optional_owner_viewport_position(keys[3])
                .unwrap(),
            Vec2::new(6., 0.)
        );
        assert_eq!(graph.resolved_positions.borrow().resolved.len(), 4);
        assert_eq!(graph.resolved_visuals.borrow().resolved.len(), 4);
    }
    let mut moved = positions.clone();
    moved[3].translation_at_scroll_zero.x = 7.;
    let graph = SpatialProjectionGraph::try_new(&[], &moved, &visuals, &[]).unwrap();
    assert_eq!(
        graph
            .derive_optional_owner_viewport_position(keys[3])
            .unwrap(),
        Vec2::new(10., 0.),
        "a new snapshot graph must not inherit another graph's prefix results"
    );
}
