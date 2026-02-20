# Project Brief - ownAI

## Project Vision

ownAI is an open-source desktop application (later mobile) that evolves into a highly personalized AI agent. Unlike conventional AI assistants with discrete conversations, ownAI builds a continuous, evolving relationship between user and agent. Its key capability: ownAI can extend itself by writing code to fulfill new requirements.

## Core Philosophy

- **Continuity over Segmentation**: One ongoing conversation, no constant restarting
- **Self-Evolution**: The agent extends its capabilities through self-written code
- **Simplicity**: Minimalist UI/UX design focused on essentials
- **Privacy First**: Local execution, user controls their data
- **Cross-Platform**: One codebase for desktop and mobile

## Core Requirements

### 1. Continuous Conversation
- No "sessions" or "chats" - one flowing conversation per AI instance
- Hierarchical memory system (working memory, summaries, long-term vector store)
- Unlimited conversation history through intelligent summarization

### 2. Self-Programming (Two Levels)
- **Level 1 - Tools (Rhai Scripts)**: Backend functions the LLM calls directly. Sandboxed scripts for data processing, API calls, etc. No UI.
- **Level 2 - Programs (Canvas)**: LLM-generated HTML/CSS/JS single-page apps rendered in an iframe. For visual use cases (dashboards, games, forms). Communicate with backend via Bridge API (postMessage).

### 3. Multi-AI-Instance System
- Multiple independent AI agents, each with separate database, tools, and memories
- Quick switching without reload
- Discreet but accessible UI element

### 4. Internationalization
- German and English from day one
- System language auto-detection
- All UI text via react-i18next

### 5. Deep Agent Features
- Planning Tool (TODO lists for task decomposition)
- Sub-Agent System (specialized agents for code-writing, research, memory management)
- Filesystem Access (context offloading, persistent state)
- Scheduled Tasks (cron-like system for recurring agent actions)
- Detailed System Prompt (context-rich instructions)

## Target Users
- Knowledge workers who need personalized AI assistance
- Professionals wanting domain-specific tools (trainers, consultants, construction managers)
- Users who want privacy-first local AI with self-extending capabilities

## Success Metrics
- Seamless continuous conversation across sessions
- Agent successfully creates and uses new tools
- Visual programs (Canvas) work for complex use cases
- Cross-platform compatibility (Desktop now, Mobile later)
