---
name: m04-ui-layout
description: rfgui layout and scroll model reference. Use whenever the user works with flow/wrap layout, container sizing, scroll containers, ScrollDirection, scrollbars, scroll_offset, or asks why x/y absolute positioning is not available, or how scroll hit-testing and bubbling work in this engine.
---

# Layout System

## Principles

- no x/y positioning
- use container layout
- support flow + wrap

## Size

- width/height in style
- percent based on parent inner size

---

## Scroll Model

- ScrollDirection:
    - None / Vertical / Horizontal / Both

- handled via hit-test + bubble

## State

- scroll_offset
- content_size

## Scrollbar

- auto show/fade
- draggable thumb
- clickable track
