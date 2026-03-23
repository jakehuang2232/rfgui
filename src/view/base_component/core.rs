use super::next_ui_node_id;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Position {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Size {
    pub width: f32,
    pub height: f32,
}

pub(crate) struct ElementCore {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub position: Position,
    pub size: Size,
    pub layout_position: Position,
    pub layout_size: Size,
    pub should_render: bool,
    pub should_paint: bool,
}

impl ElementCore {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new_with_id(next_ui_node_id(), x, y, width, height)
    }

    pub fn new_with_id(id: u64, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id,
            parent_id: None,
            position: Position { x, y },
            size: Size { width, height },
            layout_position: Position { x, y },
            layout_size: Size {
                width: width.max(0.0),
                height: height.max(0.0),
            },
            should_render: true,
            should_paint: true,
        }
    }

    pub fn set_position(&mut self, x: f32, y: f32) {
        self.position = Position { x, y };
    }

    pub fn set_x(&mut self, x: f32) {
        self.position.x = x;
    }

    pub fn set_y(&mut self, y: f32) {
        self.position.y = y;
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size = Size { width, height };
    }

    pub fn set_width(&mut self, width: f32) {
        self.size.width = width;
    }

    pub fn set_height(&mut self, height: f32) {
        self.size.height = height;
    }
}
