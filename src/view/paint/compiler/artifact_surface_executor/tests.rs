use super::*;
use crate::style::{BorderRadius, Length, Style};
use crate::view::base_component::Element;
use crate::view::compositor::property_tree::{EffectNodeId, EffectNodeSnapshot};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::paint::{
    ArtifactSurfaceRasterContext, FrameArtifactRecordOutcome, RendererMode,
    prepare_artifact_surface_raster_plan, record_closed_single_target_frame_artifact,
    seal_prepared_artifact_surface_frame,
};

fn prepared_child_mask_surface_frame() -> PreparedArtifactSurfaceFrame {
    let (arena, root, _, _) = crate::view::paint::tests::exact_isolation_fixture(1.0);
    let mut rounded = Style::new();
    rounded.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    crate::view::test_support::get_element_mut::<Element>(&arena, root).apply_style(rounded);

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let FrameArtifactRecordOutcome::Artifact { mut artifact, .. } =
        record_closed_single_target_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect("rounded surface fixture must record")
    else {
        panic!("forced rounded surface recording cannot fall back")
    };
    assert_eq!(
        artifact
            .chunks
            .iter()
            .filter(|chunk| chunk.id.slot == RETAINED_CHILD_MASK_SLOT)
            .count(),
        2,
    );

    let surface_owner = artifact
        .owner_nodes
        .iter()
        .map(|snapshot| snapshot.owner)
        .find(|owner| *owner != root)
        .expect("rounded fixture child owner");
    let effect = EffectNodeId(surface_owner);
    artifact.effect_nodes.push(EffectNodeSnapshot {
        id: effect,
        owner: surface_owner,
        parent: None,
        opacity: 1.0,
        generation: 1,
    });
    artifact
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == surface_owner)
        .expect("surface owner endpoints")
        .descendants
        .effect = Some(effect);
    artifact
        .chunks
        .iter_mut()
        .find(|chunk| chunk.owner == surface_owner)
        .expect("surface owner chunk")
        .properties
        .effect = Some(effect);

    let context = ArtifactSurfaceRasterContext::new(
        1.0,
        wgpu::TextureFormat::Bgra8Unorm,
        [0.0, 0.0],
        None,
        4096,
        256 * 1024 * 1024,
    )
    .expect("canonical raster context");
    let plan = prepare_artifact_surface_raster_plan(artifact, context)
        .expect("child-mask surface raster plan");
    seal_prepared_artifact_surface_frame(plan).expect("child-mask surface resident seal")
}

mod child_mask_depth_tests;
mod execution_seal_tests;
