use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui_components::{Theme, set_theme, use_theme};
use crate::scene_windows::about_panel::build as build_about_panel;
use crate::scene_windows::component_test::ComponentTest;
use crate::scene_windows::inline_test::build as build_inline_test_window;
use crate::scene_windows::inspector_panel::{
    InspectorPanelBindings, build as build_inspector_panel,
};
use crate::scene_windows::render_test::RenderTest;
use crate::scene_windows::transition_lab::TransitionLab;
use crate::state::{
    DEBUG_COMPILE_DETAIL, DEBUG_GEOMETRY_OVERLAY, DEBUG_RENDER_TIME, DEBUG_REUSE_PATH,
    ENABLE_LAYER_PROMOTION, THEME_DARK_MODE,
};
use crate::window_manager::WindowManager;
use std::sync::atomic::Ordering;

#[component]
pub fn MainScene() -> RsxNode {
    let switch_on = use_state(|| THEME_DARK_MODE.load(Ordering::Relaxed));
    let debug_geometry_overlay = use_state(|| false);
    let debug_render_time = use_state(|| false);
    let debug_compile_detail = use_state(|| false);
    let debug_reuse_path = use_state(|| false);
    let enable_layer_promotion = use_state(|| ENABLE_LAYER_PROMOTION.load(Ordering::Relaxed));
    let window_z_order = use_state(Vec::<usize>::new);
    let window_positions = use_state(Vec::<(f32, f32)>::new);

    let switch_on_value = switch_on.get();
    let debug_geometry_overlay_value = debug_geometry_overlay.get();
    let debug_render_time_value = debug_render_time.get();
    let debug_compile_detail_value = debug_compile_detail.get();
    let debug_reuse_path_value = debug_reuse_path.get();
    let enable_layer_promotion_value = enable_layer_promotion.get();
    let previous_theme_dark = THEME_DARK_MODE.swap(switch_on_value, Ordering::Relaxed);
    if previous_theme_dark != switch_on_value {
        if switch_on_value {
            set_theme(Theme::dark());
        } else {
            set_theme(Theme::light());
        }
    }
    DEBUG_GEOMETRY_OVERLAY.store(debug_geometry_overlay_value, Ordering::Relaxed);
    DEBUG_RENDER_TIME.store(debug_render_time_value, Ordering::Relaxed);
    DEBUG_COMPILE_DETAIL.store(debug_compile_detail_value, Ordering::Relaxed);
    DEBUG_REUSE_PATH.store(debug_reuse_path_value, Ordering::Relaxed);
    ENABLE_LAYER_PROMOTION.store(enable_layer_promotion_value, Ordering::Relaxed);
    let theme_state = use_theme();
    let theme = theme_state.get();

    let mut window_manager = WindowManager::new(window_positions.binding());
    window_manager.push(
        "Inspector Panel",
        vec![build_inspector_panel(
            &theme,
            InspectorPanelBindings {
                switch_on: switch_on.binding(),
                debug_geometry_overlay: debug_geometry_overlay.binding(),
                debug_render_time: debug_render_time.binding(),
                debug_compile_detail: debug_compile_detail.binding(),
                debug_reuse_path: debug_reuse_path.binding(),
                enable_layer_promotion: enable_layer_promotion.binding(),
            },
        )],
        (
            WindowManager::WINDOW_DEFAULT_WIDTH,
            WindowManager::WINDOW_DEFAULT_HEIGHT,
        ),
    );
    window_manager.push(
        "Component Test",
        vec![rsx! {
            <ComponentTest theme={theme.clone()} />
        }],
        (460.0, 380.0),
    );

    window_manager.push(
        "Render test",
        vec![rsx! {
            <RenderTest theme={theme.clone()} />
        }],
        (640.0, 420.0),
    );
    window_manager.push(
        "Inline test",
        vec![build_inline_test_window(&theme)],
        (620.0, 560.0),
    );
    window_manager.push(
        "Transition Plugin Lab",
        vec![rsx! {
            <TransitionLab theme={theme.clone()} />
        }],
        (760.0, 520.0),
    );

    window_manager.push("About", vec![build_about_panel(&theme)], (360.0, 280.0));
    RsxNode::fragment(window_manager.into_nodes(window_z_order.binding()))
}
