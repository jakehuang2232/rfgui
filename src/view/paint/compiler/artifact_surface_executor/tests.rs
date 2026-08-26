use super::super::{ArtifactSurfaceChunkClipSchedule, artifact_surface_chunk_opaque_order_count};
use super::*;
use crate::style::{BorderRadius, Length, Style};
use crate::view::base_component::Element;
use crate::view::base_component::UiBuildContext;
use crate::view::compositor::property_tree::{ClipBehavior, EffectNodeId, EffectNodeSnapshot};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::frame_graph::FrameGraph;
use crate::view::paint::{
    ArtifactSurfaceRasterContext, FrameArtifactRecordOutcome, PaintOp, RendererMode,
    prepare_artifact_surface_raster_plan, record_closed_single_target_frame_artifact,
    seal_prepared_artifact_surface_frame,
};
use crate::view::viewport::Viewport;

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

fn raster_context() -> ArtifactSurfaceRasterContext {
    ArtifactSurfaceRasterContext::new(
        1.0,
        wgpu::TextureFormat::Bgra8Unorm,
        [0.0, 0.0],
        None,
        4096,
        256 * 1024 * 1024,
    )
    .expect("canonical executor raster context")
}

fn execution_context() -> UiBuildContext {
    UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8Unorm, 1.0)
}

fn execution_error_name(error: ArtifactSurfaceExecutionError) -> &'static str {
    match error {
        ArtifactSurfaceExecutionError::InactiveFrameStageOwner => "inactive-frame-stage-owner",
        ArtifactSurfaceExecutionError::ChildMaskDepthOverflow { .. } => "child-mask-depth-overflow",
        ArtifactSurfaceExecutionError::PersistentKeyAlreadyDeclared(_) => {
            "persistent-key-already-declared"
        }
    }
}

fn prepared_depth_four_surface_frame() -> PreparedArtifactSurfaceFrame {
    crate::view::paint::frame_plan::tests::prepared_depth_four_surface_frame()
}

fn prepared_co_located_surface_frame() -> PreparedArtifactSurfaceFrame {
    crate::view::paint::frame_plan::tests::prepared_co_located_surface_frame()
}

fn prepared_self_clip_shadow_surface_frame(empty_suffix: bool) -> PreparedArtifactSurfaceFrame {
    let mut artifact = crate::view::paint::frame_plan::tests::exact_self_clip_shadow_artifact();
    if empty_suffix {
        artifact
            .clip_nodes
            .iter_mut()
            .find(|clip| clip.behavior == ClipBehavior::Replace)
            .expect("self Replace clip")
            .logical_scissor = [0, 0, 0, 0];
    }
    let shadow_plan = prepare_artifact_surface_raster_plan(artifact, raster_context())
        .expect("self-clip shadow root raster plan");
    let shadow_span = shadow_plan
        .roots
        .into_iter()
        .flat_map(|root| root.steps)
        .find(|step| matches!(step, PreparedArtifactSurfaceRasterStep::ArtifactSpan(_)))
        .expect("self-clip shadow root span");

    let (mut surface_plan, _) = prepared_child_mask_surface_frame().into_parts();
    surface_plan
        .roots
        .first_mut()
        .expect("surface fixture root")
        .steps
        .insert(0, shadow_span);
    seal_prepared_artifact_surface_frame(surface_plan)
        .expect("self-clip shadow root plus surface resident seal")
}

fn prepared_whole_chunk_clip_surface_frame(clip: ResolvedClip) -> PreparedArtifactSurfaceFrame {
    fn rewrite_draw_rect_spans(
        steps: &mut [PreparedArtifactSurfaceRasterStep],
        clip: ResolvedClip,
    ) -> usize {
        let mut rewritten = 0;
        for step in steps {
            let PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) = step else {
                continue;
            };
            let mut changed_span = false;
            for chunk in &mut span.chunks {
                if chunk.source.id.slot != RETAINED_CHILD_MASK_SLOT
                    && !chunk.localized_ops.is_empty()
                    && chunk
                        .localized_ops
                        .iter()
                        .all(|op| matches!(op, PaintOp::DrawRect(_)))
                {
                    chunk.clip_schedule = ArtifactSurfaceChunkClipSchedule::WholeChunk(clip);
                    rewritten += 1;
                    changed_span = true;
                }
            }
            if changed_span {
                span.opaque_order_count = span
                    .chunks
                    .iter()
                    .try_fold(0_u32, |count, chunk| {
                        count.checked_add(artifact_surface_chunk_opaque_order_count(
                            chunk.clip_schedule,
                            &chunk.localized_ops,
                        )?)
                    })
                    .expect("rewritten fixture opaque order");
            }
        }
        rewritten
    }

    let (mut plan, _) = prepared_child_mask_surface_frame().into_parts();
    let rewritten_roots = plan
        .roots
        .iter_mut()
        .map(|root| rewrite_draw_rect_spans(&mut root.steps, clip))
        .sum::<usize>();
    let rewritten_nodes = plan
        .nodes
        .iter_mut()
        .map(|node| rewrite_draw_rect_spans(&mut node.steps, clip))
        .sum::<usize>();
    assert_ne!(
        rewritten_roots + rewritten_nodes,
        0,
        "fixture must contain a non-mask DrawRect chunk"
    );
    seal_prepared_artifact_surface_frame(plan).expect("whole-chunk clip fixture resident seal")
}

mod child_mask_depth_tests;
mod composite_tests;
mod execution_seal_tests;
mod pool_transaction_tests;
mod raster_emission_tests;
