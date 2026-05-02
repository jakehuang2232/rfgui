//! Children rebuild on edit.
//!
//! Two paths:
//! * **Fast in-place path** — no `on_render` handler (or handler produces
//!   zero projections). The single existing `TextAreaTextRun` child is
//!   updated in place (preserving its `NodeKey`), or, on the empty →
//!   non-empty transition, a fresh Run is committed.
//! * **Full rebuild path (P5)** — handler set and producing projections.
//!   Calls handler → normalizes overlaps → slices content into mixed
//!   `(plain | projection)` segments → naively tears down current children
//!   and commits the new mix. Projection segments wrap the user's
//!   `RsxNode` in a `<Provider<TextAreaImeContext>>` when the caret falls
//!   inside that range, so projection-internal widgets can read preedit
//!   via `use_context::<TextAreaImeContext>()`.
//!
//! P5 deliberately keeps the rebuild *naive* (砍重建): user state inside
//! projection subtrees is lost on every edit. P6 will land the
//! range-delta / signature-keyed reconcile that preserves it.

#![allow(dead_code)]

use std::ops::Range;

use crate::ui::RsxNode;
use crate::view::base_component::{
    ElementTrait, TextAreaRenderProjection, TextAreaRenderString,
};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::TextArea2;
use super::ime_context::TextAreaImeContext;
use super::run::{InlinePreedit, TextAreaTextRun};

/// One slot in the post-slice children list.
enum Segment {
    Plain {
        /// One paragraph's visible text. **Never contains `\n`.** A
        /// trailing newline (if any) is recorded in `has_trailing_newline`
        /// and counted in `range` (so this segment's char range claims
        /// the boundary `\n` char).
        text: String,
        range: Range<usize>,
        is_placeholder: bool,
        has_trailing_newline: bool,
    },
    Projection {
        range: Range<usize>,
        node: RsxNode,
    },
}

impl TextArea2 {
    /// Sync child subtree to current `content` / `placeholder` /
    /// `on_render` projections. Called from `measure()` once per layout
    /// pass when `children_dirty` is set.
    pub(super) fn rebuild_children_if_dirty(
        &mut self,
        arena: &mut NodeArena,
        viewport_width: f32,
        viewport_height: f32,
    ) {
        if !self.children_dirty {
            return;
        }
        self.children_dirty = false;

        let projections = self.collect_normalized_projections();

        // Fast in-place path is only valid for the single-paragraph
        // single-Run case. Multi-paragraph content (contains `\n`) needs
        // the full slice path so each paragraph maps to its own Run.
        let display_has_newline = if self.content.is_empty() {
            self.placeholder.contains('\n')
        } else {
            self.content.contains('\n')
        };
        if projections.is_empty()
            && !display_has_newline
            && self.has_only_single_run(arena)
        {
            self.update_single_run_in_place(arena);
            self.route_preedit_to_runs(arena);
            return;
        }

        self.rebuild_children_full(arena, projections, viewport_width, viewport_height);
        self.route_preedit_to_runs(arena);
    }

    /// True when the current children list is either empty or a single
    /// `TextAreaTextRun` — the only shapes the fast path can update in
    /// place.
    fn has_only_single_run(&self, arena: &NodeArena) -> bool {
        if self.children.is_empty() {
            return true;
        }
        if self.children.len() > 1 {
            return false;
        }
        let key = self.children[0];
        arena
            .with_element_taken_ref(key, |child, _| {
                child.as_any().is::<TextAreaTextRun>()
            })
            .unwrap_or(false)
    }

