use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::use_theme;
use rfgui::ClipMode::Viewport;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    BlurHandlerProp, FocusHandlerProp, MouseButton, MouseDownHandlerProp, RsxComponent, RsxNode,
    ViewportListenerHandle, on_mouse_down, props, rsx, use_state,
};
use rfgui::{AlignItems, Border, BorderRadius, Color, ColorLike, Cursor, Display, FontWeight, JustifyContent, Length, Padding, Position, ScrollDirection};

const MIN_WIDTH: f32 = 220.0;
const MIN_HEIGHT: f32 = 140.0;
const TITLE_BAR_HEIGHT: f32 = 24.0;
const RESIZE_EDGE_THICKNESS: f32 = 2.0;
const RESIZE_CORNER_SIZE: f32 = 14.0;

#[derive(Clone)]
pub struct ResizeHandlerProp {
    id: u64,
    handler: Rc<RefCell<dyn FnMut(f32, f32)>>,
}

impl ResizeHandlerProp {
    pub fn new<F>(handler: F) -> Self
    where
        F: FnMut(f32, f32) + 'static,
    {
        Self {
            id: next_resize_handler_id(),
            handler: Rc::new(RefCell::new(handler)),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn call(&self, width: f32, height: f32) {
        (self.handler.borrow_mut())(width, height);
    }
}

impl PartialEq for ResizeHandlerProp {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for ResizeHandlerProp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResizeHandlerProp")
            .field("id", &self.id)
            .finish()
    }
}

pub fn on_resize<F>(handler: F) -> ResizeHandlerProp
where
    F: FnMut(f32, f32) + 'static,
{
    ResizeHandlerProp::new(handler)
}

fn next_resize_handler_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy)]
enum WindowInteraction {
    Idle,
    Dragging {
        start_mouse_x: f32,
        start_mouse_y: f32,
        start_x: f32,
        start_y: f32,
    },
    Resizing {
        edge: ResizeEdge,
        start_mouse_x: f32,
        start_mouse_y: f32,
        start_x: f32,
        start_y: f32,
        start_width: f32,
        start_height: f32,
    },
}

#[derive(Clone, Copy)]
enum ResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
    BottomLeft,
    BottomRight,
}

