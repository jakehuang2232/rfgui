use rfgui::{ColorLike, JustifyContent, Transition, TransitionProperty};
use rfgui_components::{
    init_theme, on_move, set_theme, use_theme, Button, ButtonVariant, Checkbox, NumberField,
    Select, Slider, Switch, Theme, Window, WindowProps,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rfgui::ui::host::{Element, Text, TextArea};
use rfgui::ui::{
    component, globalState, on_click, on_focus, rsx, take_state_dirty, use_state, Binding,
    FocusHandlerProp, RsxNode,
};
use rfgui::{
    Border, BorderRadius, ClipMode, Collision, CollisionBoundary, Color, Cursor, Display,
    FontFamily, Length, Padding, Position, ScrollDirection, Viewport,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::window::{CursorIcon, Window as WinitWindow, WindowId};

static DEBUG_GEOMETRY_OVERLAY: AtomicBool = AtomicBool::new(false);
static DEBUG_RENDER_TIME: AtomicBool = AtomicBool::new(false);
static THEME_DARK_MODE: AtomicBool = AtomicBool::new(true);

struct ManagedWindow {
    id: usize,
    props: WindowProps,
}

struct WindowManager {
    windows: Vec<ManagedWindow>,
    positions: Binding<Vec<(f32, f32)>>,
}

impl WindowManager {
    const WINDOW_DEFAULT_WIDTH: f64 = 360.0;
    const WINDOW_DEFAULT_HEIGHT: f64 = 240.0;
    const WINDOW_INIT_OFFSET: f32 = 48.0;

    fn new(positions: Binding<Vec<(f32, f32)>>) -> Self {
        Self {
            windows: Vec::new(),
            positions,
        }
    }

    fn push(&mut self, title: impl Into<String>, children: Vec<RsxNode>, size: (f64, f64)) {
        let id = self.windows.len();
        let positions_state = self.positions.clone();
        positions_state.update(|positions| {
            while positions.len() <= id {
                let index = positions.len() as f32;
                let offset = (index + 1.0) * Self::WINDOW_INIT_OFFSET;
                positions.push((offset, offset));
            }
        });
        let position = positions_state.get().get(id).copied().unwrap_or((0.0, 0.0));
        let on_move_handler = {
            let positions_state = self.positions.clone();
            on_move(move |x, y| {
                positions_state.update(|positions| {
                    if let Some(slot) = positions.get_mut(id) {
                        *slot = (x, y);
                    }
                });
            })
        };
        self.windows.push(ManagedWindow {
            id,
            props: WindowProps {
                title: title.into(),
                draggable: Some(true),
                width: Some(size.0),
                height: Some(size.1),
                position: Some(position),
                on_move: Some(on_move_handler),
                on_resize: None,
                on_focus: None,
                on_blur: None,
                window_slots: None,
                children,
            },
        });
    }

    fn into_nodes(self, z_order: Binding<Vec<usize>>) -> Vec<RsxNode> {
        let window_count = self.windows.len();
        z_order.update(|order| normalize_window_order(order, window_count));
        let order = z_order.get();

        let mut ordered_windows = Vec::with_capacity(window_count);
        for index in order {
            if let Some(window_entry) = self.windows.get(index) {
                let props = &window_entry.props;
                let z_order_for_focus = z_order.clone();
                let original_focus = props.on_focus.clone();
                let focus = on_focus(move |event| {
                    if let Some(handler) = &original_focus {
                        // Keep consumer-defined focus behavior.
                        handler.call(event);
                    }
                    z_order_for_focus.update(|current| bring_window_to_front(current, index));
                });
                let window = rsx! {
                    <Window
                        key={window_entry.id}
                        title={props.title.clone()}
                        draggable={props.draggable}
                        width={props.width}
                        height={props.height}
                        position={props.position}
                        on_move={props.on_move.clone()}
                        on_resize={props.on_resize.clone()}
                        on_blur={props.on_blur.clone()}
                    >
                        {props.children.clone()}
                    </Window>
                };
                let window = with_stable_key(window, window_entry.id);
                ordered_windows.push(with_focus_handler(window, focus));
            }
        }
        ordered_windows
    }
}

fn normalize_window_order(order: &mut Vec<usize>, window_count: usize) {
    order.retain(|index| *index < window_count);
    for index in 0..window_count {
        if !order.contains(&index) {
            order.push(index);
        }
    }
}

fn bring_window_to_front(order: &mut Vec<usize>, index: usize) {
    if let Some(position) = order.iter().position(|value| *value == index) {
        if position + 1 == order.len() {
            return;
        }
        let current = order.remove(position);
        order.push(current);
        return;
    }
    order.push(index);
}

fn with_focus_handler(mut node: RsxNode, handler: FocusHandlerProp) -> RsxNode {
    if let RsxNode::Element(element) = &mut node {
        element.props.retain(|(key, _)| key != "on_focus");
        element.props.push(("on_focus".to_string(), handler.into()));
    }
    node
}

fn with_stable_key(mut node: RsxNode, id: usize) -> RsxNode {
    if let RsxNode::Element(element) = &mut node {
        element.props.retain(|(key, _)| key != "key");
        element
            .props
            .push(("key".to_string(), format!("wm-window-{id}").into()));
    }
    node
}

#[component]
fn MainScene() -> RsxNode {
    fn select_label(item: &String, _: usize) -> String {
        item.clone()
    }

    let click_count = globalState(|| 0_i32);
    let message =
        use_state(|| String::from("Line 1: multiline=true\nLine 2: keep editing\n中文字測試"));
    let checked = use_state(|| true);
    let number_value = use_state(|| 3.0_f64);
    let selected_value = use_state(|| String::from("Option A"));
    let slider_value = use_state(|| 42.0_f64);
    let switch_on = use_state(|| THEME_DARK_MODE.load(Ordering::Relaxed));
    let debug_geometry_overlay = use_state(|| false);
    let debug_render_time = use_state(|| false);
    let style_transition_enabled = use_state(|| true);
    let style_target_alt = use_state(|| false);
    let layout_transition_enabled = use_state(|| true);
    let layout_expanded = use_state(|| false);
    let visual_transition_enabled = use_state(|| true);
    let visual_at_end = use_state(|| false);
    let panel_size = globalState(|| String::from("360 x 240"));
    let window_z_order = use_state(Vec::<usize>::new);
    let window_positions = use_state(Vec::<(f32, f32)>::new);

    let click_count_value = click_count.get();
    let message_value = message.get();
    let checked_value = checked.get();
    let number_value_value = number_value.get();
    let selected_value_value = selected_value.get();
    let slider_value_value = slider_value.get();
    let switch_on_value = switch_on.get();
    let debug_geometry_overlay_value = debug_geometry_overlay.get();
    let debug_render_time_value = debug_render_time.get();
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
    let increment_state = click_count.clone();
    let increment = on_click(move |event| {
        increment_state.update(|v| *v += 1);
        event.meta.stop_propagation();
    });
    let themeState = use_theme();
    let theme = themeState.get();

    let mut window_manager = WindowManager::new(window_positions.binding());
    window_manager.push(
        "Inspector Panel",
        vec![rsx! {
            <Element style={{
                gap: theme.spacing.xs,
                display: Display::flow().column().no_wrap(),
                width: Length::percent(100.0),
            }}>
                <Switch
                    label="Dark mode"
                    binding={switch_on.binding()}
                />
                <Switch
                    label="Debug Geometry Overlay"
                    binding={debug_geometry_overlay.binding()}
                />
                <Switch
                    label="Debug Render Time"
                    binding={debug_render_time.binding()}
                />
            </Element>
        }],
        (
            WindowManager::WINDOW_DEFAULT_WIDTH,
            WindowManager::WINDOW_DEFAULT_HEIGHT,
        ),
    );

    let justify_content = use_state(|| JustifyContent::Start);
    let justify_content_start = justify_content.clone();
    let justify_content_center = justify_content.clone();
    let justify_content_end = justify_content.clone();
    let justify_content_space_between = justify_content.clone();
    let justify_content_space_around = justify_content.clone();
    let justify_content_space_evenly = justify_content.clone();

    window_manager.push(
        "Render test",
        vec![rsx! {
            <Element style={{
                width: Length::percent(100.0),
                height: Length::percent(100.0),
                background: Color::transparent(),
                display: Display::flow().row().wrap().justify_content(justify_content.get()),
                gap: theme.spacing.md,
                padding: Padding::uniform(Length::px(20.0)),
                scroll_direction: ScrollDirection::Vertical,
                font: FontFamily::new(["Noto Sans CJK TC", "PingFang TC"]),
            }} anchor="root">
                <Element style={{
                    width: Length::percent(100.0),
                    display: Display::flow().row().wrap(),
                    gap: theme.spacing.md,
                }}>
                    <Button label="Start" on_click={move |_| {justify_content_start.set(JustifyContent::Start);}} />
                    <Button label="Center" on_click={move |_| {justify_content_center.set(JustifyContent::Center);}} />
                    <Button label="End" on_click={move |_| {justify_content_end.set(JustifyContent::End);}} />
                    <Button label="SpaceBetween" on_click={move |_| {justify_content_space_between.set(JustifyContent::SpaceBetween);}} />
                    <Button label="SpaceAround" on_click={move |_| {justify_content_space_around.set(JustifyContent::SpaceAround);}} />
                    <Button label="SpaceEvenly" on_click={move |_| {justify_content_space_evenly.set(JustifyContent::SpaceEvenly);}} />
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#61afef",
                    padding: Padding::uniform(Length::px(10.0)),
                }}>
                    Pure Object
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#61afef",
                    border: Border::uniform(Length::px(10.0), &Color::hex("#21252b")),
                }}>
                    Border
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#61afef",
                    border_radius: BorderRadius::uniform(Length::px(10.0))
                        .top_right(Length::px(32.0))
                        .bottom_left(Length::percent(90.0)),
                }}>
                    Border Radius
                </Element>
                <Element style={{
                    width: Length::percent(50.0),
                    height: Length::px(150.0),
                    background: "#61afef",
                    border_radius: BorderRadius::uniform(Length::px(10.0))
                        .top_right(Length::px(32.0))
                        .bottom_left(Length::percent(90.0)),
                    transition: [
                        Transition::new(TransitionProperty::All, 1000),
                    ]
                }}>
                    Percentage Width
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#61afef",
                    border: Border::uniform(Length::px(10.0), &Color::hex("#21252b"))
                        .top(Some(Length::px(20.0)), Some(&Color::hex("#e06c75")))
                        .left(Some(Length::px(15.0)), Some(&Color::hex("#2db353"))),
                    border_radius: BorderRadius::uniform(Length::px(10.0))
                        .top_right(Length::px(10.0))
                        .bottom_left(Length::percent(90.0)),
                }}>
                    Border Radius + Border
                </Element>
                <Element style={{
                    width: Length::px(170.0),
                    height: Length::px(170.0),
                    background: "#e06c75",
                    border: Border::uniform(Length::px(20.0), &Color::hex("#58622b")),
                    border_radius: 16,
                    hover: {
                        border: Border::uniform(Length::px(20.0), &Color::hex("#61afef")),
                    },
                    transition: [
                        Transition::new(TransitionProperty::Position, 1000).ease_in_out(),
                        Transition::new(TransitionProperty::BorderColor, 1000).ease_in_out()
                    ],
                }}>
                    <Element style={{
                        width: Length::percent(100.0),
                        height: Length::percent(100.0),
                        background: "#61afef",
                        border: Border::uniform(Length::px(20.0), &Color::hex("#3e4451")),
                        border_radius: 0,
                        hover: {
                            background: "#7bc1ff",
                            border: Border::uniform(Length::px(20.0), &Color::hex("#1f2937"))
                        },
                        transition: [
                            Transition::new(TransitionProperty::All, 200),
                        ],
                    }}>
                        Nested + Hover Test
                    </Element>
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#61afef",
                    border: Border::uniform(Length::px(8.0), &Color::hex("#21252b")),
                    border_radius: 50,
                    display: Display::flow().row().wrap(),
                    gap: Length::px(8.0),
                    padding: Padding::uniform(Length::px(8.0)),
                }}>
                    <Element style={{ width: Length::px(72.0), height: Length::px(48.0), background: "#d19a66", border: Border::uniform(Length::px(3.0), &Color::hex("#e06c75")) }}>
                        Clip Test
                    </Element>
                    <Element style={{ width: Length::px(56.0), height: Length::px(56.0), background: "#61afef" }} />
                    <Element style={{ width: Length::px(120.0), height: Length::px(64.0), background: "#c678dd", border: Border::uniform(Length::px(4.0), &Color::hex("#56b6c2")) }} />
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#282c34",
                    border: Border::uniform(Length::px(3.0), &Color::hex("#61afef")),
                    border_radius: 16,
                    display: Display::flow().row().wrap(),
                    gap: Length::px(8.0),
                    padding: Padding::uniform(Length::px(8.0)),
                }} >
                    <Text font_size=22 style={{ color: "#abb2bf" }} >
                        Button Test
                    </Text>
                    <Text font_size=14 style={{ color: "#abb2bf" }} >{format!("Click Count: {}", click_count_value)}</Text>
                    <Button
                        label="Click\nMe"
                        variant={Some(ButtonVariant::Contained)}
                        on_click={increment}
                    />
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#61afef",
                    border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")),
                    border_radius: 16,
                    opacity: 0.5,
                    display: Display::flow().column(),
                }}>
                    <Text font_size=16 style={{ color: "#21252b" }} >
                        Opacity Nesting
                    </Text>
                    <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 0.6 }}>
                        <Element style={{
                            width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                            <Text font_size=12 style={{ color: "#ffffff" }}  opacity=0.9>
                                Alpha
                            </Text>
                        </Element>
                    </Element>
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#61afef7f",
                    border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")),
                    border_radius: 16,
                    display: Display::flow().column(),
                }}>
                    <Text font_size=16 style={{ color: "#21252b" }} >
                        Background Opacity Nesting
                    </Text>
                    <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 1 }}>
                        <Element style={{
                            width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                            <Text font_size=12 style={{ color: "#ffffff" }}  opacity=0.9>
                                Alpha
                            </Text>
                        </Element>
                    </Element>
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(150.0),
                    background: "#61afef",
                    border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")),
                    border_radius: 16,
                    scroll_direction: ScrollDirection::Vertical,
                    display: Display::flow().column(),
                }}>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 1</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 2</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 3</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 4</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 5</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 6</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 7</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 8</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 9</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 10</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 11</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 12</Text>
                    <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 13</Text>

                </Element>
                <Element style={{
                    width: Length::px(320.0),
                    height: Length::px(170.0),
                    background: "#2c313c",
                    border: Border::uniform(Length::px(3.0), &Color::hex("#61afef")),
                    border_radius: 16,
                    display: Display::flow().column().no_wrap(),
                }}>
                    <Text font_size=14 style={{ color: "#e5e9f0" }} >TextArea Test</Text>
                    <Text font_size=12 style={{ color: "#aab4c6" }} >
                        {format!("Bound chars: {}", message_value.chars().count())}
                    </Text>
                    <TextArea x=12 y=34 width=296 height=78 font_size=13 color="#c8d0e0"  multiline=true placeholder="Please enter multiline content..." binding={message.binding()} />
                    <TextArea x=12 y=98 width=296 height=26 font_size=13 color="#c8d0e0"  multiline=false read_only=true>
                        multiline=false
                        Line breaks should become spaces
                    </TextArea>
                </Element>
                <Element style={{
                    width: Length::px(390.0),
                    height: Length::px(290.0),
                    background: "#0f172a",
                    border: Border::uniform(Length::px(2.0), &Color::hex("#1d4ed8")),
                    border_radius: 14,
                    display: Display::flow().column().no_wrap(),
                    gap: Length::px(8.0),
                    padding: Padding::uniform(Length::px(12.0)),
                }}>
                    <Text font_size=14 style={{ color: "#e2e8f0" }} >MUI Components Demo</Text>
                    <Element style={{
                        display: Display::flow().row().no_wrap(),
                        gap: Length::px(8.0)
                    }}>
                        <Button
                            label="Contained"
                            variant={Some(ButtonVariant::Contained)}
                        />
                        <Button
                            label="Outlined"
                            variant={Some(ButtonVariant::Outlined)}
                        />
                        <Button
                            label="Text"
                            variant={Some(ButtonVariant::Text)}
                        />
                    </Element>
                    <Select
                        data={(1..100).collect::<Vec<i32>>()}
                        to_label={|item, index| format!("{} Hello, Very Long Item! Long Long Long Long\n newLine", item)}
                        to_value={|item, index| format!("{}", item)}
                        value={selected_value.binding()}
                    />
                    <Checkbox
                        label="Enable feature"
                        binding={checked.binding()}
                    />
                    <Element style={{
                        display: Display::flow().row().no_wrap(),
                        gap: Length::px(8.0),
                    }}>
                        <NumberField
                            binding={number_value.binding()}
                            min=0.0
                            max=10.0
                            step=0.5
                        />
                    </Element>
                    <Slider
                        binding={slider_value.binding()}
                        min=0.0
                        max=100.0
                    />
                    <Text font_size=12 style={{ color: "#93c5fd" }} >
                        {format!(
                            "checked={} number={:.1} selected={} slider={:.0} switch={} debug_overlay={}",
                            checked_value,
                            number_value_value,
                            selected_value_value,
                            slider_value_value,
                            switch_on_value,
                            debug_geometry_overlay_value
                        )}
                    </Text>
                </Element>
                <Element style={{
                    width: Length::percent(100.0),
                    height: Length::px(300.0),
                    background: "#111827",
                    border: Border::uniform(Length::px(2.0), &Color::hex("#334155")),
                    border_radius: 14,
                    padding: Padding::uniform(Length::px(12.0)),
                    display: Display::flow().column().no_wrap(),
                    gap: Length::px(8.0),
                }} anchor="menu_button">
                    <Text font_size=14 style={{ color: "#e2e8f0" }}>Absolute + Anchor + Collision</Text>
                    <Text font_size=12 style={{ color: "#94a3b8" }}>
                        parent anchor = "menu_button"
                    </Text>
                    <Element style={{
                        width: Length::px(120.0),
                        height: Length::px(36.0),
                        background: "#1d4ed8",
                        border_radius: 8,
                    }}>
                        <Text font_size=12 style={{ color: "#eff6ff" }}>Menu Button</Text>
                    </Element>
                    <Element style={{
                        width: Length::percent(100.0),
                        height: Length::px(110.0),
                        display: Display::flow().row().no_wrap(),
                        gap: Length::px(8.0),
                    }}>
                        <Element style={{
                            width: Length::px(110.0),
                            height: Length::px(110.0),
                            background: "#1f2937",
                            border: Border::uniform(Length::px(1.0), &Color::hex("#475569")),
                            border_radius: 8,
                            padding: Padding::uniform(Length::px(8.0)),
                        }}>
                            <Text font_size=10 style={{ color: "#cbd5e1" }}>clip=Parent (default)</Text>
                            <Element style={{
                                position: Position::absolute()
                                    .top(Length::px(56.0))
                                    .left(Length::px(84.0)),
                                width: Length::px(74.0),
                                height: Length::px(24.0),
                                background: "#ef4444",
                                border_radius: 6,
                            }}>
                                <Text font_size=10 style={{ color: "#fef2f2" }}>overflow</Text>
                            </Element>
                        </Element>
                        <Element style={{
                            width: Length::px(110.0),
                            height: Length::px(110.0),
                            background: "#1f2937",
                            border: Border::uniform(Length::px(1.0), &Color::hex("#475569")),
                            border_radius: 8,
                            padding: Padding::uniform(Length::px(8.0)),
                        }}>
                            <Text font_size=10 style={{ color: "#cbd5e1" }}>clip=Viewport</Text>
                            <Element style={{
                                position: Position::absolute()
                                    .top(Length::px(56.0))
                                    .left(Length::px(84.0))
                                    .clip(ClipMode::Viewport),
                                width: Length::px(74.0),
                                height: Length::px(24.0),
                                background: "#f59e0b",
                                border_radius: 6,
                            }}>
                                <Text font_size=10 style={{ color: "#fffbeb" }}>overflow</Text>
                            </Element>
                        </Element>
                        <Element style={{
                            width: Length::px(140.0),
                            height: Length::px(110.0),
                            background: "#1f2937",
                            border: Border::uniform(Length::px(1.0), &Color::hex("#475569")),
                            border_radius: 8,
                            padding: Padding::uniform(Length::px(8.0)),
                            display: Display::flow().column().no_wrap(),
                            gap: Length::px(6.0),
                        }}>
                            <Text font_size=10 style={{ color: "#cbd5e1" }}>clip=AnchorParent</Text>
                            <Element style={{
                                width: Length::px(56.0),
                                height: Length::px(26.0),
                                background: "#1d4ed8",
                                border_radius: 6,
                            }} anchor="abs_anchor_test">
                                <Text font_size=9 style={{ color: "#dbeafe" }}>anchor</Text>
                            </Element>
                            <Element style={{
                                position: Position::absolute()
                                    .anchor("abs_anchor_test")
                                    .top(Length::px(0.0))
                                    .left(Length::px(38.0))
                                    .clip(ClipMode::AnchorParent),
                                width: Length::px(82.0),
                                height: Length::px(22.0),
                                background: "#22c55e",
                                border_radius: 6,
                            }}>
                                <Text font_size=9 style={{ color: "#ecfdf5" }}>anchor clip</Text>
                            </Element>
                        </Element>
                    </Element>
                    <Element style={{
                        position: Position::absolute()
                            .anchor("menu_button")
                            .top(Length::px(48.0))
                            .left(Length::px(132.0))
                            .collision(Collision::FlipFit, CollisionBoundary::Viewport),
                        width: Length::px(150.0),
                        height: Length::px(96.0),
                        background: "#0b1220",
                        border: Border::uniform(Length::px(1.0), &Color::hex("#3b82f6")),
                        border_radius: 10,
                        padding: Padding::uniform(Length::px(8.0)),
                        display: Display::flow().column(),
                        gap: Length::px(6.0),
                    }}>
                        <Text font_size=12 style={{ color: "#bfdbfe" }}>Popover (anchor)</Text>
                        <Text font_size=11 style={{ color: "#93c5fd" }}>collision: FlipFit + Viewport</Text>
                        <Text font_size=11 style={{ color: "#93c5fd" }}>try resizing window edge</Text>
                    </Element>
                    <Element style={{
                        position: Position::absolute()
                            .top(Length::px(10.0))
                            .bottom(Length::px(10.0))
                            .right(Length::px(12.0)),
                        width: Length::px(120.0),
                        height: Length::px(30.0),
                        background: "#166534",
                        border_radius: 8,
                    }}>
                        <Text font_size=11 style={{ color: "#dcfce7" }}>fallback parent anchor</Text>
                    </Element>
                </Element>
            </Element>
        }],
        (640.0, 420.0),
    );
    let style_start = style_transition_enabled.clone();
    let style_toggle_target = style_target_alt.clone();
    let style_remove = style_transition_enabled.clone();
    let style_reset_enable = style_transition_enabled.clone();
    let style_reset_target = style_target_alt.clone();
    let layout_start_enable = layout_transition_enabled.clone();
    let layout_toggle_size = layout_expanded.clone();
    let layout_remove = layout_transition_enabled.clone();
    let layout_reset_enable = layout_transition_enabled.clone();
    let layout_reset_size = layout_expanded.clone();
    let visual_start_enable = visual_transition_enabled.clone();
    let visual_toggle_pos = visual_at_end.clone();
    let visual_remove = visual_transition_enabled.clone();
    let visual_reset_enable = visual_transition_enabled.clone();
    let visual_reset_pos = visual_at_end.clone();
    window_manager.push(
        "Transition Plugin Lab",
        vec![rsx! {
            <Element style={{
                width: Length::percent(100.0),
                height: Length::percent(100.0),
                background: "#0f172a",
                display: Display::flow().column().no_wrap(),
                gap: Length::px(10.0),
                padding: Padding::uniform(Length::px(12.0)),
            }}>
                <Text font_size=16 style={{ color: "#e2e8f0" }}>Transition Plugins Test</Text>
                <Text font_size=11 style={{ color: "#93c5fd" }}>
                    {"How to verify: click Start Animation first, then click Remove Transition during playback. Expected: jump to the end value immediately."}
                </Text>
                <Element style={{
                    display: Display::flow().row().wrap(),
                    gap: Length::px(10.0),
                    width: Length::percent(100.0),
                }}>
                    <Element style={{
                        width: Length::px(220.0),
                        background: "#111827",
                        border: Border::uniform(Length::px(1.0), &Color::hex("#334155")),
                        border_radius: 10,
                        padding: Padding::uniform(Length::px(8.0)),
                        display: Display::flow().column().no_wrap(),
                        gap: Length::px(6.0),
                    }}>
                        <Text font_size=12 style={{ color: "#e5e7eb" }}>StyleTransitionPlugin</Text>
                        <Text font_size=10 style={{ color: "#94a3b8" }}>
                            {format!("transition={} target={}", style_transition_enabled_value, style_target_alt_value)}
                        </Text>
                        <Element style={{
                            width: Length::px(180.0),
                            height: Length::px(56.0),
                            background: if style_target_alt_value { Color::hex("#f97316") } else { Color::hex("#22c55e") },
                            border_radius: 8,
                            transition: if style_transition_enabled_value {
                                vec![Transition::new(TransitionProperty::BackgroundColor, 1400).ease_in_out()]
                            } else {
                                Vec::<Transition>::new()
                            },
                        }} />
                        <Element style={{ display: Display::flow().row().wrap(), gap: Length::px(6.0) }}>
                            <Button label="Start Animation" on_click={move |_| { style_start.set(true); style_toggle_target.update(|v| *v = !*v); }} />
                            <Button label="Remove Transition" on_click={move |_| { style_remove.set(false); }} />
                            <Button label="Reset" on_click={move |_| { style_reset_enable.set(true); style_reset_target.set(false); }} />
                        </Element>
                    </Element>
                    <Element style={{
                        width: Length::px(220.0),
                        background: "#111827",
                        border: Border::uniform(Length::px(1.0), &Color::hex("#334155")),
                        border_radius: 10,
                        padding: Padding::uniform(Length::px(8.0)),
                        display: Display::flow().column().no_wrap(),
                        gap: Length::px(6.0),
                    }}>
                        <Text font_size=12 style={{ color: "#e5e7eb" }}>LayoutTransitionPlugin</Text>
                        <Text font_size=10 style={{ color: "#94a3b8" }}>
                            {format!("transition={} expanded={}", layout_transition_enabled_value, layout_expanded_value)}
                        </Text>
                        <Element style={{
                            width: if layout_expanded_value { Length::px(180.0) } else { Length::px(92.0) },
                            height: if layout_expanded_value { Length::px(58.0) } else { Length::px(34.0) },
                            background: "#38bdf8",
                            border_radius: 8,
                            transition: if layout_transition_enabled_value {
                                vec![
                                    Transition::new(TransitionProperty::Width, 1400).ease_in_out(),
                                    Transition::new(TransitionProperty::Height, 1400).ease_in_out(),
                                ]
                            } else {
                                Vec::<Transition>::new()
                            },
                        }} />
                        <Element style={{ display: Display::flow().row().wrap(), gap: Length::px(6.0) }}>
                            <Button label="Start Animation" on_click={move |_| { layout_start_enable.set(true); layout_toggle_size.update(|v| *v = !*v); }} />
                            <Button label="Remove Transition" on_click={move |_| { layout_remove.set(false); }} />
                            <Button label="Reset" on_click={move |_| { layout_reset_enable.set(true); layout_reset_size.set(false); }} />
                        </Element>
                    </Element>
                    <Element style={{
                        width: Length::px(220.0),
                        background: "#111827",
                        border: Border::uniform(Length::px(1.0), &Color::hex("#334155")),
                        border_radius: 10,
                        padding: Padding::uniform(Length::px(8.0)),
                        display: Display::flow().column().no_wrap(),
                        gap: Length::px(6.0),
                    }}>
                        <Text font_size=12 style={{ color: "#e5e7eb" }}>VisualTransitionPlugin</Text>
                        <Text font_size=10 style={{ color: "#94a3b8" }}>
                            {format!("transition={} at_end={}", visual_transition_enabled_value, visual_at_end_value)}
                        </Text>
                        <Element style={{
                            width: Length::px(180.0),
                            height: Length::px(58.0),
                            background: "#1f2937",
                            border_radius: 8,
                            display: Display::flow().row().no_wrap().justify_content(if visual_at_end_value { JustifyContent::End } else { JustifyContent::Start }),
                            padding: Padding::uniform(Length::px(6.0)),
                        }}>
                            <Element style={{
                                width: Length::px(42.0),
                                height: Length::px(42.0),
                                background: "#f43f5e",
                                border_radius: 8,
                                transition: if visual_transition_enabled_value {
                                    vec![Transition::new(TransitionProperty::Position, 1400).ease_in_out()]
                                } else {
                                    Vec::<Transition>::new()
                                },
                            }} />
                        </Element>
                        <Element style={{ display: Display::flow().row().wrap(), gap: Length::px(6.0) }}>
                            <Button label="Start Animation" on_click={move |_| { visual_start_enable.set(true); visual_toggle_pos.update(|v| *v = !*v); }} />
                            <Button label="Remove Transition" on_click={move |_| { visual_remove.set(false); }} />
                            <Button label="Reset" on_click={move |_| { visual_reset_enable.set(true); visual_reset_pos.set(false); }} />
                        </Element>
                    </Element>
                </Element>
                <Element style={{
                    width: Length::percent(100.0),
                    height: Length::px(176.0),
                    background: "#111827",
                    border: Border::uniform(Length::px(1.0), &Color::hex("#334155")),
                    border_radius: 10,
                    padding: Padding::uniform(Length::px(8.0)),
                    display: Display::flow().column().no_wrap(),
                    gap: Length::px(6.0),
                }}>
                    <Text font_size=12 style={{ color: "#e5e7eb" }}>ScrollTransitionPlugin</Text>
                    <Text font_size=10 style={{ color: "#94a3b8" }}>
                        {"Use the mouse wheel to scroll this area and observe inertia/interpolation. This plugin is not controlled by style.transition."}
                    </Text>
                    <Element style={{
                        width: Length::percent(100.0),
                        height: Length::px(120.0),
                        background: "#0b1220",
                        border: Border::uniform(Length::px(1.0), &Color::hex("#1e293b")),
                        border_radius: 8,
                        scroll_direction: ScrollDirection::Vertical,
                        display: Display::flow().column().no_wrap(),
                        gap: Length::px(4.0),
                        padding: Padding::uniform(Length::px(8.0)),
                    }}>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 01</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 02</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 03</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 04</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 05</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 06</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 07</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 08</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 09</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 10</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 11</Text>
                        <Text font_size=11 style={{ color: "#cbd5e1" }}>Scroll row 12</Text>
                    </Element>
                </Element>
            </Element>
        }],
        (760.0, 520.0),
    );
    let managed_windows = window_manager.into_nodes(window_z_order.binding());

    rsx! {
        {managed_windows}
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<WinitWindow>>,
    viewport: Option<Viewport>,
    app: Option<RsxNode>,
    ime_composing: bool,
    ime_dirty: bool,
    last_ime_focus_id: Option<u64>,
    last_ime_allowed: bool,
    last_ime_area: Option<(i32, i32, u32, u32)>,
    cursor_in_window: bool,
    last_mouse_position_viewport: Option<(f32, f32)>,
    background_color: Box<dyn ColorLike>,
    applied_theme_dark: Option<bool>,
}

