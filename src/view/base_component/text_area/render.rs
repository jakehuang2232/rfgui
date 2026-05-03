//! `Renderable` impl for `TextArea`.
//!
//! `TextAreaTextRun`'s Renderable lives in [`super::run`] — it owns the
//! glyph buffer and emits the actual `TextPass`.
//!
//! Render layer order (per design):
//!   Layer 0 — selection background  (P3.5b)
//!   Layer 1 — children (Run glyphs / projection self-render)
//!   Layer 2 — caret                  (P3.5a, this file)

use std::time::Duration;

use crate::style::ColorLike;
use crate::ui::Rect;
use crate::view::base_component::{
    BuildState, DirtyFlags, Renderable, TextAreaSelectionRenderContext, UiBuildContext,
    with_text_area_selection_render_context,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::render_pass::DrawRectPass;
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, RectPassParams, RenderTargetIn,
};

use super::TextArea;
use super::run::TextAreaTextRun;
use crate::view::base_component::Text;

const CARET_BLINK_PERIOD: Duration = Duration::from_millis(1060);
const CARET_BLINK_VISIBLE: Duration = Duration::from_millis(530);
const CARET_WIDTH: f32 = 1.0;

impl TextArea {
    /// Caret is drawn only while focused, blinking at the standard cadence.
    pub(super) fn should_draw_caret(&self) -> bool {
        if !self.is_focused {
            return false;
        }
        let elapsed = self.caret_blink_started_at.elapsed().as_millis();
        let period = CARET_BLINK_PERIOD.as_millis();
        let visible = CARET_BLINK_VISIBLE.as_millis();
        (elapsed % period) < visible
    }

