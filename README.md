# bugfix.es

`bugfix.es` is a Rust service for receiving stacktraces from embedded agents, deduplicating them by normalized hash, and driving the operational workflow that follows:

- create a ticket when the account is configured to do so
- generate an AI recommendation alongside the ticket
- notify users only when the account severity threshold says it should
- escalate or comment on the existing ticket when the bug repeats

Ticketing, notification, and AI execution are policy-gated from raw account context. The service sends the chosen provider or advisor, whether that integration is enabled, and any configured credential to the policy engine, and the policy decides whether the action is allowed.

This repository is a fresh implementation. The abandoned Go prototype in `../celeste` was used only as a reference for system boundaries.

## What exists now

- `POST /v1/accounts` creates an account with ticketing and notification policy.
- `POST /v1/agents` creates an agent and returns its `api_key` and `api_secret`.
- `POST /v1/log` accepts raw `go-bugfixes/logs` and `bugfixes-rs` log payloads, then maps them into canonical stacktrace events.
- `POST /v1/bug` accepts raw `go-bugfixes/middleware` and `bugfixes-rs` panic payloads, then maps them into canonical stacktrace events.
- `POST /v1/events/stacktraces` accepts the canonical stacktrace event payload directly.
- `GET /healthz` returns a basic health response.

The current providers are local stubs for:

- ticketing: `jira`, `github`, `linear`, `tracklines`
- notifications: `slack`, `teams`, `resend`
- AI: `codex`, `claude`, `kimi`

Those stubs let the full workflow run end-to-end before wiring real external APIs.

The service uses Postgres for all environments, including local development, tests, and production.

Schema setup is handled through SQL migrations in [`migrations/`](./migrations).

## Run

```bash
cargo run
```

Common repo tasks:

```bash
just fmt
just clippy
just test
just check
```

Verification policy:

- run `just check` before pushing changes
- keep `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` passing
- DB-backed tests provision Postgres with testcontainers, so Docker must be available when running `cargo test --all-features`
- add or update tests for any behavior change, bug fix, policy change, migration, or API change
- if a change genuinely cannot be covered by an automated test yet, call that out explicitly in the pull request

Environment variables:

- `BUGFIXES_BIND_ADDRESS` default: `127.0.0.1:3000`
- `BUGFIXES_DATABASE_URL` default: `postgres://postgres:postgres@127.0.0.1:5432/bugfixes`
- `BUGFIXES_FEATURE_FLAGS_PROVIDER` default: `local`
- `BUGFIXES_POLICY_PROVIDER` default: `local`
- `BUGFIXES_POLICY2_ENGINE_URL` default: `https://api.policy2.net/run`
- `BUGFIXES_DISABLED_FEATURES` optional comma-separated local disable list
- `BUGFIXES_FLAGSGG_PROJECT_ID` optional when using `flagsgg`
- `BUGFIXES_FLAGSGG_AGENT_ID` optional when using `flagsgg`
- `BUGFIXES_FLAGSGG_ENVIRONMENT_ID` optional when using `flagsgg`

Use `.env.example` as the configuration reference.

## Database Migrations

`bugfix.es` now uses `refinery` as its migration system.

- Migrations live in `migrations/`.
- The application and test setup initialize databases through embedded `refinery` migrations.
- Migration files should be named like `V1__create_accounts.sql`.

Current state:

- `refinery` is now the migration mechanism and startup entrypoint.
- Migration SQL lives in `migrations/` and runs from the shared repository initialization path for Postgres URLs.
- Repository bootstrap should not own schema creation going forward.

Suggested CLI workflow:

```bash
cargo install refinery_cli
refinery migrate --help
```

When authoring migrations for this project:

- prefer SQL migrations in `migrations/`
- keep migrations additive and explicit
- run the app locally to apply pending migrations on startup
- use the `V{N}__description.sql` naming convention that `refinery` expects

## Example flow

Create an account:

