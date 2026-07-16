use crate::view::SvgSource;
use crate::view::image_resource::{ImageSnapshot, ReadyImage};
use crate::view::sampled_texture::{SampledTextureId, SvgRasterAssetId};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};
use rustc_hash::FxHashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const SVG_RASTER_BUCKET_SMALL: u32 = 32;
const SVG_RASTER_BUCKET_LARGE: u32 = 64;
const SVG_RASTER_BUCKET_THRESHOLD: u32 = 256;
const SVG_DOCUMENT_MAX_ENTRIES: usize = 1024;
const SVG_DOCUMENT_EVICT_TO_ENTRIES: usize = 768;
const SVG_DOCUMENT_PRESSURE_BYTES: usize = 32 * 1024 * 1024;
const SVG_DOCUMENT_EVICT_TO_BYTES: usize = 24 * 1024 * 1024;
const SVG_RASTER_MAX_ENTRIES: usize = 1024;
const SVG_RASTER_EVICT_TO_ENTRIES: usize = 768;
const SVG_RASTER_PRESSURE_BYTES: u64 = 32 * 1024 * 1024;
const SVG_RASTER_EVICT_TO_BYTES: u64 = 24 * 1024 * 1024;
pub(crate) const SVG_RASTER_POLICY_REVISION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
enum SvgSourceIdentity {
    Path(Arc<PathBuf>),
    Content(Arc<str>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SvgSourceLookupKey {
    raw_hash: u64,
    identity: SvgSourceIdentity,
}

impl Hash for SvgSourceLookupKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Equality still compares the tagged, owning identity. The pre-hash is
        // only a lookup accelerator and can never be authoritative.
        self.raw_hash.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SvgRasterMode {
    Uniform,
    Fill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SvgRasterRequest {
    pub(crate) physical_width: u32,
    pub(crate) physical_height: u32,
    pub(crate) mode: SvgRasterMode,
}

impl SvgRasterRequest {
    pub(crate) fn new(physical_width: u32, physical_height: u32, mode: SvgRasterMode) -> Self {
        Self {
            physical_width: physical_width.max(1),
            physical_height: physical_height.max(1),
            mode,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SvgRasterLookupKey {
    document_key: u64,
    policy_revision: u32,
    request: SvgRasterRequest,
}

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
    estimated_bytes: usize,
    #[cfg(test)]
    test_state_overridden: bool,
}

#[derive(Default)]
struct SvgDocumentRegistry {
    entries: FxHashMap<u64, SvgDocumentEntry>,
    source_ids: FxHashMap<SvgSourceLookupKey, u64>,
}

impl SvgDocumentEntry {
    fn ref_count(&self) -> usize {
        match &self.origin {
            SvgDocumentOrigin::Path { ref_count, .. }
            | SvgDocumentOrigin::Content { ref_count, .. } => *ref_count,
        }
    }

    fn last_access_tick(&self) -> u64 {
        match &self.origin {
            SvgDocumentOrigin::Path {
                last_access_tick, ..
            }
            | SvgDocumentOrigin::Content {
                last_access_tick, ..
            } => *last_access_tick,
        }
    }
}

#[derive(Debug)]
enum SvgRasterState {
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
struct SvgRasterEntry {
    asset_id: SvgRasterAssetId,
    state: SvgRasterState,
    ref_count: usize,
    last_access_tick: u64,
    #[cfg(test)]
    test_state_overridden: bool,
}

#[derive(Default)]
struct SvgRasterRegistry {
    entries: FxHashMap<u64, SvgRasterEntry>,
    request_ids: FxHashMap<SvgRasterLookupKey, u64>,
}

impl SvgRasterEntry {
    fn byte_size(&self) -> u64 {
        match &self.state {
            SvgRasterState::Ready { width, height, .. } => *width as u64 * *height as u64 * 4,
            _ => 0,
        }
    }
}

fn svg_documents() -> &'static Mutex<SvgDocumentRegistry> {
    static ENTRIES: OnceLock<Mutex<SvgDocumentRegistry>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(SvgDocumentRegistry::default()))
}

fn svg_rasters() -> &'static Mutex<SvgRasterRegistry> {
    static ENTRIES: OnceLock<Mutex<SvgRasterRegistry>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(SvgRasterRegistry::default()))
}

fn next_document_key() -> u64 {
    static NEXT_DOCUMENT_KEY: AtomicU64 = AtomicU64::new(1);
    NEXT_DOCUMENT_KEY
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("SVG document key space exhausted")
}

fn next_raster_key() -> u64 {
    static NEXT_RASTER_KEY: AtomicU64 = AtomicU64::new(1);
    NEXT_RASTER_KEY
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("SVG raster key space exhausted")
}

fn next_generation() -> u64 {
    static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
    NEXT_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("SVG raster generation ID space exhausted")
}

fn next_raster_asset_id() -> SvgRasterAssetId {
    static NEXT_RASTER_ASSET_ID: AtomicU64 = AtomicU64::new(1);
    let raw = NEXT_RASTER_ASSET_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("SVG raster asset ID space exhausted");
    SvgRasterAssetId::new(raw).expect("SVG raster asset ID allocator emitted zero")
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

fn svg_source_lookup_key(source: &SvgSource) -> SvgSourceLookupKey {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let identity = match source {
        SvgSource::Path(path) => {
            let normalized = absolute_normalized_path(path);
            0_u8.hash(&mut hasher);
            normalized.hash(&mut hasher);
            SvgSourceIdentity::Path(Arc::new(normalized))
        }
        SvgSource::Content(content) => {
            1_u8.hash(&mut hasher);
            content.hash(&mut hasher);
            SvgSourceIdentity::Content(Arc::from(content.as_str()))
        }
    };
    SvgSourceLookupKey {
        raw_hash: hasher.finish(),
        identity,
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

fn load_svg_source(source: &SvgSource) -> Result<(Arc<Tree>, f32, f32, usize), Arc<str>> {
    let svg_text = match source {
        SvgSource::Path(path) => fs::read_to_string(path).map_err(|err| {
            Arc::<str>::from(format!("Failed to load svg {}: {err}", path.display()))
        })?,
        SvgSource::Content(content) => content.clone(),
    };
    let source_bytes = svg_text.len();
    let tree = parse_svg_tree(&svg_text)?;
    let (intrinsic_width, intrinsic_height) = parse_svg_intrinsic_size(&tree);
    Ok((
        Arc::new(tree),
        intrinsic_width,
        intrinsic_height,
        source_bytes,
    ))
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

pub(crate) fn quantize_svg_uniform_raster_size(
    intrinsic_width: f32,
    intrinsic_height: f32,
    physical_scale: f32,
) -> (u32, u32) {
    let intrinsic_width = intrinsic_width.max(1.0);
    let intrinsic_height = intrinsic_height.max(1.0);
    let physical_scale = physical_scale.max(0.0001);
    let dominant_intrinsic = intrinsic_width.max(intrinsic_height);
    let dominant_requested = (dominant_intrinsic * physical_scale).ceil().max(1.0) as u32;
    let dominant_extent = quantize_dimension(dominant_requested);
    let uniform_scale = dominant_extent as f32 / dominant_intrinsic;
    (
        (intrinsic_width * uniform_scale).ceil().max(1.0) as u32,
        (intrinsic_height * uniform_scale).ceil().max(1.0) as u32,
    )
}

fn rasterize_svg(tree: &Tree, request: SvgRasterRequest) -> Result<Arc<[u8]>, Arc<str>> {
    let width = request.physical_width;
    let height = request.physical_height;
    let mut pixmap = Pixmap::new(width.max(1), height.max(1))
        .ok_or_else(|| Arc::<str>::from("Failed to allocate svg pixmap"))?;
    let size = tree.size();
    let scale_x = width.max(1) as f32 / size.width().max(1.0);
    let scale_y = height.max(1) as f32 / size.height().max(1.0);
    let (scale_x, scale_y) = match request.mode {
        SvgRasterMode::Uniform => {
            let scale = scale_x.min(scale_y);
            (scale, scale)
        }
        SvgRasterMode::Fill => (scale_x, scale_y),
    };
    resvg::render(
        tree,
        Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );
    // tiny-skia's Pixmap is explicitly an owning container of premultiplied
    // RGBA pixels in sRGB byte encoding. Rgba8UnormSrgb sampling expects
    // straight sRGB, so normalize once on the CPU; the composite shader then
    // premultiplies after the hardware sRGB decode, in linear space.
    let mut pixels = pixmap.take();
    unpremultiply_rgba8_in_place(&mut pixels);
    Ok(Arc::<[u8]>::from(pixels))
}

#[cfg(test)]
pub(crate) fn rasterize_svg_text_for_test(
    svg_text: &str,
    request: SvgRasterRequest,
) -> Result<Arc<[u8]>, Arc<str>> {
    let tree = parse_svg_tree(svg_text)?;
    rasterize_svg(&tree, request)
}

fn unpremultiply_rgba8_in_place(pixels: &mut [u8]) {
    for rgba in pixels.chunks_exact_mut(4) {
        let alpha = u32::from(rgba[3]);
        if alpha == 0 {
            rgba[0] = 0;
            rgba[1] = 0;
            rgba[2] = 0;
            continue;
        }
        for channel in &mut rgba[..3] {
            let premultiplied = u32::from(*channel).min(alpha);
            let straight = (premultiplied * 255 + alpha / 2) / alpha;
            *channel = straight.min(255) as u8;
        }
    }
}

fn total_svg_raster_bytes(rasters: &SvgRasterRegistry) -> u64 {
    rasters
        .entries
        .values()
        .map(SvgRasterEntry::byte_size)
        .sum()
}

fn evict_svg_documents_under_pressure(documents: &mut SvgDocumentRegistry) {
    let mut estimated_bytes = documents
        .entries
        .values()
        .map(|entry| entry.estimated_bytes)
        .sum::<usize>();
    if documents.entries.len() <= SVG_DOCUMENT_MAX_ENTRIES
        && estimated_bytes <= SVG_DOCUMENT_PRESSURE_BYTES
    {
        return;
    }

    let mut candidates = documents
        .entries
        .iter()
        .filter_map(|(key, entry)| {
            (entry.ref_count() == 0).then_some((
                *key,
                entry.last_access_tick(),
                entry.estimated_bytes,
            ))
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable_by_key(|(_, tick, _)| *tick);

    for (key, _, bytes) in candidates {
        if documents.entries.len() <= SVG_DOCUMENT_EVICT_TO_ENTRIES
            && estimated_bytes <= SVG_DOCUMENT_EVICT_TO_BYTES
        {
            break;
        }
        if documents.entries.remove(&key).is_some() {
            documents.source_ids.retain(|_, id| *id != key);
            estimated_bytes = estimated_bytes.saturating_sub(bytes);
        }
    }
}

fn evict_svg_rasters_under_pressure(rasters: &mut SvgRasterRegistry) {
    let mut total_bytes = total_svg_raster_bytes(rasters);
    if rasters.entries.len() <= SVG_RASTER_MAX_ENTRIES && total_bytes <= SVG_RASTER_PRESSURE_BYTES {
        return;
    }

    let mut candidates = rasters
        .entries
        .iter()
        .filter_map(|(key, entry)| {
            if entry.ref_count == 0 {
                Some((*key, entry.last_access_tick, entry.byte_size()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, tick, _)| *tick);

    for (key, _, bytes) in candidates {
        if rasters.entries.len() <= SVG_RASTER_EVICT_TO_ENTRIES
            && total_bytes <= SVG_RASTER_EVICT_TO_BYTES
        {
            break;
        }
        if rasters.entries.remove(&key).is_some() {
            rasters.request_ids.retain(|_, id| *id != key);
            total_bytes = total_bytes.saturating_sub(bytes);
        }
    }
}

pub fn acquire_svg_document(source: &SvgSource) -> u64 {
    let lookup_key = svg_source_lookup_key(source);
    let load_source = match &lookup_key.identity {
        SvgSourceIdentity::Path(path) => SvgSource::Path(path.as_ref().clone()),
        SvgSourceIdentity::Content(content) => SvgSource::Content(content.to_string()),
    };
    let key;
    let mut spawn_loader = false;
    {
        let tick = next_access_tick();
        let mut documents = svg_documents().lock().unwrap();
        if let Some(existing_key) = documents.source_ids.get(&lookup_key).copied()
            && documents.entries.contains_key(&existing_key)
        {
            key = existing_key;
            let entry = documents
                .entries
                .get_mut(&key)
                .expect("source index must point at an entry");
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
            key = next_document_key();
            let estimated_bytes = match source {
                SvgSource::Path(_) => match &lookup_key.identity {
                    SvgSourceIdentity::Path(path) => path.as_os_str().len(),
                    SvgSourceIdentity::Content(_) => unreachable!(),
                },
                SvgSource::Content(content) => content.len(),
            };
            let origin = match source {
                SvgSource::Path(_) => SvgDocumentOrigin::Path {
                    ref_count: 1,
                    last_access_tick: tick,
                },
                SvgSource::Content(_) => SvgDocumentOrigin::Content {
                    ref_count: 1,
                    last_access_tick: tick,
                },
            };
            documents.source_ids.insert(lookup_key, key);
            documents.entries.insert(
                key,
                SvgDocumentEntry {
                    state: SvgDocumentState::Loading,
                    origin,
                    estimated_bytes,
                    #[cfg(test)]
                    test_state_overridden: false,
                },
            );
            evict_svg_documents_under_pressure(&mut documents);
            spawn_loader = true;
        }
    }

    if spawn_loader {
        let source = load_source;
        #[cfg(target_arch = "wasm32")]
        {
            let loaded = load_svg_source(&source);
            let mut documents = svg_documents().lock().unwrap();
            let Some(entry) = documents.entries.get_mut(&key) else {
                return key;
            };
            #[cfg(test)]
            if entry.test_state_overridden {
                return key;
            }
            entry.state = match loaded {
                Ok((tree, intrinsic_width, intrinsic_height, source_bytes)) => {
                    entry.estimated_bytes = source_bytes;
                    SvgDocumentState::Ready {
                        tree,
                        intrinsic_width,
                        intrinsic_height,
                    }
                }
                Err(message) => {
                    entry.estimated_bytes = message.len();
                    SvgDocumentState::Error { message }
                }
            };
            evict_svg_documents_under_pressure(&mut documents);
            mark_redraw_dirty();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                let loaded = load_svg_source(&source);
                let mut documents = svg_documents().lock().unwrap();
                let Some(entry) = documents.entries.get_mut(&key) else {
                    return;
                };
                #[cfg(test)]
                if entry.test_state_overridden {
                    return;
                }
                entry.state = match loaded {
                    Ok((tree, intrinsic_width, intrinsic_height, source_bytes)) => {
                        entry.estimated_bytes = source_bytes;
                        SvgDocumentState::Ready {
                            tree,
                            intrinsic_width,
                            intrinsic_height,
                        }
                    }
                    Err(message) => {
                        entry.estimated_bytes = message.len();
                        SvgDocumentState::Error { message }
                    }
                };
                evict_svg_documents_under_pressure(&mut documents);
                mark_redraw_dirty();
            });
        }
    }

    key
}

pub fn release_svg_document(key: u64) {
    let tick = next_access_tick();
    let mut documents = svg_documents().lock().unwrap();
    let Some(entry) = documents.entries.get_mut(&key) else {
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
    evict_svg_documents_under_pressure(&mut documents);
}

pub fn snapshot_svg_document(key: u64) -> Option<SvgDocumentSnapshot> {
    let mut documents = svg_documents().lock().unwrap();
    let entry = documents.entries.get_mut(&key)?;
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

pub(crate) fn acquire_svg_raster(document_key: u64, request: SvgRasterRequest) -> u64 {
    let lookup_key = SvgRasterLookupKey {
        document_key,
        policy_revision: SVG_RASTER_POLICY_REVISION,
        request,
    };
    let key;
    let mut spawn_raster = false;
    let mut tree: Option<Arc<Tree>> = None;
    {
        let tick = next_access_tick();
        let mut rasters = svg_rasters().lock().unwrap();
        if let Some(existing_key) = rasters.request_ids.get(&lookup_key).copied()
            && rasters.entries.contains_key(&existing_key)
        {
            key = existing_key;
            let entry = rasters
                .entries
                .get_mut(&key)
                .expect("raster request index must point at an entry");
            entry.ref_count += 1;
            entry.last_access_tick = tick;
            return key;
        }
        key = next_raster_key();
        rasters.request_ids.insert(lookup_key, key);
        rasters.entries.insert(
            key,
            SvgRasterEntry {
                asset_id: next_raster_asset_id(),
                state: SvgRasterState::Loading,
                ref_count: 1,
                last_access_tick: tick,
                #[cfg(test)]
                test_state_overridden: false,
            },
        );
        let documents = svg_documents().lock().unwrap();
        if let Some(document) = documents.entries.get(&document_key)
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
        #[cfg(target_arch = "wasm32")]
        {
            let rasterized = rasterize_svg(tree.as_ref(), request);
            let mut rasters = svg_rasters().lock().unwrap();
            let Some(entry) = rasters.entries.get_mut(&key) else {
                return key;
            };
            #[cfg(test)]
            if entry.test_state_overridden {
                return key;
            }
            entry.state = match rasterized {
                Ok(pixels) => SvgRasterState::Ready {
                    width: request.physical_width,
                    height: request.physical_height,
                    pixels,
                    generation: next_generation(),
                },
                Err(message) => SvgRasterState::Error { message },
            };
            entry.last_access_tick = next_access_tick();
            evict_svg_rasters_under_pressure(&mut rasters);
            mark_redraw_dirty();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::thread::spawn(move || {
                let rasterized = rasterize_svg(tree.as_ref(), request);
                let mut rasters = svg_rasters().lock().unwrap();
                let Some(entry) = rasters.entries.get_mut(&key) else {
                    return;
                };
                #[cfg(test)]
                if entry.test_state_overridden {
                    return;
                }
                entry.state = match rasterized {
                    Ok(pixels) => SvgRasterState::Ready {
                        width: request.physical_width,
                        height: request.physical_height,
                        pixels,
                        generation: next_generation(),
                    },
                    Err(message) => SvgRasterState::Error { message },
                };
                entry.last_access_tick = next_access_tick();
                evict_svg_rasters_under_pressure(&mut rasters);
                mark_redraw_dirty();
            });
        }
    }

    key
}

pub fn release_svg_raster(key: u64) {
    let tick = next_access_tick();
    let mut rasters = svg_rasters().lock().unwrap();
    let Some(entry) = rasters.entries.get_mut(&key) else {
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
    let entry = rasters.entries.get_mut(&key)?;
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
            sampled_texture_id: SampledTextureId::SvgRaster(entry.asset_id),
            width: *width,
            height: *height,
            pixels: pixels.clone(),
            generation: *generation,
        })),
        SvgRasterState::Error { message } => Some(ImageSnapshot::Error(message.clone())),
    }
}

pub(crate) fn svg_raster_asset_id_for_request(
    key: u64,
    document_key: u64,
    request: SvgRasterRequest,
) -> Option<SvgRasterAssetId> {
    let rasters = svg_rasters().lock().unwrap();
    let lookup = SvgRasterLookupKey {
        document_key,
        policy_revision: SVG_RASTER_POLICY_REVISION,
        request,
    };
    if rasters.request_ids.get(&lookup).copied() != Some(key) {
        return None;
    }
    rasters.entries.get(&key).map(|entry| entry.asset_id)
}

#[cfg(test)]
pub(crate) fn svg_raster_ref_count_for_test(key: u64) -> Option<usize> {
    svg_rasters()
        .lock()
        .unwrap()
        .entries
        .get(&key)
        .map(|entry| entry.ref_count)
}

#[cfg(test)]
pub(crate) fn set_svg_raster_error_for_test(key: u64) {
    if let Some(entry) = svg_rasters().lock().unwrap().entries.get_mut(&key) {
        entry.state = SvgRasterState::Error {
            message: Arc::from("forced raster error"),
        };
        entry.test_state_overridden = true;
    }
}

#[cfg(test)]
pub(crate) fn set_svg_raster_ready_for_test(key: u64, width: u32, height: u32) {
    replace_svg_raster_ready_for_test(
        key,
        width,
        height,
        Arc::from(vec![0_u8; (width * height * 4) as usize]),
    );
}

#[cfg(test)]
pub(crate) fn replace_svg_raster_ready_for_test(
    key: u64,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
) -> u64 {
    let generation = next_generation();
    if let Some(entry) = svg_rasters().lock().unwrap().entries.get_mut(&key) {
        entry.state = SvgRasterState::Ready {
            width,
            height,
            pixels,
            generation,
        };
        entry.test_state_overridden = true;
    }
    generation
}

#[cfg(test)]
pub(crate) fn set_svg_raster_loading_for_test(key: u64) {
    if let Some(entry) = svg_rasters().lock().unwrap().entries.get_mut(&key) {
        entry.state = SvgRasterState::Loading;
        entry.test_state_overridden = true;
    }
}

#[cfg(test)]
pub(crate) fn prime_svg_document_ready_for_test(
    source: &SvgSource,
    intrinsic_width: f32,
    intrinsic_height: f32,
) -> u64 {
    let svg_text = format!(
        r#"<svg width="{intrinsic_width}" height="{intrinsic_height}" xmlns="http://www.w3.org/2000/svg"><rect width="100%" height="100%"/></svg>"#
    );
    let tree = Arc::new(parse_svg_tree(&svg_text).expect("test SVG document must parse"));
    let lookup = svg_source_lookup_key(source);
    let key = next_document_key();
    let tick = next_access_tick();
    let origin = match source {
        SvgSource::Path(_) => SvgDocumentOrigin::Path {
            ref_count: 0,
            last_access_tick: tick,
        },
        SvgSource::Content(_) => SvgDocumentOrigin::Content {
            ref_count: 0,
            last_access_tick: tick,
        },
    };
    let mut documents = svg_documents().lock().unwrap();
    assert!(
        !documents.source_ids.contains_key(&lookup),
        "test SVG source must be unique"
    );
    documents.source_ids.insert(lookup, key);
    documents.entries.insert(
        key,
        SvgDocumentEntry {
            state: SvgDocumentState::Ready {
                tree,
                intrinsic_width,
                intrinsic_height,
            },
            origin,
            estimated_bytes: svg_text.len(),
            test_state_overridden: true,
        },
    );
    key
}

#[cfg(test)]
pub(crate) fn set_svg_document_loading_for_test(key: u64) {
    let mut documents = svg_documents().lock().unwrap();
    let entry = documents
        .entries
        .get_mut(&key)
        .expect("test SVG document entry must exist");
    entry.state = SvgDocumentState::Loading;
    entry.test_state_overridden = true;
}

#[cfg(test)]
pub(crate) fn set_svg_document_error_for_test(key: u64) {
    let mut documents = svg_documents().lock().unwrap();
    let entry = documents
        .entries
        .get_mut(&key)
        .expect("test SVG document entry must exist");
    entry.state = SvgDocumentState::Error {
        message: Arc::from("forced document error"),
    };
    entry.test_state_overridden = true;
}

#[cfg(test)]
pub(crate) fn prime_svg_raster_ready_for_test(
    document_key: u64,
    request: SvgRasterRequest,
    pixels: Arc<[u8]>,
) -> (u64, u64) {
    let lookup = SvgRasterLookupKey {
        document_key,
        policy_revision: SVG_RASTER_POLICY_REVISION,
        request,
    };
    let generation = next_generation();
    let mut rasters = svg_rasters().lock().unwrap();
    if let Some(key) = rasters.request_ids.get(&lookup).copied()
        && let Some(entry) = rasters.entries.get_mut(&key)
    {
        entry.state = SvgRasterState::Ready {
            width: request.physical_width,
            height: request.physical_height,
            pixels,
            generation,
        };
        entry.test_state_overridden = true;
        return (key, generation);
    }
    let key = next_raster_key();
    rasters.request_ids.insert(lookup, key);
    rasters.entries.insert(
        key,
        SvgRasterEntry {
            asset_id: next_raster_asset_id(),
            state: SvgRasterState::Ready {
                width: request.physical_width,
                height: request.physical_height,
                pixels,
                generation,
            },
            ref_count: 0,
            last_access_tick: next_access_tick(),
            test_state_overridden: true,
        },
    );
    (key, generation)
}

#[cfg(test)]
pub(crate) fn remove_svg_raster_entry_for_test(key: u64) {
    let mut rasters = svg_rasters().lock().unwrap();
    let ref_count = rasters
        .entries
        .get(&key)
        .expect("test SVG raster entry must exist")
        .ref_count;
    assert_eq!(
        ref_count, 0,
        "test registry eviction requires every SVG raster lease to be dropped"
    );
    rasters.entries.remove(&key);
    rasters
        .request_ids
        .retain(|_, indexed_key| *indexed_key != key);
}

#[cfg(test)]
pub(crate) fn remove_svg_document_entry_for_test(key: u64) {
    let mut documents = svg_documents().lock().unwrap();
    let ref_count = documents
        .entries
        .get(&key)
        .expect("test SVG document entry must exist")
        .ref_count();
    assert_eq!(
        ref_count, 0,
        "test registry eviction requires every SVG document lease to be dropped"
    );
    documents.entries.remove(&key);
    documents
        .source_ids
        .retain(|_, indexed_key| *indexed_key != key);
}

#[cfg(test)]
mod tests {
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
        let pixels =
            rasterize_svg(&tree, SvgRasterRequest::new(4, 4, SvgRasterMode::Fill)).unwrap();
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
}
