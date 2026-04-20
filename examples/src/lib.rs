//! Shared helpers for the example binaries.
//!
//! The viewport decoupling plan keeps all winit-aware code out of the
//! rfgui engine; this crate is where that code lives. Each example bin
//! pulls whatever it needs from here.

#[cfg(not(target_arch = "wasm32"))]
pub mod winit_runner;

#[cfg(target_arch = "wasm32")]
pub mod web_runner;

pub mod winit_key_map;
