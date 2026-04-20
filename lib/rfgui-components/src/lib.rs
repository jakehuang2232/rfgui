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
        Accordion, Button, ButtonVariant, Checkbox, CloseIcon, NumberField, Select, Switch,
        Window,
    };
    use rfgui::ui::{
        EventMeta, NodeId, PointerButton as UiPointerButton, PointerEventData, PropValue,
        RsxElementNode, RsxNode, RsxTagDescriptor, TextChangeEvent, UiDirtyState, global_state,
        rsx, take_state_dirty,
    };
    use rfgui::view::base_component::{LayoutConstraints, LayoutPlacement};
    use rfgui::view::{
        Element, Image, NodeArena, NodeKey, Text, TextArea, commit_descriptor_tree,
        rsx_to_descriptors_with_context,
    };

    // ---- Local arena test helpers (Session 3 arena refactor) ----
    //
    // `rfgui` keeps its arena fixtures in `src/view/test_support.rs` gated
    // behind `#[cfg(test)]`, so downstream crates can't reuse them. We
    // re-implement the minimal subset using the public descriptor pipeline
    // (`commit_descriptor_tree`, `rsx_to_descriptors_with_context`).

    fn commit_rsx_tree_into(arena: &mut NodeArena, tree: &RsxNode) -> Vec<NodeKey> {
        let (descs, errors) = rsx_to_descriptors_with_context(
            tree,
            &rfgui::Style::new(),
            0.0,
            0.0,
        );
        assert!(
            errors.is_empty(),
            "commit_rsx_tree: rsx conversion errors: {errors:?}"
        );
        descs
            .into_iter()
            .map(|d| commit_descriptor_tree(arena, None, d))
            .collect()
    }

    fn measure_and_place_root(
        arena: &mut NodeArena,
        root: NodeKey,
        constraints: LayoutConstraints,
        placement: LayoutPlacement,
    ) {
        arena.with_element_taken(root, |el, a| {
            el.measure(constraints, a);
            el.place(placement, a);
        });
    }

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

        let mut arena = NodeArena::new();
        let roots = commit_rsx_tree_into(&mut arena, &tree);
        let root_key = *roots.first().expect("has root");
        measure_and_place_root(
            &mut arena,
            root_key,
            LayoutConstraints {
                max_width: 320.0,
                max_height: 120.0,
                viewport_width: 320.0,
                viewport_height: 120.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
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
            },
        );

        let mut viewport = rfgui::view::Viewport::new();
        let mut control = rfgui::view::ViewportControl::new(&mut viewport);
        let mut click = rfgui::ui::ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: 8.0,
                viewport_y: 8.0,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(UiPointerButton::Left),
                buttons: rfgui::ui::PointerButtons::default(),
                modifiers: rfgui::ui::KeyModifiers::default(),
                pointer_id: 0,
                pointer_type: rfgui::platform::PointerType::Mouse,
                pressure: 0.0,
                timestamp: rfgui::time::Instant::now(),
            },
        };

        let handled = rfgui::view::base_component::dispatch_click_from_hit_test(
            &mut arena,
            root_key,
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
            <Select::<String, String>
                data={vec![
                    String::from("Option A"),
                    String::from("Option B"),
                    String::from("Option C"),
                ]}
                to_label={select_label as fn(&String, usize) -> String}
                value={selected.binding()}
            />
        };

        let mut arena = NodeArena::new();
        let roots = commit_rsx_tree_into(&mut arena, &tree);
        let mut viewport = rfgui::view::Viewport::new();
        click_once(&mut arena, roots[0], &mut viewport, 10.0, 10.0);
        assert_eq!(selected.get(), "Option A");
        assert_ne!(take_state_dirty(), UiDirtyState::NONE);
    }

    #[test]
    fn select_open_state_persists_across_rerender() {
        let selected = global_state(|| String::from("Option A"));

        let build_tree = || {
            rsx! {
                <Select::<String, String>
                    data={vec![
                        String::from("Option A"),
                        String::from("Option B"),
                        String::from("Option C"),
                    ]}
                    to_label={select_label as fn(&String, usize) -> String}
                    value={selected.binding()}
                />
            }
        };

        let first_tree = build_tree();
        let mut arena = NodeArena::new();
        let roots = commit_rsx_tree_into(&mut arena, &first_tree);
        let mut viewport = rfgui::view::Viewport::new();
        click_once(&mut arena, roots[0], &mut viewport, 10.0, 10.0);
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
                <Select::<String, String>
                    data={vec![
                        String::from("Option A"),
                        String::from("Option B"),
                        String::from("Option C"),
                    ]}
                    to_label={select_label as fn(&String, usize) -> String}
                    value={selected.binding()}
                />
            }
        };

        let first_tree = build_tree();
        let mut arena = NodeArena::new();
        let roots = commit_rsx_tree_into(&mut arena, &first_tree);
        let mut viewport = rfgui::view::Viewport::new();
        click_once(&mut arena, roots[0], &mut viewport, 10.0, 10.0);
        assert_ne!(take_state_dirty(), UiDirtyState::NONE);

        let second_tree = build_tree();
        let mut arena = NodeArena::new();
        let roots = commit_rsx_tree_into(&mut arena, &second_tree);
        let root_key = *roots.first().expect("has root");
        measure_and_place_root(
            &mut arena,
            root_key,
            LayoutConstraints {
                max_width: 320.0,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
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
            },
        );

        let menu_key = arena.children_of(root_key)[1];
        let first_option_key = arena.children_of(menu_key)[0];
        let option_snapshot = arena
            .get(first_option_key)
            .expect("first option node")
            .element
            .box_model_snapshot();

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
        arena: &mut NodeArena,
        root_key: NodeKey,
        viewport: &mut rfgui::view::Viewport,
        x: f32,
        y: f32,
    ) {
        measure_and_place_root(
            arena,
            root_key,
            LayoutConstraints {
                max_width: 320.0,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
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
            },
        );

        let mut control = rfgui::view::ViewportControl::new(viewport);
        let mut click = rfgui::ui::ClickEvent {
            meta: EventMeta::new(NodeId::default()),
            pointer: PointerEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                button: Some(UiPointerButton::Left),
                buttons: rfgui::ui::PointerButtons::default(),
                modifiers: rfgui::ui::KeyModifiers::default(),
                pointer_id: 0,
                pointer_type: rfgui::platform::PointerType::Mouse,
                pressure: 0.0,
                timestamp: rfgui::time::Instant::now(),
            },
        };

        let handled = rfgui::view::base_component::dispatch_click_from_hit_test(
            arena,
            root_key,
            &mut click,
            &mut control,
        );
        assert!(handled);
    }

    fn find_first_text(node: &RsxNode) -> Option<&str> {
        match node {
            RsxNode::Text(t) => Some(t.content.as_str()),
            RsxNode::Element(el) => el.children.iter().find_map(find_first_text),
            RsxNode::Fragment(f) => f.children.iter().find_map(find_first_text),
        }
    }

    #[test]
    fn button_label_preserves_whitespace() {
        let tree = rsx! {
            <Button variant={Some(ButtonVariant::Contained)}>
                "Click Me"
            </Button>
        };
        let text = find_first_text(&tree).expect("button should carry text child");
        assert_eq!(text, "Click Me");
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
            arena: &NodeArena,
            keys: &[NodeKey],
            out: &mut std::collections::HashMap<u64, Box<dyn std::any::Any>>,
        ) {
            for &key in keys {
                let (id, snapshot, children) = {
                    let Some(node) = arena.get(key) else { continue };
                    let snap = node.element.snapshot_state();
                    let id = node.element.stable_id();
                    let kids = node.children.clone();
                    (id, snap, kids)
                };
                if let Some(snapshot) = snapshot {
                    out.insert(id, snapshot);
                }
                save_snapshots(arena, &children, out);
            }
        }

        fn restore_snapshots(
            arena: &mut NodeArena,
            keys: &[NodeKey],
            snapshots: &std::collections::HashMap<u64, Box<dyn std::any::Any>>,
        ) {
            for &key in keys {
                let children = {
                    arena.with_element_taken(key, |el, _| {
                        if let Some(snapshot) = snapshots.get(&el.stable_id()) {
                            let _ = el.restore_state(snapshot.as_ref());
                        }
                    });
                    arena
                        .get(key)
                        .map(|n| n.children.clone())
                        .unwrap_or_default()
                };
                restore_snapshots(arena, &children, snapshots);
            }
        }

        fn measure_and_place(
            arena: &mut NodeArena,
            keys: &[NodeKey],
        ) -> Vec<(f32, f32, f32, f32, bool)> {
            let mut out = Vec::new();
            for &key in keys {
                measure_and_place_root(
                    arena,
                    key,
                    LayoutConstraints {
                        max_width: 320.0,
                        max_height: 120.0,
                        viewport_width: 320.0,
                        viewport_height: 120.0,
                        percent_base_width: Some(320.0),
                        percent_base_height: Some(120.0),
                    },
                    LayoutPlacement {
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
                    },
                );
                fn walk(
                    arena: &NodeArena,
                    key: NodeKey,
                    out: &mut Vec<(f32, f32, f32, f32, bool)>,
                ) {
                    let (snap, children) = {
                        let Some(node) = arena.get(key) else { return };
                        (node.element.box_model_snapshot(), node.children.clone())
                    };
                    out.push((
                        snap.x,
                        snap.y,
                        snap.width,
                        snap.height,
                        snap.should_render,
                    ));
                    for child in children {
                        walk(arena, child, out);
                    }
                }
                walk(arena, key, &mut out);
            }
            out
        }

        let mut first_arena = NodeArena::new();
        let first_roots = commit_rsx_tree_into(&mut first_arena, &first);
        let first_boxes = measure_and_place(&mut first_arena, &first_roots);
        let mut snapshots = std::collections::HashMap::<u64, Box<dyn std::any::Any>>::new();
        save_snapshots(&first_arena, &first_roots, &mut snapshots);

        let mut second_arena = NodeArena::new();
        let second_roots = commit_rsx_tree_into(&mut second_arena, &second);
        restore_snapshots(&mut second_arena, &second_roots, &snapshots);
        let second_boxes = measure_and_place(&mut second_arena, &second_roots);

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

    fn collect_text_boxes(arena: &NodeArena, key: NodeKey, out: &mut Vec<(f32, f32)>) {
        let (is_text, snap, children) = {
            let Some(node) = arena.get(key) else { return };
            (
                node.element
                    .as_any()
                    .is::<rfgui::view::base_component::Text>(),
                node.element.box_model_snapshot(),
                node.children.clone(),
            )
        };
        if is_text {
            out.push((snap.width, snap.height));
        }
        for child in children {
            collect_text_boxes(arena, child, out);
        }
    }

    fn collect_layout_boxes(
        arena: &NodeArena,
        key: NodeKey,
        depth: usize,
        out: &mut Vec<(usize, String, f32, f32, f32, f32)>,
    ) {
        let (kind, snap, children) = {
            let Some(node) = arena.get(key) else { return };
            let kind = if node
                .element
                .as_any()
                .is::<rfgui::view::base_component::Text>()
            {
                "Text".to_string()
            } else {
                "Element".to_string()
            };
            (kind, node.element.box_model_snapshot(), node.children.clone())
        };
        out.push((depth, kind, snap.x, snap.y, snap.width, snap.height));
        for child in children {
            collect_layout_boxes(arena, child, depth + 1, out);
        }
    }

    #[test]
    fn checkbox_label_has_non_zero_text_layout() {
        let tree = rsx! {
            <Checkbox
                label="Enable"
            />
        };
        let mut arena = NodeArena::new();
        let roots = commit_rsx_tree_into(&mut arena, &tree);
        let root_key = *roots.first().expect("has root");
        measure_and_place_root(
            &mut arena,
            root_key,
            LayoutConstraints {
                max_width: 320.0,
                max_height: 120.0,
                viewport_width: 320.0,
                viewport_height: 120.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
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
            },
        );

        let mut boxes = Vec::new();
        collect_text_boxes(&arena, root_key, &mut boxes);
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
            meta: EventMeta::new(NodeId::default()),
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
                <Button>"Inside"</Button>
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
        let tree = rsx! {
            <Element>
                <Text>"A"</Text>
                <Text>"B"</Text>
            </Element>
        };
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
                <Button>"Inside"</Button>
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

        let mut arena = NodeArena::new();
        let roots = commit_rsx_tree_into(&mut arena, &tree);
        let mut viewport = rfgui::view::Viewport::new();
        click_once(&mut arena, roots[0], &mut viewport, 10.0, 10.0);

        assert!(expanded.get());
    }

    #[test]
    fn accordion_header_title_grows_and_icon_stays_intrinsic() {
        let tree = rsx! {
            <Accordion title="Button">
                <Text>"Content"</Text>
            </Accordion>
        };

        let mut arena = NodeArena::new();
        let roots = commit_rsx_tree_into(&mut arena, &tree);
        let root_key = *roots.first().expect("root");
        measure_and_place_root(
            &mut arena,
            root_key,
            LayoutConstraints {
                max_width: 420.0,
                max_height: 200.0,
                viewport_width: 420.0,
                viewport_height: 200.0,
                percent_base_width: Some(420.0),
                percent_base_height: Some(200.0),
            },
            LayoutPlacement {
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
            },
        );

        let mut boxes = Vec::new();
        collect_layout_boxes(&arena, root_key, 0, &mut boxes);
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
