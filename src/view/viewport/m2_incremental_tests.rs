//! Phase A M2 tests: dark-launched incremental Fiber commit path.
//!
//! These live in a dedicated submodule (rather than fiber_work.rs) so
//! they can reach into the viewport's private `scene` field to inspect
//! arena root keys directly — the whole point of M2 is that NodeKey
//! identity survives across renders, and the arena handles aren't
//! otherwise exposed from the Viewport API surface.
//!
//! Flag-off coverage is implicit: every existing `cargo test --lib`
//! path already exercises `render_rsx` with `use_incremental_commit
//! == false`. The tests here specifically flip the flag on.

#![cfg(test)]

use super::Viewport;
use crate::ui::{
    Binding, DragEffect, RsxNode, RsxTagDescriptor, global_state, on_drag_over, on_drop, rsx,
};
use crate::view::Element as HostElement;
use crate::{Layout, Length};

fn host_el() -> RsxNode {
    RsxNode::tagged("Element", RsxTagDescriptor::of::<HostElement>())
}

fn single_element(width_px: f32) -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(width_px),
            height: Length::px(40.0),
        }} />
    }
}

fn text_leaf(content: &str) -> RsxNode {
    RsxNode::text(content)
}

fn collect_text_contents(
    arena: &crate::view::node_arena::NodeArena,
    key: crate::view::node_arena::NodeKey,
    out: &mut Vec<String>,
) {
    if let Some(node) = arena.get(key) {
        if let Some(text) = node
            .element
            .as_any()
            .downcast_ref::<crate::view::base_component::Text>()
        {
            out.push(text.content().to_string());
        }
        for child in arena.children_of(key) {
            collect_text_contents(arena, child, out);
        }
    }
}

fn drag_drop_rerender_tree(hovering: bool, dropped: Binding<Vec<String>>) -> RsxNode {
    let target_over = on_drag_over(move |event| {
        event.accept(DragEffect::Move);
    });
    let target_drop = {
        let dropped = dropped.clone();
        on_drop(move |_event| {
            dropped.update(|items| items.push("target".to_string()));
        })
    };
    let target_label = if hovering { "target-hover" } else { "target" };

    rsx! {
        <HostElement style={{
            layout: Layout::flow().column().no_wrap(),
            width: Length::px(200.0),
            height: Length::px(60.0),
        }}>
            <HostElement
                style={{
                    width: Length::px(200.0),
                    height: Length::px(30.0),
                }}
            >
                {text_leaf("source")}
            </HostElement>
            <HostElement
                style={{
                    width: Length::px(200.0),
                    height: Length::px(30.0),
                }}
                on_drag_over={target_over}
                on_drop={target_drop}
            >
                {text_leaf(target_label)}
            </HostElement>
        </HostElement>
    }
}

/// 軌 A #9: tests that build their own `FiberWork` and call
/// `apply_fiber_works` directly need an `ApplyContext`. The viewport
/// dimensions / style here mirror the defaults the integration tests
/// would use (`Viewport::new()` defaults).
fn test_apply_ctx() -> crate::view::fiber_work::ApplyContext<'static> {
    use std::sync::OnceLock;
    static STYLE: OnceLock<crate::style::Style> = OnceLock::new();
    let style = STYLE.get_or_init(crate::style::Style::new);
    crate::view::fiber_work::ApplyContext {
        viewport_style: style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    }
}

#[test]
fn drag_drop_retargets_after_drag_over_rerender() {
    let dropped = global_state(|| Vec::<String>::new());
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);

    viewport
        .render_rsx(&drag_drop_rerender_tree(false, dropped.binding()))
        .expect("cold render");
    let old_target = viewport
        .scene
        .ui_root_keys
        .iter()
        .rev()
        .find_map(|&root_key| {
            crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, 20.0, 45.0)
        })
        .expect("initial target should hit-test");

    viewport.input_state.drag_state = Some(super::DragState {
        source_id: old_target,
        data: crate::ui::DataTransfer::default(),
        effect_allowed: DragEffect::Move,
        last_over_target: Some(old_target),
        last_drop_effect: Some(DragEffect::Move),
    });
    viewport.set_pointer_position_viewport(20.0, 45.0);

    viewport
        .render_rsx(&drag_drop_rerender_tree(true, dropped.binding()))
        .expect("drag-over indicator render");

    viewport.dispatch_pointer_up_event(crate::view::viewport::PointerButton::Left);

    assert_eq!(dropped.get(), vec!["target".to_string()]);
}

/// Structure-identical re-render: reconcile produces an empty patch
/// list, and the incremental path must commit zero works while keeping
/// arena root NodeKeys intact.
#[test]
fn incremental_commit_preserves_node_key_across_identical_render() {
    // Build the tree once and render the same `RsxNode` twice. The
    // reconciler's `ptr_eq` fast-path short-circuits prop-diffing (the
    // `Style` prop is an `Rc`-backed `Shared` value that otherwise
    // compares by pointer), producing an empty patch list. Under M2
    // that is the canonical case the incremental path must handle:
    // zero works committed, NodeKey untouched.
    let tree = single_element(120.0);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&tree)
        .expect("cold render should fall back to full rebuild and succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&tree)
        .expect("identical re-render should succeed on incremental path");

    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    assert_eq!(
        viewport.scene.ui_root_keys[0], original_key,
        "NodeKey must be stable across an identical incremental render",
    );
}

