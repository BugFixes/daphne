ALTER TABLE organizations
ADD COLUMN IF NOT EXISTS plan_tier TEXT NOT NULL DEFAULT 'single';

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS subprojects (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS environments (
    id TEXT PRIMARY KEY,
    subproject_id TEXT NOT NULL,
    account_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (subproject_id) REFERENCES subprojects(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_projects_organization_id ON projects (organization_id);
CREATE INDEX IF NOT EXISTS idx_subprojects_project_id ON subprojects (project_id);
CREATE INDEX IF NOT EXISTS idx_environments_subproject_id ON environments (subproject_id);

INSERT INTO projects (id, organization_id, name, created_at, updated_at)
SELECT
    SUBSTRING(md5(o.id || ':project') FROM 1 FOR 8) || '-' ||
    SUBSTRING(md5(o.id || ':project') FROM 9 FOR 4) || '-' ||
    SUBSTRING(md5(o.id || ':project') FROM 13 FOR 4) || '-' ||
    SUBSTRING(md5(o.id || ':project') FROM 17 FOR 4) || '-' ||
    SUBSTRING(md5(o.id || ':project') FROM 21 FOR 12),
    o.id,
    o.name,
    o.created_at,
    o.updated_at
FROM organizations o
WHERE NOT EXISTS (
    SELECT 1
    FROM projects p
    WHERE p.organization_id = o.id
);

INSERT INTO subprojects (id, project_id, name, created_at, updated_at)
SELECT
    SUBSTRING(md5(a.id || ':subproject') FROM 1 FOR 8) || '-' ||
    SUBSTRING(md5(a.id || ':subproject') FROM 9 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':subproject') FROM 13 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':subproject') FROM 17 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':subproject') FROM 21 FOR 12),
    p.id,
    a.name,
    o.created_at,
    o.updated_at
FROM accounts a
JOIN organizations o ON o.id = a.organization_id
JOIN projects p ON p.organization_id = o.id
WHERE NOT EXISTS (
    SELECT 1
    FROM subprojects s
    WHERE s.id = (
        SUBSTRING(md5(a.id || ':subproject') FROM 1 FOR 8) || '-' ||
        SUBSTRING(md5(a.id || ':subproject') FROM 9 FOR 4) || '-' ||
        SUBSTRING(md5(a.id || ':subproject') FROM 13 FOR 4) || '-' ||
        SUBSTRING(md5(a.id || ':subproject') FROM 17 FOR 4) || '-' ||
        SUBSTRING(md5(a.id || ':subproject') FROM 21 FOR 12)
    )
);

INSERT INTO environments (id, subproject_id, account_id, name, created_at, updated_at)
SELECT
    SUBSTRING(md5(a.id || ':environment') FROM 1 FOR 8) || '-' ||
    SUBSTRING(md5(a.id || ':environment') FROM 9 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':environment') FROM 13 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':environment') FROM 17 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':environment') FROM 21 FOR 12),
    s.id,
    a.id,
    'default',
    o.created_at,
    o.updated_at
FROM accounts a
JOIN organizations o ON o.id = a.organization_id
JOIN subprojects s ON s.id = (
    SUBSTRING(md5(a.id || ':subproject') FROM 1 FOR 8) || '-' ||
    SUBSTRING(md5(a.id || ':subproject') FROM 9 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':subproject') FROM 13 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':subproject') FROM 17 FOR 4) || '-' ||
    SUBSTRING(md5(a.id || ':subproject') FROM 21 FOR 12)
)
WHERE NOT EXISTS (
    SELECT 1
    FROM environments e
    WHERE e.account_id = a.id
);
