use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutState {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub baseline: Option<f32>,
}

impl LayoutState {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            content_width: width,
            content_height: height,
            baseline: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct LayoutStateTree {
    states: HashMap<u64, LayoutState>,
}

impl LayoutStateTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, node_id: u64, state: LayoutState) {
        self.states.insert(node_id, state);
    }

    pub fn get(&self, node_id: u64) -> Option<&LayoutState> {
        self.states.get(&node_id)
    }

    pub fn get_mut(&mut self, node_id: u64) -> Option<&mut LayoutState> {
        self.states.get_mut(&node_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u64, &LayoutState)> {
        self.states.iter()
    }
}
