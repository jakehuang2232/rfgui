use super::*;

/// A hover `color` and `font_size` reach the descendants that inherit them,
/// Text and TextArea alike, and hand back the base values when hover ends.
/// A Text with its own color keeps it.
#[test]
fn hover_inherited_fields_pass_down_while_hovered() {
    use crate::view::Text as HostText;
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::{Text as TextHost, TextArea as TextAreaHost};

    let base = Color::rgb(0, 0, 255);
    let hover = Color::rgb(255, 0, 0);
    let own = Color::rgb(0, 128, 0);
    let tree = rsx! {
        <HostElement style={{
            width: Length::px(300.0),
            height: Length::px(200.0),
            layout: Layout::flow().column(),
        }}>
            <HostElement style={{
                width: Length::px(300.0),
                height: Length::px(100.0),
                layout: Layout::flow().column(),
                color: base,
                hover: { color: hover, font_size: 30.0f32 },
            }}>
                <HostText>"inherits"</HostText>
                <HostText style={{ color: own }}>"own"</HostText>
                <HostTextArea content={"area".to_string()} />
            </HostElement>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_size(300, 200);
    viewport.render_rsx(&tree).expect("render");
    run_layout_for_test(&mut viewport, 300.0, 200.0);
    let root = viewport.scene.ui_root_keys[0];
    let target = viewport.scene.node_arena.children_of(root)[0];
    let [inherits, own_text, area]: [_; 3] = viewport
        .scene
        .node_arena
        .children_of(target)
        .try_into()
        .expect("three children");
    let read = |viewport: &Viewport| {
        let arena = &viewport.scene.node_arena;
        let text = |key| {
            let text = crate::view::test_support::get_element::<TextHost>(arena, key);
            (text.drawn_color(), text.font_size())
        };
        let area = crate::view::test_support::get_element::<TextAreaHost>(arena, area);
        (
            text(inherits),
            text(own_text).0,
            (area.color, area.font_size),
        )
    };
    let base_values = ((base, 16.0), own, (base, 16.0));
    assert_eq!(read(&viewport), base_values);

    viewport.set_pointer_position_viewport(150.0, 50.0);
    viewport.dispatch_pointer_move_event();
    assert_eq!(read(&viewport), ((hover, 30.0), own, (hover, 30.0)));

    viewport.set_pointer_position_viewport(150.0, 150.0);
    viewport.dispatch_pointer_move_event();
    assert_eq!(read(&viewport), base_values);
}

/// Removing a TextArea's style hands its text fields back to the ancestors.
#[test]
fn text_area_style_removal_falls_back_to_inherited_values() {
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea as TextAreaHost;

    let parent = Color::rgb(0, 0, 255);
    let tree = |styled: bool| {
        let area = if styled {
            rsx! {
                <HostTextArea
                    content={"area".to_string()}
                    style={{ color: Color::rgb(255, 0, 0), font_size: 30.0f32 }}
                />
            }
        } else {
            rsx! { <HostTextArea content={"area".to_string()} /> }
        };
        rsx! {
            <HostElement style={{ color: parent, font_size: 20.0f32 }}>{area}</HostElement>
        }
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&tree(true)).expect("cold render");
    let root = viewport.scene.ui_root_keys[0];
    let area_key = viewport.scene.node_arena.children_of(root)[0];
    viewport.render_rsx(&tree(false)).expect("style removal");

    assert_eq!(viewport.scene.node_arena.children_of(root), vec![area_key]);
    let area = crate::view::test_support::get_element::<TextAreaHost>(
        &viewport.scene.node_arena,
        area_key,
    );
    assert_eq!((area.color, area.font_size), (parent, 20.0));
}