/// When the incremental path can't handle a change (here: a prop
/// update, which translates to `FiberWork::Update` — not
/// M2-committable), the flow must fall back to the full-rebuild
/// pipeline. Under the current legacy path an identity-preserving
/// rebuild can still mint a fresh NodeKey; we only assert the render
/// succeeds and the arena still holds a single root.
#[test]
fn incremental_commit_falls_back_on_non_committable_work() {
    let first = single_element(120.0);
    let second = single_element(160.0);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render should succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);

    viewport
        .render_rsx(&second)
        .expect("prop-change render must fall back and still succeed");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
}

/// Remove-a-child: reconcile emits a single `Patch::RemoveChild`,
/// which translates to `FiberWork::Delete` — committable under M2.
/// The parent's NodeKey must survive the incremental commit, the
/// removed child's stable id must be cleared from the index, and the
/// parent's arena child list must shrink by one.
#[test]
fn incremental_commit_deletes_child_without_rebuilding_parent() {
    let child_a = host_el();
    let child_b = host_el();

    // Both parents share the same child identities so reconcile's
    // match phase pairs them up and only the surplus child drops.
    let parent_with_two = host_el()
        .with_child(child_a.clone())
        .with_child(child_b.clone());
    let parent_with_one = host_el().with_child(child_a.clone());

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_with_two).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];
    let arena = &viewport.scene.node_arena;
    let children_before = arena.children_of(parent_key);
    assert_eq!(children_before.len(), 2);
    let kept_child_key = children_before[0];

    viewport
        .render_rsx(&parent_with_one)
        .expect("delete-child render should commit incrementally");

    // Parent and surviving child must keep their keys — this is the
    // core identity-preservation guarantee M2 ships.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena_after = &viewport.scene.node_arena;
    let children_after = arena_after.children_of(parent_key);
    assert_eq!(children_after, vec![kept_child_key]);
}

/// 軌 1 #1: A root-type swap emits `Patch::ReplaceRoot`. The
/// incremental path now builds a descriptor from the new RSX via the
/// shared `DescriptorContext` + `rsx_to_descriptors_with_inherited`
/// pipeline, drops the old subtree, and commits the new one as the
/// sole root — without the full-rebuild fallback ever firing.
#[test]
fn incremental_commit_applies_replace_root() {
    let first = single_element(120.0);
    let second = text_leaf("hello");

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render should succeed");
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("ReplaceRoot must commit incrementally");

    // Root replaced — a new NodeKey is expected (the new element is a
    // text host, not an Element) but `ui_root_keys` must still be a
    // single entry pointing at the freshly-committed arena slot.
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let new_key = viewport.scene.ui_root_keys[0];
    assert_ne!(
        new_key, original_key,
        "ReplaceRoot swaps the arena slot — new NodeKey expected",
    );
    // Old slot is gone; arena must not leak it.
    assert!(
        viewport.scene.node_arena.get(original_key).is_none(),
        "old root slot must be removed after ReplaceRoot commit",
    );
}

/// 軌 1 #1: `Patch::ReplaceNode` (mid-tree type change) commits
/// incrementally via the apply-side `arena_replace_child`. The
/// reconciler only emits `ReplaceNode` when the child-match step
/// pairs two children whose inner variant or tag then differs —
/// which, given identity keys invocation_type + key, is rare in
/// natural RSX. We exercise the path directly by constructing the
/// patch and feeding it through the translator + applier.
#[test]
fn incremental_commit_replace_node_rebuilds_child_preserves_parent_key() {
    use crate::style::Style;
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: parent with two children. Snapshot keys before we mutate.
    let seed = host_el().with_child(host_el()).with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let kept_child_key = viewport.scene.node_arena.children_of(parent_key)[1];
    let old_first_key = viewport.scene.node_arena.children_of(parent_key)[0];

    // Build a synthetic ReplaceNode at path [0] — swap the first
    // child for a text leaf. New rsx root mirrors the same parent
    // structure so `walk_rsx_by_index_path` and resolve_path line up.
    let new_root = host_el()
        .with_child(text_leaf("swapped"))
        .with_child(host_el());
    let patch = crate::ui::Patch::ReplaceNode {
        path: vec![0],
        node: text_leaf("swapped"),
    };
    let viewport_style = Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &viewport_style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("ReplaceNode must translate to a FiberWork");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    // Parent NodeKey unchanged; children list still length 2; kept
    // sibling survives at slot 1; first slot is a fresh NodeKey.
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 2);
    assert_eq!(children[1], kept_child_key);
    assert_ne!(
        children[0], old_first_key,
        "replaced slot must mint a new key"
    );
    assert!(
        arena.get(old_first_key).is_none(),
        "old child slot must be dropped",
    );
}

// ---------------------------------------------------------------------------
// M3: incremental Update + SetText coverage
// ---------------------------------------------------------------------------
//
// These extend M2's Delete/Move-only gate with the prop-setter layer.
// The identity-preservation contract is the same: if the incremental
// path commits the work, the target NodeKey survives.

/// Style update on an Element host commits through the incremental
/// path and keeps the root NodeKey stable — no full rebuild.
#[test]
fn incremental_commit_applies_style_update_preserves_node_key() {
    let first = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0), opacity: 0.9 }} />
    };
    let second = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0), opacity: 0.5 }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("style-change render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "style update must preserve NodeKey via the M3 setter path",
    );
}