    /// Resolve `cursor_char` to a screen-space `(x, y_top, line_height)`.
    ///
    /// Walks `children` for a `TextAreaTextRun` whose `char_range` covers
    /// the cursor (boundary cases prefer the *following* Run per the caret
    /// boundary rules). Falls back to TextArea's own layout origin when
    /// no Run exists (empty content, no placeholder).
    pub(super) fn caret_screen_position(&self, arena: &NodeArena) -> Option<(f32, f32, f32)> {
        if self.children.is_empty() {
            // No child Run yet — caret pinned to TextArea's own origin.
            return Some((
                self.layout_state.layout_position.x,
                self.layout_state.layout_position.y,
                self.font_size.max(1.0) * self.line_height,
            ));
        }

        // v1-aligned rule: walk children in order, first child whose
        // half-open range contains the cursor wins. This makes boundary
        // positions prefer the following child (cursor == projection.start
        // belongs to that projection), with tail-of-content falling back
        // to the last Run drawn at its end. One child-kind special:
        //   * Run with `has_trailing_newline` AND `cursor == range.end`:
        //     caret jumps to the *next* child's top-left (CSS "after the
        //     newline = start of next visual line"). Mirrors v1's
        //     `Newline && cursor == source_range.end` branch.
        let mut chosen_idx: Option<usize> = None;
        let mut last_run_idx: Option<usize> = None;
        for (idx, child_range) in self.child_char_ranges.iter().enumerate() {
            let &child_key = self.children.get(idx)?;
            let is_run = arena
                .with_element_taken_ref(child_key, |el, _| {
                    el.as_any().is::<TextAreaTextRun>()
                })
                .unwrap_or(false);
            if is_run {
                last_run_idx = Some(idx);
            }
            if chosen_idx.is_none()
                && self.cursor_char >= child_range.start
                && self.cursor_char < child_range.end
            {
                chosen_idx = Some(idx);
                break;
            }
        }
        let idx = chosen_idx.or(last_run_idx)?;
        let &key = self.children.get(idx)?;
        let range = self.child_char_ranges.get(idx)?.clone();
        let line_h = self.font_size.max(1.0) * self.line_height;

        // Run "trailing-newline at end" special case: the cursor is past
        // the `\n`, so the caret belongs to the *following* sibling's
        // top-left. Read the next sibling's layout up front so the
        // closure below isn't double-borrowed.
        let next_sibling_origin = if self.cursor_char == range.end {
            self.children.get(idx + 1).and_then(|&next_key| {
                arena.with_element_taken_ref(next_key, |el, _| {
                    let snap = el.box_model_snapshot();
                    (snap.x, snap.y)
                })
            })
        } else {
            None
        };

        // Branch on host kind without holding a take-borrow on `key`,
        // since the projection branch needs to DFS the same subtree
        // (calling `with_element_taken_ref(key, ...)` recursively would
        // deadlock on the host slot).
        let host_is_run = arena
            .with_element_taken_ref(key, |el, _| el.as_any().is::<TextAreaTextRun>())
            .unwrap_or(false);

        if host_is_run {
            return arena.with_element_taken_ref(key, |el, _| {
                let run = el.as_any().downcast_ref::<TextAreaTextRun>()?;
                if run.has_trailing_newline && self.cursor_char == range.end {
                    if let Some((nx, ny)) = next_sibling_origin {
                        return Some((nx, ny, line_h));
                    }
                    // No next sibling: pin caret to the start of the
                    // following visual line directly under the Run.
                    let x = run.layout_state.layout_position.x;
                    let y = run.layout_state.layout_position.y + run.layout_state.layout_size.height;
                    return Some((x, y, line_h));
                }
                let (x, y_top, lh) = if run.inline_preedit.is_some() {
                    run.preedit_caret_local_position()?
                } else {
                    let start = run.char_range.start;
                    let visible_chars = run.text.chars().count();
                    let local = self.cursor_char.saturating_sub(start).min(visible_chars);
                    run.local_char_to_screen_position(local)?
                };
                let screen_x = run.layout_state.layout_position.x + x;
                let screen_y = run.layout_state.layout_position.y + y_top;
                Some((screen_x, screen_y, lh))
            })?;
        }

        // Projection host: prefer real glyph coordinates from the first
        // text-bearing descendant. For image/icon-only projections, fall
        // back to proportional positioning inside the projection root box.
        let span = range.end.saturating_sub(range.start);
        let local_char = self.projection_caret_local_char(range.start, span);
        if let Some(found) = glyph_caret_in_projection(arena, key, local_char) {
            return Some(found);
        }
        let snap = arena
            .with_element_taken_ref(key, |el, _| el.box_model_snapshot())?;
        let ratio = if span == 0 {
            0.0
        } else {
            (local_char as f32 / span as f32).clamp(0.0, 1.0)
        };
        let x = snap.x + snap.width * ratio;
        let caret_h = line_h.max(1.0);
        let y = snap.y + (snap.height - caret_h).max(0.0) * 0.5;
        Some((x, y, caret_h))
    }

    fn projection_caret_local_char(&self, projection_start: usize, projection_span: usize) -> usize {
        let base = self
            .cursor_char
            .saturating_sub(projection_start)
            .min(projection_span);
        if self.ime_preedit.is_empty() {
            return base;
        }
        base + preedit_cursor_char_offset(self.ime_preedit.as_str(), self.ime_preedit_cursor)
    }

    /// Walk Run children, collect each Run's preedit underline rects, and
    /// translate to screen coords. When the caret sits inside a projection,
    /// TextArea only draws the IME underline overlay inside the projection;
    /// the projection remains responsible for text rendering.
    fn preedit_underline_screen_rects(&self, arena: &NodeArena) -> Vec<Rect> {
        if !self.ime_preedit.is_empty()
            && let Some(rects) = self.projection_preedit_underline_screen_rects(arena)
        {
            return rects;
        }

        let mut out = Vec::new();
        for &child_key in self.children.iter() {
            arena.with_element_taken_ref(child_key, |el, _| {
                let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() else {
                    return;
                };
                let origin_x = run.layout_state.layout_position.x;
                let origin_y = run.layout_state.layout_position.y;
                for r in run.preedit_underline_rects() {
                    out.push(Rect {
                        x: origin_x + r.x,
                        y: origin_y + r.y,
                        width: r.width,
                        height: r.height,
                    });
                }
            });
        }
        out
    }

