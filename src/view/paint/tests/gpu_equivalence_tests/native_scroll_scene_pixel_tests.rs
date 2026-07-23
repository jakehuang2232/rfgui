use super::*;

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_scroll_scene_single_backing_pixels_match_and_reuse -- --ignored --nocapture
fn native_scroll_scene_single_backing_pixels_match_and_reuse() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    for case in [
        ScrollSceneGpuCase {
            name: "single-offset-zero",
            offset_y: 0.0,
            content_height: 300.0,
            backing: ScrollSceneBackingKind::Single,
            max_dimension_2d: 8192,
            transition_local_y: 20.0,
        },
        ScrollSceneGpuCase {
            name: "single-offset-fractional",
            offset_y: 47.25,
            content_height: 300.0,
            backing: ScrollSceneBackingKind::Single,
            max_dimension_2d: 8192,
            transition_local_y: 67.25,
        },
    ] {
        for scrollbar in GpuScrollbarCase::ALL {
            run_native_scroll_scene_case(gpu, case, scrollbar)?;
        }
    }
    eprintln!(
        "native single-backing scroll-scene matrix passed on {}",
        gpu.label()
    );
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_scroll_scene_tiled_cross_tile_pixels_match_and_reuse -- --ignored --nocapture
fn native_scroll_scene_tiled_cross_tile_pixels_match_and_reuse() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    for case in [
        ScrollSceneGpuCase {
            name: "tiled-cross-seam-integer",
            offset_y: 1000.0,
            content_height: 3000.0,
            backing: ScrollSceneBackingKind::Tiled,
            max_dimension_2d: 2048,
            transition_local_y: 1024.0,
        },
        ScrollSceneGpuCase {
            name: "tiled-cross-seam-fractional",
            offset_y: 1000.25,
            content_height: 3000.0,
            backing: ScrollSceneBackingKind::Tiled,
            max_dimension_2d: 2048,
            transition_local_y: 1024.0,
        },
    ] {
        for scrollbar in GpuScrollbarCase::ALL {
            run_native_scroll_scene_case(gpu, case, scrollbar)?;
        }
    }
    eprintln!("native tiled scroll-scene matrix passed on {}", gpu.label());
    Ok(())
}
