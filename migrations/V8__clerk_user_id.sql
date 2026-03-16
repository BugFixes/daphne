-- Switch user identity from email to Clerk user ID.
ALTER TABLE users ADD COLUMN clerk_user_id TEXT;
CREATE UNIQUE INDEX idx_users_clerk_user_id ON users (clerk_user_id) WHERE clerk_user_id IS NOT NULL;

-- Email is no longer the primary identifier; keep it for display but allow NULL.
ALTER TABLE users ALTER COLUMN email DROP NOT NULL;
