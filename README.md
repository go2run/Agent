# WASM Agent

Rust + WebAssembly AI Agent，基於 **Hexagonal Architecture (六角架構)**，完全運行於瀏覽器中。

支援多家 LLM Provider (DeepSeek / OpenAI / Anthropic / Google)，透過 egui 提供即時 UI，透過 WASIX Web Worker 執行 bash 指令。

---

## 架構

### 系統架構圖

```
                          ┌──────────────────┐
                          │     Browser       │
                          └──────┬───────────┘
                                 │
                    ┌────────────▼────────────┐
                    │       agent-app          │  WASM Entry Point
                    │  (DI wiring / eframe)    │  #[wasm_bindgen(start)]
                    ├──────────┬──────────────┤
                    │          │              │
          ┌─────────▼──┐  ┌───▼──────────────▼──────────┐
          │  agent-ui   │  │       agent-platform         │
          │  (egui)     │  │  LLM · Storage · Shell · VFS │
          │             │  │                              │
          │ Chat Panel  │  │ OpenAiCompatProvider         │
          │ Terminal    │  │ MemoryStorage / IndexedDB    │
          │ Settings    │  │ WasmerShellAdapter (Worker)  │
          └──────┬──────┘  │ StorageVfs                   │
                 │         └──────────┬───────────────────┘
                 │                    │
          ┌──────▼────────────────────▼──────┐
          │           agent-core              │  Pure Rust (no platform deps)
          │  Port Traits · EventBus           │
          │  ToolRegistry · AgentRuntime      │
          ├──────────────────────────────────┤
          │           agent-types             │  Zero-dep shared types
          │  Message · Event · Tool           │
          │  Config · Session · Error         │
          └──────────────────────────────────┘
```

### 六角架構 (Hexagonal Architecture)

```
        Driving Side                              Driven Side
       (UI → Core)                               (Core → Platform)

  ┌─────────────┐          ┌─────────────┐          ┌──────────────┐
  │  egui UI    │───emit──▶│  EventBus   │◀──drain──│  agent-app   │
  └─────────────┘          └─────────────┘          └──────────────┘

  ┌─────────────┐   impl   ┌─────────────┐   impl   ┌──────────────┐
  │ AgentRuntime│──────────▶│  LlmPort    │◀─────────│ OpenAiCompat │
  │             │          │  ShellPort   │          │ WasmerShell  │
  │  think →    │          │  StoragePort │          │ MemoryStorage│
  │  act →      │          │  VfsPort     │          │ IndexedDB    │
  │  observe    │          │  (traits)    │          │ StorageVfs   │
  └─────────────┘          └─────────────┘          └──────────────┘
                           agent-core                agent-platform
```

**核心原則**：`agent-core` 只定義 trait 介面，完全不依賴瀏覽器 API。`agent-platform` 實作具體適配器。這使得核心邏輯可在任何 Rust 環境中測試。

### Crates 一覽

| Crate | 依賴 | 職責 |
|-------|------|------|
| `agent-types` | serde, thiserror | Message, Event, Tool, Config, Session, Error — 零平台依賴的共享類型 |
| `agent-core` | agent-types, async-trait, futures | Port traits (LlmPort / ShellPort / StoragePort / VfsPort), EventBus, ToolRegistry, AgentRuntime |
| `agent-platform` | agent-core, web-sys, gloo-net, wasm-bindgen | 瀏覽器適配器：OpenAI-compatible LLM client, MemoryStorage, IndexedDB, VFS, Wasmer Shell Worker |
| `agent-ui` | agent-core, egui | UI 面板：Chat (對話), Terminal (互動終端), Settings (設定) |
| `agent-app` | all above, eframe | WASM 入口，DI 組裝，字體載入，工作空間初始化，設定持久化 |

### 資料流

