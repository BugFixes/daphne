# Provider Configuration Contract

This document defines the target contract for `CHE-49`: provider-specific configuration and secret handling for Daphne accounts.

## Why this exists

The current account model is too narrow for real integrations:

- `accounts` stores one provider enum and one `*_api_key` field per capability.
- `account_provider_configs` copies that state into `provider`, `api_key`, and `settings_json`.
- secrets are readable server-side values with no explicit masked or write-only contract.

That shape is acceptable for stub providers, but it will not scale once the dashboard needs to edit provider settings or real integrations need more than one credential.

## Goals

- support provider-specific public settings without forcing everything into a single `api_key`
- treat secrets as write-only inputs with explicit read metadata
- let the dashboard render a stable form contract per provider kind
- allow disabled or incomplete configurations during onboarding
- keep the model general enough for future providers

## Canonical model

Provider configuration is owned per account and capability kind, but cardinality differs by system:

- `ticketing`: zero or one config
- `notifications`: zero or many configs
- `ai`: zero or many configs

For AI specifically, zero customer-managed configs does not fully describe behavior by itself. The account also needs an AI mode that decides whether zero configs means "disabled" or "use Daphne-managed AI".

```json
{
  "account_id": "uuid",
  "kind": "ticketing",
  "provider": "jira",
  "enabled": true,
  "status": "configured",
  "fields": {
    "base_url": "https://acme.atlassian.net",
    "project_key": "OPS"
  },
  "secrets": {
    "email": {
      "present": true,
      "display_hint": "op***@acme.test",
      "last_updated_at": "2026-03-18T10:00:00Z"
    },
    "api_token": {
      "present": true,
      "display_hint": "***9fd2",
      "last_updated_at": "2026-03-18T10:00:00Z"
    }
  },
  "updated_at": "2026-03-18T10:00:00Z"
}
```

### Top-level fields

- `kind`: one of `ticketing`, `notification`, `ai`
- `provider`: provider key within that capability
- `enabled`: operational toggle for this capability
- `status`: one of `not_configured`, `incomplete`, `configured`, `invalid`
- `fields`: non-secret provider settings, always readable
- `secrets`: secret metadata, never raw secret values

## Secret contract

Secrets are never returned once written.

Read shape:

```json
{
  "present": true,
  "display_hint": "***9fd2",
  "last_updated_at": "2026-03-18T10:00:00Z"
}
```

Write shape:

```json
{
  "secrets": {
    "api_token": {
      "op": "set",
      "value": "secret-value"
    },
    "email": {
      "op": "clear"
    }
  }
}
```

Rules:

- omitting a secret key means "leave unchanged"
- `op = set` replaces the existing stored value
- `op = clear` deletes the existing stored value
- blank strings are treated as invalid for `set`
- responses only expose `present`, `display_hint`, and `last_updated_at`
- required secrets may be absent while a config is saved in `incomplete` status

### Masking rules

- tokens, API keys, and webhook URLs expose only a short suffix: `***abcd`
- email-like secrets expose a partially masked local part: `op***@acme.test`
- values shorter than four characters still return a generic `***`
- raw secret material must never appear in API responses, logs, events, or tests that model read APIs

## Capability models

### Ticketing

Cardinality:

- zero or one config per account

Provider keys:

- `none`
- `jira`
- `github`
- `linear`
- `tracklines`

Public fields by provider:

- `none`: none
- `jira`: `base_url`, `project_key`, `issue_type`
- `github`: `owner`, `repository`, `label`
- `linear`: `team_id`, `project_id`, `default_state_id`
- `tracklines`: `workspace_id`, `project_id`

Secret fields by provider:

- `none`: none
- `jira`: `email`, `api_token`
- `github`: `token`
- `linear`: `api_key`
- `tracklines`: `api_key`

### Notification

Cardinality:

- zero or many configs per account

Provider keys:

- `none`
- `slack`
- `teams`
- `resend`

Public fields by provider:

- `none`: none
- `slack`: `channel`
- `teams`: none
- `resend`: `from_email`, `audience`

Secret fields by provider:

- `none`: none
- `slack`: `webhook_url`
- `teams`: `webhook_url`
- `resend`: `api_key`

Common notification field:

- `minimum_severity`: replaces the current account-level `notify_min_level` setting and lives inside the notification config `fields`

### AI

Cardinality:

- zero or many configs per account

Account-level AI mode:

- `disabled`: no AI runs for the account
- `managed`: Daphne-managed AI runs when there are zero customer-managed configs
- `customer_managed`: one or more stored AI configs provide the available advisors

Provider keys:

- `none`
- `managed`
- `codex`
- `claude`
- `kimi`

