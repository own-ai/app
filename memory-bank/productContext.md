# Product Context - ownAI

## Why This Project Exists

Current AI assistants (ChatGPT, Claude, etc.) treat every conversation as isolated. Users constantly re-explain context, preferences, and history. ownAI solves this by creating a **continuous, evolving relationship** between user and AI agent - one that remembers, learns, and grows over time.

Additionally, existing AI tools are cloud-dependent and closed-source. ownAI is **privacy-first and local**, giving users full control over their data and AI configuration.

## Problems It Solves

1. **Context Loss**: No more re-explaining who you are, what you're working on, or your preferences
2. **Limited Capabilities**: When the AI can't do something, it creates the tool itself (self-programming)
3. **Privacy Concerns**: All data stays local, only LLM API calls go external (user-configured)
4. **Platform Lock-in**: Open-source, provider-agnostic (Anthropic, OpenAI, etc.)
5. **No Visual Output**: Unlike pure chat, ownAI can generate interactive HTML apps (Canvas) for visual use cases

## Target Use Cases (Examples)

### Professional
- **Funding Advisor**: Extract changes from newsletter emails, display in structured dashboard
- **AI Trainer**: Create simple interfaces for workshop participants to try image generation
- **Swimming Coach**: Training plan management app with automated analysis and personalized recommendations
- **Construction Manager**: Auto-generate building condition reports from photos and data

### Personal
- **Daily Task Management**: Organize and prioritize tasks, manage calendars and reminders
- **Games**: "Let's play chess" - agent generates a chess board as HTML app

### Everyday
- **Reminders**: "Remind me at 3 PM about my tax advisor appointment"
- **Timers**: "Set a timer for 15 minutes"
- **Price Monitoring**: "Notify me when flight tickets drop below 300 EUR"

## How It Should Work

### User Experience Flow
1. User launches ownAI - sees a clean, book-like conversation interface
2. Conversation continues where it left off (no sessions/chats)
3. User asks for something the agent can't do
4. Agent recognizes the gap and creates a tool (Rhai script) or program (HTML app)
5. New capability is immediately available
6. Over time, the agent becomes deeply personalized to the user

### Two-Level Self-Programming
- **Tools (Level 1)**: Backend Rhai scripts for data processing, API calls, timers - no UI needed
- **Programs (Level 2)**: Full HTML/CSS/JS apps in a Canvas iframe for visual use cases
- User can iterate on programs via chat: "Make the chess board bigger"

### Memory System
- **Working Memory**: Last ~30-50 messages in context
- **Summaries**: Older messages condensed into structured summaries
- **Long-term Memory**: Important facts stored as vector embeddings for semantic retrieval
- Seamless - user never thinks about memory management

## User Experience Goals

- **Timeless Aesthetic**: Typography-driven design that won't look dated
- **Calm Interface**: Minimal colors, no decorative elements, whitespace as design element
- **Continuous Flow**: No chat bubbles - a flowing text like a book
- **Typography as Voice**: Serif for agent, sans-serif for user, monospace for system
- **Progressive Disclosure**: Complexity only appears when needed
- **Dignity**: Treats both user and agent with respect, no infantilizing elements
