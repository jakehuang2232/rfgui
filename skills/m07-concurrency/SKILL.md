---
name: m07-concurrency
description: rfgui concurrency reference — Send/Sync trait meaning, Mutex/RwLock primitives, and Arc-wrapped sharing patterns. Use whenever the user asks about thread-safety, sees Send/Sync trait bound errors, needs to share state across threads, or is choosing between Mutex and RwLock while working in this project.
---

# Concurrency

## Traits

- Send → move across threads
- Sync → share across threads

## Primitives

- Mutex<T>
- RwLock<T>

## Patterns

- Arc<Mutex<T>>
- Arc<RwLock<T>>
