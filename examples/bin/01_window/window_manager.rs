use crate::rfgui::ui::{Binding, RsxNode, on_focus, rsx};
use crate::rfgui_components::{Window, WindowProps, on_move};

pub struct ManagedWindow {
    id: usize,
    props: WindowProps,
    children: Vec<RsxNode>,
}

pub struct WindowManager {
    windows: Vec<ManagedWindow>,
    positions: Binding<Vec<(f32, f32)>>,
}

impl WindowManager {
    pub const WINDOW_DEFAULT_WIDTH: f64 = 360.0;
    pub const WINDOW_DEFAULT_HEIGHT: f64 = 240.0;
    const WINDOW_INIT_OFFSET: f32 = 48.0;

    pub fn new(positions: Binding<Vec<(f32, f32)>>) -> Self {
        Self {
            windows: Vec::new(),
            positions,
        }
    }

    pub fn push(&mut self, title: impl Into<String>, children: Vec<RsxNode>, size: (f64, f64)) {
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
            },
            children,
        });
    }

    pub fn into_nodes(self, z_order: Binding<Vec<usize>>) -> Vec<RsxNode> {
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
                        on_focus={focus}
                        on_blur={props.on_blur.clone()}
                    >
                        {window_entry.children.clone()}
                    </Window>
                };
                ordered_windows.push(window);
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
