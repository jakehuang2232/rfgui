use crate::view::ImageSource;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct ReadyImage {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageAssetRetentionInfo {
    pub ref_count: usize,
    pub last_access_tick: u64,
}

#[derive(Debug)]
enum ImageState {
    Loading,
    Ready {
        width: u32,
        height: u32,
        pixels: Arc<[u8]>,
        generation: u64,
        uploaded_generation: Option<u64>,
    },
    Error {
        message: Arc<str>,
    },
}

#[derive(Debug)]
enum ImageOrigin {
    Path {
        _normalized_path: Arc<str>,
        ref_count: usize,
        last_access_tick: u64,
    },
    Rgba,
}

#[derive(Debug)]
struct ImageEntry {
    state: ImageState,
    origin: ImageOrigin,
}

fn image_entries() -> &'static Mutex<HashMap<u64, ImageEntry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<u64, ImageEntry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_generation() -> u64 {
    static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
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

fn normalized_path_key(path: &Path) -> (u64, Arc<str>) {
    let normalized = absolute_normalized_path(path);
    let normalized_string = normalized.to_string_lossy().into_owned();
    let normalized_path: Arc<str> = Arc::from(normalized_string.as_str());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized_path.hash(&mut hasher);
    (hasher.finish(), normalized_path)
}

fn rgba_key(width: u32, height: u32, pixels: &Arc<[u8]>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    Arc::as_ptr(pixels).hash(&mut hasher);
    pixels.len().hash(&mut hasher);
    hasher.finish()
}

fn decode_path_image(path: &Path) -> Result<(u32, u32, Arc<[u8]>), Arc<str>> {
    let decoded = image::open(path).map_err(|err| {
        Arc::<str>::from(format!("Failed to load image {}: {err}", path.display()))
    })?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok((width, height, Arc::<[u8]>::from(rgba.into_raw())))
}

pub fn acquire_image_resource(source: &ImageSource) -> u64 {
    match source {
        ImageSource::Path(path) => {
            let (key, normalized_path) = normalized_path_key(path);
            let mut spawn_loader = false;
            {
                let tick = next_access_tick();
                let mut entries = image_entries().lock().unwrap();
                if let Some(entry) = entries.get_mut(&key) {
                    if let ImageOrigin::Path {
                        ref mut ref_count,
                        ref mut last_access_tick,
                        ..
                    } = entry.origin
                    {
                        *ref_count += 1;
                        *last_access_tick = tick;
                    }
                } else {
                    entries.insert(
                        key,
                        ImageEntry {
                            state: ImageState::Loading,
                            origin: ImageOrigin::Path {
                                _normalized_path: normalized_path.clone(),
                                ref_count: 1,
                                last_access_tick: tick,
                            },
                        },
                    );
                    spawn_loader = true;
                }
            }
            if spawn_loader {
                std::thread::spawn(move || {
                    let decoded = decode_path_image(Path::new(normalized_path.as_ref()));
                    let mut entries = image_entries().lock().unwrap();
                    let Some(entry) = entries.get_mut(&key) else {
                        return;
                    };
                    match decoded {
                        Ok((width, height, pixels)) => {
                            let generation = next_generation();
                            entry.state = ImageState::Ready {
                                width,
                                height,
                                pixels,
                                generation,
                                uploaded_generation: None,
                            };
                        }
                        Err(message) => {
                            entry.state = ImageState::Error { message };
                        }
                    }
                    mark_redraw_dirty();
                });
            }
            key
        }
        ImageSource::Rgba {
            width,
            height,
            pixels,
        } => {
            let key = rgba_key(*width, *height, pixels);
            let mut entries = image_entries().lock().unwrap();
            entries.entry(key).or_insert_with(|| ImageEntry {
                state: ImageState::Ready {
                    width: (*width).max(1),
                    height: (*height).max(1),
                    pixels: pixels.clone(),
                    generation: next_generation(),
                    uploaded_generation: None,
                },
                origin: ImageOrigin::Rgba,
            });
            key
        }
    }
}

pub fn release_image_resource(source: &ImageSource, key: u64) {
    if !matches!(source, ImageSource::Path(_)) {
        return;
    }
    let tick = next_access_tick();
    let mut entries = image_entries().lock().unwrap();
    let Some(entry) = entries.get_mut(&key) else {
        return;
    };
    if let ImageOrigin::Path {
        ref mut ref_count,
        ref mut last_access_tick,
        ..
    } = entry.origin
    {
        if *ref_count > 0 {
            *ref_count -= 1;
        }
        *last_access_tick = tick;
    }
}

pub fn snapshot_image(key: u64) -> Option<ImageSnapshot> {
    let mut entries = image_entries().lock().unwrap();
    let entry = entries.get_mut(&key)?;
    if let ImageOrigin::Path {
        ref mut last_access_tick,
        ..
    } = entry.origin
    {
        *last_access_tick = next_access_tick();
    }
    match &entry.state {
        ImageState::Loading => Some(ImageSnapshot::Loading),
        ImageState::Ready {
            width,
            height,
            pixels,
            generation,
            ..
        } => Some(ImageSnapshot::Ready(ReadyImage {
            width: *width,
            height: *height,
            pixels: pixels.clone(),
            generation: *generation,
        })),
        ImageState::Error { message } => Some(ImageSnapshot::Error(message.clone())),
    }
}

pub fn needs_upload(key: u64, generation: u64) -> bool {
    let entries = image_entries().lock().unwrap();
    let Some(entry) = entries.get(&key) else {
        return false;
    };
    match &entry.state {
        ImageState::Ready {
            uploaded_generation,
            ..
        } => uploaded_generation != &Some(generation),
        _ => false,
    }
}

pub fn mark_uploaded(key: u64, generation: u64) {
    let mut entries = image_entries().lock().unwrap();
    let Some(entry) = entries.get_mut(&key) else {
        return;
    };
    if let ImageState::Ready {
        uploaded_generation,
        ..
    } = &mut entry.state
    {
        *uploaded_generation = Some(generation);
    }
}

pub fn invalidate_uploaded_images() {
    let mut entries = image_entries().lock().unwrap();
    for entry in entries.values_mut() {
        if let ImageState::Ready {
            uploaded_generation,
            ..
        } = &mut entry.state
        {
            *uploaded_generation = None;
        }
    }
}

pub fn image_asset_retention_info(key: u64) -> Option<ImageAssetRetentionInfo> {
    let entries = image_entries().lock().unwrap();
    let entry = entries.get(&key)?;
    match &entry.origin {
        ImageOrigin::Path {
            ref_count,
            last_access_tick,
            ..
        } => Some(ImageAssetRetentionInfo {
            ref_count: *ref_count,
            last_access_tick: *last_access_tick,
        }),
        ImageOrigin::Rgba => None,
    }
}

#[cfg(test)]
mod tests {
    use super::absolute_normalized_path;
    use std::path::Path;

    #[test]
    fn normalize_relative_path_without_fs_resolution() {
        let path = absolute_normalized_path(Path::new("./examples/../examples/assets/test.png"));
        let text = path.to_string_lossy();
        assert!(text.ends_with("/examples/assets/test.png"));
        assert!(!text.contains("/./"));
        assert!(!text.contains("/../"));
    }
}
