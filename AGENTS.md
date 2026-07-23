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

## 3. 時間 API 必須走 crate 封裝

不可直接使用 `std::time::Instant::now()` 或 `std::time::SystemTime`。

- 需要計時時使用 `crate::time::Instant::now()`
- 原因：`std::time::Instant` / `SystemTime` 在 `wasm32-unknown-unknown` 不支援；`crate::time::Instant` 會在 wasm target 切到 `web_time::Instant`
- 新增或修改 code 前先確認不會觸發 `build.rs` 的 wasm time guard

## 4. Test 程式碼與 production 程式碼分檔

`#[cfg(test)] mod X { … }` 不可 inline 在 production 檔案裡，也不可用 `include!("…tests.rs")` 把 test 文字內含進 production 檔。

**擺放**：`foo.rs` 的 test module 搬到 `foo/X.rs`，production 檔只留兩行宣告。

```rust
// foo.rs
#[cfg(test)]
mod tests;
```

- 用子模組（`foo/tests.rs`）而非兄弟檔（`foo_tests.rs`）：子模組看得到 parent 的私有項，`use super::*` 與存取語意完全不變
- 檔名沿用原本的 mod 名，不改名。一個檔案有多個 test module 時各自成檔（`foo/nested_scroll_tests.rs`、`foo/scroll_host_tests.rs`），這本身就是按對象分檔
- `foo/mod.rs` 的 test 直接放 `foo/X.rs`

**分檔門檻**：單一 test 檔超過 **800 行**須按被測對象再拆。

```
foo/tests.rs              共用 imports + fixtures + `mod <subject>;` 宣告
foo/tests/<subject>_tests.rs   `use super::*;` + 該對象的 #[test] fn
```

- 一個 subject 檔對應一個被測對象或一組行為，不用行數硬切
- 子檔靠 `use super::*;` 取得 parent 的 fixtures，不需要改任何 visibility
- 共用 fixtures 留在 `tests.rs`；它是 test 基礎建設，不受 800 行上限拘束
- **搬進子模組時 `super::` 要提升一層**：原本在 `foo::tests` 的 `super::bar()` 進到 `foo::tests::subject` 後要寫 `super::super::bar()`。只提升路徑開頭那一個 `super::`

**例外：掛在 production item 上的 `#[cfg(test)]` 原地保留。**

```rust
#[cfg(test)]
pub(crate) fn peek_internal_state(&self) -> u32 { self.counter }
```

這類 test-only 存取器是 production 型別的一部分，需要存取私有欄位，搬到 test 模組會編不過。規則只約束 `mod X { … }` 區塊。

**驗證**：搬移是純機械操作，`cargo test` 的 test 總數必須不變。數字變了就是搬錯了。
