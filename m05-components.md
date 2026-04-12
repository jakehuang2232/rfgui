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