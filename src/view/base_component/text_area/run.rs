//! `TextAreaTextRun` — internal plain-text segment child of `TextArea`.
//!
//! The run only sizes itself (via the shared IFC-backed measure cache) and
//! carries segment/preedit metadata. Glyph rendering and all caret /
//! selection / underline geometry live in the owning TextArea's unified
//! inline IFC package.
//!
//! See `docs/design/textarea-v2.md` (Phase 2) for the role of this
//! component within the v2 inline pipeline.

#![allow(dead_code)]

use std::{borrow::Cow, ops::Range};

use crate::style::Cursor;
use crate::ui::Rect;
use crate::view::base_component::text::measure_text_layout;
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, Position, Renderable, Size, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::inline_formatting_context::InlineIfcAlignment;
use crate::view::layout::LayoutState;
use crate::view::node_arena::NodeKey;

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

    // measure state: the constraint the current layout_size was measured
    // at. Caret/selection geometry lives in the owning TextArea's unified
    // IFC package; the run only sizes itself.
    last_measure_width: Option<Option<f32>>,

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
            last_measure_width: None,
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
        self.mark_measure_dirty();
    }

    pub(crate) fn set_preedit_run(&mut self, is_preedit_run: bool, cursor: Option<(usize, usize)>) {
        if self.is_preedit_run == is_preedit_run && self.preedit_cursor == cursor {
            return;
        }
        self.is_preedit_run = is_preedit_run;
        self.preedit_cursor = cursor;
        self.mark_measure_dirty();
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
        self.mark_measure_dirty();
    }

    /// Cascade-style cascaded set: owner TextArea calls this after edit/
    /// content-rebuild so the run picks up the up-to-date inherited values.
    pub(crate) fn cascade_style(&mut self, style: TextAreaRunStyle<'_>) {
        let font_families_changed = self.font_families.as_slice() != style.font_families;
        let layout_changed = font_families_changed
            || self.font_size != style.font_size
            || self.line_height != style.line_height
            || self.vertical_align != style.vertical_align
            || self.font_weight != style.font_weight
            || self.color != style.color
            || self.auto_wrap != style.auto_wrap;
        if font_families_changed {
            self.font_families = style.font_families.to_vec();
        }
        self.font_size = style.font_size;
        self.line_height = style.line_height;
        self.vertical_align = style.vertical_align;
        self.font_weight = style.font_weight;
        self.color = style.color;
        self.cursor = style.cursor;
        self.auto_wrap = style.auto_wrap;
        if layout_changed {
            self.mark_measure_dirty();
        }
    }

    fn mark_measure_dirty(&mut self) {
        self.last_measure_width = None;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    /// Effective text including any spliced IME preedit segment.
    pub(crate) fn effective_text(&self) -> Cow<'_, str> {
        match &self.inline_preedit {
            None => Cow::Borrowed(&self.text),
            Some(preedit) => {
                let local_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
                let mut out = String::with_capacity(self.text.len() + preedit.preedit_text.len());
                out.push_str(&self.text[..local_byte]);
                out.push_str(&preedit.preedit_text);
                out.push_str(&self.text[local_byte..]);
                Cow::Owned(out)
            }
        }
    }

    pub(crate) fn effective_preedit_backing_byte_range(
        &self,
        effective_backing_start: usize,
    ) -> Option<Range<usize>> {
        if self.is_preedit_run {
            if self.text.is_empty() {
                return None;
            }
            return Some(effective_backing_start..effective_backing_start + self.text.len());
        }
        let preedit = self.inline_preedit.as_ref()?;
        if preedit.preedit_text.is_empty() {
            return None;
        }
        let local_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
        let start = effective_backing_start + local_byte;
        Some(start..start + preedit.preedit_text.len())
    }

    pub(crate) fn effective_preedit_caret_backing_byte(
        &self,
        effective_backing_start: usize,
    ) -> Option<usize> {
        if self.is_preedit_run {
            let caret_byte = self
                .preedit_cursor
                .map(|(_, end)| clamp_utf8_boundary(&self.text, end))
                .unwrap_or(self.text.len());
            return Some(effective_backing_start + caret_byte);
        }
        let preedit = self.inline_preedit.as_ref()?;
        let local_byte = byte_index_at_char(&self.text, preedit.insert_at_local);
        let caret_byte = preedit
            .preedit_cursor
            .map(|(_, end)| clamp_utf8_boundary(&preedit.preedit_text, end))
            .unwrap_or(preedit.preedit_text.len());
        Some(effective_backing_start + local_byte + caret_byte)
    }
}

