//! Text hit-test + caret + selection-rect computation.

use std::cell::RefCell;

use crate::ui::Rect;
use crate::view::inline_formatting_context::InlineIfcCaretAffinity;

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

        // Inline-IFC-owned path: the owning root installed per-line
        // boundary geometry; answer from it directly.
        if let Some(lines) = self.inline_ifc_owned_lines.as_ref() {
            let boundary = target_char;
            for (idx, line) in lines.iter().enumerate() {
                if boundary < line.char_range.start || boundary >= line.char_range.end {
                    continue;
                }
                if upstream && boundary == line.char_range.start && idx > 0 {
                    let prev = &lines[idx - 1];
                    let tail_x = prev
                        .caret_xs
                        .last()
                        .copied()
                        .unwrap_or(prev.text_rect.x + prev.text_rect.width);
                    return Some((tail_x, prev.text_rect.y, prev.text_rect.height.max(1.0)));
                }
                if !upstream
                    && boundary + 1 == line.char_range.end
                    && lines
                        .get(idx + 1)
                        .is_some_and(|next| next.char_range.start == boundary)
                {
                    let next = &lines[idx + 1];
                    let x = next.caret_xs.first().copied().unwrap_or(next.text_rect.x);
                    return Some((x, next.text_rect.y, next.text_rect.height.max(1.0)));
                }
                let x = line
                    .caret_xs
                    .get(boundary - line.char_range.start)
                    .copied()
                    .unwrap_or(line.text_rect.x);
                return Some((x, line.text_rect.y, line.text_rect.height.max(1.0)));
            }
            let last = lines.last()?;
            let tail_x = last
                .caret_xs
                .last()
                .copied()
                .unwrap_or(last.text_rect.x + last.text_rect.width);
            return Some((tail_x, last.text_rect.y, last.text_rect.height.max(1.0)));
        }

        // Standalone path: query the measure-installed shaped context.
        let context = self.shaped_context.as_ref()?;
        let byte_index = byte_index_at_char(&self.content, target_char);
        let affinity = if upstream {
            InlineIfcCaretAffinity::Upstream
        } else {
            InlineIfcCaretAffinity::Downstream
        };
        let geom = context.caret_geometry_for_byte(byte_index, affinity)?;
        Some((
            self.layout_state.layout_position.x + geom.x,
            self.layout_state.layout_position.y + geom.y,
            geom.height,
        ))
    }

    pub(crate) fn visual_caret_screen_lines(&self) -> Vec<TextCaretLine> {
        if let Some(lines) = self.inline_ifc_owned_lines.as_ref() {
            let mut out = lines
                .iter()
                .map(|line| TextCaretLine {
                    y_top: line.text_rect.y,
                    y_bottom: line.text_rect.y + line.text_rect.height,
                    stops: line
                        .caret_xs
                        .iter()
                        .enumerate()
                        .map(|(idx, &x)| TextCaretStop {
                            local_char: line.char_range.start + idx,
                            x,
                            y_top: line.text_rect.y,
                            height: line.text_rect.height.max(1.0),
                        })
                        .collect(),
                })
                .collect::<Vec<_>>();
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

        let Some(context) = self.shaped_context.as_ref() else {
            return Vec::new();
        };
        let origin = self.layout_state.layout_position;
        let snapshot = context.text_layout_snapshot_ref();
        let mut lines: Vec<TextCaretLine> = snapshot
            .lines
            .iter()
            .map(|line| TextCaretLine {
                y_top: origin.y + line.y,
                y_bottom: origin.y + line.y + line.height.max(1.0),
                stops: Vec::new(),
            })
            .collect();
        if lines.is_empty() {
            let Some(geom) = context.caret_geometry_for_byte(0, InlineIfcCaretAffinity::Downstream)
            else {
                return Vec::new();
            };
            lines.push(TextCaretLine {
                y_top: origin.y + geom.y,
                y_bottom: origin.y + geom.y + geom.height.max(1.0),
                stops: Vec::new(),
            });
        }

        if self.content.is_empty() {
            if let Some(geom) =
                context.caret_geometry_for_byte(0, InlineIfcCaretAffinity::Downstream)
            {
                push_screen_caret_stop(
                    &mut lines,
                    TextCaretStop {
                        local_char: 0,
                        x: origin.x,
                        y_top: origin.y + geom.y,
                        height: geom.height.max(1.0),
                    },
                );
            }
            normalize_screen_caret_lines(&mut lines);
            return lines;
        }

        let char_count = self.content.chars().count();
        for char_index in 0..=char_count {
            let byte_index = byte_index_at_char(&self.content, char_index);
            let Some(downstream) =
                context.caret_geometry_for_byte(byte_index, InlineIfcCaretAffinity::Downstream)
            else {
                continue;
            };
            push_screen_caret_stop(
                &mut lines,
                TextCaretStop {
                    local_char: char_index,
                    x: origin.x + downstream.x,
                    y_top: origin.y + downstream.y,
                    height: downstream.height.max(1.0),
                },
            );

            let Some(upstream) =
                context.caret_geometry_for_byte(byte_index, InlineIfcCaretAffinity::Upstream)
            else {
                continue;
            };
            if (upstream.y - downstream.y).abs() > downstream.height.max(1.0) * 0.25
                || (upstream.x - downstream.x).abs() > 0.5
            {
                push_screen_caret_stop(
                    &mut lines,
                    TextCaretStop {
                        local_char: char_index,
                        x: origin.x + upstream.x,
                        y_top: origin.y + upstream.y,
                        height: upstream.height.max(1.0),
                    },
                );
            }
        }

        normalize_screen_caret_lines(&mut lines);
        lines
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
        if let Some(lines) = self.inline_ifc_owned_lines.as_ref() {
            return lines
                .iter()
                .map(|line| {
                    (
                        line.caret_xs.first().copied().unwrap_or(line.text_rect.x),
                        line.text_rect.y,
                        line.text_rect.height.max(1.0),
                    )
                })
                .collect();
        }

        let Some(context) = self.shaped_context.as_ref() else {
            return Vec::new();
        };
        context
            .text_layout_snapshot_ref()
            .lines
            .iter()
            .map(|line| {
                (
                    self.layout_state.layout_position.x + line.x,
                    self.layout_state.layout_position.y + line.y,
                    line.height,
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

        if let Some(lines) = self.inline_ifc_owned_lines.as_ref() {
            let mut out = Vec::new();
            for line in lines {
                // Boundaries on this line span [start, end); chars span
                // [start, end - 1) plus a trailing wrap segment when the
                // selection continues past the line tail.
                let bounds_start = line.char_range.start;
                let bounds_end = line.char_range.end;
                if bounds_end <= bounds_start {
                    continue;
                }
                let sel_left_boundary = start_char.max(bounds_start);
                if sel_left_boundary >= bounds_end || end_char <= bounds_start {
                    continue;
                }
                let left = line
                    .caret_xs
                    .get(sel_left_boundary - bounds_start)
                    .copied()
                    .unwrap_or(line.text_rect.x);
                let right = if end_char < bounds_end {
                    line.caret_xs
                        .get(end_char - bounds_start)
                        .copied()
                        .unwrap_or(line.text_rect.x + line.text_rect.width)
                } else {
                    line.caret_xs
                        .last()
                        .copied()
                        .unwrap_or(line.text_rect.x + line.text_rect.width)
                };
                if right > left {
                    out.push(Rect {
                        x: left,
                        y: line.text_rect.y,
                        width: right - left,
                        height: line.text_rect.height.max(1.0),
                    });
                }
            }
            return out;
        }

        let total_chars = self.content.chars().count();
        let start = start_char.min(total_chars);
        let end = end_char.min(total_chars);
        let Some(context) = self.shaped_context.as_ref() else {
            return Vec::new();
        };
        let start_byte = byte_index_at_char(&self.content, start);
        let end_byte = byte_index_at_char(&self.content, end);
        context
            .selection_rects_for_global_range(start_byte..end_byte)
            .into_iter()
            .map(|selection| Rect {
                x: self.layout_state.layout_position.x + selection.rect.x,
                y: self.layout_state.layout_position.y + selection.rect.y,
                width: selection.rect.width,
                height: selection.rect.height,
            })
            .collect()
    }

    /// Hit-test a screen-space `(x, y)` to a local char index inside
    /// `self.content`. Returns `None` when the point is outside the
    /// text's actual rendered bounds (so callers can fall back instead
    /// of attaching the cursor to clicks that miss the text).
    pub fn screen_position_to_local_char(&self, x: f32, y: f32) -> Option<usize> {
        // Inline-IFC-owned path: locate the installed line containing the
        // click and snap to the nearest caret boundary. Returns `None`
        // when the click misses every line (so callers don't snap a click
        // in empty space inside the outer Element to a phantom char).
        if let Some(lines) = self.inline_ifc_owned_lines.as_ref() {
            for line in lines {
                let inside = x >= line.rect.x
                    && x <= line.rect.x + line.rect.width.max(0.0)
                    && y >= line.rect.y
                    && y <= line.rect.y + line.rect.height.max(0.0);
                if !inside {
                    continue;
                }
                let mut best: Option<(f32, usize)> = None;
                for (idx, &caret_x) in line.caret_xs.iter().enumerate() {
                    let distance = (caret_x - x).abs();
                    if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                        best = Some((distance, line.char_range.start + idx));
                    }
                }
                return best.map(|(_, boundary)| boundary);
            }
            return None;
        }

        // Standalone path: bounds-check, then snap through the shaped
        // context's hit test.
        let context = self.shaped_context.as_ref()?;
        let local_x = x - self.layout_state.layout_position.x;
        let local_y = y - self.layout_state.layout_position.y;
        let bounds_w = self.layout_state.layout_size.width.max(0.0);
        let bounds_h = self.layout_state.layout_size.height.max(0.0);
        if local_x < 0.0 || local_y < 0.0 || local_x > bounds_w || local_y > bounds_h {
            return None;
        }
        let hit = context.hit_test_point(local_x, local_y)?;
        let byte = match hit.target {
            crate::view::inline_formatting_context::InlineIfcHitTarget::Text {
                byte_index, ..
            } => byte_index,
            crate::view::inline_formatting_context::InlineIfcHitTarget::InlineBox { .. } => {
                return None;
            }
        };
        let byte = clamp_utf8_boundary(&self.content, byte);
        Some(self.content[..byte].chars().count())
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn byte_index_at_char(content: &str, char_index: usize) -> usize {
    content
        .char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(content.len())
}

fn clamp_utf8_boundary(content: &str, byte_index: usize) -> usize {
    let mut byte = byte_index.min(content.len());
    while byte > 0 && !content.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Group a caret stop into the line whose vertical band contains it,
/// creating a new line when none matches (legacy grouping semantics).
fn push_screen_caret_stop(lines: &mut Vec<TextCaretLine>, stop: TextCaretStop) {
    let line_idx = lines
        .iter()
        .position(|line| {
            let height = (line.y_bottom - line.y_top).max(stop.height).max(1.0);
            (line.y_top - stop.y_top).abs() <= height * 0.5
        })
        .unwrap_or_else(|| {
            lines.push(TextCaretLine {
                y_top: stop.y_top,
                y_bottom: stop.y_top + stop.height.max(1.0),
                stops: Vec::new(),
            });
            lines.len() - 1
        });
    let line = &mut lines[line_idx];
    line.y_top = line.y_top.min(stop.y_top);
    line.y_bottom = line.y_bottom.max(stop.y_top + stop.height.max(1.0));
    line.stops.push(stop);
}

/// Sort lines by y and stops by x, dedupe stops that share a char index
/// keeping the rightmost, and drop lines without stops (legacy
/// normalization semantics).
fn normalize_screen_caret_lines(lines: &mut Vec<TextCaretLine>) {
    lines.sort_by(|a, b| {
        a.y_top
            .partial_cmp(&b.y_top)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for line in lines.iter_mut() {
        line.stops
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let mut deduped: Vec<TextCaretStop> = Vec::with_capacity(line.stops.len());
        for stop in line.stops.drain(..) {
            if let Some(last) = deduped.last_mut()
                && last.local_char == stop.local_char
            {
                if stop.x > last.x {
                    *last = stop;
                }
                continue;
            }
            deduped.push(stop);
        }
        line.stops = deduped;
    }
    lines.retain(|line| !line.stops.is_empty());
}
