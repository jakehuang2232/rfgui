use super::*;

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
            modifiers: rfgui::ui::Modifiers::default(),
            pointer_id: 0,
            pointer_type: rfgui::platform::PointerType::Mouse,
            pressure: 0.0,
            timestamp: rfgui::time::Instant::now(),
        },
        click_count: 1,
    };

    let handled =
        rfgui::view::dispatch_click_from_hit_test(&mut arena, root_key, &mut click, &mut control);
    assert!(handled);
    assert!(checked.get());
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
