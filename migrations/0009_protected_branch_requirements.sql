-- What a merge into a protected branch needs: a number of approvals, on top of the repository's own setting, and named
-- checks that must have succeeded on the merge request's current source commit. The merge request pipeline is the
-- check `pipeline`, each of its jobs the check `pipeline/<job>`; later sources of checks plug in by name.
ALTER TABLE protected_branches
    ADD COLUMN required_approvals integer NOT NULL DEFAULT 0 CHECK (required_approvals >= 0),
    ADD COLUMN required_checks text[] NOT NULL DEFAULT '{}';
