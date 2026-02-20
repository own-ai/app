# ownAI Design Philosophy

> A continuous relationship deserves a timeless interface

**Version:** 1.0
**Date:** February 2026
**Status:** Living Document

---

## Table of Contents

1. [Core Philosophy](#core-philosophy)
2. [Design Principles](#design-principles)
3. [Visual Identity](#visual-identity)
4. [Typography System](#typography-system)
5. [Color Palette](#color-palette)
6. [Layout & Composition](#layout--composition)
7. [Interaction Design](#interaction-design)
8. [Component Library](#component-library)
9. [Animations & Transitions](#animations--transitions)
10. [Responsive Behavior](#responsive-behavior)
11. [Accessibility](#accessibility)
12. [Implementation Guidelines](#implementation-guidelines)

---

## Core Philosophy

### The Central Idea

ownAI is not a chat tool. It is a **personal agent** that builds a **continuous relationship** with the user. The interface must reflect this continuity and intimacy.

### Guiding Principles

**1. Continuity over Segmentation**
- No chat bubbles or visual breaks
- A continuous flow of text like in a book
- Temporal continuity across days and months

**2. Typographic Intimacy**
- Typography as the primary design element
- Different "voices" through typography
- Readability above all

**3. Progressive Disclosure**
- Interface remains minimal
- Complexity appears only when needed
- Technology stays in the background

**4. Timeless Aesthetic**
- No trend-dependent design elements
- Focus on fundamental design principles
- Longevity over novelty

**5. Dignity and Respect**
- The interface treats both user and agent with dignity
- No infantilizing elements
- Serious yet approachable design

---

## Design Principles

### 1. Calm through Reduction

**Philosophy:** Every element must earn its place.

**In Practice:**
- Maximum 3 colors visible at once
- No decorative icons without function
- White space as an active design element

**Anti-Pattern:**
```jsx
// -- Wrong: Too many visual elements
<MessageBubble
  avatar={<Avatar />}
  badge={<Badge />}
  timestamp={<Timestamp />}
  reactions={<Reactions />}
  menu={<ContextMenu />}
/>
```

**Best Practice:**
```jsx
// -- Correct: Only the essentials
<Message
  role={role}
  content={content}
  timestamp={timestamp}
/>
```

### 2. Hierarchy through Contrast

**Philosophy:** Differences should be functional, not decorative.

**Means:**
- Typeface (Serif vs. Sans-Serif)
- Weight (400 vs. 500)
- Color (Black vs. Dark Gray)
- Position (Left indent vs. Flush)

**Not:**
- Background colors
- Borders and boxes
- Shadows
- Badges without reason

### 3. Consistency with Intent

**Philosophy:** Repetition builds trust, but not boredom.

**Consistent:**
- Spacing (always multiples of 4px)
- Typography scaling
- Transition timing
- Icon style

**Variable:**
- Content treatment (Code vs. Text vs. TODO)
- Contextual coloring (Error, Success, System)
- Animation speed depending on importance

### 4. Responsive without Compromise

**Philosophy:** Not "mobile-first" or "desktop-first", but "content-first".

**Approach:**
- Layout adapts to content, not the other way around
- Core functionality remains the same across all sizes
- No features exclusive to desktop or mobile

---

## Visual Identity

### Brand Essence

**ownAI is:**
- Intelligent, but not arrogant
- Personal, but not intrusive
- Technical, but not cold
- Evolving, but not unstable

### Visual Expression

**Form Language:**
- Organically flowing (text, line breaks)
- Geometrically precise (UI elements)
- Softly rounded (8px border-radius as standard)

**Visual Metaphors:**
- **Book/Journal:** Continuous flow of text
- **Workshop:** Tools visible but organized
- **Personal Space:** Warm, inviting, familiar

---

## Typography System

### Font Selection

Typography carries the entire visual communication.

#### Agent Voice: Serif

**Primary Choice: Noto Serif**
```css
font-family: 'Noto Serif Variable', 'Noto Serif', Georgia, serif;
```

**Characteristics:**
- Part of the comprehensive Noto family by Google
- Excellent multilingual support (100+ scripts worldwide)
- Classic yet modern serifs
- Variable font for optimal performance and flexibility
- Neutral-warm, professional, dignified

**Fallbacks (in order):**
1. Noto Serif (static variant)
2. Georgia (universal)
3. System Serif

**Why Noto Serif for the agent?**
- Universal accessibility -- works in every language
- Dignified yet not overly personal appearance
- Part of a consistent typography system
- Open source and future-proof
- Creates warmth through serifs while remaining professional
- Traditional for longer texts (like books)

#### User Voice: Sans-Serif

**Primary Choice: Noto Sans**
```css
font-family: 'Noto Sans Variable', 'Noto Sans', -apple-system, sans-serif;
```

**Characteristics:**
- Humanistic sans-serif from the Noto family
- Excellent readability at all sizes
- Consistent with Noto Serif
- Variable font for UI optimization
- Comprehensive language support (100+ scripts)

**Fallbacks (in order):**
1. Noto Sans (static variant)
2. System Sans (-apple-system, etc.)

**Why Noto Sans for the user?**
- Harmonizes perfectly with the agent font (Noto Serif)
- Clarity and functionality
- Worldwide language support
- Family consistency creates visual coherence
- Quick readability

### Typography Scale

**Base Size:** 16px (1rem)

```css
:root {
  /* Font Sizes */
  --text-xs: 0.75rem;   /* 12px - Metadata */
  --text-sm: 0.875rem;  /* 14px - Helper text */
  --text-base: 1rem;    /* 16px - Body text */
  --text-lg: 1.125rem;  /* 18px - Emphasis */
  --text-xl: 1.25rem;   /* 20px - Headings */

  /* Line Heights */
  --leading-tight: 1.25;
  --leading-normal: 1.5;
  --leading-relaxed: 1.7;
  --leading-loose: 2;

  /* Font Weights */
  --font-normal: 400;
  --font-medium: 500;
  --font-semibold: 600;
}
```

### Typography Application

**Agent Messages:**
```css
.agent-message {
  font-family: 'Noto Serif Variable', 'Noto Serif', serif;
  font-size: var(--text-base);
  line-height: var(--leading-relaxed); /* 1.7 */
  font-weight: var(--font-normal);
  color: var(--color-foreground);
}
```

**User Messages:**
```css
.user-message {
  font-family: 'Noto Sans Variable', 'Noto Sans', sans-serif;
  font-size: var(--text-base);
  line-height: var(--leading-normal); /* 1.5 */
  font-weight: var(--font-medium);
  color: var(--color-user-text);
}
```

**System Messages:**
```css
.system-message {
  font-family: 'Noto Sans Mono', 'SF Mono', 'Consolas', monospace;
  font-size: var(--text-sm);
  line-height: var(--leading-normal);
  font-weight: var(--font-normal);
  color: var(--color-system-text);
}
```

**Code (Inline & Block):**
```css
code {
  font-family: 'Noto Sans Mono', 'SF Mono', 'Consolas', monospace;
  font-size: 0.9em;
  background: var(--color-code-bg);
  padding: 0.125em 0.25em;
  border-radius: 3px;
}

pre {
  font-family: 'Noto Sans Mono', monospace;
  font-size: var(--text-sm);
  line-height: var(--leading-normal);
  background: var(--color-code-block-bg);
  padding: 1rem;
  border-radius: 8px;
  overflow-x: auto;
}
```

### Typography Hierarchy

**Level 1: Content (Agent/User)**
- Largest area, highest visibility
- Serif (Agent) or Sans-Serif (User)
- 16px, optimal readability

**Level 2: Metadata**
- Timestamps, tool names, status
- Sans-Serif, 12-14px
- Reduced opacity (60-70%)

**Level 3: System**
- TODO lists, logs, technical info
- Monospace, 14px
- Context-dependent coloring

---
## Color Palette

### Philosophy

Color is used **sparingly and intentionally**. The palette is minimal to create calm and direct attention.

### Core Palette

```css
:root {
  /* Neutrals - The Foundation */
  --color-background: #fafafa;      /* Near-white, warm */
  --color-surface: #ffffff;         /* Pure white for elevated elements */
  --color-foreground: #1a1a1a;      /* Near-black */
  --color-user-text: #2c2c2c;       /* Slightly lighter for user */
  --color-muted: #737373;           /* Metadata, disabled */
  --color-border: #e5e5e5;          /* Subtle boundaries */
  --color-border-strong: #d4d4d4;   /* Visible boundaries */

  /* Semantic Colors - Use sparingly */
  --color-accent: #6b4c9a;          /* Purple - Active elements */
  --color-accent-hover: #5a3d85;    /* Purple - Hover state */

  --color-success: #059669;         /* Green - Success, Completed */
  --color-success-bg: #d1fae5;      /* Green - Background */

  --color-warning: #d97706;         /* Orange - Warning, In Progress */
  --color-warning-bg: #fef3c7;      /* Orange - Background */

  --color-error: #dc2626;           /* Red - Error */
  --color-error-bg: #fee2e2;        /* Red - Background */

  --color-system: #8b7355;          /* Terracotta - System Messages */
  --color-system-bg: #f5f5f4;       /* Warm Grey - System BG */

  /* Code-specific */
  --color-code-bg: #f5f5f5;
  --color-code-block-bg: #fafaf9;
  --color-code-border: #e7e5e4;
}
```

### Dark Mode Palette

```css
:root[data-theme="dark"] {
  /* Neutrals - Inverted but not 1:1 */
  --color-background: #0a0a0a;      /* Near-black */
  --color-surface: #171717;         /* Elevated surfaces */
  --color-foreground: #fafafa;      /* Near-white */
  --color-user-text: #e5e5e5;       /* Slightly darker for user */
  --color-muted: #737373;           /* Stays the same */
  --color-border: #262626;          /* Dark boundaries */
  --color-border-strong: #404040;   /* Visible boundaries */

  /* Semantic Colors - Adjusted brightness */
  --color-accent: #9b7cc4;          /* Lighter purple */
  --color-accent-hover: #b294d6;

  --color-success: #10b981;
  --color-success-bg: #064e3b;

  --color-warning: #f59e0b;
  --color-warning-bg: #78350f;

  --color-error: #ef4444;
  --color-error-bg: #7f1d1d;

  --color-system: #a8956b;
  --color-system-bg: #1c1917;

  /* Code */
  --color-code-bg: #1c1c1c;
  --color-code-block-bg: #141414;
  --color-code-border: #2d2d2d;
}
```

### Color Application Rules

**1. Neutral First**
- 90% of the interface uses only neutrals
- Black, white, and gray tones form the base

**2. Semantic Colors Only with Meaning**
- Green = Success, Completed, Positive
- Orange = Warning, In Progress, Attention
- Red = Error, Critical, Blocked
- Purple = Active, Focus, Interactive

**3. Never Decorative Color**
- No color without semantic meaning
- No background colors for variety
- No gradients without function

**Examples:**

```jsx
// -- Correct: Semantically appropriate
<TodoItem status="completed" className="text-success" />
<ErrorMessage className="text-error bg-error-bg" />
<AgentThinking className="text-muted" />

// -- Wrong: Decorative, without meaning
<Message className="bg-linear-to-r from-purple-500 to-pink-500" />
<Avatar className="ring-4 ring-blue-400" />
```

---

## Layout & Composition

### Fundamental Principles

**1. Centered Content Area**
- Maximum 720px width for optimal readability
- Horizontally centered on large screens
- Full width with padding on small screens

**2. Vertical Rhythm**
- Consistent spacing (multiples of 4px)
- Larger spacing between messages (24px)
- Smaller spacing within messages (12px)

**3. Asymmetric Balance**
- User messages: Left indent (40px)
- Agent messages: Flush
- Creates visual differentiation without bubbles

### Layout Structure

```
+-----------------------------------------------------+
|  Header (64px)                                      |
|  +- Logo (left)                                     |
|  +- Actions (right): Search, Menu, Settings         |
+-----------------------------------------------------+
|                                                     |
|  Scrollable Content Area                            |
|  +- Max-width Container (720px, centered)           |
|     +- Message 1 (py-6 px-8)                        |
|     +- Message 2 (py-6 px-8 + pl-10 if user)        |
|     +- Message 3 (py-6 px-8)                        |
|                                                     |
+-----------------------------------------------------+
|  Input Area (auto-height, min 72px)                 |
|  +- Max-width Container (720px, centered)           |
|     +- Textarea (flex-1)                            |
|     +- Send Button (48x48)                          |
+-----------------------------------------------------+
```

### Spacing System

```css
:root {
  /* Spacing Scale (t-shirt sizes) */
  --space-1: 0.25rem;   /* 4px */
  --space-2: 0.5rem;    /* 8px */
  --space-3: 0.75rem;   /* 12px */
  --space-4: 1rem;      /* 16px */
  --space-6: 1.5rem;    /* 24px */
  --space-8: 2rem;      /* 32px */
  --space-12: 3rem;     /* 48px */
  --space-16: 4rem;     /* 64px */

  /* Semantic Spacing */
  --space-message-y: var(--space-6);     /* Vertical between messages */
  --space-message-x: var(--space-8);     /* Horizontal padding */
  --space-user-indent: var(--space-10);  /* User message indent */
  --space-content-max: 720px;            /* Max content width */
}
```

### Layout Components

**Container:**
```jsx
const Container = ({ children, className }) => (
  <div className={cn(
    "max-w-180 mx-auto px-8",
    className
  )}>
    {children}
  </div>
);
```

**Message Layout:**
```jsx
const MessageLayout = ({ role, children }) => {
  const isUser = role === 'user';

  return (
    <div className={cn(
      "py-6 px-8",
      isUser && "border-l-2 border-neutral-200 pl-10"
    )}>
      {children}
    </div>
  );
};
```

---
## Interaction Design

### Principles

**1. Directness**
- No unnecessary confirmation dialogs
- Enter sends message (Shift+Enter for new line)
- Clear, immediate reactions

**2. Predictability**
- Hover states reveal interactive elements
- Consistent actions (same everywhere)
- No surprising behavior

**3. Error Tolerance**
- Undo/Redo where appropriate
- Helpful error messages
- No data loss

### Interactive Elements

#### Buttons

**Primary Button (Send):**
```css
.btn-primary {
  background: var(--color-foreground);
  color: var(--color-background);
  padding: 0.75rem;
  border-radius: 0.5rem;
  transition: background 200ms ease;
}

.btn-primary:hover {
  background: var(--color-accent);
}

.btn-primary:disabled {
  background: var(--color-border-strong);
  cursor: not-allowed;
}
```

**Icon Button (Header Actions):**
```css
.btn-icon {
  padding: 0.5rem;
  border-radius: 0.5rem;
  transition: background 200ms ease;
  color: var(--color-muted);
}

.btn-icon:hover {
  background: var(--color-surface);
  color: var(--color-foreground);
}
```

#### Input Field

**Textarea:**
```jsx
<textarea
  placeholder="What are you thinking?"
  className="
    w-full resize-none
    font-sans text-base
    px-4 py-3
    border border-neutral-200
    rounded-lg
    focus:outline-none focus:border-neutral-400
    transition-colors
    min-h-14 max-h-50
  "
  onKeyDown={(e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }}
/>
```

**Auto-growing Logic:**
```javascript
const handleInput = (e) => {
  e.target.style.height = 'auto';
  e.target.style.height = Math.min(e.target.scrollHeight, 200) + 'px';
};
```

#### Hover States

**Timestamps:**
```css
.message-timestamp {
  opacity: 0;
  transition: opacity 200ms ease;
}

.message:hover .message-timestamp {
  opacity: 1;
}
```

**Menu Items:**
```css
.menu-item {
  padding: 0.5rem 1rem;
  transition: background 150ms ease;
}

.menu-item:hover {
  background: var(--color-surface);
}
```

### Feedback Mechanisms

**Loading States:**

```jsx
// Typing Indicator
const TypingIndicator = () => (
  <div className="py-6 px-8">
    <div className="font-serif text-neutral-400 animate-pulse">
      ...
    </div>
  </div>
);

// Tool Execution
const ToolIndicator = ({ tool }) => (
  <div className="mt-3 flex items-center gap-2 text-xs font-mono text-neutral-500">
    <span className="w-2 h-2 bg-amber-500 rounded-full animate-pulse" />
    Using: {tool}
  </div>
);
```

**Success/Error States:**

```jsx
// Success - Subtle green accent
<div className="text-success flex items-center gap-2">
  <CheckIcon className="w-4 h-4" />
  <span>Tool successfully created</span>
</div>

// Error - Red with helpful message
<div className="p-4 bg-error-bg border-l-2 border-error">
  <div className="font-medium text-error">Execution failed</div>
  <div className="text-sm text-neutral-600 mt-1">
    Rate limit reached. Try again in 30 seconds.
  </div>
</div>
```

---

## Component Library

### Core Components

#### 1. Message Component

```jsx
/**
 * Message - Core element of the conversation
 *
 * @param {string} role - 'user' | 'agent' | 'system'
 * @param {string} content - Message content
 * @param {Date} timestamp - Time of the message
 * @param {object} metadata - Tool calls, memories, etc.
 */
const Message = ({ role, content, timestamp, metadata }) => {
  const isUser = role === 'user';
  const isSystem = role === 'system';

  return (
    <div
      className={cn(
        "py-6 px-8 group",
        isUser && "border-l-2 border-neutral-200 pl-10",
        isSystem && "bg-neutral-50"
      )}
    >
      {/* Content */}
      <div className={cn(
        "max-w-none",
        isUser && "font-sans font-medium text-user-text",
        !isUser && !isSystem && "font-serif",
        isSystem && "font-mono text-sm text-system"
      )}>
        <MessageContent content={content} role={role} />
      </div>

      {/* Metadata */}
      {metadata?.toolCalls && (
        <ToolCallIndicator tools={metadata.toolCalls} />
      )}

      {metadata?.memories && (
        <MemoryIndicator memories={metadata.memories} />
      )}

      {/* Timestamp - Appears on hover */}
      <div className="mt-2 text-xs text-neutral-400 font-sans opacity-0 group-hover:opacity-100 transition-opacity">
        {formatTimestamp(timestamp)}
      </div>
    </div>
  );
};
```

#### 2. MessageContent Component

```jsx
/**
 * MessageContent - Renders content with Markdown, code, TODOs
 */
const MessageContent = ({ content, role }) => {
  if (role === 'system' && containsTodos(content)) {
    return <TodoList content={content} />;
  }

  if (containsCode(content)) {
    return <MarkdownContent content={content} />;
  }

  return <div>{content}</div>;
};
```

#### 3. Input Component

```jsx
/**
 * MessageInput - Auto-growing textarea with send button
 */
const MessageInput = ({ onSend, placeholder = "What are you thinking?" }) => {
  const [value, setValue] = useState('');
  const textareaRef = useRef(null);

  const handleSend = () => {
    if (!value.trim()) return;
    onSend(value);
    setValue('');
    resetHeight();
  };

  const handleKeyDown = (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="flex items-end gap-3">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        className="
          flex-1 resize-none
          font-sans text-base
          px-4 py-3
          border border-neutral-200 rounded-lg
          focus:outline-none focus:border-neutral-400
          transition-colors
          min-h-14 max-h-50
        "
        rows={1}
      />
      <button
        onClick={handleSend}
        disabled={!value.trim()}
        className="
          p-3 bg-foreground text-background rounded-lg
          hover:bg-accent
          disabled:bg-border-strong disabled:cursor-not-allowed
          transition-colors
        "
      >
        <Send className="w-5 h-5" />
      </button>
    </div>
  );
};
```

#### 4. Header Component

```jsx
/**
 * Header - Top Navigation
 */
const Header = ({ onMenuOpen, onSettingsOpen, onSearchOpen }) => {
  return (
    <header className="
      flex items-center justify-between
      px-8 py-4
      border-b border-neutral-200
      bg-background
    ">
      <h1 className="text-xl font-serif tracking-tight">
        ownAI
      </h1>

      <div className="flex items-center gap-3">
        <IconButton icon={Search} onClick={onSearchOpen} label="Search" />
        <IconButton icon={Menu} onClick={onMenuOpen} label="Menu" />
        <IconButton icon={Settings} onClick={onSettingsOpen} label="Settings" />
      </div>
    </header>
  );
};
```

### Utility Components

#### TimeSeparator

```jsx
/**
 * TimeSeparator - Shows temporal breaks
 */
const TimeSeparator = ({ label }) => (
  <div className="flex items-center gap-4 py-8 px-8">
    <div className="flex-1 h-px bg-neutral-200" />
    <div className="text-xs font-sans text-neutral-400 uppercase tracking-wide">
      {label}
    </div>
    <div className="flex-1 h-px bg-neutral-200" />
  </div>
);

// Usage
<TimeSeparator label="3 days later" />
```

#### ToolCallIndicator

```jsx
/**
 * ToolCallIndicator - Shows used tools
 */
const ToolCallIndicator = ({ tools }) => (
  <div className="mt-3 flex items-center gap-2 text-xs font-mono text-neutral-500">
    <span className="w-2 h-2 bg-amber-500 rounded-full animate-pulse" />
    <span>Using: {tools.join(', ')}</span>
  </div>
);
```

#### MemoryIndicator

```jsx
/**
 * MemoryIndicator - Shows retrieved memories
 */
const MemoryIndicator = ({ memories }) => (
  <div className="mt-3 italic text-sm text-neutral-600">
    <span className="text-neutral-400">[</span>
    {memories.map((m, i) => (
      <span key={i}>
        {m}
        {i < memories.length - 1 && ', '}
      </span>
    ))}
    <span className="text-neutral-400">]</span>
  </div>
);
```

---
## Animations & Transitions

### Philosophy

Animations should be **functional, not decorative**. They help understand state changes and guide attention.

### Timing Functions

```css
:root {
  /* Easing Functions */
  --ease-standard: cubic-bezier(0.4, 0.0, 0.2, 1);
  --ease-decelerate: cubic-bezier(0.0, 0.0, 0.2, 1);
  --ease-accelerate: cubic-bezier(0.4, 0.0, 1, 1);

  /* Durations */
  --duration-instant: 100ms;
  --duration-fast: 150ms;
  --duration-normal: 200ms;
  --duration-slow: 300ms;
}
```

### Standard Transitions

```css
/* Hover States */
.interactive {
  transition: all var(--duration-fast) var(--ease-standard);
}

/* Color Changes */
.color-transition {
  transition: color var(--duration-normal) var(--ease-standard);
}

/* Opacity Fades */
.fade {
  transition: opacity var(--duration-normal) var(--ease-standard);
}

/* Height Changes (Messages, Menus) */
.height-transition {
  transition: height var(--duration-normal) var(--ease-decelerate);
}
```

### Micro-Interactions

**Button Press:**
```css
.btn {
  transform: scale(1);
  transition: transform var(--duration-instant) var(--ease-accelerate);
}

.btn:active {
  transform: scale(0.98);
}
```

**Menu Appearance:**
```css
@keyframes slideDown {
  from {
    opacity: 0;
    transform: translateY(-8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.menu {
  animation: slideDown var(--duration-normal) var(--ease-decelerate);
}
```

**Typing Indicator:**
```css
@keyframes pulse {
  0%, 100% { opacity: 0.4; }
  50% { opacity: 1; }
}

.typing-indicator {
  animation: pulse 1.5s var(--ease-standard) infinite;
}
```

**Tool Execution Pulse:**
```css
@keyframes pulse-scale {
  0%, 100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.6;
    transform: scale(1.2);
  }
}

.tool-indicator {
  animation: pulse-scale 2s var(--ease-standard) infinite;
}
```

### Text Streaming Animation

```jsx
/**
 * Streaming Text Effect - Word by word
 */
const StreamingText = ({ text, speed = 50 }) => {
  const [displayedText, setDisplayedText] = useState('');
  const words = text.split(' ');

  useEffect(() => {
    let currentIndex = 0;
    const interval = setInterval(() => {
      if (currentIndex < words.length) {
        setDisplayedText(prev =>
          prev + (prev ? ' ' : '') + words[currentIndex]
        );
        currentIndex++;
      } else {
        clearInterval(interval);
      }
    }, speed);

    return () => clearInterval(interval);
  }, [text, speed]);

  return <span>{displayedText}</span>;
};
```

### Scroll Animations

**Smooth Scroll:**
```css
.scrollable {
  scroll-behavior: smooth;
  overflow-y: auto;
}
```

**Custom Scrollbar:**
```css
.scrollable::-webkit-scrollbar {
  width: 8px;
}

.scrollable::-webkit-scrollbar-track {
  background: transparent;
}

.scrollable::-webkit-scrollbar-thumb {
  background: var(--color-border-strong);
  border-radius: 4px;
  transition: background var(--duration-normal);
}

.scrollable::-webkit-scrollbar-thumb:hover {
  background: var(--color-muted);
}
```

### Performance Considerations

**Optimizations:**
```css
/* GPU acceleration for transforms */
.animated {
  will-change: transform, opacity;
  transform: translateZ(0);
}

/* But: use will-change sparingly */
.animated:hover {
  will-change: auto;
}
```

**What to avoid:**
```css
/* -- Wrong: Never animate layout properties */
.bad-animation {
  transition: width 300ms; /* Causes reflow */
}

/* -- Correct: Use transform instead */
.good-animation {
  transition: transform 300ms;
  transform: scaleX(1);
}
```

---

## Responsive Behavior

### Breakpoints

```css
:root {
  --breakpoint-sm: 640px;   /* Mobile */
  --breakpoint-md: 768px;   /* Tablet */
  --breakpoint-lg: 1024px;  /* Desktop */
  --breakpoint-xl: 1280px;  /* Large Desktop */
}
```

### Layout Adjustments

**Desktop (>1024px):**
```css
@media (min-width: 1024px) {
  .container {
    max-width: 720px;
    margin: 0 auto;
  }

  .header {
    padding: 1rem 2rem;
  }

  .message {
    padding: 1.5rem 2rem;
  }
}
```

**Tablet (768px - 1024px):**
```css
@media (min-width: 768px) and (max-width: 1023px) {
  .container {
    max-width: 100%;
    padding: 0 2rem;
  }

  .message {
    padding: 1.25rem 1.5rem;
  }
}
```

**Mobile (<768px):**
```css
@media (max-width: 767px) {
  .container {
    padding: 0 1rem;
  }

  .header {
    padding: 0.75rem 1rem;
  }

  .message {
    padding: 1rem;
  }

  /* Reduce user indent on mobile */
  .user-message {
    padding-left: 1.5rem;
  }

  /* Input area sticky on mobile */
  .input-area {
    position: sticky;
    bottom: 0;
    background: white;
    box-shadow: 0 -2px 8px rgba(0, 0, 0, 0.05);
  }
}
```

### Typography Adjustments

```css
/* Mobile: Smaller font sizes for better readability */
@media (max-width: 767px) {
  :root {
    --text-base: 0.9375rem; /* 15px instead of 16px */
  }

  .font-serif {
    line-height: 1.6; /* Slightly more compact */
  }
}

/* Large Desktop: Larger font sizes */
@media (min-width: 1280px) {
  :root {
    --text-base: 1.0625rem; /* 17px */
  }
}
```

### Touch Optimization

```css
/* Larger touch targets on mobile */
@media (max-width: 767px) {
  .btn-icon {
    padding: 0.75rem; /* Minimum 44x44px */
  }

  .menu-item {
    padding: 0.75rem 1rem;
  }
}

/* Touch feedback */
@media (hover: none) {
  .interactive:active {
    background: var(--color-surface);
  }
}
```

---
## Accessibility

### Fundamental Principles

**1. Keyboard Navigation**
- All functions reachable with keyboard
- Meaningful tab order
- Visible focus state

**2. Screen Reader Support**
- Semantic HTML
- ARIA labels where needed
- Descriptive texts

**3. Visual Accessibility**
- Sufficient color contrast (WCAG AA minimum)
- No information conveyed solely through color
- Scalable font sizes

### Implementation

**Focus States:**
```css
/* Visible focus indicator */
:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
}

/* No outline on mouse click */
:focus:not(:focus-visible) {
  outline: none;
}
```

**Semantic HTML:**
```jsx
// -- Correct: Semantically correct
<main>
  <article role="log" aria-live="polite">
    {messages.map(msg => (
      <section key={msg.id} aria-label={`${msg.role} message`}>
        <div>{msg.content}</div>
        <time dateTime={msg.timestamp.toISOString()}>
          {formatTime(msg.timestamp)}
        </time>
      </section>
    ))}
  </article>
</main>

// -- Wrong: Non-semantic
<div>
  <div>
    {messages.map(msg => (
      <div key={msg.id}>
        <div>{msg.content}</div>
      </div>
    ))}
  </div>
</div>
```

**ARIA Labels:**
```jsx
<button
  aria-label="Send message"
  aria-disabled={!canSend}
  onClick={handleSend}
>
  <Send aria-hidden="true" />
</button>

<div
  role="status"
  aria-live="polite"
  aria-atomic="true"
>
  {isTyping && "Agent is typing..."}
</div>
```

**Color Contrast:**
```css
/* Minimum WCAG AA: 4.5:1 for normal text */
:root {
  --color-foreground: #1a1a1a;  /* 13.8:1 on #fafafa */
  --color-user-text: #2c2c2c;   /* 11.2:1 on #fafafa */
  --color-muted: #737373;        /* 4.6:1 on #fafafa */
}

/* For small text (< 18px): At least 4.5:1 */
/* For large text (>= 18px): At least 3:1 */
```

**Reduced Motion:**
```css
@media (prefers-reduced-motion: reduce) {
  * {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
```

---

## Implementation Guidelines

### Technology Stack

**Framework:** React + TypeScript
**Styling:** Tailwind CSS + CSS Variables
**State:** Zustand
**Icons:** Lucide React
**Build:** Vite

### CSS Variables Setup

```css
:root {
  /* Colors */
  --color-background: #fafafa;
  --color-surface: #ffffff;
  --color-foreground: #1a1a1a;
  --color-user-text: #2c2c2c;
  --color-muted: #737373;
  --color-border: #e5e5e5;
  --color-border-strong: #d4d4d4;

  --color-accent: #6b4c9a;
  --color-accent-hover: #5a3d85;

  --color-success: #059669;
  --color-success-bg: #d1fae5;
  --color-warning: #d97706;
  --color-warning-bg: #fef3c7;
  --color-error: #dc2626;
  --color-error-bg: #fee2e2;
  --color-system: #8b7355;
  --color-system-bg: #f5f5f4;

  /* Typography */
  --font-serif: 'Noto Serif Variable', 'Noto Serif', Georgia, serif;
  --font-sans: 'Noto Sans Variable', 'Noto Sans', -apple-system, sans-serif;
  --font-mono: 'Noto Sans Mono', 'Consolas', monospace;

  /* Layout */
  --content-max-width: 720px;
  --header-height: 64px;
  --input-min-height: 72px;

  /* Animation */
  --ease-standard: cubic-bezier(0.4, 0.0, 0.2, 1);
  --ease-decelerate: cubic-bezier(0.0, 0.0, 0.2, 1);
  --ease-accelerate: cubic-bezier(0.4, 0.0, 1, 1);

  --duration-instant: 100ms;
  --duration-fast: 150ms;
  --duration-normal: 200ms;
  --duration-slow: 300ms;
}
```

### Best Practices

**1. Component Composition**
```jsx
// -- Correct: Small, reusable components
<Message role="user" content="...">
  <MessageContent />
  <MessageMetadata />
  <MessageTimestamp />
</Message>

// -- Wrong: Monolithic components
<Message
  role="user"
  content="..."
  showMetadata={true}
  showTimestamp={true}
  renderContent={(c) => ...}
  renderMetadata={(m) => ...}
/>
```

**2. Type Safety**
```typescript
// Define strict types
type MessageRole = 'user' | 'agent' | 'system';

interface Message {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: Date;
  metadata?: {
    toolCalls?: string[];
    memories?: string[];
  };
}
```

**3. Performance**
```jsx
// Virtualization for long lists
import { useVirtualizer } from '@tanstack/react-virtual';

const MessageList = ({ messages }) => {
  const parentRef = useRef(null);

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 150,
  });

  return (
    <div ref={parentRef} className="overflow-auto">
      <div style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map(item => (
          <Message
            key={messages[item.index].id}
            {...messages[item.index]}
          />
        ))}
      </div>
    </div>
  );
};
```

---

## Summary

### What makes ownAI's design unique:

- **Typography-driven** -- Different fonts create distinct "voices"
- **Continuous** -- Not a chat, but a flowing conversation
- **Minimal** -- Color only with meaning, white space as a design element
- **Functional** -- Every element serves a purpose
- **Timeless** -- No trends, only fundamentals
- **Personal** -- Dignity for both conversation partners

### The most important rules:

1. **Calm over activism** -- Less is more
2. **Typography over decoration** -- Type carries the design
3. **Function over form** -- But both executed perfectly
4. **Consistency with character** -- Systematic, but not robotic
5. **Timelessness over trends** -- What still looks good in 5 years

---

**Last updated:** February 2026
**Next review:** On major feature additions

*This document lives and evolves with ownAI.*
