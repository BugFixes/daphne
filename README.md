# bugfix.es

`bugfix.es` is a Rust service for receiving stacktraces from embedded agents, deduplicating them by normalized hash, and driving the operational workflow that follows:

- create a ticket when the account is configured to do so
- generate an AI recommendation alongside the ticket
- notify users only when the account severity threshold says it should
- escalate or comment on the existing ticket when the bug repeats

This repository is a fresh implementation. The abandoned Go prototype in `../celeste` was used only as a reference for system boundaries.

## What exists now

- `POST /v1/accounts` creates an account with ticketing and notification policy.
- `POST /v1/agents` creates an agent and returns its `api_key`.
- `POST /v1/events/stacktraces` ingests a stacktrace event, hashes it, stores occurrences, creates or updates a ticket, and records notifications.
- `GET /healthz` returns a basic health response.

The current providers are local stubs for:

- ticketing: `jira`, `linear`, `tracklines`
- notifications: `slack`, `teams`, `resend`
- AI: heuristic recommendation generator

Those stubs let the full workflow run end-to-end before wiring real external APIs.

## Run

```bash
cargo run
```

Environment variables:

- `BUGFIXES_BIND_ADDRESS` default: `127.0.0.1:3000`
- `BUGFIXES_DATABASE_URL` default: `sqlite://bugfixes.db`

## Example flow

Create an account:

```bash
curl -X POST http://127.0.0.1:3000/v1/accounts \
  -H 'content-type: application/json' \
  -d '{
    "name": "Acme",
    "create_tickets": true,
    "ticket_provider": "jira",
    "notification_provider": "slack",
    "notify_min_level": "error",
    "rapid_occurrence_window_minutes": 30,
    "rapid_occurrence_threshold": 3
  }'
```

Create an agent:

```bash
curl -X POST http://127.0.0.1:3000/v1/agents \
  -H 'content-type: application/json' \
  -d '{
    "account_id": "REPLACE_WITH_ACCOUNT_ID",
    "name": "backend-prod"
  }'
```

Send a stacktrace:

```bash
curl -X POST http://127.0.0.1:3000/v1/events/stacktraces \
  -H 'content-type: application/json' \
  -d '{
    "agent_key": "REPLACE_WITH_AGENT_KEY",
    "language": "rust",
    "level": "error",
    "stacktrace": "thread '\''main'\'' panicked at '\''index out of bounds'\''"
  }'
```

## Data model

- `accounts` own the policy: create tickets or not, which ticketing system to use, when to notify, and what counts as a rapid repeat.
- `agents` authenticate intake requests.
- `bugs` are deduplicated by `account_id + stacktrace_hash`.
- `occurrences` store each event so rapid-repeat detection can be based on time windows.
- `tickets` store the external issue reference plus AI recommendation and current priority.
- `notifications` and `ticket_comments` keep a local audit trail.

## Next steps

- replace stub ticketing providers with Jira, Linear, and Tracklines clients
- replace the heuristic AI advisor with a real model-backed implementation
- add account-specific provider credentials and webhook targets
- add richer normalization per language runtime so equivalent traces hash together more reliably
