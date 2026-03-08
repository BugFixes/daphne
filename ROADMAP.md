# bugfix.es Roadmap

This document is the working roadmap for `bugfix.es`.

It has two jobs:

- define the order we build things in
- provide a place to attach dates and delivery expectations later

Dates in this file are intentionally provisional until we convert them into committed milestones.

## Planning Rules

- Keep platform work ahead of integration sprawl.
- Prefer one stable contract for agents over custom per-language APIs.
- Build the internal abstractions before adding external providers.
- Treat `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` as the default Rust verification baseline.
- Require GitHub Actions to run the same verification baseline before merge.
- Add timelines only when the dependency chain is clear.
- Mark anything date-based with a last-reviewed date.

## Current State

Status as of `2026-03-08`:

- Rust service scaffold exists.
- SQLite-backed persistence exists.
- Core intake flow exists: dedupe, ticket create/update, AI recommendation stub, notification stub.
- Go agent contract is supported on `/v1/log` and `/v1/bug`.
- Rust agent contract is compatible with the same endpoints.
- Provider implementations are still stubs.
- Module structure still needs to be reorganized into clearer provider groups.

## Delivery Phases

### Phase 0: Foundation Cleanup

Goal:
- make the codebase easier to extend safely

Scope:
- split `providers` into `ticketing/*`, `notifications/*`, and `ai/*`
- move tests into separate sibling files
- keep one crate for now
- document module boundaries and ownership
- add a repo-level `justfile` for standard dev and verification commands
- add GitHub Actions so pull requests run the same checks automatically

Target window:
- estimate: `1-3 days`
- target dates: `TBD`

Dependencies:
- none

Exit criteria:
- provider code is grouped by concern
- tests are separated from production files
- module layout is stable enough for new integrations
- verification commands are standardized and documented
- CI runs format, clippy, and tests on pull requests

Status:
- `planned`

### Phase 1: Core Product Readiness

Goal:
- make the current service usable as a real internal alpha

Scope:
- improve stacktrace normalization
- add migrations instead of schema-on-start only
- add configuration for provider credentials
- add structured audit history for ticket updates and notifications
- add better error handling around external provider failures
- add end-to-end integration tests

Target window:
- estimate: `1-2 weeks`
- target dates: `TBD`

Dependencies:
- Phase 0

Exit criteria:
- local development flow is stable
- schemas are versioned
- intake behavior is deterministic and test-covered

Status:
- `planned`

### Phase 2: First External Integrations

Goal:
- support the highest-value external systems first

Scope:
- Jira ticketing
- Slack notifications
- GitHub Issues ticketing
- Teams notifications

Recommended order:
1. Jira
2. Slack
3. GitHub Issues
4. Teams

Why this order:
- Jira gives the broadest commercial ticketing coverage.
- Slack is the fastest path to developer-facing notifications.
- GitHub Issues is a strong second ticketing target with simple API ergonomics.
- Teams is important, but usually lands later in enterprise rollouts.

Target window:
- estimate: `2-4 weeks`
- target dates: `TBD`

Dependencies:
- Phase 1

Exit criteria:
- each provider supports create/update flows
- provider credentials are configurable per account
- failures are visible and recoverable

Status:
- `planned`

### Phase 3: Product Differentiation

Goal:
- make `bugfix.es` meaningfully better than a plain ticket-forwarder

Scope:
- replace heuristic AI with model-backed recommendations
- attach remediation suggestions directly to tickets
- add frequency-aware escalation policies
- add language-specific stacktrace normalization
- add account-level routing policy controls

Target window:
- estimate: `2-6 weeks`
- target dates: `TBD`

Dependencies:
- Phase 2

Exit criteria:
- AI recommendations are useful enough to keep enabled by default
- repeat-event handling is policy-driven, not hard-coded
- multiple language runtimes normalize reliably

Status:
- `planned`

### Phase 4: Tracklines Dogfooding

Goal:
- use `bugfix.es` to improve `Tracklines` while validating the internal provider path

Scope:
- implement Tracklines ticket adapter
- use Tracklines as an internal reference integration
- ensure the provider interface supports both internal and external systems cleanly

Target window:
- estimate: `parallel with Phase 2 or Phase 3`
- target dates: `TBD`

Dependencies:
- Phase 0 minimum

Exit criteria:
- Tracklines works through the same ticketing abstraction as Jira/GitHub/Linear
- internal dogfooding produces real issues and workflow feedback

Status:
- `planned`

### Phase 5: Broader Market Coverage

Goal:
- expand to the next most useful systems once the first wave is stable

Scope:
- Linear
- Resend
- webhooks / generic outbound notifications
- additional ticketing systems after demand validation

Target window:
- estimate: `TBD`
- target dates: `TBD`

Dependencies:
- Phase 2

Exit criteria:
- roadmap additions are driven by user demand, not guesswork

Status:
- `planned`

## Integration Priority

### Ticketing

Priority order:
1. Jira
2. GitHub Issues
3. Linear
4. Tracklines

Notes:
- `Tracklines` should be built early enough for dogfooding, but not treated as the first external-market integration.
- If target customers are mostly startups already living in GitHub, GitHub Issues can be promoted ahead of Jira.

### Notifications

Priority order:
1. Slack
2. Teams
3. Resend

Notes:
- Slack is the first developer-friendly default.
- Teams is likely necessary for enterprise adoption.
- Resend is useful once email notification strategy is clearer.

## Timeline Template

Use this section when we are ready to convert estimates into actual delivery dates.

### Milestone Template

- milestone:
- owner:
- status:
- start date:
- target date:
- actual date:
- dependencies:
- risks:
- notes:

## Open Questions

- Should GitHub Issues move ahead of Jira if the first target users are solo developers and small teams?
- Should Tracklines be built in parallel with Jira for internal validation?
- Do we want one generic notification abstraction plus typed providers, or separate internal paths for chat and email?
- When do we replace SQLite with Postgres for multi-tenant production use?
- Should AI recommendations be synchronous on intake, or queued asynchronously after ticket creation?

## Near-Term Next Steps

1. Refactor the source layout into `ticketing/*`, `notifications/*`, and `ai/*`.
2. Standardize verification through `just fmt`, `just clippy`, `just test`, and `just check`.
3. Enforce the same verification in GitHub Actions before merge.
4. Add a migrations strategy.
5. Implement Jira as the first real ticketing provider.
6. Implement Slack as the first real notification provider.
7. Turn this roadmap into dated milestones once Phase 0 is finished.

## Change Log

- `2026-03-08`: initial roadmap created
- `2026-03-08`: added repo verification workflow and `justfile` requirement
- `2026-03-08`: added GitHub Actions requirement for pre-merge verification
