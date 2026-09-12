use crate::material_symbol::CloseIcon;
use crate::{
    Accordion, BranchNode, Button, ButtonVariant, Checkbox, LeafNode, NumberField, Select, Switch,
    TreeNode, TreeView, Window,
};
use rfgui::ui::{
    EventMeta, NodeId, PointerButton as UiPointerButton, PointerEventData, PropValue,
    RsxElementNode, RsxNode, RsxTagDescriptor, TextChangeEvent, UiDirtyState, global_state, rsx,
    take_state_dirty,
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
    let (descs, errors) =
        rsx_to_descriptors_with_context(tree, &rfgui::style::Style::new(), 0.0, 0.0);
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
            modifiers: rfgui::ui::Modifiers::default(),
            pointer_id: 0,
            pointer_type: rfgui::platform::PointerType::Mouse,
            pressure: 0.0,
            timestamp: rfgui::time::Instant::now(),
        },
        click_count: 1,
    };

    let handled =
        rfgui::view::dispatch_click_from_hit_test(arena, root_key, &mut click, &mut control);
    assert!(handled);
}

fn find_first_text(node: &RsxNode) -> Option<&str> {
    match node {
        RsxNode::Text(t) => Some(t.content.as_str()),
        RsxNode::Element(el) => el.children.iter().find_map(find_first_text),
        RsxNode::Fragment(f) => f.children.iter().find_map(find_first_text),
        RsxNode::Component(c) => c.children.iter().find_map(find_first_text),
        RsxNode::Provider(p) => find_first_text(&p.child),
    }
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
        RsxNode::Component(component) => {
            for child in &component.children {
                collect_text_nodes(child, out);
            }
        }
        RsxNode::Provider(provider) => collect_text_nodes(&provider.child, out),
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
        RsxNode::Component(component) => component
            .children
            .iter()
            .find_map(|child| find_first_element_by_tag(child, tag)),
        RsxNode::Provider(provider) => find_first_element_by_tag(&provider.child, tag),
        RsxNode::Text(_) => None,
    }
}

// Phase B: `switch_checked_layout_stays_stable_across_forced_rebuild`
// removed. It exercised the now-deleted
// `ElementTrait::{snapshot_state, restore_state}` host-state save/
// restore hack — incremental commit keeps Element instances alive
// across renders, so the rebuild scenario the test simulated no
// longer occurs on the happy path.

fn collect_text_boxes(arena: &NodeArena, key: NodeKey, out: &mut Vec<(f32, f32)>) {
    let (is_text, snap, children) = {
        let Some(node) = arena.get(key) else { return };
        (
            node.element
                .as_any()
                .is::<rfgui::view::base_component::Text>(),
            node.element.box_model_snapshot(),
            node.children().to_vec(),
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
        (
            kind,
            node.element.box_model_snapshot(),
            node.children().to_vec(),
        )
    };
    out.push((depth, kind, snap.x, snap.y, snap.width, snap.height));
    for child in children {
        collect_layout_boxes(arena, child, depth + 1, out);
    }
}

fn find_text_node(arena: &NodeArena, key: NodeKey, content: &str) -> Option<NodeKey> {
    let (matches, children) = {
        let node = arena.get(key)?;
        (
            node.element
                .as_any()
                .downcast_ref::<rfgui::view::base_component::Text>()
                .is_some_and(|text| text.content() == content),
            node.children().to_vec(),
        )
    };
    if matches {
        return Some(key);
    }
    for child in children {
        if let Some(found) = find_text_node(arena, child, content) {
            return Some(found);
        }
    }
    None
}

fn is_ancestor_or_self(arena: &NodeArena, ancestor: NodeKey, mut node: NodeKey) -> bool {
    loop {
        if node == ancestor {
            return true;
        }
        let Some(parent) = arena.parent_of(node) else {
            return false;
        };
        node = parent;
    }
}

fn sample_tree_nodes() -> Vec<TreeNode> {
    vec![
        BranchNode::new("root", "Root")
            .with_children(vec![LeafNode::new("child", "Child").into()])
            .into(),
    ]
}

mod accordion_tests;
mod component_composition_tests;
mod input_tests;
mod select_tests;
mod tree_view_tests;
