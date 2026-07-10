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

use crate::ui::{RsxNode, RsxNodeIdentity};
use crate::view::base_component::{ElementTrait, TextAreaRenderProjection, TextAreaRenderString};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::ime_context::TextAreaImeContext;
use super::run::{InlinePreedit, TextAreaLineBreak, TextAreaTextRun};
use super::{TextArea, TextAreaProjectionSegment};

/// P6 reconcile metadata, one per `TextArea.children[i]`. Parallel to
/// `child_char_ranges`. `Run` is the plain-text path (no user state to
/// preserve). `Projection` remembers the post-Provider-unwrap identity of
/// the projection root plus the last committed `RsxNode` so the next
/// rebuild can identity-match and reconcile in place.
#[derive(Clone, Debug)]
pub(crate) enum ChildSlot {
    Run,
    LineBreak,
    Projection {
        identity: RsxNodeIdentity,
        last_node: RsxNode,
    },
}

/// Strip leading `RsxNode::Provider` wrapper(s) and return the inner node's
/// identity. Used to derive a projection root's reconcile key when the v2
/// pipeline wraps the user node in `<Provider<TextAreaImeContext>>`.
pub(crate) fn projection_root_identity(node: &RsxNode) -> RsxNodeIdentity {
    let mut cursor = node;
    while let RsxNode::Provider(provider) = cursor {
        cursor = &provider.child;
    }
    *cursor.identity()
}

/// One slot in the post-slice children list.
enum Segment {
    Plain {
        /// One paragraph's visible text. **Never contains `\n`.**
        text: String,
        range: Range<usize>,
        is_placeholder: bool,
        is_preedit: bool,
        preedit_cursor: Option<(usize, usize)>,
    },
    LineBreak {
        range: Range<usize>,
    },
    Projection {
        range: Range<usize>,
        node: RsxNode,
    },
}

impl TextArea {
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
        self.bump_unified_ifc_source_revision();

        let projections = self.collect_normalized_projections();

