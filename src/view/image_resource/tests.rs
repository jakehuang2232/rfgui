#[cfg(not(target_arch = "wasm32"))]
use super::absolute_normalized_path;
use super::{
    ImageEntry, ImageRegistry, ImageSourceIdentity, ImageState, acquire_source_entry,
    evict_image_entries_under_pressure, release_source_entry, remove_registry_entry,
};
use crate::view::ImageSource;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
fn path_identity(value: &str) -> super::NormalizedPathIdentity {
    Arc::new(PathBuf::from(value))
}

#[cfg(target_arch = "wasm32")]
fn path_identity(value: &str) -> super::NormalizedPathIdentity {
    Arc::from(value)
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn normalize_relative_path_without_fs_resolution() {
    let path = absolute_normalized_path(Path::new("./examples/../examples/assets/test.png"));
    let text = path.to_string_lossy();
    assert!(text.ends_with("/examples/assets/test.png"));
    assert!(!text.contains("/./"));
    assert!(!text.contains("/../"));
}

#[test]
#[cfg(all(unix, not(target_arch = "wasm32")))]
fn normalized_native_path_identity_preserves_non_utf8_os_bytes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let left = PathBuf::from(OsString::from_vec(vec![b'a', 0xff]));
    let right = PathBuf::from(OsString::from_vec(vec![b'a', 0xfe]));
    let (_, left) = super::normalized_path_key(&left);
    let (_, right) = super::normalized_path_key(&right);
    assert_ne!(left, right);
}

#[test]
fn image_cache_evicts_unreferenced_entries_but_keeps_live_entries() {
    let mut registry = ImageRegistry::default();
    let mut live_asset_id = None;
    let mut evicted_asset_id = None;
    let reacquire_pixels: Arc<[u8]> = Arc::from([9_u8; 4]);
    for key in 0..4097_u64 {
        let asset_id = super::next_asset_id();
        if key == 0 {
            live_asset_id = Some(asset_id);
        }
        if key == 1 {
            evicted_asset_id = Some(asset_id);
        }
        let pixels: Arc<[u8]> = if key == 1 {
            reacquire_pixels.clone()
        } else {
            Arc::from([0_u8; 4])
        };
        registry.entries.insert(
            asset_id,
            ImageEntry {
                asset_id,
                source_hash: key,
                source_identity: ImageSourceIdentity::InlineRgba {
                    width: 1,
                    height: 1,
                    pixels: pixels.clone(),
                },
                state: ImageState::Ready {
                    width: 1,
                    height: 1,
                    pixels,
                    generation: key + 1,
                },
                ref_count: usize::from(key == 0),
                last_access_tick: key,
                test_state_overridden: false,
            },
        );
        registry.source_buckets.insert(key, vec![asset_id]);
    }

    evict_image_entries_under_pressure(&mut registry);

    assert!(
        registry
            .entries
            .contains_key(&live_asset_id.expect("live id"))
    );
    assert!(registry.entries.len() <= super::IMAGE_CACHE_EVICT_TO_ENTRIES);
    assert_eq!(registry.source_buckets.len(), registry.entries.len());
    let evicted_asset_id = evicted_asset_id.expect("eviction candidate");
    assert!(!registry.entries.contains_key(&evicted_asset_id));
    assert!(!registry.source_buckets.contains_key(&1));
    let (reacquired, inserted) = acquire_source_entry(
        &mut registry,
        1,
        ImageSourceIdentity::InlineRgba {
            width: 1,
            height: 1,
            pixels: reacquire_pixels,
        },
        || ImageState::Loading,
        5000,
    );
    assert!(inserted);
    assert_ne!(reacquired, evicted_asset_id);
}

