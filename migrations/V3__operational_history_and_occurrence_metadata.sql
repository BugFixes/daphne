ALTER TABLE occurrences ADD COLUMN IF NOT EXISTS service TEXT;
ALTER TABLE occurrences ADD COLUMN IF NOT EXISTS environment TEXT;
ALTER TABLE occurrences ADD COLUMN IF NOT EXISTS attributes_json TEXT NOT NULL DEFAULT '{}';

CREATE TABLE IF NOT EXISTS account_provider_configs (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    provider TEXT NOT NULL,
    api_key TEXT,
    settings_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(account_id, kind),
    FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ticket_events (
    id TEXT PRIMARY KEY,
    ticket_id TEXT NOT NULL,
    bug_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    action TEXT NOT NULL,
    comment TEXT,
    previous_priority TEXT,
    next_priority TEXT,
    occurred_at TEXT NOT NULL,
    FOREIGN KEY(ticket_id) REFERENCES tickets(id) ON DELETE CASCADE,
    FOREIGN KEY(bug_id) REFERENCES bugs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS notification_events (
    id TEXT PRIMARY KEY,
    bug_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    status TEXT NOT NULL,
    reason TEXT NOT NULL,
    message TEXT,
    severity TEXT NOT NULL,
    ticket_action TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    FOREIGN KEY(bug_id) REFERENCES bugs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_occurrences_bug_occurred_at
    ON occurrences (bug_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_ticket_events_bug_occurred_at
    ON ticket_events (bug_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_notification_events_bug_occurred_at
    ON notification_events (bug_id, occurred_at DESC);
