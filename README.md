# 🧩 RFGUI

![example](https://github.com/user-attachments/assets/5274eb04-0329-46c2-9e14-424fd0dd3791)

**RFGUI** is a **🦀 Rust-based retained-mode GUI framework** built on top of a **🧠 frame graph–driven rendering architecture**.

It is designed for developers who want **🎛 explicit control over rendering passes**, predictable performance, and a **📐 modern retained UI model**, rather than an immediate-mode GUI.

RFGUI treats UI rendering as a **🔗 directed acyclic graph (DAG) of render passes**, similar to frame graph systems used in modern game engines.  
Each UI component contributes render passes and resources, which are composed and scheduled automatically.

## ✨ Key Characteristics

- 🧱 **Retained-mode GUI** — UI state is preserved and updated declaratively, instead of redrawn every frame
- 🧠 **Frame Graph architecture** — rendering is expressed as connected render passes with explicit resource dependencies
- 🧮 **Deterministic rendering order** — pass execution is derived from graph topology, not ad-hoc draw calls
- 🗂 **Explicit resource management** — textures, buffers, and render targets are modeled as graph resources
- 🚀 **Designed for modern GPU APIs** — suitable for rendering backends

RFGUI is **not** an immediate-mode GUI like egui or imgui.  
It is closer in spirit to **🏗 retained UI frameworks combined with 🎮 engine-style render pipelines**.

🛠 This project is currently under active development and focuses on **core architecture, correctness, and composability** before higher-level widgets.

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
cargo run -p example-01-window --bin 01_window
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
use rfgui::view::Element;
use rfgui::{Layout, Length};

#[component]
fn Card() -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(180.0),
            height: Length::px(100.0),
            display: Layout::flow().column(),
        }}>
            Hello Component
        </Element>
    }
}
```

### 2) Hand-written typed `create_tag_element(...)`

```rust
use rfgui::view::{Element, ElementPropSchema};
use rfgui::ui::{create_tag_element, RsxChildrenPolicy, RsxComponent, RsxNode, props};
use rfgui::{Border, BorderRadius, Color, Length, Padding, Style};

pub struct Card;

#[props]
pub struct CardProps {
    pub style: Option<Style>,
}

impl RsxComponent<CardProps> for Card {
    fn render(props: CardProps, children: Vec<RsxNode>) -> RsxNode {
        let mut style = props.style.unwrap_or_else(Style::new);
        style = style
            .with_padding(Padding::uniform(Length::px(12.0)))
            .with_border(Border::uniform(Length::px(2.0), &Color::hex("#1f2937")))
            .with_border_radius(BorderRadius::uniform(Length::px(10.0)));

        create_tag_element::<Element, _, _>(
            ElementPropSchema {
                anchor: None,
                style: Some(style),
                on_mouse_down: None,
                on_mouse_up: None,
                on_mouse_move: None,
                on_mouse_enter: None,
                on_mouse_leave: None,
                on_click: None,
                on_key_down: None,
                on_key_up: None,
                on_focus: None,
                on_blur: None,
            },
            children,
        )
    }
}

impl RsxChildrenPolicy for Card {
    const ACCEPTS_CHILDREN: bool = true;
}
```

## Key Semantics

RSX 的 `key` 目前分成兩種：

- local key：只影響同層 sibling 的 identity
- global key：要求同一輪 build 全域唯一，並可在跨 parent 搬移時保留 component state

```rust
use rfgui::ui::{GlobalKey, rsx};
use rfgui::view::Element;

let tree = rsx! {
    <Element style={{}}>
        <Element key="item-1" style={{}} />
        <Element key={GlobalKey::from("dialog-root")} style={{}} />
    </Element>
};
```

注意：

- 字串或數字 `key` 會走 local key，例如 `key="item-1"`
- `GlobalKey` 必須寫成 Rust expression，所以要用 `key={GlobalKey::from("dialog-root")}`
- `GlobalKey` 若在同一輪 build 重複出現，會直接報錯
- reconciliation identity 以 `type + key` 為準；`<Button key={...} />` 與 `<Element key={...} />` 不會視為同一個節點

## Development

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
```