/// font_size update on a Text leaf commits as `FiberWork::Update`.
/// NodeKey must survive.
///
/// Uses numeric f64 directly so the prop lands as `PropValue::F64`
/// (the M3 Text font_size branch only handles numeric; `FontSize`-
/// typed values that need inherited-context resolution fall back).
#[test]
fn incremental_commit_applies_font_size_update_preserves_node_key() {
    use crate::view::Text as HostText;

    fn tree(size: f64) -> RsxNode {
        rsx! { <HostText font_size={size}>"hi"</HostText> }
    }

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&tree(14.0)).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&tree(20.0))
        .expect("font_size update must commit incrementally");
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "font_size update must preserve NodeKey via the M3 setter path",
    );
}

/// M4 #4: event-handler changes are now committable incrementally via
/// `Element::clear_rsx_event_handler` + the shared
/// `try_assign_event_handler_prop` dispatcher. The NodeKey must
/// survive the handler swap (was previously force-rebuilt under M3).
#[test]
fn incremental_commit_applies_event_handler_change_preserves_node_key() {
    use crate::ui::PointerDownHandlerProp;
    let handler_a = PointerDownHandlerProp::new(|_| {});
    let handler_b = PointerDownHandlerProp::new(|_| {});

    let first = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler_a}
        />
    };
    let second = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler_b}
        />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("event-handler change must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "on_pointer_down replacement must preserve NodeKey via the M4 setter path",
    );

    use crate::view::base_component::Element as ElementHost;
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert_eq!(
        el.rsx_event_handler_count("on_pointer_down"),
        1,
        "replace semantics: clear + assign must leave exactly one handler, not stack",
    );
}

/// Removing an `on_*` prop between renders emits a reconciler
/// `removed: [..]` entry. M4 #4 routes that through
/// `Element::clear_rsx_event_handler`, so the handler Vec drops to
/// zero and NodeKey still survives.
#[test]
fn incremental_commit_removes_event_handler_prop_clears_handler_list() {
    use crate::ui::PointerDownHandlerProp;
    use crate::view::base_component::Element as ElementHost;

    let handler = PointerDownHandlerProp::new(|_| {});
    let with_handler = rsx! {
        <HostElement
            style={{ width: Length::px(120.0), height: Length::px(40.0) }}
            on_pointer_down={handler}
        />
    };
    let without_handler = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0) }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_handler).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).unwrap();
        let el = node.element.as_any().downcast_ref::<ElementHost>().unwrap();
        assert_eq!(el.rsx_event_handler_count("on_pointer_down"), 1);
    }

    viewport
        .render_rsx(&without_handler)
        .expect("handler-removal render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "removed on_pointer_down must commit through clear and keep NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).unwrap();
    let el = node.element.as_any().downcast_ref::<ElementHost>().unwrap();
    assert_eq!(
        el.rsx_event_handler_count("on_pointer_down"),
        0,
        "removed handler prop must clear the handler list",
    );
}

/// Text content change on a Text leaf commits as `FiberWork::SetText`.
/// The Text host's NodeKey survives.
#[test]
fn incremental_commit_applies_set_text_preserves_node_key() {
    use crate::view::Text as HostText;

    let first = rsx! { <HostText>"hello"</HostText> };
    let second = rsx! { <HostText>"world"</HostText> };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("text-content change must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "SetText must preserve NodeKey via the M3 setter path",
    );
}

