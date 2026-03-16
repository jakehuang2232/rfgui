use crate::ui::host::ImageSource;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
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

static IMAGE_REDRAW_DIRTY: AtomicBool = AtomicBool::new(false);

fn image_store() -> &'static Mutex<HashMap<u64, ImageState>> {
    static STORE: OnceLock<Mutex<HashMap<u64, ImageState>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mark_redraw_dirty() {
    IMAGE_REDRAW_DIRTY.store(true, Ordering::SeqCst);
}

pub fn take_image_redraw_dirty() -> bool {
    IMAGE_REDRAW_DIRTY.swap(false, Ordering::SeqCst)
}

pub fn image_source_key(source: &ImageSource) -> u64 {
    let mut hasher = DefaultHasher::new();
    match source {
        ImageSource::Path(path) => {
            0_u8.hash(&mut hasher);
            path.hash(&mut hasher);
        }
        ImageSource::Rgba {
            width,
            height,
            pixels,
        } => {
            1_u8.hash(&mut hasher);
            width.hash(&mut hasher);
            height.hash(&mut hasher);
            pixels.as_ptr().hash(&mut hasher);
            pixels.len().hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn ensure_image_resource(source: &ImageSource) -> u64 {
    let key = image_source_key(source);
    let mut store = image_store().lock().unwrap();
    if store.contains_key(&key) {
        return key;
    }

    match source {
        ImageSource::Rgba {
            width,
            height,
            pixels,
        } => {
            store.insert(
                key,
                ImageState::Ready {
                    width: *width,
                    height: *height,
                    pixels: pixels.clone(),
                    generation: 1,
                    uploaded_generation: None,
                },
            );
            mark_redraw_dirty();
        }
        ImageSource::Path(path) => {
            let path = path.clone();
            store.insert(key, ImageState::Loading);
            std::thread::spawn(move || {
                let next = match image::open(&path) {
                    Ok(image) => {
                        let rgba = image.to_rgba8();
                        let (width, height) = rgba.dimensions();
                        ImageState::Ready {
                            width,
                            height,
                            pixels: Arc::<[u8]>::from(rgba.into_raw()),
                            generation: 1,
                            uploaded_generation: None,
                        }
                    }
                    Err(error) => ImageState::Error {
                        message: Arc::<str>::from(error.to_string()),
                    },
                };
                if let Ok(mut store) = image_store().lock() {
                    store.insert(key, next);
                }
                mark_redraw_dirty();
            });
        }
    }

    key
}

pub fn snapshot_image(key: u64) -> Option<ImageSnapshot> {
    let store = image_store().lock().unwrap();
    match store.get(&key)? {
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
    let store = image_store().lock().unwrap();
    match store.get(&key) {
        Some(ImageState::Ready {
            uploaded_generation,
            ..
        }) => uploaded_generation != &Some(generation),
        _ => false,
    }
}

pub fn mark_uploaded(key: u64, generation: u64) {
    let mut store = image_store().lock().unwrap();
    if let Some(ImageState::Ready {
        uploaded_generation,
        ..
    }) = store.get_mut(&key)
    {
        *uploaded_generation = Some(generation);
    }
}

pub fn invalidate_uploaded_images() {
    let mut store = image_store().lock().unwrap();
    for state in store.values_mut() {
        if let ImageState::Ready {
            uploaded_generation,
            ..
        } = state
        {
            *uploaded_generation = None;
        }
    }
    mark_redraw_dirty();
}
