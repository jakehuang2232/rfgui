# Style System

## Three Layers

1. Parsed Style
- typed input only
- no string values
- enum PropertyId

2. ComputedStyle
- struct-based
- no parsing

3. LayoutState
- layout result only

---

## Typed Rules

- no string styles
- use Length / ColorLike

### Length
- px
- percent
- Zero

### % rules

1. requires definite parent size
2. else → auto
3. no back-propagation