-- Protected branches: a repository names the branches nobody pushes to directly, by name or with a `*` pattern. The
-- Git HTTP endpoint refuses a push that would move, force-push or delete one of them.
CREATE TABLE protected_branches (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id uuid NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    pattern text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (repository_id, pattern)
);
