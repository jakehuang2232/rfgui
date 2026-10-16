use super::*;
use crate::platform::ImeCommand;
use crate::platform::input::{Key, Modifiers};
use crate::ui::{BlurEvent, KeyDownEvent, KeyEventData, KeyLocation, TextAreaFocusEvent};
use crate::view::TextArea as HostTextArea;
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<&'static str>>>;

fn press(viewport: &mut Viewport) {
    viewport.dispatch_key_down_event(KeyEventData {
        key: Key::Enter,
        characters: None,
        modifiers: Modifiers::empty(),
        repeat: false,
        is_composing: false,
        location: KeyLocation::Standard,
        timestamp: crate::time::Instant::now(),
    });
}

fn take(log: &Log) -> Vec<&'static str> {
    std::mem::take(&mut *log.borrow_mut())
}

/// A root that logs key events, around an editor that logs focus changes.
fn tree(log: &Log, editor: bool, color: Color) -> RsxNode {
    let (root_key, focus, blur) = (log.clone(), log.clone(), log.clone());
    let editor = editor.then(|| {
        rsx! {
            <HostTextArea
                on_focus={move |_: &mut TextAreaFocusEvent| {
                    focus.borrow_mut().push("editor focus")
                }}
                on_blur={move |_: &mut BlurEvent| blur.borrow_mut().push("editor blur")}
                style={{ width: Length::px(100.0), height: Length::px(30.0), color: color }}
            />
        }
    });
    rsx! {
        <HostElement
            on_key_down={move |_: &mut KeyDownEvent| root_key.borrow_mut().push("root key")}
            style={{ width: Length::px(200.0), height: Length::px(120.0) }}
        >
            {editor.unwrap_or_else(|| RsxNode::fragment(Vec::new()))}
        </HostElement>
    }
}

fn render(viewport: &mut Viewport, tree: RsxNode) {
    viewport.render_rsx(&tree).expect("render");
    run_layout_for_test(viewport, 200.0, 120.0);
}

fn focus_editor(viewport: &mut Viewport) {
    let editor = viewport.scene.node_arena.children_of(root(viewport))[0];
    viewport.set_focused_node_id(Some(editor));
    viewport.sync_focus_dispatch();
}

fn root(viewport: &Viewport) -> crate::view::node_arena::NodeKey {
    viewport.scene.ui_root_keys[0]
}

#[test]
fn key_events_without_focus_go_to_the_first_root() {
    let log = Log::default();
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);
    render(&mut viewport, tree(&log, true, Color::rgb(0, 0, 0)));
    assert_eq!(viewport.keyboard_dispatch_target(), Some(root(&viewport)));
    press(&mut viewport);
    assert_eq!(take(&log), ["root key"]);
    assert_eq!(viewport.focused_node_id(), None);
}

/// As in the HTML focus fixup rule: nothing is focused afterwards, and the
/// removed element gets no blur.
#[test]
fn removing_the_focused_element_leaves_nothing_focused() {
    let log = Log::default();
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);
    render(&mut viewport, tree(&log, true, Color::rgb(0, 0, 0)));
    focus_editor(&mut viewport);
    assert_eq!(take(&log), ["editor focus"]);
    viewport.drain_platform_requests();

    render(&mut viewport, tree(&log, false, Color::rgb(0, 0, 0)));
    assert_eq!(viewport.focused_node_id(), None);
    assert_eq!(viewport.keyboard_dispatch_target(), Some(root(&viewport)));
    // The blur that would have turned off the editor's IME never comes.
    let requests = viewport.drain_platform_requests();
    assert!(matches!(requests.ime_commands[..], [ImeCommand::Disable]));
    press(&mut viewport);
    assert_eq!(take(&log), ["root key"]);
}

/// A rebuild that gives the same element a new node keeps it focused; its
/// new instance starts unfocused, so it gets a focus event.
#[test]
fn a_rebuilt_focused_element_keeps_focus() {
    let log = Log::default();
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);
    viewport.set_use_incremental_commit(false);
    render(&mut viewport, tree(&log, true, Color::rgb(0, 0, 0)));
    focus_editor(&mut viewport);
    let before = viewport.focused_node_id();
    take(&log);

    render(&mut viewport, tree(&log, true, Color::rgb(255, 0, 0)));
    let after = viewport.focused_node_id();
    assert!(
        after.is_some() && after != before,
        "the editor got a new node"
    );
    assert_eq!(take(&log), ["editor focus"]);
    assert_eq!(viewport.keyboard_dispatch_target(), after);
}
