use crate::view::ImageSource;
use crate::view::sampled_texture::{ImageAssetId, SampledTextureId};
#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;
use rustc_hash::FxHashMap;
use std::hash::{Hash, Hasher};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Component;
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::{JsFuture, spawn_local};

const IMAGE_CACHE_MAX_ENTRIES: usize = 4096;
const IMAGE_CACHE_EVICT_TO_ENTRIES: usize = 3072;
const IMAGE_CACHE_PRESSURE_BYTES: usize = 64 * 1024 * 1024;
const IMAGE_CACHE_EVICT_TO_BYTES: usize = 48 * 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
type NormalizedPathIdentity = Arc<PathBuf>;
#[cfg(target_arch = "wasm32")]
type NormalizedPathIdentity = Arc<str>;

#[derive(Clone, Debug)]
pub struct ReadyImage {
    pub(crate) sampled_texture_id: SampledTextureId,
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>,
    pub generation: u64,
}

#[derive(Clone, Debug)]
pub enum ImageSnapshot {
    Loading,
    Ready(ReadyImage),
    Error(Arc<str>),
}

#[derive(Debug)]
enum ImageState {
    Loading,
    Ready {
        width: u32,
        height: u32,
        pixels: Arc<[u8]>,
        generation: u64,
    },
    Error {
        message: Arc<str>,
    },
}

#[derive(Debug)]
struct ImageEntry {
    asset_id: ImageAssetId,
    source_hash: u64,
    source_identity: ImageSourceIdentity,
    state: ImageState,
    ref_count: usize,
    last_access_tick: u64,
    #[cfg(test)]
    test_state_overridden: bool,
}

#[derive(Debug)]
enum ImageSourceIdentity {
    Path(NormalizedPathIdentity),
    InlineRgba {
        width: u32,
        height: u32,
        pixels: Arc<[u8]>,
    },
}

impl ImageSourceIdentity {
    fn exactly_matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Path(left), Self::Path(right)) => left == right,
            (
                Self::InlineRgba {
                    width: left_width,
                    height: left_height,
                    pixels: left_pixels,
                },
                Self::InlineRgba {
                    width: right_width,
                    height: right_height,
                    pixels: right_pixels,
                },
            ) => {
                left_width == right_width
                    && left_height == right_height
                    && Arc::ptr_eq(left_pixels, right_pixels)
            }
            _ => false,
        }
    }
}

#[derive(Default)]
struct ImageRegistry {
    entries: FxHashMap<ImageAssetId, ImageEntry>,
    source_buckets: FxHashMap<u64, Vec<ImageAssetId>>,
}

impl ImageEntry {
    fn estimated_bytes(&self) -> usize {
        match &self.state {
            ImageState::Loading => 0,
            ImageState::Ready { pixels, .. } => pixels.len(),
            ImageState::Error { message } => message.len(),
        }
    }
}

fn remove_registry_entry(
    registry: &mut ImageRegistry,
    asset_id: ImageAssetId,
) -> Option<ImageEntry> {
    let entry = registry.entries.remove(&asset_id)?;
    if let Some(bucket) = registry.source_buckets.get_mut(&entry.source_hash) {
        bucket.retain(|candidate| *candidate != asset_id);
        if bucket.is_empty() {
            registry.source_buckets.remove(&entry.source_hash);
        }
    }
    Some(entry)
}

fn evict_image_entries_under_pressure(registry: &mut ImageRegistry) {
    let mut estimated_bytes = registry
        .entries
        .values()
        .map(ImageEntry::estimated_bytes)
        .sum::<usize>();
    if registry.entries.len() <= IMAGE_CACHE_MAX_ENTRIES
        && estimated_bytes <= IMAGE_CACHE_PRESSURE_BYTES
    {
        return;
    }

    let mut candidates = registry
        .entries
        .iter()
        .filter_map(|(asset_id, entry)| {
            (entry.ref_count == 0).then_some((
                *asset_id,
                entry.last_access_tick,
                entry.estimated_bytes(),
            ))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|(_, tick, _)| *tick);

    for (asset_id, _, bytes) in candidates {
        if registry.entries.len() <= IMAGE_CACHE_EVICT_TO_ENTRIES
            && estimated_bytes <= IMAGE_CACHE_EVICT_TO_BYTES
        {
            break;
        }
        if remove_registry_entry(registry, asset_id).is_some() {
            estimated_bytes = estimated_bytes.saturating_sub(bytes);
        }
    }
}

/// RAII handle for a cached image asset. Holds a ref on the entry for its
/// lifetime; dropping the handle decrements the ref count so the entry can be
/// evicted under memory pressure.
#[derive(Debug)]
pub struct ImageHandle {
    asset_id: ImageAssetId,
}

impl ImageHandle {
    pub(crate) fn asset_id(&self) -> ImageAssetId {
        self.asset_id
    }
}

impl Drop for ImageHandle {
    fn drop(&mut self) {
        release_image_entry(self.asset_id);
    }
}

fn image_registry() -> &'static Mutex<ImageRegistry> {
    static REGISTRY: OnceLock<Mutex<ImageRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(ImageRegistry::default()))
}

