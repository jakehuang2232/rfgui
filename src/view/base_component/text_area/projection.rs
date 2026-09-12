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
mod tests;
