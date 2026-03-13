# CLAUDE.md

## Project Overview

`bugfix.es` (codename: Daphne) is a Rust microservice that receives stacktraces from embedded agents, deduplicates them by normalized hash, and drives the operational workflow: ticket creation, AI recommendations, notifications, and escalation on repeat occurrences.

## Build and Development Commands

```bash
cargo run                                                      # Run the service
cargo fmt --all                                                # Format code
cargo clippy --all-targets --all-features -- -D warnings      # Lint
cargo test --all-features                                      # Run all tests
cargo run --features flagsgg                                   # Run with flags.gg support
```

The README references `just fmt`, `just clippy`, `just test`, and `just check` — a Justfile is planned but not yet present. Use the `cargo` commands directly.

Run `just check` (or its cargo equivalent) before pushing. All three checks — fmt, clippy, tests — must pass.

## Architecture

### Layer Structure

```
api/          HTTP routes and handlers (Axum)
service/      IntakeService — core workflow orchestration
repository/   Database operations (SQLite + Postgres via SQLx)
domain/       Data models and types
ticketing/    Pluggable ticketing providers (Jira, GitHub, Linear, Tracklines)
notifications/ Pluggable notification providers (Slack, Teams, Resend)
ai/           Pluggable AI advisors (Codex, Claude, Kimi)
policy/       Policy engine (local Rust evaluator or policy2.net delegation)
feature_flags/ Feature flag system (local or flags.gg)
config.rs     Environment variable configuration
migrations.rs Refinery migration runner (runs at startup)
```

### Key Flows

1. Stacktrace arrives via `POST /v1/log`, `POST /v1/bug`, or `POST /v1/events/stacktraces`
2. Auth is validated via `X-API-KEY` / `X-API-SECRET` headers
3. `IntakeService` normalizes and hashes the stacktrace, deduplicates against existing bugs
4. Policy engine gates: ticket creation, AI recommendation, notification sending, repeat escalation
5. Registry dispatches to the appropriate provider stub

### Provider Architecture

All three provider categories (ticketing, notifications, AI) follow the same pattern:
- A `*Registry` that maps provider type enums to trait implementations
- A trait defining the provider interface
- Stub implementations that log actions but do not call external APIs yet

## Database

- **Development / tests**: SQLite (default `sqlite://bugfixes.db`; in-memory for tests)
- **Production**: Postgres (set `BUGFIXES_DATABASE_URL`)
- **Migrations**: Refinery, embedded at compile time, run at startup before serving traffic
- Migration files live in `migrations/`, named `V{N}__description.sql`
- Migrations are additive and forward-only

## Policy Engine

Two implementations, selected by `BUGFIXES_POLICY_PROVIDER`:

- `local` (default) — Rust evaluator embedded in the binary
- `policy2` — delegates to `policy2.net`; sends embedded rule text + input payload

Policy rule files: `policies/*.policy`
JSON Schemas for payloads: `policies/*.schema.json`

Four policy decisions: `create_ticket`, `escalate_repeat`, `send_notification`, `use_ai`

## Feature Flags

Two implementations, selected by `BUGFIXES_FEATURE_FLAGS_PROVIDER`:

- `local` (default) — always-on with optional disables via `BUGFIXES_DISABLED_FEATURES`
- `flagsgg` — runtime evaluation via flags.gg (requires `--features flagsgg` build and `BUGFIXES_FLAGSGG_*` env vars)

## Environment Variables

| Variable | Default | Notes |
|---|---|---|
| `BUGFIXES_BIND_ADDRESS` | `127.0.0.1:3000` | |
| `BUGFIXES_DATABASE_URL` | `sqlite://bugfixes.db` | Use `postgres://...` for Postgres |
| `BUGFIXES_FEATURE_FLAGS_PROVIDER` | `local` | `local` or `flagsgg` |
| `BUGFIXES_POLICY_PROVIDER` | `local` | `local` or `policy2` |
| `BUGFIXES_POLICY2_ENGINE_URL` | `https://api.policy2.net/run` | Only when using `policy2` |
| `BUGFIXES_DISABLED_FEATURES` | — | Optional comma-separated disable list |
| `BUGFIXES_FLAGSGG_PROJECT_ID` | — | Required for `flagsgg` |
| `BUGFIXES_FLAGSGG_AGENT_ID` | — | Required for `flagsgg` |
| `BUGFIXES_FLAGSGG_ENVIRONMENT_ID` | — | Required for `flagsgg` |

See `.env.example` for a complete reference.

## Testing

Tests live in `tests.rs` files per module. Run with:

```bash
cargo test --all-features
```

Test requirements:
- Add or update tests for every behavior change, bug fix, policy change, migration, or API change
- Prefer focused tests near the changed module
- When fixing a bug, write the failing test first when practical
- If a change genuinely cannot be covered, call it out explicitly in the PR

## Contribution Rules

- Do not merge code that leaves `fmt`, `clippy`, or `test` failing
- New functionality is incomplete without matching tests
- All provider implementations are currently stubs — this is intentional
- Follow the phase-based roadmap in `ROADMAP.md` for sequencing work
