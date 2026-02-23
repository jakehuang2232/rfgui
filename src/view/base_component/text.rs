use std::cell::RefCell;
use std::collections::HashMap;

use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::TextPass;
use crate::{ColorLike, HexColor};
use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap};

use super::{
    BoxModelSnapshot, Element, ElementTrait, EventTarget, Layoutable, Position, Renderable, Size,
    UiBuildContext,
};

pub struct Text {
    element: Element,
    position: Position,
    size: Size,
    layout_position: Position,
    layout_size: Size,
    should_render: bool,
    content: String,
    color: Box<dyn ColorLike>,
    font_families: Vec<String>,
    font_size: f32,
    line_height: f32,
    opacity: f32,
    auto_width: bool,
    auto_height: bool,
    measure_revision: u64,
    cached_intrinsic_width: Option<(u64, f32)>,
    cached_height_for_width: Option<(u64, f32, f32)>,
}

thread_local! {
    static MEASURE_FONT_SYSTEM: RefCell<FontSystem> = RefCell::new(FontSystem::new());
    static MEASURE_TEXT_CACHE: RefCell<HashMap<TextMeasureCacheKey, (f32, f32)>> =
        RefCell::new(HashMap::new());
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextMeasureCacheKey {
    content: String,
    max_width_milli: i32,
    font_size_milli: i32,
    line_height_milli: i32,
    font_families: Vec<String>,
}

fn quantize_milli(value: f32) -> i32 {
    (value * 1000.0).round() as i32
}

fn make_measure_cache_key(
    content: &str,
    max_width: Option<f32>,
    font_size: f32,
    line_height: f32,
    font_families: &[String],
) -> TextMeasureCacheKey {
    TextMeasureCacheKey {
        content: content.to_string(),
        max_width_milli: max_width.map(quantize_milli).unwrap_or(-1),
        font_size_milli: quantize_milli(font_size),
        line_height_milli: quantize_milli(line_height),
        font_families: font_families.to_vec(),
    }
}

impl Text {
    pub fn from_content(content: impl Into<String>) -> Self {
        let mut text = Self::new(0.0, 0.0, 10_000.0, 10_000.0, content);
        text.set_auto_width(true);
        text.set_auto_height(true);
        text
    }

    pub fn from_content_with_id(id: u64, content: impl Into<String>) -> Self {
        let mut text = Self::new_with_id(id, 0.0, 0.0, 10_000.0, 10_000.0, content);
        text.set_auto_width(true);
        text.set_auto_height(true);
        text
    }

    pub fn new(x: f32, y: f32, width: f32, height: f32, content: impl Into<String>) -> Self {
        Self::new_with_id(0, x, y, width, height, content)
    }

    pub fn new_with_id(
        id: u64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        content: impl Into<String>,
    ) -> Self {
        Self {
            element: Element::new_with_id(id, x, y, width, height),
            position: Position { x, y },
            size: Size { width, height },
            layout_position: Position { x, y },
            layout_size: Size {
                width: width.max(0.0),
                height: height.max(0.0),
            },
            should_render: true,
            content: content.into(),
            color: Box::new(HexColor::new("#111111")),
            font_families: Vec::new(),
            font_size: 16.0,
            line_height: 1.25,
            opacity: 1.0,
            auto_width: false,
            auto_height: false,
            measure_revision: 0,
            cached_intrinsic_width: None,
            cached_height_for_width: None,
        }
    }

    fn mark_measure_dirty(&mut self) {
        self.measure_revision = self.measure_revision.wrapping_add(1);
        self.cached_intrinsic_width = None;
        self.cached_height_for_width = None;
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.element.set_position(x, y);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
        self.element.set_size(width, height);
        self.auto_width = false;
        self.auto_height = false;
    }

    pub fn set_width(&mut self, width: f32) {
        self.size.width = width;
        self.element.set_width(width);
        self.auto_width = false;
    }

    pub fn set_height(&mut self, height: f32) {
        self.size.height = height;
        self.element.set_height(height);
        self.auto_height = false;
    }

    pub fn set_text(&mut self, content: impl Into<String>) {
        let next = content.into();
        if self.content != next {
            self.content = next;
            self.mark_measure_dirty();
        }
    }

    pub fn set_color<T: ColorLike + 'static>(&mut self, color: T) {
        self.color = Box::new(color);
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
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        if (self.font_size - font_size).abs() > f32::EPSILON {
            self.font_size = font_size;
            self.mark_measure_dirty();
        }
    }

    pub fn set_line_height(&mut self, line_height: f32) {
        if (self.line_height - line_height).abs() > f32::EPSILON {
            self.line_height = line_height;
            self.mark_measure_dirty();
        }
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }

    pub fn set_auto_width(&mut self, auto: bool) {
        self.auto_width = auto;
    }

    pub fn set_auto_height(&mut self, auto: bool) {
        self.auto_height = auto;
    }
}

