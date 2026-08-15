use super::*;
use slotmap::SlotMap;

pub(super) fn same_owner_transform_effect_scroll_role_fixture() -> (
    TransformNodeId,
    EffectPropertySurfaceArtifactContract,
    SameOwnerEffectScrollRasterRoleStamp,
    SameOwnerTransformEffectScrollRasterRoleStamp,
) {
    let mut keys = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    let owner = keys.insert(());
    let content_root = keys.insert(());
    let stable_id = 94_001;
    let content_stable_id = 94_002;
    let transform = TransformNodeId(owner);
    let effect = EffectNodeSnapshot {
        id: EffectNodeId(owner),
        owner,
        parent: None,
        opacity: 0.625,
        generation: 17,
    };
    let contract = EffectPropertySurfaceArtifactContract::new(
        owner,
        stable_id,
        effect,
        vec![effect],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![super::super::EffectPropertyContentWitness {
            owner,
            stable_id,
            parent: None,
            self_paint_revision: 19,
            topology_revision: 23,
        }],
    )
    .expect("same-owner effect contract");
    let scroll = ScrollNodeId(owner);
    let contents_clip = ClipNodeId {
        owner,
        role: ClipNodeRole::ContentsClip,
    };
    let inner = SameOwnerEffectScrollRasterRoleStamp {
        owner,
        stable_id,
        effect: effect.id,
        scroll,
        contents_clip,
        content_root,
        content_stable_id,
    };
    let outer = SameOwnerTransformEffectScrollRasterRoleStamp {
        owner,
        stable_id,
        transform,
        effect: effect.id,
        scroll,
        contents_clip,
        content_root,
        content_stable_id,
    };
    (transform, contract, inner, outer)
}

mod property_boundary_forest_branching_tests;
mod artifact_space_translation_tests;
mod local_clip_generation_semantics_tests;
mod property_boundary_forest_multi_root_tests;
mod property_boundary_forest_plain_root_tests;
mod property_boundary_forest_tests;
mod property_boundary_forest_linear_tests;
mod same_owner_transform_effect_scroll_tests;
mod stage_c_deletion_inventory_tests;
