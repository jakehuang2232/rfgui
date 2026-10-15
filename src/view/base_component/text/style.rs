//! Text typography setters + style/inherited cascade.

use crate::style::{
    ColorLike, ComputedStyle, Cursor, Style, StyleComputeContext, TextAlign, TextWrap,
    compute_style_with_context,
};
use crate::view::base_component::{DirtyFlags, Position, Size};
use crate::view::inline_formatting_context::InlineIfcAlignment;
use crate::view::renderer_adapter::{StyleCascadeContext, computed_parent_from_style_cascade};

use super::{
    DEFAULT_COLOR, DEFAULT_FONT_SIZE, DEFAULT_FONT_WEIGHT, DEFAULT_LINE_HEIGHT, Text,
    TextExplicitProps, TextInlineIfcStyleMetadata,
};

#[derive(Debug)]
pub(crate) struct TextComputedStyleBridge {
    computed: ComputedStyle,
    has_font_family: bool,
    has_font_size: bool,
    has_font_weight: bool,
    has_color: bool,
    has_cursor: bool,
    has_text_wrap: bool,
    has_line_height: bool,
    has_text_align: bool,
    has_vertical_align: bool,
    has_transform: bool,
    has_transform_origin: bool,
    has_backface_visibility: bool,
    has_opacity: bool,
    has_transition: bool,
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
            has_font_family: style.get(PropertyId::FontFamily).is_some(),
            has_font_size: style.get(PropertyId::FontSize).is_some(),
            has_font_weight: style.get(PropertyId::FontWeight).is_some(),
            has_color: style.get(PropertyId::Color).is_some(),
            has_cursor: style.get(PropertyId::Cursor).is_some(),
            has_text_wrap: style.get(PropertyId::TextWrap).is_some(),
            has_line_height: style.get(PropertyId::LineHeight).is_some(),
            has_text_align: style.get(PropertyId::TextAlign).is_some(),
            has_vertical_align: style.get(PropertyId::VerticalAlign).is_some(),
            has_transform: style.get(PropertyId::Transform).is_some(),
            has_transform_origin: style.get(PropertyId::TransformOrigin).is_some(),
            has_backface_visibility: style.get(PropertyId::BackfaceVisibility).is_some(),
            has_opacity: style.get(PropertyId::Opacity).is_some(),
            has_transition: style.get(PropertyId::Transition).is_some(),
            computed,
        }
    }
}

impl Text {
    pub fn set_position(&mut self, x: f32, y: f32) {
        if self.position.x.to_bits() == x.to_bits() && self.position.y.to_bits() == y.to_bits() {
            return;
        }
        self.position = Position { x, y };
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::RUNTIME);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
        self.layout_override_width = None;
        self.layout_override_height = None;
        self.auto_width = false;
        self.auto_height = false;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    pub fn set_width(&mut self, width: f32) {
        self.size.width = width;
        self.layout_override_width = None;
        self.auto_width = false;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    pub fn set_height(&mut self, height: f32) {
        self.size.height = height;
        self.layout_override_height = None;
        self.auto_height = false;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
    }

    pub fn set_text(&mut self, content: impl Into<String>) {
        let next = content.into();
        if self.content != next {
            self.content_hash = super::content_signature(&next);
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
            brush: self.drawn_color().to_rgba_u8(),
            font_families: self.font_families.clone(),
            vertical_align: self.vertical_align,
        }
    }

    pub(super) fn has_explicit_color(&self) -> bool {
        self.explicit_props.contains(TextExplicitProps::COLOR)
    }

    pub fn set_color<T: ColorLike + 'static>(&mut self, color: T) {
        if self.inherited_color_override.take().is_some() {
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
        }
        if self.color.to_rgba_u8() == color.to_rgba_u8() {
            self.explicit_props.insert(TextExplicitProps::COLOR);
            return;
        }
        self.color = Box::new(color);
        self.explicit_props.insert(TextExplicitProps::COLOR);
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
        self.explicit_props.insert(TextExplicitProps::FONT_FAMILY);
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
        self.explicit_props.insert(TextExplicitProps::FONT_FAMILY);
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() > f32::EPSILON {
            self.font_size = font_size;
            self.mark_measure_dirty();
        }
        self.explicit_props.insert(TextExplicitProps::FONT_SIZE);
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
        self.explicit_props.insert(TextExplicitProps::LINE_HEIGHT);
    }