fn measure_text_size(
    content: &str,
    max_width: Option<f32>,
    font_size: f32,
    line_height: f32,
    font_families: &[String],
) -> (f32, f32) {
    let cache_key = make_measure_cache_key(content, max_width, font_size, line_height, font_families);
    if let Some(cached) = MEASURE_TEXT_CACHE.with(|cache| cache.borrow().get(&cache_key).copied()) {
        return cached;
    }

    MEASURE_FONT_SYSTEM.with(|slot| {
        let mut font_system = slot.borrow_mut();
        let mut buffer = Buffer::new(
            &mut font_system,
            Metrics::new(
                font_size.max(1.0),
                (font_size * line_height.max(0.8)).max(1.0),
            ),
        );
        buffer.set_wrap(&mut font_system, Wrap::WordOrGlyph);
        buffer.set_size(&mut font_system, max_width.map(|w| w.max(1.0)), None);

        let attrs = if let Some(first) = font_families.first() {
            Attrs::new().family(Family::Name(first.as_str()))
        } else {
            Attrs::new()
        };

        // Keep one visible line box for empty text to match previous layout semantics.
        let content = if content.is_empty() { " " } else { content };
        buffer.set_text(&mut font_system, content, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        let mut max_line_width = 0.0_f32;
        let mut line_count = 0_usize;
        for run in buffer.layout_runs() {
            line_count += 1;
            max_line_width = max_line_width.max(run.line_w);
        }
        let resolved_lines = line_count.max(1);
        let resolved_height = resolved_lines as f32 * buffer.metrics().line_height;
        let measured = (max_line_width.max(1.0), resolved_height.max(1.0));
        MEASURE_TEXT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache.insert(cache_key, measured);
            if cache.len() > 4096 {
                cache.clear();
            }
        });
        measured
    })
}

impl ElementTrait for Text {
    fn id(&self) -> u64 {
        self.element.id()
    }

    fn parent_id(&self) -> Option<u64> {
        self.element.parent_id()
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.element.set_parent_id(parent_id);
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.element.id(),
            parent_id: self.element.parent_id(),
            x: self.layout_position.x,
            y: self.layout_position.y,
            width: self.layout_size.width,
            height: self.layout_size.height,
            border_radius: 0.0,
            should_render: self.should_render,
        }
    }

    fn children(&self) -> Option<&[Box<dyn ElementTrait>]> {
        None
    }

    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl EventTarget for Text {
    fn dispatch_mouse_down(
        &mut self,
        event: &mut crate::ui::MouseDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_down(event, control);
    }

    fn dispatch_mouse_up(
        &mut self,
        event: &mut crate::ui::MouseUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_up(event, control);
    }

    fn dispatch_mouse_move(
        &mut self,
        event: &mut crate::ui::MouseMoveEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_move(event, control);
    }

    fn dispatch_click(
        &mut self,
        event: &mut crate::ui::ClickEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_click(event, control);
    }

    fn dispatch_key_down(
        &mut self,
        event: &mut crate::ui::KeyDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_key_down(event, control);
    }

    fn dispatch_key_up(
        &mut self,
        event: &mut crate::ui::KeyUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_key_up(event, control);
    }

    fn dispatch_focus(
        &mut self,
        event: &mut crate::ui::FocusEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_focus(event, control);
    }

    fn dispatch_blur(
        &mut self,
        event: &mut crate::ui::BlurEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_blur(event, control);
    }
}

impl Layoutable for Text {
    fn measured_size(&self) -> (f32, f32) {
        (self.size.width, self.size.height)
    }

    fn set_layout_width(&mut self, width: f32) {
        self.size.width = width;
        self.element.set_width(width);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.size.height = height;
        self.element.set_height(height);
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
        self.element.set_position(x, y);
    }

    fn measure(&mut self, constraints: crate::view::base_component::LayoutConstraints) {
        if !self.auto_width && !self.auto_height {
            return;
        }
        if self.auto_width {
            let intrinsic_width = if let Some((revision, width)) = self.cached_intrinsic_width {
                if revision == self.measure_revision {
                    width
                } else {
                    let (width, _) = measure_text_size(
                        self.content.as_str(),
                        None,
                        self.font_size,
                        self.line_height,
                        self.font_families.as_slice(),
                    );
                    self.cached_intrinsic_width = Some((self.measure_revision, width));
                    width
                }
            } else {
                let (width, _) = measure_text_size(
                    self.content.as_str(),
                    None,
                    self.font_size,
                    self.line_height,
                    self.font_families.as_slice(),
                );
                self.cached_intrinsic_width = Some((self.measure_revision, width));
                width
            };
            let available = constraints.max_width.max(1.0);
            self.size.width = intrinsic_width.min(available);
            self.element.set_width(self.size.width);
        }
        if self.auto_height {
            let effective_width = if self.auto_width {
                self.size.width.max(1.0)
            } else {
                self.size.width.min(constraints.max_width.max(1.0)).max(1.0)
            };
            let measured_height = if let Some((revision, cached_width, cached_height)) =
                self.cached_height_for_width
            {
                if revision == self.measure_revision && (cached_width - effective_width).abs() < 0.01
                {
                    cached_height
                } else {
                    let (_, height) = measure_text_size(
                        self.content.as_str(),
                        Some(effective_width),
                        self.font_size,
                        self.line_height,
                        self.font_families.as_slice(),
                    );
                    self.cached_height_for_width =
                        Some((self.measure_revision, effective_width, height));
                    height
                }
            } else {
                let (_, height) = measure_text_size(
                    self.content.as_str(),
                    Some(effective_width),
                    self.font_size,
                    self.line_height,
                    self.font_families.as_slice(),
                );
                self.cached_height_for_width = Some((self.measure_revision, effective_width, height));
                height
            };
            self.size.height = measured_height.max(1.0);
            self.element.set_height(self.size.height);
        }
    }

