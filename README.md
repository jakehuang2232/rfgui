# rfgui 🚀

A Rust GUI/rendering experiment with typed styles, RSX-based UI authoring, and a frame-graph-driven renderer.

![example](https://github.com/user-attachments/assets/928dabf3-f3bf-4ab2-a40b-a43a98f9d279)

## Features

- `wgpu` rendering pipeline
- RSX-style UI declaration via `rust-gui-rsx`
- `#[component]` for reusable UI composition
- Custom host-element extension by composing from `Element`
- Typed style/layout model (`Length`, `Border`, `BorderRadius`, `ColorLike`)
- Frame Graph abstraction for pass/resource orchestration
- Built-in interaction primitives: hover, scroll, bubbling events, transitions

## Quick Start

### Requirements

- Rust (stable recommended)
- A graphics-capable environment (macOS / Linux / Windows)

### Build

```bash
cargo build
```

### Run demo

```bash
cargo run --example 01_window
```

## Project Layout

```text
.
├── src/
│   ├── style/         # typed style model + parsing/computation
│   ├── ui/            # RSX tree, events, runtime, host elements
│   ├── view/          # viewport, render passes, frame graph
│   ├── transition/    # animation/transition system
│   └── shader/        # WGSL shaders
├── rsx-macro/         # proc-macro crate for RSX
└── examples/          # runnable demos
```

## Style Model

- Parsed Style: typed external style input (`PropertyId` + typed values)
- ComputedStyle: structured engine-level style (no string parsing)
- LayoutState: solver output only (position/size/baseline, etc.)

## Frame Graph

`src/view/frame_graph/` manages render-stage dependencies and resources.

Key files:
- `frame_graph.rs` (core graph/execution)
- `builder.rs` (graph construction)
- `render_node.rs` (node contracts)
- `texture_resource.rs`, `buffer_resource.rs` (resources)
- `slot.rs` (node input/output slots)

## Custom Components

### 1) `#[component]` reusable composition

```rust
use rfgui::ui::{component, rsx, RsxNode};
use rfgui::ui::host::Element;
use rfgui::{Display, FlowDirection, Length};

#[component]
fn Card() -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(180.0),
            height: Length::px(100.0),
            display: Display::Flow,
            flow_direction: FlowDirection::Column,
        }}>
            Hello Component
        </Element>
    }
}
```

### 2) Extend from `Element` via composition

```rust
use rfgui::ui::host::Element;
use rfgui::ui::{RsxChildrenPolicy, RsxNode, RsxPropSchema, RsxProps, RsxTag};
use rfgui::{Border, BorderRadius, Color, Length, Padding, Style};

pub struct Card;

pub struct CardProps {
    pub style: Style,
    pub children: Vec<RsxNode>,
}

impl RsxTag for Card {
    fn rsx_render(mut props: RsxProps, children: Vec<RsxNode>) -> Result<RsxNode, String> {
        let mut style = props.remove_t::<Style>("style")?.unwrap_or_else(Style::new);
        style = style
            .with_padding(Padding::uniform(Length::px(12.0)))
            .with_border(Border::uniform(Length::px(2.0), &Color::hex("#1f2937")))
            .with_border_radius(BorderRadius::uniform(Length::px(10.0)));

        let mut element_props = RsxProps::new();
        element_props.push("style", style);

        props.reject_remaining("Card")?;
        Element::rsx_render(element_props, children)
    }
}

impl RsxPropSchema for Card {
    type PropsSchema = CardProps;
}

impl RsxChildrenPolicy for Card {
    const ACCEPTS_CHILDREN: bool = true;
}
```

## Development

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
```