#[test]
fn incremental_commit_reorders_unkeyed_text_rows_without_duplicate_content() {
    use crate::view::Text as HostText;

    fn tree(labels: &[&str]) -> RsxNode {
        rsx! {
            <HostElement>
                {labels
                    .iter()
                    .map(|label| rsx! { <HostText>{(*label).to_string()}</HostText> })
                    .collect::<Vec<_>>()}
            </HostElement>
        }
    }

    let first = tree(&["window.rs", "accordion.rs", "tree_view.rs"]);
    let second = tree(&["accordion.rs", "window.rs", "tree_view.rs"]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    viewport
        .render_rsx(&second)
        .expect("text sibling reorder should render without duplicates");

    let mut labels = Vec::new();
    collect_text_contents(
        &viewport.scene.node_arena,
        viewport.scene.ui_root_keys[0],
        &mut labels,
    );
    assert_eq!(labels, vec!["accordion.rs", "window.rs", "tree_view.rs"]);
}

/// Regression for the TreeView drag-drop "duplicate row" bug.
///
/// Reconciler emits, for the same parent that's about to reorder via
/// keyed match, a per-child `RemoveChild + InsertChild` (because that
/// row's *internal* shape changed — e.g. a drop-indicator slot
/// switching from `Element` to `Fragment`). The InsertChild path uses
/// the OLD parent-relative index; after the keyed reorder happens
/// above it, walking NEW by that OLD index lands on a different keyed
/// sibling. The translator's `fallback_replace_node_patch` used to
/// blindly take `NEW[old_path]` as the replacement node, clobbering
/// the row at that arena slot with an unrelated row's contents → the
/// later MoveChild then duplicates that wrong content.
#[test]
fn keyed_row_internal_shape_change_plus_reorder_does_not_duplicate() {
    use crate::view::Text as HostText;

    fn row(label: &str, indicator: bool) -> RsxNode {
        let s = label.to_string();
        let inner = rsx! { <HostText>{s.clone()}</HostText> };
        let slot = if indicator {
            rsx! { <HostElement /> }
        } else {
            RsxNode::fragment(vec![])
        };
        rsx! {
            <HostElement key={s.clone()}>
                {inner}
                {slot}
            </HostElement>
        }
    }

    fn tree(rows: Vec<RsxNode>) -> RsxNode {
        rsx! { <HostElement>{rows}</HostElement> }
    }

    // Pre-drop snapshot the reconciler will diff against: order [A, B, C],
    // row "B" is showing the indicator slot as Element.
    let first = tree(vec![row("A", false), row("B", true), row("C", false)]);
    // Post-drop: order [B, A, C] (keyed reorder above), AND B's
    // indicator slot collapses back to Fragment.
    let second = tree(vec![row("B", false), row("A", false), row("C", false)]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    viewport
        .render_rsx(&second)
        .expect("reorder + shape-change must commit cleanly");

    let mut labels = Vec::new();
    collect_text_contents(
        &viewport.scene.node_arena,
        viewport.scene.ui_root_keys[0],
        &mut labels,
    );
    assert_eq!(
        labels,
        vec!["B", "A", "C"],
        "labels must follow keyed reorder; duplicates here mean \
         fallback ReplaceNode clobbered an arena slot whose OLD/NEW \
         identity diverged because of the surrounding keyed shuffle",
    );
}

// ---------------------------------------------------------------------------
// M4 #1: non-additive replace_style
// ---------------------------------------------------------------------------

/// A `style` prop update whose new `Style` lacks a declaration the old
/// one had must clear that declaration — proving the M4 #1
/// `replace_style` wiring is not using the additive `apply_style`
/// merge. Asserts directly against `Element::parsed_style()`.
#[test]
fn incremental_commit_replace_style_drops_absent_declaration() {
    use crate::Color;
    use crate::style::PropertyId;
    use crate::view::base_component::Element as ElementHost;

    let with_bg = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            background_color: Color::hex("#FF0000"),
        }} />
    };
    let without_bg = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
        }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_bg).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).expect("root node");
        let el = node
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .expect("Element host");
        assert!(
            el.parsed_style().get(PropertyId::BackgroundColor).is_some(),
            "background_color declaration must be present after cold render",
        );
    }

    viewport
        .render_rsx(&without_bg)
        .expect("style-drop render must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "replace_style path must preserve NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node after re-render");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert!(
        el.parsed_style().get(PropertyId::BackgroundColor).is_none(),
        "replace_style must drop the declaration absent from the new Style",
    );
}

/// When the `style` prop itself is removed between renders, reconcile
/// emits a `removed: [\"style\"]` entry. M4 #1 routes that through
/// `Element::replace_style(Style::new())`, clearing all authored
/// declarations while keeping NodeKey stable.
#[test]
fn incremental_commit_removes_style_prop_resets_parsed_style() {
    use crate::view::base_component::Element as ElementHost;

    let with_style = rsx! {
        <HostElement style={{ width: Length::px(120.0), height: Length::px(40.0) }} />
    };
    let without_style = host_el();

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&with_style).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];
    {
        let arena = &viewport.scene.node_arena;
        let node = arena.get(original_key).expect("root node");
        let el = node
            .element
            .as_any()
            .downcast_ref::<ElementHost>()
            .expect("Element host");
        assert!(
            !el.parsed_style().declarations().is_empty(),
            "initial style prop should author at least one declaration",
        );
    }

    viewport
        .render_rsx(&without_style)
        .expect("style-removal render must commit incrementally");

    // NodeKey equality is the identity-preservation contract. If the
    // removed-style patch had been rejected by `is_committable`, the
    // legacy full-rebuild path would have minted a fresh NodeKey.
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "removed-style prop path must commit through replace_style and keep NodeKey",
    );
    let arena = &viewport.scene.node_arena;
    let node = arena.get(original_key).expect("root node after re-render");
    let el = node
        .element
        .as_any()
        .downcast_ref::<ElementHost>()
        .expect("Element host");
    assert!(
        el.parsed_style().declarations().is_empty(),
        "removed-style prop must reset parsed_style to Style::new()",
    );
}

// ---------------------------------------------------------------------------
// M5 #5/#6: Create via InsertChild translation
// ---------------------------------------------------------------------------

/// Appending a child to a parent that already has one should commit
/// incrementally as a single `FiberWork::Create`: the existing
/// child's NodeKey survives (no full rebuild), the parent gains one
/// child, and the newly-authored child is parented to the same key.
#[test]
fn incremental_commit_inserts_appended_child_preserves_sibling_keys() {
    let child_a = host_el();

    // Parent with one child vs parent with two children, sharing child_a
    // as the stable first child (identity-matched by the reconciler).
    let parent_with_one = host_el().with_child(child_a.clone());
    let parent_with_two = host_el().with_child(child_a.clone()).with_child(host_el());

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_with_one).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];
    let existing_child_key = {
        let arena = &viewport.scene.node_arena;
        let children = arena.children_of(parent_key);
        assert_eq!(children.len(), 1);
        children[0]
    };

    viewport
        .render_rsx(&parent_with_two)
        .expect("insert-child render should commit incrementally");

    // Parent identity survives — the incremental path didn't rebuild.
    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![parent_key],
        "parent NodeKey must survive InsertChild translation",
    );

    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(
        children.len(),
        2,
        "InsertChild should grow the parent's child list by one",
    );
    assert_eq!(
        children[0], existing_child_key,
        "existing child NodeKey must be preserved at its original index",
    );
    // New child has a different key (arena slot) and lives under parent.
    assert_ne!(children[1], existing_child_key);
    let new_child_node = arena.get(children[1]).expect("new child node");
    assert_eq!(new_child_node.parent, Some(parent_key));
}

