# AGENTS.md

## 1. Question Routing

Route Rust / UI questions:

### Rust Core
- Ownership / borrowing → m01-ownership
- Smart pointers → m02-resource
- Error handling → m06-error-handling
- Concurrency → m07-concurrency
- Unsafe code → unsafe-checker

### UI / Layout / Engine
- Style system / typed style → m03-style-system
- Layout / flow / scroll → m04-ui-layout
- Component / RSX → m05-components

---

## 2. Code Style

- snake_case: variables/functions
- PascalCase: types/traits
- SCREAMING_SNAKE_CASE: constants
- Max line length: 100
- Library code must avoid unwrap(); prefer `?`

---

## 3. Common Error Fixes

| Error | Cause | Fix |
|------|------|-----|
| E0382 | moved value | clone / borrow |
| E0597 | lifetime too short | extend lifetime |
| E0502 | borrow conflict | split borrows |
| E0499 | multiple mut borrows | restructure |
| E0277 | missing trait | add bound / impl |