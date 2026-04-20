---
name: m02-resource
description: Smart pointer selection and shared-mutability patterns for rfgui. Use whenever the user asks about Box/Rc/Arc/RefCell, needs to pick between them, is designing shared mutable state, or is deciding between Rc<RefCell<T>> and Arc<Mutex<T>> style patterns while working in this project.
---

# Smart Pointers

## Box<T>
- heap allocation

## Rc<T>
- shared ownership (single-thread)

## Arc<T>
- thread-safe Rc

## RefCell<T>
- runtime borrow checking

## Patterns

- Rc<RefCell<T>> → shared mutable (single-thread)
- Arc<Mutex<T>> → shared mutable (multi-thread)
