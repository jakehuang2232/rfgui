//! Screen position → child NodeKey → run-local query → root char index
//! (decision A8). Plus caret-boundary rules for hits that fall between
//! children.
//!
//! For Run children: convert screen → run-local via the Run's layout
//! position, query `screen_position_to_local_char`, then add
//! `run.char_range.start`. For projection children: query the first
//! text-bearing descendant when possible; otherwise linearly interpolate
//! across the projection bounds per decision A8.

use crate::view::base_component::{Element, ElementTrait, Text};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::run::TextAreaTextRun;
use super::TextArea2;

#[derive(Clone, Copy)]
struct HitRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl TextArea2 {
    /// Resolve a screen-space `(x, y)` hit to a root-content char index.
    /// Always returns *some* char index — falls back to the nearest
    /// child boundary when the click misses every child rect.
    pub(super) fn cursor_char_at_screen(&self, arena: &NodeArena, x: f32, y: f32) -> usize {
        if self.children.is_empty() {
            return self.cursor_char.min(self.content.chars().count());
        }

        // First pass: direct hit test inside each child's bounds.
        // Run children: query glyph buffer via the Run's hit-test helper.
        // Projection children: DFS the projection subtree for the first
        // text-bearing element (Text / TextAreaTextRun) and query *its*
        // glyph buffer — mirrors v1's
        // `projection_fragment_cursor_char_from_viewport_position` which
        // delegated to a nested TextArea. Falls back to linear interp
        // across the projection's bounds when no text-bearing descendant
        // is found (image-only / icon-only projections).
        for (idx, &child_key) in self.children.iter().enumerate() {
            // Step 1: classify and bounds-check inside the closure.
            enum Hit {
                Run(usize),
                Projection(HitRect),
            }
            let kind = arena.with_element_taken_ref(child_key, |child, _| {
                let snap = child.box_model_snapshot();
                if let Some(run) = child.as_any().downcast_ref::<TextAreaTextRun>() {
                    if !point_in_rect(x, y, snap.x, snap.y, snap.width, snap.height) {
                        return None;
                    }
                    let local_x = x - snap.x;
                    let local_y = y - snap.y;
                    let local = run
                        .screen_position_to_local_char(local_x, local_y)
                        .unwrap_or(0);
                    let absolute = run.char_range.start + local;
                    return Some(Hit::Run(absolute));
                }
                let hit_rect = projection_hit_rect(child.as_ref(), x, y)?;
                Some(Hit::Projection(hit_rect))
            });
            match kind {
                Some(Some(Hit::Run(c))) => {
                    return c.min(self.content.chars().count());
                }
                Some(Some(Hit::Projection(hit_rect))) => {
                    let r = match self.child_char_ranges.get(idx) {
                        Some(r) => r.clone(),
                        None => continue,
                    };
                    if let Some(local_char) = glyph_local_char_in_projection(arena, child_key, x, y)
                    {
                        let span = r.end.saturating_sub(r.start);
                        return (r.start + local_char.min(span)).min(self.content.chars().count());
                    }
                    // No text-bearing descendant: fall back to linear
                    // interp across the projection's bounds.
                    let span = r.end.saturating_sub(r.start);
                    let cursor = if span == 0 || hit_rect.width <= f32::EPSILON {
                        r.start
                    } else {
                        let ratio = ((x - hit_rect.x) / hit_rect.width).clamp(0.0, 1.0);
                        let offset = (ratio * span as f32).round() as usize;
                        r.start + offset.min(span)
                    };
                    return cursor.min(self.content.chars().count());
                }
                _ => {}
            }
        }

        // Fallback: pick nearest child by vertical-band proximity. Both
        // Run and projection children participate so a click in a vertical
        // gap between a Run row and a projection still snaps somewhere
        // sensible.
        let mut nearest_char = 0usize;
        let mut nearest_dy = f32::INFINITY;
        let mut last_child_end = 0usize;
        for (idx, &child_key) in self.children.iter().enumerate() {
            arena.with_element_taken_ref(child_key, |child, _| {
                let snap = child.box_model_snapshot();
                let (range_start, range_end) =
                    if let Some(run) = child.as_any().downcast_ref::<TextAreaTextRun>() {
                        (run.char_range.start, run.char_range.end)
                    } else if let Some(r) = self.child_char_ranges.get(idx) {
                        (r.start, r.end)
                    } else {
                        return;
                    };
                last_child_end = range_end;
                let dy = if y < snap.y {
                    snap.y - y
                } else if y > snap.y + snap.height {
                    y - (snap.y + snap.height)
                } else {
                    0.0
                };
                if dy < nearest_dy {
                    nearest_dy = dy;
                    nearest_char = if x <= snap.x + snap.width * 0.5 {
                        range_start
                    } else {
                        range_end
                    };
                }
            });
        }
        if nearest_dy.is_infinite() {
            return last_child_end.min(self.content.chars().count());
        }
        nearest_char.min(self.content.chars().count())
    }
}

fn point_in_rect(x: f32, y: f32, rx: f32, ry: f32, rw: f32, rh: f32) -> bool {
    x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
}

