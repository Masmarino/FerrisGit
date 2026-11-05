-- API tokens: what a program uses in place of a session JWT. Distinct from `api_tokens`, which are the `fg_` Git tokens
-- (see docs/api/authentification.md): these are scoped and always expire. Only the SHA-256 of the token is kept.
CREATE TABLE access_tokens (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name text NOT NULL,
    token_hash text NOT NULL UNIQUE,
    scopes text[] NOT NULL,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz
);

CREATE INDEX access_tokens_user_id_idx ON access_tokens (user_id);

-- A sign-in from a terminal, while the user confirms it in the web interface. The token doesn't exist yet at that point:
-- it is created, and shown once, when the terminal collects it, so its plaintext is never stored. Only the SHA-256 of
-- the device code is kept, since it is what collects the token. The user code is short and typed by hand, but it only
-- names a request: approving one takes a session and a second factor.
CREATE TABLE device_authorizations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    device_code_hash text NOT NULL UNIQUE,
    user_code text NOT NULL UNIQUE,
    client_name text NOT NULL,
    scopes text[] NOT NULL,
    token_lifetime_days integer NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'denied')),
    user_id uuid REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    last_polled_at timestamptz
);

CREATE INDEX device_authorizations_expires_at_idx ON device_authorizations (expires_at);