#[test]
fn forced_hash_collision_never_aliases_distinct_exact_sources() {
    let mut registry = ImageRegistry::default();
    let forced_hash = 17;
    let (left_path, _) = acquire_source_entry(
        &mut registry,
        forced_hash,
        ImageSourceIdentity::Path(path_identity("/tmp/left.png")),
        || ImageState::Loading,
        1,
    );
    let (right_path, _) = acquire_source_entry(
        &mut registry,
        forced_hash,
        ImageSourceIdentity::Path(path_identity("/tmp/right.png")),
        || ImageState::Loading,
        2,
    );
    assert_ne!(left_path, right_path);

    let left_pixels: Arc<[u8]> = Arc::from([1_u8; 4]);
    let right_pixels: Arc<[u8]> = Arc::from([1_u8; 4]);
    let (left_rgba, _) = acquire_source_entry(
        &mut registry,
        forced_hash,
        ImageSourceIdentity::InlineRgba {
            width: 1,
            height: 1,
            pixels: left_pixels,
        },
        || ImageState::Loading,
        3,
    );
    let (right_rgba, _) = acquire_source_entry(
        &mut registry,
        forced_hash,
        ImageSourceIdentity::InlineRgba {
            width: 1,
            height: 1,
            pixels: right_pixels,
        },
        || ImageState::Loading,
        4,
    );
    assert_ne!(left_rgba, right_rgba);
    assert_eq!(registry.source_buckets[&forced_hash].len(), 4);
}

#[test]
fn same_rgba_arc_reuses_id_and_refcounts_while_distinct_arc_does_not() {
    let mut registry = ImageRegistry::default();
    let pixels: Arc<[u8]> = Arc::from([7_u8; 4]);
    let identity = || ImageSourceIdentity::InlineRgba {
        width: 1,
        height: 1,
        pixels: pixels.clone(),
    };
    let (first, inserted) =
        acquire_source_entry(&mut registry, 9, identity(), || ImageState::Loading, 1);
    assert!(inserted);
    let (second, inserted) =
        acquire_source_entry(&mut registry, 9, identity(), || ImageState::Loading, 2);
    assert!(!inserted);
    assert_eq!(first, second);
    assert_eq!(registry.entries[&first].ref_count, 2);
    release_source_entry(&mut registry, first, 3);
    assert_eq!(registry.entries[&first].ref_count, 1);
    release_source_entry(&mut registry, first, 4);
    assert_eq!(registry.entries[&first].ref_count, 0);

    let distinct: Arc<[u8]> = Arc::from([7_u8; 4]);
    let (third, _) = acquire_source_entry(
        &mut registry,
        9,
        ImageSourceIdentity::InlineRgba {
            width: 1,
            height: 1,
            pixels: distinct,
        },
        || ImageState::Loading,
        5,
    );
    assert_ne!(first, third);
}

#[test]
fn entry_removal_cleans_bucket_and_reacquire_allocates_fresh_id() {
    let mut registry = ImageRegistry::default();
    let pixels: Arc<[u8]> = Arc::from([5_u8; 4]);
    let identity = || ImageSourceIdentity::InlineRgba {
        width: 1,
        height: 1,
        pixels: pixels.clone(),
    };
    let (first, _) = acquire_source_entry(&mut registry, 33, identity(), || ImageState::Loading, 1);
    remove_registry_entry(&mut registry, first).expect("entry removed");
    assert!(!registry.source_buckets.contains_key(&33));
    let (second, _) =
        acquire_source_entry(&mut registry, 33, identity(), || ImageState::Loading, 2);
    assert_ne!(first, second);
}

#[test]
fn ready_snapshot_owns_pixels_after_handle_and_registry_entry_drop() {
    let pixels: Arc<[u8]> = Arc::from([11_u8, 22, 33, 44]);
    let handle = super::acquire_image_resource(&ImageSource::Rgba {
        width: 1,
        height: 1,
        pixels: pixels.clone(),
    });
    let asset_id = handle.asset_id();
    let super::ImageSnapshot::Ready(ready) = super::snapshot_image(asset_id).expect("ready image")
    else {
        panic!("inline RGBA must be ready")
    };
    drop(handle);
    let mut registry = super::image_registry().lock().unwrap();
    remove_registry_entry(&mut registry, asset_id).expect("test entry removed");
    drop(registry);
    drop(pixels);
    assert_eq!(ready.pixels.as_ref(), &[11, 22, 33, 44]);
}
