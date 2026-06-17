ALTER TABLE users ADD COLUMN workos_user_id TEXT;
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;
CREATE UNIQUE INDEX users_workos_user_id_uq ON users (workos_user_id) WHERE workos_user_id IS NOT NULL;
