-- Add 'tool_result' role to messages table for storing tool call results
-- in the conversation history. This enables proper multi-turn tool calling
-- where tool_use/tool_result blocks are preserved across sessions.
--
-- SQLite does not support ALTER TABLE to modify CHECK constraints,
-- so we recreate the table with the expanded constraint.

CREATE TABLE messages_new (
    id TEXT PRIMARY KEY,
    role TEXT NOT NULL CHECK(role IN ('user', 'agent', 'system', 'tool_result')),
    content TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    tokens_used INTEGER,
    importance_score REAL,
    metadata TEXT,
    summary_id TEXT REFERENCES summaries(id)
);

INSERT INTO messages_new SELECT * FROM messages;

DROP TABLE messages;

ALTER TABLE messages_new RENAME TO messages;

CREATE INDEX IF NOT EXISTS idx_messages_timestamp
    ON messages(timestamp);
