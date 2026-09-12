use super::*;
use crate::style::Opacity;
use crate::view::paint::{
    PreparedArtifactSurfaceRasterPlan, PreparedArtifactSurfaceRasterStep,
    SurfaceDagExecutionTargetId,
};
use crate::view::test_support::{commit_child, commit_element};

mod budget_tests;
mod contracts_tests;
mod culling_reentry_tests;
mod gpu_source_tests;
mod multi_target_tests;
mod pixel_tests;
mod raster_window_tests;
mod viewport_tests;

use crate::view::paint::tests::retained_acceptance_fixtures::*;

fn fixture(scene: Scene) -> Fixture {
    let mut fixture = unlaid_out_fixture(scene);
    let mut layout = Viewport::new();
    for &root in &fixture.roots {
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut layout,
            &mut fixture.arena,
            root,
            EXTENT.map(|v| v as f32),
        );
    }
    fixture
}

fn record(fixture: &Fixture) -> PaintArtifact {
    let (properties, generations) = sync_identity(&fixture.arena, &fixture.roots);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &fixture.arena,
        &fixture.roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("complete production layout must record") else {
        panic!("fallback")
    };
    artifact
}

fn prepare(
    artifact: PaintArtifact,
    dpr: f32,
    offset: [f32; 2],
) -> PreparedArtifactSurfaceRasterPlan {
    prepare_artifact_surface_raster_plan(
        artifact,
        ArtifactSurfaceRasterContext::new(dpr, FORMAT, offset, None, 8192, 128 * 1024 * 1024)
            .unwrap(),
    )
    .expect("generic plan")
}