    fn projection_preedit_underline_screen_rects(&self, arena: &NodeArena) -> Option<Vec<Rect>> {
        let preedit_chars = self.ime_preedit.chars().count();
        if preedit_chars == 0 {
            return None;
        }
        let cursor = self.cursor_char.min(self.content.chars().count());
        for (idx, range) in self.child_char_ranges.iter().enumerate() {
            if cursor < range.start || cursor >= range.end {
                continue;
            }
            let &child_key = self.children.get(idx)?;
            let is_projection = arena
                .with_element_taken_ref(child_key, |el, _| !el.as_any().is::<TextAreaTextRun>())
                .unwrap_or(false);
            if !is_projection {
                return None;
            }

            let local_start = cursor.saturating_sub(range.start);
            let local_end = local_start + preedit_chars;
            if let Some(rects) =
                glyph_selection_rects_in_projection(arena, child_key, local_start, local_end)
            {
                let underlines = rects
                    .into_iter()
                    .map(|rect| Rect {
                        x: rect.x,
                        y: rect.y + rect.height.max(1.0) - 1.0,
                        width: rect.width.max(1.0),
                        height: 1.0,
                    })
                    .collect::<Vec<_>>();
                if !underlines.is_empty() {
                    return Some(underlines);
                }
            }

            let local_caret = self.projection_caret_local_char(
                range.start,
                range.end.saturating_sub(range.start),
            );
            if let Some((x, y, line_h)) =
                glyph_caret_in_projection(arena, child_key, local_caret)
            {
                let width = (self.font_size.max(1.0) * 0.6 * preedit_chars as f32).max(1.0);
                return Some(vec![Rect {
                    x,
                    y: y + line_h.max(1.0) - 1.0,
                    width,
                    height: 1.0,
                }]);
            }
            return None;
        }
        None
    }

    /// Walk Run children whose `char_range` overlaps the selection range.
    /// Projection selections are rendered by the projection's Text children
    /// via `TextAreaSelectionRenderContext` so they appear above projection
    /// backgrounds and below projection text.
    fn selection_screen_rects(&self, arena: &NodeArena) -> Vec<Rect> {
        let Some((sel_start, sel_end)) = self.selection_range_chars() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (idx, &child_key) in self.children.iter().enumerate() {
            let Some(child_range) = self.child_char_ranges.get(idx).cloned() else {
                continue;
            };
            if child_range.end <= sel_start || child_range.start >= sel_end {
                continue;
            }

            let is_run = arena
                .with_element_taken_ref(child_key, |el, _| el.as_any().is::<TextAreaTextRun>())
                .unwrap_or(false);
            if is_run {
                arena.with_element_taken_ref(child_key, |el, _| {
                    let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() else {
                        return;
                    };
                    let cr = run.char_range.clone();
                    let local_start = sel_start.saturating_sub(cr.start);
                    let local_end = sel_end.saturating_sub(cr.start).min(cr.end - cr.start);
                    let origin_x = run.layout_state.layout_position.x;
                    let origin_y = run.layout_state.layout_position.y;
                    for r in run.local_selection_rects(local_start, local_end) {
                        out.push(Rect {
                            x: origin_x + r.x,
                            y: origin_y + r.y,
                            width: r.width,
                            height: r.height,
                        });
                    }
                });
                continue;
            }

        }
        out
    }