    pub fn set_vertical_align(&mut self, vertical_align: crate::style::VerticalAlign) {
        if self.vertical_align != vertical_align {
            self.vertical_align = vertical_align;
            // Standalone shaping ignores vertical-align, but an ancestor
            // inline IFC includes it in the child's style/cache key.
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
        }
        self.explicit_props
            .insert(TextExplicitProps::VERTICAL_ALIGN);
    }

    pub fn set_font_weight(&mut self, font_weight: u16) {
        let clamped = font_weight.clamp(100, 900);
        if self.font_weight != clamped {
            self.font_weight = clamped;
            self.mark_measure_dirty();
        }
        self.explicit_props.insert(TextExplicitProps::FONT_WEIGHT);
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
        if self.opacity.to_bits() == opacity.to_bits() {
            return;
        }
        self.opacity = opacity;
        self.dirty_flags = self
            .dirty_flags
            .union(DirtyFlags::PAINT)
            .union(DirtyFlags::COMPOSITE);
    }

    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn set_transform(&mut self, transform: crate::style::Transform) {
        if self.transform == transform {
            return;
        }
        self.transform = transform;
        self.update_resolved_transform();
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::RUNTIME);
    }

    pub fn set_transform_origin(&mut self, transform_origin: crate::style::TransformOrigin) {
        if self.transform_origin == transform_origin {
            return;
        }
        self.transform_origin = transform_origin;
        self.update_resolved_transform();
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::RUNTIME);
    }

    pub fn transform(&self) -> &crate::style::Transform {
        &self.transform
    }

    pub fn transform_origin(&self) -> crate::style::TransformOrigin {
        self.transform_origin
    }

    pub fn set_backface_visibility(
        &mut self,
        backface_visibility: crate::style::BackfaceVisibility,
    ) {
        if self.backface_visibility == backface_visibility {
            return;
        }
        self.backface_visibility = backface_visibility;
        self.update_resolved_transform();
    }

    pub fn backface_visibility(&self) -> crate::style::BackfaceVisibility {
        self.backface_visibility
    }

    pub fn set_text_wrap(&mut self, text_wrap: TextWrap) {
        if self.text_wrap != text_wrap {
            self.text_wrap = text_wrap;
            self.clear_layout_caches();
            self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
        }
        self.explicit_props.insert(TextExplicitProps::TEXT_WRAP);
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
        self.explicit_props.insert(TextExplicitProps::CURSOR);
    }

    /// Replays the cold-path `style` handling when the `style` prop of a
    /// `<Text>` is updated or removed (`None`).
    pub(crate) fn apply_style_incremental(
        &mut self,
        style: Option<&Style>,
        inherited: &StyleCascadeContext,
    ) {
        self.apply_style_with_computed_bridge(style, inherited, TextStyleApplyMode::Incremental);
    }

    pub(crate) fn apply_style_cold(
        &mut self,
        style: Option<&Style>,
        inherited: &StyleCascadeContext,
    ) {
        self.apply_style_with_computed_bridge(style, inherited, TextStyleApplyMode::Cold);
    }

    fn apply_style_with_computed_bridge(
        &mut self,
        style: Option<&Style>,
        inherited: &StyleCascadeContext,
        mode: TextStyleApplyMode,
    ) {
        let bridge = style.map(|style| TextComputedStyleBridge::from_style(style, inherited));
        // As for an Element, the first build does not transition.
        let previous =
            (mode == TextStyleApplyMode::Incremental).then(|| self.transition_snapshot());
        self.transitions = bridge
            .as_ref()
            .filter(|bridge| bridge.has_transition)
            .map(|bridge| bridge.computed.transition.clone())
            .unwrap_or_default();

        // The style is the only source of these fields: whatever it does
        // not set comes from the ancestors, or else the default.
        self.explicit_props = TextExplicitProps::default();
        if let Some(bridge) = &bridge {
            self.apply_computed_text_bridge(bridge);
        } else {
            self.set_transform(crate::style::Transform::default());
            self.set_transform_origin(crate::style::TransformOrigin::center());
            self.set_backface_visibility(crate::style::BackfaceVisibility::Visible);
            self.set_text_align(TextAlign::Left);
            self.set_opacity(1.0);
        }

        self.apply_inherited(inherited);
        if let Some(previous) = previous {
            self.queue_style_transitions(&previous);
        }
    }

    fn apply_computed_text_bridge(&mut self, bridge: &TextComputedStyleBridge) {
        if bridge.has_transform {
            self.set_transform(bridge.computed.transform.clone());
        } else {
            self.set_transform(crate::style::Transform::default());
        }
        if bridge.has_transform_origin {
            self.set_transform_origin(bridge.computed.transform_origin);
        } else {
            self.set_transform_origin(crate::style::TransformOrigin::center());
        }
        self.set_backface_visibility(if bridge.has_backface_visibility {
            bridge.computed.backface_visibility
        } else {
            crate::style::BackfaceVisibility::Visible
        });
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
        if bridge.has_line_height {
            self.set_line_height(bridge.computed.line_height);
        }
        if bridge.has_vertical_align {
            self.set_vertical_align(bridge.computed.vertical_align);
        }
        self.set_text_align(if bridge.has_text_align {
            bridge.computed.text_align
        } else {
            TextAlign::Left
        });
        self.set_opacity(if bridge.has_opacity {
            bridge.computed.opacity
        } else {
            1.0
        });
    }

    /// Gives every field the style does not set the value an ancestor
    /// passes down, or else the default. Returns whether any field changed.
    pub(crate) fn apply_inherited(&mut self, inherited: &StyleCascadeContext) -> bool {
        let explicit = self.explicit_props;
        let unset = |flag| !explicit.contains(flag);
        let mut changed = false;
        let mut measure = false;
        if unset(TextExplicitProps::FONT_FAMILY) {
            let next = inherited
                .inherited_font_families()
                .filter(|families| !families.is_empty())
                .map(<[String]>::to_vec)
                .unwrap_or_default();
            if self.font_families != next {
                self.font_families = next;
                measure = true;
            }
        }
        if unset(TextExplicitProps::FONT_SIZE) {
            let next = inherited.inherited_font_size().unwrap_or(DEFAULT_FONT_SIZE);
            if (self.font_size - next).abs() > f32::EPSILON {
                self.font_size = next;
                measure = true;
            }
        }
        if unset(TextExplicitProps::FONT_WEIGHT) {
            let next = inherited
                .inherited_font_weight()
                .unwrap_or(DEFAULT_FONT_WEIGHT);
            if self.font_weight != next {
                self.font_weight = next;
                measure = true;
            }
        }
        if unset(TextExplicitProps::LINE_HEIGHT) {
            let next = inherited
                .inherited_line_height()
                .unwrap_or(DEFAULT_LINE_HEIGHT);
            if (self.line_height - next).abs() > f32::EPSILON {
                self.line_height = next;
                measure = true;
            }
        }
        if measure {
            self.mark_measure_dirty();
            changed = true;
        }
        if unset(TextExplicitProps::COLOR) {
            let next = inherited.inherited_color().unwrap_or(DEFAULT_COLOR);
            if self.color.to_rgba_u8() != next.to_rgba_u8() {
                self.color = Box::new(next);
                self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
                changed = true;
            }
        }
        if unset(TextExplicitProps::TEXT_WRAP) {
            let next = inherited.inherited_text_wrap().unwrap_or(TextWrap::Wrap);
            if self.text_wrap != next {
                self.text_wrap = next;
                self.clear_layout_caches();
                self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
                changed = true;
            }
        }
        if unset(TextExplicitProps::CURSOR) {
            let next = inherited.inherited_cursor().unwrap_or(Cursor::Default);
            if self.cursor != next {
                self.cursor = next;
                self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
                changed = true;
            }
        }
        if unset(TextExplicitProps::VERTICAL_ALIGN) {
            let next = inherited.inherited_vertical_align().unwrap_or_default();
            if self.vertical_align != next {
                self.vertical_align = next;
                self.dirty_flags = self.dirty_flags.union(DirtyFlags::ALL);
                changed = true;
            }
        }
        changed
    }
}
