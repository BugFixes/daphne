CREATE TABLE IF NOT EXISTS logs (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    language TEXT NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    stacktrace TEXT,
    occurred_at TEXT NOT NULL,
    service TEXT,
    environment TEXT,
    attributes_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
    FOREIGN KEY(agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS log_archives (
    id TEXT PRIMARY KEY,
    cutoff_before TEXT NOT NULL,
    log_count INTEGER NOT NULL,
    summary_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_logs_occurred_at
    ON logs (occurred_at ASC);

CREATE INDEX IF NOT EXISTS idx_logs_account_occurred_at
    ON logs (account_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_bug_provider_sent_at
    ON notifications (bug_id, provider, sent_at DESC);
