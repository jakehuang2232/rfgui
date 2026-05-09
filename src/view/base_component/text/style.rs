//! Text typography setters + style/inherited cascade.

use cosmic_text::Align;

use crate::style::{ColorLike, Cursor, Style, TextAlign, TextWrap};
use crate::view::base_component::{DirtyFlags, Position, Size};
use crate::view::renderer_adapter::InheritedTextStyle;

use super::Text;

impl Text {
    pub fn set_position(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::RUNTIME);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
        self.render_size = Size {
            width: width.max(0.0),
            height: height.max(0.0),
        };
        self.layout_override_width = None;
        self.layout_override_height = None;
        self.auto_width = false;
        self.auto_height = false;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    pub fn set_width(&mut self, width: f32) {
        self.size.width = width;
        self.render_size.width = width.max(0.0);
        self.layout_override_width = None;
        self.auto_width = false;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    pub fn set_height(&mut self, height: f32) {
        self.size.height = height;
        self.render_size.height = height.max(0.0);
        self.layout_override_height = None;
        self.auto_height = false;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    pub fn set_text(&mut self, content: impl Into<String>) {
        let next = content.into();
        if self.content != next {
            self.content = next;
            self.mark_measure_dirty();
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn set_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.color = Box::new(color);
        self.color_explicit = true;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
    }

    pub fn set_font(&mut self, font_family: impl Into<String>) {
        let raw = font_family.into();
        let families: Vec<String> = raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if self.font_families != families {
            self.font_families = families;
            self.mark_measure_dirty();
        }
        self.font_family_explicit = true;
    }

    pub fn set_fonts<I, S>(&mut self, font_families: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let next: Vec<String> = font_families
            .into_iter()
            .map(Into::into)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if self.font_families != next {
            self.font_families = next;
            self.mark_measure_dirty();
        }
        self.font_family_explicit = true;
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() > f32::EPSILON {
            self.font_size = font_size;
            self.mark_measure_dirty();
        }
        self.font_size_explicit = true;
    }

    /// Crate-visible read for the M6 cascade tests; Text resolves
    /// font_size at convert time from `InheritedTextStyle`, so
    /// exposing the stored value lets tests assert parent cascade
    /// reached the Text leaf correctly.
    #[cfg(test)]
    pub(crate) fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn set_line_height(&mut self, line_height: f32) {
        if (self.line_height - line_height).abs() > f32::EPSILON {
            self.line_height = line_height;
            self.mark_measure_dirty();
        }
        self.line_height_explicit = true;
    }

    pub fn set_vertical_align(&mut self, vertical_align: crate::style::VerticalAlign) {
        // No measure invalidation — vertical_align affects place only.
        self.vertical_align = vertical_align;
    }

    pub fn set_font_weight(&mut self, font_weight: u16) {
        let clamped = font_weight.clamp(100, 900);
        if self.font_weight != clamped {
            self.font_weight = clamped;
            self.mark_measure_dirty();
        }
        self.font_weight_explicit = true;
    }

    pub fn set_align(&mut self, align: Align) {
        if std::mem::discriminant(&self.align) != std::mem::discriminant(&align) {
            self.align = align;
            self.mark_measure_dirty();
        }
    }

    pub fn set_text_align(&mut self, align: TextAlign) {
        self.set_align(match align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        });
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
    }

    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn set_text_wrap(&mut self, text_wrap: TextWrap) {
        if self.text_wrap != text_wrap {
            self.text_wrap = text_wrap;
            self.clear_layout_caches();
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
        }
        self.text_wrap_explicit = true;
    }

    pub fn set_auto_width(&mut self, auto: bool) {
        if self.auto_width != auto {
            self.auto_width = auto;
            self.clear_layout_caches();
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
        }
    }

    pub fn set_auto_height(&mut self, auto: bool) {
        if self.auto_height != auto {
            self.auto_height = auto;
            self.clear_layout_caches();
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
        }
    }

    pub fn set_cursor(&mut self, cursor: Cursor) {
        if self.cursor != cursor {
            self.cursor = cursor;
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
        }
        self.cursor_explicit = true;
    }

    /// 軌 1 #8: incremental-path replay of the cold-path `style`
    /// fan-out. Called by `apply_update_to_text` when a `style` prop
    /// on a `<Text>` element is updated or removed.
    ///
    /// Mirror of `convert_text_element`'s style handling (lines
    /// ~950-996), with one addition: every author-controllable
    /// explicit flag is flipped back to `false` before applying the
    /// new style, so props removed from the declaration are free to
    /// pick up the ancestor cascade again via `apply_inherited`.
    ///
    /// Scope caveat: if a declaration is removed and no inherited
    /// value exists for it, the prior explicit value sticks (the
    /// cold path starts from `Text::new` defaults; the incremental
    /// path cannot cheaply reset to defaults without hardcoding
    /// them here). Ancestor cascade covers the common case.
    pub(crate) fn apply_style_incremental(
        &mut self,
        style: Option<&Style>,
        inherited: &InheritedTextStyle,
    ) {
        use crate::style::{ParsedValue, PropertyId};

        // Track 1 #10 scope fix: do NOT blanket-reset the per-prop
        // explicit flags before re-applying. A Text can source the
        // same prop (e.g. `font_size`) from an independent prop
        // (`<Text font_size={14} style={{color:...}}>`) — resetting
        // the flag would cause `apply_inherited` to overwrite the
        // author's value with the parent cascade on the next style
        // re-apply (observed: Switch label font-size jumping after
        // any re-render of its parent scene).
        //
        // Consequence: removing a declaration from the style block
        // won't re-pick the ancestor cascade on its own. Cold-path
        // rebuild still handles wholesale resets; callers that need
        // incremental declaration-removal cascade refill must author
        // an explicit reset value.

        let mut width: Option<f32> = None;
        let mut height: Option<f32> = None;

        if let Some(style) = style {
            if let Some(value) = style.get(PropertyId::Width) {
                width = crate::view::renderer_adapter::length_from_parsed_value(
                    value,
                    "Text style.width",
                )
                .ok()
                .flatten();
            }
            if let Some(value) = style.get(PropertyId::Height) {
                height = crate::view::renderer_adapter::length_from_parsed_value(
                    value,
                    "Text style.height",
                )
                .ok()
                .flatten();
            }
            if let Some(ParsedValue::FontFamily(font_family)) = style.get(PropertyId::FontFamily) {
                self.set_fonts(font_family.as_slice().iter().cloned());
            }
            if let Some(font_size) = crate::view::renderer_adapter::resolve_font_size_from_style(
                style,
                inherited.font_size.unwrap_or(inherited.root_font_size),
                inherited.root_font_size,
                inherited.viewport_width,
                inherited.viewport_height,
            ) {
                self.set_font_size(font_size);
            }
            if let Some(ParsedValue::FontWeight(font_weight)) = style.get(PropertyId::FontWeight) {
                self.set_font_weight(font_weight.value());
            }
            if let Some(ParsedValue::Color(color)) = style.get(PropertyId::Color) {
                self.set_color(color.clone());
            }
            if let Some(ParsedValue::Cursor(cursor)) = style.get(PropertyId::Cursor) {
                self.set_cursor(*cursor);
            }
            if let Some(ParsedValue::TextWrap(text_wrap)) = style.get(PropertyId::TextWrap) {
                self.set_text_wrap(*text_wrap);
            }
        }

        self.apply_inherited(inherited);

        if let Some(width) = width {
            self.set_width(width);
        } else {
            self.set_auto_width(true);
        }
        if let Some(height) = height {
            self.set_height(height);
        } else {
            self.set_auto_height(true);
        }
    }

    /// 軌 A #7: apply an ancestor-derived `InheritedTextStyle` to any
    /// Text prop that the author didn't set explicitly. Returns
    /// `true` if any prop changed (so the caller can short-circuit
    /// redundant dirty-marking). Explicit values — anything the
    /// author set via a public setter — are preserved.
    pub(crate) fn apply_inherited(
        &mut self,
        inherited: &InheritedTextStyle,
    ) -> bool {
        let mut changed = false;
        if !self.font_family_explicit
            && !inherited.font_families.is_empty()
            && self.font_families != inherited.font_families
        {
            self.font_families = inherited.font_families.clone();
            self.mark_measure_dirty();
            changed = true;
        }
        if !self.font_size_explicit
            && let Some(fs) = inherited.font_size
            && (self.font_size - fs).abs() > f32::EPSILON
        {
            self.font_size = fs;
            self.mark_measure_dirty();
            changed = true;
        }
        if !self.font_weight_explicit
            && let Some(fw) = inherited.font_weight
            && self.font_weight != fw
        {
            self.font_weight = fw;
            self.mark_measure_dirty();
            changed = true;
        }
        if !self.color_explicit
            && let Some(color) = &inherited.color
        {
            self.color = Box::new(color.clone());
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
            changed = true;
        }
        if !self.text_wrap_explicit
            && let Some(tw) = inherited.text_wrap
            && self.text_wrap != tw
        {
            self.text_wrap = tw;
            self.clear_layout_caches();
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
            changed = true;
        }
        if !self.line_height_explicit
            && let Some(lh) = inherited.line_height
            && (self.line_height - lh).abs() > f32::EPSILON
        {
            self.line_height = lh;
            self.mark_measure_dirty();
            changed = true;
        }
        if !self.cursor_explicit
            && let Some(cursor) = inherited.cursor
            && self.cursor != cursor
        {
            self.cursor = cursor;
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
            changed = true;
        }
        // vertical-align is layout-only (place pass); no measure dirty.
        // No explicit-flag tracking yet — explicit `set_vertical_align`
        // will be re-overwritten by an ancestor cascade. Matches Sprint
        // 3 acceptance footgun (`docs/design/inline-baseline.md` Risk
        // #6: inheritance footgun, document-only mitigation).
        if let Some(va) = inherited.vertical_align
            && self.vertical_align != va
        {
            self.vertical_align = va;
            changed = true;
        }
        changed
    }

    #[cfg(test)]
    pub(crate) fn inline_fragment_positions(&self) -> Vec<(String, Position)> {
        self.inline_plan
            .as_ref()
            .map(|plan| plan.runs.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter_map(|fragment| {
                fragment
                    .position
                    .map(|position| (fragment.content.clone(), position))
            })
            .collect()
    }
}
