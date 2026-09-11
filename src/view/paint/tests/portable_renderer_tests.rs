//! The same production Viewport, scenes and absolute expectations run on native
//! hardware and browser WebGPU. Only device acquisition and readback scheduling
//! differ. This is test-only; no alternative renderer or authority is introduced.
use super::retained_acceptance_fixtures::*;
use super::*;
use crate::view::paint::RetainedSurfaceCompileAction;
use crate::view::viewport::{Viewport, ViewportPaintRendererMode};
use std::sync::Mutex;
use std::task::{Poll, Waker};

mod device_switch_tests;
mod lifecycle_tests;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

async fn read_pixels(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    [width, height]: [u32; 2],
) -> Result<Vec<u8>, String> {
    let stride = (width * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("portable acceptance readback"),
        size: u64::from(stride) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    // Browser callbacks must get an event-loop turn. Never block_on or poll(wait)
    // there. On native, poll drives the callback before awaiting the same future.
    type Completion = (Option<Result<(), wgpu::BufferAsyncError>>, Option<Waker>);
    let completion: Arc<Mutex<Completion>> = Arc::new(Mutex::new((None, None)));
    let callback = completion.clone();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let mut state = callback.lock().unwrap();
            state.0 = Some(result);
            if let Some(waker) = state.1.take() {
                waker.wake();
            }
        });
    #[cfg(not(target_arch = "wasm32"))]
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("poll: {e:?}"))?;
    std::future::poll_fn(|cx| {
        let mut state = completion.lock().unwrap();
        if let Some(result) = state.0.take() {
            Poll::Ready(result)
        } else {
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    })
    .await
    .map_err(|e| format!("readback: {e:?}"))?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| format!("mapped range: {e:?}"))?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in mapped.chunks_exact(stride as usize) {
        pixels.extend_from_slice(&row[..width as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}

fn probe(
    pixels: &[u8],
    extent: [u32; 2],
    dpr: u32,
    name: &str,
    [x, y]: [u32; 2],
    expected: [u8; 4],
) -> Result<(), String> {
    if x >= extent[0] || y >= extent[1] {
        return Err(format!("out-of-bounds probe {name}"));
    }
    let at = ((y * dpr * extent[0] * dpr + x * dpr) * 4) as usize;
    let actual = &pixels[at..at + 4];
    if actual.iter().zip(expected).any(|(a, b)| a.abs_diff(b) > 1) {
        return Err(format!("{name}: actual={actual:?}, expected={expected:?}"));
    }
    Ok(())
}

fn begin(viewport: &mut Viewport, gpu: &Gpu, extent: [u32; 2], dpr: u32) -> Result<(), String> {
    if viewport.device().is_none() {
        // Mirror create_surface before this harness starts layout/record/plan.
        // The low-level offscreen helper also serves already prepared graph
        // tests, so initialization must live here, before any transaction.
        // Never release on warm frames: their resident reuse is under test.
        viewport.release_render_resource_caches();
    }
    viewport.begin_offscreen_test_frame(
        gpu.device.clone(),
        gpu.queue.clone(),
        extent[0] * dpr,
        extent[1] * dpr,
        FORMAT,
    )?;
    viewport.set_scale_factor(dpr as f32);
    assert_eq!(viewport.scale_factor(), dpr as f32);
    assert_eq!(
        viewport.logical_size(),
        (extent[0] as f32, extent[1] as f32)
    );
    Ok(())
}

fn progress(message: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{message}");
    #[cfg(target_arch = "wasm32")]
    let _ = js_sys::Reflect::set(
        &js_sys::global(),
        &"rfguiAcceptanceProgress".into(),
        &message.into(),
    );
}

async fn request_adapter(instance: &wgpu::Instance) -> Result<wgpu::Adapter, String> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: Default::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|e| format!("GPU adapter required: {e:?}"))?;
    let info = adapter.get_info();
    #[cfg(not(target_arch = "wasm32"))]
    if !matches!(
        info.device_type,
        wgpu::DeviceType::IntegratedGpu
            | wgpu::DeviceType::DiscreteGpu
            | wgpu::DeviceType::VirtualGpu
    ) {
        return Err(format!("hardware adapter required: {info:?}"));
    }
    #[cfg(target_arch = "wasm32")]
    if info.backend != wgpu::Backend::BrowserWebGpu {
        return Err(format!("WebGPU required: {info:?}"));
    }
    Ok(adapter)
}