```
User Input → Chat Panel
           → AgentRuntime.run_turn()
           → LlmPort.chat_completion()    ← OpenAI-compatible API
           → Tool calls? → execute_tool()
              ├── "bash"       → ShellPort.execute()  ← Web Worker
              ├── "read_file"  → VfsPort.read_file()  ← Storage-backed VFS
              ├── "write_file" → VfsPort.write_file()
              └── "list_dir"   → VfsPort.list_dir()
           → Tool results → append to messages → loop back to LLM
           → Final text response → EventBus → UI update
```

### 非同步模型

- 所有 Port traits 使用 `#[async_trait(?Send)]`（瀏覽器 WASM 是單執行緒）
- Agent loop 透過 `wasm_bindgen_futures::spawn_local()` 啟動，不阻塞 UI
- EventBus 使用 `Rc<RefCell<VecDeque<AgentEvent>>>`，clone 即共享
- Shell 指令在 Web Worker 中執行，透過 `postMessage` 傳送結果

---

## 快速開始

### 依賴

| 工具 | 安裝指令 |
|------|---------|
| Rust (stable) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| wasm32 target | `rustup target add wasm32-unknown-unknown` |
| wasm-bindgen-cli | `cargo install wasm-bindgen-cli` |
| wasm-pack (測試用) | `cargo install wasm-pack` |
| HTTP server | Python3 (內建) 或 Node.js |

### 一鍵啟動

```bash
./start_all.sh
```

自動執行：依賴檢查 → 測試 → 建構 → 啟動 HTTP 伺服器 → `http://127.0.0.1:8080`

```bash
PORT=3000 ./start_all.sh          # 自訂端口
SKIP_TESTS=1 ./start_all.sh      # 跳過測試直接啟動
```

### 手動建構

```bash
./build.sh              # 開發建構
./build.sh release      # 生產建構 (含 wasm-opt)
cd dist && python3 -m http.server 8080
```

---

## 功能說明

### Chat (對話面板)
- 發送訊息給 AI Agent
- Agent 使用 Think → Act → Observe 迴圈自動執行工具
- 即時顯示 streaming 回應
- 支援繁體中文 (NotoSansTC 字體)

### Terminal (互動終端)
- 直接輸入 shell 指令（不經過 Agent LLM）
- 顯示 stdout / stderr 輸出
- 指令歷史記錄 (↑/↓ 鍵切換)
- Clear 按鈕清除輸出

### Settings (設定面板)
- **LLM Provider**: DeepSeek / OpenAI / Anthropic / Google / Custom
- **Model / API Key / Base URL / Temperature / Max Tokens**
- **Storage Backend**: Auto / Memory / IndexedDB / OPFS
- 設定變更自動儲存到 Storage，重啟瀏覽器後恢復

### Workspace (工作空間)
- 每次啟動自動建立 `/workspace` 目錄結構
- Agent 的所有檔案操作都在 `/workspace/` 下進行
- 目錄：`/workspace/home/`, `/workspace/tmp/`, `/workspace/src/`

---

## 測試

```bash
./test.sh
```

| 類別 | Crates | 測試數 |
|------|--------|--------|
| Native (cargo test) | agent-types, agent-core, agent-platform | 89 |
| WASM/Node (wasm-pack) | agent-types, agent-core, agent-platform | 86 |
| **合計** | | **175** |

涵蓋：序列化往返、EventBus、ToolRegistry、Agent Loop (Mock LLM/Shell/VFS)、MemoryStorage CRUD、VFS 操作、Unicode、錯誤處理

---

## LLM Provider 設定

| Provider | Default Base URL | 模型範例 |
|----------|----------|----------|
| DeepSeek | `https://api.deepseek.com` | `deepseek-chat` |
| OpenAI | `https://api.openai.com` | `gpt-4o` |
| Anthropic | `https://api.anthropic.com` | `claude-sonnet-4-20250514` |
| Google | `https://generativelanguage.googleapis.com` | `gemini-pro` |
| Custom | 自訂 | 任意 OpenAI-compatible endpoint |

---

## 已知限制與未來規劃

### 目前限制

