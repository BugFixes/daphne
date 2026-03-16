-- Set all existing organizations to the dev Clerk org for local development.
-- This will be wiped before going live.
UPDATE organizations SET clerk_org_id = 'org_3Ax63TjpWPLZlHDNlqx9EndGIRr' WHERE clerk_org_id IS NULL;
