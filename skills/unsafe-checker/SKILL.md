---
name: unsafe-checker
description: Review and author unsafe Rust code in rfgui — when it is justified, scope minimization rules, safe-API wrapping, invariant documentation, and a UB/pointer/lifetime checklist. Use whenever the user writes, reviews, or audits unsafe { } blocks, FFI bindings, raw pointer code, or asks whether a given piece of code needs unsafe at all.
---

# Unsafe Code

## When to use

- FFI
- performance critical
- low-level memory

## Rules

- minimize unsafe scope
- wrap unsafe in safe API
- document invariants

## Checklist

- no UB
- valid pointers
- correct lifetimes
