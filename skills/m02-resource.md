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