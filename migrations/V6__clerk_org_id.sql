ALTER TABLE organizations ADD COLUMN clerk_org_id TEXT;
CREATE UNIQUE INDEX idx_organizations_clerk_org_id ON organizations (clerk_org_id) WHERE clerk_org_id IS NOT NULL;
