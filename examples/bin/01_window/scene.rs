use crate::rfgui::ColorLike;
use crate::rfgui::ui::{RsxNode, component, rsx, use_mount, use_state, use_viewport};
use crate::rfgui_components::use_theme;
use crate::scene_windows::about_panel::build as build_about_panel;
use crate::scene_windows::component_test::ComponentTest;
use crate::scene_windows::inline_test::build as build_inline_test_window;
use crate::scene_windows::inspector_panel::build as build_inspector_panel;
use crate::scene_windows::render_test::RenderTest;
use crate::scene_windows::transition_lab::TransitionLab;
use crate::window_manager::WindowManager;

#[component]
pub fn MainScene() -> RsxNode {
    let window_z_order = use_state(Vec::<usize>::new);
    let window_positions = use_state(Vec::<(f32, f32)>::new);

    let (theme, _) = use_theme();

    let background = theme.color.background.base.clone();
    let viewport = use_viewport();
    use_mount(move || {
        viewport.set_clear_color(background.to_style_color().to_color());
    });

    let mut window_manager = WindowManager::new(window_positions.binding());
    window_manager.push(
        "Inspector Panel",
        vec![build_inspector_panel(&theme)],
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
