use crate::view::SvgSource;
use crate::view::image_resource::{ImageAssetRetentionInfo, ImageSnapshot, ReadyImage};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const SVG_RASTER_BUCKET_SMALL: u32 = 32;
const SVG_RASTER_BUCKET_LARGE: u32 = 64;
const SVG_RASTER_BUCKET_THRESHOLD: u32 = 256;
const SVG_RASTER_PRESSURE_BYTES: u64 = 32 * 1024 * 1024;
const SVG_RASTER_EVICT_TO_BYTES: u64 = 24 * 1024 * 1024;

#[derive(Clone, Debug)]
pub enum SvgDocumentSnapshot {
    Loading,
    Ready {
        intrinsic_width: f32,
        intrinsic_height: f32,
    },
    Error(Arc<str>),
}

#[derive(Debug)]
enum SvgDocumentState {
    Loading,
    Ready {
        tree: Arc<Tree>,
        intrinsic_width: f32,
        intrinsic_height: f32,
    },
    Error {
        message: Arc<str>,
    },
}

#[derive(Debug)]
enum SvgDocumentOrigin {
    Path {
        _normalized_path: Arc<str>,
        ref_count: usize,
        last_access_tick: u64,
    },
    Content {
        ref_count: usize,
        last_access_tick: u64,
    },
}

#[derive(Debug)]
struct SvgDocumentEntry {
    state: SvgDocumentState,
    origin: SvgDocumentOrigin,
}

#[derive(Debug)]
enum SvgRasterState {
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
struct SvgRasterEntry {
    state: SvgRasterState,
    ref_count: usize,
    last_access_tick: u64,
}

impl SvgRasterEntry {
    fn byte_size(&self) -> u64 {
        match &self.state {
            SvgRasterState::Ready { width, height, .. } => *width as u64 * *height as u64 * 4,
            _ => 0,
        }
    }
}

fn svg_documents() -> &'static Mutex<HashMap<u64, SvgDocumentEntry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<u64, SvgDocumentEntry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn svg_rasters() -> &'static Mutex<HashMap<u64, SvgRasterEntry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<u64, SvgRasterEntry>>> = OnceLock::new();
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
    static SVG_REDRAW_DIRTY: AtomicBool = AtomicBool::new(false);
    &SVG_REDRAW_DIRTY
}

fn mark_redraw_dirty() {
    redraw_dirty_flag().store(true, Ordering::Release);
}

pub fn take_svg_redraw_dirty() -> bool {
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

fn svg_source_key(source: &SvgSource) -> (u64, Option<Arc<str>>) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match source {
        SvgSource::Path(path) => {
            let normalized = absolute_normalized_path(path);
            let normalized_string = normalized.to_string_lossy().into_owned();
            let normalized_path: Arc<str> = Arc::from(normalized_string.as_str());
            normalized_path.hash(&mut hasher);
            (hasher.finish(), Some(normalized_path))
        }
        SvgSource::Content(content) => {
            content.hash(&mut hasher);
            (hasher.finish(), None)
        }
    }
}

fn parse_svg_tree(svg_text: &str) -> Result<Tree, Arc<str>> {
    Tree::from_str(svg_text, &Options::default())
        .map_err(|err| Arc::<str>::from(format!("Failed to parse svg: {err}")))
}

fn parse_svg_intrinsic_size(tree: &Tree) -> (f32, f32) {
    let size = tree.size();
    (size.width().max(1.0), size.height().max(1.0))
}

fn load_svg_source(source: &SvgSource) -> Result<(Arc<Tree>, f32, f32), Arc<str>> {
    let svg_text = match source {
        SvgSource::Path(path) => fs::read_to_string(path).map_err(|err| {
            Arc::<str>::from(format!("Failed to load svg {}: {err}", path.display()))
        })?,
        SvgSource::Content(content) => content.clone(),
    };
    let tree = parse_svg_tree(&svg_text)?;
    let (intrinsic_width, intrinsic_height) = parse_svg_intrinsic_size(&tree);
    Ok((Arc::new(tree), intrinsic_width, intrinsic_height))
}

fn raster_key(document_key: u64, width: u32, height: u32) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    document_key.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    hasher.finish()
}

fn quantize_dimension(size: u32) -> u32 {
    let size = size.max(1);
    let bucket = if size <= SVG_RASTER_BUCKET_THRESHOLD {
        SVG_RASTER_BUCKET_SMALL
    } else {
        SVG_RASTER_BUCKET_LARGE
    };
    size.div_ceil(bucket) * bucket
}

pub fn quantize_svg_raster_size(width: u32, height: u32) -> (u32, u32) {
    (quantize_dimension(width), quantize_dimension(height))
}

