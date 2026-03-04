-- Initial schema for ownAI per-instance databases.

-- ============================================================
-- Messages (short-term memory)
-- ============================================================
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    role TEXT NOT NULL CHECK(role IN ('user', 'agent', 'system')),
    content TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    tokens_used INTEGER,
    importance_score REAL,
    metadata TEXT,
    summary_id TEXT REFERENCES summaries(id)
);

CREATE INDEX IF NOT EXISTS idx_messages_timestamp
    ON messages(timestamp);

-- ============================================================
-- Summaries (mid-term memory)
-- ============================================================
CREATE TABLE IF NOT EXISTS summaries (
    id TEXT PRIMARY KEY,
    start_message_id TEXT NOT NULL,
    end_message_id TEXT NOT NULL,
    summary_text TEXT NOT NULL,
    key_facts TEXT NOT NULL,        -- JSON array
    tools_mentioned TEXT,           -- JSON array
    topics TEXT,                    -- JSON array
    timestamp DATETIME NOT NULL,
    token_savings INTEGER,
    embedding BLOB,
    FOREIGN KEY (start_message_id) REFERENCES messages(id),
    FOREIGN KEY (end_message_id) REFERENCES messages(id)
);

CREATE INDEX IF NOT EXISTS idx_summaries_timestamp
    ON summaries(timestamp DESC);

-- ============================================================
-- Memory Entries (long-term memory with vector embeddings)
-- ============================================================
CREATE TABLE IF NOT EXISTS memory_entries (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    embedding BLOB NOT NULL,
    entry_type TEXT NOT NULL,
    importance REAL NOT NULL DEFAULT 0.5,
    created_at DATETIME NOT NULL,
    last_accessed DATETIME NOT NULL,
    access_count INTEGER NOT NULL DEFAULT 0,
    tags TEXT,                      -- JSON array
    source_message_ids TEXT,        -- JSON array
    collection_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_memory_type
    ON memory_entries(entry_type);

CREATE INDEX IF NOT EXISTS idx_memory_importance
    ON memory_entries(importance DESC);

CREATE INDEX IF NOT EXISTS idx_memory_collection
    ON memory_entries(collection_id);

-- ============================================================
-- Knowledge Collections (topic-based grouping of memory entries)
-- ============================================================
CREATE TABLE IF NOT EXISTS knowledge_collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    source TEXT,
    document_count INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_collections_name
    ON knowledge_collections(name);

-- ============================================================
-- User Profile (key-value settings)
-- ============================================================
CREATE TABLE IF NOT EXISTS user_profile (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME NOT NULL
);

-- ============================================================
-- Dynamic Tools (Rhai scripts)
-- ============================================================
CREATE TABLE IF NOT EXISTS tools (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '1.0.0',
    script_content TEXT NOT NULL,
    parameters TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'deprecated', 'testing')),
    created_at DATETIME NOT NULL,
    last_used DATETIME,
    usage_count INTEGER DEFAULT 0,
    success_count INTEGER DEFAULT 0,
    failure_count INTEGER DEFAULT 0,
    parent_tool_id TEXT
);

-- ============================================================
-- Tool Execution Log
-- ============================================================
CREATE TABLE IF NOT EXISTS tool_executions (
    id TEXT PRIMARY KEY,
    tool_id TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    execution_time_ms INTEGER,
    error_message TEXT,
    input_params TEXT,
    output TEXT,
    FOREIGN KEY (tool_id) REFERENCES tools(id)
);

CREATE INDEX IF NOT EXISTS idx_tool_executions_tool_id
    ON tool_executions(tool_id);

-- ============================================================
-- Canvas Programs
-- ============================================================
CREATE TABLE IF NOT EXISTS programs (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '1.0.0',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    UNIQUE(instance_id, name)
);

CREATE INDEX IF NOT EXISTS idx_programs_instance_id
    ON programs(instance_id);

-- ============================================================
-- Program Data (key-value storage per program, used by Bridge API)
-- ============================================================
CREATE TABLE IF NOT EXISTS program_data (
    program_name TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    updated_at DATETIME NOT NULL,
    PRIMARY KEY (program_name, key)
);

-- ============================================================
-- Scheduled Tasks (cron-based recurring agent tasks)
-- ============================================================
CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    name TEXT NOT NULL,
    cron_expression TEXT NOT NULL,
    task_prompt TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    notify INTEGER NOT NULL DEFAULT 1,
    last_run DATETIME,
    last_result TEXT,
    created_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_instance_id
    ON scheduled_tasks(instance_id);