fn projection_hit_rect(child: &dyn ElementTrait, x: f32, y: f32) -> Option<HitRect> {
    if let Some(element) = child.as_any().downcast_ref::<Element>() {
        let fragments = element.inline_fragment_rects();
        if !fragments.is_empty() {
            return fragments
                .iter()
                .find(|rect| point_in_rect(x, y, rect.x, rect.y, rect.width, rect.height))
                .map(|rect| HitRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                });
        }
    }
    let snap = child.box_model_snapshot();
    if point_in_rect(x, y, snap.x, snap.y, snap.width, snap.height) {
        Some(HitRect {
            x: snap.x,
            y: snap.y,
            width: snap.width,
            height: snap.height,
        })
    } else {
        None
    }
}

/// DFS the projection subtree rooted at `root_key` for the first
/// text-bearing element (`<Text>` or `TextAreaTextRun`) and run its
/// screen→local-char query. Returns `None` when the subtree has no
/// text-bearing descendant or when the hit fell outside its bounds.
fn glyph_local_char_in_projection(
    arena: &NodeArena,
    root_key: NodeKey,
    x: f32,
    y: f32,
) -> Option<usize> {
    if let Some(found) = query_local_char_on(arena, root_key, x, y) {
        return Some(found);
    }
    let mut stack: Vec<NodeKey> = arena.children_of(root_key).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if let Some(found) = query_local_char_on(arena, key, x, y) {
            return Some(found);
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

fn query_local_char_on(arena: &NodeArena, key: NodeKey, x: f32, y: f32) -> Option<usize> {
    arena
        .with_element_taken_ref(key, |el, _| {
            if let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() {
                let snap = run.box_model_snapshot();
                let local_x = x - snap.x;
                let local_y = y - snap.y;
                return run.screen_position_to_local_char(local_x, local_y);
            }
            if let Some(text) = el.as_any().downcast_ref::<Text>() {
                return text.screen_position_to_local_char(x, y);
            }
            None
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Layout, ParsedValue, PropertyId, Style};
    use crate::view::base_component::{
        ElementTrait, InlineMeasureContext, InlinePlacement,
    };

    #[test]
    fn projection_hit_test_ignores_union_box_gap_between_inline_fragments() {
        let mut projection = Element::new(0.0, 0.0, 0.0, 0.0);
        projection.set_intrinsic_size_as_percent_base(false);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Auto);
        style.insert(PropertyId::Height, ParsedValue::Auto);
        projection.apply_style(style);

        let mut text = Text::from_content("wrapped text");
        text.set_font_size(20.0);

        let mut arena = crate::view::test_support::new_test_arena();
        let projection_key = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(projection) as Box<dyn ElementTrait>,
        );
        crate::view::test_support::commit_child(
            &mut arena,
            projection_key,
            Box::new(text) as Box<dyn ElementTrait>,
        );

        arena.with_element_taken(projection_key, |el, arena| {
            el.measure_inline(
                InlineMeasureContext {
                    first_available_width: 48.0,
                    full_available_width: 48.0,
                    viewport_width: 300.0,
                    viewport_height: 200.0,
                    percent_base_width: Some(48.0),
                    percent_base_height: Some(80.0),
                },
                arena,
            );
            el.place_inline(
                InlinePlacement {
                    node_index: 0,
                    x: 10.0,
                    y: 10.0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 48.0,
                    available_height: 80.0,
                    viewport_width: 300.0,
                    viewport_height: 200.0,
                    percent_base_width: Some(48.0),
                    percent_base_height: Some(80.0),
                },
                arena,
            );
            el.place_inline(
                InlinePlacement {
                    node_index: 1,
                    x: 10.0,
                    y: 40.0,
                    offset_x: 0.0,
                    offset_y: 30.0,
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 48.0,
                    available_height: 80.0,
                    viewport_width: 300.0,
                    viewport_height: 200.0,
                    percent_base_width: Some(48.0),
                    percent_base_height: Some(80.0),
                },
                arena,
            );
        });

        let projection_node = arena.get(projection_key).expect("projection");
        let projection_ref = projection_node.element.as_ref();
        let element = projection_ref
            .as_any()
            .downcast_ref::<Element>()
            .expect("projection element");
        let fragments = element.inline_fragment_rects();
        assert!(fragments.len() >= 2, "expected wrapped inline fragments");

        let first = fragments[0];
        let second = fragments[1];
        let gap_x = first.x + first.width * 0.5;
        let gap_y = first.y + first.height + (second.y - (first.y + first.height)) * 0.5;
        let snap = projection_ref.box_model_snapshot();
        assert!(
            point_in_rect(gap_x, gap_y, snap.x, snap.y, snap.width, snap.height),
            "test point must be inside union box"
        );
        assert!(
            !fragments
                .iter()
                .any(|rect| point_in_rect(gap_x, gap_y, rect.x, rect.y, rect.width, rect.height)),
            "test point must be outside actual fragments"
        );
        assert!(projection_hit_rect(projection_ref, gap_x, gap_y).is_none());
    }
}
