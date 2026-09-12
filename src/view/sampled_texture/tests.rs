use super::{
    ImageAssetId, SampledTextureAlphaMode, SampledTextureId, SampledTextureUpload, SvgRasterAssetId,
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
