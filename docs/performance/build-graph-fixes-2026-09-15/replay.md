# 重播

修正來源為 `b138789`；比較基線為 `0c422b7`，中間批次為 `329f332` 與 `accb8e7`。不要把診斷 overlay 套到使用者原本的 main working directory。

在獨立 checkout 選定其中一個 commit，套用本目錄 `replay-overlay.patch`（`patch -p1`）。此 overlay 加入 test-only harness／觀察欄位／profiling scopes，並只在隔離場景中把 animation 初值設為 false、Inspector debug panel 設為展開。沒有跳過 hover 的反事實捷徑，也沒有 selector override 或放寬 fallback／validation。

在該隔離目錄執行：

```sh
cargo test --offline -p examples --bin 01_window native_build_graph_diagnosis --no-run
```

需要依賴快取與可存取的 Metal 裝置。由輸出中的 `Executable` 取得 binary 路徑；將 build script 生成的 `target/debug/assets` 複製到 test binary 旁的 `target/debug/deps/assets`。各 checkpoint 都必須使用相同完整素材。

將 `RFGUI_REPLAY_BINARY` 設為該絕對 binary 路徑後，執行本目錄的 `run_replay.py baseline`／`slots`／`hover`／`projection`。每次只有一個 process；不要與 Cargo compile 或其他 GPU benchmark 同時執行。baseline 與 projection 另加第二參數 `pixels`，額外進行兩個獨立 process 的像素擷取，不混入 timing runs。

`run_replay.py` 的六輪順序是 artifact、legacy、legacy、artifact、artifact、legacy。`summarize.py` 與 `compare_pixels.py` 可重建統計；後者需要 NumPy，能讀 `.rgba` 與 `.rgba.gz`。固定輸出尺寸 2560×1600 RGBA。`compress_pixels.py` 只做無損壓縮及格式轉換，壓縮後會逐 byte 回解比對。

`prepare_stage.py` 是本機重測用輔助程式：只把 13 個 production 修正檔案依指定 commit 複製到既有診斷目錄，保留同一份 overlay。它記錄各檔 SHA-256。它的診斷路徑固定為 `/private/tmp/rfgui-build-graph-diagnosis`，不是一般安裝或部署程式。

原生視窗檢查額外編譯 `cargo build --offline -p examples --bin 01_window --features renderer-perf`，環境為 `RFGUI_PAINT_RENDERER=retained-auto`（或 `legacy`）、`RFGUI_WINDOW_PERF_FRAMES=100000`、`RFGUI_WINDOW_PERF_DEMAND=1`、`RFGUI_PERF_UPDATES=idle`。透過 UI automation 拖曳報告中的座標，保留 `window-input`／`window-perf` 輸出。不要把嚴格入口的 fallback assertion 關掉來宣告通過。

本次原始 log、壓縮像素留在本機的報告目錄；精簡報告、統計、overlay 與關鍵驗證 log 另行提交。完整 binary 雜湊見 `native-binary-sha256.json`：`rfgui-fixed-window-binary` 是 b138789 視窗版本，最後一次 `target/debug/01_window` 則是用來反查既有 fallback 的 0c422b7 版本；test binary `01_window-72fd5e24881af1e1` 是最終 b138789 重播版本。請勿只依檔名判斷來源。
