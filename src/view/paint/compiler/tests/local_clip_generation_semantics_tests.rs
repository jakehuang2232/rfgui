use super::*;

fn legacy_local_clip_stamp() -> RetainedSurfaceRasterStamp {
    crate::view::paint::tests::atomic_projection_content_stamp_for_test("projected", 0xc3_b001)
        .expect("legacy local-clip fixture must produce one canonical stamp")
}

fn semantics_name(semantics: LocalClipGenerationSemantics) -> &'static str {
    match semantics {
        LocalClipGenerationSemantics::LegacyDetached => "legacy-detached",
        LocalClipGenerationSemantics::ArtifactLive => "artifact-live",
    }
}

#[test]
fn local_clip_generation_semantics_are_an_exhaustive_closed_set() {
    assert_eq!(
        [
            semantics_name(LocalClipGenerationSemantics::LegacyDetached),
            semantics_name(LocalClipGenerationSemantics::ArtifactLive),
        ],
        ["legacy-detached", "artifact-live"],
    );
}

#[test]
fn legacy_semantics_keeps_the_detached_generation_gate_strict() {
    let legacy = legacy_local_clip_stamp();
    assert_eq!(
        legacy.local_clip_generation_semantics,
        Some(LocalClipGenerationSemantics::LegacyDetached),
    );
    assert!(retained_surface_raster_stamp_is_canonical(&legacy));

    let mut live_generation = legacy;
    live_generation.clip_nodes[0].generation = 29;
    let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
        live_generation.ordered_steps.as_mut_slice()
    else {
        panic!("local clip fixture owns one artifact span")
    };
    span.clip_nodes[0].generation = 29;
    assert!(!retained_surface_raster_stamp_is_canonical(
        &live_generation
    ));
}