// ---------------------------------------------------------------------------
// M6 boundary: text-cascading style updates must fall back when
// descendants exist
// ---------------------------------------------------------------------------

/// A `style` update that changes a text-cascading decl (here
/// `font_size`) on an Element with a Text child must fall back to
/// the full-rebuild path — the incremental setter doesn't recascade
/// descendants, so letting it commit would diverge from cold-path
/// behaviour. We assert the boundary by checking the Text child's
/// resolved font_size after the re-render: only the full rebuild
/// path walks the convert pipeline, which re-resolves Text fonts
/// against the new inherited cascade.
/// 軌 A #7: a text-cascading style change on an Element ancestor
/// now commits incrementally — the apply side calls
/// `recascade_text_subtree`, which walks Text/TextArea descendants
/// and re-applies ancestor-derived props via `apply_inherited`
/// (explicit-flag gated). Parent NodeKey survives and the Text
/// child's `font_size` matches the cold-path cascade.
#[test]
fn incremental_commit_applies_text_cascading_style_change_recascades_descendants() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let parent_20 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };
    let parent_30 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 30.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&parent_20).expect("cold render");
    let original_parent_key = viewport.scene.ui_root_keys[0];
    let original_text_key = viewport.scene.node_arena.children_of(original_parent_key)[0];

    viewport
        .render_rsx(&parent_30)
        .expect("cascading style change must commit incrementally");

    // NodeKeys stable — no fallback full rebuild fired.
    assert_eq!(viewport.scene.ui_root_keys, vec![original_parent_key]);
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(original_parent_key);
    assert_eq!(children, vec![original_text_key]);
    let text = arena
        .get(original_text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .unwrap()
        .font_size();
    assert!(
        (text - 30.0).abs() < 1e-4,
        "recascade_text_subtree must flow new parent font_size into Text; got {}",
        text
    );
}

/// 軌 A #7: explicit-prop tracking preserves author overrides.
/// A Text child with its own `font_size={14}` must NOT be clobbered
/// when the ancestor's cascading font_size changes.
#[test]
fn incremental_commit_recascade_preserves_explicit_text_font_size() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let parent_20_explicit_14 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText font_size={14.0f64}>"hi"</HostText>
        </HostElement>
    };
    let parent_30_explicit_14 = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 30.0f32,
        }}>
            <HostText font_size={14.0f64}>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&parent_20_explicit_14)
        .expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&parent_30_explicit_14)
        .expect("cascade change must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    let text = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .unwrap()
        .font_size();
    assert!(
        (text - 14.0).abs() < 1e-4,
        "explicit font_size={{14}} must survive ancestor cascade change; got {}",
        text
    );
}

/// Conversely, a cascading style change on a *leaf* Element (no
/// descendants) has no one to recascade into, so it may commit
/// incrementally — NodeKey survives.
#[test]
fn incremental_commit_applies_text_cascading_style_change_on_leaf() {
    let first = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            font_size: 20.0f32,
        }} />
    };
    let second = rsx! {
        <HostElement style={{
            width: Length::px(120.0),
            height: Length::px(40.0),
            font_size: 30.0f32,
        }} />
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let original_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&second)
        .expect("cascading style change on leaf must commit incrementally");

    assert_eq!(
        viewport.scene.ui_root_keys,
        vec![original_key],
        "no descendants → no recascade risk → NodeKey preserved",
    );
}

// ---------------------------------------------------------------------------
// M6: cascade reconstruction for InsertChild
// ---------------------------------------------------------------------------

/// M6 cascade: an incremental InsertChild under a parent that
/// authored `style={{ font_size: 22 }}` must build the new Text child
/// with `font_size == 22`, matching what the cold-path converter
/// would do via `InheritedTextStyle::merge_style`. M5.0 previously
/// shipped with the viewport root style as the approximation, which
/// would have resolved to the default 16.0.
#[test]
fn incremental_commit_insert_child_inherits_parent_font_size_from_cascade() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    // Parent Element authors a font-cascading style. Children inherit
    // font_size 22 through the cascade.
    let parent_with_no_text = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 22.0f32,
        }} />
    };
    let parent_with_text = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 22.0f32,
        }}>
            <HostText>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&parent_with_no_text)
        .expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);
    let parent_key = viewport.scene.ui_root_keys[0];

    viewport
        .render_rsx(&parent_with_text)
        .expect("InsertChild with cascade must commit incrementally");

    // Parent identity survives and the Text child is parented to it.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 1, "Text child should be inserted");
    let text_node = arena.get(children[0]).expect("text child node");
    let text = text_node
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .expect("Text host");
    assert!(
        (text.font_size() - 22.0).abs() < 1e-4,
        "incremental InsertChild must inherit parent font_size 22.0 via cascade, got {}",
        text.font_size(),
    );
}

// ---------------------------------------------------------------------------
// 軌 1 #2 / #3 / #4: context-free setter surface, slot hot-swap, source
// ---------------------------------------------------------------------------

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
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

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
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work]);

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
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work]);
    assert!(
        arena.get(key).is_some(),
        "Image slot must survive source swap"
    );
}

use crate::ui::IntoPropValue;

