---
name: m09-code-style
description: rfgui code style conventions — identifier casing, line length, and library-code error handling rules. Use whenever the user writes new code, renames items, formats code, or asks about project conventions. Apply proactively when editing any Rust file in this project to keep style consistent.
---

# Code Style

- snake_case: variables/functions
- PascalCase: types/traits
- SCREAMING_SNAKE_CASE: constants
- Max line length: 100
- Library code must avoid `unwrap()`; prefer `?`
