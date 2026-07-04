//! Text typography setters + style/inherited cascade.

use crate::style::{
    ColorLike, ComputedStyle, Cursor, Length, SizeValue, Style, StyleComputeContext, TextAlign,
    TextWrap, compute_style_with_context,
};
use crate::view::base_component::{DirtyFlags, Position, Size};
use crate::view::inline_formatting_context::InlineIfcAlignment;
use crate::view::renderer_adapter::{StyleCascadeContext, computed_parent_from_style_cascade};

use super::{Text, TextInlineIfcStyleMetadata};

#[derive(Debug)]
pub(crate) struct TextComputedStyleBridge {
    computed: ComputedStyle,
    width: Result<Option<f32>, String>,
    height: Result<Option<f32>, String>,
    has_font_family: bool,
    has_font_size: bool,
    has_font_weight: bool,
    has_color: bool,
    has_cursor: bool,
    has_text_wrap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextStyleApplyMode {
    Cold,
    Incremental,
}

impl TextComputedStyleBridge {
    /// Normalize a local `<Text style=...>` declaration against the
    /// inherited text cascade. This uses the same compute pipeline as retained
    /// style consumers, but Text needs authored-field masks and inherited
    /// parent context before applying fields, so it is deliberately a local
    /// bridge instead of a `ComputedStyleConsumer` implementation. Ancestor
    /// cascade still flows through `Text::apply_inherited`.
    pub(crate) fn from_style(style: &Style, inherited: &StyleCascadeContext) -> Self {
        use crate::style::PropertyId;

        let parent = computed_parent_from_style_cascade(inherited);

        let computed = compute_style_with_context(
            style,
            StyleComputeContext {
                parent: Some(&parent),
                viewport_width: inherited.viewport_width,
                viewport_height: inherited.viewport_height,
                root_font_size: inherited.root_font_size,
                hovered: false,
            },
        );

        Self {
            width: authored_size_value_px(
                style,
                PropertyId::Width,
                computed.width,
                "Text style.width",
            ),
            height: authored_size_value_px(
                style,
                PropertyId::Height,
                computed.height,
                "Text style.height",
            ),
            has_font_family: style.get(PropertyId::FontFamily).is_some(),
            has_font_size: style.get(PropertyId::FontSize).is_some(),
            has_font_weight: style.get(PropertyId::FontWeight).is_some(),
            has_color: style.get(PropertyId::Color).is_some(),
            has_cursor: style.get(PropertyId::Cursor).is_some(),
            has_text_wrap: style.get(PropertyId::TextWrap).is_some(),
            computed,
        }
    }
}

fn authored_size_value_px(
    style: &Style,
    property: crate::style::PropertyId,
    computed: SizeValue,
    context: &str,
) -> Result<Option<f32>, String> {
    if style.get(property).is_none() {
        return Ok(None);
    }
    match computed {
        SizeValue::Auto => Ok(None),
        SizeValue::Length(Length::Px(value)) => Ok(Some(value)),
        SizeValue::Length(Length::Zero) => Ok(Some(0.0)),
        SizeValue::Length(length @ Length::Calc(_)) => {
            if length.needs_percent_base() {
                return Err(format!("{context} does not support relative length"));
            }
            Ok(Some(length.resolve_without_percent_base(0.0, 0.0)))
        }
        SizeValue::Length(Length::Percent(_) | Length::Vh(_) | Length::Vw(_)) => {
            Err(format!("{context} does not support relative length"))
        }
    }
}

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

