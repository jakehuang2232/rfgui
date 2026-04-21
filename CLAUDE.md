# rfgui Architectural Invariants

硬規則。每次動刀前先對齊。

## 1. Engine core 不依賴平台 backend

`rfgui/Cargo.toml` 的 dependencies **不可**含 winit、arboard、web-sys 等 platform-facing crate（feature gate 也不行）。

- Backend 實作住 `examples/` 或下游 consumer crate
- 規劃 viewport decoupling 時不提議把 winit 加進 rfgui crate
- 寫新 code 前先驗要加的 dep 不污染 rfgui/Cargo.toml
- 意圖：rfgui 可嵌進任何 host（非 winit event loop、embedded、headless、其他 framework）而不拖入無關依賴

現況基線：winit 只在 `examples/Cargo.toml`。arboard 已從 viewport src 拔除（Cargo.toml 殘留項待清）。

## 2. rsx-macro 不依賴具體 component types

`rsx-macro` crate 不 reference 任何具體 component（`Element` / `Text` / `Button` 等）。只 emit 走 `rfgui::ui::*` generic trait 抽象的 code。

- Tag-specific 行為（fast paths、type lookup、nested struct literal）走 call site 解析的 trait / assoc type
- Component 只透過 user 的 `#tag` token 參照，macro 內不出現型別名
- 意圖：rsx-macro 是 foundation proc-macro，coupling 到 view layer 會破 layering（與規則 1 同精神）
