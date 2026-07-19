---
name: m05-components
description: "rfgui component and RSX authoring rules — typed-only RSX, #[props] conventions, required vs optional props, host prop cold/apply/reset data flow, style={{ }} usage, and typed event handlers. Use whenever the user writes or modifies a component or host prop, defines props, uses the rsx! macro, asks about #[component] / RsxComponent / RsxTag, or wonders why dynamic tags or string-based styles are rejected."
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