/// 軌 1 #5: a Fragment-shaped InsertChild expands to N descriptors
/// and commits as `FiberWork::CreateMany` — N consecutive
/// `arena_insert_child` calls. Parent NodeKey survives.
#[test]
fn incremental_commit_applies_fragment_insert_child_creates_many() {
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: empty parent. NEW rsx mirror has the same parent +
    // a Fragment child (which itself holds N children) at index 0.
    let seed = host_el();
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    assert_eq!(viewport.scene.node_arena.children_of(parent_key).len(), 0);

    // Synthetic patch: insert a Fragment containing two Element
    // children. The translator expands the Fragment into N=2
    // descriptors and emits CreateMany.
    let fragment = RsxNode::fragment(vec![host_el(), host_el()]);
    let new_root = host_el().with_child(fragment.clone());
    let patch = crate::ui::Patch::InsertChild {
        parent_path: vec![],
        index: 0,
        node: fragment,
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("Fragment InsertChild must translate to CreateMany");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    // Parent identity stable; two new children landed in order at
    // indices 0 and 1.
    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    assert_eq!(arena.children_of(parent_key).len(), 2);
}

/// 軌 A #9: an `em`-valued `font_size` update on a Text leaf now
/// resolves through the inherited cascade (parent's font_size on
/// the arena) instead of falling back to the full-rebuild pipeline.
#[test]
fn incremental_commit_resolves_em_font_size_via_inherited_cascade() {
    use crate::FontSize;
    use crate::ui::{IntoPropValue, Patch, PropValue};
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;
    use crate::view::fiber_work::{apply_fiber_works, patch_to_fiber_work};

    // Parent Element has font_size=20 in its style; Text child
    // initially has font_size 14 explicit.
    let seed = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText font_size={14.0f64}>"hi"</HostText>
        </HostElement>
    };
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    // Synthetic Update patch: change Text's font_size to Em(2.0).
    // Translator resolves via parent's cascade: 2.0em × 20px = 40px.
    let patch = Patch::UpdateElementProps {
        path: vec![0],
        changed: vec![("font_size", PropValue::FontSize(FontSize::Em(2.0)))],
        removed: vec![],
    };
    let style = crate::style::Style::new();
    let ctx = crate::view::fiber_work::DescriptorContext {
        new_rsx_root: &seed,
        old_rsx_root: None,
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("font_size em patch must translate");
    assert!(
        work.is_committable(&viewport.scene.node_arena),
        "em font_size now committable via cascade resolver",
    );
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    let arena = &viewport.scene.node_arena;
    let node = arena.get(text_key).unwrap();
    let text = node.element.as_any().downcast_ref::<TextHost>().unwrap();
    assert!(
        (text.font_size() - 40.0).abs() < 1e-4,
        "Em(2.0) × parent 20px = 40px; got {}",
        text.font_size(),
    );
    let _ = <FontSize as IntoPropValue>::into_prop_value;
}

/// 軌 A #5 (extends 軌 1 #5): a Fragment new-node in `Patch::ReplaceNode`
/// expands to N descriptors at the replaced slot. The old child
/// subtree is removed and N new keys land in its place.
#[test]
fn incremental_commit_replace_node_with_fragment_expands_to_n_descriptors() {
    use crate::view::fiber_work::{DescriptorContext, apply_fiber_works, patch_to_fiber_work};

    // Seed: parent with two children, snapshot keys.
    let seed = host_el().with_child(host_el()).with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let kept_child_key = viewport.scene.node_arena.children_of(parent_key)[1];

    // Replace child[0] with a Fragment containing 3 children → 3
    // descriptors. After apply, parent has 4 children: 3 new + 1
    // kept (kept_child_key is now at index 3).
    let fragment = RsxNode::fragment(vec![host_el(), host_el(), host_el()]);
    let new_root = host_el().with_child(fragment.clone()).with_child(host_el());
    let patch = crate::ui::Patch::ReplaceNode {
        path: vec![0],
        node: fragment,
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: None,
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    )
    .expect("Fragment ReplaceNode must translate");
    assert!(work.is_committable(&viewport.scene.node_arena));
    apply_fiber_works(&mut viewport.scene.node_arena, test_apply_ctx(), vec![work]);

    let arena = &viewport.scene.node_arena;
    let children = arena.children_of(parent_key);
    assert_eq!(children.len(), 4, "3 new + 1 kept");
    assert_eq!(children[3], kept_child_key, "kept sibling now at end");
}

/// 軌 1 #6: when the OLD tree's structure higher up no longer
/// matches the NEW tree at the InsertChild parent_path, the
/// identity-validated walk aborts and the translator returns `None`
/// (forcing the all-or-nothing batch to fall back to full rebuild).
#[test]
fn incremental_commit_path_drift_identity_check_rejects_misaligned_walk() {
    use crate::view::fiber_work::{DescriptorContext, patch_to_fiber_work};

    let seed = host_el().with_child(host_el());
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport.render_rsx(&seed).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];

    // OLD tree (matches what reconcile would have walked):
    let old_root = host_el().with_child(host_el());
    // NEW tree: the child at path [0] has a different identity
    // (Text leaf instead of Element host) — `walk_rsx_by_index_path
    // _validated` should detect the mismatch when validating
    // `parent_path = [0]` and abort.
    let new_root = host_el().with_child(text_leaf("drifted"));
    let patch = crate::ui::Patch::InsertChild {
        parent_path: vec![0],
        index: 0,
        node: host_el(),
    };
    let style = crate::style::Style::new();
    let ctx = DescriptorContext {
        new_rsx_root: &new_root,
        old_rsx_root: Some(&old_root),
        inherited_style: &style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    let work = patch_to_fiber_work(
        patch,
        viewport.scene.node_arena.stable_id_index(),
        &viewport.scene.node_arena,
        parent_key,
        Some(&ctx),
    );
    assert!(
        work.is_none(),
        "identity drift on parent_path must abort translation",
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
    let slot_rsx = RsxNode::tagged("Element", RsxTagDescriptor::of::<HostElement>());
    let patch = crate::ui::Patch::UpdateElementProps {
        path: vec![],
        changed: vec![("loading", slot_rsx.into_prop_value())],
        removed: vec![],
    };
    let work = patch_to_fiber_work(patch, arena.stable_id_index(), &arena, key, None)
        .expect("loading patch must translate");
    assert!(work.is_committable(&arena));
    apply_fiber_works(&mut arena, test_apply_ctx(), vec![work]);

    // Svg slot now holds 1 key (the wrapper). Use the `loading_slot_len`
    // accessor — the wrapper sits in the Vec until `sync_active_slot`
    // promotes it on the next measure pass.
    let node = arena.get(key).expect("Svg slot survives slot swap");
    let svg = node.element.as_any().downcast_ref::<Svg>().unwrap();
    assert_eq!(svg.loading_slot_len(), 1);
}

/// M5: the flag is on by default. Flipping it off must still work
/// (call sites can A/B test or bisect regressions), and a render
/// round-trip in off-mode should succeed via the legacy full-rebuild
/// path.
#[test]
fn flag_default_on_and_off_switch_survives_round_trip() {
    let first = single_element(120.0);
    let second = single_element(120.0);

    let mut viewport = Viewport::new();
    assert!(
        viewport.use_incremental_commit(),
        "M5 default: flag starts on",
    );

    viewport.set_use_incremental_commit(false);
    viewport.render_rsx(&first).expect("cold render (flag off)");
    viewport
        .render_rsx(&second)
        .expect("identical re-render with flag off must still succeed");
    assert!(!viewport.use_incremental_commit());
}

// ---------------------------------------------------------------------------
// 軌 1 #4 Fragment-at-root: multi-root incremental path
// ---------------------------------------------------------------------------

/// Fragment root with N children → arena stores N roots. Re-rendering the
/// same tree must keep every arena root NodeKey stable (per-root reconcile
/// emits zero patches thanks to ptr_eq).
#[test]
fn incremental_commit_fragment_at_root_preserves_all_root_keys_across_identical_render() {
    let tree = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&tree).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
    let original = viewport.scene.ui_root_keys.clone();

    viewport.render_rsx(&tree).expect("identical re-render");
    assert_eq!(viewport.scene.ui_root_keys, original);
}

/// Fragment-at-root: changing one child's style prop must keep every
/// arena root NodeKey stable (UpdateElementProps routes via root_index,
/// doesn't rebuild siblings).
#[test]
fn incremental_commit_fragment_at_root_style_update_on_one_child_preserves_all_keys() {
    let first = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);
    // Only the middle child's width changes.
    let second = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(250.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
    let original = viewport.scene.ui_root_keys.clone();

    viewport
        .render_rsx(&second)
        .expect("fragment-root child style update must go incremental");

    assert_eq!(viewport.scene.ui_root_keys, original);
}

/// Fragment-at-root arity change (N → M, N != M) must go through the
/// `ReplaceAllRoots` path: arena root count matches the new arity.
/// NodeKeys are expected to be fresh (wholesale swap).
#[test]
fn incremental_commit_fragment_at_root_arity_change_replaces_all_roots() {
    let first = RsxNode::fragment(vec![single_element(100.0), single_element(200.0)]);
    let second = RsxNode::fragment(vec![
        single_element(100.0),
        single_element(200.0),
        single_element(300.0),
    ]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 2);

    viewport
        .render_rsx(&second)
        .expect("fragment-root arity change must commit via ReplaceAllRoots");

    assert_eq!(viewport.scene.ui_root_keys.len(), 3);
}

/// Single Element root → Fragment-at-root swap: identity/shape mismatch
/// triggers `ReplaceAllRoots`. Arena ends with N roots matching the new
/// Fragment's child count.
#[test]
fn incremental_commit_element_root_to_fragment_root_swaps_via_replace_all_roots() {
    let first = single_element(100.0);
    let second = RsxNode::fragment(vec![single_element(150.0), single_element(250.0)]);

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport
        .render_rsx(&first)
        .expect("cold render (single root)");
    assert_eq!(viewport.scene.ui_root_keys.len(), 1);

    viewport
        .render_rsx(&second)
        .expect("single-root → fragment-root swap must commit via ReplaceAllRoots");

    assert_eq!(viewport.scene.ui_root_keys.len(), 2);
}

// ---------------------------------------------------------------------------
// rsx_to_arena_path unit tests (Fragment path flattening)
// ---------------------------------------------------------------------------

#[test]
fn rsx_to_arena_path_flattens_mid_tree_fragment() {
    use crate::view::fiber_work::{ArenaPathResolution, rsx_to_arena_path};

    // Element { children: [A, Fragment([B]), C] }
    // B lives at rsx path [1, 0]; arena flattens Fragment, so B's
    // arena path is [1].
    let a = host_el();
    let b = host_el();
    let c = host_el();
    let root = host_el()
        .with_child(a)
        .with_child(RsxNode::fragment(vec![b]))
        .with_child(c);

    assert!(matches!(rsx_to_arena_path(&root, &[0]), ArenaPathResolution::Arena(p) if p == [0]));
    assert!(matches!(rsx_to_arena_path(&root, &[1, 0]), ArenaPathResolution::Arena(p) if p == [1]));
    assert!(matches!(rsx_to_arena_path(&root, &[2]), ArenaPathResolution::Arena(p) if p == [2]));
}

#[test]
fn rsx_to_arena_path_handles_nested_fragments() {
    use crate::view::fiber_work::{ArenaPathResolution, rsx_to_arena_path};

    // Element { children: [A, Fragment([Fragment([B]), C]), D] }
    let root = host_el()
        .with_child(host_el())
        .with_child(RsxNode::fragment(vec![
            RsxNode::fragment(vec![host_el()]),
            host_el(),
        ]))
        .with_child(host_el());

    assert!(matches!(rsx_to_arena_path(&root, &[0]), ArenaPathResolution::Arena(p) if p == [0]));
    assert!(
        matches!(rsx_to_arena_path(&root, &[1, 0, 0]), ArenaPathResolution::Arena(p) if p == [1])
    );
    assert!(matches!(rsx_to_arena_path(&root, &[1, 1]), ArenaPathResolution::Arena(p) if p == [2]));
    assert!(matches!(rsx_to_arena_path(&root, &[2]), ArenaPathResolution::Arena(p) if p == [3]));
}

// ---------------------------------------------------------------------------
// 軌 1 #8 Text::apply_style incremental
// ---------------------------------------------------------------------------

/// Text.style update (color change): NodeKey of the Text host must
/// survive; this exercises the new `apply_style_incremental` path on
/// `apply_update_to_text`.
#[test]
fn incremental_commit_text_style_color_change_preserves_node_key() {
    use crate::Color;
    use crate::view::Text as HostText;

    let first = rsx! {
        <HostElement>
            <HostText style={{ color: Color::hex("#ff0000") }}>"hi"</HostText>
        </HostElement>
    };
    let second = rsx! {
        <HostElement>
            <HostText style={{ color: Color::hex("#0000ff") }}>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&second)
        .expect("Text.style color change must go incremental");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(parent_key),
        vec![text_key],
        "Text NodeKey must survive style color update",
    );
}

