use crate::rfgui::ui::{RsxNode, component, globalState, on_click, use_state};
use crate::rfgui::{Align, CrossSize, JustifyContent};
use crate::rfgui_components::{Theme, set_theme, use_theme};
use crate::scene_windows::component_test::{
    ComponentTestBindings, ComponentTestValues, build as build_component_test_window,
};
use crate::scene_windows::about_panel::build as build_about_panel;
use crate::scene_windows::inspector_panel::{
    InspectorPanelBindings, build as build_inspector_panel,
};
use crate::scene_windows::render_test::{
    RenderTestBindings, RenderTestValues, build as build_render_test_window,
};
use crate::scene_windows::transition_lab::{
    TransitionLabBindings, TransitionLabValues, build as build_transition_lab_window,
};
use crate::state::{
    DEBUG_GEOMETRY_OVERLAY, DEBUG_RENDER_TIME, DEBUG_REUSE_PATH, ENABLE_LAYER_PROMOTION,
    THEME_DARK_MODE,
};
use crate::window_manager::WindowManager;
use std::sync::atomic::Ordering;

#[component]
pub fn MainScene() -> RsxNode {
    let click_count = globalState(|| 0_i32);
    let message =
        use_state(|| String::from("Line 1: multiline=true\nLine 2: keep editing\n中文字測試"));
    let switch_on = use_state(|| THEME_DARK_MODE.load(Ordering::Relaxed));
    let component_test_count = use_state(|| 0_i32);
    let component_test_checked = use_state(|| false);
    let component_test_number = use_state(|| 5.0_f64);
    let component_test_selected = use_state(|| String::from("Option A"));
    let component_test_slider = use_state(|| 25.0_f64);
    let component_test_switch = use_state(|| true);
    let debug_geometry_overlay = use_state(|| false);
    let debug_render_time = use_state(|| false);
    let debug_reuse_path = use_state(|| false);
    let enable_layer_promotion = use_state(|| ENABLE_LAYER_PROMOTION.load(Ordering::Relaxed));
    let justify_content = use_state(|| JustifyContent::Start);
    let align = use_state(|| Align::Start);
    let cross_size = use_state(|| CrossSize::Fit);
    let style_transition_enabled = use_state(|| true);
    let style_target_alt = use_state(|| false);
    let layout_transition_enabled = use_state(|| true);
    let layout_expanded = use_state(|| false);
    let visual_transition_enabled = use_state(|| true);
    let visual_at_end = use_state(|| false);
    let window_z_order = use_state(Vec::<usize>::new);
    let window_positions = use_state(Vec::<(f32, f32)>::new);

    let click_count_value = click_count.get();
    let message_value = message.get();
    let switch_on_value = switch_on.get();
    let component_test_count_value = component_test_count.get();
    let component_test_checked_value = component_test_checked.get();
    let component_test_number_value = component_test_number.get();
    let component_test_selected_value = component_test_selected.get();
    let component_test_slider_value = component_test_slider.get();
    let component_test_switch_value = component_test_switch.get();
    let debug_geometry_overlay_value = debug_geometry_overlay.get();
    let debug_render_time_value = debug_render_time.get();
    let debug_reuse_path_value = debug_reuse_path.get();
    let enable_layer_promotion_value = enable_layer_promotion.get();
    let style_transition_enabled_value = style_transition_enabled.get();
    let style_target_alt_value = style_target_alt.get();
    let layout_transition_enabled_value = layout_transition_enabled.get();
    let layout_expanded_value = layout_expanded.get();
    let visual_transition_enabled_value = visual_transition_enabled.get();
    let visual_at_end_value = visual_at_end.get();
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
    DEBUG_REUSE_PATH.store(debug_reuse_path_value, Ordering::Relaxed);
    ENABLE_LAYER_PROMOTION.store(enable_layer_promotion_value, Ordering::Relaxed);
    let increment_state = click_count.clone();
    let increment = on_click(move |event| {
        increment_state.update(|value| *value += 1);
        event.meta.stop_propagation();
    });
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
        vec![build_component_test_window(
            &theme,
            ComponentTestBindings {
                count: component_test_count.binding(),
                checked: component_test_checked.binding(),
                number: component_test_number.binding(),
                selected: component_test_selected.binding(),
                slider: component_test_slider.binding(),
                switch_state: component_test_switch.binding(),
            },
            ComponentTestValues {
                count: component_test_count_value,
                checked: component_test_checked_value,
                number: component_test_number_value,
                selected: component_test_selected_value,
                slider: component_test_slider_value,
                switch_state: component_test_switch_value,
            },
        )],
        (460.0, 380.0),
    );

    window_manager.push(
        "Render test",
        vec![build_render_test_window(
            &theme,
            RenderTestBindings {
                justify_content: justify_content.binding(),
                align: align.binding(),
                cross_size: cross_size.binding(),
                message: message.binding(),
            },
            RenderTestValues {
                click_count: click_count_value,
                message: message_value,
            },
            increment.clone(),
        )],
        (640.0, 420.0),
    );
    window_manager.push(
        "Transition Plugin Lab",
        vec![build_transition_lab_window(
            &theme,
            TransitionLabBindings {
                style_enabled: style_transition_enabled.binding(),
                style_target_alt: style_target_alt.binding(),
                layout_enabled: layout_transition_enabled.binding(),
                layout_expanded: layout_expanded.binding(),
                visual_enabled: visual_transition_enabled.binding(),
                visual_at_end: visual_at_end.binding(),
            },
            TransitionLabValues {
                style_enabled: style_transition_enabled_value,
                style_target_alt: style_target_alt_value,
                layout_enabled: layout_transition_enabled_value,
                layout_expanded: layout_expanded_value,
                visual_enabled: visual_transition_enabled_value,
                visual_at_end: visual_at_end_value,
            },
        )],
        (760.0, 520.0),
    );

    window_manager.push(
        "About",
        vec![build_about_panel(&theme)],
        (360.0, 280.0),
    );
    RsxNode::fragment(window_manager.into_nodes(window_z_order.binding()))
}
