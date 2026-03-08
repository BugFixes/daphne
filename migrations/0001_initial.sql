CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    create_tickets INTEGER NOT NULL,
    ticket_provider TEXT NOT NULL,
    notification_provider TEXT NOT NULL,
    notify_min_level TEXT NOT NULL,
    rapid_occurrence_window_minutes INTEGER NOT NULL,
    rapid_occurrence_threshold INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    name TEXT NOT NULL,
    api_key TEXT NOT NULL UNIQUE,
    api_secret TEXT NOT NULL,
    FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS bugs (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    language TEXT NOT NULL,
    severity TEXT NOT NULL,
    stacktrace_hash TEXT NOT NULL,
    normalized_stacktrace TEXT NOT NULL,
    latest_stacktrace TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    occurrence_count INTEGER NOT NULL,
    UNIQUE(account_id, stacktrace_hash),
    FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
    FOREIGN KEY(agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS occurrences (
    id TEXT PRIMARY KEY,
    bug_id TEXT NOT NULL,
    severity TEXT NOT NULL,
    stacktrace TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    FOREIGN KEY(bug_id) REFERENCES bugs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tickets (
    id TEXT PRIMARY KEY,
    bug_id TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL,
    remote_id TEXT NOT NULL,
    remote_url TEXT NOT NULL,
    priority TEXT NOT NULL,
    recommendation TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(bug_id) REFERENCES bugs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ticket_comments (
    id TEXT PRIMARY KEY,
    ticket_id TEXT NOT NULL,
    comment TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(ticket_id) REFERENCES tickets(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY,
    bug_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    message TEXT NOT NULL,
    sent_at TEXT NOT NULL,
    FOREIGN KEY(bug_id) REFERENCES bugs(id) ON DELETE CASCADE
);