        // Fast in-place path is only valid for the single-paragraph
        // single-Run case. Multi-paragraph content (contains `\n`) needs
        // the full slice path so each paragraph maps to its own Run.
        let preedit_active = !self.ime_preedit.is_empty() || self.ime_preedit_cursor.is_some();
        let display_has_newline = if self.content.is_empty() {
            self.placeholder.contains('\n')
        } else {
            self.content.contains('\n')
        };
        if !preedit_active
            && projections.is_empty()
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
            .with_element_taken_ref(key, |child, _| child.as_any().is::<TextAreaTextRun>())
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
            // Run text feeds the unified IFC source; invalidate the
            // package cache's revision fast path.
            self.bump_unified_ifc_source_revision();
            arena.mutate_element_with_invalidation(run_key, |child, cx| {
                if let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() {
                    run.is_placeholder = is_placeholder;
                    run.set_text(display_text.clone(), 0..char_count);
                    run.cascade_style(self.run_style(cascade_color));
                    cx.invalidate(run.local_dirty_flags());
                    updated = true;
                }
            });
            if updated {
                self.child_char_ranges = vec![0..char_count];
                self.child_slots = vec![ChildSlot::Run];
                return;
            }
            // Existing child wasn't a Run: drop into full path.
        }

        // Empty → non-empty (no Run yet, but we now need one). Mint a
        // fresh Run and parent it to self.
        let preedit_active = !self.ime_preedit.is_empty() || self.ime_preedit_cursor.is_some();
        if (!display_text.is_empty() || preedit_active)
            && let Some(self_key) = self.self_node_key
        {
            let run_key = self.commit_run_segment(
                arena,
                self_key,
                display_text,
                0..char_count,
                is_placeholder,
                false,
                None,
            );
            self.children = vec![run_key];
            self.child_char_ranges = vec![0..char_count];
            self.child_slots = vec![ChildSlot::Run];
            arena.set_children(self_key, self.children.clone());
            return;
        }

        // Both content + placeholder empty and no active preedit: clear everything.
        if display_text.is_empty() && !preedit_active {
            for k in std::mem::take(&mut self.children) {
                arena.remove_subtree(k);
            }
            self.child_char_ranges.clear();
            self.child_slots.clear();
            if let Some(self_key) = self.self_node_key {
                arena.set_children(self_key, Vec::new());
            }
        }
    }

    /// Reconcile children against the new segment list.
    ///
    /// P6 replacement for the original P5 砍重建. Identity-keyed match
    /// against the previous slot list (`child_slots`):
    ///
    /// * **Plain Run** → pop the next existing Run from the FIFO queue
    ///   and update it in place (preserving its `NodeKey`); commit a
    ///   fresh Run when the queue is empty.
    /// * **Projection** → identity-match against the previous projection
    ///   slots (post-Provider-unwrap identity of the projection root).
    ///   Matched slot → `reconcile_existing_subtree`; on success the
    ///   subtree's `NodeKey`s survive, so any user state inside is
    ///   preserved. On `Err` (shape change the wrapper can't apply in
    ///   place) → tear down + commit fresh.
    /// * **Unmatched** → fresh `commit_*_segment`.
    /// * **Leftover old slots** → `arena.remove_subtree`.
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

        let segments = self.slice_into_segments(&projections);

        let cursor_char = self.cursor_char.min(self.content.chars().count());
        let preedit_active = !self.ime_preedit.is_empty() || self.ime_preedit_cursor.is_some();
        let projection_holding_cursor = projections
            .iter()
            .find(|p| cursor_char >= p.range.start && cursor_char < p.range.end)
            .map(|p| p.range.clone());

        // Snapshot the previous slot map so we can look up existing
        // children to reuse. After this we mutate `self.children` /
        // `self.child_slots` freely; leftover old keys are collected
        // and freed at the end.
        let old_children = std::mem::take(&mut self.children);
        let old_slots = std::mem::take(&mut self.child_slots);
        self.child_char_ranges.clear();

        let mut run_queue: std::collections::VecDeque<NodeKey> = std::collections::VecDeque::new();
        let mut line_break_queue: std::collections::VecDeque<NodeKey> =
            std::collections::VecDeque::new();
        let mut proj_buckets: rustc_hash::FxHashMap<RsxNodeIdentity, Vec<(NodeKey, RsxNode)>> =
            rustc_hash::FxHashMap::default();
        for (key, slot) in old_children.iter().zip(old_slots.into_iter()) {
            match slot {
                ChildSlot::Run => run_queue.push_back(*key),
                ChildSlot::LineBreak => line_break_queue.push_back(*key),
                ChildSlot::Projection {
                    identity,
                    last_node,
                } => {
                    proj_buckets
                        .entry(identity)
                        .or_default()
                        .push((*key, last_node));
                }
            }
        }

        let inherited_style = self.projection_inherited_style();
        let apply_ctx = crate::view::fiber_work::ApplyContext {
            viewport_style: &inherited_style,
            viewport_width,
            viewport_height,
        };

        let mut new_children = Vec::with_capacity(segments.len());
        let mut new_ranges = Vec::with_capacity(segments.len());
        let mut new_slots: Vec<ChildSlot> = Vec::with_capacity(segments.len());

        for segment in segments {
            match segment {
                Segment::Plain {
                    text,
                    range,
                    is_placeholder,
                    is_preedit,
                    preedit_cursor,
                } => {
                    let key = match run_queue.pop_front() {
                        Some(existing_key) => {
                            self.update_run_in_place_for_segment(
                                arena,
                                existing_key,
                                &text,
                                range.clone(),
                                is_placeholder,
                                is_preedit,
                                preedit_cursor,
                            );
                            existing_key
                        }
                        None => self.commit_run_segment(
                            arena,
                            self_key,
                            text,
                            range.clone(),
                            is_placeholder,
                            is_preedit,
                            preedit_cursor,
                        ),
                    };
                    new_children.push(key);
                    new_ranges.push(range);
                    new_slots.push(ChildSlot::Run);
                }
                Segment::LineBreak { range } => {
                    let key = match line_break_queue.pop_front() {
                        Some(existing_key) => {
                            self.update_line_break_in_place_for_segment(
                                arena,
                                existing_key,
                                range.clone(),
                            );
                            existing_key
                        }
                        None => self.commit_line_break_segment(arena, self_key, range.clone()),
                    };
                    new_children.push(key);
                    new_ranges.push(range);
                    new_slots.push(ChildSlot::LineBreak);
                }
                Segment::Projection { range, node } => {
                    // P6/M4: always wrap projection segments in a
                    // `<Provider<TextAreaImeContext>>`. Carrying an
                    // empty default ctx when the caret is elsewhere
                    // (or no preedit is active) keeps the wrapper
                    // structurally stable across rebuilds — the
                    // reconcile pass sees the same `Provider→inner`
                    // shape every frame and matches by inner identity
                    // without churn. Cost is one cheap `Rc<dyn Any>`
                    // alloc per projection segment per rebuild.
                    let cursor_in_this_range = projection_holding_cursor
                        .as_ref()
                        .is_some_and(|h| *h == range);
                    let ctx = if preedit_active && cursor_in_this_range {
                        TextAreaImeContext {
                            preedit: self.ime_preedit.clone(),
                            preedit_cursor: self.ime_preedit_cursor,
                            local_cursor_in_projection: cursor_char.saturating_sub(range.start),
                        }
                    } else {
                        TextAreaImeContext {
                            preedit: String::new(),
                            preedit_cursor: None,
                            local_cursor_in_projection: 0,
                        }
                    };
                    let final_node = crate::ui::provide_context_node(ctx, node);

                    let identity = projection_root_identity(&final_node);
                    let segment_index = new_children.len();
                    let scope = [self.stable_id(), 0x5445_5832, segment_index as u64];

                    // Identity-keyed lookup against previous projection slots.
                    let reused_key = if let Some(bucket) = proj_buckets.get_mut(&identity) {
                        bucket.pop()
                    } else {
                        None
                    };

                    let final_key = match reused_key {
                        Some((existing_key, last_node)) => {
                            let reconcile_anchor =
                                self.projection_reconcile_anchor(arena, existing_key);
                            let result = reconcile_anchor
                                .ok_or("projection slot wrapper mismatch")
                                .and_then(|anchor| {
                                    super::reconcile::reconcile_existing_subtree(
                                        arena,
                                        anchor,
                                        &last_node,
                                        &final_node,
                                        &apply_ctx,
                                        &inherited_style,
                                        &scope,
                                    )
                                });
                            match result {
                                Ok(()) => {
                                    self.update_projection_segment_style(
                                        arena,
                                        existing_key,
                                        range.clone(),
                                    );
                                    Some(existing_key)
                                }
                                Err(_) => {
                                    arena.remove_subtree(existing_key);
                                    self.commit_projection_segment(
                                        arena,
                                        self_key,
                                        segment_index,
                                        range.clone(),
                                        &final_node,
                                        viewport_width,
                                        viewport_height,
                                    )
                                }
                            }
                        }
                        None => self.commit_projection_segment(
                            arena,
                            self_key,
                            segment_index,
                            range.clone(),
                            &final_node,
                            viewport_width,
                            viewport_height,
                        ),
                    };

                    let Some(key) = final_key else {
                        continue;
                    };
                    new_children.push(key);
                    new_ranges.push(range);
                    new_slots.push(ChildSlot::Projection {
                        identity,
                        last_node: final_node,
                    });
                }
            }
        }

        // Free leftover old slots that nothing reused.
        for stale in run_queue.drain(..) {
            arena.remove_subtree(stale);
        }
        for stale in line_break_queue.drain(..) {
            arena.remove_subtree(stale);
        }
        for (_, bucket) in proj_buckets.drain() {
            for (stale_key, _) in bucket {
                arena.remove_subtree(stale_key);
            }
        }

        self.children = new_children;
        self.child_char_ranges = new_ranges;
        self.child_slots = new_slots;
        arena.set_children(self_key, self.children.clone());
    }

    /// In-place update of an existing `TextAreaTextRun` to match the
    /// `Segment::Plain` payload for its new position. Reused by both
    /// the fast single-Run path and the full-rebuild path under M3.
    fn update_run_in_place_for_segment(
        &self,
        arena: &mut NodeArena,
        key: NodeKey,
        text: &str,
        range: Range<usize>,
        is_placeholder: bool,
        is_preedit: bool,
        preedit_cursor: Option<(usize, usize)>,
    ) {
        let cascade_color = if is_placeholder {
            self.placeholder_color
        } else {
            self.color
        };
        let text_owned = text.to_string();
        // Run text feeds the unified IFC source; invalidate the package
        // cache's revision fast path.
        self.bump_unified_ifc_source_revision();
        arena.mutate_element_with_invalidation(key, |child, cx| {
            let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() else {
                return;
            };
            run.is_placeholder = is_placeholder;
            run.set_preedit_run(is_preedit, preedit_cursor);
            run.set_text(text_owned, range);
            run.cascade_style(self.run_style(cascade_color));
            cx.invalidate(run.local_dirty_flags());
        });
    }

    fn update_line_break_in_place_for_segment(
        &self,
        arena: &mut NodeArena,
        key: NodeKey,
        range: Range<usize>,
    ) {
        arena.mutate_element_with_invalidation(key, |child, cx| {
            let Some(line_break) = child.as_any_mut().downcast_mut::<TextAreaLineBreak>() else {
                return;
            };
            line_break.set_char_range(range);
            line_break.cascade_style(self.font_size, self.line_height, self.vertical_align);
            cx.invalidate(line_break.local_dirty_flags());
        });
    }

    fn update_projection_segment_style(
        &self,
        arena: &mut NodeArena,
        key: NodeKey,
        range: Range<usize>,
    ) {
        arena.mutate_element_with_invalidation(key, |child, cx| {
            let Some(segment) = child
                .as_any_mut()
                .downcast_mut::<TextAreaProjectionSegment>()
            else {
                return;
            };
            // A reused wrapper keeps its node across rebuilds, but edits
            // before the projection shift its range — without this the
            // unified IFC source (and caret stops) keep the stale range.
            segment.set_char_range(range);
            segment.set_vertical_align(self.vertical_align);
            segment.set_owner_inline_baseline(self.font_size, self.line_height);
            segment.set_auto_wrap(self.auto_wrap);
            cx.invalidate(segment.local_dirty_flags());
        });
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
    /// further split at `\n` boundaries so that newline characters are
    /// explicit `LineBreak` formatting objects instead of hidden Run flags.
    fn slice_into_segments(&self, projections: &[TextAreaRenderProjection]) -> Vec<Segment> {
        let total_chars = self.content.chars().count();

        // Empty content + placeholder special case (no projection
        // semantically applies — placeholder is a single decorative Run).
        if self.content.is_empty() {
            if !self.ime_preedit.is_empty() {
                return vec![Segment::Plain {
                    text: self.ime_preedit.clone(),
                    range: 0..0,
                    is_placeholder: false,
                    is_preedit: true,
                    preedit_cursor: self.ime_preedit_cursor,
                }];
            }
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
        self.insert_preedit_segment(&mut out);
        out
    }

    fn insert_preedit_segment(&self, segments: &mut Vec<Segment>) {
        if self.ime_preedit.is_empty() {
            return;
        }
        let cursor = self.cursor_char.min(self.content.chars().count());
        if segments.iter().any(|segment| {
            matches!(
                segment,
                Segment::Projection { range, .. } if cursor >= range.start && cursor < range.end
            )
        }) {
            return;
        }
        let preedit = Segment::Plain {
            text: self.ime_preedit.clone(),
            range: cursor..cursor,
            is_placeholder: false,
            is_preedit: true,
            preedit_cursor: self.ime_preedit_cursor,
        };

        let mut idx = 0;
        while idx < segments.len() {
            match &segments[idx] {
                Segment::LineBreak { range } if range.start == cursor => {
                    segments.insert(idx, preedit);
                    return;
                }
                Segment::Plain {
                    text,
                    range,
                    is_placeholder,
                    is_preedit,
                    ..
                } if !*is_preedit && cursor >= range.start && cursor <= range.end => {
                    let local = cursor.saturating_sub(range.start);
                    let prefix = slice_chars(text, 0..local);
                    let suffix = slice_chars(text, local..text.chars().count());
                    let range_start = range.start;
                    let range_end = range.end;
                    let is_placeholder = *is_placeholder;
                    let mut replacement = Vec::new();
                    if !prefix.is_empty() {
                        replacement.push(Segment::Plain {
                            text: prefix,
                            range: range_start..cursor,
                            is_placeholder,
                            is_preedit: false,
                            preedit_cursor: None,
                        });
                    }
                    replacement.push(preedit);
                    if !suffix.is_empty() || cursor == range_end {
                        replacement.push(Segment::Plain {
                            text: suffix,
                            range: cursor..range_end,
                            is_placeholder,
                            is_preedit: false,
                            preedit_cursor: None,
                        });
                    }
                    segments.splice(idx..=idx, replacement);
                    return;
                }
                _ => {}
            }
            idx += 1;
        }
        segments.push(preedit);
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
        is_preedit: bool,
        preedit_cursor: Option<(usize, usize)>,
    ) -> NodeKey {
        let cascade_color = if is_placeholder {
            self.placeholder_color
        } else {
            self.color
        };
        let mut run = TextAreaTextRun::new(text, range);
        run.is_placeholder = is_placeholder;
        run.set_preedit_run(is_preedit, preedit_cursor);
        run.cascade_style(self.run_style(cascade_color));
        let desc = crate::view::renderer_adapter::ElementDescriptor::leaf(
            Box::new(run) as Box<dyn ElementTrait>
        );
        crate::view::renderer_adapter::commit_descriptor_tree(arena, Some(parent_key), desc)
    }

    fn commit_line_break_segment(
        &self,
        arena: &mut NodeArena,
        parent_key: NodeKey,
        range: Range<usize>,
    ) -> NodeKey {
        let mut line_break = TextAreaLineBreak::new(range);
        line_break.cascade_style(self.font_size, self.line_height, self.vertical_align);
        let desc = crate::view::renderer_adapter::ElementDescriptor::leaf(
            Box::new(line_break) as Box<dyn ElementTrait>
        );
        crate::view::renderer_adapter::commit_descriptor_tree(arena, Some(parent_key), desc)
    }

    /// Convert a projection RsxNode into descriptors and commit them under
    /// `parent_key`. Multi-root projections are wrapped in a transparent
    /// inline Element so the projection still presents as a single child
    /// of TextArea (mirroring v1's `wrap_projection_children_desc`).
    fn commit_projection_segment(
        &self,
        arena: &mut NodeArena,
        parent_key: NodeKey,
        segment_index: usize,
        range: Range<usize>,
        node: &RsxNode,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<NodeKey> {
        let scope = [self.stable_id(), 0x5445_5832, segment_index as u64];
        let inherited_style = self.projection_inherited_style();
        let children = match descriptors_unwrap_providers(
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
        let desc = wrap_projection_children(
            self.stable_id(),
            segment_index,
            range,
            self.vertical_align,
            self.font_size,
            self.line_height,
            self.auto_wrap,
            children,
        );
        Some(crate::view::renderer_adapter::commit_descriptor_tree(
            arena,
            Some(parent_key),
            desc,
        ))
    }

    fn projection_reconcile_anchor(
        &self,
        arena: &NodeArena,
        projection_key: NodeKey,
    ) -> Option<NodeKey> {
        let is_segment = arena
            .with_element_taken_ref(projection_key, |el, _| {
                el.as_any().is::<TextAreaProjectionSegment>()
            })
            .unwrap_or(false);
        if !is_segment {
            return None;
        }
        let children = arena.children_of(projection_key);
        (children.len() == 1).then_some(children[0])
    }

    /// Inherited style cascaded into projection child subtrees: font /
    /// color from TextArea itself. Mirrors v1.
    fn projection_inherited_style(&self) -> crate::style::Style {
        use crate::style::{
            FontFamily, FontSize, FontWeight, LineHeight, ParsedValue, PropertyId, Style, TextWrap,
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
        style.insert(
            PropertyId::LineHeight,
            ParsedValue::LineHeight(LineHeight::new(self.line_height)),
        );
        style.insert(
            PropertyId::VerticalAlign,
            ParsedValue::VerticalAlign(self.vertical_align),
        );
        style.insert(PropertyId::Color, ParsedValue::Color(self.color.into()));
        style.insert(PropertyId::Cursor, ParsedValue::Cursor(self.cursor));
        // When TextArea has wrap disabled, projection subtrees must also not
        // wrap. Without this cascade, a `<Text>` inside a projection keeps
        // its default `TextWrap::Wrap` and the outer measure pass passes
        // down a tight width once preceding inline content has consumed
        // line space. The projection Text then wraps and pushes the
        // trailing run to a new visual line even though `solver_wrap=false`.
        if !self.auto_wrap {
            style.insert(
                PropertyId::TextWrap,
                ParsedValue::TextWrap(TextWrap::NoWrap),
            );
        }
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
        let has_preedit_run = self.children.iter().any(|&child_key| {
            arena
                .with_element_taken_ref(child_key, |child, _| {
                    child
                        .as_any()
                        .downcast_ref::<TextAreaTextRun>()
                        .is_some_and(|run| run.is_preedit_run())
                })
                .unwrap_or(false)
        });
        if has_preedit_run {
            for &child_key in self.children.iter() {
                arena.mutate_element_ref_with_invalidation(child_key, |child, cx| {
                    let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() else {
                        return;
                    };
                    run.set_inline_preedit(None);
                    cx.invalidate(run.local_dirty_flags());
                });
            }
            return;
        }
        let preedit_active = !self.ime_preedit.is_empty() || self.ime_preedit_cursor.is_some();
        let cursor_char = self.cursor_char;
        let preedit_text = self.ime_preedit.clone();
        let preedit_cursor = self.ime_preedit_cursor;

        // Locate whether the cursor sits inside a projection. In that case
        // Projection-owned text still receives preedit through context.
        // Plain TextArea preedit is represented by a transient Run segment
        // during rebuild, so this router only clears stale legacy splices
        // once that Run exists.
        let mut cursor_in_projection = false;
        for (range, &key) in self.child_char_ranges.iter().zip(self.children.iter()) {
            if cursor_char < range.start || cursor_char >= range.end {
                continue;
            }
            cursor_in_projection = arena
                .with_element_taken_ref(key, |child, _| {
                    child.as_any().downcast_ref::<TextAreaTextRun>().is_none()
                        && child.as_any().downcast_ref::<TextAreaLineBreak>().is_none()
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
                let cursor_hits_empty_run = range.start == range.end && cursor_char == range.start;
                if target_idx_local.is_none() && (cursor_hits_empty_run || cursor_char < range.end)
                {
                    let local = cursor_char
                        .saturating_sub(range.start)
                        .min(range.end.saturating_sub(range.start));
                    target_idx_local = Some((i, local));
                }
            }
            if preedit_active
                && target_idx_local.is_none()
                && let Some(i) = last_run_idx
                && let Some(range) = run_range(i)
            {
                target_idx_local = Some((i, range.end.saturating_sub(range.start)));
            }
        }

        for (i, &child_key) in self.children.iter().enumerate() {
            let is_target = preedit_active && target_idx_local.map(|(t, _)| t) == Some(i);
            let local = target_idx_local.filter(|(t, _)| *t == i).map(|(_, l)| l);
            let target_preedit = is_target.then(|| InlinePreedit {
                insert_at_local: local.unwrap_or(0),
                preedit_text: preedit_text.clone(),
                preedit_cursor,
            });
            arena.mutate_element_ref_with_invalidation(child_key, |child, cx| {
                let Some(run) = child.as_any_mut().downcast_mut::<TextAreaTextRun>() else {
                    return;
                };
                if let Some(mut preedit) = target_preedit {
                    let len = run.char_range.end.saturating_sub(run.char_range.start);
                    preedit.insert_at_local = preedit.insert_at_local.min(len);
                    run.set_inline_preedit(Some(preedit));
                } else {
                    run.set_inline_preedit(None);
                }
                cx.invalidate(run.local_dirty_flags());
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
            } else if slice_chars(content, start..end).contains('\n') {
                // A projection node is produced by a FnOnce render
                // closure for the original slice, so it cannot be
                // safely split across paragraphs. Reject cross-line
                // projections and let the plain paragraph path own the
                // hard newline semantics.
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
    s.chars()
        .skip(range.start)
        .take(range.end - range.start)
        .collect()
}

/// Split `text` (covering global char range `range`) at `\n` boundaries.
/// Visible paragraph text becomes `Segment::Plain`; each newline character
/// becomes an explicit `Segment::LineBreak` that owns that source char.
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
            if para_end_excl_nl > paragraph_start || paragraph_start == char_index {
                out.push(Segment::Plain {
                    text: paragraph_chars.iter().collect(),
                    range: paragraph_start..para_end_excl_nl,
                    is_placeholder,
                    is_preedit: false,
                    preedit_cursor: None,
                });
            }
            out.push(Segment::LineBreak {
                range: char_index..char_index + 1,
            });
            paragraph_chars.clear();
            paragraph_start = char_index + 1;
            char_index += 1;
        } else {
            paragraph_chars.push(ch);
            char_index += 1;
        }
    }
    // Final paragraph (no trailing `\n` in source). A trailing newline
    // leaves an empty paragraph after it; keep a zero-length Run there so
    // caret placement and IME preedit have a line-local host.
    if !paragraph_chars.is_empty() || paragraph_start == range.end {
        out.push(Segment::Plain {
            text: paragraph_chars.iter().collect(),
            range: paragraph_start..char_index,
            is_placeholder,
            is_preedit: false,
            preedit_cursor: None,
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
/// projection presents as a single child of TextArea.
fn wrap_projection_children(
    text_area_stable_id: u64,
    segment_index: usize,
    range: Range<usize>,
    vertical_align: crate::style::VerticalAlign,
    font_size: f32,
    line_height: f32,
    auto_wrap: bool,
    children: Vec<crate::view::renderer_adapter::ElementDescriptor>,
) -> crate::view::renderer_adapter::ElementDescriptor {
    let wrapper_id = text_area_stable_id
        .wrapping_mul(1_000_003)
        .wrapping_add(segment_index as u64 + 1);
    let mut wrapper = TextAreaProjectionSegment::with_stable_id(wrapper_id);
    wrapper.set_char_range(range);
    wrapper.set_vertical_align(vertical_align);
    wrapper.set_owner_inline_baseline(font_size, line_height);
    wrapper.set_auto_wrap(auto_wrap);

    crate::view::renderer_adapter::ElementDescriptor {
        element: Box::new(wrapper) as Box<dyn ElementTrait>,
        children,
        side_slots: Vec::new(),
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
    use crate::ui::{RsxKey, RsxNode, RsxTagDescriptor};
    use crate::view::ElementStylePropSchema;
    use crate::view::base_component::text_area::inline_ifc::TextAreaUnifiedIfcSourceKind;
    use crate::view::base_component::text_area::{TextAreaProjectionSegment, TextAreaTextRun};
    use crate::view::base_component::{
        DirtyFlags, ElementTrait, LayoutConstraints, LayoutPlacement, Text, TextArea,
    };
    use crate::view::node_arena::{NodeArena, NodeKey};

    #[test]
    fn normalize_rejects_projection_ranges_that_cross_newline() {
        let projections = vec![super::TextAreaRenderProjection {
            range: 1..4,
            node: RsxNode::text("a\nb"),
        }];
        let normalized = super::normalize_projections("xa\nby", &projections);
        assert!(
            normalized.is_empty(),
            "cross-line projections must not swallow hard newline semantics"
        );
    }

    fn fixture_with_caret_in_projection(
        ime_preedit: &str,
        ime_preedit_cursor: Option<(usize, usize)>,
    ) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
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
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop("style", style)
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
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
        let (arena, root) = fixture_with_caret_in_projection("\u{4E2D}", Some((1, 1)));

        let children = arena.children_of(root);
        assert_eq!(
            children.len(),
            3,
            "expected 3 children (Run / projection / Run); got {}",
            children.len(),
        );

        let projection_key = children[1];
        let is_segment = arena
            .with_element_taken_ref(projection_key, |el, _| {
                el.as_any().is::<TextAreaProjectionSegment>()
            })
            .unwrap_or(false);
        assert!(
            is_segment,
            "projection slot should hold a TextAreaProjectionSegment wrapper",
        );
        assert!(
            !arena.children_of(projection_key).is_empty(),
            "projection segment should have its descriptor children committed",
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

    #[test]
    fn projection_reconcile_updates_text_with_preedit_context() {
        let (mut arena, root) = fixture_with_caret_in_projection("", None);
        let projection_key = arena.children_of(root)[1];
        let text_key_before = first_text_descendant(&arena, projection_key);

        arena.with_element_taken(root, |el, _| {
            let ta = el
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root");
            ta.ime_preedit = "\u{4E2D}".to_string();
            ta.ime_preedit_cursor = Some((3, 3));
            ta.children_dirty = true;
            ta.dirty_flags = ta.dirty_flags.union(DirtyFlags::ALL);
        });
        relayout(&mut arena, root);

        let projection_key_after = arena.children_of(root)[1];
        let text_key_after = first_text_descendant(&arena, projection_key_after);
        assert_eq!(
            text_key_after, text_key_before,
            "projection Text should be reconciled in place",
        );
        let text_content = arena
            .with_element_taken_ref(text_key_after, |el, _| {
                el.as_any()
                    .downcast_ref::<Text>()
                    .expect("projection Text")
                    .content()
                    .to_string()
            })
            .expect("text exists");
        assert_eq!(text_content, "X\u{4E2D}YZ");
    }

    /// Caret inside a projection segment with preedit active should not
    /// route preedit text onto adjacent Runs. The projection owns text
    /// rendering via `TextAreaImeContext`.
    #[test]
    fn projection_preedit_does_not_route_to_adjacent_run_when_caret_in_projection() {
        let (arena, root) = fixture_with_caret_in_projection("\u{4E2D}\u{6587}", Some((2, 2)));

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

    #[test]
    fn text_area_projection_fixed_width_wrap_keeps_plain_run_slicing() {
        let content = concat!(
            "Fetch a long environment URL from {{API_HOST}} while the line wraps automatically\n",
            "/v1/users/{{USER_ID}}/profiles/preferences/activity/export/sessions"
        );
        let api_host = char_range_of(content, "{{API_HOST}}");
        let user_id = char_range_of(content, "{{USER_ID}}");

        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(api_host.clone(), |_text_area_node| {
                projection_chip_node("API_HOST", 88.0)
            });
            render.range(user_id.clone(), |_text_area_node| {
                projection_chip_node("USER_ID", 72.0)
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
                max_width: 168.0,
                max_height: 360.0,
                viewport_width: 168.0,
                viewport_height: 360.0,
                percent_base_width: Some(168.0),
                percent_base_height: Some(360.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 168.0,
                available_height: 360.0,
                viewport_width: 168.0,
                viewport_height: 360.0,
                percent_base_width: Some(168.0),
                percent_base_height: Some(360.0),
            },
        );

        let children = arena.children_of(root);
        let run_segments = children
            .iter()
            .filter_map(|key| {
                arena
                    .with_element_taken_ref(*key, |child, _| {
                        child
                            .as_any()
                            .downcast_ref::<TextAreaTextRun>()
                            .map(|run| (run.text.clone(), run.char_range.clone()))
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        let projection_count = children
            .iter()
            .filter(|key| {
                arena
                    .with_element_taken_ref(**key, |child, _| {
                        child.as_any().is::<TextAreaProjectionSegment>()
                    })
                    .unwrap_or(false)
            })
            .count();

        assert_eq!(projection_count, 2, "expected two atomic projection slots");
        assert_eq!(
            run_segments.len(),
            4,
            "projection slicing should leave four plain TextAreaTextRun segments"
        );

        let run_texts = run_segments
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            run_texts,
            vec![
                "Fetch a long environment URL from ",
                " while the line wraps automatically",
                "/v1/users/",
                "/profiles/preferences/activity/export/sessions",
            ],
            "projection tokens should be removed from plain run slices",
        );

        for (run_text, char_range) in &run_segments {
            assert_eq!(
                run_text.chars().count(),
                char_range.end.saturating_sub(char_range.start),
                "plain run char range should describe the sliced run text"
            );
        }

        let run_contents = run_segments
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join("");
        assert!(
            !run_contents.contains("API_HOST") && !run_contents.contains("USER_ID"),
            "plain run slices do not carry projection atomic boxes"
        );
        assert_ne!(
            run_contents, content,
            "plain run slices alone do not describe the visible unified render/layout path"
        );
    }

    #[test]
    fn text_area_inline_ifc_projection_fixed_width_wrap_builds_unified_root_source() {
        let content = concat!(
            "Fetch a long environment URL from {{API_HOST}} while the line wraps automatically\n",
            "/v1/users/{{USER_ID}}/profiles/preferences/activity/export/sessions"
        );
        let api_host = char_range_of(content, "{{API_HOST}}");
        let user_id = char_range_of(content, "{{USER_ID}}");

        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(api_host.clone(), |_text_area_node| {
                projection_chip_node("API_HOST", 88.0)
            });
            render.range(user_id.clone(), |_text_area_node| {
                projection_chip_node("USER_ID", 72.0)
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
                max_width: 168.0,
                max_height: 360.0,
                viewport_width: 168.0,
                viewport_height: 360.0,
                percent_base_width: Some(168.0),
                percent_base_height: Some(360.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 168.0,
                available_height: 360.0,
                viewport_width: 168.0,
                viewport_height: 360.0,
                percent_base_width: Some(168.0),
                percent_base_height: Some(360.0),
            },
        );

        let package = arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .unified_inline_ifc_root_package(&arena)
            })
            .flatten()
            .expect("TextArea should expose a unified IFC root package");

        assert_eq!(package.width_constraint, Some(168.0));
        assert!(package.allow_wrap);
        assert_eq!(package.text_run_count(), 4);
        assert_eq!(package.projection_segment_count(), 2);
        assert_eq!(
            package.source_segments.len(),
            arena.children_of(root).len(),
            "each TextArea child source should map to one IFC root item"
        );

        let source_kinds = package
            .source_segments
            .iter()
            .map(|segment| segment.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            source_kinds,
            vec![
                TextAreaUnifiedIfcSourceKind::TextRun,
                TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox,
                TextAreaUnifiedIfcSourceKind::TextRun,
                TextAreaUnifiedIfcSourceKind::LineBreak,
                TextAreaUnifiedIfcSourceKind::TextRun,
                TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox,
                TextAreaUnifiedIfcSourceKind::TextRun,
            ],
            "unified root source should preserve TextArea text/projection/newline sequence"
        );

        let projection_ranges = package
            .source_segments
            .iter()
            .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
            .map(|segment| segment.char_range.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            projection_ranges,
            vec![
                char_range_of(content, "{{API_HOST}}"),
                char_range_of(content, "{{USER_ID}}"),
            ],
            "projection atomic boxes must keep original TextArea char/source ranges"
        );

        assert_eq!(
            package.ifc.backing_text(),
            concat!(
                "Fetch a long environment URL from  while the line wraps automatically\n",
                "/v1/users//profiles/preferences/activity/export/sessions"
            ),
            "projection tokens should be represented by atomic boxes, not glyph text"
        );
        assert_eq!(package.atomic_sources.len(), 2);
        for source in &package.atomic_sources {
            let atomic_package = package.ifc.atomic_box_placement_package(*source);
            assert_eq!(atomic_package.source, *source);
            assert_eq!(
                atomic_package.placements.len(),
                1,
                "each TextArea projection source should produce one atomic placement"
            );
            let placement = &atomic_package.placements[0];
            assert_eq!(placement.source, *source);
            assert!(
                placement.measurement.measured_size.width > 0.0
                    && placement.measurement.measured_size.height > 0.0,
                "projection atomic measurement should come from the laid-out projection segment"
            );
        }
    }

    #[test]
    fn text_area_projection_tall_child_keeps_unified_ifc_geometry_height() {
        let content = "before {{TALL}} after";
        let tall_range = char_range_of(content, "{{TALL}}");
        let tall_height = 240.0;

        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(tall_range.clone(), move |_text_area_node| {
                tall_projection_block_node(96.0, tall_height)
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
                max_width: 180.0,
                max_height: 2_000.0,
                viewport_width: 180.0,
                viewport_height: 2_000.0,
                percent_base_width: Some(180.0),
                percent_base_height: Some(2_000.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 180.0,
                available_height: 2_000.0,
                viewport_width: 180.0,
                viewport_height: 2_000.0,
                percent_base_width: Some(180.0),
                percent_base_height: Some(2_000.0),
            },
        );

        let package = arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .unified_inline_ifc_root_package(&arena)
            })
            .flatten()
            .expect("TextArea should expose a unified IFC root package");
        let projection_segment = package
            .source_segments
            .iter()
            .find(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
            .cloned()
            .expect("projection source segment");
        let atomic_package = package
            .atomic_package_for_child(projection_segment.child_key)
            .expect("projection child should have an atomic package");
        let placement = atomic_package
            .placements
            .first()
            .expect("projection child should have an atomic placement");
        let projection_snapshot = arena
            .with_element_taken_ref(projection_segment.child_key, |child, _| {
                child.box_model_snapshot()
            })
            .expect("projection child snapshot");
        let content_size = package.content_size();

        assert!(
            placement.measurement.measured_size.height >= tall_height - 0.01,
            "atomic measurement should keep the tall projection height: measured={}",
            placement.measurement.measured_size.height
        );
        assert!(
            placement.rect.height >= tall_height - 0.01,
            "atomic placement rect should keep the tall projection height: rect={}",
            placement.rect.height
        );
        assert!(
            projection_snapshot.height >= tall_height - 0.01,
            "projection snapshot should keep the tall child height: snapshot={projection_snapshot:?}"
        );
        assert!(
            content_size.height >= tall_height - 0.01,
            "TextArea unified IFC content size should include tall projection height: content_size={content_size:?}"
        );
    }

    #[test]
    fn text_area_inline_ifc_projection_fixed_width_wrap_applies_unified_atomic_placement() {
        let content = concat!(
            "Fetch a long environment URL from {{API_HOST}} while the line wraps automatically\n",
            "/v1/users/{{USER_ID}}/profiles/preferences/activity/export/sessions"
        );
        let api_host = char_range_of(content, "{{API_HOST}}");
        let user_id = char_range_of(content, "{{USER_ID}}");

        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(api_host.clone(), |_text_area_node| {
                projection_chip_node("API_HOST", 88.0)
            });
            render.range(user_id.clone(), |_text_area_node| {
                projection_chip_node("USER_ID", 72.0)
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
                max_width: 168.0,
                max_height: 360.0,
                viewport_width: 168.0,
                viewport_height: 360.0,
                percent_base_width: Some(168.0),
                percent_base_height: Some(360.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 168.0,
                available_height: 360.0,
                viewport_width: 168.0,
                viewport_height: 360.0,
                percent_base_width: Some(168.0),
                percent_base_height: Some(360.0),
            },
        );

        let package = arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .unified_inline_ifc_root_package(&arena)
            })
            .flatten()
            .expect("TextArea should expose a unified IFC root package");

        let projection_segments = package
            .source_segments
            .iter()
            .filter(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(projection_segments.len(), 2);

        for segment in projection_segments {
            let atomic_package = package
                .atomic_package_for_child(segment.child_key)
                .expect("projection child should have an atomic package");
            let placement = atomic_package
                .placements
                .first()
                .expect("projection child should have an atomic placement");
            let snapshot = arena
                .with_element_taken_ref(segment.child_key, |child, _| child.box_model_snapshot())
                .expect("projection child snapshot");
            assert!(
                (snapshot.x - placement.rect.x).abs() < 0.01,
                "projection x should be applied from unified atomic placement: snapshot={}, placement={}",
                snapshot.x,
                placement.rect.x
            );
            assert!(
                (snapshot.y - placement.rect.y).abs() < 0.01,
                "projection y should be applied from unified atomic placement: snapshot={}, placement={}",
                snapshot.y,
                placement.rect.y
            );
            assert!(
                (snapshot.width - placement.rect.width).abs() < 0.01,
                "projection width should remain aligned with unified atomic measurement"
            );
            assert!(
                (snapshot.height - placement.rect.height).abs() < 0.01,
                "projection height should remain aligned with unified atomic measurement"
            );
        }
    }

    #[test]
    fn text_area_inline_ifc_projection_overlay_sources_match_unified_atomic_placement() {
        let content = concat!(
            "Fetch a long environment URL from {{API_HOST}} while the line wraps automatically\n",
            "/v1/users/{{USER_ID}}/profiles/preferences/activity/export/sessions"
        );
        let api_host = char_range_of(content, "{{API_HOST}}");
        let user_id = char_range_of(content, "{{USER_ID}}");

        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.auto_wrap = true;
        text_area.cursor_char = api_host.start + 2;
        text_area.selection_anchor_char = Some(api_host.start + 1);
        text_area.selection_focus_char = Some(api_host.start + 2);
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(api_host.clone(), |_text_area_node| {
                projection_chip_node("API_HOST", 88.0)
            });
            render.range(user_id.clone(), |_text_area_node| {
                projection_chip_node("USER_ID", 72.0)
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
                max_width: 168.0,
                max_height: 360.0,
                viewport_width: 168.0,
                viewport_height: 360.0,
                percent_base_width: Some(168.0),
                percent_base_height: Some(360.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 168.0,
                available_height: 360.0,
                viewport_width: 168.0,
                viewport_height: 360.0,
                percent_base_width: Some(168.0),
                percent_base_height: Some(360.0),
            },
        );

        let package = arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .unified_inline_ifc_root_package(&arena)
            })
            .flatten()
            .expect("TextArea should expose a unified IFC root package");
        let overlay_sources = package.projection_overlay_sources();
        assert_eq!(overlay_sources.len(), 2);

        let children = arena.children_of(root);
        let projection_child_index = children
            .iter()
            .position(|key| {
                arena
                    .with_element_taken_ref(*key, |child, _| {
                        child.as_any().is::<TextAreaProjectionSegment>()
                    })
                    .unwrap_or(false)
            })
            .expect("first projection child");
        let projection_key = children[projection_child_index];
        let overlay_source = package
            .projection_overlay_source_for_child(projection_key)
            .expect("projection overlay source should come from unified IFC root");
        assert_eq!(
            overlay_source.char_range,
            char_range_of(content, "{{API_HOST}}")
        );
        assert_eq!(
            overlay_source.backing_byte_range.start,
            overlay_source.backing_byte_range.end
        );

        let selection_context = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .projection_selection_context_for_child(
                        projection_child_index,
                        projection_key,
                        arena,
                    )
            })
            .expect("root exists")
            .expect("projection selection context should remain available");
        assert_eq!(selection_context.start, 1);
        assert_eq!(selection_context.end, 2);
        assert_eq!(
            overlay_source.char_range.start + selection_context.start
                ..overlay_source.char_range.start + selection_context.end,
            char_range_of(content, "{{API_HOST}}").start + 1
                ..char_range_of(content, "{{API_HOST}}").start + 2,
            "projection selection overlay should map through the same TextArea source range as unified IFC"
        );

        let snapshot = arena
            .with_element_taken_ref(projection_key, |child, _| child.box_model_snapshot())
            .expect("projection child snapshot");
        assert!(
            (snapshot.x - overlay_source.atomic_rect.x).abs() < 0.01
                && (snapshot.y - overlay_source.atomic_rect.y).abs() < 0.01
                && (snapshot.width - overlay_source.atomic_rect.width).abs() < 0.01
                && (snapshot.height - overlay_source.atomic_rect.height).abs() < 0.01,
            "projection overlay source should use the same atomic rect applied to the visible projection placement"
        );
    }

    #[test]
    fn text_area_inline_ifc_projection_atomic_placement_honors_vertical_align() {
        fn projection_y_for(vertical_align: crate::style::VerticalAlign) -> (f32, f32) {
            let content = "aaa{{BIG}}bbb";
            let projection_range = char_range_of(content, "{{BIG}}");
            let mut text_area = TextArea::new();
            text_area.content = content.to_string();
            text_area.font_size = 14.0;
            text_area.line_height = 1.25;
            text_area.vertical_align = vertical_align;
            text_area.auto_wrap = true;
            text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
                render.range(projection_range.clone(), |_text_area_node| {
                    projection_chip_node("BIG", 88.0)
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
                    max_width: 240.0,
                    max_height: 120.0,
                    viewport_width: 240.0,
                    viewport_height: 120.0,
                    percent_base_width: Some(240.0),
                    percent_base_height: Some(120.0),
                },
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 240.0,
                    available_height: 120.0,
                    viewport_width: 240.0,
                    viewport_height: 120.0,
                    percent_base_width: Some(240.0),
                    percent_base_height: Some(120.0),
                },
            );

            let package = arena
                .with_element_taken_ref(root, |el, _| {
                    el.as_any()
                        .downcast_ref::<TextArea>()
                        .expect("TextArea root")
                        .unified_inline_ifc_root_package(&arena)
                })
                .flatten()
                .expect("TextArea should expose a unified IFC root package");
            let projection_child = package
                .source_segments
                .iter()
                .find(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox)
                .expect("projection source")
                .child_key;
            let placement_y = package
                .atomic_package_for_child(projection_child)
                .expect("projection atomic package")
                .placements
                .first()
                .expect("projection atomic placement")
                .rect
                .y;
            let snapshot_y = arena
                .with_element_taken_ref(projection_child, |child, _| child.box_model_snapshot().y)
                .expect("projection snapshot");
            let glyph_y = package
                .text_pass_staging_input([0.0, 0.0], 1.0, 0, 1.0)
                .glyphs
                .first()
                .expect("plain glyph")
                .final_paint_pos[1];
            assert!(
                (placement_y - snapshot_y).abs() < 0.01,
                "visible projection placement should stay sourced from unified atomic placement"
            );
            (placement_y, glyph_y)
        }

        let (top_projection_y, top_glyph_y) = projection_y_for(crate::style::VerticalAlign::Top);
        let (bottom_projection_y, bottom_glyph_y) =
            projection_y_for(crate::style::VerticalAlign::Bottom);

        assert!(
            (bottom_projection_y - top_projection_y).abs() < 0.01,
            "the tallest projection box can stay pinned while shorter glyphs move, top={top_projection_y}, bottom={bottom_projection_y}",
        );
        assert!(
            bottom_glyph_y > top_glyph_y + 1.0,
            "TextArea unified glyph render must move when vertical_align changes, top={top_glyph_y}, bottom={bottom_glyph_y}",
        );
    }

    #[test]
    fn text_area_inline_ifc_auto_width_projection_keeps_following_text_after_atomic_box() {
        let content = concat!(
            "First line with a long value that can wrap when auto wrap is enabled.",
            "{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\n",
            "Tail line"
        );
        let api_host = char_range_of(content, "{{API_HOST}}");
        let user_id = char_range_of(content, "{{USER_ID}}");

        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(api_host.clone(), |_text_area_node| {
                auto_projection_chip_node("{{API_HOST}}")
            });
            render.range(user_id.clone(), |_text_area_node| {
                auto_projection_chip_node("{{USER_ID}}")
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
                max_width: 360.0,
                max_height: 176.0,
                viewport_width: 360.0,
                viewport_height: 176.0,
                percent_base_width: Some(360.0),
                percent_base_height: Some(176.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 360.0,
                available_height: 176.0,
                viewport_width: 360.0,
                viewport_height: 176.0,
                percent_base_width: Some(360.0),
                percent_base_height: Some(176.0),
            },
        );

        let package = arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .unified_inline_ifc_root_package(&arena)
            })
            .flatten()
            .expect("TextArea should expose a unified IFC root package");
        let user_segment = package
            .source_segments
            .iter()
            .find(|segment| {
                segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                    && segment.char_range == char_range_of(content, "{{USER_ID}}")
            })
            .expect("USER_ID projection segment");
        let following_run = package
            .source_segments
            .iter()
            .find(|segment| {
                segment.kind == TextAreaUnifiedIfcSourceKind::TextRun
                    && segment.char_range.start == char_range_of(content, "{{USER_ID}}").end
            })
            .expect("text run after USER_ID");
        let atomic = package
            .atomic_package_for_child(user_segment.child_key)
            .expect("USER_ID atomic package")
            .placements
            .first()
            .expect("USER_ID atomic placement")
            .clone();
        let snapshot = package.ifc.text_layout_snapshot_ref();
        let following_glyph = snapshot
            .lines
            .iter()
            .flat_map(|line| &line.glyphs)
            .find(|glyph| glyph.source == following_run.source)
            .expect("first glyph after USER_ID");

        if following_glyph.y >= atomic.rect.y
            && following_glyph.y < atomic.rect.y + atomic.rect.height.max(1.0)
        {
            assert!(
                following_glyph.x >= atomic.rect.x + atomic.rect.width - 0.5,
                "text after USER_ID must not be painted under the projection chip: glyph_x={}, atomic=({}, {}, {}, {})",
                following_glyph.x,
                atomic.rect.x,
                atomic.rect.y,
                atomic.rect.width,
                atomic.rect.height,
            );
        }
    }

    #[test]
    fn text_area_inline_ifc_selection_rects_follow_unified_text_render() {
        let content = concat!(
            "First line with a long value that can wrap when auto wrap is enabled.",
            "{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\n",
            "Tail line"
        );
        let api_host = char_range_of(content, "{{API_HOST}}");
        let user_id = char_range_of(content, "{{USER_ID}}");

        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.auto_wrap = true;
        text_area.vertical_align = crate::style::VerticalAlign::Bottom;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(api_host.clone(), |_text_area_node| {
                auto_projection_chip_node("{{API_HOST}}")
            });
            render.range(user_id.clone(), |_text_area_node| {
                auto_projection_chip_node("{{USER_ID}}")
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
                max_width: 360.0,
                max_height: 176.0,
                viewport_width: 360.0,
                viewport_height: 176.0,
                percent_base_width: Some(360.0),
                percent_base_height: Some(176.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 360.0,
                available_height: 176.0,
                viewport_width: 360.0,
                viewport_height: 176.0,
                percent_base_width: Some(360.0),
                percent_base_height: Some(176.0),
            },
        );

        let package = arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .unified_inline_ifc_root_package(&arena)
            })
            .flatten()
            .expect("TextArea should expose a unified IFC root package");
        let first_run = package
            .source_segments
            .iter()
            .find(|segment| segment.kind == TextAreaUnifiedIfcSourceKind::TextRun)
            .expect("first text source");
        let selected_range = first_run.char_range.start..first_run.char_range.start + 10;
        let selection_rect = package
            .selection_rects_for_char_range(selected_range)
            .into_iter()
            .next()
            .expect("unified selection rect");
        let first_glyph = package
            .text_pass_staging_input([0.0, 0.0], 1.0, 0, 1.0)
            .glyphs
            .into_iter()
            .find(|glyph| glyph.final_paint_pos[0] >= selection_rect.x - 0.5)
            .expect("selected staged glyph");

        assert!(
            first_glyph.final_paint_pos[0] >= selection_rect.x - 0.5
                && first_glyph.final_paint_pos[0] <= selection_rect.x + selection_rect.width + 0.5,
            "selection rect must track unified glyph render x, selection=({}, {}, {}, {}), glyph_x={}",
            selection_rect.x,
            selection_rect.y,
            selection_rect.width,
            selection_rect.height,
            first_glyph.final_paint_pos[0],
        );
        assert!(
            first_glyph.final_paint_pos[1] >= selection_rect.y - 0.5
                && first_glyph.final_paint_pos[1] <= selection_rect.y + selection_rect.height + 0.5,
            "selection rect must track unified glyph render y, selection=({}, {}, {}, {}), glyph_y={}",
            selection_rect.x,
            selection_rect.y,
            selection_rect.width,
            selection_rect.height,
            first_glyph.final_paint_pos[1],
        );
    }

    #[test]
    fn preedit_inserts_transient_run_on_middle_empty_paragraph() {
        let (arena, root) = plain_textarea_with_preedit("a\n\nb", 2, "\u{4E2D}");

        let children = arena.children_of(root);
        assert_eq!(
            children.len(),
            6,
            "expected Run / LineBreak / preedit Run / empty Run / LineBreak / Run"
        );
        assert_preedit_run(&arena, children[2], "\u{4E2D}", 2..2);
        assert_run_text_range(&arena, children[3], "", 2..2);
        assert!(run_inline_preedit(&arena, children[0]).is_none());
        assert!(run_inline_preedit(&arena, children[5]).is_none());
    }

    #[test]
    fn preedit_inserts_transient_run_on_trailing_empty_paragraph() {
        let (arena, root) = plain_textarea_with_preedit("a\n", 2, "\u{4E2D}");

        let children = arena.children_of(root);
        assert_eq!(
            children.len(),
            4,
            "expected Run / LineBreak / preedit Run / trailing empty Run"
        );
        assert_preedit_run(&arena, children[2], "\u{4E2D}", 2..2);
        assert_run_text_range(&arena, children[3], "", 2..2);
        assert!(run_inline_preedit(&arena, children[0]).is_none());
    }

    #[test]
    fn preedit_inserts_transient_run_in_empty_textarea() {
        let (arena, root) = plain_textarea_with_preedit("", 0, "\u{4E2D}");

        let children = arena.children_of(root);
        assert_eq!(
            children.len(),
            1,
            "empty TextArea should create a preedit Run"
        );
        assert_preedit_run(&arena, children[0], "\u{4E2D}", 0..0);
    }

    fn plain_textarea_with_preedit(
        content: &str,
        cursor_char: usize,
        ime_preedit: &str,
    ) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.cursor_char = cursor_char;
        text_area.ime_preedit = ime_preedit.to_string();
        text_area.ime_preedit_cursor = Some((ime_preedit.len(), ime_preedit.len()));

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
        relayout(&mut arena, root);
        (arena, root)
    }

    fn char_range_of(content: &str, needle: &str) -> std::ops::Range<usize> {
        let start_byte = content.find(needle).expect("needle exists");
        let end_byte = start_byte + needle.len();
        content[..start_byte].chars().count()..content[..end_byte].chars().count()
    }

    fn projection_chip_node(label: &str, width: f32) -> RsxNode {
        RsxNode::tagged(
            "Element",
            RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
        )
        .with_prop(
            "style",
            ElementStylePropSchema {
                width: Some(Length::px(width)),
                height: Some(Length::px(22.0)),
                ..Default::default()
            },
        )
        .with_child(
            RsxNode::tagged(
                "Text",
                RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
            )
            .with_child(RsxNode::text(label)),
        )
    }

    fn tall_projection_block_node(width: f32, height: f32) -> RsxNode {
        RsxNode::tagged(
            "Element",
            RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
        )
        .with_prop(
            "style",
            ElementStylePropSchema {
                width: Some(Length::px(width)),
                height: Some(Length::px(height)),
                ..Default::default()
            },
        )
    }

    fn auto_projection_chip_node(label: &str) -> RsxNode {
        RsxNode::tagged(
            "Element",
            RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
        )
        .with_prop(
            "style",
            ElementStylePropSchema {
                padding: Some(crate::style::Padding::uniform(Length::px(0.0)).x(Length::px(20.0))),
                font_size: Some(crate::style::FontSize::Px(24.0)),
                border: Some(crate::style::Border::uniform(
                    Length::px(1.0),
                    &crate::style::Color::hex("#42566f"),
                )),
                ..Default::default()
            },
        )
        .with_child(
            RsxNode::tagged(
                "Text",
                RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
            )
            .with_child(RsxNode::text(label)),
        )
    }

    fn assert_run_text_range(
        arena: &NodeArena,
        key: NodeKey,
        text: &str,
        range: std::ops::Range<usize>,
    ) {
        arena
            .with_element_taken_ref(key, |child, _| {
                let run = child
                    .as_any()
                    .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                    .expect("TextAreaTextRun");
                assert_eq!(run.text, text);
                assert_eq!(run.char_range, range);
            })
            .expect("run exists");
    }

    fn assert_preedit_run(
        arena: &NodeArena,
        key: NodeKey,
        text: &str,
        range: std::ops::Range<usize>,
    ) {
        arena
            .with_element_taken_ref(key, |child, _| {
                let run = child
                    .as_any()
                    .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                    .expect("TextAreaTextRun");
                assert_eq!(run.text, text);
                assert_eq!(run.char_range, range);
                assert!(run.is_preedit_run(), "expected transient preedit Run");
                assert!(run.inline_preedit.is_none());
            })
            .expect("run exists");
    }

    fn run_inline_preedit(
        arena: &NodeArena,
        key: NodeKey,
    ) -> Option<crate::view::base_component::text_area::run::InlinePreedit> {
        arena
            .with_element_taken_ref(key, |child, _| {
                child
                    .as_any()
                    .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                    .and_then(|run| run.inline_preedit.clone())
            })
            .flatten()
    }

    // -----------------------------------------------------------------
    // P6 regression tests — `rebuild_children_full` reconcile preserves
    // matched-projection `NodeKey`s across rebuild instead of full
    // teardown.
    // -----------------------------------------------------------------

    /// Standard test layout pass — re-runs measure/place at the same
    /// constraints so a content edit that flagged `children_dirty`
    /// drives `rebuild_children_if_dirty` again.
    fn relayout(arena: &mut NodeArena, root: NodeKey) {
        crate::view::test_support::measure_and_place(
            arena,
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
    }

    fn fixture_with_keyed_projection(content: &str) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..5, |_text_area_node| {
                let style = ElementStylePropSchema {
                    width: Some(Length::px(40.0)),
                    height: Some(Length::px(20.0)),
                    ..Default::default()
                };
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_key(RsxKey::Local(0xC0AC_C0AC_0001))
                .with_prop("style", style)
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text("X")),
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
        relayout(&mut arena, root);
        (arena, root)
    }

    /// After `set_content_from_external`, the projection Element's
    /// `NodeKey` should be the same instance as before — proving that
    /// `reconcile_existing_subtree` reused the slot rather than the
    /// rebuild tearing it down. The Run keys may legitimately change
    /// (Run reuse is a queue, but for this single-projection layout
    /// the Run count is stable so they should also be reused).
    #[test]
    fn projection_node_key_preserved_across_outer_edit() {
        let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
        let kids_before = arena.children_of(root);
        assert_eq!(kids_before.len(), 3, "Run / projection / Run");
        let proj_key_before = kids_before[1];
        let projection_text_before = first_text_descendant(&arena, proj_key_before);

        // Outer edit: append "!". Projection range stays 2..5.
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_content_from_external("abXYZcd!".to_string());
        });
        relayout(&mut arena, root);

        let kids_after = arena.children_of(root);
        assert_eq!(kids_after.len(), 3);
        assert_eq!(
            kids_after[1], proj_key_before,
            "projection NodeKey should survive outer edit",
        );
        let projection_text_after = first_text_descendant(&arena, kids_after[1]);
        assert_eq!(
            projection_text_after, projection_text_before,
            "projection inner Text NodeKey should also survive",
        );
    }

    /// Run NodeKeys should also survive a rebuild when the segment
    /// shape (Plain/Projection counts) is unchanged. This is the
    /// in-place plain-Run reuse path inside the full-rebuild flow.
    #[test]
    fn run_node_keys_preserved_across_outer_edit() {
        let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
        let kids_before = arena.children_of(root);
        let run_a_before = kids_before[0];
        let run_b_before = kids_before[2];

        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_content_from_external("abXYZcde".to_string());
        });
        relayout(&mut arena, root);

        let kids_after = arena.children_of(root);
        assert_eq!(kids_after.len(), 3);
        assert_eq!(kids_after[0], run_a_before, "leading Run reused");
        assert_eq!(kids_after[2], run_b_before, "trailing Run reused");
    }

    /// Identity-mismatched projection (different `key=`) forces a
    /// fresh commit — the old projection NodeKey must NOT survive.
    #[test]
    fn projection_node_key_changes_when_key_mismatches() {
        // First fixture has key="counter".
        let (mut arena, root) = fixture_with_keyed_projection("abXYZcd");
        let kids_before = arena.children_of(root);
        let proj_key_before = kids_before[1];

        // Swap handler to one with key="other".
        arena.with_element_taken(root, |el, _| {
            let ta = el
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root");
            ta.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
                render.range(2..5, |_text_area_node| {
                    let style = ElementStylePropSchema {
                        width: Some(Length::px(40.0)),
                        height: Some(Length::px(20.0)),
                        ..Default::default()
                    };
                    RsxNode::tagged(
                        "Element",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_key(RsxKey::Local(0xC0AC_C0AC_0002))
                    .with_prop("style", style)
                    .with_child(
                        RsxNode::tagged(
                            "Text",
                            RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(RsxNode::text("X")),
                    )
                });
            }));
            ta.mark_content_dirty();
        });
        relayout(&mut arena, root);

        let kids_after = arena.children_of(root);
        assert_eq!(kids_after.len(), 3);
        assert_ne!(
            kids_after[1], proj_key_before,
            "key=counter → key=other: projection NodeKey must NOT survive",
        );
    }

    /// Multi-paragraph plain content (no projections) with a paragraph
    /// count change. The full-rebuild path's Run reuse queue should
    /// keep the leading paragraphs' Run `NodeKey`s stable across the
    /// edit and only mint a fresh Run for the appended paragraph.
    /// Pins the M6 promise that the M3 reconcile path covers
    /// multi-paragraph plain content efficiently — no separate fast
    /// path needed.
    #[test]
    fn multi_paragraph_plain_reuses_existing_runs() {
        let mut text_area = TextArea::new();
        text_area.content = "line one\nline two".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;

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
        relayout(&mut arena, root);

        let kids_before = arena.children_of(root);
        assert_eq!(
            kids_before.len(),
            3,
            "two paragraphs → two Runs plus one LineBreak"
        );
        let run_a_before = kids_before[0];
        let break_before = kids_before[1];
        let run_b_before = kids_before[2];

        // Append a third paragraph.
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_content_from_external("line one\nline two\nline three".to_string());
        });
        relayout(&mut arena, root);

        let kids_after = arena.children_of(root);
        assert_eq!(kids_after.len(), 5);
        assert_eq!(kids_after[0], run_a_before, "para 0 Run reused");
        assert_eq!(kids_after[1], break_before, "line break reused");
        assert_eq!(kids_after[2], run_b_before, "para 1 Run reused");
    }

    /// Two keyed projections in reverse order: identity-keyed match
    /// must follow `key=` rather than position, so each projection's
    /// NodeKey tracks its key across the swap.
    #[test]
    fn keyed_projection_reorder_preserves_state() {
        // Build a fixture with two keyed projections in order [a, b].
        let mut text_area = TextArea::new();
        text_area.content = "X1Y2Z".to_string(); // ranges 1..2 and 3..4
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        let order: std::rc::Rc<std::cell::Cell<bool>> =
            std::rc::Rc::new(std::cell::Cell::new(false));
        let order_for_handler = order.clone();
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            let swapped = order_for_handler.get();
            let key_a = RsxKey::Local(0xA);
            let key_b = RsxKey::Local(0xB);
            let key_first = if swapped { key_b } else { key_a };
            let key_second = if swapped { key_a } else { key_b };
            render.range(1..2, move |_text_area_node| {
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_key(key_first)
                .with_prop(
                    "style",
                    ElementStylePropSchema {
                        width: Some(Length::px(20.0)),
                        height: Some(Length::px(20.0)),
                        ..Default::default()
                    },
                )
            });
            render.range(3..4, move |_text_area_node| {
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_key(key_second)
                .with_prop(
                    "style",
                    ElementStylePropSchema {
                        width: Some(Length::px(20.0)),
                        height: Some(Length::px(20.0)),
                        ..Default::default()
                    },
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
        relayout(&mut arena, root);

        let kids_before = arena.children_of(root);
        // Layout: Run "X" / projA / Run "Y" / projB / Run "Z".
        assert_eq!(kids_before.len(), 5);
        let key_a_before = kids_before[1];
        let key_b_before = kids_before[3];

        // Swap.
        order.set(true);
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .mark_content_dirty();
        });
        relayout(&mut arena, root);

        let kids_after = arena.children_of(root);
        assert_eq!(kids_after.len(), 5);
        // Position 1 used to be key=a; now it's key=b. Identity-keyed
        // match relocates the key=b NodeKey from old position 3 to
        // new position 1.
        assert_eq!(
            kids_after[1], key_b_before,
            "key=b projection migrated to position 1",
        );
        assert_eq!(
            kids_after[3], key_a_before,
            "key=a projection migrated to position 3",
        );
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
