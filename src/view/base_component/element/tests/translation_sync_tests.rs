use super::*;
use crate::view::compositor::property_tree::{
    ClipNodeId, ClipNodeRole, EffectNodeId, LayoutPositionNodeId, PropertyTrees, ScrollNodeId,
    TransformNodeId, VisualOffsetNodeId,
};

const VIEWPORT: [f32; 2] = [800.0, 600.0];

/// A viewport-sized host and an absolutely positioned "window": inline text
/// and a scroll container of inline lines, as in a demo window.
struct WindowScene {
    arena: NodeArena,
    host: NodeKey,
    window: NodeKey,
}

impl WindowScene {
    fn new() -> Self {
        let mut arena = new_test_arena();
        let element = |id: u64, style: Style| {
            let mut element = Element::new_with_id(id, 0.0, 0.0, 0.0, 0.0);
            element.apply_style(style);
            Box::new(element)
        };
        let sized = |layout: Layout, width: f32, height: Option<f32>| {
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(layout));
            style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
            style.insert(
                PropertyId::Height,
                height.map_or(ParsedValue::Auto, |h| ParsedValue::Length(Length::px(h))),
            );
            style
        };
        let host = commit_element(
            &mut arena,
            element(
                0x7f00,
                sized(
                    Layout::flow().column().into(),
                    VIEWPORT[0],
                    Some(VIEWPORT[1]),
                ),
            ),
        );
        let window = commit_child(
            &mut arena,
            host,
            element(0x7f01, Self::window_style([0.0; 2])),
        );
        let title = commit_child(
            &mut arena,
            window,
            element(0x7f02, sized(Layout::Inline, 300.0, None)),
        );
        commit_child(&mut arena, title, Box::new(Text::from_content("About")));
        let mut content_style = sized(Layout::flow().column().into(), 300.0, Some(180.0));
        content_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        let content = commit_child(&mut arena, window, element(0x7f03, content_style));
        for line in 0..12_u64 {
            let row = commit_child(
                &mut arena,
                content,
                element(0x7f10 + line, sized(Layout::Inline, 280.0, None)),
            );
            commit_child(
                &mut arena,
                row,
                Box::new(Text::from_content(format!(
                    "Third party license line {line}"
                ))),
            );
        }
        Self {
            arena,
            host,
            window,
        }
    }

    fn window_style(at: [f32; 2]) -> Style {
        let mut style = Style::new();
        style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flow().column().into()),
        );
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(at[0]))
                    .top(Length::px(at[1])),
            ),
        );
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(300.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(220.0)));
        style
    }

    /// Moves the window and lays out one frame, leaving every node clean as
    /// a finished frame does.
    fn move_window(&mut self, at: [f32; 2]) {
        crate::view::test_support::get_element_mut::<Element>(&self.arena, self.window)
            .apply_style(Self::window_style(at));
        measure_and_place(
            &mut self.arena,
            self.host,
            LayoutConstraints {
                max_width: VIEWPORT[0],
                max_height: VIEWPORT[1],
                viewport_width: VIEWPORT[0],
                viewport_height: VIEWPORT[1],
                percent_base_width: Some(VIEWPORT[0]),
                percent_base_height: Some(VIEWPORT[1]),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: VIEWPORT[0],
                available_height: VIEWPORT[1],
                viewport_width: VIEWPORT[0],
                viewport_height: VIEWPORT[1],
                percent_base_width: Some(VIEWPORT[0]),
                percent_base_height: Some(VIEWPORT[1]),
            },
        );
        assert!(
            crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
                &mut self.arena,
                self.host,
                DirtyFlags::ALL,
            )
        );
    }

    fn nodes(&self) -> Vec<NodeKey> {
        let mut out = Vec::new();
        let mut pending = vec![self.host];
        while let Some(key) = pending.pop() {
            out.push(key);
            pending.extend(self.arena.children_of(key));
        }
        out
    }
}

/// Debug text of every property snapshot `key` owns, without generation
/// counters: an incremental sync and a fresh one number their writes
/// differently.
fn property_snapshots(trees: &PropertyTrees, key: NodeKey) -> String {
    let clips = [ClipNodeRole::SelfClip, ClipNodeRole::ContentsClip]
        .map(|role| trees.clip_node_snapshot_for(ClipNodeId { owner: key, role }));
    let text = format!(
        "state={:?} position={:?} visual={:?} transform={:?} effect={:?} scroll={:?} clips={:?}",
        trees.node_state_for(key),
        trees.layout_position_snapshot_for(LayoutPositionNodeId(key)),
        trees.visual_offset_snapshot_for(VisualOffsetNodeId(key)),
        trees.transform_snapshot_for(TransformNodeId(key)),
        trees.effect_node_snapshot_for(EffectNodeId(key)),
        trees.scroll_snapshot_for(ScrollNodeId(key)),
        clips,
    );
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(index) = rest.find("generation: ") {
        let (head, tail) = rest.split_at(index + "generation: ".len());
        out.push_str(head);
        rest = tail.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// Property sync replays a subtree that only translated. Whatever it leaves
/// unobserved must match a fresh sync after every move of the window,
/// including moves across the viewport's top-left edge, where clips and
/// the scrollport's scissor start to clamp.
#[test]
fn incremental_sync_after_moving_a_window_matches_a_fresh_sync() {
    let mut scene = WindowScene::new();
    let mut incremental = PropertyTrees::default();
    let mut translated_offscreen = 0;
    for at in [
        [100.0, 100.0],
        [113.3, 91.3],
        [40.5, 20.25],
        [-60.0, -45.5],
        [-61.25, -47.0],
        [-400.0, -300.0],
        [300.0, 280.0],
    ] {
        let revisions = |arena: &NodeArena, key| {
            (
                arena.mutation_revision(key),
                arena.translation_revision(key),
            )
        };
        let before = scene
            .nodes()
            .into_iter()
            .map(|key| (key, revisions(&scene.arena, key)))
            .collect::<Vec<_>>();
        scene.move_window(at);
        let moved_by_translation = before
            .iter()
            .filter(|(key, (mutation, translation))| {
                let (now_mutation, now_translation) = revisions(&scene.arena, *key);
                now_mutation == *mutation && now_translation != *translation
            })
            .count();
        if at[0] < 0.0 && at[1] < 0.0 {
            translated_offscreen += moved_by_translation;
        }
        incremental.sync(&scene.arena, &[scene.host]);
        let mut fresh = PropertyTrees::default();
        fresh.sync(&scene.arena, &[scene.host]);
        for key in scene.nodes() {
            assert_eq!(
                property_snapshots(&incremental, key),
                property_snapshots(&fresh, key),
                "window at {at:?}: {key:?}"
            );
        }
    }
    assert!(
        translated_offscreen > 0,
        "moving the window past the edge still translates part of its subtree"
    );
}
