ALTER TABLE accounts ADD COLUMN ticketing_api_key TEXT;
ALTER TABLE accounts ADD COLUMN notification_api_key TEXT;
ALTER TABLE accounts ADD COLUMN ai_enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE accounts ADD COLUMN use_managed_ai INTEGER NOT NULL DEFAULT 1;
ALTER TABLE accounts ADD COLUMN ai_api_key TEXT;