    /// In-place fast path: no projections, single Run (or empty). Update
    /// or create the Run without touching the arena slot map.
    fn update_single_run_in_place(&mut self, arena: &mut NodeArena) {
        let (display_text, is_placeholder) = self.compute_display_text();
        let char_count = display_text.chars().count();

        if let Some(&run_key) = self.children.first() {
            let cascade_color = if is_placeholder {
                self.placeholder_color
            } else {
                self.color
            };
            let mut updated = false;
            arena.with_element_taken(run_key, |child, _| {
                if let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() {
                    run.is_placeholder = is_placeholder;
                    run.set_text(display_text.clone(), 0..char_count);
                    run.cascade_style(
                        self.font_families.clone(),
                        self.font_size,
                        self.line_height,
                        self.font_weight,
                        cascade_color,
                        self.auto_wrap,
                    );
                    updated = true;
                }
            });
            if updated {
                self.child_char_ranges = vec![0..char_count];
                return;
            }
            // Existing child wasn't a Run: drop into full path.
        }

        // Empty → non-empty (no Run yet, but we now need one). Mint a
        // fresh Run and parent it to self.
        if !display_text.is_empty()
            && let Some(self_key) = self.self_node_key
        {
            let run_key = self.commit_run_segment(
                arena,
                self_key,
                display_text,
                0..char_count,
                is_placeholder,
                false,
            );
            self.children = vec![run_key];
            self.child_char_ranges = vec![0..char_count];
            arena.set_children(self_key, self.children.clone());
            return;
        }

        // Both content + placeholder empty: clear everything.
        if display_text.is_empty() {
            for &k in &self.children.clone() {
                arena.remove_subtree(k);
            }
            self.children.clear();
            self.child_char_ranges.clear();
            if let Some(self_key) = self.self_node_key {
                arena.set_children(self_key, Vec::new());
            }
        }
    }

    /// Full rebuild path: tear down current children, slice content into
    /// `(plain | projection)` segments, commit fresh subtrees.
    fn rebuild_children_full(
        &mut self,
        arena: &mut NodeArena,
        projections: Vec<TextAreaRenderProjection>,
        viewport_width: f32,
        viewport_height: f32,
    ) {
        let Some(self_key) = self.self_node_key else {
            return;
        };

        // Drop existing subtree(s).
        for k in std::mem::take(&mut self.children) {
            arena.remove_subtree(k);
        }
        self.child_char_ranges.clear();

        let segments = self.slice_into_segments(&projections);

        // Determine the projection that hosts the caret (if any) so we
        // can wrap that projection's RsxNode with the IME context
        // provider before commit.
        let cursor_char = self.cursor_char.min(self.content.chars().count());
        let preedit_active =
            !self.ime_preedit.is_empty() || self.ime_preedit_cursor.is_some();
        let projection_holding_cursor = projections
            .iter()
            .find(|p| cursor_char >= p.range.start && cursor_char < p.range.end)
            .map(|p| p.range.clone());

        let mut new_children = Vec::with_capacity(segments.len());
        let mut new_ranges = Vec::with_capacity(segments.len());

        for segment in segments {
            match segment {
                Segment::Plain {
                    text,
                    range,
                    is_placeholder,
                    has_trailing_newline,
                } => {
                    let key = self.commit_run_segment(
                        arena,
                        self_key,
                        text,
                        range.clone(),
                        is_placeholder,
                        has_trailing_newline,
                    );
                    new_children.push(key);
                    new_ranges.push(range);
                }
                Segment::Projection { range, node } => {
                    let final_node = if preedit_active
                        && projection_holding_cursor
                            .as_ref()
                            .is_some_and(|h| *h == range)
                    {
                        let ctx = TextAreaImeContext {
                            preedit: self.ime_preedit.clone(),
                            preedit_cursor: self.ime_preedit_cursor,
                            local_cursor_in_projection: cursor_char
                                .saturating_sub(range.start),
                        };
                        crate::ui::provide_context_node(ctx, node)
                    } else {
                        node
                    };

                    let Some(key) = self.commit_projection_segment(
                        arena,
                        self_key,
                        new_children.len(),
                        &final_node,
                        viewport_width,
                        viewport_height,
                    ) else {
                        continue;
                    };
                    new_children.push(key);
                    new_ranges.push(range);
                }
            }
        }

        self.children = new_children;
        self.child_char_ranges = new_ranges;
        arena.set_children(self_key, self.children.clone());
    }