| 項目 | 現狀 | 說明 |
|------|------|------|
| Shell 執行 | Web Worker fallback shell | Wasmer-JS WASIX 完整 bash 尚未整合，目前使用 JS fallback (echo, date, ls 等基本指令) |
| Streaming LLM | 僅 non-streaming | `stream_chat()` 已定義但 UI 尚未接入 SSE streaming |
| IndexedDB 自動切換 | 手動選擇 | Settings 可選 IndexedDB，但 Auto-detect 尚未在啟動時非同步初始化 IndexedDB |
| OPFS Storage | 未實作 | Port trait 已定義，adapter 待開發 |
| 多 Session | 未實作 | `Session` 類型已定義，但 UI 尚未支援多會話切換 |
| Anthropic/Google API 格式 | 未適配 | 目前所有 provider 走 OpenAI-compatible 格式，Anthropic Messages API 和 Google Gemini API 需要獨立 adapter |
| 離線支援 | 無 | Service Worker 快取待實作 |
| 字體大小 | 5.5MB runtime 載入 | NotoSansTC 字體從伺服器 fetch，增加首次載入時間 |

### 未來增強 (Roadmap)

1. **Wasmer-JS 完整 bash** — 整合 `@aspect/wasmer-js` SDK，在 Web Worker 中運行真實 WASIX bash
2. **SSE Streaming** — 接入 `stream_chat()` 讓 LLM 回應逐字顯示
3. **Anthropic Messages API adapter** — 支援 Claude 原生 API 格式
4. **Google Gemini adapter** — 支援 Gemini 原生 API 格式
5. **IndexedDB 自動初始化** — 啟動時非同步偵測並切換到 IndexedDB
6. **OPFS adapter** — 高效能本地檔案存取
7. **多 Session 管理** — 左側面板列出歷史會話，支援切換
8. **File Explorer 面板** — 視覺化瀏覽 VFS 檔案系統
9. **Service Worker 離線快取** — PWA 離線執行
10. **字體子集化** — 減少 CJK 字體大小至 ~2MB

---

## 目錄結構

```
Agent/
├── Cargo.toml              # Workspace 根設定
├── build.sh                # WASM 建構腳本
├── start_all.sh            # 一鍵啟動 (測試 + 建構 + HTTP 伺服器)
├── test.sh                 # 一鍵測試
├── web/
│   ├── index.html          # HTML 入口 + Loading 畫面
│   ├── worker.js           # Web Worker (Shell fallback)
│   └── fonts/
│       └── NotoSansTC-Regular.otf  # 繁體中文字體
└── crates/
    ├── agent-types/        # 共享類型 (零平台依賴)
    │   └── src/
    │       ├── message.rs  # Message, Role, ToolCallRequest
    │       ├── event.rs    # AgentEvent, WorkerCommand/Event
    │       ├── tool.rs     # ToolDefinition, ExecResult
    │       ├── config.rs   # AgentConfig, LlmProvider
    │       ├── session.rs  # Session, SessionSummary
    │       └── error.rs    # AgentError enum
    ├── agent-core/         # Runtime + Port Traits
    │   └── src/
    │       ├── ports.rs    # LlmPort, ShellPort, StoragePort, VfsPort
    │       ├── event_bus.rs
    │       ├── tools.rs    # ToolRegistry (bash, read/write/list)
    │       └── runtime.rs  # AgentRuntime (think→act→observe)
    ├── agent-platform/     # 瀏覽器適配器
    │   └── src/
    │       ├── llm/        # OpenAiCompatProvider
    │       ├── storage/    # MemoryStorage, IndexedDbStorage
    │       ├── shell.rs    # WasmerShellAdapter (Web Worker)
    │       └── vfs.rs      # StorageVfs (POSIX paths → storage keys)
    ├── agent-ui/           # egui UI 面板
    │   └── src/
    │       ├── state.rs    # UiState, ChatEntry, TerminalLine
    │       ├── theme.rs    # Dark theme 常數
    │       └── panels/     # chat.rs, terminal.rs, settings.rs
    └── agent-app/          # WASM 入口
        └── src/
            ├── lib.rs      # #[wasm_bindgen(start)]
            └── app.rs      # AgentApp, font loading, workspace init
```

## License

Apache-2.0
