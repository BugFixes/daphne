# Database Migrations

`bugfix.es` uses `refinery` for schema migrations.

Conventions:

- keep migrations in this directory
- use SQL migrations
- name files like `V1__create_accounts.sql`
- keep migrations forward-only

Operational model:

- the application embeds migrations at compile time
- pending migrations run during startup before the API begins serving traffic
- the existing schema bootstrap code remains in place until the initial migration conversion lands
