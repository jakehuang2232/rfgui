use crate::rfgui::style::{Border, Length};
use crate::rfgui::ui::{RsxNode, component, on_focus, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{Window, on_move, use_theme};
use crate::showcase::Demo;

const TITLES: [&str; 2] = ["Notes", "Preview"];
const INITIAL_POSITIONS: [(f32, f32); 2] = [(16.0, 16.0), (220.0, 72.0)];

#[component]
pub fn Windows() -> RsxNode {
    let (theme, _) = use_theme();
    let positions = use_state(|| INITIAL_POSITIONS);
    // Window indices back to front; a focused window moves to the end.
    let order = use_state(|| [0_usize, 1]);

    let windows = order
        .get()
        .into_iter()
        .map(|index| {
            let moved = {
                let positions = positions.clone();
                on_move(move |x, y| positions.update(move |all| all[index] = (x, y)))
            };
            let raised = {
                let order = order.clone();
                on_focus(move |_| {
                    order.update(move |order| {
                        if order[1] != index {
                            order.swap(0, 1);
                        }
                    })
                })
            };
            rsx! {
                <Window
                    key={index}
                    title={TITLES[index].to_string()}
                    width={260.0}
                    height={180.0}
                    position={positions.get()[index]}
                    on_move={moved}
                    on_focus={raised}
                    scrollable={false}
                >
                    <Text>{format!(
                        "Drag the title bar to move {}, or an edge to resize it. Clicking a window brings it to the front.",
                        TITLES[index]
                    )}</Text>
                </Window>
            }
        })
        .collect::<Vec<_>>();

    rsx! {
        <Demo
            title="Window"
            description="Draggable, resizable windows. They are positioned inside this area and clipped to it."
        >
            <Element style={{
                width: Length::percent(100.0),
                height: Length::px(320.0),
                background: theme.color.layer.raised.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.md,
            }}>
                {windows}
            </Element>
        </Demo>
    }
}
