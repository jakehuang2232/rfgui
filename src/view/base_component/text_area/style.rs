//! TextArea local style bridge.

use crate::style::{
    Color, ComputedStyle, Cursor, PropertyId, Style, StyleComputeContext,
    compute_style_with_context,
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
    has_selection_background: bool,
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
            has_selection_background: style
                .selection()
                .is_some_and(|selection| selection.background_color().is_some()),
            has_width: style.get(PropertyId::Width).is_some(),
            has_height: style.get(PropertyId::Height).is_some(),
            computed,
        }
    }
}

/// What a TextArea draws with when neither its style nor an ancestor sets
/// the value.
pub(super) const DEFAULT_FONT_SIZE: f32 = 14.0;
pub(super) const DEFAULT_FONT_WEIGHT: u16 = 400;
pub(super) const DEFAULT_LINE_HEIGHT: f32 = 1.25;
pub(super) const DEFAULT_COLOR: Color = Color::rgb(17, 17, 17);
pub(super) const DEFAULT_SELECTION_BACKGROUND: Color = Color::rgba(71, 133, 240, 89);

/// The inherited text fields a TextArea's own style sets. The others take
/// the ancestors' value, or else the default.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TextAreaStyledFields {
    font_family: bool,
    font_size: bool,
    font_weight: bool,
    color: bool,
    line_height: bool,
    vertical_align: bool,
    selection_background: bool,
}

impl TextArea {
    pub(crate) fn apply_style_cold(
        &mut self,
        style: Option<&Style>,
        inherited: &StyleCascadeContext,
    ) {
        self.apply_style(style, inherited);
    }

    /// Applies an updated `style` prop, or its removal (`None`).
    pub(crate) fn apply_style_incremental(
        &mut self,
        style: Option<&Style>,
        inherited: &StyleCascadeContext,
    ) {
        self.apply_style(style, inherited);
        self.mark_content_dirty();
    }

    /// The style is the only source of these fields: whatever it does not
    /// set comes from the ancestors, or else the default.
    fn apply_style(&mut self, style: Option<&Style>, inherited: &StyleCascadeContext) {
        self.styled = TextAreaStyledFields::default();
        self.cursor = Cursor::Text;
        if let Some(style) = style {
            let bridge = TextAreaComputedStyleBridge::from_style(style, inherited);
            self.apply_computed_style_bridge(&bridge);
        }
        self.apply_inherited(inherited);
    }

    fn apply_computed_style_bridge(&mut self, bridge: &TextAreaComputedStyleBridge) {
        let styled = &mut self.styled;
        if bridge.has_font_family {
            self.font_families = bridge.computed.font_families.clone();
            styled.font_family = true;
        }
        if bridge.has_font_size {
            self.font_size = bridge.computed.font_size;
            styled.font_size = true;
        }
        if bridge.has_font_weight {
            self.font_weight = bridge.computed.font_weight;
            styled.font_weight = true;
        }
        if bridge.has_color {
            self.color = bridge.computed.color;
            styled.color = true;
        }
        if bridge.has_cursor {
            self.cursor = bridge.computed.cursor;
        }
        if bridge.has_line_height {
            self.line_height = bridge.computed.line_height;
            styled.line_height = true;
        }
        if bridge.has_vertical_align {
            self.vertical_align = bridge.computed.vertical_align;
            styled.vertical_align = true;
        }
        if bridge.has_selection_background
            && let Some(selection) = bridge.computed.selection_background_color
        {
            self.selection_background_color = selection;
            styled.selection_background = true;
        }
    }

    /// Gives every text field the style does not set the value an ancestor
    /// passes down, or else the default. Called after the style applies and
    /// by [`crate::view::fiber_work::recascade_text_subtree`] when an
    /// ancestor changes. Marks the content dirty when a field changes, so
    /// the runs pick it up.
    pub(crate) fn apply_inherited(&mut self, inherited: &StyleCascadeContext) -> bool {
        let styled = self.styled;
        let mut changed = false;
        let mut set = |changed_field: bool| changed |= changed_field;
        if !styled.font_family {
            let next = inherited
                .inherited_font_families()
                .filter(|families| !families.is_empty())
                .map(<[String]>::to_vec)
                .unwrap_or_default();
            set(replace(&mut self.font_families, next));
        }
        if !styled.font_size {
            let next = inherited.inherited_font_size().unwrap_or(DEFAULT_FONT_SIZE);
            set(replace(&mut self.font_size, next));
        }
        if !styled.font_weight {
            let next = inherited
                .inherited_font_weight()
                .unwrap_or(DEFAULT_FONT_WEIGHT);
            set(replace(&mut self.font_weight, next));
        }
        if !styled.color {
            let next = inherited.inherited_color().unwrap_or(DEFAULT_COLOR);
            set(replace(&mut self.color, next));
        }
        if !styled.line_height {
            let next = inherited
                .inherited_line_height()
                .unwrap_or(DEFAULT_LINE_HEIGHT);
            set(replace(&mut self.line_height, next));
        }
        if !styled.vertical_align {
            let next = inherited.inherited_vertical_align().unwrap_or_default();
            set(replace(&mut self.vertical_align, next));
        }
        if !styled.selection_background {
            let next = inherited
                .inherited_selection_background_color()
                .unwrap_or(DEFAULT_SELECTION_BACKGROUND);
            set(replace(&mut self.selection_background_color, next));
        }
        if changed {
            self.mark_content_dirty();
        }
        changed
    }
}

/// Writes `next` into `slot`; returns whether the value changed.
fn replace<T: PartialEq>(slot: &mut T, next: T) -> bool {
    if *slot == next {
        return false;
    }
    *slot = next;
    true
}
