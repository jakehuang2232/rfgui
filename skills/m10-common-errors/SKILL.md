---
name: m10-common-errors
description: Quick-fix reference for common Rust compiler errors hit in rfgui — E0382 moved value, E0597 lifetime too short, E0502 borrow conflict, E0499 multiple mut borrows, E0277 missing trait bound. Use whenever the user pastes a cargo error message, mentions an E0xxx code, or hits a borrow-checker / trait-bound failure and wants a fast diagnosis.
---

# Common Error Fixes

| Error | Cause | Fix |
|------|------|-----|
| E0382 | moved value | clone / borrow |
| E0597 | lifetime too short | extend lifetime |
| E0502 | borrow conflict | split borrows |
| E0499 | multiple mut borrows | restructure |
| E0277 | missing trait | add bound / impl |
