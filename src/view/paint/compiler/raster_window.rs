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
/// An unclipped scene-root receiver reads only its target. A transform maps the
/// receiver clip back through its matrix; the preimage's bounding box holds every
/// source pixel the clip can show, whatever the rotation or perspective.
/// A 256 physical pixel grid plus a one-texel sampling guard avoids reallocating
/// for each scroll tick. No command or full content envelope is discarded.
pub(super) fn select(
    content_bounds_bits: [u32; 4],
    geometry: ArtifactSurfaceCompositeGeometryStamp,
    receiver_is_scene_root: bool,
    receiver_paint_offset: [f32; 2],
    context: ArtifactSurfaceRasterContext,
) -> Option<RasterWindow> {
    let full = content_bounds_bits.map(f32::from_bits);
    let dest = geometry.destination_bounds_bits().map(f32::from_bits);
    if full.into_iter().chain(dest).any(|n| !n.is_finite()) || dest[2] <= 0. || dest[3] <= 0. {
        return None;
    }
    let clip = match geometry.resolved_receiver_clip() {
        ArtifactSurfaceResolvedClip::Scissor(GraphicsPassScissor::Logical(clip)) => {
            clip.map(|n| n as f32)
        }
        ArtifactSurfaceResolvedClip::Unclipped if receiver_is_scene_root => {
            context.target_logical_rect()
        }
        // There is no receiver read at all. Keep a minimal valid target rather
        // than allocating a huge invisible envelope; an exposed clip replans.
        ArtifactSurfaceResolvedClip::Empty => [dest[0], dest[1], 0., 0.],
        _ => return None,
    };
    // The source rectangle the receiver clip reads, before snapping.
    let read = match geometry {
        ArtifactSurfaceCompositeGeometryStamp::Transform {
            receiver_transform_bits,
            ..
        } if !geometry.transform_quad().is_some_and(|quad| {
            let [x, y, w, h] = dest;
            quad == [[x, y + h], [x + w, y + h], [x + w, y], [x, y]]
        }) =>
        {
            let matrix = glam::Mat4::from_cols_array(&receiver_transform_bits.map(f32::from_bits));
            // A clip corner seen from behind the eye has no finite preimage:
            // every source pixel may then be visible.
            transformed_preimage(matrix, receiver_paint_offset, clip).unwrap_or(full)
        }
        _ => {
            let mut read = [0.; 4];
            for axis in 0..2 {
                let ratio = full[axis + 2] / dest[axis + 2];
                read[axis] = full[axis] + (clip[axis] - dest[axis]) * ratio;
                read[axis + 2] = clip[axis + 2] * ratio;
            }
            read
        }
    };
    let scale = context.scale_factor();
    let mut region = [0.; 4];
    for axis in 0..2 {
        let start = read[axis];
        let end = read[axis] + read[axis + 2];
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

/// The bounding box of the source points, on the `z = 0` plane, that `matrix`
/// followed by `offset` maps into `clip`. `None` when a clip corner has no
/// preimage in front of the eye.
fn transformed_preimage(matrix: glam::Mat4, offset: [f32; 2], clip: [f32; 4]) -> Option<[f32; 4]> {
    let homography = glam::Mat3::from_cols(
        glam::Vec3::new(matrix.x_axis.x, matrix.x_axis.y, matrix.x_axis.w),
        glam::Vec3::new(matrix.y_axis.x, matrix.y_axis.y, matrix.y_axis.w),
        glam::Vec3::new(matrix.w_axis.x, matrix.w_axis.y, matrix.w_axis.w),
    );
    let inverse = homography.inverse();
    if !inverse.is_finite() {
        return None;
    }
    let [x, y, width, height] = clip;
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for [corner_x, corner_y] in [
        [x, y],
        [x + width, y],
        [x, y + height],
        [x + width, y + height],
    ] {
        let point = inverse * glam::Vec3::new(corner_x - offset[0], corner_y - offset[1], 1.0);
        if !point.is_finite() || point.z <= 0.000_001 {
            return None;
        }
        for (axis, value) in [point.x / point.z, point.y / point.z]
            .into_iter()
            .enumerate()
        {
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    let bounds = [min[0], min[1], max[0] - min[0], max[1] - min[1]];
    bounds.into_iter().all(f32::is_finite).then_some(bounds)
}