    fn projection_selection_context_for_child(
        &self,
        idx: usize,
        child_key: NodeKey,
        arena: &NodeArena,
    ) -> Option<TextAreaSelectionRenderContext> {
        let (sel_start, sel_end) = self.selection_range_chars()?;
        let range = self.child_char_ranges.get(idx)?;
        if range.end <= sel_start || range.start >= sel_end {
            return None;
        }
        let is_projection = arena
            .with_element_taken_ref(child_key, |el, _| !el.as_any().is::<TextAreaTextRun>())
            .unwrap_or(false);
        if !is_projection {
            return None;
        }
        let local_start = sel_start.saturating_sub(range.start);
        let local_end = sel_end
            .saturating_sub(range.start)
            .min(range.end.saturating_sub(range.start));
        if local_start >= local_end {
            return None;
        }
        Some(TextAreaSelectionRenderContext {
            start: local_start,
            end: local_end,
            fill: self.selection_background_color.to_rgba_f32(),
        })
    }
}

impl Renderable for TextArea {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        // Layer 0 — selection background. Drawn under children so glyphs
        // overlay the highlight.
        if let Some(target) = ctx.current_target() {
            let fill = self.selection_background_color.to_rgba_f32();
            for rect in self.selection_screen_rects(arena) {
                let mut sel_pass = DrawRectPass::new(
                    RectPassParams {
                        position: [rect.x, rect.y],
                        size: [rect.width.max(1.0), rect.height.max(1.0)],
                        fill_color: fill,
                        opacity: 1.0,
                        ..Default::default()
                    },
                    DrawRectInput {
                        pass_context: ctx.graphics_pass_context(),
                        ..Default::default()
                    },
                    DrawRectOutput {
                        render_target: target,
                        ..Default::default()
                    },
                );
                sel_pass.set_input(
                    target
                        .handle()
                        .map(RenderTargetIn::with_handle)
                        .unwrap_or_default(),
                );
                graph.add_graphics_pass(sel_pass);
            }
            ctx.set_current_target(target);
        }

        // Layer 1 — walk arena children (Run / projection self-render).
        //
        // TextArea is promotion-aware (Phase 2): a child that ends up in
        // the promoted set goes through `Element::build_promoted_child`,
        // which allocates its own layer target, runs the build into it,
        // and composites the layer back onto TextArea's current target.
        // Non-promoted children render inline directly.
        let child_keys: Vec<NodeKey> = self.children.clone();
        for (idx, child_key) in child_keys.into_iter().enumerate() {
            let selection_context =
                self.projection_selection_context_for_child(idx, child_key, arena);
            let child_promoted = arena
                .get(child_key)
                .map(|n| ctx.is_node_promoted(n.element.stable_id()))
                .unwrap_or(false);
            if child_promoted {
                with_text_area_selection_render_context(selection_context, || {
                    crate::view::base_component::Element::build_promoted_child(
                        graph, arena, &mut ctx, child_key, None,
                    );
                });
                continue;
            }
            let viewport = ctx.viewport();
            let taken_state = ctx.state_clone();
            let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
            let next_ctx = with_text_area_selection_render_context(selection_context, || {
                arena.with_element_taken(child_key, |child, arena| {
                    let ctx_local = ctx_in;
                    let vp = ctx_local.viewport();
                    let next_state = child.build(graph, arena, ctx_local);
                    UiBuildContext::from_parts(vp, next_state)
                })
            });
            if let Some(c) = next_ctx {
                ctx = c;
            }
        }

        // Layer 1.5 — IME preedit underline (above glyphs, below caret).
        if let Some(target) = ctx.current_target() {
            let stroke = self.color.to_rgba_f32();
            for rect in self.preedit_underline_screen_rects(arena) {
                let mut underline_pass = DrawRectPass::new(
                    RectPassParams {
                        position: [rect.x, rect.y],
                        size: [rect.width.max(1.0), rect.height.max(1.0)],
                        fill_color: stroke,
                        opacity: 1.0,
                        ..Default::default()
                    },
                    DrawRectInput {
                        pass_context: ctx.graphics_pass_context(),
                        ..Default::default()
                    },
                    DrawRectOutput {
                        render_target: target,
                        ..Default::default()
                    },
                );
                underline_pass.set_input(
                    target
                        .handle()
                        .map(RenderTargetIn::with_handle)
                        .unwrap_or_default(),
                );
                graph.add_graphics_pass(underline_pass);
            }
            ctx.set_current_target(target);
        }

