use super::*;

#[test]
fn checkbox_click_updates_binding() {
    let checked = global_state(|| false);

    let tree = render_pass(|| {
        rsx! {
            <Checkbox
                label="Enable"
                binding={checked.binding()}
            />
        }
    });

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
    assert!(checked.snapshot().get());
}

#[test]
fn checkbox_renders_label_text_node() {
    let tree = render_pass(|| {
        rsx! {
            <Checkbox
                label="Enable"
            />
        }
    });
    let mut texts = Vec::new();
    collect_text_nodes(&tree, &mut texts);
    assert!(
        texts.iter().any(|text| text == "Enable"),
        "checkbox text nodes: {texts:?}"
    );
}

#[test]
fn switch_renders_label_text_node() {
    let tree = render_pass(|| {
        rsx! {
            <Switch
                label="Switch state"
            />
        }
    });
    let mut texts = Vec::new();
    collect_text_nodes(&tree, &mut texts);
    assert!(
        texts.iter().any(|text| text == "Switch state"),
        "switch text nodes: {texts:?}"
    );
}

#[test]
fn checkbox_label_has_non_zero_text_layout() {
    let tree = render_pass(|| {
        rsx! {
            <Checkbox
                label="Enable"
            />
        }
    });
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
    let tree = render_pass(|| {
        rsx! {
            <NumberField binding={value.binding()} />
        }
    });

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

    assert_eq!(value.snapshot().get(), 12.5);
}

#[test]
fn checkbox_distinct_clicks_before_redraw_toggle_committed_value() {
    let checked = rfgui::ui::Binding::new(false);
    let changes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = changes.clone();
    let on_change: std::rc::Rc<dyn Fn(bool)> =
        std::rc::Rc::new(move |value| log.borrow_mut().push(value));
    let tree = render_pass(
        || rsx! { <Checkbox label="toggle" binding={checked.clone()} on_change={on_change}/> },
    );
    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &tree);
    let mut viewport = rfgui::view::Viewport::new();
    click_once(&mut arena, roots[0], &mut viewport, 8.0, 8.0);
    assert!(checked.snapshot().get());
    click_once(&mut arena, roots[0], &mut viewport, 8.0, 8.0);
    assert!(!checked.snapshot().get());
    assert!(!checked.get(), "old render snapshot remains unchanged");
    assert_eq!(*changes.borrow(), vec![true, false]);
}

#[test]
fn number_field_blur_reads_latest_draft_without_an_intervening_render() {
    use rfgui::ui::{Binding, BlurEvent, FocusReason, FromPropValue, batch_state_updates};
    let value = Binding::new(1.0);
    let tree = render_pass(|| rsx! { <NumberField binding={value.clone()} /> });
    let textarea = find_first_element_by_tag(&tree, "TextArea").unwrap();
    let draft = Binding::<String>::from_prop_value(
        textarea
            .props
            .iter()
            .find(|(key, _)| *key == "binding")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    let PropValue::OnChange(change) = &textarea
        .props
        .iter()
        .find(|(key, _)| *key == "on_change")
        .unwrap()
        .1
    else {
        panic!("change handler");
    };
    let PropValue::OnBlur(blur) = &textarea
        .props
        .iter()
        .find(|(key, _)| *key == "on_blur")
        .unwrap()
        .1
    else {
        panic!("blur handler");
    };
    for text in ["12", "12.50"] {
        batch_state_updates(|| {
            draft.set(text.to_owned());
            change.call(&mut TextChangeEvent {
                meta: EventMeta::new(NodeId::default()),
                value: text.to_owned(),
            });
        });
    }
    blur.call(&mut BlurEvent {
        meta: EventMeta::new(NodeId::default()),
        reason: FocusReason::Programmatic,
    });
    assert_eq!(value.snapshot().get(), 12.5);
    assert_eq!(draft.snapshot().get(), "12.5");
    assert_eq!(value.get(), 1.0);
}

#[test]
fn retained_tooltip_ref_observes_committed_visibility() {
    struct Owner;
    let handle =
        rfgui::ui::build_scope(|| rfgui::ui::render_component::<Owner, _>(crate::use_tooltip_ref));
    rfgui::ui::batch_state_updates(|| {
        handle.show();
        assert!(!handle.visible());
    });
    assert!(handle.visible());
    rfgui::ui::batch_state_updates(|| handle.toggle());
    assert!(!handle.visible());
}
