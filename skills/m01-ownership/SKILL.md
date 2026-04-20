---
name: m01-ownership
description: Rust ownership and borrowing guidance for rfgui. Use whenever the user hits borrow-checker errors (E0382 moved value, E0502/E0499 conflicting borrows), asks about move vs borrow, needs help restructuring code to satisfy the borrow checker, or is confused about ownership semantics while working in this codebase.
---

# Ownership / Borrowing

## Rules

- Move by default
- One mutable reference OR multiple immutable
- No dangling references

## Common Errors

### E0382 (moved value)
Fix:
- clone()
- borrow (&)

### E0502 / E0499
Fix:
- split borrows
- reduce scope