        // Layer 2 — caret.
        if self.should_draw_caret()
            && let Some((cx, cy, line_height)) = self.caret_screen_position(arena)
            && let Some(target) = ctx.current_target()
        {
            let mut caret_pass = DrawRectPass::new(
                RectPassParams {
                    position: [cx, cy],
                    size: [CARET_WIDTH, line_height.max(1.0)],
                    fill_color: self.color.to_rgba_f32(),
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput {
                    pass_context: ctx.graphics_pass_context(),
                    ..Default::default()
                },
                DrawRectOutput {
                    render_target: target,
                    ..Default::default()
                },
            );
            caret_pass.set_input(
                target
                    .handle()
                    .map(RenderTargetIn::with_handle)
                    .unwrap_or_default(),
            );
            graph.add_graphics_pass(caret_pass);
            ctx.set_current_target(target);
        }

        self.dirty_flags = self.dirty_flags.without(DirtyFlags::PAINT);
        ctx.into_state()
    }
}

/// DFS the projection subtree rooted at `root_key` for the first
/// text-bearing element (a `<Text>` or a `TextAreaTextRun`) and query
/// its glyph buffer for the screen-space caret position at `local_char`
/// (0-based char offset into the projected slice).
fn glyph_caret_in_projection(
    arena: &NodeArena,
    root_key: NodeKey,
    local_char: usize,
) -> Option<(f32, f32, f32)> {
    if let Some(found) = query_caret_on(arena, root_key, local_char) {
        return Some(found);
    }
    let mut stack: Vec<NodeKey> = arena.children_of(root_key).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if let Some(found) = query_caret_on(arena, key, local_char) {
            return Some(found);
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

fn query_caret_on(
    arena: &NodeArena,
    key: NodeKey,
    local_char: usize,
) -> Option<(f32, f32, f32)> {
    arena
        .with_element_taken_ref(key, |el, _| {
            if let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() {
                let visible = run.text.chars().count();
                let local = local_char.min(visible);
                let (x, y_top, lh) = run.local_char_to_screen_position(local)?;
                return Some((
                    run.layout_state.layout_position.x + x,
                    run.layout_state.layout_position.y + y_top,
                    lh,
                ));
            }
            if let Some(text) = el.as_any().downcast_ref::<Text>() {
                let visible = text.content().chars().count();
                let local = local_char.min(visible);
                return text.local_char_to_screen_position(local);
            }
            None
        })
        .flatten()
}

fn glyph_selection_rects_in_projection(
    arena: &NodeArena,
    root_key: NodeKey,
    local_start: usize,
    local_end: usize,
) -> Option<Vec<Rect>> {
    if let Some(found) = query_selection_on(arena, root_key, local_start, local_end) {
        return Some(found);
    }
    let mut stack: Vec<NodeKey> = arena.children_of(root_key).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if let Some(found) = query_selection_on(arena, key, local_start, local_end) {
            return Some(found);
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

fn query_selection_on(
    arena: &NodeArena,
    key: NodeKey,
    local_start: usize,
    local_end: usize,
) -> Option<Vec<Rect>> {
    arena
        .with_element_taken_ref(key, |el, _| {
            if let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() {
                let visible = run.text.chars().count();
                let start = local_start.min(visible);
                let end = local_end.min(visible);
                let origin_x = run.layout_state.layout_position.x;
                let origin_y = run.layout_state.layout_position.y;
                let rects = run
                    .local_selection_rects(start, end)
                    .into_iter()
                    .map(|rect| Rect {
                        x: origin_x + rect.x,
                        y: origin_y + rect.y,
                        width: rect.width,
                        height: rect.height,
                    })
                    .collect();
                return Some(rects);
            }
            if let Some(text) = el.as_any().downcast_ref::<Text>() {
                let visible = text.content().chars().count();
                let start = local_start.min(visible);
                let end = local_end.min(visible);
                return Some(text.local_selection_screen_rects(start, end));
            }
            None
        })
        .flatten()
}

fn preedit_cursor_char_offset(preedit: &str, cursor: Option<(usize, usize)>) -> usize {
    let byte = cursor
        .map(|(_, end)| clamp_utf8_boundary(preedit, end))
        .unwrap_or(preedit.len());
    preedit[..byte].chars().count()
}

fn clamp_utf8_boundary(value: &str, mut byte_index: usize) -> usize {
    byte_index = byte_index.min(value.len());
    while byte_index > 0 && !value.is_char_boundary(byte_index) {
        byte_index -= 1;
    }
    byte_index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Length;
    use crate::ui::{RsxNode, RsxTagDescriptor};
    use crate::view::ElementStylePropSchema;
    use crate::view::base_component::{
        ElementTrait, LayoutConstraints, LayoutPlacement, Text,
    };

    fn projection_fixture(cursor_char: usize, with_text_child: bool) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = "abXYZcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.cursor_char = cursor_char;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..5, |_text_area_node| {
                let style = ElementStylePropSchema {
                    width: Some(Length::px(90.0)),
                    height: Some(Length::px(42.0)),
                    ..Default::default()
                };
                let node = RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<crate::view::tags::Element>())
                    .with_prop("style", style);
                if with_text_child {
                    node.with_child(
                        RsxNode::tagged("Text", RsxTagDescriptor::for_tag::<crate::view::tags::Text>())
                            .with_child(RsxNode::text("XYZ")),
                    )
                } else {
                    node
                }
            });
        }));

        let mut arena = crate::view::test_support::new_test_arena();
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(text_area) as Box<dyn ElementTrait>,
        );
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_self_node_key(root);
        });
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 300.0,
                available_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        (arena, root)
    }

    fn caret_position(arena: &NodeArena, root: NodeKey) -> (f32, f32, f32) {
        arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .caret_screen_position(arena)
                    .expect("caret position")
            })
            .expect("root exists")
    }

    fn projection_key(arena: &NodeArena, root: NodeKey) -> NodeKey {
        arena.children_of(root)[1]
    }

    fn projection_snapshot(
        arena: &NodeArena,
        root: NodeKey,
    ) -> crate::view::base_component::BoxModelSnapshot {
        let projection_key = projection_key(arena, root);
        arena
            .get(projection_key)
            .expect("projection child")
            .element
            .box_model_snapshot()
    }

    fn first_text_descendant(arena: &NodeArena, root: NodeKey) -> NodeKey {
        let mut stack: Vec<NodeKey> = arena.children_of(root).into_iter().rev().collect();
        while let Some(key) = stack.pop() {
            if arena
                .get(key)
                .is_some_and(|node| node.element.as_any().is::<Text>())
            {
                return key;
            }
            for child in arena.children_of(key).into_iter().rev() {
                stack.push(child);
            }
        }
        panic!("expected Text descendant");
    }

    fn snapshot(
        arena: &NodeArena,
        key: NodeKey,
    ) -> crate::view::base_component::BoxModelSnapshot {
        arena
            .get(key)
            .expect("node")
            .element
            .box_model_snapshot()
    }

    #[test]
    fn projection_fallback_caret_start_uses_projection_left_edge() {
        let (arena, root) = projection_fixture(2, false);
        let snap = projection_snapshot(&arena, root);
        let (x, y, height) = caret_position(&arena, root);
        let line_h = 14.0 * 1.25;

        assert!((x - snap.x).abs() < 0.5, "x={x}, snap.x={}", snap.x);
        assert!((height - line_h).abs() < 0.01, "height={height}");
        assert!(
            (y - (snap.y + (snap.height - line_h) * 0.5)).abs() < 0.5,
            "y={y}, snap.y={}, snap.height={}",
            snap.y,
            snap.height
        );
    }

    #[test]
    fn projection_fallback_caret_interpolates_inside_projection() {
        let (arena, root) = projection_fixture(3, false);
        let snap = projection_snapshot(&arena, root);
        let (x, _, height) = caret_position(&arena, root);
        let expected_x = snap.x + snap.width / 3.0;

        assert!((x - expected_x).abs() < 0.5, "x={x}, expected={expected_x}");
        assert!((height - 17.5).abs() < 0.01, "height={height}");
    }

    #[test]
    fn projection_caret_uses_real_text_coordinates_when_available() {
        let (arena, root) = projection_fixture(4, true);
        let (x, y, height) = caret_position(&arena, root);
        let text_key = first_text_descendant(&arena, projection_key(&arena, root));
        let text_snap = snapshot(&arena, text_key);
        let expected = arena
            .with_element_taken_ref(text_key, |el, _| {
                el.as_any()
                    .downcast_ref::<Text>()
                    .expect("Text descendant")
                    .local_char_to_screen_position(2)
                    .expect("text caret position")
            })
            .expect("text key exists");

        assert!((x - expected.0).abs() < 0.5, "x={x}, expected={}", expected.0);
        assert!((y - expected.1).abs() < 0.5, "y={y}, expected={}", expected.1);
        assert!((height - expected.2).abs() < 0.01, "height={height}, expected={}", expected.2);
        assert!(
            x >= text_snap.x - 0.5 && x <= text_snap.x + text_snap.width + 0.5,
            "caret x should be inside text bounds: x={x}, text=({}, {})",
            text_snap.x,
            text_snap.width
        );
        assert!(
            y >= text_snap.y - 0.5 && y <= text_snap.y + text_snap.height + 0.5,
            "caret y should be inside text bounds: y={y}, text=({}, {})",
            text_snap.y,
            text_snap.height
        );
    }

    #[test]
    fn projection_selection_uses_text_rects_instead_of_projection_bounds() {
        let mut text_area = TextArea::new();
        text_area.content = "ab/activity/with/a/very/long/pathcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.select_range(19, 28);
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..34, |_text_area_node| {
                let style = ElementStylePropSchema {
                    width: Some(Length::px(120.0)),
                    height: Some(Length::px(80.0)),
                    ..Default::default()
                };
                RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<crate::view::tags::Element>())
                    .with_prop("style", style)
                    .with_child(
                        RsxNode::tagged("Text", RsxTagDescriptor::for_tag::<crate::view::tags::Text>())
                            .with_child(RsxNode::text("/activity/with/a/very/long/path")),
                    )
            });
        }));

        let mut arena = crate::view::test_support::new_test_arena();
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(text_area) as Box<dyn ElementTrait>,
        );
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_self_node_key(root);
        });
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 300.0,
                available_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );

        let projection_snap = projection_snapshot(&arena, root);
        let root_el = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .projection_selection_context_for_child(1, projection_key(&arena, root), arena)
            })
            .expect("root exists");

        let context = root_el.expect("expected projection selection render context");
        assert_eq!(context.start, 17);
        assert_eq!(context.end, 26);
        let text_key = first_text_descendant(&arena, projection_key(&arena, root));
        let rects = arena
            .with_element_taken_ref(text_key, |el, _| {
                el.as_any()
                    .downcast_ref::<Text>()
                    .expect("projection Text")
                    .local_selection_screen_rects(context.start, context.end)
            })
            .expect("text exists");

        assert!(!rects.is_empty(), "expected projection text selection rects");
        assert!(
            rects.iter().all(|rect| rect.height < projection_snap.height - 1.0),
            "selection should use visual text-line rects, not projection bounds: rects={rects:?}, projection={projection_snap:?}"
        );
        assert!(
            rects.iter().any(|rect| rect.width < projection_snap.width - 1.0),
            "selection should be narrower than the projection union bounds: rects={rects:?}, projection={projection_snap:?}"
        );
    }

    #[test]
    fn projection_preedit_underline_uses_projection_text_rects() {
        let mut text_area = TextArea::new();
        text_area.content = "abXYZcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.cursor_char = 3;
        text_area.ime_preedit = "\u{4E2D}".to_string();
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..5, |_text_area_node| {
                RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<crate::view::tags::Element>())
                    .with_prop(
                        "style",
                        ElementStylePropSchema {
                            width: Some(Length::px(90.0)),
                            height: Some(Length::px(42.0)),
                            ..Default::default()
                        },
                    )
                    .with_child(
                        RsxNode::tagged("Text", RsxTagDescriptor::for_tag::<crate::view::tags::Text>())
                            .with_child(RsxNode::text("XYZ")),
                    )
            });
        }));

        let mut arena = crate::view::test_support::new_test_arena();
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(text_area) as Box<dyn ElementTrait>,
        );
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_self_node_key(root);
        });
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 300.0,
                available_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );

        let projection_snap = projection_snapshot(&arena, root);
        let rects = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .preedit_underline_screen_rects(arena)
            })
            .expect("root exists");

        assert!(!rects.is_empty(), "expected projection IME underline");
        assert!(
            rects.iter().all(|rect| rect.height == 1.0),
            "IME underline should be 1px high: {rects:?}"
        );
        assert!(
            rects.iter().all(|rect| {
                rect.x >= projection_snap.x - 0.5
                    && rect.x + rect.width <= projection_snap.x + projection_snap.width + 0.5
                    && rect.y >= projection_snap.y - 0.5
                    && rect.y <= projection_snap.y + projection_snap.height + 0.5
            }),
            "IME underline should be drawn inside projection bounds: rects={rects:?}, projection={projection_snap:?}"
        );
    }

    #[test]
    fn projection_preedit_caret_follows_preedit_cursor() {
        let (arena_start, root_start) =
            projection_fixture_with_preedit_cursor(Some((0, 0)));
        let (arena_end, root_end) =
            projection_fixture_with_preedit_cursor(Some((3, 3)));

        let (start_x, start_y, _) = caret_position(&arena_start, root_start);
        let (end_x, end_y, _) = caret_position(&arena_end, root_end);

        assert!(
            end_x > start_x + 0.5,
            "preedit caret should move right when IME cursor moves to the end: start_x={start_x}, end_x={end_x}"
        );
        assert!(
            (end_y - start_y).abs() < 0.5,
            "same-line preedit caret should keep y stable: start_y={start_y}, end_y={end_y}"
        );
    }

    fn projection_fixture_with_preedit_cursor(
        preedit_cursor: Option<(usize, usize)>,
    ) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = "abXYZcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.is_focused = true;
        text_area.cursor_char = 3;
        text_area.ime_preedit = "\u{4E2D}".to_string();
        text_area.ime_preedit_cursor = preedit_cursor;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..5, |_text_area_node| {
                RsxNode::tagged("Element", RsxTagDescriptor::for_tag::<crate::view::tags::Element>())
                    .with_prop(
                        "style",
                        ElementStylePropSchema {
                            width: Some(Length::px(90.0)),
                            height: Some(Length::px(42.0)),
                            ..Default::default()
                        },
                    )
                    .with_child(
                        RsxNode::tagged("Text", RsxTagDescriptor::for_tag::<crate::view::tags::Text>())
                            .with_child(RsxNode::text("XYZ")),
                    )
            });
        }));

        let mut arena = crate::view::test_support::new_test_arena();
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(text_area) as Box<dyn ElementTrait>,
        );
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_self_node_key(root);
        });
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 300.0,
                available_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        (arena, root)
    }
}
