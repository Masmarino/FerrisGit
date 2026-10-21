-- Renaming or transferring a repository leaves its old path behind as a redirect, so that old clone URLs and links keep
-- working. A redirect names a repository by id: it follows the repository through later moves, and goes away with it.
-- A personal path is an owner and a name, a group path a group and a name, like the repositories themselves.
CREATE TABLE repository_redirects (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id uuid NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    owner_id uuid REFERENCES users (id) ON DELETE CASCADE,
    group_id uuid REFERENCES groups (id) ON DELETE CASCADE,
    name text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT repository_redirects_one_namespace CHECK ((owner_id IS NULL) <> (group_id IS NULL))
);

CREATE UNIQUE INDEX repository_redirects_owner_name_unique ON repository_redirects (owner_id, name) WHERE owner_id IS NOT NULL;
CREATE UNIQUE INDEX repository_redirects_group_name_unique ON repository_redirects (group_id, name) WHERE group_id IS NOT NULL;
CREATE INDEX repository_redirects_repository_idx ON repository_redirects (repository_id);
