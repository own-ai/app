# ownAI

A personal AI agent that evolves with you through continuous conversation and self-programming capabilities.

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)
[![GitHub](https://img.shields.io/badge/github-own--ai%2Fapp-blue)](https://github.com/own-ai/app)

## What is ownAI?

Unlike traditional AI assistants that treat each conversation as a discrete interaction, ownAI builds a **continuous, evolving relationship** with its user. It remembers everything important, learns your preferences over time, and can even extend its own capabilities by generating code when it encounters tasks it cannot yet perform.

ownAI is designed as a **privacy-first, self-improving personal agent** that runs locally on your machine, giving you complete control over your data while providing an increasingly personalized experience.

## Key Features

- **Continuous Conversation**: One flowing dialogue that picks up where you left off, no matter how much time has passed.
- **Hierarchical Memory System**: Working memory for recent context, automatic summarization for efficiency, and long-term memory with semantic search.
- **Self-Programming Capabilities**: The agent can write and deploy new tools when it encounters tasks beyond its current abilities.
- **Privacy-First Architecture**: All data stored locally, complete user control, no cloud dependencies required.
- **Cross-Platform**: Desktop application built with Tauri 2.0, with mobile support planned.

## Tech Stack

### Frontend
- **React** with TypeScript
- **Tailwind CSS** for styling with custom design system
- **Tauri 2.0** for native performance and small binary size

### Backend
- **Rust** for performance, safety, and cross-platform compatibility
- **rig-core** for unified LLM provider interface
- **SQLite** with fastembed for local vector search and embeddings
- **sqlx** for type-safe database queries

## Getting Started

### Prerequisites

- **Rust** 1.70 or later ([rustup.rs](https://rustup.rs/))
- **Node.js** 18 or later ([nodejs.org](https://nodejs.org/))
- **pnpm** ([pnpm.io](https://pnpm.io/))

### Installation

```bash
# Clone the repository
git clone https://github.com/own-ai/app.git
cd app

# Install frontend dependencies
pnpm install

# Run in development mode
pnpm tauri dev
```

### Development Commands

```bash
# Start frontend dev server
pnpm dev

# Run Tauri app with hot reload
pnpm tauri dev

# Build for production
pnpm tauri build

# Run Rust tests
cargo test

# Lint Rust code
cargo clippy
```

## Architecture Overview

ownAI uses a layered architecture:

- **Frontend**: React-based chat interface with virtual scrolling for performance
- **Tauri Bridge**: Type-safe commands connecting frontend to Rust backend
- **Memory System**: Three-tier memory (working, mid-term summaries, long-term vector store)
- **Database**: Single SQLite file containing messages, summaries, embeddings, and metadata
- **LLM Integration**: Provider-agnostic through rig-core

For detailed technical documentation including code style guidelines, component structure, and common development tasks, see [AGENTS.md](AGENTS.md).

## Contributing

Contributions are welcome! Whether you're fixing bugs, adding features, or improving documentation, we appreciate your help.

### Guidelines

- Please follow the existing code style (see [AGENTS.md](AGENTS.md) for details).
- Please write clear commit messages.
- Please add tests for new functionality.
- Please check [Issues](https://github.com/own-ai/app/issues) for open tasks.

### Design Philosophy

ownAI follows a minimalist, typography-driven design philosophy. Key principles:

- **Simplicity**: Every element must earn its place.
- **Typography-first**: Different voices through typefaces (serif for agent, sans-serif for user).
- **Timeless aesthetics**: No trends, just fundamentals.
- **Functionality over decoration**: Color and animation only when they serve a purpose.

## Roadmap

- Memory system with automatic summarization
- LLM provider integration
- Self-programming capabilities via Rhai scripting
- Tool registry and dynamic tool loading
- Mobile applications (iOS/Android via Tauri Mobile)
- Proactive suggestions based on learned patterns

## License

This project is licensed under the Mozilla Public License 2.0 - see the [LICENSE](LICENSE) file for details.

---

**Built with a vision of cooperative AI infrastructure: community-owned agents that automate businesses while keeping control, ownership, and value with humans.**