impl App {
    fn rebuild_app(&mut self) {
        self.app = Some(rsx! { <MainScene /> });
    }

    fn mark_ime_dirty(&mut self) {
        self.ime_dirty = true;
    }

    fn sync_theme_visuals(&mut self) {
        let theme_dark = THEME_DARK_MODE.load(Ordering::Relaxed);
        if self.applied_theme_dark == Some(theme_dark) {
            return;
        }
        self.applied_theme_dark = Some(theme_dark);
        self.background_color = app_background_color(theme_dark);
        if let Some(viewport) = &mut self.viewport {
            viewport.set_clear_color(self.background_color.clone());
            viewport.request_redraw();
        }
        #[cfg(target_os = "macos")]
        if let Some(window) = &self.window {
            with_shadow(window, !self.background_color.is_transparent());
        }
    }

    fn sync_ime_state(&mut self, force: bool) {
        let (Some(window), Some(viewport)) = (&self.window, &self.viewport) else {
            return;
        };

        let focused_id = viewport.focused_node_id();
        if !force && !self.ime_dirty && focused_id == self.last_ime_focus_id {
            return;
        }
        self.last_ime_focus_id = focused_id;

        let mut next_area = None;
        if let Some((x, y, w, h)) = viewport.focused_ime_cursor_rect() {
            next_area = Some(viewport.logical_to_physical_rect(x, y, w, h));
        }
        // Enable IME as long as there is a focused node; cursor area can arrive later.
        let next_allowed = focused_id.is_some();

        if self.last_ime_allowed != next_allowed {
            window.set_ime_allowed(next_allowed);
            self.last_ime_allowed = next_allowed;
        }

        if let Some((x, y, w, h)) = next_area {
            if self.last_ime_area != Some((x, y, w, h)) {
                let pos = PhysicalPosition::new(x, y);
                let size = PhysicalSize::new(w, h);
                window.set_ime_cursor_area(pos, size);
            }
        }
        self.last_ime_area = next_area;
        self.ime_dirty = false;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(WinitWindow::default_attributes().with_transparent(true))
                .unwrap(),
        );
        let mut viewport = Viewport::new();
        viewport.set_scale_factor(window.scale_factor() as f32);
        let size = window.inner_size();
        viewport.set_size(size.width, size.height);
        viewport.set_clear_color(self.background_color.clone());
        let cursor_window = window.clone();
        viewport.set_cursor_handler(move |cursor| {
            cursor_window.set_cursor(map_cursor_icon(cursor));
        });
        pollster::block_on(viewport.set_window(window.clone()));
        pollster::block_on(viewport.create_surface());
        window.set_ime_allowed(false);

