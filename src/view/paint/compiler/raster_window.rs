use super::*;

/// Full command envelope and the resident subrectangle are separate facts.
/// Only the latter is allocated. Its source-space origin belongs to raster
/// identity, so moving to another window cannot reuse the previous pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RasterWindow {
    pub(super) content_bounds_bits: [u32; 4],
    pub(super) raster_bounds_bits: [u32; 4],
}
impl RasterWindow {
    pub(super) fn matches_target(self, target: &RetainedSurfaceRasterInputs) -> bool {
        let full = self.content_bounds_bits.map(f32::from_bits);
        let part = self.raster_bounds_bits.map(f32::from_bits);
        full.into_iter().chain(part).all(f32::is_finite)
            && full[2] > 0.
            && full[3] > 0.
            && part[2] > 0.
            && part[3] > 0.
            && part[0] >= full[0]
            && part[1] >= full[1]
            && part[0] + part[2] <= full[0] + full[2]
            && part[1] + part[3] <= full[1] + full[3]
            && ArtifactSurfaceRasterOriginProjection::new(
                self.raster_bounds_bits,
                target.scale_factor_bits,
            )
            .is_some_and(|origin| {
                origin.target_size == [target.color.width(), target.color.height()]
                    && origin.normalized_source_bounds_bits == target.source_bounds_bits
            })
    }
}

/// A finite receiver clip is a proof that pixels outside its inverse image
/// cannot contribute. The predicate consumes geometry, never property families.
/// Initial windows require an axis-aligned, orientation-preserving mapping;
/// unproved rotations/perspective keep the original fail-closed budget path.
/// A 256 physical pixel grid plus a one-texel sampling guard avoids reallocating
/// for each scroll tick. No command or full content envelope is discarded.
pub(super) fn select(
    content_bounds_bits: [u32; 4],
    geometry: ArtifactSurfaceCompositeGeometryStamp,
    context: ArtifactSurfaceRasterContext,
) -> Option<RasterWindow> {
    let full = content_bounds_bits.map(f32::from_bits);
    let dest = geometry.destination_bounds_bits().map(f32::from_bits);
    if let Some(quad) = geometry.transform_quad() {
        let [x, y, w, h] = dest;
        if quad != [[x, y + h], [x + w, y + h], [x + w, y], [x, y]] {
            return None;
        }
    }
    let clip = match geometry.resolved_receiver_clip() {
        ArtifactSurfaceResolvedClip::Scissor(GraphicsPassScissor::Logical(clip)) => {
            clip.map(|n| n as f32)
        }
        // There is no receiver read at all. Keep a minimal valid target rather
        // than allocating a huge invisible envelope; an exposed clip replans.
        ArtifactSurfaceResolvedClip::Empty => [dest[0], dest[1], 0., 0.],
        _ => return None,
    };
    if full.into_iter().chain(dest).any(|n| !n.is_finite()) || dest[2] <= 0. || dest[3] <= 0. {
        return None;
    }
    let scale = context.scale_factor();
    let mut region = [0.; 4];
    for axis in 0..2 {
        let ratio = full[axis + 2] / dest[axis + 2];
        let start = full[axis] + (clip[axis] - dest[axis]) * ratio;
        let end = full[axis] + (clip[axis] + clip[axis + 2] - dest[axis]) * ratio;
        if !start.is_finite() || !end.is_finite() {
            return None;
        }
        let low = (full[axis]).max(((start * scale - 1.) / 256.).floor() * 256. / scale);
        let high =
            (full[axis] + full[axis + 2]).min(((end * scale + 1.) / 256.).ceil() * 256. / scale);
        if high > low {
            region[axis] = low;
            region[axis + 2] = high - low;
        } else {
            region[axis] = full[axis];
            region[axis + 2] = full[axis + 2].min(1. / scale);
        }
    }
    let origin = ArtifactSurfaceRasterOriginProjection::new(
        region.map(f32::to_bits),
        context.scale_factor_bits,
    )?;
    if origin
        .target_size
        .iter()
        .any(|n| *n > context.max_texture_dimension_2d)
    {
        return None;
    }
    Some(RasterWindow {
        content_bounds_bits,
        raster_bounds_bits: region.map(f32::to_bits),
    })
}