fn next_generation() -> u64 {
    static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
    NEXT_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("image generation ID space exhausted")
}

fn next_asset_id() -> ImageAssetId {
    static NEXT_ASSET_ID: AtomicU64 = AtomicU64::new(1);
    let raw = NEXT_ASSET_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("image asset ID space exhausted");
    ImageAssetId::new(raw).expect("image asset ID allocator emitted zero")
}

fn next_access_tick() -> u64 {
    static ACCESS_TICK: AtomicU64 = AtomicU64::new(1);
    ACCESS_TICK.fetch_add(1, Ordering::Relaxed)
}

fn redraw_dirty_flag() -> &'static AtomicBool {
    static IMAGE_REDRAW_DIRTY: AtomicBool = AtomicBool::new(false);
    &IMAGE_REDRAW_DIRTY
}

fn mark_redraw_dirty() {
    redraw_dirty_flag().store(true, Ordering::Release);
}

pub fn take_image_redraw_dirty() -> bool {
    redraw_dirty_flag().swap(false, Ordering::AcqRel)
}

#[cfg(not(target_arch = "wasm32"))]
fn absolute_normalized_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

#[cfg(not(target_arch = "wasm32"))]
fn normalized_path_key(path: &Path) -> (u64, NormalizedPathIdentity) {
    let normalized = absolute_normalized_path(path);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut hasher);
    (hasher.finish(), Arc::new(normalized))
}

#[cfg(not(target_arch = "wasm32"))]
fn path_source_key(path: &Path) -> (u64, NormalizedPathIdentity) {
    normalized_path_key(path)
}

#[cfg(target_arch = "wasm32")]
fn path_source_key(path: &Path) -> (u64, NormalizedPathIdentity) {
    let mut url = path.to_string_lossy().replace('\\', "/");
    while let Some(stripped) = url.strip_prefix("./") {
        url = stripped.to_string();
    }
    let url: Arc<str> = Arc::from(url.as_str());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    (hasher.finish(), url)
}

fn rgba_key(width: u32, height: u32, pixels: &Arc<[u8]>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    Arc::as_ptr(pixels).hash(&mut hasher);
    pixels.len().hash(&mut hasher);
    hasher.finish()
}

fn find_source_asset_id(
    registry: &ImageRegistry,
    source_hash: u64,
    identity: &ImageSourceIdentity,
) -> Option<ImageAssetId> {
    registry
        .source_buckets
        .get(&source_hash)?
        .iter()
        .copied()
        .find(|asset_id| {
            registry
                .entries
                .get(asset_id)
                .is_some_and(|entry| entry.source_identity.exactly_matches(identity))
        })
}

fn insert_source_entry(
    registry: &mut ImageRegistry,
    source_hash: u64,
    source_identity: ImageSourceIdentity,
    state: ImageState,
    tick: u64,
) -> ImageAssetId {
    let asset_id = next_asset_id();
    registry.entries.insert(
        asset_id,
        ImageEntry {
            asset_id,
            source_hash,
            source_identity,
            state,
            ref_count: 1,
            last_access_tick: tick,
            #[cfg(test)]
            test_state_overridden: false,
        },
    );
    registry
        .source_buckets
        .entry(source_hash)
        .or_default()
        .push(asset_id);
    asset_id
}

