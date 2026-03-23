mod inputs;
mod layout;
mod theme;

pub use inputs::*;
pub use layout::*;
pub use theme::*;

#[cfg(test)]
mod tests {
    use crate::{Accordion, Button, ButtonVariant, Checkbox, Select, Switch, Window};
    use rfgui::ui::host::{Element, ElementPropSchema, Text, TextPropSchema};
    use rfgui::ui::{
        EventMeta, MouseButton, MouseEventData, RsxNode, create_element, global_state, rsx,
        take_state_dirty,
    };
    use std::marker::PhantomData;

    fn select_label(item: &String, _: usize) -> String {
        item.clone()
    }

    #[test]
    fn checkbox_click_updates_binding() {
        let checked = global_state(|| false);

        let tree = rsx! {
            <Checkbox
                label="Enable"
                binding={checked.binding()}
            />
        };

        let mut roots = rfgui::rsx_to_elements(&tree).expect("convert checkbox");
        let root = roots.get_mut(0).expect("has root");
        root.measure(rfgui::view::base_component::LayoutConstraints {
            max_width: 320.0,
            max_height: 120.0,
            viewport_width: 320.0,
            viewport_height: 120.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(120.0),
        });
        root.place(rfgui::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 120.0,
            viewport_width: 320.0,
            viewport_height: 120.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(120.0),
        });

