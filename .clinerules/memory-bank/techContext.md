# Tech Context - ownAI

## Technology Stack

### Frontend
- **Framework**: React 19 + TypeScript
- **Build Tool**: Vite
- **Styling**: Tailwind CSS v4 (new `@theme` / `@utility` syntax)
- **State Management**: Zustand
- **i18n**: react-i18next + i18next with language detector
- **Markdown**: react-markdown + remark-gfm + react-syntax-highlighter
- **Icons**: Lucide React
- **Fonts**: Noto Serif Variable, Noto Sans Variable, Noto Sans Mono (via @fontsource)
- **Utilities**: clsx + tailwind-merge (via `cn()` helper)

### Backend (Rust)
- **Desktop Framework**: Tauri 2.0
- **LLM Framework**: rig-core 0.30
- **Database**: SQLite via sqlx 0.8 (async, compile-time checked)
- **Embeddings**: fastembed 5.8 (local, Qwen3-Embedding-0.6B model)
- **Tensor Ops**: candle-core 0.9
- **ONNX Runtime**: ort 2.0-rc
- **Scripting**: Rhai 1.24
- **API Key Storage**: keyring 3.6 (OS keychain)
- **HTTP Client**: reqwest 0.13
- **JSON Schema**: schemars 1.2
- **Serialization**: serde + serde_json
- **Async Runtime**: tokio (full features)
- **UUID**: uuid 1.16 with v4
- **Time**: chrono
- **Logging**: tracing + tracing-subscriber
- **Error Handling**: anyhow
- **Streaming**: futures crate

### Development Tools
- **Package Manager**: pnpm
- **Rust Edition**: 2021
- **License**: MPL-2.0

## Development Setup

### Prerequisites
- Rust 1.70+
- Node.js 18+
- pnpm
- macOS: Xcode Command Line Tools
- Linux: webkit2gtk, libgtk-3-dev, libayatana-appindicator3-dev
- Windows: WebView2 runtime

### Commands
```bash
# Install frontend dependencies
pnpm install

# Development (frontend + Tauri)
pnpm tauri dev

# Frontend only
pnpm dev

# Build production binary
pnpm tauri build

# Rust tests (fast, no external dependencies)
cargo test

# Slow/external tests only (marked with #[ignore])
cargo test -- --ignored

# ALL tests (fast + slow)
cargo test -- --include-ignored

# Specific ignored test categories:
cargo test long_term -- --ignored        # fastembed/embedding tests (~1GB model)
cargo test keychain -- --ignored         # OS keychain tests
cargo test --test memory_integration -- --ignored  # LLM integration tests

# Rust linting
cargo clippy

# Rust formatting
cargo fmt

# TypeScript type-check
pnpm tsc --noEmit

# Local CI (all checks, fast tests only)
./scripts/ci.sh

# Local CI including slow/external tests
RUN_ALL_TESTS=1 ./scripts/ci.sh
```

### Database
- Auto-initialized on first run at `~/.ownai/instances/{id}/ownai.db`
- Migrations embedded in code via `schema.rs` (run_migrations function)
- Schema includes: messages, summaries, memory_entries, user_profile, tools, tool_executions

## Technical Constraints

### Cross-Platform Requirements
- No Node.js sidecar (iOS forbids JIT compilers)
- All backend logic must be in Rust
- Must use system WebView (Tauri requirement)
- Local-only data storage (privacy-first)

### LLM Constraints
- Token budget management for context window
- Streaming required for good UX (token-by-token)
- API keys needed for Anthropic/OpenAI (stored in OS keychain)
- No local LLM support yet (Ollama planned for future)

### Embedding Constraints
- fastembed runs locally (no API calls)
- Uses Qwen3-Embedding-0.6B model
- ONNX Runtime dependency (ort crate)
- First-time model download may be slow

### Frontend Constraints
- Tailwind CSS v4 syntax (different from v3)
- All text must go through i18n system
- No emojis anywhere in UI, code, or commits
- Typography-driven design (no decorative elements)
- Maximum content width: 720px

## Project Structure

```
app/
├── src/                          # React frontend
│   ├── components/
│   │   ├── chat/                 # Message, MessageContent, MessageInput, MessageList
│   │   ├── instances/            # AIInstanceSelector, CreateInstanceDialog
│   │   ├── layout/               # Header, Container
│   │   ├── settings/             # Settings panel
│   │   └── ui/                   # Button, IconButton, Input
│   ├── i18n/                     # i18n configuration
│   ├── locales/                  # Translation files (en, de)
│   ├── stores/                   # Zustand stores (chat, instance)
│   ├── styles/                   # app.css (design system)
│   ├── types/                    # TypeScript types
│   ├── utils/                    # cn() helper
│   └── App.tsx                   # Root component
│
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── agent/                # OwnAIAgent (rig-core integration)
│   │   ├── ai_instances/         # Manager, models, keychain
│   │   ├── commands/             # Tauri commands (chat, instances, memory, tools)
│   │   ├── database/             # SQLite schema and migrations
│   │   ├── memory/               # Working memory, summarization, long-term, context builder
│   │   ├── tools/                # Filesystem, planning, registry, rhai_engine, rhai_bridge, code_generation
│   │   ├── utils/                # Path helpers
│   │   ├── lib.rs                # App setup, command registration
│   │   └── main.rs               # Entry point
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── .clinerules/                  # Memory bank + rules
└── public/                       # Static assets
```
