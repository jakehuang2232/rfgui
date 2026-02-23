# AGENT.md

This document defines the core UI / Style / Layout conventions for this project (`rust-gui`) and serves as the single source of truth for implementation and refactoring.

## 1. Three-layer Style Model

1. Parsed Style (external input layer)
- Purpose: accept RSX / DSL / CSS-like expressions.
- Typing strategy: string values are disallowed; all values must use explicit types (for example, `Length::px(10.0)`, `Length::percent(50.0)`).
- Property keys must use enum (`PropertyId`) to avoid dynamic string keys.

2. ComputedStyle (engine core layer)
- Must be a struct (not a map representation).
- Reuse Parsed Style types where possible (`Length`, `BorderRadius`, `Border`, etc.).
- No string parsing at this layer.

3. LayoutState (solver output layer)
- Contains layout results only (position, size, baseline, etc.).
- Not part of style; do not mix declaration properties into it.

## 2. Typed Style Rules

- String style values are disallowed (except colors can be constructed via `Color::hex(...)`, but not through a raw-string parsing pipeline).
- `Length` is the base sizing unit:
  - `Length::px(f32)`
  - `Length::percent(f32)`
  - `Length::Zero`
- `%` rules (important):
  1. Effective only when parent content size is known.
  2. If parent size is unresolved, `%` is treated as `auto` (for `width/height`).
  3. `%` must not back-propagate to influence parent measurement.

## 3. Color System

- Color implementation is centralized in the `style::color` module.
- Always flow through `ColorLike`/`Color` typed APIs; avoid string parsing pipelines.
- All color fields in `ElementStylePropSchema` and `parsed_style` must use the ColorLike-oriented design.
- Style color values must use `ColorLike`.

## 4. Element and Props Policy

- Visual styling for `Element` is provided only via `style`.
- Remove legacy visual props (for example direct props like `background`, `border_color`, `border_width`).
- Usage:
  - `style={{ background: Color::hex("#000") }}`
  - `style={{ border: Border::uniform(...) }}`
  - `style={{ border_radius: BorderRadius::uniform(...) }}`
- `border-radius` and `border` are separate and not coupled.

## 5. Box Model API

### Padding
- Support fluent API: `uniform/all/x/y/top/right/bottom/left/xy`
- Example: `Padding::all(Length::px(10.0)).xy(Length::percent(20.0), Length::px(8.0))`

### Border
- CSS style: uniform + per-side overrides
- Support `top/right/bottom/left/x/y` overrides for width and color
- Example: `Border::uniform(Length::px(2.0), &Color::hex("#000")).top(Some(Length::px(4.0)), None)`

### BorderRadius
- Independent corners: `uniform/top/right/bottom/left/top_left/top_right/bottom_left/bottom_right`
- Outer and inner corner clipping must stay consistent (including border + inner clip).

## 6. Layout Direction (SwiftUI mindset + CSS expression)

- Avoid using `x/y` for general positioning; use container layout.
- Support flow / inline + flex-wrap behavior with line wrapping based on available width.
- Put `width/height` in `style`, typed as `Length`.
- `Length::percent` is based on parent inner size (when resolvable).

## 7. Scroll Model

- Do not use `overflow`; use `ScrollDirection` (SwiftUI style):
  - `None / Vertical / Horizontal / Both`
- Events: wheel uses hit-test + bubble; handled by the first scrollable container in path.
- Visual state: `scroll_offset`, `content_size` are maintained by Element.
- Scrollbar UI:
  - Auto show/fade (triggered by hover/scroll/drag)
  - Support thumb dragging
  - Support clicking track to jump
  - Both track and thumb must be rendered

## 8. Hover / Re-render / Stable Identity

- Hover state changes must trigger redraw.
- The same node id must stay stable; do not rebuild per frame and cause id drift.
- RSX render pipeline should avoid rebuilding the entire UI tree on every redraw (otherwise interactive state such as `scroll_offset` gets reset).

## 9. RSX Experience

- `style` props inside `rsx!` should be navigable (IDE jump-friendly).
- Keep `ElementStylePropSchema` aligned with the style system; do not keep deprecated fields (for example old `padding_x`-style fields).
- rsx components 偏好用宣告式結構

## 10. Implementation Guidelines

- Preserve type correctness first, then expand semantics.
- When adding style capabilities, update in this order:
  1. parsed_style
  2. computed_style
  3. schema / macro
  4. element sync
  5. renderer
  6. tests + example
- Prioritize regression tests (especially `%`, border radius, scroll, text measurement).
