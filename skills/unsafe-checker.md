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