fn rasterize_svg(tree: &Tree, width: u32, height: u32) -> Result<Arc<[u8]>, Arc<str>> {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1))
        .ok_or_else(|| Arc::<str>::from("Failed to allocate svg pixmap"))?;
    let size = tree.size();
    let scale_x = width.max(1) as f32 / size.width().max(1.0);
    let scale_y = height.max(1) as f32 / size.height().max(1.0);
    resvg::render(
        tree,
        Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );
    Ok(Arc::<[u8]>::from(pixmap.take()))
}

fn total_svg_raster_bytes(rasters: &HashMap<u64, SvgRasterEntry>) -> u64 {
    rasters.values().map(SvgRasterEntry::byte_size).sum()
}

fn evict_svg_rasters_under_pressure(rasters: &mut HashMap<u64, SvgRasterEntry>) {
    let mut total_bytes = total_svg_raster_bytes(rasters);
    if total_bytes <= SVG_RASTER_PRESSURE_BYTES {
        return;
    }

    let mut candidates = rasters
        .iter()
        .filter_map(|(key, entry)| {
            if entry.ref_count == 0 && entry.byte_size() > 0 {
                Some((*key, entry.last_access_tick, entry.byte_size()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, tick, _)| *tick);

    for (key, _, bytes) in candidates {
        if total_bytes <= SVG_RASTER_EVICT_TO_BYTES {
            break;
        }
        if rasters.remove(&key).is_some() {
            total_bytes = total_bytes.saturating_sub(bytes);
        }
    }
}

pub fn acquire_svg_document(source: &SvgSource) -> u64 {
    let (key, normalized_path) = svg_source_key(source);
    let mut spawn_loader = false;
    {
        let tick = next_access_tick();
        let mut entries = svg_documents().lock().unwrap();
        if let Some(entry) = entries.get_mut(&key) {
            match &mut entry.origin {
                SvgDocumentOrigin::Path {
                    ref_count,
                    last_access_tick,
                    ..
                }
                | SvgDocumentOrigin::Content {
                    ref_count,
                    last_access_tick,
                } => {
                    *ref_count += 1;
                    *last_access_tick = tick;
                }
            }
        } else {
            let origin = match source {
                SvgSource::Path(_) => SvgDocumentOrigin::Path {
                    _normalized_path: normalized_path.expect("path source should normalize"),
                    ref_count: 1,
                    last_access_tick: tick,
                },
                SvgSource::Content(_) => SvgDocumentOrigin::Content {
                    ref_count: 1,
                    last_access_tick: tick,
                },
            };
            entries.insert(
                key,
                SvgDocumentEntry {
                    state: SvgDocumentState::Loading,
                    origin,
                },
            );
            spawn_loader = true;
        }
    }

    if spawn_loader {
        let source = source.clone();
        std::thread::spawn(move || {
            let loaded = load_svg_source(&source);
            let mut entries = svg_documents().lock().unwrap();
            let Some(entry) = entries.get_mut(&key) else {
                return;
            };
            entry.state = match loaded {
                Ok((tree, intrinsic_width, intrinsic_height)) => SvgDocumentState::Ready {
                    tree,
                    intrinsic_width,
                    intrinsic_height,
                },
                Err(message) => SvgDocumentState::Error { message },
            };
            mark_redraw_dirty();
        });
    }

    key
}

pub fn release_svg_document(key: u64) {
    let tick = next_access_tick();
    let mut entries = svg_documents().lock().unwrap();
    let Some(entry) = entries.get_mut(&key) else {
        return;
    };
    match &mut entry.origin {
        SvgDocumentOrigin::Path {
            ref_count,
            last_access_tick,
            ..
        }
        | SvgDocumentOrigin::Content {
            ref_count,
            last_access_tick,
        } => {
            if *ref_count > 0 {
                *ref_count -= 1;
            }
            *last_access_tick = tick;
        }
    }
}

pub fn snapshot_svg_document(key: u64) -> Option<SvgDocumentSnapshot> {
    let mut entries = svg_documents().lock().unwrap();
    let entry = entries.get_mut(&key)?;
    match &mut entry.origin {
        SvgDocumentOrigin::Path {
            last_access_tick, ..
        }
        | SvgDocumentOrigin::Content {
            last_access_tick, ..
        } => *last_access_tick = next_access_tick(),
    }
    match &entry.state {
        SvgDocumentState::Loading => Some(SvgDocumentSnapshot::Loading),
        SvgDocumentState::Ready {
            intrinsic_width,
            intrinsic_height,
            ..
        } => Some(SvgDocumentSnapshot::Ready {
            intrinsic_width: *intrinsic_width,
            intrinsic_height: *intrinsic_height,
        }),
        SvgDocumentState::Error { message } => Some(SvgDocumentSnapshot::Error(message.clone())),
    }
}

pub fn acquire_svg_raster(document_key: u64, width: u32, height: u32) -> u64 {
    let (width, height) = quantize_svg_raster_size(width, height);
    let key = raster_key(document_key, width, height);
    let mut spawn_raster = false;
    let mut tree: Option<Arc<Tree>> = None;
    {
        let tick = next_access_tick();
        let mut rasters = svg_rasters().lock().unwrap();
        if let Some(entry) = rasters.get_mut(&key) {
            entry.ref_count += 1;
            entry.last_access_tick = tick;
            return key;
        }
        rasters.insert(
            key,
            SvgRasterEntry {
                state: SvgRasterState::Loading,
                ref_count: 1,
                last_access_tick: tick,
            },
        );
        let documents = svg_documents().lock().unwrap();
        if let Some(document) = documents.get(&document_key)
            && let SvgDocumentState::Ready { tree: doc_tree, .. } = &document.state
        {
            tree = Some(doc_tree.clone());
            spawn_raster = true;
        }
        evict_svg_rasters_under_pressure(&mut rasters);
    }

    if let Some(tree) = tree
        && spawn_raster
    {
        std::thread::spawn(move || {
            let rasterized = rasterize_svg(tree.as_ref(), width, height);
            let mut rasters = svg_rasters().lock().unwrap();
            let Some(entry) = rasters.get_mut(&key) else {
                return;
            };
            entry.state = match rasterized {
                Ok(pixels) => SvgRasterState::Ready {
                    width,
                    height,
                    pixels,
                    generation: next_generation(),
                    uploaded_generation: None,
                },
                Err(message) => SvgRasterState::Error { message },
            };
            entry.last_access_tick = next_access_tick();
            evict_svg_rasters_under_pressure(&mut rasters);
            mark_redraw_dirty();
        });
    }

    key
}

pub fn release_svg_raster(key: u64) {
    let tick = next_access_tick();
    let mut rasters = svg_rasters().lock().unwrap();
    let Some(entry) = rasters.get_mut(&key) else {
        return;
    };
    if entry.ref_count > 0 {
        entry.ref_count -= 1;
    }
    entry.last_access_tick = tick;
    evict_svg_rasters_under_pressure(&mut rasters);
}

pub fn snapshot_svg_raster(key: u64) -> Option<ImageSnapshot> {
    let mut rasters = svg_rasters().lock().unwrap();
    let entry = rasters.get_mut(&key)?;
    entry.last_access_tick = next_access_tick();
    match &entry.state {
        SvgRasterState::Loading => Some(ImageSnapshot::Loading),
        SvgRasterState::Ready {
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
        SvgRasterState::Error { message } => Some(ImageSnapshot::Error(message.clone())),
    }
}

pub fn needs_upload(key: u64, generation: u64) -> bool {
    let rasters = svg_rasters().lock().unwrap();
    let Some(entry) = rasters.get(&key) else {
        return false;
    };
    match &entry.state {
        SvgRasterState::Ready {
            uploaded_generation,
            ..
        } => uploaded_generation != &Some(generation),
        _ => false,
    }
}

pub fn mark_uploaded(key: u64, generation: u64) {
    let mut rasters = svg_rasters().lock().unwrap();
    let Some(entry) = rasters.get_mut(&key) else {
        return;
    };
    if let SvgRasterState::Ready {
        uploaded_generation,
        ..
    } = &mut entry.state
    {
        *uploaded_generation = Some(generation);
    }
}

pub fn invalidate_uploaded_images() {
    let mut rasters = svg_rasters().lock().unwrap();
    for entry in rasters.values_mut() {
        if let SvgRasterState::Ready {
            uploaded_generation,
            ..
        } = &mut entry.state
        {
            *uploaded_generation = None;
        }
    }
}

pub fn svg_asset_retention_info(key: u64) -> Option<ImageAssetRetentionInfo> {
    let rasters = svg_rasters().lock().unwrap();
    let entry = rasters.get(&key)?;
    Some(ImageAssetRetentionInfo {
        ref_count: entry.ref_count,
        last_access_tick: entry.last_access_tick,
    })
}

#[cfg(test)]
mod tests {
    use super::quantize_svg_raster_size;

    #[test]
    fn quantize_svg_raster_size_rounds_up_small_sizes_to_32px_buckets() {
        assert_eq!(quantize_svg_raster_size(1, 31), (32, 32));
        assert_eq!(quantize_svg_raster_size(33, 65), (64, 96));
    }

    #[test]
    fn quantize_svg_raster_size_rounds_large_sizes_to_64px_buckets() {
        assert_eq!(quantize_svg_raster_size(257, 513), (320, 576));
    }
}
