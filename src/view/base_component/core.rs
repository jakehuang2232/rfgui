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

    pub fn calculate_layout(
        &mut self,
        available_width: f32,
        available_height: f32,
        parent_x: f32,
        parent_y: f32,
        clamp_to_parent: bool,
    ) {
        let available_width = available_width.max(0.0);
        let available_height = available_height.max(0.0);

        if clamp_to_parent {
            let max_width = (available_width - self.position.x.max(0.0)).max(0.0);
            let max_height = (available_height - self.position.y.max(0.0)).max(0.0);
            self.layout_size = Size {
                width: self.size.width.max(0.0).min(max_width),
                height: self.size.height.max(0.0).min(max_height),
            };
        } else {
            self.layout_size = Size {
                width: self.size.width.max(0.0),
                height: self.size.height.max(0.0),
            };
        }

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
}