async fn request_gpu(adapter: &wgpu::Adapter) -> Result<Gpu, String> {
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("portable renderer acceptance"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: Default::default(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|e| format!("device: {e:?}"))?;
    Ok(Gpu { device, queue })
}

async fn run() -> Result<String, String> {
    // Browser hosts must register fonts before layout, as the production web
    // runner does. Use the same bundled font on native so wrapping and scroll
    // extents cannot depend on the machine's installed fonts.
    crate::view::register_font_bytes(include_bytes!(
        "../../../../examples/assets/NotoSans-Regular.ttf"
    ));
    crate::view::set_default_font_families("Noto Sans", "Noto Sans", "Noto Sans");
    #[cfg(target_arch = "wasm32")]
    let backends = wgpu::Backends::BROWSER_WEBGPU;
    #[cfg(not(target_arch = "wasm32"))]
    let backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: wgpu::InstanceFlags::empty(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: None,
    });
    let adapter = request_adapter(&instance).await?;
    let info = adapter.get_info();
    let gpu = request_gpu(&adapter).await?;
    let mut frames = 0;
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for scene in Scene::ALL {
            for dpr in [1, 2] {
                let fixture = unlaid_out_fixture(scene);
                let mut viewport = Viewport::new();
                viewport.set_paint_renderer_mode(mode);
                viewport.install_single_viewport_forest_for_test(fixture.arena, fixture.roots);
                let mut first_targets = None;
                for frame in 0..3 {
                    let label = format!("{mode:?} {scene:?} DPR={dpr} frame={frame}");
                    progress(&label);
                    begin(&mut viewport, &gpu, EXTENT, dpr)?;
                    let observed = viewport
                        .render_single_viewport_scene_for_test()
                        .map_err(|e| format!("{label}: {e}"))?;
                    let pixels =
                        read_pixels(&gpu, &observed.texture, EXTENT.map(|v| v * dpr)).await?;
                    for (name, at, expected) in &fixture.probes {
                        probe(&pixels, EXTENT, dpr, name, *at, *expected)
                            .map_err(|e| format!("{label}: {e}"))?;
                    }
                    assert_eq!(observed.frame_number, frame + 1);
                    if mode == ViewportPaintRendererMode::RetainedAuto {
                        assert!(observed.artifact_selected);
                        assert!(!observed.actions.is_empty());
                        assert_eq!(observed.actions.len(), observed.color_targets.len());
                        assert!(observed.actions.iter().all(|a| *a
                            == if frame == 0 {
                                RetainedSurfaceCompileAction::Reraster
                            } else {
                                RetainedSurfaceCompileAction::Reuse
                            }));
                        let mut bytes = 0;
                        for (key, desc) in &observed.color_targets {
                            assert!(
                                viewport.has_compatible_persistent_render_target_pair(*key, desc)
                            );
                            bytes += u64::from(desc.width()) * u64::from(desc.height()) * 12;
                        }
                        assert_eq!(observed.texture_bytes, bytes);
                        if let Some(first) = &first_targets {
                            assert_eq!(&observed.color_targets, first);
                        } else {
                            first_targets = Some(observed.color_targets);
                        }
                    } else {
                        assert!(observed.legacy_selected);
                    }
                    frames += 1;
                }
            }
        }
    }
    assert_eq!(frames, 192);
    let lifecycle_frames = lifecycle_tests::run(&gpu).await?;
    let (device_switch_frames, initialization_adapter) =
        device_switch_tests::run(&instance, &gpu).await?;
    Ok(format!(
        "adapter={info:?}; corpus_frames={frames}; lifecycle_frames={lifecycle_frames}; device_switch_frames={device_switch_frames}; initialization_adapter={initialization_adapter:?}; renderers=2; dprs=1,2"
    ))
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_portable_renderer_acceptance() -> Result<(), String> {
    let report = pollster::block_on(run())?;
    eprintln!("portable renderer acceptance passed: {report}");
    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn run_renderer_acceptance() -> Result<String, wasm_bindgen::JsValue> {
    std::panic::set_hook(Box::new(|info| {
        let _ = js_sys::Reflect::set(
            &js_sys::global(),
            &"rfguiAcceptancePanic".into(),
            &info.to_string().into(),
        );
    }));
    run()
        .await
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error))
}
