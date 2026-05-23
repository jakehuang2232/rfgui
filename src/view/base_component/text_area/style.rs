//! TextArea local style bridge.

use crate::style::{
    ComputedStyle, PropertyId, Style, StyleComputeContext, compute_style_with_context,
};
use crate::view::renderer_adapter::{StyleCascadeContext, computed_parent_from_style_cascade};

use super::TextArea;

#[derive(Debug)]
pub(crate) struct TextAreaComputedStyleBridge {
    computed: ComputedStyle,
    has_font_family: bool,
    has_font_size: bool,
    has_font_weight: bool,
    has_color: bool,
    has_cursor: bool,
    has_line_height: bool,
    has_vertical_align: bool,
    #[allow(dead_code)]
    has_width: bool,
    #[allow(dead_code)]
    has_height: bool,
}

impl TextAreaComputedStyleBridge {
    /// Normalize a local `<TextArea style=...>` declaration against the
    /// inherited text cascade. This uses the same compute pipeline as retained
    /// style consumers, but TextArea needs authored-field masks and inherited
    /// parent context before applying fields, so it is deliberately a local
    /// bridge instead of a `ComputedStyleConsumer` implementation. Box-model
    /// fields may be accepted by the shared Element style schema, but this
    /// bridge only applies the TextArea text fields that already had visual
    /// effect.
    pub(crate) fn from_style(style: &Style, inherited: &StyleCascadeContext) -> Self {
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
            has_line_height: style.get(PropertyId::LineHeight).is_some(),
            has_vertical_align: style.get(PropertyId::VerticalAlign).is_some(),
            has_width: style.get(PropertyId::Width).is_some(),
            has_height: style.get(PropertyId::Height).is_some(),
            computed,
        }
    }
}

impl TextArea {
    pub(crate) fn apply_style_cold(
        &mut self,
        style: Option<&Style>,
        inherited: &StyleCascadeContext,
        explicit_font_size: Option<f32>,
        explicit_font: Option<String>,
    ) {
        self.apply_cold_inherited(inherited);

        if let Some(style) = style {
            let bridge = TextAreaComputedStyleBridge::from_style(style, inherited);
            self.apply_computed_style_bridge(&bridge);
        }

        // Explicit `font_size` / `font` props keep their existing
        // priority over both inherited values and `style.*`.
        if let Some(size) = explicit_font_size {
            self.font_size = size;
        }
        if let Some(family) = explicit_font {
            self.font_families = vec![family];
        }
    }

    pub(crate) fn apply_style_incremental(
        &mut self,
        style: &Style,
        inherited: &StyleCascadeContext,
    ) {
        let bridge = TextAreaComputedStyleBridge::from_style(style, inherited);
        self.apply_computed_style_bridge(&bridge);
        self.mark_content_dirty();
    }

    fn apply_cold_inherited(&mut self, inherited: &StyleCascadeContext) {
        if self.font_families.is_empty()
            && let Some(font_families) = inherited.inherited_font_families()
        {
            self.font_families = font_families.to_vec();
        }
        if let Some(inherited_size) = inherited.inherited_font_size() {
            self.font_size = inherited_size;
        }
        self.font_weight = inherited.inherited_font_weight().unwrap_or(400);
        if let Some(inherited_line_height) = inherited.inherited_line_height() {
            self.line_height = inherited_line_height;
        }
        if let Some(inherited_vertical_align) = inherited.inherited_vertical_align() {
            self.vertical_align = inherited_vertical_align;
        }
        if let Some(inherited_color) = inherited.inherited_color() {
            self.color = inherited_color;
        }
    }

    fn apply_computed_style_bridge(&mut self, bridge: &TextAreaComputedStyleBridge) {
        if bridge.has_font_family {
            self.font_families = bridge.computed.font_families.clone();
        }
        if bridge.has_font_size {
            self.font_size = bridge.computed.font_size;
        }
        if bridge.has_font_weight {
            self.font_weight = bridge.computed.font_weight;
        }
        if bridge.has_color {
            self.color = bridge.computed.color;
        }
        if bridge.has_cursor {
            self.cursor = bridge.computed.cursor;
        }
        if bridge.has_line_height {
            self.line_height = bridge.computed.line_height;
        }
        if bridge.has_vertical_align {
            self.vertical_align = bridge.computed.vertical_align;
        }
    }
}
