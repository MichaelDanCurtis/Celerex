-- Per-install webhook token: fixes cross-tenant misattribution + adds the missing inbound secret.
-- Replaces the old `plugin_id LIMIT 1` inbound lookup (which could route a webhook to ANY
-- engagement that installed the same plugin) with a per-install secret routing token.
ALTER TABLE plugin_installs ADD COLUMN webhook_token TEXT;

-- Backfill existing rows with a random token. gen_random_bytes requires pgcrypto;
-- gen_random_uuid() is already used in 0003, so a UUID-derived token is a safe, dependency-free
-- fallback. Two concatenated dashless UUIDs => a 64-char random token.
UPDATE plugin_installs
   SET webhook_token = replace(gen_random_uuid()::text, '-', '') || replace(gen_random_uuid()::text, '-', '')
 WHERE webhook_token IS NULL;

-- Belt-and-suspenders: a DEFAULT so any future direct INSERT (tests, manual SQL) also gets a
-- valid token even though the app insert sets it explicitly.
ALTER TABLE plugin_installs
    ALTER COLUMN webhook_token
    SET DEFAULT (replace(gen_random_uuid()::text, '-', '') || replace(gen_random_uuid()::text, '-', ''));

ALTER TABLE plugin_installs ALTER COLUMN webhook_token SET NOT NULL;
ALTER TABLE plugin_installs ADD CONSTRAINT plugin_installs_webhook_token_key UNIQUE (webhook_token);
