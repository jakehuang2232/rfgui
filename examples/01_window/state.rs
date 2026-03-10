use std::sync::atomic::AtomicBool;

pub static DEBUG_GEOMETRY_OVERLAY: AtomicBool = AtomicBool::new(false);
pub static DEBUG_RENDER_TIME: AtomicBool = AtomicBool::new(false);
pub static THEME_DARK_MODE: AtomicBool = AtomicBool::new(true);
pub static REQUEST_DUMP_FRAME_GRAPH_DOT: AtomicBool = AtomicBool::new(false);
