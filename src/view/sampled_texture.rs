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
mod tests {
    use super::{
        ImageAssetId, SampledTextureAlphaMode, SampledTextureId, SampledTextureUpload,
        SvgRasterAssetId,
    };
    use crate::view::ImageSampling;
    use rustc_hash::FxHashSet;
    use std::sync::Arc;

    #[test]
    fn equal_registry_local_ids_do_not_alias_across_asset_kinds() {
        let mut ids = FxHashSet::default();
        ids.insert(SampledTextureId::Image(ImageAssetId::for_test(7)));
        ids.insert(SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(7)));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn upload_validation_fails_closed_before_gpu_cache_mutation() {
        let base = SampledTextureUpload {
            id: SampledTextureId::Image(ImageAssetId::for_test(1)),
            generation: 1,
            width: 1,
            height: 1,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha_mode: SampledTextureAlphaMode::Straight,
            pixels: Arc::from([0_u8; 4]),
            sampling: ImageSampling::Linear,
        };
        assert!(base.validate_rgba8().is_some());

        let mut invalid = base.clone();
        invalid.generation = 0;
        assert!(invalid.validate_rgba8().is_none());

        let mut invalid = base.clone();
        invalid.width = 0;
        assert!(invalid.validate_rgba8().is_none());

        let mut invalid = base.clone();
        invalid.format = wgpu::TextureFormat::Rgba16Float;
        assert!(invalid.validate_rgba8().is_none());

        let mut invalid = base.clone();
        invalid.pixels = Arc::from([0_u8; 3]);
        assert!(invalid.validate_rgba8().is_none());

        let mut invalid = base;
        invalid.width = u32::MAX;
        assert!(invalid.validate_rgba8().is_none());
    }

    #[test]
    #[should_panic(expected = "ImageAssetId must be non-zero")]
    fn zero_asset_id_is_rejected_by_constructor() {
        let _ = ImageAssetId::for_test(0);
    }
}