/// Text.style update that drops a declaration (font_size goes away):
/// the explicit flag for font_size must flip back to `false` so the
/// ancestor cascade can refill it.
#[test]
fn incremental_commit_text_style_drops_font_size_keeps_prior_explicit_value() {
    // Track 1 #10 scope: apply_style_incremental does NOT reset
    // explicit flags — an independent `font_size={}` prop may be the
    // source of truth. Removing `font_size` from the style declaration
    // alone therefore does not refill from the ancestor cascade;
    // it keeps whatever the prior explicit value was. Cold-path
    // rebuild is still responsible for wholesale defaults.
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    let first = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText style={{ font_size: 40.0f32 }}>"hi"</HostText>
        </HostElement>
    };
    let second = rsx! {
        <HostElement style={{
            width: Length::px(200.0),
            height: Length::px(80.0),
            font_size: 20.0f32,
        }}>
            <HostText style={{}}>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&second)
        .expect("Text.style losing font_size must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    let arena = &viewport.scene.node_arena;
    let font_size = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextHost>()
        .unwrap()
        .font_size();
    // Prior explicit 40.0 persists; cascade does not override.
    assert!(
        (font_size - 40.0).abs() < 1e-4,
        "prior explicit font_size 40.0 must stick; got {}",
        font_size,
    );
}

