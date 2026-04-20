---
name: m03-style-system
description: rfgui typed style system reference covering the Parsed / Computed / Layout three-layer model, Length and ColorLike rules, and percent-sizing semantics. Use whenever the user works with style={{ ... }} blocks, PropertyId, Length (px/percent), ColorLike, or asks why a percent width resolves to auto or why a style value is not accepted.
---

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
