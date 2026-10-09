//! Inspector labels must survive activating scrolling in the complete demo.
extern crate rfgui;
extern crate rfgui_components;

use rfgui::ui::{PointerButton, RsxNode, rsx};
use rfgui::view::base_component::{BoxModelSnapshot, Text};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{NodeArena, NodeKey, Viewport};

#[path = "../bin/01_window/components.rs"]
mod components;
#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;
#[path = "../bin/01_window/scene.rs"]
mod scene;
#[path = "../bin/01_window/scene_windows/mod.rs"]
mod scene_windows;
#[path = "../bin/01_window/utils.rs"]
mod utils;
#[path = "../bin/01_window/window_manager.rs"]
mod window_manager;
use scene::MainScene;

fn scene(case: u32) -> RsxNode {
    rfgui::ui::render_pass(|| rsx! { <MainScene key={case} /> })
}

fn text_bounds(viewport: &Viewport, label: &str) -> Option<BoxModelSnapshot> {
    fn find(arena: &NodeArena, key: NodeKey, label: &str) -> Option<BoxModelSnapshot> {
        let node = arena.get(key)?;
        if node
            .element
            .as_any()
            .downcast_ref::<Text>()
            .is_some_and(|text| text.content() == label)
        {
            return Some(node.element.box_model_snapshot());
        }
        drop(node);
        arena
            .children_of(key)
            .iter()
            .find_map(|&key| find(arena, key, label))
    }
    viewport
        .node_arena()
        .roots()
        .iter()
        .find_map(|&key| find(viewport.node_arena(), key, label))
}

fn pointer_down(viewport: &mut Viewport, x: f32, y: f32) {
    viewport.set_pointer_position_viewport(x, y);
    viewport.dispatch_pointer_move_event();
    viewport.set_pointer_button_pressed(PointerButton::Left, true);
    viewport.dispatch_pointer_down_event(PointerButton::Left);
}

fn pointer_up_and_click(viewport: &mut Viewport) {
    viewport.set_pointer_button_pressed(PointerButton::Left, false);
    viewport.dispatch_pointer_up_event(PointerButton::Left);
    assert!(viewport.dispatch_click_event(PointerButton::Left));
}

fn press_label(viewport: &mut Viewport, label: &str) {
    let b = text_bounds(viewport, label).expect("mounted Inspector label");
    pointer_down(viewport, b.x + b.width / 2.0, b.y + b.height / 2.0);
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn inspector_labels_survive_enabling_scroll_in_complete_demo() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        for dpr in [1_u32, 2] {
            let case = dpr
                + if mode == ViewportPaintRendererMode::Legacy {
                    0
                } else {
                    10
                };
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let mut options = viewport.debug_options().clone();
            // Fallback annotations can obscure labels. The renderer helper
            // still rejects any fallback, independently of overlay visibility.
            options.retained_auto_fallback_reasons = false;
            viewport.set_debug_options(options);
            let now = rfgui::time::Instant::now();
            let mut root = None;
            let size = [1280 * dpr, 800 * dpr];
            for frame in 0_u64..10 {
                match frame {
                    1 => {
                        pointer_down(&mut viewport, 188.0, 60.0);
                        pointer_up_and_click(&mut viewport);
                    }
                    2 => press_label(&mut viewport, "Debug Render Time"),
                    3 | 6 => pointer_up_and_click(&mut viewport),
                    5 => press_label(&mut viewport, "Debug RetainedAuto"),
                    _ => {}
                }
                // Match App::render_frame: preserve the root until state
                // requests a rebuild. Keep every other demo window present.
                if root.is_none() || rfgui::ui::peek_state_dirty().needs_rebuild() {
                    root = Some(scene(case));
                }
                let output = viewport
                    .render_rsx_offscreen_for_test(
                        root.as_ref().unwrap(),
                        gpu.device.clone(),
                        gpu.queue.clone(),
                        size,
                        dpr as f32,
                        now + std::time::Duration::from_secs(frame),
                    )
                    .map_err(|e| format!("{mode:?} DPR {dpr} frame {frame}: {e}"))?;
                if frame < 2 {
                    continue;
                }
                if frame >= 4 {
                    assert!(viewport.debug_options().trace_render_time);
                }
                if frame >= 7 {
                    assert!(viewport.debug_options().retained_auto_overlay);
                    assert!(text_bounds(&viewport, "Authority").unwrap().should_render);
                }
                // The default 360x240 Inspector must remain unresized: enabling
                // both sections activates its content scroll target. Enlarging
                // it would remove the very transition this regression covers.
                let inspector_height = viewport.node_arena().roots().iter().find_map(|&key| {
                    let node = viewport.node_arena().get(key)?;
                    let b = node.element.box_model_snapshot();
                    (b.x == 48.0 && b.y == 48.0 && b.width == 360.0).then_some(b.height)
                });
                assert_eq!(inspector_height, Some(240.0));
                let pixels = gpu.read(&output.texture, size)?;
                // Dark mode can be covered by another window's debug title.
                // These two labels remain unobscured and visible in the scrollport.
                for label in ["Debug Render Time", "Debug Geometry Overlay"] {
                    let b = text_bounds(&viewport, label).expect("persistent label");
                    assert!(b.should_render && b.width > 0.0 && b.height > 0.0);
                    let mut ink = 0;
                    for y in (b.y * dpr as f32).ceil() as u32
                        ..((b.y + b.height) * dpr as f32).floor() as u32
                    {
                        for x in (b.x * dpr as f32).ceil() as u32
                            ..((b.x + b.width) * dpr as f32).floor() as u32
                        {
                            let p = &pixels[((y * size[0] + x) * 4) as usize..][..4];
                            let lo = *p[..3].iter().min().unwrap();
                            let hi = *p[..3].iter().max().unwrap();
                            ink += usize::from(lo > 65 && hi - lo < 50);
                        }
                    }
                    assert!(
                        ink >= 20 * dpr as usize,
                        "{mode:?} DPR {dpr} frame {frame}: {label} disappeared ({ink} pixels), {b:?}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[path = "inspector_text_visibility/animation_switch_tests.rs"]
mod animation_switch_tests;
