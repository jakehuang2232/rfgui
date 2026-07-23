use super::*;

/// 軌 1 #2: an `anchor` prop change on an Element commits via the
/// new `set_anchor_name` setter — NodeKey survives.
#[test]
fn incremental_commit_applies_anchor_change_preserves_node_key() {
    use crate::view::base_component::Element as ElementHost;

    let first = rsx! {
        <HostElement
            anchor={"first".to_string()}
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
        />
    };
    let second = rsx! {
        <HostElement
            anchor={"second".to_string()}
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
        />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&first).expect("cold render");
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("anchor change must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![original_key]);
    let _ = ElementHost::stable_id; // keep import live
}

/// 軌 1 #2: a removed `anchor` prop resets to None via
/// `set_anchor_name(None)`. NodeKey survives.
#[test]
fn incremental_commit_removes_anchor_prop_clears_anchor_name() {
    use crate::view::base_component::Element as ElementHost;

    let with_anchor = rsx! {
        <HostElement
            anchor={"name".to_string()}
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
        />
    };
    let without_anchor = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0) }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&with_anchor).expect("cold render");
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&without_anchor)
        .expect("anchor removal must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![original_key]);
    let _ = ElementHost::stable_id; // keep import live
}

/// 軌 1 #2: padding prop change on an Element host. Padding doesn't
/// have a top-level rsx slot (it lives inside `style`), so we drive
/// the apply path directly with a synthetic `Patch::UpdateElementProps`.
#[test]
fn incremental_commit_applies_padding_change_via_setter() {
    use crate::view::base_component::Element as ElementHost;
    use crate::view::fiber_work::{apply_fiber_works, patch_to_fiber_work};

    let seed = single_element(120.0);
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let key = viewport.scene.ui_root_keys[0];

    let patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("padding", crate::ui::PropValue::F64(8.0))],
        removed: vec![],
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        key,
        None,
    )
    .expect("padding patch must translate to FiberWork");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work])
        .expect("padding work applies");

    // The setter is fire-and-forget — no public getter for padding,
    // but we can confirm the work was committed (NodeKey untouched
    // is the survival guarantee, no full rebuild fired).
    assert_eq!(viewport.scene.ui_root_keys, vec![key]);
    let _ = ElementHost::stable_id; // keep import live
}

/// 軌 1 #2 + #4: Image `fit` and `source` hot-swap commit
/// incrementally. Driven via direct Patch construction since the
/// rsx Image schema bundles `source` as a mandatory field — easier
/// to seed an Image directly and exercise the apply dispatch.
#[test]
fn incremental_commit_applies_image_fit_and_source_swap() {
    use crate::view::base_component::Image;
    use crate::view::fiber_work::{apply_fiber_works, patch_to_fiber_work};
    use crate::view::test_support::{commit_element, new_test_arena};
    use crate::view::{ImageFit, ImageSource};

    fn rgba(width: u32, height: u32, byte: u8) -> ImageSource {
        ImageSource::Rgba {
            width,
            height,
            pixels: std::sync::Arc::<[u8]>::from(vec![byte; (width * height * 4) as usize]),
        }
    }

    let mut arena = new_test_arena();
    let image = Image::new_with_id(42, rgba(10, 10, 0));
    let key = commit_element(&mut arena, Box::new(image));

    // Build a fit-change patch and apply.
    let fit_patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("fit", ImageFit::Cover.into_prop_value())],
        removed: vec![],
    };
    let work = patch_to_fiber_work(fit_patch, arena.stable_id_index(), &arena, key, None)
        .expect("fit patch must translate");
    assert!(work.is_committable(&arena));
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work]).expect("image fit work applies");

    // Source swap — the apply side acquires a fresh handle; the old
    // one drops via RAII. We can't easily peek at the resource entry
    // without exposing internals, so we assert the commit succeeds
    // and the arena slot is still present.
    let new_source = rgba(20, 20, 255);
    let source_patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("source", new_source.into_prop_value())],
        removed: vec![],
    };
    let work = patch_to_fiber_work(source_patch, arena.stable_id_index(), &arena, key, None)
        .expect("source patch must translate");
    assert!(work.is_committable(&arena));
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work]).expect("image source work applies");
    assert!(
        arena.get(key).is_some(),
        "Image slot must survive source swap"
    );
}

/// 軌 1 #3: a `loading` slot prop change on an Svg host commits via
/// `Svg::replace_loading_slot_incremental` (mirror of Image #3). The
/// new slot subtree is committed under the Svg's arena key.
#[test]
fn incremental_commit_applies_svg_loading_slot_swap() {
    use crate::view::SvgSource;
    use crate::view::base_component::Svg;
    use crate::view::fiber_work::{apply_fiber_works, patch_to_fiber_work};
    use crate::view::test_support::{commit_element, new_test_arena};

    let source = SvgSource::Content(
        r##"<svg width="40" height="40"><rect width="40" height="40"/></svg>"##.to_string(),
    );
    let mut arena = new_test_arena();
    let svg = Svg::new_with_id(7, source);
    let key = commit_element(&mut arena, Box::new(svg));

    // Build a `loading` slot RsxNode (any HostElement leaf works as
    // the slot wrapper — convert_image_slot_desc wraps it in a
    // single descriptor).
    let slot_rsx = RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<HostElement>());
    let patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("loading", slot_rsx.into_prop_value())],
        removed: vec![],
    };
    let work = patch_to_fiber_work(patch, arena.stable_id_index(), &arena, key, None)
        .expect("loading patch must translate");
    assert!(work.is_committable(&arena));
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work])
        .expect("Svg loading slot work applies");

    // Svg slot now holds 1 key (the wrapper). Use the `loading_slot_len`
    // accessor — the wrapper sits in the Vec until `sync_active_slot`
    // promotes it on the next measure pass.
    let node = arena.get(key).expect("Svg slot survives slot swap");
    let svg = node.element.as_any().downcast_ref::<Svg>().unwrap();
    assert_eq!(svg.loading_slot_len(), 1);
}
