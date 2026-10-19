-- Merge methods: a merge request is merged with a merge commit, squashed into one commit, or rebased onto its target.
-- Each repository picks the one offered first.
ALTER TABLE repository_settings
    ADD COLUMN default_merge_method text NOT NULL DEFAULT 'merge'
        CHECK (default_merge_method IN ('merge', 'squash', 'rebase'));
