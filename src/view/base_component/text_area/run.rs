//! `TextAreaTextRun` — internal plain-text segment child of `TextArea`.
//!
//! P2.1: shapes its segment via cosmic-text, exposes inline measure/place,
//! and emits a single `TextPassFragment` per visual run during paint. Wrap
//! happens inside the cosmic-text `Buffer` (controlled by the cascaded
//! `auto_wrap` flag) — not via parent-level fragment splitting. The Run is
//! treated as a single atomic inline node by the parent `TextArea`'s
//! Inline layout solver, so wrapping happens *between* runs at run
//! boundaries, plus *within* a run via cosmic-text's internal layout.
//!
//! See `docs/design/textarea-v2.md` (Phase 2) for the role of this
//! component within the v2 inline pipeline.

#![allow(dead_code)]

use std::ops::Range;
use std::sync::Arc;

use cosmic_text::{Align, Buffer as GlyphBuffer};

use crate::style::{ColorLike, Cursor};
use crate::ui::Rect;
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, InlineMeasureContext,
    InlineNodeSize, InlinePlacement, LayoutConstraints, LayoutPlacement, Layoutable, Position,
    Renderable, Size, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::font_system::with_shared_font_system;
use crate::view::layout::LayoutState;
use crate::view::node_arena::NodeKey;
use crate::view::render_pass::TextPass;
use crate::view::render_pass::text_pass::{TextInput, TextOutput, TextPassFragment, TextPassParams};
use crate::view::text_layout::{build_text_buffer, measure_buffer_size};

use super::super::next_ui_node_id;
use super::edit::byte_index_at_char;