    /// Run handler (if set) + normalize overlaps. Returns sorted, disjoint
    /// projections; empty when no handler or handler emits nothing.
    fn collect_normalized_projections(&self) -> Vec<TextAreaRenderProjection> {
        let Some(handler) = self.on_render_handler.as_ref() else {
            return Vec::new();
        };
        let mut render_string = TextAreaRenderString::new(self.content.clone());
        handler.call(&mut render_string);
        normalize_projections(self.content.as_str(), render_string.projections())
    }

    pub(super) fn cursor_is_inside_projection(&self) -> bool {
        let cursor = self.cursor_char.min(self.content.chars().count());
        self.collect_normalized_projections()
            .iter()
            .any(|projection| cursor >= projection.range.start && cursor < projection.range.end)
    }

    /// Walk content [0..N], emit Plain / Projection segments interleaved
    /// against the (sorted, disjoint) projection list. Each Plain is
    /// further split at `\n` boundaries so that no Run carries an
    /// embedded newline — see `Segment::Plain.has_trailing_newline`.
    fn slice_into_segments(
        &self,
        projections: &[TextAreaRenderProjection],
    ) -> Vec<Segment> {
        let total_chars = self.content.chars().count();

        // Empty content + placeholder special case (no projection
        // semantically applies — placeholder is a single decorative Run).
        if self.content.is_empty() {
            return if !self.placeholder.is_empty() {
                let mut out = Vec::new();
                expand_plain_paragraphs(
                    &mut out,
                    self.placeholder.as_str(),
                    0..self.placeholder.chars().count(),
                    true,
                );
                out
            } else {
                Vec::new()
            };
        }

        let mut out = Vec::new();
        let mut cursor = 0_usize;
        for projection in projections {
            let proj_start = projection.range.start.min(total_chars);
            let proj_end = projection.range.end.min(total_chars);
            if proj_end <= cursor || proj_start >= proj_end {
                continue;
            }
            if cursor < proj_start {
                let plain = slice_chars(self.content.as_str(), cursor..proj_start);
                expand_plain_paragraphs(&mut out, &plain, cursor..proj_start, false);
            }
            out.push(Segment::Projection {
                range: proj_start..proj_end,
                node: projection.node.clone(),
            });
            cursor = proj_end;
        }
        if cursor < total_chars {
            let plain = slice_chars(self.content.as_str(), cursor..total_chars);
            expand_plain_paragraphs(&mut out, &plain, cursor..total_chars, false);
        }
        out
    }

    /// Build + commit a fresh `TextAreaTextRun` under `parent_key`,
    /// returning the new NodeKey. Cascades current text style.
    fn commit_run_segment(
        &self,
        arena: &mut NodeArena,
        parent_key: NodeKey,
        text: String,
        range: Range<usize>,
        is_placeholder: bool,
        has_trailing_newline: bool,
    ) -> NodeKey {
        let cascade_color = if is_placeholder {
            self.placeholder_color
        } else {
            self.color
        };
        let mut run = TextAreaTextRun::new(text, range);
        run.is_placeholder = is_placeholder;
        run.has_trailing_newline = has_trailing_newline;
        run.cascade_style(
            self.font_families.clone(),
            self.font_size,
            self.line_height,
            self.font_weight,
            cascade_color,
            self.auto_wrap,
        );
        let desc = crate::view::renderer_adapter::ElementDescriptor::leaf(
            Box::new(run) as Box<dyn ElementTrait>,
        );
        crate::view::renderer_adapter::commit_descriptor_tree(arena, Some(parent_key), desc)
    }