        #[cfg(target_os = "macos")]
        with_shadow(&window, !self.background_color.is_transparent());

        self.window = Some(window);
        self.viewport = Some(viewport);
        self.ime_composing = false;
        self.ime_dirty = true;
        self.last_ime_focus_id = None;
        self.last_ime_allowed = false;
        self.last_ime_area = None;
        self.cursor_in_window = false;
        self.last_mouse_position_viewport = None;
        self.applied_theme_dark = None;
        self.sync_theme_visuals();
        self.rebuild_app();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                self.window = None;
            }
            WindowEvent::Resized(size) => {
                if let Some(viewport) = &mut self.viewport {
                    viewport.set_size(size.width, size.height);
                    viewport.request_redraw();
                }
                self.mark_ime_dirty();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
                    viewport.set_scale_factor(scale_factor as f32);
                    let size: PhysicalSize<u32> = window.inner_size();
                    viewport.set_size(size.width, size.height);
                    viewport.request_redraw();
                }
                self.mark_ime_dirty();
            }
            WindowEvent::RedrawRequested => {
                self.sync_theme_visuals();
                if let (Some(viewport), Some(app)) = (&mut self.viewport, &self.app) {
                    viewport
                        .set_debug_geometry_overlay(DEBUG_GEOMETRY_OVERLAY.load(Ordering::Relaxed));
                    viewport.set_debug_trace_render_time(DEBUG_RENDER_TIME.load(Ordering::Relaxed));
                    let _ = viewport.render_rsx(&app);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_in_window = true;
                if let Some(viewport) = &mut self.viewport {
                    let (logical_x, logical_y) =
                        viewport.physical_to_logical_point(position.x as f32, position.y as f32);
                    self.last_mouse_position_viewport = Some((logical_x, logical_y));
                    viewport.set_mouse_position_viewport(logical_x, logical_y);
                    viewport.dispatch_mouse_move_event();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_in_window = false;
                if let Some(viewport) = &mut self.viewport {
                    viewport.clear_mouse_position_viewport();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(viewport) = &mut self.viewport {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (x * 24.0, y * 24.0),
                        MouseScrollDelta::PixelDelta(pos) => {
                            viewport.physical_to_logical_point(pos.x as f32, pos.y as f32)
                        }
                    };
                    viewport.dispatch_mouse_wheel_event(-dx, -dy);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(viewport) = &mut self.viewport {
                    let button = map_mouse_button(button);
                    viewport.set_mouse_button_pressed(button, state == ElementState::Pressed);
                    if state == ElementState::Pressed {
                        viewport.dispatch_mouse_down_event(button);
                    } else {
                        viewport.dispatch_mouse_up_event(button);
                        viewport.dispatch_click_event(button);
                    }
                }
                self.mark_ime_dirty();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(viewport) = &mut self.viewport {
                    let key = key_to_string(&event.logical_key);
                    let pressed = event.state == ElementState::Pressed;
                    viewport.set_key_pressed(key.clone(), pressed);
                    let code = format!("{:?}", event.physical_key);
                    if pressed {
                        viewport.dispatch_key_down_event(key, code, event.repeat);
                        if !self.ime_composing {
                            if let Some(text) = event.text.as_deref() {
                                if should_dispatch_keyboard_text(viewport, text) {
                                    viewport.dispatch_text_input_event(text.to_string());
                                }
                            }
                        }
                    } else {
                        viewport.dispatch_key_up_event(key, code, event.repeat);
                    }
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.ime_composing = false;
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_text_input_event(text);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                self.ime_composing = !text.is_empty();
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_ime_preedit_event(text, cursor);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Disabled) => {
                self.ime_composing = false;
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_ime_preedit_event(String::new(), None);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Focused(false) => {
                self.ime_composing = false;
                if let Some(viewport) = &mut self.viewport {
                    viewport.clear_input_state();
                }
                self.mark_ime_dirty();
            }
            _ => (),
        }

        if take_state_dirty() {
            self.rebuild_app();
            if let Some(viewport) = &mut self.viewport {
                viewport.request_redraw();
            }
        }

        if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
            if viewport.take_redraw_request() {
                window.request_redraw();
            }
        }
        self.sync_ime_state(false);
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self.cursor_in_window {
            return;
        }
        let Some(viewport) = &mut self.viewport else {
            return;
        };
        if !viewport.has_viewport_mouse_listeners() {
            return;
        }

        match event {
            DeviceEvent::MouseMotion { delta } => {
                let Some((x, y)) = self.last_mouse_position_viewport else {
                    return;
                };
                let (dx, dy) = viewport.physical_to_logical_point(delta.0 as f32, delta.1 as f32);
                let next = (x + dx, y + dy);
                self.last_mouse_position_viewport = Some(next);
                viewport.set_mouse_position_viewport(next.0, next.1);
                viewport.dispatch_mouse_move_event();
            }
            DeviceEvent::Button { button, state } => {
                if state != ElementState::Released {
                    return;
                }
                let Some(mapped_button) = map_device_button(button) else {
                    return;
                };
                if let Some((x, y)) = self.last_mouse_position_viewport {
                    viewport.set_mouse_position_viewport(x, y);
                }
                viewport.set_mouse_button_pressed(mapped_button, false);
                viewport.dispatch_mouse_up_event(mapped_button);
            }
            _ => {}
        }
    }
}

fn map_mouse_button(button: winit::event::MouseButton) -> rfgui::MouseButton {
    match button {
        winit::event::MouseButton::Left => rfgui::MouseButton::Left,
        winit::event::MouseButton::Right => rfgui::MouseButton::Right,
        winit::event::MouseButton::Middle => rfgui::MouseButton::Middle,
        winit::event::MouseButton::Back => rfgui::MouseButton::Back,
        winit::event::MouseButton::Forward => rfgui::MouseButton::Forward,
        winit::event::MouseButton::Other(v) => rfgui::MouseButton::Other(v),
    }
}

fn map_device_button(button: u32) -> Option<rfgui::MouseButton> {
    match button {
        1 => Some(rfgui::MouseButton::Left),
        2 => Some(rfgui::MouseButton::Right),
        3 => Some(rfgui::MouseButton::Middle),
        4 => Some(rfgui::MouseButton::Back),
        5 => Some(rfgui::MouseButton::Forward),
        _ => None,
    }
}

fn key_to_string(key: &Key) -> String {
    match key {
        Key::Character(text) => text.to_string(),
        _ => format!("{key:?}"),
    }
}

fn should_dispatch_keyboard_text(viewport: &Viewport, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if text.chars().any(|ch| ch.is_control()) {
        return false;
    }
    // Keep shortcuts (Ctrl/Alt/Cmd + key) out of text-input path.
    let has_alt = viewport.is_key_pressed("Named(Alt)")
        || viewport.is_key_pressed("Named(AltGraph)")
        || viewport.is_key_pressed("Code(AltLeft)")
        || viewport.is_key_pressed("Code(AltRight)");
    let has_ctrl = viewport.is_key_pressed("Named(Control)")
        || viewport.is_key_pressed("Code(ControlLeft)")
        || viewport.is_key_pressed("Code(ControlRight)");
    let has_meta = viewport.is_key_pressed("Named(Super)")
        || viewport.is_key_pressed("Named(Meta)")
        || viewport.is_key_pressed("Code(SuperLeft)")
        || viewport.is_key_pressed("Code(SuperRight)")
        || viewport.is_key_pressed("Code(MetaLeft)")
        || viewport.is_key_pressed("Code(MetaRight)");
    !(has_alt || has_ctrl || has_meta)
}

fn map_cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::ContextMenu => CursorIcon::ContextMenu,
        Cursor::Help => CursorIcon::Help,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Progress => CursorIcon::Progress,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Cell => CursorIcon::Cell,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Text => CursorIcon::Text,
        Cursor::VerticalText => CursorIcon::VerticalText,
        Cursor::Alias => CursorIcon::Alias,
        Cursor::Copy => CursorIcon::Copy,
        Cursor::Move => CursorIcon::Move,
        Cursor::NoDrop => CursorIcon::NoDrop,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::EResize => CursorIcon::EResize,
        Cursor::NResize => CursorIcon::NResize,
        Cursor::NeResize => CursorIcon::NeResize,
        Cursor::NwResize => CursorIcon::NwResize,
        Cursor::SResize => CursorIcon::SResize,
        Cursor::SeResize => CursorIcon::SeResize,
        Cursor::SwResize => CursorIcon::SwResize,
        Cursor::WResize => CursorIcon::WResize,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
        Cursor::AllScroll => CursorIcon::AllScroll,
        Cursor::ZoomIn => CursorIcon::ZoomIn,
        Cursor::ZoomOut => CursorIcon::ZoomOut,
        Cursor::DndAsk => CursorIcon::DndAsk,
        Cursor::AllResize => CursorIcon::AllResize,
    }
}

fn main() {
    init_theme(Theme::dark());
    THEME_DARK_MODE.store(true, Ordering::Relaxed);
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    app.background_color = app_background_color(true);
    app.applied_theme_dark = Some(true);
    event_loop.run_app(&mut app).unwrap();
}

fn app_background_color(is_dark: bool) -> Box<dyn ColorLike> {
    if is_dark {
        Box::new(Color::hex("#282c34"))
    } else {
        Box::new(Color::hex("#f8fafc"))
    }
}

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowExtMacOS;

#[cfg(target_os = "macos")]
fn with_shadow(window: &winit::window::Window, has_shadow: bool) {
    window.set_has_shadow(has_shadow);
}
