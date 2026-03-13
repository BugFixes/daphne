# bugfix.es

`bugfix.es` is a Rust service for receiving stacktraces from embedded agents, deduplicating them by normalized hash, and driving the operational workflow that follows:

- create a ticket when the account is configured to do so
- generate an AI recommendation alongside the ticket
- notify users only when the account severity threshold says it should
- escalate or comment on the existing ticket when the bug repeats

This repository is a fresh implementation. The abandoned Go prototype in `../celeste` was used only as a reference for system boundaries.

## What exists now

- `POST /v1/accounts` creates an account with ticketing and notification policy.
- `POST /v1/agents` creates an agent and returns its `api_key` and `api_secret`.
- `POST /v1/log` accepts the native `go-bugfixes/logs` and `bugfixes-rs` log payload shape.
- `POST /v1/bug` accepts the native `go-bugfixes/middleware` and `bugfixes-rs` panic payload shape.
- `POST /v1/events/stacktraces` remains available as a generic non-Go intake path.
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

## Database Migrations

`bugfix.es` now uses `refinery` as its migration system.

- Migrations live in `migrations/`.
- The application runs embedded `refinery` migrations during startup before the API begins serving traffic.
- Migration files should be named like `V1__create_accounts.sql`.

Current state:

- `refinery` is now the migration mechanism and startup entrypoint.
- The existing schema is still created by repository bootstrap code until the follow-up migration tickets land.
- That means this change establishes the migration workflow first, and the schema ownership move follows next.

Suggested CLI workflow:

```bash
cargo install refinery_cli
refinery migrate --help
```

When authoring migrations for this project:

- prefer SQL migrations in `migrations/`
- keep migrations additive and explicit
- run the app locally to apply pending migrations on startup
- treat schema bootstrap removal as a separate change once the initial migration exists

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

The agent response includes both `api_key` and `api_secret`.

Send a `go-bugfixes/logs` payload:

```bash
curl -X POST http://127.0.0.1:3000/v1/log \
  -H 'content-type: application/json' \
  -H 'X-API-KEY: REPLACE_WITH_AGENT_KEY' \
  -H 'X-API-SECRET: REPLACE_WITH_AGENT_SECRET' \
  -d '{
    "log": "database timeout",
    "level": "error",
    "file": "/srv/app/main.go",
    "line": "42",
    "line_number": 42,
    "stack": "Z29yb3V0aW5lIDEgW3J1bm5pbmdd"
  }'
```

Send a `go-bugfixes/middleware` panic payload:

```bash
curl -X POST http://127.0.0.1:3000/v1/bug \
  -H 'content-type: application/json' \
  -H 'X-API-KEY: REPLACE_WITH_AGENT_KEY' \
  -H 'X-API-SECRET: REPLACE_WITH_AGENT_SECRET' \
  -d '{
    "bug": "panic: index out of bounds",
    "raw": "main.go:42\npanic: index out of bounds",
    "bug_line": "main.go:42",
    "file": "main.go",
    "line": "42",
    "line_number": 42,
    "level": "panic"
  }'
```

`../bugfixes-rs` currently targets the same `POST /log` and `POST /bug` contract with the same `X-API-KEY` and `X-API-SECRET` headers, so the Rust agent can use this service without a separate intake path.

## Data model

- `accounts` own the policy: create tickets or not, which ticketing system to use, when to notify, and what counts as a rapid repeat.
- `agents` authenticate intake requests with `X-API-KEY` and `X-API-SECRET`.
- `bugs` are deduplicated by `account_id + stacktrace_hash`.
- `occurrences` store each event so rapid-repeat detection can be based on time windows.
- `tickets` store the external issue reference plus AI recommendation and current priority.
- `notifications` and `ticket_comments` keep a local audit trail.

## Next steps

- replace stub ticketing providers with Jira, Linear, and Tracklines clients
- replace the heuristic AI advisor with a real model-backed implementation
- add account-specific provider credentials and webhook targets
- add richer normalization per language runtime so equivalent traces hash together more reliably
- add request fixtures derived from `../go-bugfixes` so the agent and service contract stays locked together
- add request fixtures derived from both `../go-bugfixes` and `../bugfixes-rs` so agent compatibility is tested, not assumed