Public fields by provider:

- `none`: none
- `managed`: `model`
- `codex`: `model`
- `claude`: `model`
- `kimi`: `model`

Secret fields by provider:

- `none`: none
- `managed`: none
- `codex`: `api_key`
- `claude`: `api_key`
- `kimi`: `api_key`

AI normalization rules:

- zero stored AI configs with account mode `managed` means "use Daphne-managed AI"
- zero stored AI configs with account mode `disabled` means "AI disabled"
- account mode `customer_managed` expects one or more stored AI configs; zero configs in that mode should be treated as incomplete or invalid configuration
- `provider = none` is not needed as a persisted AI config in the target model
- `provider = managed` is not needed as a persisted AI config in the target model
- `provider in {codex, claude, kimi}` represents a customer-managed advisor

## API contract

The dashboard and future integration work should rely on capability-specific collection endpoints.

Read:

- `GET /v1/accounts/{account_id}/ticketing/configs`
- `GET /v1/accounts/{account_id}/ticketing/configs/{id}`
- `GET /v1/accounts/{account_id}/notifications/configs`
- `GET /v1/accounts/{account_id}/notifications/configs/{id}`
- `GET /v1/accounts/{account_id}/ai/configs`
- `GET /v1/accounts/{account_id}/ai/configs/{id}`

Write:

- `POST /v1/accounts/{account_id}/ticketing/configs`
- `POST /v1/accounts/{account_id}/notifications/configs`
- `POST /v1/accounts/{account_id}/ai/configs`
- `PATCH /v1/accounts/{account_id}/ticketing/configs/{id}`
- `PATCH /v1/accounts/{account_id}/notifications/configs/{id}`
- `PATCH /v1/accounts/{account_id}/ai/configs/{id}`
- `DELETE /v1/accounts/{account_id}/ticketing/configs/{id}`
- `DELETE /v1/accounts/{account_id}/notifications/configs/{id}`
- `DELETE /v1/accounts/{account_id}/ai/configs/{id}`
- write endpoints accept partial updates for `provider`, `enabled`, `fields`, and `secrets`
- write endpoints require `ManageProviders`

Example patch:

```json
{
  "provider": "jira",
  "enabled": true,
  "fields": {
    "base_url": "https://acme.atlassian.net",
    "project_key": "OPS",
    "issue_type": "Bug"
  },
  "secrets": {
    "email": {
      "op": "set",
      "value": "ops-bot@acme.test"
    },
    "api_token": {
      "op": "set",
      "value": "jira-token"
    }
  }
}
```

Validation rules:

- switching `provider` replaces the allowed field and secret namespace for that config
- unknown fields or secrets are rejected
- `enabled = true` with missing required secrets yields `status = incomplete`
- provider-specific syntax validation happens on write where feasible, but remote credential verification is separate from persistence

Endpoint rules:

- ticketing endpoints only accept ticketing providers
- notification endpoints only accept notification providers
- AI endpoints only accept AI providers
- `GET /configs` returns the capability-scoped config collection
- `GET /configs/{id}` returns a single config by id
- `POST /configs` creates one config inside that capability collection
- `PATCH /configs/{id}` updates an existing capability config
- `DELETE /configs/{id}` removes one stored capability config
- ticketing `POST /configs` must reject creation when a ticketing config already exists for the account
- notifications and AI may contain multiple configs, so the service should support a future `is_default` or priority field without changing the route shape

## Persistence rules

Implementation should stop storing integration secrets directly on `accounts`.

Target storage split:

- `account_provider_configs`: account id, kind, provider, enabled, status, fields JSON, timestamps
- `account_provider_secrets`: config id, secret key, encrypted secret value, timestamps

Storage rules:

- `accounts` may keep compatibility columns temporarily during migration, but they become derived or deprecated fields
- `account_provider_configs` remains the canonical readable configuration source
- `account_provider_secrets` is the canonical secret source
- secret updates are atomic with config updates for a single patch request

## Migration guidance

Current account columns map into the target model like this:

- `ticket_provider` + `ticketing_api_key` -> `ticketing` config
- `notification_provider` + `notification_api_key` + `notify_min_level` -> `notification` config
- `ai_enabled` + `use_managed_ai` + `ai_api_key` -> `ai` config

Existing rows should migrate as:

- provider enums become `provider`
- boolean enablement becomes `enabled`
- non-secret compatibility fields move into `fields`
- current `*_api_key` values move into named secrets using provider defaults
- configs with provider `none` become `not_configured`

## Non-goals for this ticket

- remote provider connectivity checks
- key rotation workflows beyond `set` and `clear`
- per-provider OAuth flows
- exposing raw secrets anywhere in the UI or API