/// Run-local caret stop produced by [`TextAreaLineBreak::caret_stops`].
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

#[derive(Clone, Copy)]
pub(crate) struct TextAreaRunStyle<'a> {
    pub(crate) font_families: &'a [String],
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) vertical_align: crate::style::VerticalAlign,
    pub(crate) font_weight: u16,
    pub(crate) color: crate::style::Color,
    pub(crate) cursor: Cursor,
    pub(crate) auto_wrap: bool,
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
    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.measure_generated_text_child(
            constraints.max_width,
            constraints.viewport_width,
            constraints.viewport_height,
            constraints.percent_base_width,
            constraints.percent_base_height,
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

    fn inline_relative_position(&self) -> (f32, f32) {
        (
            self.layout_state.layout_position.x,
            self.layout_state.layout_position.y,
        )
    }
}

impl TextAreaTextRun {
    pub(crate) fn measure_generated_text_child(
        &mut self,
        width: f32,
        _viewport_width: f32,
        _viewport_height: f32,
        _percent_base_width: Option<f32>,
        _percent_base_height: Option<f32>,
    ) {
        let layout_clean = !self.dirty_flags.intersects(DirtyFlags::LAYOUT);
        let measure_width = self.auto_wrap.then_some(width.max(1.0));
        if layout_clean && self.last_measure_width == Some(measure_width) {
            return;
        }

        let (width, height) = if self.text.is_empty() && self.inline_preedit.is_none() {
            // Empty paragraph: skip shaping (which would substitute a
            // space and report a visible glyph width). The Run still claims
            // a `line_height`-tall slot so TextArea gives it a
            // proper blank line. Floor at 0.8 to match every other line-
            // height path (`line_height_px`, the shaped path) so a blank
            // paragraph and a shaped one report the same height.
            (0.0_f32, self.font_size.max(1.0) * self.line_height.max(0.8))
        } else {
            let measured = measure_text_layout(
                self.effective_text().as_ref(),
                measure_width,
                self.auto_wrap,
                self.font_size,
                self.line_height,
                self.font_weight,
                InlineIfcAlignment::Left,
                self.font_families.as_slice(),
            );
            (measured.width, measured.height)
        };
        self.last_measure_width = Some(measure_width);

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
}

impl Layoutable for TextAreaLineBreak {
    fn measure(
        &mut self,
        _constraints: LayoutConstraints,
        _arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.measure_generated_text_child();
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

    fn measured_size(&self) -> (f32, f32) {
        (0.0, self.line_height_px())
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_state.layout_size.width = width.max(0.0);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_state.layout_size.height = height.max(0.0);
    }
}

impl TextAreaLineBreak {
    pub(crate) fn measure_generated_text_child(&mut self) {
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
}

impl Renderable for TextAreaTextRun {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        // TextArea editable glyphs are emitted by the owning TextArea's
        // unified IFC root package. Runs keep layout/audit helpers, but
        // they no longer own a render pass in the editable path.
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
    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        // Conservative: the TextArea family manages an internal projection /
        // IME / caret subtree whose placement is not yet proven stable under
        // ancestor-skip, so it blocks placement-skip for now (preserving the
        // pre-trait behavior). Text/Image/Svg leaves are transparent instead.
        crate::view::node_arena::PlacementEligibilityMetadata::non_base_blocker()
    }

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
    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        // Conservative: the TextArea family manages an internal projection /
        // IME / caret subtree whose placement is not yet proven stable under
        // ancestor-skip, so it blocks placement-skip for now (preserving the
        // pre-trait behavior). Text/Image/Svg leaves are transparent instead.
        crate::view::node_arena::PlacementEligibilityMetadata::non_base_blocker()
    }

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