```bash
curl -X POST http://127.0.0.1:3000/v1/accounts \
  -H 'content-type: application/json' \
  -d '{
    "name": "Acme",
    "create_tickets": true,
    "ticket_provider": "jira",
    "ticketing_api_key": "jira_test_key",
    "notification_provider": "slack",
    "notification_api_key": "slack_webhook_or_key",
    "ai_enabled": true,
    "use_managed_ai": true,
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

The intake model is stacktrace-first:

- raw agent and API payloads describe what was observed at the edge
- canonical stacktrace events are the service input used for hashing, deduplication, and workflow decisions
- bugs, occurrences, tickets, and notifications are derived from those canonical stacktrace events

## Canonical bug identity

A deduplicated bug is currently defined by the tuple `account_id + stacktrace_hash`.

`stacktrace_hash` is derived as `sha256(normalize(stacktrace))`. The current normalization trims each line, drops empty lines, and replaces hex memory addresses with `0xADDR` before hashing.

Field roles in the canonical event:

- transport and authentication only: `agent_key`, `agent_secret`
- bug identity primitive: `stacktrace`
- occurrence-level metadata: `level`, `occurred_at`, `service`, `environment`, `attributes`

Fields derived or materialized downstream from those primitives:

- `normalized_stacktrace` stores the canonicalized form of the raw `stacktrace`
- `stacktrace_hash` stores the hash of `normalized_stacktrace`
- `first_seen_at` comes from the occurrence that created the bug
- `last_seen_at` comes from the most recent occurrence for the bug
- `occurrence_count` is the count of stored occurrences for the bug
- `severity` is the highest severity seen for the bug so far
- `latest_stacktrace` is the newest raw stacktrace stored for the bug

Current bug record semantics that are easy to miss:

- `language` is stored on the bug, but it is copied from the first occurrence and is not part of deduplication
- `agent_id` is also copied from the first occurrence that created the bug
- `service`, `environment`, and `attributes` are accepted on canonical events but do not currently participate in deduplication or persistence

## Data model

- `accounts` own the policy: create tickets or not, which ticketing system to use, when to notify, and what counts as a rapid repeat.
- `agents` authenticate intake requests with `X-API-KEY` and `X-API-SECRET`.
- `bugs` are deduplicated by `account_id + stacktrace_hash`.
- `occurrences` store each event so rapid-repeat detection can be based on time windows.
- `tickets` store the external issue reference plus AI recommendation and current priority.
- `notifications` and `ticket_comments` keep a local audit trail.

## Feature Flags

Feature flags can be used to dark-launch integrations and AI providers.

Current provider flag keys:

- `jira`
- `github`
- `linear`
- `tracklines`
- `slack`
- `teams`
- `resend`

Current AI flag key:

- `ai/codex`

By default the service uses local always-on flags with optional disables via `BUGFIXES_DISABLED_FEATURES`. Local disables accept the bare provider names above and still accept the older namespaced values for compatibility.

If you want runtime flags from `flags.gg`, build with:

```bash
cargo run --features flagsgg
```

and set the `BUGFIXES_FLAGSGG_*` environment variables.

## Policies

Business decisions can now run locally or through `policy2`.

Current embedded policies live in [`policies/create_ticket.policy`](./policies/create_ticket.policy), [`policies/escalate_repeat.policy`](./policies/escalate_repeat.policy), [`policies/send_notification.policy`](./policies/send_notification.policy), and [`policies/use_ai.policy`](./policies/use_ai.policy).

Matching JSON Schemas for those `decision` payloads live in [`policies/create_ticket.schema.json`](./policies/create_ticket.schema.json), [`policies/escalate_repeat.schema.json`](./policies/escalate_repeat.schema.json), [`policies/send_notification.schema.json`](./policies/send_notification.schema.json), and [`policies/use_ai.schema.json`](./policies/use_ai.schema.json).

By default the service uses a local policy engine that preserves the current Rust behavior. To delegate those checks to `policy2`, set:

```bash
BUGFIXES_POLICY_PROVIDER=policy2
BUGFIXES_POLICY2_ENGINE_URL=https://api.policy2.net/run
```

The `policy2` client sends the embedded rule text and the policy input payload to the engine. That payload includes stack facts, the chosen provider or advisor, the relevant enablement booleans, and the configured API key where that action requires one.

When `BUGFIXES_POLICY_PROVIDER=policy2`, `policy2` is authoritative for the `true` or `false` decision. The Rust service only supplies facts and performs the operation if policy returns `true`. Use `BUGFIXES_POLICY_PROVIDER=local` only for local development or when you explicitly want the built-in Rust evaluator.

## Contribution Expectations

- do not merge code that leaves `fmt`, `clippy`, or `test` failing
- treat new functionality and behavior changes as incomplete unless the matching tests are added or updated
- prefer focused tests near the changed module over broad unstructured regression coverage
- when fixing a bug, add the test that would have caught it first when practical

## Next steps

- replace stub ticketing providers with Jira, GitHub Issues, Linear, and Tracklines clients
- replace stub AI advisors with real model-backed Codex, Claude, and Kimi integrations
- add account-specific provider credentials and webhook targets
- add richer normalization per language runtime so equivalent traces hash together more reliably
- add request fixtures derived from `../go-bugfixes` so the agent and service contract stays locked together
- add request fixtures derived from both `../go-bugfixes` and `../bugfixes-rs` so agent compatibility is tested, not assumed
