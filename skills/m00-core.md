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