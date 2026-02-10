use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::TextPass;
use crate::{Color, HexColor};

use super::{ElementTrait, UiBuildContext};

#[derive(Clone, Copy, Debug)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
struct Size {
    width: f32,
    height: f32,
}

pub struct Text {
    position: Position,
    size: Size,
    layout_position: Position,
    layout_size: Size,
    should_render: bool,
    content: String,
    color: Box<dyn Color>,
    font_families: Vec<String>,
    font_size: f32,
    line_height: f32,
    opacity: f32,
}

impl Text {
    pub fn from_content(content: impl Into<String>) -> Self {
        Self::new(0.0, 0.0, 10_000.0, 10_000.0, content)
    }

    pub fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        content: impl Into<String>,
    ) -> Self {
        Self {
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
        }
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
    }

    pub fn set_text(&mut self, content: impl Into<String>) {
        self.content = content.into();
    }

    pub fn set_color<T: Color + 'static>(&mut self, color: T) {
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

        self.font_families = families;
    }

    pub fn set_fonts<I, S>(&mut self, font_families: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.font_families = font_families
            .into_iter()
            .map(Into::into)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        self.font_size = font_size;
    }

    pub fn set_line_height(&mut self, line_height: f32) {
        self.line_height = line_height;
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }
}

impl ElementTrait for Text {
    fn calculate_layout(
        &mut self,
        available_width: f32,
        available_height: f32,
        parent_x: f32,
        parent_y: f32,
    ) {
        let available_width = available_width.max(0.0);
        let available_height = available_height.max(0.0);

        self.layout_size = Size {
            width: self.size.width.max(0.0),
            height: self.size.height.max(0.0),
        };
        self.layout_position = Position {
            x: parent_x + self.position.x,
            y: parent_y + self.position.y,
        };

        let parent_left = parent_x;
        let parent_top = parent_y;
        let parent_right = parent_x + available_width;
        let parent_bottom = parent_y + available_height;
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
