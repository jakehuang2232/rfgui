mod icons;
mod inputs;
mod layout;
mod theme;

pub use icons::*;
pub use inputs::*;
pub use layout::*;
pub use theme::*;

#[cfg(test)]
mod tests {
    use crate::{
        Accordion, Button, ButtonHoverStyleSlot, ButtonStyleSlot, ButtonVariant, Checkbox,
        CloseIcon, NumberField, Select, Switch, Window,
    };
    use rfgui::ui::{
        EventMeta, MouseButton as UiMouseButton, MouseEventData, PropValue, RsxElementNode,
        RsxNode, RsxTagDescriptor, TextChangeEvent, UiDirtyState, create_tag_element, global_state,
        rsx, take_state_dirty,
    };
    use rfgui::view::{Element, ElementPropSchema, Image, Text, TextArea, TextPropSchema};
    use rfgui::{
        Border, BorderRadius, Color, Length, Padding, ParsedValue, PropertyId, StyleColor,
    };

    fn select_label(item: &String, _: usize) -> String {
        item.clone()
    }

    fn is_host_tag<T: 'static>(node: &RsxElementNode) -> bool {
        node.tag_descriptor == Some(RsxTagDescriptor::of::<T>())
    }

    fn shared_element_style(node: &RsxElementNode) -> Option<rfgui::view::ElementStylePropSchema> {
        node.props
            .iter()
            .find_map(|(key, value)| match (*key, value) {
                ("style", PropValue::Shared(shared)) => shared
                    .value()
                    .downcast::<rfgui::view::ElementStylePropSchema>()
                    .ok()
                    .map(|style| (*style).clone()),
                _ => None,
            })
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
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(UiMouseButton::Left),
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
        assert_ne!(take_state_dirty(), UiDirtyState::NONE);
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
        assert_ne!(take_state_dirty(), UiDirtyState::NONE);

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

    #[test]
    fn select_menu_option_row_keeps_content_height() {
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
        assert_ne!(take_state_dirty(), UiDirtyState::NONE);

        let second_tree = build_tree();
        let mut roots = rfgui::rsx_to_elements(&second_tree).expect("convert open select");
        let root = roots.get_mut(0).expect("has root");
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

        let root_children = root.children().expect("root children");
        let menu = root_children.get(1).expect("menu child");
        let menu_children = menu.children().expect("menu options");
        let first_option = menu_children.first().expect("first option");
        let option_snapshot = first_option.box_model_snapshot();

        assert!(
            option_snapshot.height < 80.0,
            "expected option row to keep content height, got {}",
            option_snapshot.height
        );
    }

    #[test]
    fn material_symbol_icon_renders_as_typed_element_with_symbol_font() {
        let tree = rsx! { <CloseIcon /> };

        let RsxNode::Element(root) = tree else {
            panic!("icon should render element root");
        };
        assert_eq!(root.tag, "Element");

        let style = shared_element_style(&root).expect("missing icon root style");
        assert_eq!(
            style.font.as_ref().expect("missing icon font").as_slice(),
            &[String::from("Material Symbols Outlined")]
        );
        assert_eq!(
            style.font_size,
            Some(rfgui::FontSize::px(24.0)),
            "icon should default to 24px"
        );

        let text_child = root.children.first().expect("missing text child");
        let RsxNode::Element(text_node) = text_child else {
            panic!("icon child should be text element");
        };
        assert!(is_host_tag::<Text>(text_node));
        assert_eq!(text_node.children.len(), 1);
        match &text_node.children[0] {
            RsxNode::Text(content) => assert_eq!(content.content, "close"),
            other => panic!("expected ligature text child, got {other:?}"),
        }
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
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(UiMouseButton::Left),
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

    #[test]
    fn button_style_slot_overrides_base_and_hover_styles() {
        let tree = rsx! {
            <Button
                label="Styled"
                style={Some(ButtonStyleSlot {
                    color: Some(Color::rgb(1, 2, 3)),
                    background: Some(Color::rgb(4, 5, 6)),
                    padding: Some(
                        Padding::uniform(Length::Zero).xy(Length::px(12.0), Length::px(8.0))
                    ),
                    border: Some(Border::uniform(Length::px(2.0), &Color::rgb(7, 8, 9))),
                    border_radius: Some(BorderRadius::uniform(Length::px(10.0))),
                    hover: Some(ButtonHoverStyleSlot {
                        color: Some(Color::rgb(10, 11, 12)),
                        background: Some(Color::rgb(13, 14, 15)),
                        border: Some(Border::uniform(Length::px(3.0), &Color::rgb(16, 17, 18))),
                    }),
                })}
            />
        };

        let RsxNode::Element(root) = &tree else {
            panic!("button should render element root");
        };
        let root_style = find_style(root);
        assert_eq!(
            root_style.get(PropertyId::BackgroundColor),
            Some(&ParsedValue::Color(StyleColor::Srgb(Color::rgb(4, 5, 6))))
        );
        assert_eq!(
            root_style.get(PropertyId::PaddingTop),
            Some(&ParsedValue::Length(Length::px(8.0)))
        );
        assert_eq!(
            root_style.get(PropertyId::PaddingRight),
            Some(&ParsedValue::Length(Length::px(12.0)))
        );
        assert_eq!(
            root_style.get(PropertyId::BorderTopWidth),
            Some(&ParsedValue::Length(Length::px(2.0)))
        );
        assert_eq!(
            root_style.get(PropertyId::BorderTopColor),
            Some(&ParsedValue::Color(StyleColor::Srgb(Color::rgb(7, 8, 9))))
        );
        assert_eq!(
            root_style.get(PropertyId::BorderTopLeftRadius),
            Some(&ParsedValue::Length(Length::px(10.0)))
        );

        let root_hover = root_style.hover().expect("button root hover style");
        assert_eq!(
            root_hover.get(PropertyId::BackgroundColor),
            Some(&ParsedValue::Color(StyleColor::Srgb(Color::rgb(
                13, 14, 15
            ))))
        );
        assert_eq!(
            root_hover.get(PropertyId::BorderTopWidth),
            Some(&ParsedValue::Length(Length::px(3.0)))
        );
        assert_eq!(
            root_hover.get(PropertyId::BorderTopColor),
            Some(&ParsedValue::Color(StyleColor::Srgb(Color::rgb(
                16, 17, 18
            ))))
        );

        let Some(RsxNode::Element(text)) = root.children.first() else {
            panic!("button should have text child");
        };
        let text_style = find_style(text);
        assert_eq!(
            text_style.get(PropertyId::Color),
            Some(&ParsedValue::Color(StyleColor::Srgb(Color::rgb(1, 2, 3))))
        );
        let text_hover = text_style.hover().expect("button text hover style");
        assert_eq!(
            text_hover.get(PropertyId::Color),
            Some(&ParsedValue::Color(StyleColor::Srgb(Color::rgb(
                10, 11, 12
            ))))
        );
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

    fn find_first_element_by_tag<'a>(node: &'a RsxNode, tag: &str) -> Option<&'a RsxElementNode> {
        match node {
            RsxNode::Element(element) => {
                let matches = match tag {
                    "Element" => is_host_tag::<Element>(element),
                    "Text" => is_host_tag::<Text>(element),
                    "TextArea" => is_host_tag::<TextArea>(element),
                    "Image" => is_host_tag::<Image>(element),
                    _ => element.tag == tag,
                };
                if matches {
                    return Some(element);
                }
                element
                    .children
                    .iter()
                    .find_map(|child| find_first_element_by_tag(child, tag))
            }
            RsxNode::Fragment(fragment) => fragment
                .children
                .iter()
                .find_map(|child| find_first_element_by_tag(child, tag)),
            RsxNode::Text(_) => None,
        }
    }

    fn find_style(node: &RsxElementNode) -> rfgui::Style {
        node.props
            .iter()
            .find_map(|(key, value)| match (key, value) {
                (&"style", PropValue::Shared(shared)) => shared
                    .value()
                    .downcast::<rfgui::view::ElementStylePropSchema>()
                    .ok()
                    .map(|style| style.to_style())
                    .or_else(|| {
                        shared
                            .value()
                            .downcast::<rfgui::view::TextStylePropSchema>()
                            .ok()
                            .map(|style| style.to_style())
                    }),
                _ => None,
            })
            .expect("missing style prop")
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

    #[test]
    fn switch_checked_layout_stays_stable_across_forced_rebuild() {
        let first = rsx! {
            <Switch
                label="Dark mode"
                checked={Some(true)}
            />
        };
        let second = rsx! {
            <Switch
                label="Dark mode"
                checked={Some(true)}
            />
        };

        fn save_snapshots(
            nodes: &[Box<dyn rfgui::view::base_component::ElementTrait>],
            out: &mut std::collections::HashMap<u64, Box<dyn std::any::Any>>,
        ) {
            for node in nodes {
                if let Some(snapshot) = node.snapshot_state() {
                    out.insert(node.id(), snapshot);
                }
                if let Some(children) = node.children() {
                    save_snapshots(children, out);
                }
            }
        }

        fn restore_snapshots(
            nodes: &mut [Box<dyn rfgui::view::base_component::ElementTrait>],
            snapshots: &std::collections::HashMap<u64, Box<dyn std::any::Any>>,
        ) {
            for node in nodes {
                if let Some(snapshot) = snapshots.get(&node.id()) {
                    let _ = node.restore_state(snapshot.as_ref());
                }
                if let Some(children) = node.children_mut() {
                    restore_snapshots(children, snapshots);
                }
            }
        }

        fn measure_and_place(
            nodes: &mut [Box<dyn rfgui::view::base_component::ElementTrait>],
        ) -> Vec<(f32, f32, f32, f32, bool)> {
            let mut out = Vec::new();
            for node in nodes.iter_mut() {
                node.measure(rfgui::view::base_component::LayoutConstraints {
                    max_width: 320.0,
                    max_height: 120.0,
                    viewport_width: 320.0,
                    viewport_height: 120.0,
                    percent_base_width: Some(320.0),
                    percent_base_height: Some(120.0),
                });
                node.place(rfgui::view::base_component::LayoutPlacement {
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
                fn walk(
                    node: &dyn rfgui::view::base_component::ElementTrait,
                    out: &mut Vec<(f32, f32, f32, f32, bool)>,
                ) {
                    let snapshot = node.box_model_snapshot();
                    out.push((
                        snapshot.x,
                        snapshot.y,
                        snapshot.width,
                        snapshot.height,
                        snapshot.should_render,
                    ));
                    if let Some(children) = node.children() {
                        for child in children {
                            walk(child.as_ref(), out);
                        }
                    }
                }
                walk(node.as_ref(), &mut out);
            }
            out
        }

        let mut first_roots = rfgui::rsx_to_elements(&first).expect("convert first switch");
        let first_boxes = measure_and_place(&mut first_roots);
        let mut snapshots = std::collections::HashMap::<u64, Box<dyn std::any::Any>>::new();
        save_snapshots(&first_roots, &mut snapshots);

        let mut second_roots = rfgui::rsx_to_elements(&second).expect("convert rebuilt switch");
        restore_snapshots(&mut second_roots, &snapshots);
        let second_boxes = measure_and_place(&mut second_roots);

        assert_eq!(
            first_boxes, second_boxes,
            "switch boxes changed after rebuild"
        );
        assert!(
            second_boxes.iter().any(|(_, _, width, height, _)| {
                (*width - 44.0).abs() < 0.01 && (*height - 18.0).abs() < 0.01
            }),
            "expected switch track box in {second_boxes:?}"
        );
        let child_twenty_count = second_boxes
            .iter()
            .filter(|(_, _, width, height, _)| {
                (*width - 20.0).abs() < 0.01 && (*height - 14.0).abs() < 0.01
            })
            .count();
        assert!(
            child_twenty_count >= 2,
            "expected checked switch spacer + thumb boxes in {second_boxes:?}"
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

    fn collect_layout_boxes(
        node: &dyn rfgui::view::base_component::ElementTrait,
        depth: usize,
        out: &mut Vec<(usize, String, f32, f32, f32, f32)>,
    ) {
        let snapshot = node.box_model_snapshot();
        let kind = if node.as_any().is::<rfgui::view::base_component::Text>() {
            "Text".to_string()
        } else {
            "Element".to_string()
        };
        out.push((
            depth,
            kind,
            snapshot.x,
            snapshot.y,
            snapshot.width,
            snapshot.height,
        ));
        if let Some(children) = node.children() {
            for child in children {
                collect_layout_boxes(child.as_ref(), depth + 1, out);
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
    fn number_field_textarea_on_change_updates_numeric_binding() {
        let value = global_state(|| 1.0);
        let tree = rsx! {
            <NumberField binding={value.binding()} />
        };

        let textarea = find_first_element_by_tag(&tree, "TextArea").expect("textarea node");
        let Some((_, PropValue::OnChange(handler))) =
            textarea.props.iter().find(|(key, _)| *key == "on_change")
        else {
            panic!("missing on_change prop");
        };

        let mut event = TextChangeEvent {
            meta: EventMeta::new(0),
            value: "12.5".to_string(),
        };
        handler.call(&mut event);

        assert_eq!(value.get(), 12.5);
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
        assert_eq!(root.tag_descriptor, Some(RsxTagDescriptor::of::<Window>()));
        assert!(!root.children.is_empty());
    }

    #[test]
    fn create_element_supports_multiple_children() {
        let tree = create_tag_element::<Element, _, _>(
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
                create_tag_element::<Text, _, _>(
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
                create_tag_element::<Text, _, _>(
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
        assert_eq!(root.tag_descriptor, Some(RsxTagDescriptor::of::<Element>()));
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
        assert_eq!(root.tag_descriptor, Some(RsxTagDescriptor::of::<Window>()));
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

    #[test]
    fn accordion_header_title_grows_and_icon_stays_intrinsic() {
        let tree = rsx! {
            <Accordion title="Button">
                <Text>"Content"</Text>
            </Accordion>
        };

        let mut roots = rfgui::rsx_to_elements(&tree).expect("convert accordion");
        let root = roots.get_mut(0).expect("root");
        root.measure(rfgui::view::base_component::LayoutConstraints {
            max_width: 420.0,
            max_height: 200.0,
            viewport_width: 420.0,
            viewport_height: 200.0,
            percent_base_width: Some(420.0),
            percent_base_height: Some(200.0),
        });
        root.place(rfgui::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 420.0,
            available_height: 200.0,
            viewport_width: 420.0,
            viewport_height: 200.0,
            percent_base_width: Some(420.0),
            percent_base_height: Some(200.0),
        });

        let mut boxes = Vec::new();
        collect_layout_boxes(root.as_ref(), 0, &mut boxes);
        let header = &boxes[1];
        let title = &boxes[2];
        let icon = &boxes[4];

        assert!(header.4 > 400.0, "header should use full width: {boxes:#?}");
        assert!(
            title.4 > 300.0,
            "title should grow to fill remaining width: {boxes:#?}"
        );
        assert!(icon.4 < 40.0, "icon text should stay intrinsic: {boxes:#?}");
        assert!(
            icon.2 > title.2 + title.4 - 1.0,
            "icon should be pushed after title: {boxes:#?}"
        );
    }
}
