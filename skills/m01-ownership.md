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