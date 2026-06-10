-- Real authentication: a user has an email + argon2 password hash and belongs
-- to an engagement (the tenant key carried in the JWT). Replaces the dev-mode
-- "type any engagement name" login.
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    engagement_id UUID NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Case-insensitive unique email (lookups use LOWER(email)).
CREATE UNIQUE INDEX users_email_lower_uq ON users (LOWER(email));