        let mut viewport = rfgui::view::Viewport::new();
        let mut control = rfgui::view::ViewportControl::new(&mut viewport);
        let mut click = rfgui::ui::ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: 8.0,
                viewport_y: 8.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: rfgui::ui::MouseButtons::default(),
                modifiers: rfgui::ui::KeyModifiers::default(),
            },
        };

        let handled = rfgui::view::base_component::dispatch_click_from_hit_test(
            root.as_mut(),
            &mut click,
            &mut control,
        );
        assert!(handled);
        assert!(checked.get());
    }

    #[test]
    fn select_trigger_click_does_not_change_binding_value() {
        let selected = global_state(|| String::from("Option A"));
        let tree = rsx! {
            <Select
                data={vec![
                    String::from("Option A"),
                    String::from("Option B"),
                    String::from("Option C"),
                ]}
                to_label={select_label}
                value={selected.binding()}
            />
        };

        let mut roots = rfgui::rsx_to_elements(&tree).expect("convert select");
        let mut viewport = rfgui::view::Viewport::new();
        click_once(roots[0].as_mut(), &mut viewport, 10.0, 10.0);
        assert_eq!(selected.get(), "Option A");
        assert!(take_state_dirty());
    }

    #[test]
    fn select_open_state_persists_across_rerender() {
        let selected = global_state(|| String::from("Option A"));

        let build_tree = || {
            rsx! {
                <Select
                    data={vec![
                        String::from("Option A"),
                        String::from("Option B"),
                        String::from("Option C"),
                    ]}
                    to_label={select_label}
                    value={selected.binding()}
                />
            }
        };

        let first_tree = build_tree();
        let mut roots = rfgui::rsx_to_elements(&first_tree).expect("convert select");
        let mut viewport = rfgui::view::Viewport::new();
        click_once(roots[0].as_mut(), &mut viewport, 10.0, 10.0);
        assert!(take_state_dirty());

        let second_tree = build_tree();
        let RsxNode::Element(root) = second_tree else {
            panic!("select should render element root");
        };
        assert_eq!(
            root.children.len(),
            2,
            "select menu should remain open after rerender"
        );
    }

    fn click_once(
        root: &mut dyn rfgui::view::base_component::ElementTrait,
        viewport: &mut rfgui::view::Viewport,
        x: f32,
        y: f32,
    ) {
        root.measure(rfgui::view::base_component::LayoutConstraints {
            max_width: 320.0,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        });
        root.place(rfgui::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        });

        let mut control = rfgui::view::ViewportControl::new(viewport);
        let mut click = rfgui::ui::ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(MouseButton::Left),
                buttons: rfgui::ui::MouseButtons::default(),
                modifiers: rfgui::ui::KeyModifiers::default(),
            },
        };

        let handled = rfgui::view::base_component::dispatch_click_from_hit_test(
            root,
            &mut click,
            &mut control,
        );
        assert!(handled);
    }

    #[test]
    fn button_label_preserves_whitespace() {
        let tree = rsx! {
            <Button
                label="Click Me"
                variant={Some(ButtonVariant::Contained)}
            />
        };
        let RsxNode::Element(root) = tree else {
            panic!("button should render element root");
        };
        let Some(RsxNode::Element(text_node)) = root.children.first() else {
            panic!("button should have text child");
        };
        let Some(RsxNode::Text(content)) = text_node.children.first() else {
            panic!("text should carry string child");
        };
        assert_eq!(content.content, "Click Me");
    }

    fn collect_text_nodes(node: &RsxNode, out: &mut Vec<String>) {
        match node {
            RsxNode::Text(content) => out.push(content.content.clone()),
            RsxNode::Element(element) => {
                for child in &element.children {
                    collect_text_nodes(child, out);
                }
            }
            RsxNode::Fragment(fragment) => {
                for child in &fragment.children {
                    collect_text_nodes(child, out);
                }
            }
        }
    }

    #[test]
    fn checkbox_renders_label_text_node() {
        let tree = rsx! {
            <Checkbox
                label="Enable"
            />
        };
        let mut texts = Vec::new();
        collect_text_nodes(&tree, &mut texts);
        assert!(
            texts.iter().any(|text| text == "Enable"),
            "checkbox text nodes: {texts:?}"
        );
    }

    #[test]
    fn switch_renders_label_text_node() {
        let tree = rsx! {
            <Switch
                label="Switch state"
            />
        };
        let mut texts = Vec::new();
        collect_text_nodes(&tree, &mut texts);
        assert!(
            texts.iter().any(|text| text == "Switch state"),
            "switch text nodes: {texts:?}"
        );
    }

    fn collect_text_boxes(
        node: &dyn rfgui::view::base_component::ElementTrait,
        out: &mut Vec<(f32, f32)>,
    ) {
        if node.as_any().is::<rfgui::view::base_component::Text>() {
            let snapshot = node.box_model_snapshot();
            out.push((snapshot.width, snapshot.height));
        }
        if let Some(children) = node.children() {
            for child in children {
                collect_text_boxes(child.as_ref(), out);
            }
        }
    }

    #[test]
    fn checkbox_label_has_non_zero_text_layout() {
        let tree = rsx! {
            <Checkbox
                label="Enable"
            />
        };
        let mut roots = rfgui::rsx_to_elements(&tree).expect("convert checkbox");
        let root = roots.get_mut(0).expect("has root");
        root.measure(rfgui::view::base_component::LayoutConstraints {
            max_width: 320.0,
            max_height: 120.0,
            viewport_width: 320.0,
            viewport_height: 120.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(120.0),
        });
        root.place(rfgui::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 120.0,
            viewport_width: 320.0,
            viewport_height: 120.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(120.0),
        });

        let mut boxes = Vec::new();
        collect_text_boxes(root.as_ref(), &mut boxes);
        let max_width = boxes
            .iter()
            .map(|(width, _)| *width)
            .fold(0.0_f32, f32::max);
        assert!(max_width > 20.0, "text boxes: {boxes:?}");
    }

    #[test]
    fn window_supports_children_with_optional_size_props() {
        let tree = rsx! {
            <Window
                title="Panel"
                width=420.0
            >
                <Button
                    label="Inside"
                />
            </Window>
        };
        let RsxNode::Element(root) = tree else {
            panic!("window should render element root");
        };
        assert_eq!(root.tag, "Element");
        assert!(!root.children.is_empty());
    }

    #[test]
    fn create_element_supports_multiple_children() {
        let tree = create_element(
            PhantomData::<Element>,
            ElementPropSchema {
                anchor: None,
                style: None,
                on_mouse_down: None,
                on_mouse_up: None,
                on_mouse_move: None,
                on_mouse_enter: None,
                on_mouse_leave: None,
                on_click: None,
                on_key_down: None,
                on_key_up: None,
                on_focus: None,
                on_blur: None,
            },
            vec![
                create_element(
                    PhantomData::<Text>,
                    TextPropSchema {
                        style: None,
                        align: None,
                        font_size: None,
                        line_height: None,
                        font: None,
                        opacity: None,
                    },
                    "A",
                ),
                create_element(
                    PhantomData::<Text>,
                    TextPropSchema {
                        style: None,
                        align: None,
                        font_size: None,
                        line_height: None,
                        font: None,
                        opacity: None,
                    },
                    "B",
                ),
            ],
        );

        let RsxNode::Element(root) = tree else {
            panic!("create_element should produce an element root");
        };
        assert_eq!(root.tag, "Element");
        assert_eq!(root.children.len(), 2);
    }

    #[test]
    fn window_supports_nested_optional_object_props() {
        let tree = rsx! {
            <Window
                title="Panel"
                window_slots={{
                    root_style: {
                        background: rfgui::Color::hex("#ffffff"),
                    },
                    title_bar_style: {
                        height: rfgui::Length::px(28.0),
                    },
                }}
            >
                <Button label="Inside" />
            </Window>
        };

        let RsxNode::Element(root) = tree else {
            panic!("window should render element root");
        };
        assert_eq!(root.tag, "Element");
        assert!(!root.children.is_empty());
    }

    #[test]
    fn accordion_default_expanded_renders_children() {
        let tree = rsx! {
            <Accordion
                title="Section A"
                default_expanded={Some(true)}
            >
                <Text>"Content A"</Text>
            </Accordion>
        };

        let mut texts = Vec::new();
        collect_text_nodes(&tree, &mut texts);
        assert!(texts.iter().any(|text| text == "Section A"));
        assert!(texts.iter().any(|text| text == "Content A"));
    }

    #[test]
    fn accordion_collapsed_keeps_children_in_tree() {
        let tree = rsx! {
            <Accordion title="Section B">
                <Text>"Content B"</Text>
            </Accordion>
        };

        let mut texts = Vec::new();
        collect_text_nodes(&tree, &mut texts);
        assert!(texts.iter().any(|text| text == "Section B"));
        assert!(texts.iter().any(|text| text == "Content B"));
    }

    #[test]
    fn accordion_click_updates_expanded_binding() {
        let expanded = global_state(|| false);

        let tree = rsx! {
            <Accordion
                title="Section C"
                expanded_binding={Some(expanded.binding())}
            >
                <Text>"Content C"</Text>
            </Accordion>
        };

        let mut roots = rfgui::rsx_to_elements(&tree).expect("convert accordion");
        let mut viewport = rfgui::view::Viewport::new();
        click_once(roots[0].as_mut(), &mut viewport, 10.0, 10.0);

        assert!(expanded.get());
    }
}
