# bugfix.es Policies

This document contains the current authored policy rules for `bugfix.es`.

These are the rules to create in `policy2`. The runtime copies also live in the [`policies/`](./policies) directory and are used by the `policy2` client in the service.

Each policy now has a matching JSON Schema file so the expected `decision` payload is explicit:

- [`policies/create_ticket.schema.json`](./policies/create_ticket.schema.json)
- [`policies/escalate_repeat.schema.json`](./policies/escalate_repeat.schema.json)
- [`policies/send_notification.schema.json`](./policies/send_notification.schema.json)
- [`policies/use_ai.schema.json`](./policies/use_ai.schema.json)

## Ticket Creation

```policy
# bugfix.es ticket creation policy

A **ticket** is created
  if the **stack** is new
  and the **account** has ticketing enabled
  and the **ticketing** configuration chose a supported provider
  and the **ticketing** configuration is enabled
  and the **account** has a ticketing api key.

A **stack** is new
  if the __hash_exists__ of the **stack** is equal to false.

An **account** has ticketing enabled
  if the __ticketing_enabled__ of the **account** is equal to true.

The **ticketing** configuration chose a supported provider
  if the __provider__ of the **ticketing** configuration matches regex "^(jira|github|linear|tracklines)$".

The **ticketing** configuration is enabled
  if the __enabled__ of the **ticketing** configuration is equal to true.

An **account** has a ticketing api key
  if the __api_key__ of the **account** matches regex "^.+$".
```

Payload shape:

- `stack.hash_exists`
- `account.ticketing_enabled`
- `account.api_key`
- `ticketing.provider`
- `ticketing.enabled`

## Repeat Escalation

```policy
# bugfix.es repeat escalation policy

A **bug** escalates a repeated ticket
  if the **bug** already has a ticket
  and the **bug** exceeded the rapid repeat threshold
  and the **ticket** can still increase priority.

A **bug** already has a ticket
  if the __has_ticket__ of the **bug** is equal to true.

A **bug** exceeded the rapid repeat threshold
  if the __recent_count__ of the **bug** is greater than or equal to the __rapid_occurrence_threshold__ of the **bug**.

The **ticket** can still increase priority
  if the __next_priority_rank__ of the **ticket** is greater than the __current_priority_rank__ of the **ticket**.
```

Payload shape:

- `bug.has_ticket`
- `bug.recent_count`
- `bug.rapid_occurrence_threshold`
- `ticket.current_priority_rank`
- `ticket.next_priority_rank`

## Notification Sending

```policy
# bugfix.es notification policy

A **notification** is sent
  if the **notification** configuration chose a supported provider
  and the **notification** configuration is enabled
  and the **event** meets the account threshold
  and the **ticket** has a notifiable action
  and the **account** has a notification api key.

The **notification** configuration chose a supported provider
  if the __provider__ of the **notification** configuration matches regex "^(slack|teams|resend)$".

The **notification** configuration is enabled
  if the __enabled__ of the **notification** configuration is equal to true.

The **event** meets the account threshold
  if the __rank__ of the **event** is greater than or equal to the __notify_min_rank__ of the **account**.

The **ticket** has a notifiable action
  if the __action__ of the **ticket** matches regex "^(created|escalated)$".

An **account** has a notification api key
  if the __api_key__ of the **account** matches regex "^.+$".
```

Payload shape:

- `event.rank`
- `account.notify_min_rank`
- `account.api_key`
- `notification.provider`
- `notification.enabled`
- `ticket.action`

## AI Usage

```policy
# bugfix.es ai usage policy

An **ai** recommendation is requested
  if the **account** has ai enabled
  and the **ai** configuration chose a supported advisor
  and the **ai** configuration is enabled
  and the **account** is ready to authenticate ai usage.

An **account** has ai enabled
  if the __enabled__ of the **account** is equal to true.

The **ai** configuration chose a supported advisor
  if the __advisor__ of the **ai** configuration matches regex "^(codex|claude|kimi)$".

The **ai** configuration is enabled
  if the __enabled__ of the **ai** configuration is equal to true.

An **account** is ready to authenticate ai usage
  if the __use_managed__ of the **account** is equal to true
  or the __api_key__ of the **account** matches regex "^.+$".
```

Payload shape:

- `account.enabled`
- `account.use_managed`
- `account.api_key`
- `ai.advisor`
- `ai.enabled`

## Notes

- These policies are boolean by design because the current `policy2` usage in this service is decision-gating, not value selection.
- The service sends raw state into policy, not precomputed outcomes. That includes stack facts, account settings, selected provider or advisor, feature-flag state, and the configured API key where relevant.
- Priority selection, message generation, and AI recommendation text still remain in Rust.
- `BUGFIXES_POLICY_PROVIDER=policy2` makes `policy2` the authoritative decision-maker for these booleans.
- `BUGFIXES_POLICY_PROVIDER=local` exists for local development and explicit local evaluation only.
