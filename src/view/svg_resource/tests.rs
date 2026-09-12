use super::{
    SvgDocumentEntry, SvgDocumentOrigin, SvgDocumentRegistry, SvgDocumentState, SvgRasterEntry,
    SvgRasterLookupKey, SvgRasterMode, SvgRasterRegistry, SvgRasterRequest, SvgRasterState,
    SvgSourceIdentity, SvgSourceLookupKey, evict_svg_documents_under_pressure,
    evict_svg_rasters_under_pressure, parse_svg_tree, quantize_svg_raster_size,
    quantize_svg_uniform_raster_size, rasterize_svg, svg_source_lookup_key,
    unpremultiply_rgba8_in_place,
};
use crate::view::SvgSource;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[test]
fn quantize_svg_raster_size_rounds_up_small_sizes_to_32px_buckets() {
    assert_eq!(quantize_svg_raster_size(1, 31), (32, 32));
    assert_eq!(quantize_svg_raster_size(33, 65), (64, 96));
}

#[test]
fn quantize_svg_raster_size_rounds_large_sizes_to_64px_buckets() {
    assert_eq!(quantize_svg_raster_size(257, 513), (320, 576));
}

#[test]
fn uniform_quantization_uses_one_scale_for_wide_and_tall_geometry() {
    assert_eq!(quantize_svg_uniform_raster_size(80.0, 40.0, 1.0), (96, 48));
    assert_eq!(quantize_svg_uniform_raster_size(40.0, 80.0, 1.0), (48, 96));
    assert_eq!(quantize_svg_uniform_raster_size(80.0, 40.0, 2.0), (160, 80));
}

#[test]
fn tagged_source_identity_and_raw_hash_collisions_do_not_alias() {
    let content = svg_source_lookup_key(&SvgSource::Content("same.svg".to_string()));
    let path = svg_source_lookup_key(&SvgSource::Path("same.svg".into()));
    assert_ne!(content, path);

    let left = SvgSourceLookupKey {
        raw_hash: 7,
        identity: SvgSourceIdentity::Content(Arc::from("left")),
    };
    let right = SvgSourceLookupKey {
        raw_hash: 7,
        identity: SvgSourceIdentity::Content(Arc::from("right")),
    };
    let mut keys = FxHashMap::default();
    keys.insert(left, 1_u64);
    keys.insert(right, 2_u64);
    assert_eq!(keys.len(), 2);
}

#[test]
fn raster_identity_includes_policy_revision_physical_extent_and_mode() {
    let base = SvgRasterLookupKey {
        document_key: 11,
        policy_revision: super::SVG_RASTER_POLICY_REVISION,
        request: SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform),
    };
    let mut changed = base;
    changed.policy_revision += 1;
    assert_ne!(base, changed);
    changed = base;
    changed.request.physical_width += 1;
    assert_ne!(base, changed);
    changed = base;
    changed.request.mode = SvgRasterMode::Fill;
    assert_ne!(base, changed);
}

#[test]
fn premultiplied_srgb_bytes_are_normalized_to_straight_exhaustively() {
    for alpha in 0_u32..=255 {
        for premultiplied in 0_u32..=alpha {
            let mut pixel = [premultiplied as u8, 0, 0, alpha as u8];
            unpremultiply_rgba8_in_place(&mut pixel);
            if alpha == 0 {
                assert_eq!(pixel, [0, 0, 0, 0]);
            } else {
                let repremultiplied = (u32::from(pixel[0]) * alpha + 127) / 255;
                assert_eq!(repremultiplied, premultiplied);
                assert_eq!(pixel[3], alpha as u8);
            }
        }
    }
    let mut illegal = [255, 1, 2, 64];
    unpremultiply_rgba8_in_place(&mut illegal);
    assert_eq!(illegal[0], 255);
}

#[test]
fn semitransparent_nonprimary_svg_raster_publishes_straight_srgb() {
    let tree = parse_svg_tree(
        r##"<svg width="4" height="4" xmlns="http://www.w3.org/2000/svg"><rect width="4" height="4" fill="#c86432" fill-opacity="0.5"/></svg>"##,
    )
    .unwrap();
    let pixels = rasterize_svg(&tree, SvgRasterRequest::new(4, 4, SvgRasterMode::Fill)).unwrap();
    let center = &pixels[(2 * 4 + 2) * 4..(2 * 4 + 3) * 4];
    assert_eq!(center[3], 128);
    for (actual, expected) in center[..3].iter().zip([200_u8, 100, 50]) {
        assert!(actual.abs_diff(expected) <= 1, "{actual} != {expected}");
    }
}

#[test]
fn svg_document_cache_evicts_unreferenced_entries() {
    let mut registry = SvgDocumentRegistry::default();
    for key in 0..1025_u64 {
        registry.entries.insert(
            key,
            SvgDocumentEntry {
                state: SvgDocumentState::Loading,
                origin: SvgDocumentOrigin::Content {
                    ref_count: usize::from(key == 0),
                    last_access_tick: key,
                },
                estimated_bytes: 1,
                test_state_overridden: false,
            },
        );
    }

    evict_svg_documents_under_pressure(&mut registry);

    assert!(registry.entries.contains_key(&0));
    assert!(registry.entries.len() <= super::SVG_DOCUMENT_EVICT_TO_ENTRIES);
}

#[test]
fn svg_raster_cache_caps_zero_byte_entries() {
    let mut registry = SvgRasterRegistry::default();
    for key in 0..1025_u64 {
        registry.entries.insert(
            key,
            SvgRasterEntry {
                asset_id: super::next_raster_asset_id(),
                state: SvgRasterState::Loading,
                ref_count: usize::from(key == 0),
                last_access_tick: key,
                test_state_overridden: false,
            },
        );
    }

    evict_svg_rasters_under_pressure(&mut registry);

    assert!(registry.entries.contains_key(&0));
    assert!(registry.entries.len() <= super::SVG_RASTER_EVICT_TO_ENTRIES);
}
