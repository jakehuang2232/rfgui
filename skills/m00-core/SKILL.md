---
name: m00-core
description: Core Rust reference for ownership, smart pointers, and concurrency primitives used throughout rfgui. Use whenever the user asks about basic Rust language fundamentals (ownership rules, borrowing, Box/Rc/Arc/RefCell, Send/Sync, Mutex/RwLock) or needs a quick refresher on Rust building blocks while working in this project.
---

# Core Rust Reference

## Ownership

- Each value has one owner
- Borrowing:
    - &T (shared)
    - &mut T (exclusive)
- Lifetimes: 'a

## Smart Pointers

- Box<T>: heap
- Rc<T>: single-thread ref count
- Arc<T>: thread-safe ref count
- RefCell<T>: interior mutability

## Concurrency

- Send / Sync
- Mutex<T>
- RwLock<T>
