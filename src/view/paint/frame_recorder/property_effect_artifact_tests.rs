use super::*;
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
