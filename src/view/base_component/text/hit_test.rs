//! Text hit-test + caret + selection-rect computation.

use std::cell::RefCell;

use cosmic_text::Buffer as GlyphBuffer;

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

pub(super) fn current_text_area_selection_render_context() -> Option<TextAreaSelectionRenderContext> {
    TEXT_AREA_SELECTION_RENDER_CONTEXT.with(|slot| slot.borrow().clone())
}

// ---------------------------------------------------------------------------
// Text impl — caret / selection-rect / hit-test
// ---------------------------------------------------------------------------

impl Text {
    /// Translate a `local_char` index into `self.content` to a screen-space
    /// `(x, y_top, line_height)`. Returns `None` if no glyph buffer is
    /// available. Used by `TextArea` to query precise caret coordinates
    /// inside a projection that wraps a `<Text>`.
    ///
    /// Inline-laid-out Text uses `inline_plan` (one fragment per visual
    /// line, each with its own buffer + position relative to Text's
    /// `layout_position`); block-laid-out Text uses `layout_buffer`. Both
    /// paths report a `line_height` from the buffer's metrics, *not*
    /// the element's outer `layout_size.height` — caret stays one visual
    /// line tall even when the Text wraps internally.
    ///
    /// Returned `(x, y)` are absolute screen coordinates (already
    /// translated by `self.layout_position`).
    pub fn local_char_to_screen_position(&self, local_char: usize) -> Option<(f32, f32, f32)> {
        let total_chars = self.content.chars().count();
        let target_char = local_char.min(total_chars);

        // Inline path: walk inline_plan fragments accumulating char
        // counts; the fragment hosting `target_char` provides the buffer
        // (and a position offset relative to Text's layout_position).
        if let Some(plan) = self.inline_plan.as_ref()
            && !plan.runs.is_empty()
        {
            let mut consumed_chars = 0_usize;
            let mut last_fragment_end_screen: Option<(f32, f32, f32)> = None;
            for fragment in plan.runs.iter() {
                let frag_chars = fragment.content.chars().count();
                let frag_origin = fragment.position.unwrap_or(Position { x: 0.0, y: 0.0 });
                let buffer = fragment.layout_buffer.as_ref()?;
                let line_height = buffer.metrics().line_height;
                let in_fragment_char = target_char.saturating_sub(consumed_chars);
                if in_fragment_char <= frag_chars {
                    let local_byte = fragment
                        .content
                        .char_indices()
                        .nth(in_fragment_char)
                        .map(|(b, _)| b)
                        .unwrap_or(fragment.content.len());
                    let pos = caret_position_in_buffer(buffer, local_byte);
                    let (gx, gy) = pos.unwrap_or((fragment.width.max(0.0), 0.0));
                    return Some((frag_origin.x + gx, frag_origin.y + gy, line_height));
                }
                consumed_chars += frag_chars;
                last_fragment_end_screen = Some((
                    frag_origin.x + fragment.width.max(0.0),
                    frag_origin.y,
                    line_height,
                ));
            }
            return last_fragment_end_screen;
        }

        // Block / non-inline path: query layout_buffer directly.
        let buffer = self.layout_buffer.as_ref()?;
        let line_height = buffer.metrics().line_height;
        let target_byte = self
            .content
            .char_indices()
            .nth(target_char)
            .map(|(b, _)| b)
            .unwrap_or(self.content.len());
        let mut last_x = 0.0_f32;
        let mut last_top = 0.0_f32;
        let mut had_run = false;
        for run in buffer.layout_runs() {
            had_run = true;
            last_top = run.line_top;
            last_x = run.glyphs.last().map(|g| g.x + g.w.max(0.0)).unwrap_or(0.0);
            for glyph in run.glyphs.iter() {
                if glyph.start >= target_byte {
                    return Some((
                        self.layout_state.layout_position.x + glyph.x,
                        self.layout_state.layout_position.y + run.line_top,
                        line_height,
                    ));
                }
            }
        }
        if had_run {
            return Some((
                self.layout_state.layout_position.x + last_x,
                self.layout_state.layout_position.y + last_top,
                line_height,
            ));
        }
        None
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
                .filter_map(|fragment| {
                    let buffer = fragment.layout_buffer.as_ref()?;
                    let line_height = buffer.metrics().line_height;
                    let origin = fragment.position.unwrap_or(Position { x: 0.0, y: 0.0 });
                    Some((origin.x, origin.y, line_height))
                })
                .collect();
        }
        let Some(buffer) = self.layout_buffer.as_ref() else {
            return Vec::new();
        };
        let line_height = buffer.metrics().line_height;
        buffer
            .layout_runs()
            .map(|run| {
                let head_x = run.glyphs.first().map(|g| g.x).unwrap_or(0.0);
                (
                    self.layout_state.layout_position.x + head_x,
                    self.layout_state.layout_position.y + run.line_top,
                    line_height,
                )
            })
            .collect()
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
                let Some(buffer) = fragment.layout_buffer.as_ref() else {
                    continue;
                };
                let origin = fragment.position.unwrap_or(Position { x: 0.0, y: 0.0 });
                let fragment_start = start_char.saturating_sub(frag_start);
                let fragment_end = end_char.saturating_sub(frag_start).min(frag_chars);
                for rect in selection_rects_in_buffer(
                    buffer,
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
            }
            return out;
        }

        let Some(buffer) = self.layout_buffer.as_ref() else {
            return Vec::new();
        };
        let total_chars = self.content.chars().count();
        let start = start_char.min(total_chars);
        let end = end_char.min(total_chars);
        selection_rects_in_buffer(buffer, self.content.as_str(), start, end)
            .into_iter()
            .map(|rect| Rect {
                x: self.layout_state.layout_position.x + rect.x,
                y: self.layout_state.layout_position.y + rect.y,
                width: rect.width,
                height: rect.height,
            })
            .collect()
    }

    /// Hit-test a screen-space `(x, y)` to a local char index inside
    /// `self.content`. Returns `None` when the point is outside the
    /// text's actual rendered bounds (so callers can fall back instead
    /// of attaching the cursor to clicks that miss the text).
    pub fn screen_position_to_local_char(&self, x: f32, y: f32) -> Option<usize> {
        // Inline path: locate the fragment whose position+size contains
        // the click; query that fragment's buffer for paragraph-local
        // hit; map back to Text-content char index by accumulating
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
                    let buffer = fragment.layout_buffer.as_ref()?;
                    let local_x = x - frag_x;
                    let local_y = y - frag_y;
                    let cursor = buffer.hit(local_x, local_y)?;
                    let mut byte_offset = 0_usize;
                    for (line_i, paragraph) in fragment.content.split('\n').enumerate() {
                        if line_i == cursor.line {
                            let local_byte = cursor.index.min(paragraph.len());
                            let in_fragment_char =
                                fragment.content[..byte_offset + local_byte].chars().count();
                            return Some(consumed_chars + in_fragment_char);
                        }
                        byte_offset += paragraph.len() + 1;
                    }
                    return None;
                }
                consumed_chars += fragment.content.chars().count();
            }
            return None;
        }

        // Block / non-inline path.
        let buffer = self.layout_buffer.as_ref()?;
        let local_x = x - self.layout_state.layout_position.x;
        let local_y = y - self.layout_state.layout_position.y;
        let bounds_w = self.layout_state.layout_size.width.max(0.0);
        let bounds_h = self.layout_state.layout_size.height.max(0.0);
        if local_x < 0.0 || local_y < 0.0 || local_x > bounds_w || local_y > bounds_h {
            return None;
        }
        let cursor = buffer.hit(local_x, local_y)?;
        let mut byte_offset = 0_usize;
        for (line_i, paragraph) in self.content.split('\n').enumerate() {
            if line_i == cursor.line {
                let local_byte = cursor.index.min(paragraph.len());
                return Some(self.content[..byte_offset + local_byte].chars().count());
            }
            byte_offset += paragraph.len() + 1;
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Find the `(x, y_top)` of the caret at `target_byte` (paragraph-local
/// byte offset against the buffer's shaped text). Returns `None` when
/// the buffer has no layout runs. Walks all layout runs in order; the
/// first glyph with `start >= target_byte` wins, else falls through to
/// "end of last run". Used by `Text::local_char_to_screen_position` for
/// the inline-fragment per-line query.
fn caret_position_in_buffer(buffer: &GlyphBuffer, target_byte: usize) -> Option<(f32, f32)> {
    let mut last_x = 0.0_f32;
    let mut last_top = 0.0_f32;
    let mut had_run = false;
    for run in buffer.layout_runs() {
        had_run = true;
        last_top = run.line_top;
        last_x = run.glyphs.last().map(|g| g.x + g.w.max(0.0)).unwrap_or(0.0);
        for glyph in run.glyphs.iter() {
            if glyph.start >= target_byte {
                return Some((glyph.x, run.line_top));
            }
        }
    }
    if had_run {
        return Some((last_x, last_top));
    }
    None
}

fn byte_index_at_char(content: &str, char_index: usize) -> usize {
    content
        .char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(content.len())
}

fn paragraph_line_and_offset(content: &str, byte_index: usize) -> (usize, usize) {
    let clamped = byte_index.min(content.len());
    let mut line_i = 0_usize;
    let mut line_start = 0_usize;
    for (byte, ch) in content.char_indices() {
        if byte >= clamped {
            break;
        }
        if ch == '\n' {
            line_i += 1;
            line_start = byte + ch.len_utf8();
        }
    }
    (line_i, clamped.saturating_sub(line_start))
}

fn selection_rects_in_buffer(
    buffer: &GlyphBuffer,
    content: &str,
    local_start: usize,
    local_end: usize,
) -> Vec<Rect> {
    let start_char = local_start.min(local_end);
    let end_char = local_start.max(local_end);
    if start_char == end_char {
        return Vec::new();
    }
    let start_byte = byte_index_at_char(content, start_char);
    let end_byte = byte_index_at_char(content, end_char);
    let (start_line_i, start_local) = paragraph_line_and_offset(content, start_byte);
    let (end_line_i, end_local) = paragraph_line_and_offset(content, end_byte);

    let mut out = Vec::new();
    let line_height = buffer.metrics().line_height;
    for run in buffer.layout_runs() {
        if run.line_i < start_line_i || run.line_i > end_line_i {
            continue;
        }
        let line_local_start = if run.line_i == start_line_i {
            start_local
        } else {
            0
        };
        let line_local_end = if run.line_i == end_line_i {
            end_local
        } else {
            usize::MAX
        };
        if line_local_end <= line_local_start {
            continue;
        }

        let mut left: Option<f32> = None;
        let mut right = 0.0_f32;
        for glyph in run.glyphs.iter() {
            if glyph.end <= line_local_start || glyph.start >= line_local_end {
                continue;
            }
            let glyph_left = glyph.x;
            let glyph_right = glyph.x + glyph.w.max(0.0);
            left = Some(
                left.map(|current| current.min(glyph_left))
                    .unwrap_or(glyph_left),
            );
            right = right.max(glyph_right);
        }
        if let Some(left) = left {
            out.push(Rect {
                x: left,
                y: run.line_top,
                width: (right - left).max(1.0),
                height: line_height,
            });
        }
    }
    out
}
