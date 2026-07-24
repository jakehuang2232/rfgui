use super::*;
use crate::view::base_component::Element;
use crate::view::compositor::property_tree::{EffectNodeId, EffectNodeSnapshot};

fn contract(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    generations: &PaintGenerationTracker,
    root: NodeKey,
    cutouts: &FxHashSet<NodeKey>,
) -> super::super::EffectPropertySurfaceArtifactContract {
    let live = property_trees
        .effect_snapshot_for(Some(EffectNodeId(root)))
        .expect("live effect chain");
    let isolated = EffectNodeSnapshot {
        parent: None,
        ..live[0]
    };
    let mut content = Vec::new();
    let mut stack = vec![root];
    while let Some(owner) = stack.pop() {
        if owner != root && cutouts.contains(&owner) {
            continue;
        }
        let node = arena.get(owner).expect("content owner");
        let revisions = generations
            .local_generations_for(owner)
            .expect("content generations");
        content.push(super::super::EffectPropertyContentWitness {
            owner,
            stable_id: node.element.stable_id(),
            parent: (owner != root).then(|| arena.parent_of(owner)).flatten(),
            self_paint_revision: revisions.self_paint_revision,
            topology_revision: revisions.topology_revision,
        });
        stack.extend(node.element.children().iter().rev().copied());
    }
    super::super::EffectPropertySurfaceArtifactContract::new(
        root,
        arena.get(root).unwrap().element.stable_id(),
        isolated,
        live.clone(),
        live[1..].to_vec(),
        Vec::new(),
        Vec::new(),
        content,
    )
    .expect("canonical effect contract")
}

#[test]
fn property_effect_artifact_records_cutouts_and_detaches_ancestor_effects() {
    let (arena, root, mut properties, mut generations) =
        super::super::tests::exact_isolation_fixture(0.5);
    let child = arena.get(root).unwrap().element.children()[0];
    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);

    let root_contract = contract(
        &arena,
        &properties,
        &generations,
        root,
        &FxHashSet::from_iter([child]),
    );
    let child_contract = contract(
        &arena,
        &properties,
        &generations,
        child,
        &FxHashSet::default(),
    );
    let cutouts = super::super::PlannedBoundaryCutoutSet::from_iter([(
        child,
        super::super::PlannedBoundary {
            root: child,
            stable_id: arena.get(child).unwrap().element.stable_id(),
            kind: super::super::PlannedBoundaryKind::Isolation(EffectNodeId(child)),
        },
    )]);
    let root_steps = record_effect_property_surface_steps_for_plan(
        &arena,
        &properties,
        &generations,
        &root_contract,
        [0.0, 0.0],
        &cutouts,
        None,
    )
    .expect("root effect steps");
    assert!(matches!(
        root_steps.as_slice(),
        [RecordedTransformSurfaceStep::Artifact(_), RecordedTransformSurfaceStep::Boundary(boundary)]
            if boundary.root == child
    ));

    let child_steps = record_effect_property_surface_steps_for_plan(
        &arena,
        &properties,
        &generations,
        &child_contract,
        [0.0, 0.0],
        &super::super::PlannedBoundaryCutoutSet::default(),
        None,
    )
    .expect("child effect steps");
    let [RecordedTransformSurfaceStep::Artifact(child_artifact)] = child_steps.as_slice() else {
        panic!("child effect surface must be one artifact span")
    };
    assert_eq!(
        child_artifact.effect_nodes.as_slice(),
        [child_contract.isolated_leaf()]
    );
    assert!(
        child_artifact
            .effect_nodes
            .iter()
            .all(|effect| effect.parent.is_none())
    );
    assert!(
        super::super::validate_effect_property_surface_artifact(child_artifact, &child_contract,)
            .is_some()
    );

    let mut leaked = child_artifact.clone();
    leaked.effect_nodes = child_contract.live_effect_chain().to_vec();
    assert!(
        super::super::validate_effect_property_surface_artifact(&leaked, &child_contract,)
            .is_none()
    );
}
