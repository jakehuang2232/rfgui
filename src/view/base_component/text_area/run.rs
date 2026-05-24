//! `TextAreaTextRun` — internal plain-text segment child of `TextArea`.
//!
//! P2.1: shapes its segment via the shared text layout adapter, exposes inline measure/place,
//! and emits a single `TextPassFragment` per visual run during paint. Wrap
//! happens inside the text layout engine (controlled by the cascaded
//! `auto_wrap` flag), but wrapped visual lines are exposed back to the parent
//! inline solver as individual fragments so the next sibling receives a
//! `first_available_width` derived from the real last visual line.
//!
//! See `docs/design/textarea-v2.md` (Phase 2) for the role of this
//! component within the v2 inline pipeline.

#![allow(dead_code)]

use std::ops::Range;
use std::sync::Arc;

use crate::style::{ColorLike, Cursor};
use crate::ui::Rect;
use crate::view::base_component::text::measure_text_layout;
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, InlineMeasureContext,
    InlineNodeSize, InlinePlacement, LayoutConstraints, LayoutPlacement, Layoutable, Position,
    Renderable, Size, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::layout::LayoutState;
use crate::view::node_arena::NodeKey;
use crate::view::render_pass::TextPass;
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassFragment, TextPassParams,
};
use crate::view::text_layout::{TextLayout, TextLayoutAlignment};

use super::super::next_ui_node_id;
use super::edit::byte_index_at_char;

/// Legacy in-run IME preedit splice used by projection/context paths.
/// Plain TextArea preedit is represented as a transient sibling Run.
#[derive(Clone, Debug, PartialEq)]
pub struct InlinePreedit {
    pub insert_at_local: usize,
    pub preedit_text: String,
    pub preedit_cursor: Option<(usize, usize)>,
}

pub(crate) struct TextAreaTextRun {
    pub(crate) text: String,
    pub(crate) char_range: Range<usize>,
    pub(crate) is_placeholder: bool,
    pub(crate) is_preedit_run: bool,
    pub(crate) preedit_cursor: Option<(usize, usize)>,
    /// `text` is the visible content of one paragraph. Hard newline
    /// characters are represented by a sibling [`TextAreaLineBreak`], not
    /// by flags on the text run.
    // style cascaded from owning TextArea
    pub(crate) font_families: Vec<String>,
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) font_weight: u16,
    pub(crate) color: crate::style::Color,
    pub(crate) cursor: Cursor,
    pub(crate) auto_wrap: bool,
    pub(crate) vertical_align: crate::style::VerticalAlign,

    // Legacy IME splice path. Plain TextArea preedit uses `is_preedit_run`.
    pub(crate) inline_preedit: Option<InlinePreedit>,

    // text layout state
    text_layout: Option<Arc<TextLayout>>,
    last_inline_measure_context: Option<InlineMeasureContext>,

    // layout output
    pub(crate) layout_state: LayoutState,
    pub(crate) inline_paint_fragments: Vec<Rect>,
    pub(crate) dirty_flags: DirtyFlags,

    // identity
    pub(crate) node_id: u64,
    pub(crate) parent_id: Option<u64>,
    pub(crate) children: Vec<NodeKey>,
}

impl TextAreaTextRun {
    pub(crate) fn new(text: String, char_range: Range<usize>) -> Self {
        Self {
            text,
            char_range,
            is_placeholder: false,
            is_preedit_run: false,
            preedit_cursor: None,
            font_families: Vec::new(),
            font_size: 14.0,
            line_height: 1.25,
            font_weight: 400,
            color: crate::style::Color::rgba(17, 17, 17, 255),
            cursor: Cursor::Text,
            auto_wrap: true,
            vertical_align: crate::style::VerticalAlign::Baseline,
            inline_preedit: None,
            text_layout: None,
            last_inline_measure_context: None,
            layout_state: LayoutState::new(0.0, 0.0, 0.0, 0.0),
            inline_paint_fragments: Vec::new(),
            dirty_flags: DirtyFlags::ALL,
            node_id: next_ui_node_id(),
            parent_id: None,
            children: Vec::new(),
        }
    }

    pub fn char_range(&self) -> Range<usize> {
        self.char_range.clone()
    }

    pub fn set_inline_preedit(&mut self, preedit: Option<InlinePreedit>) {
        if self.inline_preedit == preedit {
            return;
        }
        self.inline_preedit = preedit;
        self.invalidate_text_layout();
    }

    pub(crate) fn set_preedit_run(&mut self, is_preedit_run: bool, cursor: Option<(usize, usize)>) {
        if self.is_preedit_run == is_preedit_run && self.preedit_cursor == cursor {
            return;
        }
        self.is_preedit_run = is_preedit_run;
        self.preedit_cursor = cursor;
        self.invalidate_text_layout();
    }

    pub(crate) fn is_preedit_run(&self) -> bool {
        self.is_preedit_run
    }

