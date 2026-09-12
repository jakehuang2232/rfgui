use crate::view::ImageSampling;
use std::num::NonZeroU64;
use std::sync::Arc;

/// Stable identity of an image registry entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ImageAssetId(NonZeroU64);

impl ImageAssetId {
    pub(super) const fn new(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn for_test(raw: u64) -> Self {
        match Self::new(raw) {
            Some(id) => id,
            None => panic!("ImageAssetId must be non-zero"),
        }
    }
}

/// Stable identity of an SVG raster registry entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SvgRasterAssetId(NonZeroU64);

impl SvgRasterAssetId {
    pub(super) const fn new(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn for_test(raw: u64) -> Self {
        match Self::new(raw) {
            Some(id) => id,
            None => panic!("SvgRasterAssetId must be non-zero"),
        }
    }
}

/// GPU cache key. The enum discriminant keeps Image and SVG raster namespaces
/// distinct even when their registry-local numeric IDs are equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SampledTextureId {
    Image(ImageAssetId),
    SvgRaster(SvgRasterAssetId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SampledTextureAlphaMode {
    Straight,
}

/// Immutable, owning CPU-side upload truth carried by a render pass.
///
/// A Viewport may outlive or reset its GPU cache independently of the global
/// asset registries, so execute must never rely on a registry-global
/// "already uploaded" bit.
#[derive(Clone, Debug)]
pub(crate) struct SampledTextureUpload {
    pub(crate) id: SampledTextureId,
    pub(crate) generation: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) alpha_mode: SampledTextureAlphaMode,
    pub(crate) pixels: Arc<[u8]>,
    pub(crate) sampling: ImageSampling,
}

impl SampledTextureUpload {
    pub(crate) fn extent(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn validate_rgba8(&self) -> Option<ValidatedRgba8Upload> {
        if self.generation == 0
            || self.width == 0
            || self.height == 0
            || self.format != wgpu::TextureFormat::Rgba8UnormSrgb
        {
            return None;
        }
        let bytes_per_row = self.width.checked_mul(4)?;
        let expected_len = (bytes_per_row as usize).checked_mul(self.height as usize)?;
        (self.pixels.len() == expected_len).then_some(ValidatedRgba8Upload {
            width: self.width,
            height: self.height,
            bytes_per_row,
        })
    }
}

pub(crate) struct ValidatedRgba8Upload {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bytes_per_row: u32,
}

#[cfg(test)]
mod tests;