    pub(crate) fn inline_ifc_text_style_metadata(&self) -> TextInlineIfcStyleMetadata {
        TextInlineIfcStyleMetadata {
            font_size: self.font_size,
            line_height: self.line_height,
            font_weight: self.font_weight,
            brush: self.color.to_rgba_u8(),
            font_families: self.font_families.clone(),
        }
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
    /// font_size at convert time from `StyleCascadeContext`, so
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

    pub(crate) fn set_align(&mut self, align: InlineIfcAlignment) {
        if self.align != align {
            self.align = align;
            self.mark_measure_dirty();
        }
    }

    pub fn set_text_align(&mut self, align: TextAlign) {
        self.set_align(match align {
            TextAlign::Left => InlineIfcAlignment::Left,
            TextAlign::Center => InlineIfcAlignment::Center,
            TextAlign::Right => InlineIfcAlignment::Right,
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
        inherited: &StyleCascadeContext,
    ) {
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

        let _ = self.apply_style_with_computed_bridge(
            style,
            inherited,
            TextStyleApplyMode::Incremental,
        );
    }

    pub(crate) fn apply_style_cold(
        &mut self,
        style: Option<&Style>,
        inherited: &StyleCascadeContext,
    ) -> Result<(), String> {
        self.apply_style_with_computed_bridge(style, inherited, TextStyleApplyMode::Cold)
    }

    fn apply_style_with_computed_bridge(
        &mut self,
        style: Option<&Style>,
        inherited: &StyleCascadeContext,
        mode: TextStyleApplyMode,
    ) -> Result<(), String> {
        let bridge = style.map(|style| TextComputedStyleBridge::from_style(style, inherited));

        if let Some(bridge) = &bridge {
            self.apply_computed_text_bridge(bridge);
        }

        self.apply_inherited(inherited);

        let width = match bridge.as_ref().map(|bridge| &bridge.width) {
            Some(Ok(width)) => *width,
            Some(Err(err)) if mode == TextStyleApplyMode::Cold => return Err(err.clone()),
            Some(Err(_)) | None => None,
        };
        let height = match bridge.as_ref().map(|bridge| &bridge.height) {
            Some(Ok(height)) => *height,
            Some(Err(err)) if mode == TextStyleApplyMode::Cold => return Err(err.clone()),
            Some(Err(_)) | None => None,
        };

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

        Ok(())
    }

    fn apply_computed_text_bridge(&mut self, bridge: &TextComputedStyleBridge) {
        if bridge.has_font_family {
            self.set_fonts(bridge.computed.font_families.iter().cloned());
        }
        if bridge.has_font_size {
            self.set_font_size(bridge.computed.font_size);
        }
        if bridge.has_font_weight {
            self.set_font_weight(bridge.computed.font_weight);
        }
        if bridge.has_color {
            self.set_color(bridge.computed.color);
        }
        if bridge.has_cursor {
            self.set_cursor(bridge.computed.cursor);
        }
        if bridge.has_text_wrap {
            self.set_text_wrap(bridge.computed.text_wrap);
        }
    }

    /// 軌 A #7: apply an ancestor-derived `StyleCascadeContext` to any
    /// Text prop that the author didn't set explicitly. Returns
    /// `true` if any prop changed (so the caller can short-circuit
    /// redundant dirty-marking). Explicit values — anything the
    /// author set via a public setter — are preserved.
    pub(crate) fn apply_inherited(&mut self, inherited: &StyleCascadeContext) -> bool {
        let mut changed = false;
        if !self.font_family_explicit
            && let Some(font_families) = inherited.inherited_font_families()
            && !font_families.is_empty()
            && self.font_families != font_families
        {
            self.font_families = font_families.to_vec();
            self.mark_measure_dirty();
            changed = true;
        }
        if !self.font_size_explicit
            && let Some(fs) = inherited.inherited_font_size()
            && (self.font_size - fs).abs() > f32::EPSILON
        {
            self.font_size = fs;
            self.mark_measure_dirty();
            changed = true;
        }
        if !self.font_weight_explicit
            && let Some(fw) = inherited.inherited_font_weight()
            && self.font_weight != fw
        {
            self.font_weight = fw;
            self.mark_measure_dirty();
            changed = true;
        }
        if !self.color_explicit
            && let Some(color) = inherited.inherited_color()
        {
            self.color = Box::new(color);
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
            changed = true;
        }
        if !self.text_wrap_explicit {
            let next = inherited.inherited_text_wrap().unwrap_or(TextWrap::Wrap);
            if self.text_wrap != next {
                self.text_wrap = next;
                self.clear_layout_caches();
                self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
                changed = true;
            }
        }
        if !self.line_height_explicit
            && let Some(lh) = inherited.inherited_line_height()
            && (self.line_height - lh).abs() > f32::EPSILON
        {
            self.line_height = lh;
            self.mark_measure_dirty();
            changed = true;
        }
        if !self.cursor_explicit
            && let Some(cursor) = inherited.inherited_cursor()
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
        if let Some(va) = inherited.inherited_vertical_align()
            && self.vertical_align != va
        {
            self.vertical_align = va;
            changed = true;
        }
        changed
    }
}