impl ResizeEdge {
    fn cursor(self) -> Cursor {
        match self {
            Self::Left | Self::Right => Cursor::EwResize,
            Self::Top | Self::Bottom => Cursor::NsResize,
            Self::BottomLeft => Cursor::NeswResize,
            Self::BottomRight => Cursor::NwseResize,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct WindowViewportListenerState {
    move_listener: Option<ViewportListenerHandle>,
    up_listener: Option<ViewportListenerHandle>,
}

pub struct Window;

#[props]
pub struct WindowProps {
    pub title: String,
    pub draggable: Option<bool>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub on_resize: Option<ResizeHandlerProp>,
    pub on_focus: Option<FocusHandlerProp>,
    pub on_blur: Option<BlurHandlerProp>,
    pub window_slots: Option<WindowSlotsProp>,
    pub children: Vec<RsxNode>,
}

#[props]
pub struct WindowSlotsProp {
    pub root_style: Option<WindowRootStyleSlot>,
    pub title_bar_style: Option<WindowTitleBarStyleSlot>,
    pub title_text_style: Option<WindowTitleTextStyleSlot>,
    pub content_style: Option<WindowContentStyleSlot>,
}

#[props]
pub struct WindowRootStyleSlot {
    pub background: Option<Color>,
    pub border: Option<Border>,
    pub border_radius: Option<BorderRadius>,
}

#[props]
#[derive(Clone, Copy)]
pub struct WindowTitleBarStyleSlot {
    pub background: Option<Color>,
    pub padding: Option<Padding>,
    pub height: Option<Length>,
}

#[props]
#[derive(Clone, Copy)]
pub struct WindowTitleTextStyleSlot {
    pub color: Option<Color>,
    pub font_weight: Option<FontWeight>,
}

#[props]
#[derive(Clone, Copy)]
pub struct WindowContentStyleSlot {
    pub padding: Option<Padding>,
    pub background: Option<Color>,
}

impl RsxComponent<WindowProps> for Window {
    fn render(props: WindowProps) -> RsxNode {
        let width = props.width.unwrap_or(360.0).max(MIN_WIDTH as f64) as f32;
        let height = props.height.unwrap_or(240.0).max(MIN_HEIGHT as f64) as f32;

        rsx! {
            <WindowView
                title={props.title}
                draggable={props.draggable.unwrap_or(true)}
                initial_width={width}
                initial_height={height}
                on_resize={props.on_resize}
                on_focus={props.on_focus}
                on_blur={props.on_blur}
                window_slots={props.window_slots}
                children={props.children}
            />
        }
    }
}

#[rfgui::ui::component]
fn WindowView(
    title: String,
    draggable: bool,
    initial_width: f32,
    initial_height: f32,
    on_resize: Option<ResizeHandlerProp>,
    on_focus: Option<FocusHandlerProp>,
    on_blur: Option<BlurHandlerProp>,
    window_slots: Option<WindowSlotsProp>,
    children: Vec<RsxNode>,
) -> RsxNode {
    let theme = use_theme().get();
    let position = use_state(|| (24.0_f32, 24.0_f32));
    let size = use_state(|| (initial_width, initial_height));
    let interaction = use_state(|| WindowInteraction::Idle);
    let viewport_listeners = use_state(WindowViewportListenerState::default);

    let (x, y) = position.get();
    let (width, height) = size.get();
    let (root_style_slot, title_bar_style_slot, title_text_style_slot, content_style_slot) =
        if let Some(slots) = window_slots {
            (
                slots.root_style,
                slots.title_bar_style,
                slots.title_text_style,
                slots.content_style,
            )
        } else {
            (None, None, None, None)
        };

    let mut root_background = None;
    let mut root_border = None;
    let mut root_border_radius = None;
    if let Some(root_style) = root_style_slot {
        root_background = root_style.background;
        root_border = root_style.border;
        root_border_radius = root_style.border_radius;
    }

    let root_background = root_background
        .unwrap_or_else(|| color_like_to_color(theme.color.layer.raised.as_ref()));
    let root_border =
        root_border.unwrap_or(Border::uniform(Length::px(1.0), theme.color.border.as_ref()));
    let root_border_radius =
        root_border_radius.unwrap_or(theme.component.card.radius);

    let title_bar_height_length = title_bar_style_slot
        .and_then(|style| style.height)
        .unwrap_or(Length::px(TITLE_BAR_HEIGHT));
    let title_bar_height_px = title_bar_height_length
        .resolve_with_base(Some(height), width, height)
        .unwrap_or(0.0);
    let content_height = (height - title_bar_height_px).max(0.0);

    let title_bar_background = title_bar_style_slot
        .and_then(|style| style.background)
        .unwrap_or_else(|| color_like_to_color(theme.color.layer.inverse.as_ref()));
    let title_bar_padding = title_bar_style_slot
        .and_then(|style| style.padding)
        .unwrap_or(Padding::uniform(Length::px(0.0)).x(theme.spacing.sm));
    let title_text_color = title_text_style_slot
        .and_then(|style| style.color)
        .unwrap_or_else(|| color_like_to_color(theme.color.layer.on_inverse.as_ref()));
    let title_text_weight = title_text_style_slot
        .and_then(|style| style.font_weight)
        .unwrap_or(FontWeight::semi_bold());
    let content_padding = content_style_slot
        .and_then(|style| style.padding)
        .unwrap_or(theme.component.card.padding);
    let content_text_color = theme.color.text.primary;
    let content_background = content_style_slot
        .and_then(|style| style.background)
        .unwrap_or_else(|| color_like_to_color(theme.color.layer.surface.as_ref()));

    let title_down: MouseDownHandlerProp = {
        let interaction = interaction.binding();
        let position = position.binding();
        let viewport_listeners = viewport_listeners.binding();
        on_mouse_down(move |event| {
            if !draggable || event.mouse.button != Some(MouseButton::Left) {
                return;
            }
            event.viewport.set_focus(Some(event.meta.current_target_id()));
            let (start_x, start_y) = position.get();
            interaction.set(WindowInteraction::Dragging {
                start_mouse_x: event.mouse.viewport_x,
                start_mouse_y: event.mouse.viewport_y,
                start_x,
                start_y,
            });
            let listeners = viewport_listeners.get();
            if let Some(handle) = listeners.move_listener {
                event.viewport.remove_listener(handle);
            }
            if let Some(handle) = listeners.up_listener {
                event.viewport.remove_listener(handle);
            }
            let interaction_for_move = interaction.clone();
            let position_for_move = position.clone();
            let move_listener =
                event.viewport.add_mouse_move_listener(
                    move |move_event| match interaction_for_move.get() {
                        WindowInteraction::Dragging {
                            start_mouse_x,
                            start_mouse_y,
                            start_x,
                            start_y,
                        } => {
                            let next_x = start_x + (move_event.mouse.viewport_x - start_mouse_x);
                            let next_y = start_y + (move_event.mouse.viewport_y - start_mouse_y);
                            position_for_move.set((next_x, next_y));
                            move_event.meta.stop_propagation();
                        }
                        WindowInteraction::Resizing { .. } => {}
                        WindowInteraction::Idle => {}
                    },
                );
            let interaction_for_up = interaction.clone();
            let viewport_listeners_for_up = viewport_listeners.clone();
            let up_listener = event.viewport.add_mouse_up_listener_until(move |up_event| {
                if up_event.mouse.button != Some(MouseButton::Left) {
                    return false;
                }
                up_event.viewport.remove_listener(move_listener);
                interaction_for_up.set(WindowInteraction::Idle);
                viewport_listeners_for_up.set(WindowViewportListenerState::default());
                up_event.meta.stop_propagation();
                true
            });
            viewport_listeners.set(WindowViewportListenerState {
                move_listener: Some(move_listener),
                up_listener: Some(up_listener),
            });
            event.meta.stop_propagation();
        })
    };

    let make_resize_down = |edge: ResizeEdge| {
        let interaction = interaction.binding();
        let size = size.binding();
        let position = position.binding();
        let on_resize = on_resize.clone();
        let viewport_listeners = viewport_listeners.binding();
        on_mouse_down(move |event| {
            if event.mouse.button != Some(MouseButton::Left) {
                return;
            }
            event.viewport.set_focus(Some(event.meta.current_target_id()));
            event.viewport.set_cursor(Some(edge.cursor()));
            let (start_x, start_y) = position.get();
            let (start_width, start_height) = size.get();
            interaction.set(WindowInteraction::Resizing {
                edge,
                start_mouse_x: event.mouse.viewport_x,
                start_mouse_y: event.mouse.viewport_y,
                start_x,
                start_y,
                start_width,
                start_height,
            });
            let listeners = viewport_listeners.get();
            if let Some(handle) = listeners.move_listener {
                event.viewport.remove_listener(handle);
            }
            if let Some(handle) = listeners.up_listener {
                event.viewport.remove_listener(handle);
            }
            let interaction_for_move = interaction.clone();
            let size_for_move = size.clone();
            let position_for_move = position.clone();
            let on_resize_for_move = on_resize.clone();
            let move_listener = event.viewport.add_mouse_move_listener(move |move_event| {
                if let WindowInteraction::Resizing {
                    edge,
                    start_mouse_x,
                    start_mouse_y,
                    start_x,
                    start_y,
                    start_width,
                    start_height,
                } = interaction_for_move.get()
                {
                    let dx = move_event.mouse.viewport_x - start_mouse_x;
                    let dy = move_event.mouse.viewport_y - start_mouse_y;

                    let mut next_x = start_x;
                    let mut next_y = start_y;
                    let mut next_width = start_width;
                    let mut next_height = start_height;

                    match edge {
                        ResizeEdge::Right => {
                            next_width = (start_width + dx).max(MIN_WIDTH);
                        }
                        ResizeEdge::Bottom => {
                            next_height = (start_height + dy).max(MIN_HEIGHT);
                        }
                        ResizeEdge::BottomRight => {
                            next_width = (start_width + dx).max(MIN_WIDTH);
                            next_height = (start_height + dy).max(MIN_HEIGHT);
                        }
                        ResizeEdge::Left => {
                            let raw_width = start_width - dx;
                            next_width = raw_width.max(MIN_WIDTH);
                            next_x = start_x + (start_width - next_width);
                        }
                        ResizeEdge::Top => {
                            let raw_height = start_height - dy;
                            next_height = raw_height.max(MIN_HEIGHT);
                            next_y = start_y + (start_height - next_height);
                        }
                        ResizeEdge::BottomLeft => {
                            let raw_width = start_width - dx;
                            next_width = raw_width.max(MIN_WIDTH);
                            next_x = start_x + (start_width - next_width);
                            next_height = (start_height + dy).max(MIN_HEIGHT);
                        }
                    }

                    position_for_move.set((next_x, next_y));
                    size_for_move.set((next_width, next_height));
                    if let Some(handler) = &on_resize_for_move {
                        handler.call(next_width, next_height);
                    }
                    move_event.meta.stop_propagation();
                }
            });
            let interaction_for_up = interaction.clone();
            let viewport_listeners_for_up = viewport_listeners.clone();
            let up_listener = event.viewport.add_mouse_up_listener_until(move |up_event| {
                if up_event.mouse.button != Some(MouseButton::Left) {
                    return false;
                }
                up_event.viewport.remove_listener(move_listener);
                if let WindowInteraction::Resizing { .. } = interaction_for_up.get() {
                    up_event.viewport.set_cursor(None);
                }
                interaction_for_up.set(WindowInteraction::Idle);
                viewport_listeners_for_up.set(WindowViewportListenerState::default());
                up_event.meta.stop_propagation();
                true
            });
            viewport_listeners.set(WindowViewportListenerState {
                move_listener: Some(move_listener),
                up_listener: Some(up_listener),
            });
            event.meta.stop_propagation();
        })
    };
    let resize_left_down = make_resize_down(ResizeEdge::Left);
    let resize_right_down = make_resize_down(ResizeEdge::Right);
    let resize_top_down = make_resize_down(ResizeEdge::Top);
    let resize_bottom_down = make_resize_down(ResizeEdge::Bottom);
    let resize_bottom_left_down = make_resize_down(ResizeEdge::BottomLeft);
    let resize_bottom_right_down = make_resize_down(ResizeEdge::BottomRight);

    rsx! {
        <Element
            style={{
                position: Position::absolute().left(Length::px(x)).top(Length::px(y)).anchor("root").clip(Viewport),
                width: Length::px(width),
                height: Length::px(height),
                display: Display::flow().column().no_wrap(),
                background: root_background,
                border: root_border,
                border_radius: root_border_radius,
                box_shadow: vec![
                    theme.shadow.level_3,
                ],
            }}
            on_focus={on_focus}
            on_blur={on_blur}
        >
            <Element
                style={{
                    height: title_bar_height_length,
                    width: Length::percent(100.0),
                    display: Display::flow()
                        .row()
                        .no_wrap()
                        .justify_content(JustifyContent::SpaceBetween),
                    align_items: AlignItems::Center,
                    padding: title_bar_padding,
                    background: title_bar_background,
                    border_radius: BorderRadius::uniform(Length::px(0.0)).top(theme.radius.lg),
                }}
                on_mouse_down={title_down}
            >
                <Text style={{ color: title_text_color, font_weight: title_text_weight }}>{title}</Text>
            </Element>
            <Element
                style={{
                    width: Length::percent(100.0),
                    height: Length::px(content_height),
                    padding: content_padding,
                    display: Display::flow().column(),
                    background: content_background,
                    color: content_text_color,
                    scroll_direction: ScrollDirection::Both,
                }}
            >
                {children}
            </Element>
            <Element
                style={{
                    position: Position::absolute()
                        .left(Length::px(-RESIZE_EDGE_THICKNESS))
                        .top(Length::px(0.0))
                        .bottom(Length::px(0.0))
                        .clip(Viewport),
                    width: Length::px(RESIZE_EDGE_THICKNESS * 2.0),
                    cursor: Cursor::EwResize,
                }}
                on_mouse_down={resize_left_down}
            />
            <Element
                style={{
                    position: Position::absolute()
                        .right(Length::px(-RESIZE_EDGE_THICKNESS))
                        .top(Length::px(0.0))
                        .bottom(Length::px(0.0))
                        .clip(Viewport),
                    width: Length::px(RESIZE_EDGE_THICKNESS * 2.0),
                    cursor: Cursor::EwResize,
                }}
                on_mouse_down={resize_right_down}
            />
            <Element
                style={{
                    position: Position::absolute()
                        .left(Length::px(0.0))
                        .right(Length::px(0.0))
                        .top(Length::px(-RESIZE_EDGE_THICKNESS))
                        .clip(Viewport),
                    height: Length::px(RESIZE_EDGE_THICKNESS * 2.0),
                    cursor: Cursor::NsResize,
                }}
                on_mouse_down={resize_top_down}
            />
            <Element
                style={{
                    position: Position::absolute()
                        .left(Length::px(0.0))
                        .right(Length::px(0.0))
                        .bottom(Length::px(-RESIZE_EDGE_THICKNESS))
                        .clip(Viewport),
                    height: Length::px(RESIZE_EDGE_THICKNESS * 2.0),
                    cursor: Cursor::NsResize,
                }}
                on_mouse_down={resize_bottom_down}
            />
            <Element
                style={{
                    position: Position::absolute()
                        .left(Length::px(-RESIZE_CORNER_SIZE / 2.0))
                        .bottom(Length::px(-RESIZE_CORNER_SIZE / 2.0))
                        .clip(Viewport),
                    width: Length::px(RESIZE_CORNER_SIZE),
                    height: Length::px(RESIZE_CORNER_SIZE),
                    cursor: Cursor::NeswResize,
                }}
                on_mouse_down={resize_bottom_left_down}
            />
            <Element
                style={{
                    position: Position::absolute()
                        .right(Length::px(-RESIZE_CORNER_SIZE / 2.0))
                        .bottom(Length::px(-RESIZE_CORNER_SIZE / 2.0))
                        .clip(Viewport),
                    width: Length::px(RESIZE_CORNER_SIZE),
                    height: Length::px(RESIZE_CORNER_SIZE),
                    cursor: Cursor::NwseResize,
                }}
                on_mouse_down={resize_bottom_right_down}
            />
        </Element>
    }
}

fn color_like_to_color(color: &dyn ColorLike) -> Color {
    let [r, g, b, a] = color.to_rgba_u8();
    Color::rgba(r, g, b, a)
}