    /// Convert a projection RsxNode into descriptors and commit them under
    /// `parent_key`. Multi-root projections are wrapped in a transparent
    /// inline Element so the projection still presents as a single child
    /// of TextArea2 (mirroring v1's `wrap_projection_children_desc`).
    fn commit_projection_segment(
        &self,
        arena: &mut NodeArena,
        parent_key: NodeKey,
        segment_index: usize,
        node: &RsxNode,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<NodeKey> {
        let scope = [self.stable_id(), 0x5445_5832, segment_index as u64];
        let inherited_style = self.projection_inherited_style();
        let mut children = match descriptors_unwrap_providers(
            node,
            &scope,
            &inherited_style,
            viewport_width,
            viewport_height,
        ) {
            Ok(c) => c,
            Err(_) => return None,
        };
        if children.is_empty() {
            return None;
        }
        let desc = if children.len() == 1 {
            children.remove(0)
        } else {
            wrap_projection_children(self.stable_id(), segment_index, children)
        };
        Some(crate::view::renderer_adapter::commit_descriptor_tree(
            arena,
            Some(parent_key),
            desc,
        ))
    }

    /// Inherited style cascaded into projection child subtrees: font /
    /// color from TextArea2 itself. Mirrors v1.
    fn projection_inherited_style(&self) -> crate::style::Style {
        use crate::style::{
            FontFamily, FontSize, FontWeight, ParsedValue, PropertyId, Style,
        };
        let mut style = Style::new();
        if !self.font_families.is_empty() {
            style.insert(
                PropertyId::FontFamily,
                ParsedValue::FontFamily(FontFamily::new(self.font_families.clone())),
            );
        }
        style.insert(
            PropertyId::FontSize,
            ParsedValue::FontSize(FontSize::px(self.font_size)),
        );
        style.insert(
            PropertyId::FontWeight,
            ParsedValue::FontWeight(FontWeight::new(self.font_weight)),
        );
        style.insert(PropertyId::Color, ParsedValue::Color(self.color.into()));
        style
    }

    fn compute_display_text(&self) -> (String, bool) {
        if !self.content.is_empty() {
            (self.content.clone(), false)
        } else if !self.placeholder.is_empty() {
            (self.placeholder.clone(), true)
        } else {
            (String::new(), false)
        }
    }

    /// Push the current `ime_preedit` / `ime_preedit_cursor` into the Run
    /// child whose `char_range` covers `cursor_char`; clear preedit on
    /// every other Run. When the cursor sits inside a projection segment
    /// the IME context is routed via `<Provider<TextAreaImeContext>>`
    /// during rebuild instead — every Run gets its preedit cleared here.
    pub(super) fn route_preedit_to_runs(&self, arena: &NodeArena) {
        let preedit_active = !self.ime_preedit.is_empty()
            || self.ime_preedit_cursor.is_some();
        let cursor_char = self.cursor_char;
        let preedit_text = self.ime_preedit.clone();
        let preedit_cursor = self.ime_preedit_cursor;

        // Locate whether the cursor sits inside a projection. In that case
        // TextArea2 does not draw preedit text through an adjacent Run; the
        // projection owns text rendering via `TextAreaImeContext`, while
        // TextArea2 only draws IME underline overlay in render.rs.
        let mut cursor_in_projection = false;
        for (range, &key) in self
            .child_char_ranges
            .iter()
            .zip(self.children.iter())
        {
            if cursor_char < range.start || cursor_char >= range.end {
                continue;
            }
            cursor_in_projection = arena
                .with_element_taken_ref(key, |child, _| {
                    child.as_any().downcast_ref::<TextAreaTextRun>().is_none()
                })
                .unwrap_or(false);
            break;
        }

        let run_range = |i: usize| -> Option<Range<usize>> {
            let key = self.children.get(i).copied()?;
            arena
                .with_element_taken_ref(key, |child, _| {
                    child
                        .as_any()
                        .downcast_ref::<TextAreaTextRun>()
                        .map(|run| run.char_range.clone())
                })
                .flatten()
        };

        let mut target_idx_local: Option<(usize, usize)> = None;
        let mut last_run_idx: Option<usize> = None;
        if target_idx_local.is_none() && !cursor_in_projection {
            for (i, &child_key) in self.children.iter().enumerate() {
                let range = arena
                    .with_element_taken_ref(child_key, |child, _| {
                        child
                            .as_any()
                            .downcast_ref::<TextAreaTextRun>()
                            .map(|run| run.char_range.clone())
                    })
                    .flatten();
                let Some(range) = range else {
                    continue;
                };
                last_run_idx = Some(i);
                if target_idx_local.is_none() && cursor_char < range.end {
                    let local = cursor_char.saturating_sub(range.start)
                        .min(range.end.saturating_sub(range.start));
                    target_idx_local = Some((i, local));
                }
            }
            if preedit_active && target_idx_local.is_none()
                && let Some(i) = last_run_idx
                && let Some(range) = run_range(i)
            {
                target_idx_local = Some((i, range.end.saturating_sub(range.start)));
            }
        }

        for (i, &child_key) in self.children.iter().enumerate() {
            let is_target = preedit_active && target_idx_local.map(|(t, _)| t) == Some(i);
            let local = target_idx_local.filter(|(t, _)| *t == i).map(|(_, l)| l);
            let pe_text = preedit_text.clone();
            arena.with_element_taken_ref(child_key, |child, _| {
                let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() else {
                    return;
                };
                if is_target {
                    let len = run
                        .char_range
                        .end
                        .saturating_sub(run.char_range.start);
                    let insert_at_local = local.unwrap_or(0).min(len);
                    run.set_inline_preedit(Some(InlinePreedit {
                        insert_at_local,
                        preedit_text: pe_text,
                        preedit_cursor,
                    }));
                } else {
                    run.set_inline_preedit(None);
                }
            });
        }
    }
}

/// Re-exported v1 normalize behavior, ported to v2 (sort + later-wins
/// overlap resolution). Unlike v1 this keeps the user's `RsxNode` intact:
/// re-slicing only narrows the `range`, never rewrites the inner node.
fn normalize_projections(
    content: &str,
    projections: &[TextAreaRenderProjection],
) -> Vec<TextAreaRenderProjection> {
    let total = content.chars().count();
    let mut sorted: Vec<TextAreaRenderProjection> = projections
        .iter()
        .filter_map(|p| {
            let start = p.range.start.min(total);
            let end = p.range.end.min(total);
            if end <= start {
                None
            } else {
                Some(TextAreaRenderProjection {
                    range: start..end,
                    node: p.node.clone(),
                })
            }
        })
        .collect();
    sorted.sort_by_key(|p| p.range.start);

    let mut normalized: Vec<TextAreaRenderProjection> = Vec::new();
    for projection in sorted {
        let mut next: Vec<TextAreaRenderProjection> = Vec::new();
        for existing in normalized {
            next.extend(subtract_overlap(existing, &projection.range));
        }
        next.push(projection);
        normalized = next;
    }
    normalized.sort_by_key(|p| p.range.start);
    normalized
}

fn subtract_overlap(
    projection: TextAreaRenderProjection,
    covering: &Range<usize>,
) -> Vec<TextAreaRenderProjection> {
    if covering.end <= projection.range.start || covering.start >= projection.range.end {
        return vec![projection];
    }
    let mut out = Vec::new();
    if projection.range.start < covering.start {
        out.push(TextAreaRenderProjection {
            range: projection.range.start..covering.start.min(projection.range.end),
            node: projection.node.clone(),
        });
    }
    if projection.range.end > covering.end {
        out.push(TextAreaRenderProjection {
            range: covering.end.max(projection.range.start)..projection.range.end,
            node: projection.node.clone(),
        });
    }
    out
}

fn slice_chars(s: &str, range: Range<usize>) -> String {
    s.chars().skip(range.start).take(range.end - range.start).collect()
}

/// Split `text` (covering global char range `range`) at `\n` boundaries
/// and append one `Segment::Plain` per paragraph. Each Plain's
/// `has_trailing_newline` reflects whether the source had a `\n` at that
/// paragraph's end; the `\n` char itself is counted in the segment's
/// `range.end`. Empty paragraphs (consecutive `\n`s, or trailing `\n`)
/// produce empty-text Runs that still claim the `\n` boundary.
fn expand_plain_paragraphs(
    out: &mut Vec<Segment>,
    text: &str,
    range: Range<usize>,
    is_placeholder: bool,
) {
    if text.is_empty() {
        return;
    }
    let mut paragraph_chars: Vec<char> = Vec::new();
    let mut paragraph_start = range.start;
    let mut char_index = range.start;
    for ch in text.chars() {
        if ch == '\n' {
            let para_end_excl_nl = char_index;
            let para_end_incl_nl = char_index + 1;
            out.push(Segment::Plain {
                text: paragraph_chars.iter().collect(),
                range: paragraph_start..para_end_incl_nl,
                is_placeholder,
                has_trailing_newline: true,
            });
            let _ = para_end_excl_nl;
            paragraph_chars.clear();
            paragraph_start = para_end_incl_nl;
            char_index += 1;
        } else {
            paragraph_chars.push(ch);
            char_index += 1;
        }
    }
    // Final paragraph (no trailing `\n` in source).
    if !paragraph_chars.is_empty() || paragraph_start == range.start {
        out.push(Segment::Plain {
            text: paragraph_chars.iter().collect(),
            range: paragraph_start..char_index,
            is_placeholder,
            has_trailing_newline: false,
        });
    }
}

/// Walk leading `RsxNode::Provider` wrapper(s), pushing each
/// `(type_id, value)` onto `CONTEXT_STACK` for the duration of the
/// descriptor build, then convert the unwrapped child via the standard
/// scoped converter. The `rsx_to_descriptors_*` walker rejects
/// `Provider` variants, so providers added by v2 itself (e.g. the IME
/// context wrap in `rebuild_children_full`) must be dissolved here.
/// Mirrors `unwrap_components`'s Provider handling.
fn descriptors_unwrap_providers(
    node: &RsxNode,
    scope: &[u64],
    inherited_style: &crate::style::Style,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<Vec<crate::view::renderer_adapter::ElementDescriptor>, String> {
    if let RsxNode::Provider(provider) = node {
        crate::ui::with_pushed_context_raw(
            provider.type_id,
            std::rc::Rc::clone(&provider.value),
            || {
                descriptors_unwrap_providers(
                    &provider.child,
                    scope,
                    inherited_style,
                    viewport_width,
                    viewport_height,
                )
            },
        )
    } else {
        crate::view::renderer_adapter::rsx_to_descriptors_scoped_with_context(
            node,
            scope,
            inherited_style,
            viewport_width,
            viewport_height,
        )
    }
}

/// Wrap multi-root projection descriptors in an inline-row Element so the
/// projection presents as a single child of TextArea2.
fn wrap_projection_children(
    text_area_stable_id: u64,
    segment_index: usize,
    children: Vec<crate::view::renderer_adapter::ElementDescriptor>,
) -> crate::view::renderer_adapter::ElementDescriptor {
    use crate::style::{Layout, ParsedValue, PropertyId, Style};
    use crate::view::base_component::Element;

    let wrapper_id = text_area_stable_id
        .wrapping_mul(1_000_003)
        .wrapping_add(segment_index as u64 + 1);
    let mut wrapper = Element::new_with_id(wrapper_id, 0.0, 0.0, 0.0, 0.0);
    wrapper.set_intrinsic_size_as_percent_base(false);
    let mut style = Style::new();
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
    );
    style.insert(PropertyId::Width, ParsedValue::Auto);
    style.insert(PropertyId::Height, ParsedValue::Auto);
    wrapper.apply_style(style);

    crate::view::renderer_adapter::ElementDescriptor {
        element: Box::new(wrapper) as Box<dyn ElementTrait>,
        children,
        post_commit: None,
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for `commit_projection_segment`'s Provider unwrap.
    //!
    //! P5 wraps a projection's `RsxNode` in `<Provider<TextAreaImeContext>>`
    //! when the caret falls inside that projection while preedit is active.
    //! The descriptor walker rejects `RsxNode::Provider`, so the original
    //! P5 commit silently dropped the segment in this case
    //! (returning `None` and skipping it). These tests pin the unwrap path
    //! that dissolves the Provider into a `CONTEXT_STACK` push for the
    //! duration of the descriptor build.
    use crate::style::Length;
    use crate::ui::{RsxNode, RsxTagDescriptor};
    use crate::view::ElementStylePropSchema;
    use crate::view::base_component::{
        Element, ElementTrait, LayoutConstraints, LayoutPlacement, Text, TextArea2,
    };
    use crate::view::node_arena::{NodeArena, NodeKey};

    fn fixture_with_caret_in_projection(
        ime_preedit: &str,
        ime_preedit_cursor: Option<(usize, usize)>,
    ) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea2::new();
        text_area.content = "abXYZcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        // Caret 3 falls inside the projection range 2..5.
        text_area.cursor_char = 3;
        text_area.ime_preedit = ime_preedit.to_string();
        text_area.ime_preedit_cursor = ime_preedit_cursor;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..5, |_text_area_node| {
                let style = ElementStylePropSchema {
                    width: Some(Length::px(90.0)),
                    height: Some(Length::px(42.0)),
                    ..Default::default()
                };
                RsxNode::tagged("Element", RsxTagDescriptor::of::<Element>())
                    .with_prop("style", style)
                    .with_child(
                        RsxNode::tagged("Text", RsxTagDescriptor::of::<Text>())
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
                .downcast_mut::<TextArea2>()
                .expect("TextArea2 root")
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
                parent_x: 0.0,
                parent_y: 0.0,
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

    /// Caret-in-projection + preedit active triggers the
    /// `provide_context_node` wrap. With the unwrap fix the projection
    /// segment commits as expected: 3 children (Run "ab" / projection
    /// Element / Run "cd") and the projection holds an `<Element>` node,
    /// not a missing slot.
    #[test]
    fn projection_commits_when_caret_inside_with_preedit() {
        let (arena, root) =
            fixture_with_caret_in_projection("\u{4E2D}", Some((1, 1)));

        let children = arena.children_of(root);
        assert_eq!(
            children.len(),
            3,
            "expected 3 children (Run / projection / Run); got {}",
            children.len(),
        );

        let projection_key = children[1];
        let is_element = arena
            .with_element_taken_ref(projection_key, |el, _| el.as_any().is::<Element>())
            .unwrap_or(false);
        assert!(
            is_element,
            "projection slot should hold an Element (the projection root)",
        );
        assert!(
            !arena.children_of(projection_key).is_empty(),
            "projection Element should have its descriptor children committed",
        );

        let text_key = first_text_descendant(&arena, projection_key);
        let text_content = arena
            .with_element_taken_ref(text_key, |el, _| {
                el.as_any()
                    .downcast_ref::<Text>()
                    .expect("projection Text")
                    .content()
                    .to_string()
            })
            .expect("text exists");
        assert_eq!(text_content, "X\u{4E2D}YZ");
    }

    /// Sanity baseline: same fixture without preedit (no Provider wrap).
    /// Ensures the test isn't passing for an unrelated reason — both
    /// shapes should produce the same 3-child arena layout.
    #[test]
    fn projection_commits_when_caret_inside_without_preedit() {
        let (arena, root) = fixture_with_caret_in_projection("", None);
        assert_eq!(arena.children_of(root).len(), 3);
    }

    /// Caret inside a projection segment with preedit active should not
    /// route preedit text onto adjacent Runs. The projection owns text
    /// rendering via `TextAreaImeContext`; TextArea2 only draws the IME
    /// underline overlay in render.rs.
    #[test]
    fn projection_preedit_does_not_route_to_adjacent_run_when_caret_in_projection() {
        let (arena, root) =
            fixture_with_caret_in_projection("\u{4E2D}\u{6587}", Some((2, 2)));

        let children = arena.children_of(root);
        assert_eq!(children.len(), 3, "expected Run / projection / Run");

        let preceding_pe = run_inline_preedit(&arena, children[0]);
        let following_pe = run_inline_preedit(&arena, children[2]);

        assert!(
            preceding_pe.is_none(),
            "Run before projection should not host the preedit; got {preceding_pe:?}",
        );
        assert!(
            following_pe.is_none(),
            "Run after projection should not host the preedit; got {following_pe:?}",
        );
    }

    fn run_inline_preedit(
        arena: &NodeArena,
        key: NodeKey,
    ) -> Option<crate::view::base_component::text_area_v2::run::InlinePreedit> {
        arena
            .with_element_taken_ref(key, |child, _| {
                child
                    .as_any()
                    .downcast_ref::<crate::view::base_component::text_area_v2::TextAreaTextRun>()
                    .and_then(|run| run.inline_preedit.clone())
            })
            .flatten()
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
}
