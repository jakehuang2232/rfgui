//! Temporary survey probe for retiring the Legacy renderer. Delete it together
//! with the Legacy renderer.
//!
//! When `RFGUI_RETAINED_FALLBACK_LEDGER` names a file, every frame in which a
//! viewport that requested `RetainedAuto` painted through Legacy, or armed the
//! terminal circuit breaker, appends one JSON line to that file. Each line
//! carries the process and thread name (libtest names a test's thread after
//! the test), the authority trace with the exact prepare and compile errors,
//! any frame graph failure, and the per-owner fallback records of the debug
//! capture. The census coverage pass is deliberately not forced: it walks
//! with a default recording context instead of the production Surface DAG
//! policy and reports blockers production would admit. Frames painted in
//! requested Legacy mode are not recorded: they never took the fallback.

use super::*;

#[cfg(not(target_arch = "wasm32"))]
const LEDGER_ENV: &str = "RFGUI_RETAINED_FALLBACK_LEDGER";

#[cfg(not(target_arch = "wasm32"))]
fn sink() -> Option<&'static std::sync::Mutex<std::fs::File>> {
    static SINK: std::sync::OnceLock<Option<std::sync::Mutex<std::fs::File>>> =
        std::sync::OnceLock::new();
    SINK.get_or_init(|| {
        let path = std::env::var_os(LEDGER_ENV)?;
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(file) => Some(std::sync::Mutex::new(file)),
            Err(error) => {
                eprintln!("[warn] {LEDGER_ENV}={path:?} cannot be opened: {error}");
                None
            }
        }
    })
    .as_ref()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn enabled() -> bool {
    sink().is_some()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn enabled() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
pub(super) fn record(
    _frame_number: u64,
    _telemetry: &PaintAuthorityTelemetry,
    _capture: Option<&crate::view::debug::DebugRetainedAutoCaptureInput>,
    _graph_failure: Option<&str>,
) {
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn record(
    frame_number: u64,
    telemetry: &PaintAuthorityTelemetry,
    capture: Option<&crate::view::debug::DebugRetainedAutoCaptureInput>,
    graph_failure: Option<&str>,
) {
    use crate::view::debug::census::{
        fallback_category_label, fallback_detail_label, fallback_stage_label, short_element_type,
    };
    use std::io::Write;

    if telemetry.requested_mode != ViewportPaintRendererMode::RetainedAuto
        || !(telemetry.final_authority_is_legacy() || telemetry.terminal_failure_stage.is_some())
    {
        return;
    }
    let Some(sink) = sink() else {
        return;
    };
    let mut line = String::with_capacity(512);
    line.push('{');
    push_string_field(&mut line, "process", process_name());
    line.push(',');
    push_string_field(
        &mut line,
        "thread",
        std::thread::current().name().unwrap_or("<unnamed>"),
    );
    line.push_str(&format!(",\"frame\":{frame_number},"));
    push_string_field(
        &mut line,
        "legacy_fallback_stage",
        telemetry
            .legacy_fallback_stage
            .map_or("none", PaintAuthorityFallbackStage::label),
    );
    line.push(',');
    push_string_field(
        &mut line,
        "terminal_failure_stage",
        telemetry
            .terminal_failure_stage
            .map_or("none", PaintAuthorityFallbackStage::label),
    );
    line.push(',');
    push_string_field(&mut line, "authority", &telemetry.format_debug());
    if let Some(failure) = graph_failure {
        line.push(',');
        push_string_field(&mut line, "graph_failure", failure);
    }
    line.push_str(",\"fallbacks\":[");
    let fallbacks = capture.map_or(&[][..], |capture| &capture.frame.fallback_stages[..]);
    for (index, fallback) in fallbacks.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        line.push('{');
        push_string_field(&mut line, "stage", fallback_stage_label(fallback.stage));
        line.push(',');
        push_string_field(
            &mut line,
            "category",
            fallback_category_label(fallback.category),
        );
        line.push(',');
        push_string_field(
            &mut line,
            "detail",
            &fallback_detail_label(&fallback.detail),
        );
        line.push(',');
        push_string_field(
            &mut line,
            "element",
            fallback.element_type.map_or("-", short_element_type),
        );
        if let Some(stable_id) = fallback.stable_id {
            line.push_str(&format!(",\"stable_id\":{stable_id}"));
        }
        line.push('}');
    }
    line.push_str("]}\n");
    if let Ok(mut file) = sink.lock() {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn process_name() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        std::env::args()
            .next()
            .as_deref()
            .map(std::path::Path::new)
            .and_then(std::path::Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<unknown>".to_owned())
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn push_string_field(line: &mut String, key: &str, value: &str) {
    line.push('"');
    line.push_str(key);
    line.push_str("\":\"");
    for character in value.chars() {
        match character {
            '"' => line.push_str("\\\""),
            '\\' => line.push_str("\\\\"),
            '\n' => line.push_str("\\n"),
            '\r' => line.push_str("\\r"),
            '\t' => line.push_str("\\t"),
            character if character.is_control() => {
                line.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => line.push(character),
        }
    }
    line.push('"');
}
