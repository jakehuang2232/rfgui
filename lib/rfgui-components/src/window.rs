use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use rfgui::ClipMode::Viewport;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{MouseButton, MouseDownHandlerProp, MouseMoveHandlerProp, MouseUpHandlerProp, RsxComponent, RsxNode, on_mouse_down, on_mouse_move, on_mouse_up, rsx, use_state, props};
use rfgui::{
    AlignItems, Border, BorderRadius, Color, Display, FontWeight, JustifyContent, Length, Padding,
    Position,
};

const MIN_WIDTH: f32 = 220.0;
const MIN_HEIGHT: f32 = 140.0;
const TITLE_BAR_HEIGHT: f32 = 24.0;

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
        start_mouse_x: f32,
        start_mouse_y: f32,
        start_width: f32,
        start_height: f32,
    },
}

pub struct Window;

#[props]
pub struct WindowProps {
    pub title: String,
    pub draggable: Option<bool>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub on_resize: Option<ResizeHandlerProp>,
    pub children: Vec<RsxNode>,
}

impl RsxComponent for Window {
    type Props = WindowProps;

    fn render(props: Self::Props) -> RsxNode {
        let width = props.width.unwrap_or(360.0).max(MIN_WIDTH as f64) as f32;
        let height = props.height.unwrap_or(240.0).max(MIN_HEIGHT as f64) as f32;

        rsx! {
            <WindowView
                title={props.title}
                draggable={props.draggable.unwrap_or(true)}
                initial_width={width}
                initial_height={height}
                on_resize={props.on_resize}
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
    children: Vec<RsxNode>,
) -> RsxNode {
    let position = use_state(|| (24.0_f32, 24.0_f32));
    let size = use_state(|| (initial_width, initial_height));
    let interaction = use_state(|| WindowInteraction::Idle);

    let (x, y) = position.get();
    let (width, height) = size.get();
    let content_height = (height - TITLE_BAR_HEIGHT).max(0.0);

    let title_down: MouseDownHandlerProp = {
        let interaction = interaction.binding();
        on_mouse_down(move |event| {
            if !draggable || event.mouse.button != Some(MouseButton::Left) {
                return;
            }
            event.meta.request_pointer_capture();
            interaction.set(WindowInteraction::Dragging {
                start_mouse_x: event.mouse.viewport_x,
                start_mouse_y: event.mouse.viewport_y,
                start_x: x,
                start_y: y,
            });
            event.meta.stop_propagation();
        })
    };

    let resize_down: MouseDownHandlerProp = {
        let interaction = interaction.binding();
        on_mouse_down(move |event| {
            if event.mouse.button != Some(MouseButton::Left) {
                return;
            }
            event.meta.request_pointer_capture();
            interaction.set(WindowInteraction::Resizing {
                start_mouse_x: event.mouse.viewport_x,
                start_mouse_y: event.mouse.viewport_y,
                start_width: width,
                start_height: height,
            });
            event.meta.stop_propagation();
        })
    };

    let root_move: MouseMoveHandlerProp = {
        let interaction = interaction.binding();
        let position = position.binding();
        let size = size.binding();
        let on_resize = on_resize.clone();
        on_mouse_move(move |event| {
            if !event.mouse.buttons.left {
                interaction.set(WindowInteraction::Idle);
                return;
            }

            match interaction.get() {
                WindowInteraction::Idle => {}
                WindowInteraction::Dragging {
                    start_mouse_x,
                    start_mouse_y,
                    start_x,
                    start_y,
                } => {
                    let next_x = start_x + (event.mouse.viewport_x - start_mouse_x);
                    let next_y = start_y + (event.mouse.viewport_y - start_mouse_y);
                    position.set((next_x, next_y));
                    event.meta.stop_propagation();
                }
                WindowInteraction::Resizing {
                    start_mouse_x,
                    start_mouse_y,
                    start_width,
                    start_height,
                } => {
                    let next_width =
                        (start_width + (event.mouse.viewport_x - start_mouse_x)).max(MIN_WIDTH);
                    let next_height =
                        (start_height + (event.mouse.viewport_y - start_mouse_y)).max(MIN_HEIGHT);
                    size.set((next_width, next_height));
                    if let Some(handler) = &on_resize {
                        handler.call(next_width, next_height);
                    }
                    event.meta.stop_propagation();
                }
            }
        })
    };

    let root_up: MouseUpHandlerProp = {
        let interaction = interaction.binding();
        on_mouse_up(move |event| {
            if event.mouse.button == Some(MouseButton::Left) {
                interaction.set(WindowInteraction::Idle);
                event.meta.stop_propagation();
            }
        })
    };

    rsx! {
        <Element
            style={{
                position: Position::absolute().left(Length::px(x)).top(Length::px(y)).anchor("root").clip(Viewport),
                width: Length::px(width),
                height: Length::px(height),
                display: Display::flow().column().no_wrap(),
                border: Border::uniform(Length::px(1.0), &Color::hex("#94A3B8")),
                border_radius: BorderRadius::uniform(Length::px(10.0)),
                background: Color::hex("#FFFFFF"),
            }}
            on_mouse_move={root_move}
            on_mouse_up={root_up}
        >
            <Element
                style={{
                    height: Length::px(TITLE_BAR_HEIGHT),
                    width: Length::percent(100.0),
                    display: Display::flow().row().no_wrap(),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: Padding::uniform(Length::px(0.0)).x(Length::px(12.0)),
                    background: Color::hex("#E2E8F0"),
                    border_radius: BorderRadius::uniform(Length::px(0.0)).top(Length::px(10.0)),
                }}
                on_mouse_down={title_down}
            >
                <Text style={{ color: "#0F172A", font_weight: FontWeight::semi_bold() }}>{title}</Text>
            </Element>
            <Element
                style={{
                    width: Length::percent(100.0),
                    height: Length::px(content_height),
                    padding: Padding::uniform(Length::px(12.0)),
                    display: Display::flow().column(),
                    background: Color::hex("#FFFFFF"),
                }}
            >
                {children}
            </Element>
            <Element
                style={{
                    position: Position::absolute()
                        .right(Length::px(0.0))
                        .bottom(Length::px(0.0)),
                    width: Length::px(14.0),
                    height: Length::px(14.0),
                    background: Color::hex("#CBD5E1"),
                    border_radius: BorderRadius::uniform(Length::px(3.0)),
                }}
                on_mouse_down={resize_down}
            />
        </Element>
    }
}
