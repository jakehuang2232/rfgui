use super::*;
use crate::ui::{Binding, InputType, rsx};
use crate::view::TextArea;

#[test]
fn bound_textarea_keeps_sequential_input_and_ime_across_one_frame() {
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        check_text_input(mode);
    }
}

fn check_text_input(mode: ViewportPaintRendererMode) {
    let binding = Binding::new(String::new());
    let mut viewport = Viewport::new();
    viewport.set_size(320, 100);
    viewport.set_paint_renderer_mode(mode);
    let build = || rsx! { <TextArea binding={binding.clone()} /> };
    viewport.render_rsx(&build()).unwrap();
    let owner = viewport.scene.ui_root_keys[0];
    viewport.set_focused_node_id(Some(owner));
    viewport.sync_focus_dispatch();
    assert!(viewport.dispatch_text_input_event("a".into()));
    assert!(viewport.dispatch_text_input_event("b".into()));
    assert_eq!(binding.snapshot().get(), "ab");
    assert_eq!(binding.get(), "");
    assert!(viewport.dispatch_ime_enabled_event());
    assert!(viewport.dispatch_ime_preedit_event("中".into(), Some((3, 3))));
    assert_eq!(
        binding.snapshot().get(),
        "ab",
        "preedit is not committed text"
    );
    assert!(viewport.dispatch_ime_commit_event("中".into()));
    viewport.dispatch_text_input_event_full("中".into(), InputType::ImeCommit, false);
    assert_eq!(binding.snapshot().get(), "ab中");
    viewport.render_rsx(&build()).unwrap();
    assert_eq!(
        binding.snapshot().get(),
        "ab中",
        "rebuild must not restore an old value"
    );
    crate::ui::batch_state_updates(|| binding.set("external".into()));
    viewport.render_rsx(&build()).unwrap();
    let area = viewport
        .scene
        .node_arena
        .get(viewport.scene.ui_root_keys[0])
        .unwrap();
    assert_eq!(
        area.element
            .as_any()
            .downcast_ref::<crate::view::base_component::TextArea>()
            .unwrap()
            .content,
        "external"
    );
}