    fn place(&mut self, placement: crate::view::base_component::LayoutPlacement) {
        let available_width = placement.available_width.max(0.0);
        let available_height = placement.available_height.max(0.0);
        let max_width = (available_width - self.position.x.max(0.0)).max(0.0);
        let max_height = (available_height - self.position.y.max(0.0)).max(0.0);
        self.layout_size = Size {
            width: self.size.width.max(0.0).min(max_width),
            height: self.size.height.max(0.0).min(max_height),
        };
        self.layout_position = Position {
            x: placement.parent_x + self.position.x + placement.visual_offset_x,
            y: placement.parent_y + self.position.y + placement.visual_offset_y,
        };

        let parent_left = placement.parent_x + placement.visual_offset_x;
        let parent_top = placement.parent_y + placement.visual_offset_y;
        let parent_right = parent_left + available_width;
        let parent_bottom = parent_top + available_height;
        let self_left = self.layout_position.x;
        let self_top = self.layout_position.y;
        let self_right = self.layout_position.x + self.layout_size.width;
        let self_bottom = self.layout_position.y + self.layout_size.height;
        self.should_render = self.layout_size.width > 0.0
            && self.layout_size.height > 0.0
            && self_right > parent_left
            && self_left < parent_right
            && self_bottom > parent_top
            && self_top < parent_bottom;
    }
}

impl Renderable for Text {
    fn build(&mut self, graph: &mut FrameGraph, ctx: &mut UiBuildContext) {
        if !self.should_render || self.content.is_empty() {
            return;
        }

        let opacity = self.opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return;
        }

        let mut pass = TextPass::new(
            self.content.clone(),
            self.layout_position.x,
            self.layout_position.y,
            self.layout_size.width,
            self.layout_size.height,
            self.color.to_rgba_f32(),
            opacity,
            self.font_size,
            self.line_height,
            self.font_families.clone(),
        );
        pass.set_scissor_rect(None);
        ctx.push_pass(graph, pass);
    }
}

#[cfg(test)]
mod tests {
    use super::{ElementTrait, Layoutable, Text};
    use crate::view::base_component::{LayoutConstraints, LayoutPlacement};

    #[test]
    fn layout_clamps_to_parent_available_area() {
        let mut text = Text::new(0.0, 0.0, 10_000.0, 10_000.0, "demo");
        text.set_position(8.0, 4.0);
        text.measure(LayoutConstraints {
            max_width: 240.0,
            max_height: 140.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(140.0),
        });
        text.place(LayoutPlacement {
            parent_x: 40.0,
            parent_y: 40.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 240.0,
            available_height: 140.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(140.0),
        });

        let snapshot = text.box_model_snapshot();
        assert_eq!(snapshot.x, 48.0);
        assert_eq!(snapshot.y, 44.0);
        assert_eq!(snapshot.width, 232.0);
        assert_eq!(snapshot.height, 136.0);
    }

    #[test]
    fn auto_height_accounts_for_wrapped_lines() {
        let mut text = Text::from_content("123456789012345678901234567890");
        text.set_width(60.0);
        text.set_auto_height(true);
        text.measure(LayoutConstraints {
            max_width: 60.0,
            max_height: 200.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
        });
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 60.0,
            available_height: 200.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
        });

        let snapshot = text.box_model_snapshot();
        assert_eq!(snapshot.width, 60.0);
        assert!(snapshot.height > 20.0);
    }

    #[test]
    fn auto_width_for_cjk_text_is_not_underestimated() {
        let mut text = Text::from_content("This is a Chinese text segment");
        text.measure(LayoutConstraints {
            max_width: 300.0,
            max_height: 200.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(200.0),
        });
        text.place(LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 200.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(200.0),
        });
        let snapshot = text.box_model_snapshot();
        assert!(snapshot.width >= 80.0);
    }
}
