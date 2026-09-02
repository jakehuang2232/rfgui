use super::*;

#[derive(Clone, Copy)]
pub(super) struct IntermediateSurfaceCoverage<'a> {
    pub(super) pixels: &'a [u8],
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) source_physical_origin: [f32; 2],
}

impl IntermediateSurfaceCoverage<'_> {
    fn alpha_at_final_pixel(self, x: u32, y: u32) -> Result<u8, String> {
        let coordinates = [
            x as f32 - self.source_physical_origin[0],
            y as f32 - self.source_physical_origin[1],
        ];
        if coordinates.into_iter().any(|value| !value.is_finite()) {
            return Err("intermediate coverage coordinate is not finite".to_owned());
        }
        let rounded = coordinates.map(f32::round);
        if coordinates
            .into_iter()
            .zip(rounded)
            .any(|(value, rounded)| (value - rounded).abs() > f32::EPSILON)
        {
            return Err(format!(
                "intermediate coverage coordinate is not pixel-aligned: final=({x}, {y}), source_origin={:?}, mapped={coordinates:?}",
                self.source_physical_origin
            ));
        }
        let [texture_x, texture_y] = rounded.map(|value| value as i64);
        if texture_x < 0
            || texture_y < 0
            || texture_x >= i64::from(self.width)
            || texture_y >= i64::from(self.height)
        {
            return Err(format!(
                "final pixel maps outside the intermediate surface: final=({x}, {y}), mapped=({texture_x}, {texture_y}), surface={}x{}",
                self.width, self.height
            ));
        }
        let pixel_index = texture_y as usize * self.width as usize + texture_x as usize;
        self.pixels
            .get(pixel_index * BYTES_PER_PIXEL as usize + 3)
            .copied()
            .ok_or_else(|| "intermediate surface pixel buffer is truncated".to_owned())
    }
}

pub(super) fn validate_artifact_roundtrip_differences_are_partial_coverage_only(
    legacy: &[u8],
    artifact: &[u8],
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    validate_artifact_roundtrip_differences(legacy, artifact, None, adapter, case)
}

/// Classifies round-trip quantization at the stage where coverage still
/// exists. Final alpha is not a valid proxy after a partial-coverage texel is
/// composited over an opaque receiver because that operation raises final
/// alpha to 255. A final-opaque difference is accepted only when the actual
/// Artifact intermediate texel proves partial coverage mechanically.
pub(super) fn validate_artifact_roundtrip_differences_use_intermediate_partial_coverage(
    legacy: &[u8],
    artifact: &[u8],
    intermediate: IntermediateSurfaceCoverage<'_>,
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    validate_artifact_roundtrip_differences(legacy, artifact, Some(intermediate), adapter, case)
}

fn validate_artifact_roundtrip_differences(
    legacy: &[u8],
    artifact: &[u8],
    intermediate: Option<IntermediateSurfaceCoverage<'_>>,
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    if legacy.len() != artifact.len() {
        return Err(format!(
            "{case}: pixel buffer lengths differ on {adapter}: legacy={}, artifact={}",
            legacy.len(),
            artifact.len()
        ));
    }
    for (pixel_index, (legacy, artifact)) in legacy
        .chunks_exact(BYTES_PER_PIXEL as usize)
        .zip(artifact.chunks_exact(BYTES_PER_PIXEL as usize))
        .enumerate()
    {
        if legacy == artifact {
            continue;
        }
        let legacy_alpha = legacy[3];
        let artifact_alpha = artifact[3];
        if legacy_alpha != artifact_alpha {
            return Err(format!(
                "{case}: alpha differs on {adapter} at ({}, {}): legacy={legacy:?}, artifact={artifact:?}",
                pixel_index as u32 % WIDTH,
                pixel_index as u32 / WIDTH,
            ));
        }
        if (1..=254).contains(&legacy_alpha) {
            continue;
        }
        let Some(intermediate) = intermediate else {
            return Err(format!(
                "{case}: non-partial-final pixel differs on {adapter} at ({}, {}): legacy={legacy:?}, artifact={artifact:?}",
                pixel_index as u32 % WIDTH,
                pixel_index as u32 / WIDTH,
            ));
        };
        let x = pixel_index as u32 % WIDTH;
        let y = pixel_index as u32 / WIDTH;
        let intermediate_alpha = intermediate.alpha_at_final_pixel(x, y)?;
        if !(1..=254).contains(&intermediate_alpha) {
            return Err(format!(
                "{case}: final-opaque difference did not originate from intermediate partial coverage on {adapter} at ({x}, {y}): intermediate_alpha={intermediate_alpha}, legacy={legacy:?}, artifact={artifact:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn intermediate_partial_coverage_guard_rejects_opaque_source_differences() {
    let legacy = [2, 7, 16, 255];
    let artifact = [2, 7, 17, 255];
    let partial_intermediate = [2, 7, 16, 254];
    let opaque_intermediate = [2, 7, 16, 255];
    let coverage = |pixels| IntermediateSurfaceCoverage {
        pixels,
        width: 1,
        height: 1,
        source_physical_origin: [0.0, 0.0],
    };

    assert!(
        validate_artifact_roundtrip_differences_use_intermediate_partial_coverage(
            &legacy,
            &artifact,
            coverage(&partial_intermediate),
            "test-device",
            "partial-intermediate-positive-control",
        )
        .is_ok(),
        "a final-opaque difference may be classified only by its partial intermediate texel"
    );
    assert!(
        validate_artifact_roundtrip_differences_use_intermediate_partial_coverage(
            &legacy,
            &artifact,
            coverage(&opaque_intermediate),
            "test-device",
            "opaque-intermediate-negative-control",
        )
        .is_err(),
        "an opaque intermediate texel must not be hidden by the one-LSB round-trip allowance"
    );
}