fn acquire_source_entry(
    registry: &mut ImageRegistry,
    source_hash: u64,
    source_identity: ImageSourceIdentity,
    initial_state: impl FnOnce() -> ImageState,
    tick: u64,
) -> (ImageAssetId, bool) {
    if let Some(asset_id) = find_source_asset_id(registry, source_hash, &source_identity) {
        let entry = registry
            .entries
            .get_mut(&asset_id)
            .expect("source index must reference a live image entry");
        entry.ref_count += 1;
        entry.last_access_tick = tick;
        (asset_id, false)
    } else {
        (
            insert_source_entry(
                registry,
                source_hash,
                source_identity,
                initial_state(),
                tick,
            ),
            true,
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_path_image(path: &Path) -> Result<(u32, u32, Arc<[u8]>), Arc<str>> {
    let decoded = image::open(path).map_err(|err| {
        Arc::<str>::from(format!("Failed to load image {}: {err}", path.display()))
    })?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok((width, height, Arc::<[u8]>::from(rgba.into_raw())))
}

#[cfg(target_arch = "wasm32")]
async fn fetch_path_image(url: &str) -> Result<(u32, u32, Arc<[u8]>), Arc<str>> {
    let window = web_sys::window().ok_or_else(|| Arc::<str>::from("window not available"))?;
    let response_value = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|err| Arc::<str>::from(format!("Failed to fetch image {url}: {err:?}")))?;
    let response: web_sys::Response = response_value
        .dyn_into()
        .map_err(|err| Arc::<str>::from(format!("Failed to read image response {url}: {err:?}")))?;
    if !response.ok() {
        return Err(Arc::<str>::from(format!("Failed to fetch image {url}")));
    }
    let buffer =
        JsFuture::from(response.array_buffer().map_err(|err| {
            Arc::<str>::from(format!("Failed to read image bytes {url}: {err:?}"))
        })?)
        .await
        .map_err(|err| Arc::<str>::from(format!("Failed to read image bytes {url}: {err:?}")))?;
    let bytes = Uint8Array::new(&buffer).to_vec();
    let decoded = image::load_from_memory(&bytes)
        .map_err(|err| Arc::<str>::from(format!("Failed to decode image {url}: {err}")))?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok((width, height, Arc::<[u8]>::from(rgba.into_raw())))
}

pub fn acquire_image_resource(source: &ImageSource) -> ImageHandle {
    match source {
        ImageSource::Path(path) => {
            let (source_hash, path_key) = path_source_key(path);
            let source_identity = ImageSourceIdentity::Path(path_key.clone());
            let asset_id = {
                let tick = next_access_tick();
                let mut registry = image_registry().lock().unwrap();
                let (asset_id, inserted) = acquire_source_entry(
                    &mut registry,
                    source_hash,
                    source_identity,
                    || ImageState::Loading,
                    tick,
                );
                (asset_id, inserted)
            };
            let (asset_id, spawn_loader) = asset_id;
            if spawn_loader {
                #[cfg(target_arch = "wasm32")]
                {
                    let url = path_key.to_string();
                    spawn_local(async move {
                        let decoded = fetch_path_image(&url).await;
                        let mut registry = image_registry().lock().unwrap();
                        let Some(entry) = registry.entries.get_mut(&asset_id) else {
                            return;
                        };
                        #[cfg(test)]
                        if entry.test_state_overridden {
                            return;
                        }
                        match decoded {
                            Ok((width, height, pixels)) => {
                                let generation = next_generation();
                                entry.state = ImageState::Ready {
                                    width,
                                    height,
                                    pixels,
                                    generation,
                                };
                            }
                            Err(message) => {
                                entry.state = ImageState::Error { message };
                            }
                        }
                        evict_image_entries_under_pressure(&mut registry);
                        mark_redraw_dirty();
                    });
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    std::thread::spawn(move || {
                        let decoded = decode_path_image(path_key.as_path());
                        let mut registry = image_registry().lock().unwrap();
                        let Some(entry) = registry.entries.get_mut(&asset_id) else {
                            return;
                        };
                        #[cfg(test)]
                        if entry.test_state_overridden {
                            return;
                        }
                        match decoded {
                            Ok((width, height, pixels)) => {
                                let generation = next_generation();
                                entry.state = ImageState::Ready {
                                    width,
                                    height,
                                    pixels,
                                    generation,
                                };
                            }
                            Err(message) => {
                                entry.state = ImageState::Error { message };
                            }
                        }
                        evict_image_entries_under_pressure(&mut registry);
                        mark_redraw_dirty();
                    });
                }
            }
            ImageHandle { asset_id }
        }
        ImageSource::Rgba {
            width,
            height,
            pixels,
        } => {
            let source_hash = rgba_key(*width, *height, pixels);
            let source_identity = ImageSourceIdentity::InlineRgba {
                width: *width,
                height: *height,
                pixels: pixels.clone(),
            };
            let tick = next_access_tick();
            let mut registry = image_registry().lock().unwrap();
            let (asset_id, _) = acquire_source_entry(
                &mut registry,
                source_hash,
                source_identity,
                || ImageState::Ready {
                    // Preserve the public dimensions exactly. Zero extent
                    // is malformed input and must fail PreparedImage upload
                    // validation rather than becoming a synthetic 1x1.
                    width: *width,
                    height: *height,
                    pixels: pixels.clone(),
                    generation: next_generation(),
                },
                tick,
            );
            evict_image_entries_under_pressure(&mut registry);
            ImageHandle { asset_id }
        }
    }
}

fn release_image_entry(asset_id: ImageAssetId) {
    let tick = next_access_tick();
    let mut registry = image_registry().lock().unwrap();
    release_source_entry(&mut registry, asset_id, tick);
    evict_image_entries_under_pressure(&mut registry);
}

fn release_source_entry(registry: &mut ImageRegistry, asset_id: ImageAssetId, tick: u64) {
    let Some(entry) = registry.entries.get_mut(&asset_id) else {
        return;
    };
    if entry.ref_count > 0 {
        entry.ref_count -= 1;
    }
    entry.last_access_tick = tick;
}

pub fn snapshot_image(asset_id: ImageAssetId) -> Option<ImageSnapshot> {
    let mut registry = image_registry().lock().unwrap();
    let entry = registry.entries.get_mut(&asset_id)?;
    entry.last_access_tick = next_access_tick();
    match &entry.state {
        ImageState::Loading => Some(ImageSnapshot::Loading),
        ImageState::Ready {
            width,
            height,
            pixels,
            generation,
            ..
        } => Some(ImageSnapshot::Ready(ReadyImage {
            sampled_texture_id: SampledTextureId::Image(entry.asset_id),
            width: *width,
            height: *height,
            pixels: pixels.clone(),
            generation: *generation,
        })),
        ImageState::Error { message } => Some(ImageSnapshot::Error(message.clone())),
    }
}

#[cfg(test)]
pub(crate) fn replace_ready_image_for_test(
    asset_id: ImageAssetId,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
) -> u64 {
    let generation = next_generation();
    let mut registry = image_registry().lock().unwrap();
    let entry = registry
        .entries
        .get_mut(&asset_id)
        .expect("test image entry must exist");
    entry.state = ImageState::Ready {
        width,
        height,
        pixels,
        generation,
    };
    entry.test_state_overridden = true;
    generation
}

#[cfg(test)]
pub(crate) fn set_image_loading_for_test(asset_id: ImageAssetId) {
    let mut registry = image_registry().lock().unwrap();
    let entry = registry
        .entries
        .get_mut(&asset_id)
        .expect("test image entry must exist");
    entry.state = ImageState::Loading;
    entry.test_state_overridden = true;
}

#[cfg(test)]
pub(crate) fn set_image_error_for_test(asset_id: ImageAssetId, message: &'static str) {
    let mut registry = image_registry().lock().unwrap();
    let entry = registry
        .entries
        .get_mut(&asset_id)
        .expect("test image entry must exist");
    entry.state = ImageState::Error {
        message: Arc::from(message),
    };
    entry.test_state_overridden = true;
}

#[cfg(test)]
pub(crate) fn remove_image_entry_for_test(asset_id: ImageAssetId) {
    let mut registry = image_registry().lock().unwrap();
    let ref_count = registry
        .entries
        .get(&asset_id)
        .expect("test image entry must exist")
        .ref_count;
    assert_eq!(
        ref_count, 0,
        "test registry eviction requires every ImageHandle to be dropped"
    );
    remove_registry_entry(&mut registry, asset_id).expect("test image entry removed");
}

#[cfg(test)]
mod tests {
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
        let (first, _) =
            acquire_source_entry(&mut registry, 33, identity(), || ImageState::Loading, 1);
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
        let super::ImageSnapshot::Ready(ready) =
            super::snapshot_image(asset_id).expect("ready image")
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
}