    pub(crate) fn set_text(&mut self, text: String, char_range: Range<usize>) {
        if self.text == text && self.char_range == char_range {
            return;
        }
        self.text = text;
        self.char_range = char_range;
        self.invalidate_text_layout();
    }

    /// Cascade-style cascaded set: owner TextArea calls this after edit/
    /// content-rebuild so the run picks up the up-to-date inherited values.
    pub(crate) fn cascade_style(
        &mut self,
        font_families: Vec<String>,
        font_size: f32,
        line_height: f32,
        vertical_align: crate::style::VerticalAlign,
        font_weight: u16,
        color: crate::style::Color,
        cursor: Cursor,
        auto_wrap: bool,
    ) {
        let layout_changed = self.font_families != font_families
            || self.font_size != font_size
            || self.line_height != line_height
            || self.vertical_align != vertical_align
            || self.font_weight != font_weight
            || self.color != color
            || self.auto_wrap != auto_wrap;
        self.font_families = font_families;
        self.font_size = font_size;
        self.line_height = line_height;
        self.vertical_align = vertical_align;
        self.font_weight = font_weight;
        self.color = color;
        self.cursor = cursor;
        self.auto_wrap = auto_wrap;
        if layout_changed {
            self.invalidate_text_layout();
        }
    }

    fn invalidate_text_layout(&mut self) {
        self.text_layout = None;
        self.last_inline_measure_context = None;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    /// Effective text including any spliced IME preedit segment.
    fn effective_text(&self) -> String {
        match &self.inline_preedit {
            None => self.text.clone(),
            Some(preedit) => {
                let local_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
                let mut out = String::with_capacity(self.text.len() + preedit.preedit_text.len());
                out.push_str(&self.text[..local_byte]);
                out.push_str(&preedit.preedit_text);
                out.push_str(&self.text[local_byte..]);
                out
            }
        }
    }

    fn build_run_text_layout(&self, max_width: Option<f32>) -> Arc<TextLayout> {
        measure_text_layout(
            &self.effective_text(),
            max_width,
            self.auto_wrap,
            self.font_size,
            self.line_height,
            self.font_weight,
            TextLayoutAlignment::Left,
            &self.font_families,
        )
        .text_layout
    }

    /// `local_char` here is in the run's *own* char index
    /// (0..self.text.chars().count()). Returns `(x, y_top, line_height)`
    /// in run-local coordinates.
    ///
    /// The plain-text `local_char` is translated into the matching byte
    /// inside `effective_text` (which includes any spliced preedit text)
    /// before asking the adapter for caret geometry.
    pub fn local_char_to_screen_position(&self, local_char: usize) -> Option<(f32, f32, f32)> {
        self.local_char_to_screen_position_with_affinity(
            local_char,
            super::caret_map::CaretAffinity::Downstream,
        )
    }

    /// Like [`Self::local_char_to_screen_position`] but biases the
    /// soft-wrap boundary based on `affinity`. `Upstream` returns the
    /// upper line's tail position when `local_char` lands at the wrap
    /// point; `Downstream` returns the lower line's head (current
    /// pre-affinity behaviour).
    pub fn local_char_to_screen_position_with_affinity(
        &self,
        local_char: usize,
        affinity: super::caret_map::CaretAffinity,
    ) -> Option<(f32, f32, f32)> {
        let effective = self.effective_text();
        let target_byte = self.plain_local_char_to_effective_byte(local_char, &effective);
        if let Some(layout) = self.text_layout.as_ref() {
            let target_char = effective.get(..target_byte)?.chars().count();
            let geom = layout.caret_geometry_for_char_with_affinity(
                &effective,
                target_char,
                affinity == super::caret_map::CaretAffinity::Upstream,
            );
            return Some((geom.x, geom.y, geom.height));
        }
        Some(self.empty_line_caret_position())
    }

    /// Caret position when the IME preedit is open inside this Run. Honors
    /// the preedit's own caret (`preedit_cursor`) so the visible caret sits
    /// inside the composing text rather than at the splice point — mirrors
    /// v1's `preedit_fragment_caret_screen_position`.
    pub fn preedit_caret_local_position(&self) -> Option<(f32, f32, f32)> {
        if self.is_preedit_run {
            let caret_byte = match self.preedit_cursor {
                Some((_, end)) => clamp_utf8_boundary(&self.text, end),
                None => self.text.len(),
            };
            if let Some(layout) = self.text_layout.as_ref() {
                let geom = layout.cursor_geometry(caret_byte, false);
                return Some((geom.x, geom.y, geom.height));
            }
            return Some(self.empty_line_caret_position());
        }
        let preedit = self.inline_preedit.as_ref()?;
        let pre_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
        let caret_byte_in_preedit = match preedit.preedit_cursor {
            Some((_, end)) => clamp_utf8_boundary(&preedit.preedit_text, end),
            None => preedit.preedit_text.len(),
        };
        let target_byte = pre_byte + caret_byte_in_preedit;
        if let Some(layout) = self.text_layout.as_ref() {
            // TODO: Fold IME composing into `CaretNavigationMap` once the
            // map can represent transient preedit positions separately from
            // committed document char indices. For Phase 5A this special
            // path stays, but its geometry is adapter-backed.
            let geom = layout.cursor_geometry(target_byte, false);
            return Some((geom.x, geom.y, geom.height));
        }
        Some(self.empty_line_caret_position())
    }

    fn plain_local_char_to_effective_byte(&self, local_char: usize, effective: &str) -> usize {
        let char_in_effective = match &self.inline_preedit {
            Some(preedit) if local_char > preedit.insert_at_local => {
                local_char + preedit.preedit_text.chars().count()
            }
            _ => local_char,
        };
        byte_index_at_char(effective, char_in_effective)
    }

    fn empty_line_caret_position(&self) -> (f32, f32, f32) {
        let line_h = self.font_size.max(1.0) * self.line_height.max(0.8);
        (0.0, 0.0, line_h)
    }

    fn fallback_first_baseline(&self) -> f32 {
        let font_size = self.font_size.max(1.0);
        let line_height = font_size * self.line_height.max(0.8);
        let approx_ascent = font_size * 0.8779297;
        let leading = (line_height - font_size).max(0.0);
        (approx_ascent + leading / 2.0).max(0.0)
    }

    fn inline_line_nodes(&self) -> Vec<InlineNodeSize> {
        let Some(layout) = self.text_layout.as_ref() else {
            let nodes = vec![InlineNodeSize {
                width: self.layout_state.layout_size.width,
                height: self.layout_state.layout_size.height,
                baseline: self.fallback_first_baseline(),
                vertical_align: self.vertical_align,
                force_break_after: false,
            }];
            return nodes;
        };
        let effective = self.effective_text();
        let mut nodes: Vec<InlineNodeSize> = layout
            .inline_line_fragments(&effective)
            .into_iter()
            .map(|line| InlineNodeSize {
                width: line.width,
                height: line.height,
                baseline: line.baseline,
                vertical_align: self.vertical_align,
                force_break_after: false,
            })
            .collect();
        if nodes.is_empty() {
            nodes.push(InlineNodeSize {
                width: 0.0,
                height: self.font_size.max(1.0) * self.line_height.max(0.8),
                baseline: self.fallback_first_baseline(),
                vertical_align: self.vertical_align,
                force_break_after: false,
            });
        }
        let last = nodes.len().saturating_sub(1);
        for (idx, node) in nodes.iter_mut().enumerate() {
            node.force_break_after = idx < last;
        }
        nodes
    }

    fn inline_text_pass_fragments(
        &self,
        opacity: f32,
        paint_offset: [f32; 2],
    ) -> Vec<TextPassFragment> {
        let Some(layout) = self.text_layout.as_ref() else {
            return Vec::new();
        };
        let effective = self.effective_text();
        let line_fragments = layout.inline_line_fragments(&effective);
        if line_fragments.len() <= 1 || line_fragments.len() != self.inline_paint_fragments.len() {
            return Vec::new();
        }
        line_fragments
            .into_iter()
            .zip(self.inline_paint_fragments.iter())
            .filter_map(|(line, rect)| {
                if line.content.is_empty() {
                    return None;
                }
                let x = rect.x + paint_offset[0];
                let y = rect.y + paint_offset[1];
                let fragment_layout = measure_text_layout(
                    line.content.as_str(),
                    Some(line.width.max(1.0)),
                    false,
                    self.font_size,
                    self.line_height,
                    self.font_weight,
                    TextLayoutAlignment::Left,
                    self.font_families.as_slice(),
                );
                Some(TextPassFragment {
                    content: line.content,
                    x,
                    y,
                    width: rect.width.max(line.width).max(1.0),
                    height: rect.height.max(line.height).max(1.0),
                    color: self.color.to_rgba_f32(),
                    opacity,
                    text_layout: Some(fragment_layout.text_layout),
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn inline_fragment_positions(&self) -> Vec<(String, Rect)> {
        let Some(layout) = self.text_layout.as_ref() else {
            return Vec::new();
        };
        layout
            .inline_line_fragments(&self.effective_text())
            .into_iter()
            .zip(self.inline_paint_fragments.iter().copied())
            .map(|(line, rect)| (line.content, rect))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn inline_text_pass_fragment_positions(&self) -> Vec<(String, Rect)> {
        self.inline_text_pass_fragment_positions_with_offset([0.0, 0.0])
    }

    #[cfg(test)]
    pub(crate) fn inline_text_pass_fragment_positions_with_offset(
        &self,
        paint_offset: [f32; 2],
    ) -> Vec<(String, Rect)> {
        let fragments = self.inline_text_pass_fragments(1.0, paint_offset);
        if fragments.is_empty() && self.text_layout.is_some() && !self.effective_text().is_empty() {
            return vec![(
                self.effective_text(),
                Rect {
                    x: self.layout_state.layout_position.x + paint_offset[0],
                    y: self.layout_state.layout_position.y + paint_offset[1],
                    width: self.layout_state.layout_size.width.max(1.0),
                    height: self.layout_state.layout_size.height.max(1.0),
                },
            )];
        }
        fragments
            .into_iter()
            .map(|fragment| {
                (
                    fragment.content,
                    Rect {
                        x: fragment.x,
                        y: fragment.y,
                        width: fragment.width,
                        height: fragment.height,
                    },
                )
            })
            .collect()
    }

    /// Hit-test: run-local (x, y) → char index in `effective_text`
    /// (i.e. the spliced text the adapter laid out). When
    /// no preedit is active this matches `self.text`; with preedit, the
    /// returned index counts preedit chars too. Callers in commit-tap
    /// flows commit the preedit first, after which `self.content` matches
    /// the effective text for this Run, so the index maps directly to
    /// the post-commit content char index.
    ///
    pub fn screen_position_to_local_char(&self, x: f32, y: f32) -> Option<usize> {
        if self.is_preedit_run {
            return Some(0);
        }
        if let Some(layout) = self.text_layout.as_ref() {
            let effective = self.effective_text();
            let byte = clamp_utf8_boundary(&effective, layout.hit_byte(x, y));
            let prefix = effective.get(..byte)?;
            return Some(prefix.chars().count());
        }
        None
    }

    /// Run-local selection range → visual rects (one per visual line covered).
    pub fn local_selection_rects(&self, local_start: usize, local_end: usize) -> Vec<Rect> {
        let start_char = local_start.min(local_end);
        let end_char = local_start.max(local_end);
        if start_char == end_char {
            return Vec::new();
        }
        if let Some(layout) = self.text_layout.as_ref() {
            let line_fragments = layout.inline_line_fragments(&self.text);
            if line_fragments.len() > 1 && line_fragments.len() == self.inline_paint_fragments.len()
            {
                let origin = self.layout_state.layout_position;
                let mut out = Vec::new();
                let mut consumed_chars = 0_usize;
                for (line, fragment_rect) in line_fragments
                    .into_iter()
                    .zip(self.inline_paint_fragments.iter())
                {
                    let frag_chars = line.content.chars().count();
                    let frag_start = consumed_chars;
                    let frag_end = consumed_chars + frag_chars;
                    consumed_chars = frag_end;
                    if frag_end <= start_char || frag_start >= end_char {
                        continue;
                    }
                    let fragment_start = start_char.saturating_sub(frag_start);
                    let fragment_end = end_char.saturating_sub(frag_start).min(frag_chars);
                    let fragment_layout = measure_text_layout(
                        line.content.as_str(),
                        Some(line.width.max(1.0)),
                        false,
                        self.font_size,
                        self.line_height,
                        self.font_weight,
                        TextLayoutAlignment::Left,
                        self.font_families.as_slice(),
                    );
                    for rect in fragment_layout.text_layout.selection_rects(
                        line.content.as_str(),
                        fragment_start,
                        fragment_end,
                    ) {
                        out.push(Rect {
                            x: fragment_rect.x - origin.x + rect.x,
                            y: fragment_rect.y - origin.y + rect.y,
                            width: rect.width,
                            height: rect.height,
                        });
                    }
                }
                return out;
            }
            return layout.selection_rects(self.text.as_str(), start_char, end_char);
        }
        Vec::new()
    }

    /// Underline rects (run-local coords) covering the active IME preedit
    /// segment. One rect per visual line the preedit spans. Empty when no
    /// preedit is active or layout is stale. 1-px-tall stripes pinned to
    /// the visual-line baseline — matches v1's
    /// `ime_preedit_underline_rects` look.
    pub fn preedit_underline_rects(&self) -> Vec<Rect> {
        if self.is_preedit_run {
            return self
                .local_selection_rects(0, self.text.chars().count())
                .into_iter()
                .map(|rect| Rect {
                    x: rect.x,
                    y: rect.y + rect.height.max(1.0) - 1.0,
                    width: rect.width.max(1.0),
                    height: 1.0,
                })
                .collect();
        }
        let Some(preedit) = self.inline_preedit.as_ref() else {
            return Vec::new();
        };
        if preedit.preedit_text.is_empty() {
            return Vec::new();
        }
        let effective = self.effective_text();
        if let Some(layout) = self.text_layout.as_ref() {
            return layout
                .selection_rects(
                    &effective,
                    preedit.insert_at_local,
                    preedit.insert_at_local + preedit.preedit_text.chars().count(),
                )
                .into_iter()
                .map(|rect| Rect {
                    x: rect.x,
                    y: rect.y + rect.height.max(1.0) - 1.0,
                    width: rect.width.max(1.0),
                    height: 1.0,
                })
                .collect();
        }
        Vec::new()
    }

    /// Number of visual (post-wrap) lines in the current layout. Useful for
    /// vertical caret movement and sticky-x bookkeeping.
    pub fn visual_line_count(&self) -> usize {
        if let Some(layout) = self.text_layout.as_ref() {
            return layout.lines().len().max(1);
        }
        1
    }

    /// Run-local caret stops grouped by visual line. Each line carries the
    /// stops needed by the TextArea-level `CaretNavigationMap` builder so it
    /// can drive vertical Up/Down navigation, caret rendering, and pointer
    /// hit-test from a single source of truth (see
    /// `docs/design/textarea-caret-navigation.md`).
    ///
    /// Coordinates returned here are **run-local**: the map builder adds
    /// `layout_position` to translate to screen space. Char indices are
    /// **run-local** too (`0..self.text.chars().count()`); the builder adds
    /// `char_range.start` for the root content index.
    ///
    /// Lines come from the shared text layout adapter. Empty paragraphs
    /// (created by `\n\n` or a trailing `\n`) get
    /// a synthesized line so caret navigation can land on the blank line.
    pub fn caret_stops(&self) -> Vec<RunCaretLine> {
        if let Some(layout) = self.text_layout.as_ref() {
            let effective = self.effective_text();
            let lines: Vec<RunCaretLine> = layout
                .visual_caret_lines(&effective)
                .into_iter()
                .map(|line| RunCaretLine {
                    local_y_top: line.y_top,
                    local_y_bottom: line.y_bottom,
                    stops: line
                        .stops
                        .into_iter()
                        .map(|stop| RunCaretStop {
                            local_char: self.effective_char_to_plain_local_char(stop.char_index),
                            local_x: stop.x,
                            local_y_top: line.y_top,
                            height: stop.height,
                        })
                        .collect(),
                })
                .collect();
            return lines;
        }

        let line_height = self.font_size.max(1.0) * self.line_height.max(0.8);
        vec![RunCaretLine {
            local_y_top: 0.0,
            local_y_bottom: line_height,
            stops: vec![RunCaretStop {
                local_char: 0,
                local_x: 0.0,
                local_y_top: 0.0,
                height: line_height,
            }],
        }]
    }

    fn effective_char_to_plain_local_char(&self, effective_char: usize) -> usize {
        if self.is_preedit_run {
            return 0;
        }
        match &self.inline_preedit {
            Some(preedit) => {
                let preedit_len = preedit.preedit_text.chars().count();
                let insert_at = preedit.insert_at_local;
                if effective_char <= insert_at {
                    effective_char
                } else if effective_char <= insert_at + preedit_len {
                    insert_at
                } else {
                    effective_char - preedit_len
                }
            }
            None => effective_char,
        }
    }
}

/// Run-local caret stop produced by [`TextAreaTextRun::caret_stops`].
#[derive(Clone, Debug)]
pub struct RunCaretStop {
    pub local_char: usize,
    pub local_x: f32,
    pub local_y_top: f32,
    pub height: f32,
}

/// One visual line worth of caret stops in run-local coordinates.
#[derive(Clone, Debug)]
pub struct RunCaretLine {
    pub local_y_top: f32,
    pub local_y_bottom: f32,
    pub stops: Vec<RunCaretStop>,
}

pub(crate) struct TextAreaLineBreak {
    pub(crate) char_range: Range<usize>,
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) vertical_align: crate::style::VerticalAlign,
    pub(crate) caret_fragments: [Option<Rect>; 2],
    pub(crate) layout_state: LayoutState,
    pub(crate) dirty_flags: DirtyFlags,
    pub(crate) node_id: u64,
    pub(crate) parent_id: Option<u64>,
    pub(crate) children: Vec<NodeKey>,
}

impl TextAreaLineBreak {
    pub(crate) fn new(char_range: Range<usize>) -> Self {
        Self {
            char_range,
            font_size: 14.0,
            line_height: 1.25,
            vertical_align: crate::style::VerticalAlign::Baseline,
            caret_fragments: [None, None],
            layout_state: LayoutState::new(0.0, 0.0, 0.0, 0.0),
            dirty_flags: DirtyFlags::ALL,
            node_id: next_ui_node_id(),
            parent_id: None,
            children: Vec::new(),
        }
    }

    pub(crate) fn set_char_range(&mut self, char_range: Range<usize>) {
        if self.char_range == char_range {
            return;
        }
        self.char_range = char_range;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    pub(crate) fn cascade_style(
        &mut self,
        font_size: f32,
        line_height: f32,
        vertical_align: crate::style::VerticalAlign,
    ) {
        if self.font_size == font_size
            && self.line_height == line_height
            && self.vertical_align == vertical_align
        {
            return;
        }
        self.font_size = font_size;
        self.line_height = line_height;
        self.vertical_align = vertical_align;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    fn line_height_px(&self) -> f32 {
        self.font_size.max(1.0) * self.line_height.max(0.8)
    }

    fn baseline(&self) -> f32 {
        let font_size = self.font_size.max(1.0);
        let line_height = self.line_height_px();
        let approx_ascent = font_size * 0.8779297;
        let leading = (line_height - font_size).max(0.0);
        (approx_ascent + leading / 2.0).max(0.0)
    }

    pub(crate) fn caret_stops(&self) -> Vec<RunCaretLine> {
        let line_height = self.line_height_px();
        self.caret_fragments
            .iter()
            .enumerate()
            .filter_map(|(idx, rect)| {
                let rect = rect.as_ref()?;
                let local_x = rect.x - self.layout_state.layout_position.x;
                let local_y_top = rect.y - self.layout_state.layout_position.y;
                let stops = if idx == 0 {
                    vec![
                        RunCaretStop {
                            local_char: 0,
                            local_x,
                            local_y_top,
                            height: line_height,
                        },
                        RunCaretStop {
                            local_char: 1,
                            local_x,
                            local_y_top,
                            height: line_height,
                        },
                    ]
                } else {
                    vec![RunCaretStop {
                        local_char: 1,
                        local_x,
                        local_y_top,
                        height: line_height,
                    }]
                };
                Some(RunCaretLine {
                    local_y_top,
                    local_y_bottom: local_y_top + line_height,
                    stops,
                })
            })
            .collect()
    }
}

impl Layoutable for TextAreaTextRun {
    fn measure_inline(
        &mut self,
        context: InlineMeasureContext,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let layout_clean = !self.dirty_flags.intersects(DirtyFlags::LAYOUT);
        if layout_clean
            && self.last_inline_measure_context == Some(context)
            && self.text_layout.is_some()
        {
            return;
        }

        // Shape budget = container's full inner width, not the line's
        // *remaining* width. Otherwise a Run that can't fit in the
        // remaining slot would shape narrow and produce ugly wraps; with
        // `full_available_width` the text engine wraps at the same width as if
        // the Run started on a fresh line, and the inline solver places
        // the Run on the next line if needed.
        let max_width = if self.auto_wrap {
            Some(context.full_available_width.max(1.0))
        } else {
            None
        };
        let (width, height) = if self.text.is_empty() && self.inline_preedit.is_none() {
            // Empty paragraph: skip shaping (which would substitute a
            // space and report a visible glyph width). The Run still claims
            // a `line_height`-tall slot so the inline solver gives it a
            // proper blank line. Floor at 0.8 to match every other line-
            // height path (`line_height_px`, `empty_line_caret_position`,
            // the shaped path's `build_text_layout`) so a blank paragraph
            // and a shaped one report the same height.
            self.text_layout = None;
            (0.0_f32, self.font_size.max(1.0) * self.line_height.max(0.8))
        } else {
            let layout = self.build_run_text_layout(max_width);
            let (w, h) = layout.measure_size();
            self.text_layout = Some(layout);
            (w, h)
        };
        self.last_inline_measure_context = Some(context);

        self.layout_state.layout_size = Size {
            width: width.max(0.0),
            height: height.max(0.0),
        };
        self.layout_state.content_size = self.layout_state.layout_size;
        self.dirty_flags = self
            .dirty_flags
            .without(DirtyFlags::LAYOUT)
            .union(DirtyFlags::PLACE)
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
            .union(DirtyFlags::PAINT);
    }

    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        // Block-level measurement falls back to inline layout with the
        // available width as the wrap budget.
        self.measure_inline(
            InlineMeasureContext {
                first_available_width: constraints.max_width,
                full_available_width: constraints.max_width,
                available_height: 1_000_000.0,
                viewport_width: constraints.viewport_width,
                viewport_height: constraints.viewport_height,
                percent_base_width: constraints.percent_base_width,
                percent_base_height: constraints.percent_base_height,
            },
            arena,
        );
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let x = placement.parent_x + placement.visual_offset_x;
        let y = placement.parent_y + placement.visual_offset_y;
        self.layout_state.layout_position = Position { x, y };
        self.layout_state.layout_inner_position = Position { x, y };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;
        self.inline_paint_fragments.clear();
        self.inline_paint_fragments.push(Rect {
            x,
            y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
        });
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn place_inline(
        &mut self,
        placement: InlinePlacement,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let fragments = self.inline_line_nodes();
        let Some(fragment) = fragments.get(placement.node_index).copied() else {
            return;
        };
        let x = placement.x;
        let y = placement.y;
        if placement.node_index == 0 {
            self.inline_paint_fragments.clear();
            self.layout_state.layout_position = Position { x, y };
            self.layout_state.layout_size = Size {
                width: 0.0,
                height: 0.0,
            };
            self.layout_state.should_render = false;
        }
        let left = x;
        let top = y;
        let right = x + fragment.width.max(0.0);
        let bottom = y + fragment.height.max(0.0);
        if self.layout_state.should_render {
            let current_right =
                self.layout_state.layout_position.x + self.layout_state.layout_size.width;
            let current_bottom =
                self.layout_state.layout_position.y + self.layout_state.layout_size.height;
            self.layout_state.layout_position.x = self.layout_state.layout_position.x.min(left);
            self.layout_state.layout_position.y = self.layout_state.layout_position.y.min(top);
            self.layout_state.layout_size.width =
                current_right.max(right) - self.layout_state.layout_position.x;
            self.layout_state.layout_size.height =
                current_bottom.max(bottom) - self.layout_state.layout_position.y;
        } else {
            self.layout_state.layout_position = Position { x: left, y: top };
            self.layout_state.layout_size = Size {
                width: (right - left).max(0.0),
                height: (bottom - top).max(0.0),
            };
        }
        self.layout_state.should_render =
            self.layout_state.layout_size.width > 0.0 && self.layout_state.layout_size.height > 0.0;
        self.layout_state.layout_inner_position = Position { x, y };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;
        self.inline_paint_fragments.push(Rect {
            x,
            y,
            width: fragment.width.max(0.0),
            height: fragment.height.max(0.0),
        });
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn measured_size(&self) -> (f32, f32) {
        (
            self.layout_state.layout_size.width,
            self.layout_state.layout_size.height,
        )
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_state.layout_size.width = width.max(0.0);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_state.layout_size.height = height.max(0.0);
    }

    fn get_inline_nodes_size(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<InlineNodeSize> {
        self.inline_line_nodes()
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (
            self.layout_state.layout_position.x,
            self.layout_state.layout_position.y,
        )
    }
}

impl Layoutable for TextAreaLineBreak {
    fn measure_inline(
        &mut self,
        _context: InlineMeasureContext,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let line_height = self.line_height_px();
        self.layout_state.layout_size = Size {
            width: 0.0,
            height: line_height,
        };
        self.layout_state.content_size = self.layout_state.layout_size;
        self.dirty_flags = self
            .dirty_flags
            .without(DirtyFlags::LAYOUT)
            .union(DirtyFlags::PLACE)
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
            .union(DirtyFlags::PAINT);
    }

    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.measure_inline(
            InlineMeasureContext {
                first_available_width: constraints.max_width,
                full_available_width: constraints.max_width,
                available_height: constraints.max_height,
                viewport_width: constraints.viewport_width,
                viewport_height: constraints.viewport_height,
                percent_base_width: constraints.percent_base_width,
                percent_base_height: constraints.percent_base_height,
            },
            arena,
        );
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let x = placement.parent_x + placement.visual_offset_x;
        let y = placement.parent_y + placement.visual_offset_y;
        self.layout_state.layout_position = Position { x, y };
        self.layout_state.layout_inner_position = Position { x, y };
        self.layout_state.layout_flow_position = Position { x, y };
        self.layout_state.layout_flow_inner_position = Position { x, y };
        self.layout_state.should_render = false;
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn place_inline(
        &mut self,
        placement: InlinePlacement,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let line_height = self.line_height_px();
        let rect = Rect {
            x: placement.x,
            y: placement.y,
            width: 0.0,
            height: line_height,
        };
        if placement.node_index == 0 {
            self.caret_fragments = [None, None];
            self.layout_state.layout_position = Position {
                x: placement.x,
                y: placement.y,
            };
            self.layout_state.layout_size = Size {
                width: 0.0,
                height: line_height,
            };
            self.layout_state.should_render = false;
        }
        if let Some(slot) = self.caret_fragments.get_mut(placement.node_index) {
            *slot = Some(rect);
        }
        let left = self.layout_state.layout_position.x.min(rect.x);
        let top = self.layout_state.layout_position.y.min(rect.y);
        let right =
            (self.layout_state.layout_position.x + self.layout_state.layout_size.width).max(rect.x);
        let bottom = (self.layout_state.layout_position.y + self.layout_state.layout_size.height)
            .max(rect.y + rect.height);
        self.layout_state.layout_position = Position { x: left, y: top };
        self.layout_state.layout_size = Size {
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
        };
        self.layout_state.layout_inner_position = self.layout_state.layout_position;
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn measured_size(&self) -> (f32, f32) {
        (0.0, self.line_height_px())
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_state.layout_size.width = width.max(0.0);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_state.layout_size.height = height.max(0.0);
    }

    fn get_inline_nodes_size(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<InlineNodeSize> {
        let line_height = self.line_height_px();
        vec![
            InlineNodeSize {
                width: 0.0,
                height: line_height,
                baseline: self.baseline(),
                vertical_align: self.vertical_align,
                force_break_after: true,
            },
            InlineNodeSize {
                width: 0.0,
                height: line_height,
                baseline: self.baseline(),
                vertical_align: self.vertical_align,
                force_break_after: false,
            },
        ]
    }
}

impl Renderable for TextAreaTextRun {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        if self.text.is_empty() && self.inline_preedit.is_none() {
            return ctx.into_state();
        }
        if self.text_layout.is_none() {
            return ctx.into_state();
        }
        let Some(input_target) = ctx.current_target() else {
            return ctx.into_state();
        };
        let fragments = {
            let inline_fragments = self.inline_text_pass_fragments(1.0, ctx.paint_offset());
            if inline_fragments.is_empty() {
                let [x, y] = ctx.paint_point(
                    self.layout_state.layout_position.x,
                    self.layout_state.layout_position.y,
                );

                vec![TextPassFragment {
                    content: self.effective_text(),
                    x,
                    y,
                    width: self.layout_state.layout_size.width.max(1.0),
                    height: self.layout_state.layout_size.height.max(1.0),
                    color: self.color.to_rgba_f32(),
                    opacity: 1.0,
                    text_layout: self.text_layout.clone(),
                }]
            } else {
                inline_fragments
            }
        };
        let pass = TextPass::new(
            TextPassParams {
                fragments,
                font_size: self.font_size,
                line_height: self.line_height,
                font_weight: self.font_weight,
                font_families: self.font_families.clone(),
                allow_wrap: false,
                scissor_rect: None,
                stencil_clip_id: None,
            },
            TextInput {
                pass_context: ctx.graphics_pass_context(),
            },
            TextOutput {
                render_target: input_target,
                ..Default::default()
            },
        );
        graph.add_graphics_pass(pass);
        ctx.set_current_target(input_target);
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::PAINT);
        ctx.into_state()
    }
}

impl Renderable for TextAreaLineBreak {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        self.dirty_flags = self.dirty_flags.without(DirtyFlags::PAINT);
        ctx.into_state()
    }
}

impl EventTarget for TextAreaTextRun {
    fn cursor(&self) -> Cursor {
        self.cursor
    }
}

impl EventTarget for TextAreaLineBreak {}

impl ElementTrait for TextAreaTextRun {
    fn stable_id(&self) -> u64 {
        self.node_id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.node_id,
            parent_id: self.parent_id,
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
            border_radius: 0.0,
            should_render: self.layout_state.should_render,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.parent_id = parent_id;
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn promotion_node_info(&self) -> crate::view::promotion::PromotionNodeInfo {
        crate::view::promotion::PromotionNodeInfo {
            estimated_pass_count: 1,
            opacity: 1.0,
            ..Default::default()
        }
    }

    /// Hash everything that affects the rendered glyph fragment so a
    /// promoted ancestor's `base_signature` dirties on edit / style /
    /// preedit / layout changes. Default `0` would let the ancestor reuse
    /// a stale layer texture.
    fn promotion_self_signature(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.layout_state.should_render.hash(&mut hasher);
        self.layout_state
            .layout_position
            .x
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_position
            .y
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_size
            .width
            .max(0.0)
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_size
            .height
            .max(0.0)
            .to_bits()
            .hash(&mut hasher);
        self.text.hash(&mut hasher);
        self.char_range.start.hash(&mut hasher);
        self.char_range.end.hash(&mut hasher);
        self.is_placeholder.hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        self.color.to_rgba_u8().hash(&mut hasher);
        self.auto_wrap.hash(&mut hasher);
        if let Some(preedit) = &self.inline_preedit {
            preedit.insert_at_local.hash(&mut hasher);
            preedit.preedit_text.hash(&mut hasher);
            preedit.preedit_cursor.hash(&mut hasher);
        } else {
            u64::MAX.hash(&mut hasher);
        }
        self.is_preedit_run.hash(&mut hasher);
        self.preedit_cursor.hash(&mut hasher);
        hasher.finish()
    }
}

impl ElementTrait for TextAreaLineBreak {
    fn stable_id(&self) -> u64 {
        self.node_id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.node_id,
            parent_id: self.parent_id,
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
            border_radius: 0.0,
            should_render: false,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.parent_id = parent_id;
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn promotion_self_signature(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.char_range.start.hash(&mut hasher);
        self.char_range.end.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.layout_state
            .layout_position
            .x
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_position
            .y
            .to_bits()
            .hash(&mut hasher);
        hasher.finish()
    }
}

/// Round `byte_index` down to the nearest valid UTF-8 char boundary in
/// `value`. Caller protection for IME preedit cursor offsets that may
/// land on a continuation byte.
fn clamp_utf8_boundary(value: &str, mut byte_index: usize) -> usize {
    byte_index = byte_index.min(value.len());
    while byte_index > 0 && !value.is_char_boundary(byte_index) {
        byte_index -= 1;
    }
    byte_index
}
