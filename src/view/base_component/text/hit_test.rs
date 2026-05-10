//! Text hit-test + caret + selection-rect computation.

use std::cell::RefCell;

use crate::ui::Rect;
use crate::view::base_component::Position;

use super::Text;

// ---------------------------------------------------------------------------
// TextArea selection render context (thread-local)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct TextAreaSelectionRenderContext {
    pub start: usize,
    pub end: usize,
    pub fill: [f32; 4],
}

thread_local! {
    static TEXT_AREA_SELECTION_RENDER_CONTEXT: RefCell<Option<TextAreaSelectionRenderContext>> =
        const { RefCell::new(None) };
}

pub(crate) fn with_text_area_selection_render_context<R>(
    context: Option<TextAreaSelectionRenderContext>,
    f: impl FnOnce() -> R,
) -> R {
    TEXT_AREA_SELECTION_RENDER_CONTEXT.with(|slot| {
        let previous = slot.replace(context);
        let out = f();
        slot.replace(previous);
        out
    })
}

pub(super) fn current_text_area_selection_render_context() -> Option<TextAreaSelectionRenderContext>
{
    TEXT_AREA_SELECTION_RENDER_CONTEXT.with(|slot| slot.borrow().clone())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TextCaretStop {
    pub(crate) local_char: usize,
    pub(crate) x: f32,
    pub(crate) y_top: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct TextCaretLine {
    pub(crate) y_top: f32,
    pub(crate) y_bottom: f32,
    pub(crate) stops: Vec<TextCaretStop>,
}

// ---------------------------------------------------------------------------
// Text impl — caret / selection-rect / hit-test
// ---------------------------------------------------------------------------

impl Text {
    /// Translate a `local_char` index into `self.content` to a screen-space
    /// `(x, y_top, line_height)`. Used by `TextArea` to query precise caret
    /// coordinates inside a projection that wraps a `<Text>`.
    ///
    /// Returned `(x, y)` are absolute screen coordinates (already
    /// translated by `self.layout_position`).
    pub fn local_char_to_screen_position(&self, local_char: usize) -> Option<(f32, f32, f32)> {
        self.local_char_to_screen_position_with_affinity(local_char, false)
    }

    pub fn local_char_to_screen_position_with_affinity(
        &self,
        local_char: usize,
        upstream: bool,
    ) -> Option<(f32, f32, f32)> {
        let total_chars = self.content.chars().count();
        let target_char = local_char.min(total_chars);

        // Inline path: walk inline_plan fragments accumulating char
        // counts; the fragment hosting `target_char` provides the layout
        // and a screen position.
        if let Some(plan) = self.inline_plan.as_ref()
            && !plan.runs.is_empty()
        {
            let mut consumed_chars = 0_usize;
            let mut last_fragment_end_screen: Option<(f32, f32, f32)> = None;
            for fragment in plan.runs.iter() {
                let frag_chars = fragment.content.chars().count();
                let frag_origin = fragment.position.unwrap_or(Position { x: 0.0, y: 0.0 });
                let in_fragment_char = target_char.saturating_sub(consumed_chars);
                if in_fragment_char <= frag_chars {
                    let local_byte = fragment
                        .content
                        .char_indices()
                        .nth(in_fragment_char)
                        .map(|(b, _)| b)
                        .unwrap_or(fragment.content.len());
                    if let Some(layout) = fragment.text_layout.as_ref() {
                        let geom = layout.cursor_geometry(local_byte, upstream);
                        return Some((frag_origin.x + geom.x, frag_origin.y + geom.y, geom.height));
                    }
                    return None;
                }
                consumed_chars += frag_chars;
                let line_height = fragment
                    .text_layout
                    .as_ref()
                    .and_then(|layout| layout.visual_line_heads().first().copied())
                    .map(|head| head.height)
                    .unwrap_or(fragment.height.max(1.0));
                last_fragment_end_screen = Some((
                    frag_origin.x + fragment.width.max(0.0),
                    frag_origin.y,
                    line_height,
                ));
            }
            return last_fragment_end_screen;
        }

        // Block / non-inline path.
        if let Some(layout) = self.text_layout.as_ref() {
            let geom = layout.caret_geometry_for_char_with_affinity(
                self.content.as_str(),
                target_char,
                upstream,
            );
            return Some((
                self.layout_state.layout_position.x + geom.x,
                self.layout_state.layout_position.y + geom.y,
                geom.height,
            ));
        }
        None
    }

    pub(crate) fn visual_caret_screen_lines(&self) -> Vec<TextCaretLine> {
        if let Some(plan) = self.inline_plan.as_ref()
            && !plan.runs.is_empty()
        {
            let mut out = Vec::new();
            let mut consumed_chars = 0_usize;
            for fragment in plan.runs.iter() {
                let frag_chars = fragment.content.chars().count();
                let origin = fragment.position.unwrap_or(Position { x: 0.0, y: 0.0 });
                if let Some(layout) = fragment.text_layout.as_ref() {
                    for line in layout.visual_caret_lines(fragment.content.as_str()) {
                        out.push(TextCaretLine {
                            y_top: origin.y + line.y_top,
                            y_bottom: origin.y + line.y_bottom,
                            stops: line
                                .stops
                                .into_iter()
                                .map(|stop| TextCaretStop {
                                    local_char: consumed_chars + stop.char_index,
                                    x: origin.x + stop.x,
                                    y_top: origin.y + stop.y_top,
                                    height: stop.height,
                                })
                                .collect(),
                        });
                    }
                }
                consumed_chars += frag_chars;
            }
            for idx in 1..out.len() {
                let Some(prev_tail) = out[idx - 1].stops.last().copied() else {
                    continue;
                };
                let next_stops = out[idx].stops.iter().take(2).copied().collect::<Vec<_>>();
                for next_stop in next_stops {
                    if out[idx - 1]
                        .stops
                        .iter()
                        .any(|stop| stop.local_char == next_stop.local_char)
                    {
                        continue;
                    }
                    out[idx - 1].stops.push(TextCaretStop {
                        local_char: next_stop.local_char,
                        x: prev_tail.x,
                        y_top: prev_tail.y_top,
                        height: prev_tail.height,
                    });
                }
            }
            return out;
        }

        if let Some(layout) = self.text_layout.as_ref() {
            let origin = self.layout_state.layout_position;
            return layout
                .visual_caret_lines(self.content.as_str())
                .into_iter()
                .map(|line| TextCaretLine {
                    y_top: origin.y + line.y_top,
                    y_bottom: origin.y + line.y_bottom,
                    stops: line
                        .stops
                        .into_iter()
                        .map(|stop| TextCaretStop {
                            local_char: stop.char_index,
                            x: origin.x + stop.x,
                            y_top: origin.y + stop.y_top,
                            height: stop.height,
                        })
                        .collect(),
                })
                .collect();
        }

        Vec::new()
    }

    /// Screen-space leading-edge positions of every visual line, in y
    /// order. Each entry is `(x, y_top, line_height)` for the caret
    /// position **before** the first glyph on that line.
    ///
    /// Pure geometry — knows nothing about caret affinity. Exists
    /// because `local_char_to_screen_position` cannot reach a
    /// non-leading fragment's leading edge (its `<=` match captures
    /// the boundary source char as the previous fragment's tail), so
    /// `TextArea` calls this to render the lower-line head when its
    /// affinity logic decides the caret belongs to the new visual line.
    pub fn visual_line_heads(&self) -> Vec<(f32, f32, f32)> {
        if let Some(plan) = self.inline_plan.as_ref()
            && !plan.runs.is_empty()
        {
            return plan
                .runs
                .iter()
                .flat_map(|fragment| {
                    let origin = fragment.position.unwrap_or(Position { x: 0.0, y: 0.0 });
                    if let Some(layout) = fragment.text_layout.as_ref() {
                        return layout
                            .visual_line_heads()
                            .into_iter()
                            .map(move |head| (origin.x + head.x, origin.y + head.y, head.height))
                            .collect::<Vec<_>>();
                    }
                    Vec::new()
                })
                .collect();
        }
        if let Some(layout) = self.text_layout.as_ref() {
            return layout
                .visual_line_heads()
                .into_iter()
                .map(|head| {
                    (
                        self.layout_state.layout_position.x + head.x,
                        self.layout_state.layout_position.y + head.y,
                        head.height,
                    )
                })
                .collect();
        }
        Vec::new()
    }

    /// Translate a local char selection range into screen-space selection
    /// rects. Inline-laid-out text returns one or more rects scoped to the
    /// actual inline fragments instead of the union bounds.
    pub fn local_selection_screen_rects(&self, local_start: usize, local_end: usize) -> Vec<Rect> {
        let start_char = local_start.min(local_end);
        let end_char = local_start.max(local_end);
        if start_char == end_char {
            return Vec::new();
        }

        if let Some(plan) = self.inline_plan.as_ref()
            && !plan.runs.is_empty()
        {
            let mut out = Vec::new();
            let mut consumed_chars = 0_usize;
            for fragment in plan.runs.iter() {
                let frag_chars = fragment.content.chars().count();
                let frag_start = consumed_chars;
                let frag_end = consumed_chars + frag_chars;
                consumed_chars = frag_end;

                if frag_end <= start_char || frag_start >= end_char {
                    continue;
                }
                let origin = fragment.position.unwrap_or(Position { x: 0.0, y: 0.0 });
                let fragment_start = start_char.saturating_sub(frag_start);
                let fragment_end = end_char.saturating_sub(frag_start).min(frag_chars);
                if let Some(layout) = fragment.text_layout.as_ref() {
                    for rect in layout.selection_rects(
                        fragment.content.as_str(),
                        fragment_start,
                        fragment_end,
                    ) {
                        out.push(Rect {
                            x: origin.x + rect.x,
                            y: origin.y + rect.y,
                            width: rect.width,
                            height: rect.height,
                        });
                    }
                    continue;
                }
            }
            return out;
        }

        let total_chars = self.content.chars().count();
        let start = start_char.min(total_chars);
        let end = end_char.min(total_chars);
        if let Some(layout) = self.text_layout.as_ref() {
            return layout
                .selection_rects(self.content.as_str(), start, end)
                .into_iter()
                .map(|rect| Rect {
                    x: self.layout_state.layout_position.x + rect.x,
                    y: self.layout_state.layout_position.y + rect.y,
                    width: rect.width,
                    height: rect.height,
                })
                .collect();
        }
        Vec::new()
    }

    /// Hit-test a screen-space `(x, y)` to a local char index inside
    /// `self.content`. Returns `None` when the point is outside the
    /// text's actual rendered bounds (so callers can fall back instead
    /// of attaching the cursor to clicks that miss the text).
    pub fn screen_position_to_local_char(&self, x: f32, y: f32) -> Option<usize> {
        // Inline path: locate the fragment whose position+size contains
        // the click; query that fragment's adapter layout and map back by accumulating
        // preceding fragments' char counts. Returning `None` when the
        // click misses every fragment (so callers don't snap a click in
        // empty space inside the outer Element to a phantom char).
        if let Some(plan) = self.inline_plan.as_ref()
            && !plan.runs.is_empty()
        {
            let mut consumed_chars = 0_usize;
            for fragment in plan.runs.iter() {
                let origin = fragment.position.unwrap_or(Position { x: 0.0, y: 0.0 });
                let frag_x = origin.x;
                let frag_y = origin.y;
                let inside = x >= frag_x
                    && x <= frag_x + fragment.width.max(0.0)
                    && y >= frag_y
                    && y <= frag_y + fragment.height.max(0.0);
                if inside {
                    let local_x = x - frag_x;
                    let local_y = y - frag_y;
                    if let Some(layout) = fragment.text_layout.as_ref() {
                        let byte = clamp_utf8_boundary(
                            &fragment.content,
                            layout.hit_byte(local_x, local_y),
                        );
                        return Some(consumed_chars + fragment.content[..byte].chars().count());
                    }
                    return None;
                }
                consumed_chars += fragment.content.chars().count();
            }
            return None;
        }

        // Block / non-inline path.
        if let Some(layout) = self.text_layout.as_ref() {
            let local_x = x - self.layout_state.layout_position.x;
            let local_y = y - self.layout_state.layout_position.y;
            let bounds_w = self.layout_state.layout_size.width.max(0.0);
            let bounds_h = self.layout_state.layout_size.height.max(0.0);
            if local_x < 0.0 || local_y < 0.0 || local_x > bounds_w || local_y > bounds_h {
                return None;
            }
            let byte = clamp_utf8_boundary(&self.content, layout.hit_byte(local_x, local_y));
            return Some(self.content[..byte].chars().count());
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn clamp_utf8_boundary(content: &str, byte_index: usize) -> usize {
    let mut byte = byte_index.min(content.len());
    while byte > 0 && !content.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}
