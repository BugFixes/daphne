# AGENTS.md

## Project Summary
- `bugfix.es` is a Rust service that ingests stacktraces and logs, deduplicates bugs, and drives policy-gated ticketing, notification, and AI workflows.
- The application uses Axum for HTTP, SQLx for persistence, and Refinery SQL migrations for schema changes.
- The service uses Postgres in all environments via `BUGFIXES_DATABASE_URL`.

## Repository Layout
- `src/api/` contains HTTP routes and handlers.
- `src/service/` contains workflow orchestration, especially `IntakeService`.
- `src/repository/` contains database access (Postgres via SQLx).
- `src/domain/` contains core models and shared types.
- `src/ticketing/`, `src/notifications/`, and `src/ai/` contain provider registries, traits, and stub implementations.
- `src/policy/` and `policies/` contain policy execution and policy assets.
- `migrations/` contains additive SQL migrations named `V{N}__description.sql`.

## Working Rules
- Keep changes focused and minimal; fix root causes rather than layering workarounds.
- Follow existing Rust style and module structure; do not rename files or move modules unless required.
- Provider implementations are intentionally stubbed unless the task explicitly requires real integrations.
- Treat migrations as forward-only and additive; do not rewrite applied migration history.
- Update documentation when behavior, API shape, environment variables, or workflows change.

## Validation
- Run formatting, linting, and tests before finalizing code changes:
  - `just fmt`
  - `just clippy`
  - `just test`
  - `just check`
- If you change behavior, add or update focused tests near the affected module when practical.
- Do not conclude work with failing `fmt`, `clippy`, or `test` unless the user explicitly asks for partial progress.

## Environment Notes
- Main runtime config lives in environment variables documented in `README.md` and `.env.example`.
- Default bind address: `127.0.0.1:3000`
- Default local database: `postgres://postgres:postgres@127.0.0.1:5432/bugfixes`

## Common Expectations
- Prefer `rg` for search and file discovery.
- Read large files in chunks instead of loading entire files at once.
- Before starting a new web/server process, stop any previous instance you started.
- Do not commit, create branches, or perform destructive Git operations unless the user asks.
