---
name: m04-ui-layout
description: rfgui layout and scroll model reference. Use whenever the user works with inline, flow/wrap or flex layout, chooses between them, container sizing, scroll containers, ScrollDirection, scrollbars, scroll_offset, or asks why x/y absolute positioning is not available, or how scroll hit-testing and bubbling work in this engine.
---

# Layout System

## Principles

- no x/y positioning
- use container layout: Inline, Flow or Flex

## Layout Modes

- `Layout::Inline` (default)
    - text and inline children run in lines that wrap at the element's width, like CSS inline formatting
    - an inline child's horizontal padding and border stay on the line of the content they border
- `Layout::flow()`: children in order along a row or column
    - each child is measured against the container's whole inner size and keeps that size; Flow never resizes a child along the line
    - `wrap()`: a child that does not fit the rest of the line starts the next one
    - `no_wrap()` (default): the line overflows
- `cross_size(CrossSize::Stretch)` on `flow()` or `flex()`: children with an auto cross size take the cross size of the largest child on their line, so siblings line up (buttons and their dividers); it does not fill the container, so give a child a 100% width or height for that
- `Layout::flex()`: children on a single line, sized by `flex` grow / shrink / basis
    - a child with no width, basis or `min_width` sizes to its content and is not shrunk below it
    - when the line overflows, those children first share the room the others leave and are measured again at it, so their text wraps; shrink then takes any overflow left from the other children
    - unlike CSS, an auto basis is 0 rather than the content size (images and SVGs use their intrinsic size), so a child with an auto width and a `min_width` starts from 0

## Choosing a Mode

- text that must wrap next to an icon, a button or another item → `Layout::flex().row()`; a flow row lets the text take the whole width and push its siblings out
- items that keep their own size (chips, tags, buttons) → `Layout::flow().row()`, with `wrap()` to start new lines
- a vertical stack → `Layout::flow().column()`

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