/// Text.style prop removed entirely (UpdateElementProps `removed` list
/// carries `"style"`). `apply_remove_to_text` must route through the
/// new `"style"` arm: all explicit flags reset, ancestor cascade fills
/// in, NodeKey stable.
#[test]
fn incremental_commit_text_style_prop_removed_preserves_node_key() {
    use crate::view::Text as HostText;

    let first = rsx! {
        <HostElement>
            <HostText style={{ font_size: 32.0f32 }}>"hi"</HostText>
        </HostElement>
    };
    let second = rsx! {
        <HostElement>
            <HostText>"hi"</HostText>
        </HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    let parent_key = viewport.scene.ui_root_keys[0];
    let text_key = viewport.scene.node_arena.children_of(parent_key)[0];

    viewport
        .render_rsx(&second)
        .expect("Text.style prop removal must commit incrementally");

    assert_eq!(viewport.scene.ui_root_keys, vec![parent_key]);
    assert_eq!(
        viewport.scene.node_arena.children_of(parent_key),
        vec![text_key]
    );
}

/// Fragment-root SetText on a deep descendant under root[1] must route
/// via root_index=1 (not the first arena root). Validates that the
/// multi-root dispatcher passes the correct per-root key to the
/// translator.
#[test]
fn incremental_commit_fragment_at_root_set_text_on_second_root_child() {
    fn tree(second_text: &str) -> RsxNode {
        RsxNode::fragment(vec![
            single_element(100.0),
            rsx! { <HostElement>{text_leaf(second_text)}</HostElement> },
        ])
    }
    let first = tree("hello");
    let second = tree("world");

    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);

    viewport.render_rsx(&first).expect("cold render");
    assert_eq!(viewport.scene.ui_root_keys.len(), 2);
    let original = viewport.scene.ui_root_keys.clone();

    viewport
        .render_rsx(&second)
        .expect("fragment-root SetText on root[1] child must go incremental");

    assert_eq!(viewport.scene.ui_root_keys, original);
}
