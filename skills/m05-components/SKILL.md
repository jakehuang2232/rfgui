---
name: m05-components
description: "rfgui component and RSX authoring rules — typed-only RSX, #[props] conventions, required vs optional props, host prop cold/apply/reset data flow, custom host paint (typed retained recorders, GpuPaintSource), style={{ }} usage, and typed event handlers. Use whenever the user writes or modifies a component or host prop, defines props, uses the rsx! macro, asks about #[component] / RsxComponent / RsxTag, writes a host element that paints itself or draws with a shader, or wonders why dynamic tags or string-based styles are rejected."
---

# Components / RSX

## Core

- typed-only RSX
- no runtime parsing

## Props

- #[props]
- Option<T> → optional
- non-Option → required

## Rules

- no Default for required props
- resolve Option in render

## Host prop data flow

When adding an optional host prop:

1. Declare `Option<T>` in the `#[props]` schema; the macro treats non-`Option` fields as required.
2. Forward `Some(value)` from `RsxComponent::render` into the `RsxNode`.
3. Prefer a concrete runtime field when the value has a natural default; normalize omission at the host boundary.
4. Decode and store it in `ElementTrait::ingest_props` for cold conversion.
5. Decode and replace it in `ElementTrait::apply_prop` for incremental reconciliation.
6. Restore the runtime default in `ElementTrait::reset_prop` when the prop disappears.
7. Do not mark layout/paint dirty for diagnostic-only metadata.
8. Test actual `rsx!` authoring plus default, cold, incremental, and reset paths.

---

## Host paint

A host element paints only through retained hooks; the engine records, reuses, and composites the result. Pick the first that fits:

1. Compose built-in `Element` / `Text` / `Image` / `Svg` nodes.
2. `ElementTrait::record_custom_leaf_paint` / `record_custom_wrapper_paint`: typed fills covering exactly the engine-provided bounds. The engine traverses children; never call `child.build(...)`.
3. `ElementTrait::prepared_gpu_paint_source`: a `GpuPaintSource` frozen in `Layoutable::prepare_paint_resources` (`requires_paint_resource_preparation` returns `true`). One WGSL draw validated by `GpuPaintProgram::new`; extent = ceil(layout size × device scale); bump the revision only when input bytes change. Reference: `examples/bin/01_window/scene_windows/particle_demo.rs`.

Rules:

- Hooks are pure reads; freeze per-frame state in `prepare_paint_resources`.
- Report paint changes through `retained_paint_signature` and dirty flags; a false-clean host reuses stale pixels.
- `Renderable::build` is still required while the Legacy renderer exists. Make it paint exactly what the retained hook records (`GpuPaintSource::paint` for a GPU source). It goes away with Legacy, so put nothing there that only Legacy can draw.
- Test pixels against the fixture's own geometry and colors, or a fresh viewport drawing the same state, never against Legacy output.

---

## Structure

- one props struct per component
- no duplicated schema
- render directly in RsxComponent

---

## Style

- use style={{ ... }}
- avoid dynamic insert
- all typed values

---

## Events

- typed handlers
- local state via use_state

---

## Forbidden

- runtime parsing
- dynamic tag registry
- string-based style
