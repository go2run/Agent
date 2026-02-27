# WASM Agent

Rust + WebAssembly AI Agent，基於 Hexagonal Architecture (六角架構)，完全運行於瀏覽器中。

## 架構

```
┌─────────────────────────────────────────────┐
│                 agent-app                    │  入口 (WASM entry point)
├─────────────┬───────────────────────────────┤
│  agent-ui   │       agent-platform          │  UI 面板 / 平台適配器
│  (egui)     │  (LLM, Storage, Shell, VFS)   │
├─────────────┴───────────────────────────────┤
│              agent-core                      │  Runtime, Port Traits, EventBus
├─────────────────────────────────────────────┤
│              agent-types                     │  共享類型 (零平台依賴)
└─────────────────────────────────────────────┘
```

### Crates

| Crate | 說明 |
|-------|------|
| `agent-types` | Message, Event, Tool, Config, Session, Error 等共享類型 |
| `agent-core` | Port traits (LLM/Shell/Storage/VFS), EventBus, ToolRegistry, AgentRuntime |
| `agent-platform` | 瀏覽器適配器: OpenAI-compatible LLM, MemoryStorage, IndexedDB, VFS, Wasmer Shell |
| `agent-ui` | egui 面板: Chat, Terminal, Settings |
| `agent-app` | WASM 入口，DI 組裝所有模組 |

## 快速開始

### 依賴

- [Rust](https://rustup.rs/) (stable)
- [wasm-bindgen-cli](https://rustwasm.github.io/wasm-bindgen/) (`cargo install wasm-bindgen-cli`)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/) (測試用, `cargo install wasm-pack`)
- wasm32-unknown-unknown target (`rustup target add wasm32-unknown-unknown`)
- Python3 或 Node.js (HTTP 伺服器)

### 一鍵啟動

```bash
./start_all.sh
```

自動執行：依賴檢查 → 測試 → 建構 → 啟動 HTTP 伺服器 → `http://127.0.0.1:8080`

可選參數：
```bash
PORT=3000 ./start_all.sh          # 自訂端口
SKIP_TESTS=1 ./start_all.sh      # 跳過測試直接啟動
```

### 手動建構

```bash
# 安裝依賴
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli

# 開發建構
./build.sh

# 生產建構 (含 wasm-opt 優化)
./build.sh release

# 啟動伺服器
cd dist && python3 -m http.server 8080
```

## 測試

```bash
./test.sh
```

執行所有測試套件：

| 類別 | 框架 | 測試數 |
|------|------|--------|
| Native (cargo test) | agent-types, agent-core, agent-platform | 89 |
| WASM/Node (wasm-pack) | agent-types, agent-core, agent-platform | 86 |
| **合計** | | **175** |

涵蓋範圍：
- 訊息/事件序列化往返
- Agent 迴圈 (think → act → observe) 含 Mock LLM/Shell/VFS
- ToolRegistry 與參數解析
- MemoryStorage CRUD
- 虛擬檔案系統操作 (讀寫刪除/目錄/Unicode)
- 錯誤處理

## LLM Provider 設定

在 UI 的 Settings 面板設定：

| Provider | Base URL | 模型範例 |
|----------|----------|----------|
| DeepSeek | `https://api.deepseek.com` | `deepseek-chat` |
| OpenAI | `https://api.openai.com` | `gpt-4o` |
| Anthropic | `https://api.anthropic.com` | `claude-sonnet-4-20250514` |
| Google | `https://generativelanguage.googleapis.com` | `gemini-pro` |
| Custom | 任意 OpenAI-compatible URL | - |

## 技術要點

- **JS Bridge 架構**: egui (`wasm32-unknown-unknown`) 為主 UI，Wasmer-JS 透過 Web Worker 執行 bash
- **Hexagonal Architecture**: Core 定義 trait 介面，Platform 實作適配器
- **非同步模型**: `#[async_trait(?Send)]` — 瀏覽器單執行緒，透過 `spawn_local` 執行
- **儲存層**: 自動偵測 IndexedDB，降級至 Memory
- **Agent Loop**: Think → Act → Observe 循環，最多 20 次迭代

## 目錄結構

```
Agent/
├── Cargo.toml          # Workspace 根設定
├── build.sh            # WASM 建構腳本
├── start_all.sh        # 一鍵啟動 (建構 + HTTP 伺服器)
├── test.sh             # 一鍵測試
├── web/
│   ├── index.html      # HTML 入口 + Loading 畫面
│   └── worker.js       # Web Worker (Wasmer-JS bash)
└── crates/
    ├── agent-types/    # 共享類型
    ├── agent-core/     # Runtime + Port Traits
    ├── agent-platform/ # 瀏覽器適配器
    ├── agent-ui/       # egui UI 面板
    └── agent-app/      # WASM 入口
```

## License

Apache-2.0
