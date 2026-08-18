use super::*;

#[test]
fn artifact_surface_program_step_taxonomy_is_an_exhaustive_two_variant_set() {
    fn label(step: &ArtifactSurfaceRasterProgramStepStamp) -> &'static str {
        match step {
            ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(_) => "artifact-span",
            ArtifactSurfaceRasterProgramStepStamp::NestedSurface(_) => "nested-surface",
        }
    }
    let _ = label as fn(&ArtifactSurfaceRasterProgramStepStamp) -> &'static str;
}

#[test]
fn artifact_surface_resident_seal_error_taxonomy_is_exhaustive() {
    fn label(error: ArtifactSurfaceResidentSealError) -> &'static str {
        match error {
            ArtifactSurfaceResidentSealError::MissingPreparedNode(_) => "missing-node",
            ArtifactSurfaceResidentSealError::DuplicateResidentKey(_) => "duplicate-key",
            ArtifactSurfaceResidentSealError::InvalidArtifactSpan { .. } => "artifact-span",
            ArtifactSurfaceResidentSealError::InvalidNestedSurface { .. } => "nested-surface",
            ArtifactSurfaceResidentSealError::InvalidClipClosure(_) => "clip-closure",
            ArtifactSurfaceResidentSealError::NonCanonicalSet => "non-canonical-set",
        }
    }
    let _ = label as fn(ArtifactSurfaceResidentSealError) -> &'static str;
}
