//! Build-time guard: forbid `std::time::Instant` / `std::time::SystemTime`
//! in non-test code when targeting wasm. `std`'s time impl panics at runtime
//! on `wasm32-unknown-unknown`. Use `crate::time::Instant` (cfg-gated to
//! `web_time::Instant` on wasm) instead.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");

    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.starts_with("wasm32") {
        return;
    }

    let src = Path::new("src");
    if !src.exists() {
        return;
    }
    let mut hits: Vec<String> = Vec::new();
    scan_dir(src, &mut hits);
    if !hits.is_empty() {
        for h in &hits {
            println!("cargo:warning={}", h);
        }
        panic!(
            "rfgui: `std::time::Instant`/`std::time::SystemTime` is not supported on wasm32; \
             use `crate::time::Instant` instead. Offending sites listed above."
        );
    }
}

fn scan_dir(dir: &Path, hits: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, hits);
            continue;
        }
        if path.extension() != Some(OsStr::new("rs")) {
            continue;
        }
        let Ok(src) = fs::read_to_string(&path) else {
            continue;
        };
        let mut in_test_module = false;
        let mut brace_depth_at_test_start: i32 = -1;
        let mut depth: i32 = 0;
        let mut prev_trimmed: &str = "";
        for (lineno, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();

            if !in_test_module
                && (trimmed.contains("#[cfg(test)]") || trimmed.contains("#[cfg(all(test"))
            {
                in_test_module = true;
                brace_depth_at_test_start = depth;
            }

            for ch in line.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if in_test_module && depth == brace_depth_at_test_start {
                            in_test_module = false;
                            brace_depth_at_test_start = -1;
                        }
                    }
                    _ => {}
                }
            }

            let cfg_gated_off_wasm = prev_trimmed
                .contains("#[cfg(not(target_arch = \"wasm32\"))]")
                || prev_trimmed.contains("cfg(not(target_arch=\"wasm32\"))");
            prev_trimmed = trimmed;

            if in_test_module {
                continue;
            }
            if trimmed.starts_with("//") {
                continue;
            }
            if cfg_gated_off_wasm {
                continue;
            }

            if line.contains("std::time::Instant") || line.contains("std::time::SystemTime") {
                hits.push(format!("{}:{}: {}", path.display(), lineno + 1, line.trim()));
            }
        }
    }
}
