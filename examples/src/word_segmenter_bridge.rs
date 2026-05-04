//! Bridges `rfgui-segmenter` (OS-native dictionary word segmentation)
//! into `rfgui::platform::WordSegmenter`. Engine core stays platform-
//! clean — host bins call [`install_system_word_segmenter`] once at
//! startup so caret word-jump (Option/Alt+Arrow) gets CJK / Thai /
//! Khmer / Burmese support on macOS / Windows / Web.

use rfgui::platform::{WordSegmenter as RfguiSegmenter, install_word_segmenter};
use rfgui_segmenter::{WordSegmenter as SystemSegmenter, system_segmenter};

struct SystemBridge(Box<dyn SystemSegmenter>);

impl RfguiSegmenter for SystemBridge {
    fn boundaries(&self, text: &str) -> Vec<usize> {
        self.0.boundaries(text)
    }
}

/// Install `rfgui-segmenter::system_segmenter()` as the process-wide
/// rfgui word segmenter. Idempotent — returns silently if a segmenter
/// is already installed.
pub fn install_system_word_segmenter() {
    let _ = install_word_segmenter(Box::new(SystemBridge(system_segmenter())));
}