/// IME preedit overlay routed in by the owning `TextArea` when the cursor
/// falls inside this run. Run reshapes with the preedit text spliced in at
/// `insert_at_local`.
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
    /// `text` is the visible content of one paragraph; the trailing `\n`
    /// boundary (if any) is *not* in `text` but IS counted in `char_range`
    /// (so this Run claims that newline char in the global content's char
    /// space). When set, `get_inline_nodes_size` reports an extra
    /// zero-height force-wrap fragment so the inline solver advances
    /// to the next line, mirroring CSS `\n` behavior without us having
    /// to fragment cosmic-text shape across multiple buffers.
    pub(crate) has_trailing_newline: bool,

    // style cascaded from owning TextArea
    pub(crate) font_families: Vec<String>,
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) font_weight: u16,
    pub(crate) color: crate::style::Color,
    pub(crate) cursor: Cursor,
    pub(crate) auto_wrap: bool,
    pub(crate) vertical_align: crate::style::VerticalAlign,

    // IME preedit overlay (TextArea routes via set_inline_preedit)
    pub(crate) inline_preedit: Option<InlinePreedit>,

    // shape state
    glyph_buffer: Option<Arc<GlyphBuffer>>,
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
            has_trailing_newline: false,
            font_families: Vec::new(),
            font_size: 14.0,
            line_height: 1.25,
            font_weight: 400,
            color: crate::style::Color::rgba(17, 17, 17, 255),
            cursor: Cursor::Text,
            auto_wrap: true,
            vertical_align: crate::style::VerticalAlign::Baseline,
            inline_preedit: None,
            glyph_buffer: None,
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
        self.invalidate_shape();
    }

    pub(crate) fn set_text(&mut self, text: String, char_range: Range<usize>) {
        if self.text == text && self.char_range == char_range {
            return;
        }
        self.text = text;
        self.char_range = char_range;
        self.invalidate_shape();
    }

    pub(crate) fn set_has_trailing_newline(&mut self, has_trailing_newline: bool) {
        if self.has_trailing_newline == has_trailing_newline {
            return;
        }
        self.has_trailing_newline = has_trailing_newline;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    /// Cascade-style cascaded set: owner TextArea calls this after edit/
    /// content-rebuild so the run picks up the up-to-date inherited values.
    pub(crate) fn cascade_style(
        &mut self,
        font_families: Vec<String>,
        font_size: f32,
        line_height: f32,
        font_weight: u16,
        color: crate::style::Color,
        cursor: Cursor,
        auto_wrap: bool,
    ) {
        let shape_changed = self.font_families != font_families
            || self.font_size != font_size
            || self.line_height != line_height
            || self.font_weight != font_weight
            || self.color != color
            || self.auto_wrap != auto_wrap;
        self.font_families = font_families;
        self.font_size = font_size;
        self.line_height = line_height;
        self.font_weight = font_weight;
        self.color = color;
        self.cursor = cursor;
        self.auto_wrap = auto_wrap;
        if shape_changed {
            self.invalidate_shape();
        }
    }

    fn invalidate_shape(&mut self) {
        self.glyph_buffer = None;
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

    fn shape_to_buffer(&self, max_width: Option<f32>) -> GlyphBuffer {
        with_shared_font_system(|font_system| {
            build_text_buffer(
                font_system,
                &self.effective_text(),
                max_width,
                None,
                self.auto_wrap,
                self.font_size,
                self.line_height,
                self.font_weight,
                Align::Left,
                &self.font_families,
            )
        })
    }

    /// `local_char` here is in the run's *own* char index
    /// (0..self.text.chars().count()). Returns `(x, y_top, line_height)`
    /// in run-local coordinates.
    ///
    /// cosmic-text glyph byte offsets are **per-paragraph (line_i) local**,
    /// against the *shaped* text (which is `effective_text` — preedit
    /// spliced in). We translate the plain-text local_char to the matching
    /// byte inside `effective_text`, then hunt the layout_run.
    pub fn local_char_to_screen_position(&self, local_char: usize) -> Option<(f32, f32, f32)> {
        let effective = self.effective_text();
        let target_byte = self.plain_local_char_to_effective_byte(local_char, &effective);
        self.screen_position_for_byte_in(target_byte, &effective)
    }

    /// Caret position when the IME preedit is open inside this Run. Honors
    /// the preedit's own caret (`preedit_cursor`) so the visible caret sits
    /// inside the composing text rather than at the splice point — mirrors
    /// v1's `preedit_fragment_caret_screen_position`.
    pub fn preedit_caret_local_position(&self) -> Option<(f32, f32, f32)> {
        let preedit = self.inline_preedit.as_ref()?;
        let effective = self.effective_text();
        let pre_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
        let caret_byte_in_preedit = match preedit.preedit_cursor {
            Some((_, end)) => clamp_utf8_boundary(&preedit.preedit_text, end),
            None => preedit.preedit_text.len(),
        };
        let target_byte = pre_byte + caret_byte_in_preedit;
        self.screen_position_for_byte_in(target_byte, &effective)
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

    fn screen_position_for_byte_in(
        &self,
        target_byte: usize,
        text: &str,
    ) -> Option<(f32, f32, f32)> {
        let buffer = self.glyph_buffer.as_ref()?;
        let line_height = buffer.metrics().line_height;
        let (target_line_i, target_local_byte) = paragraph_line_and_offset(text, target_byte);

        let mut last_paragraph_run_x = 0.0_f32;
        let mut last_paragraph_run_top = 0.0_f32;
        let mut had_paragraph_run = false;
        for run in buffer.layout_runs() {
            if run.line_i != target_line_i {
                continue;
            }
            had_paragraph_run = true;
            last_paragraph_run_top = run.line_top;
            last_paragraph_run_x = run
                .glyphs
                .last()
                .map(|g| g.x + g.w.max(0.0))
                .unwrap_or(0.0);
            for glyph in run.glyphs.iter() {
                if glyph.start >= target_local_byte {
                    return Some((glyph.x, run.line_top, line_height));
                }
            }
        }
        if had_paragraph_run {
            // Cursor sits at the end of this paragraph (or on a wrapped
            // tail with no glyph at exactly that index).
            return Some((last_paragraph_run_x, last_paragraph_run_top, line_height));
        }
        // Empty paragraph (no layout runs for this line_i) — pin to the
        // y of the next paragraph's first run if any, else fall back.
        let mut next_top: Option<f32> = None;
        for run in buffer.layout_runs() {
            if run.line_i > target_line_i {
                next_top = Some(run.line_top);
                break;
            }
        }
        let y_top = next_top.unwrap_or_else(|| {
            buffer
                .layout_runs()
                .last()
                .map(|r| r.line_top + line_height)
                .unwrap_or(0.0)
        });
        Some((0.0, y_top - line_height, line_height))
    }

    /// Hit-test: run-local (x, y) → char index in `effective_text`
    /// (i.e. the spliced text the buffer was actually shaped from). When
    /// no preedit is active this matches `self.text`; with preedit, the
    /// returned index counts preedit chars too. Callers in commit-tap
    /// flows commit the preedit first, after which `self.content` matches
    /// the effective text for this Run, so the index maps directly to
    /// the post-commit content char index.
    ///
    /// `Buffer::hit` returns a `Cursor` whose `index` is **paragraph-local**
    /// byte offset under `cursor.line` against the shaped text.
    pub fn screen_position_to_local_char(&self, x: f32, y: f32) -> Option<usize> {
        let buffer = self.glyph_buffer.as_ref()?;
        let cursor = buffer.hit(x, y)?;
        let effective = self.effective_text();
        let paragraph_start_byte = paragraph_start_byte(&effective, cursor.line);
        let global_byte = (paragraph_start_byte + cursor.index).min(effective.len());
        let prefix = effective.get(..global_byte)?;
        Some(prefix.chars().count())
    }

    /// Run-local selection range → visual rects (one per visual line covered).
    /// Uses paragraph-local byte filtering against each `layout_run.line_i`,
    /// matching cosmic-text's per-paragraph glyph indexing.
    pub fn local_selection_rects(&self, local_start: usize, local_end: usize) -> Vec<Rect> {
        let Some(buffer) = self.glyph_buffer.as_ref() else {
            return Vec::new();
        };
        let start_char = local_start.min(local_end);
        let end_char = local_start.max(local_end);
        if start_char == end_char {
            return Vec::new();
        }
        let start_byte = byte_index_at_char(&self.text, start_char);
        let end_byte = byte_index_at_char(&self.text, end_char);
        let (start_line_i, start_local) = paragraph_line_and_offset(&self.text, start_byte);
        let (end_line_i, end_local) = paragraph_line_and_offset(&self.text, end_byte);

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
            let mut right: f32 = 0.0;
            for glyph in run.glyphs.iter() {
                if glyph.end <= line_local_start || glyph.start >= line_local_end {
                    continue;
                }
                let gx = glyph.x;
                let gw = glyph.w.max(0.0);
                left = Some(left.map(|cur| cur.min(gx)).unwrap_or(gx));
                right = right.max(gx + gw);
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

    /// Underline rects (run-local coords) covering the active IME preedit
    /// segment. One rect per visual line the preedit spans. Empty when no
    /// preedit is active or shape is stale. 1-px-tall stripes pinned to
    /// the visual-line baseline — matches v1's
    /// `ime_preedit_underline_rects` look.
    pub fn preedit_underline_rects(&self) -> Vec<Rect> {
        let Some(preedit) = self.inline_preedit.as_ref() else {
            return Vec::new();
        };
        if preedit.preedit_text.is_empty() {
            return Vec::new();
        }
        let Some(buffer) = self.glyph_buffer.as_ref() else {
            return Vec::new();
        };
        let effective = self.effective_text();
        let pre_start_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
        let pre_end_byte = pre_start_byte + preedit.preedit_text.len();
        let (start_line_i, start_local) =
            paragraph_line_and_offset(&effective, pre_start_byte);
        let (end_line_i, end_local) = paragraph_line_and_offset(&effective, pre_end_byte);
        let line_height = buffer.metrics().line_height;
        let mut out = Vec::new();
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
            let mut right: f32 = 0.0;
            for glyph in run.glyphs.iter() {
                if glyph.end <= line_local_start || glyph.start >= line_local_end {
                    continue;
                }
                let gx = glyph.x;
                let gw = glyph.w.max(0.0);
                left = Some(left.map(|cur| cur.min(gx)).unwrap_or(gx));
                right = right.max(gx + gw);
            }
            if let Some(left) = left {
                let width = (right - left).max(1.0);
                out.push(Rect {
                    x: left,
                    y: run.line_top + line_height - 1.0,
                    width,
                    height: 1.0,
                });
            }
        }
        out
    }

    /// Number of visual (post-wrap) lines in the shaped buffer. Useful for
    /// vertical caret movement and sticky-x bookkeeping.
    pub fn visual_line_count(&self) -> usize {
        self.glyph_buffer
            .as_ref()
            .map(|b| b.layout_runs().count().max(1))
            .unwrap_or(1)
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
            && self.glyph_buffer.is_some()
        {
            return;
        }

        // Shape budget = container's full inner width, not the line's
        // *remaining* width. Otherwise a Run that can't fit in the
        // remaining slot would shape narrow and produce ugly wraps; with
        // `full_available_width` cosmic-text wraps at the same width as if
        // the Run started on a fresh line, and the inline solver places
        // the Run on the next line if needed.
        let max_width = if self.auto_wrap {
            Some(context.full_available_width.max(1.0))
        } else {
            None
        };
        let (width, height) = if self.text.is_empty() && self.inline_preedit.is_none() {
            // Empty paragraph: skip cosmic shaping (which would substitute a
            // space and report a visible glyph width). The Run still claims
            // a `line_height`-tall slot so the inline solver gives it a
            // proper blank line.
            self.glyph_buffer = None;
            (0.0_f32, self.font_size.max(1.0) * self.line_height)
        } else {
            let buffer = self.shape_to_buffer(max_width);
            let (w, h) = measure_buffer_size(&buffer);
            self.glyph_buffer = Some(Arc::new(buffer));
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
        // Block-level measurement falls back to inline shape with the
        // available width as the wrap budget.
        self.measure_inline(
            InlineMeasureContext {
                first_available_width: constraints.max_width,
                full_available_width: constraints.max_width,
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
        if placement.node_index != 0 {
            return; // single-fragment run; ignore extra slots.
        }
        let x = placement.x;
        let y = placement.y;
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
        // Single fragment per Run. Trailing `\n` is signaled via
        // `force_break_after` so the inline solver wraps the next sibling
        // even when its `solver_wrap` (soft overflow wrap) is off.
        // Baseline: per `docs/design/inline-baseline.md` D1, atomic
        // multi-visual-line runs report the first visual line baseline.
        // Empty paragraph (no shaped buffer) falls back to a synthesized
        // baseline using the line-height / font-size leading split, so
        // blank lines still align to surrounding text.
        let baseline = if let Some(buffer) = self.glyph_buffer.as_ref() {
            buffer
                .layout_runs()
                .next()
                .map(|run| (run.line_y - run.line_top).max(0.0))
                .unwrap_or(0.0)
        } else {
            let font_size = self.font_size.max(1.0);
            let line_height = font_size * self.line_height.max(0.8);
            // Approximate font ascent at ~0.8 of em (cosmic-text reports
            // ~0.78–0.83 for typical fonts); empty paragraphs only need a
            // reasonable baseline so the empty line shares a vertical
            // anchor with adjacent text runs.
            let approx_ascent = font_size * 0.8;
            let leading = (line_height - font_size).max(0.0);
            (approx_ascent + leading / 2.0).max(0.0)
        };
        vec![InlineNodeSize {
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
            baseline,
            vertical_align: self.vertical_align,
            force_break_after: self.has_trailing_newline,
        }]
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (
            self.layout_state.layout_position.x,
            self.layout_state.layout_position.y,
        )
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
        let Some(buffer) = self.glyph_buffer.as_ref().cloned() else {
            return ctx.into_state();
        };
        let Some(input_target) = ctx.current_target() else {
            return ctx.into_state();
        };

        let fragment = TextPassFragment {
            content: self.effective_text(),
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width.max(1.0),
            height: self.layout_state.layout_size.height.max(1.0),
            color: self.color.to_rgba_f32(),
            opacity: 1.0,
            layout_buffer: Some(buffer),
        };
        let pass = TextPass::new(
            TextPassParams::single_fragment(
                fragment,
                self.font_size,
                self.line_height,
                self.font_weight,
                self.font_families.clone(),
                Align::Left,
                self.auto_wrap,
                None,
                None,
            ),
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

impl EventTarget for TextAreaTextRun {
    fn cursor(&self) -> Cursor {
        self.cursor
    }
}

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
        self.layout_state.layout_position.x.to_bits().hash(&mut hasher);
        self.layout_state.layout_position.y.to_bits().hash(&mut hasher);
        self.layout_state.layout_size.width.max(0.0).to_bits().hash(&mut hasher);
        self.layout_state.layout_size.height.max(0.0).to_bits().hash(&mut hasher);
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
        hasher.finish()
    }
}

/// Translate a global byte offset within `text` to
/// `(paragraph_line_i, line_local_byte)` — matching the indexing used by
/// cosmic-text's `BufferLine` / `LayoutGlyph`. Paragraphs are split by
/// `\n`; the newline byte itself is treated as the end of the preceding
/// paragraph (target_byte == newline_byte → end of paragraph above).
fn paragraph_line_and_offset(text: &str, target_byte: usize) -> (usize, usize) {
    let target = target_byte.min(text.len());
    let mut line_i = 0usize;
    let mut line_start = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < target {
        if bytes[i] == b'\n' {
            line_i += 1;
            line_start = i + 1;
        }
        i += 1;
    }
    (line_i, target - line_start)
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

/// Global byte offset where paragraph `line_i` starts within `text`.
fn paragraph_start_byte(text: &str, line_i: usize) -> usize {
    if line_i == 0 {
        return 0;
    }
    let mut count = 0usize;
    for (byte, ch) in text.char_indices() {
        if ch == '\n' {
            count += 1;
            if count == line_i {
                return byte + 1;
            }
        }
    }
    text.len()
}
