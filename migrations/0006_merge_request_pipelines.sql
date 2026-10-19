-- Merge request pipelines: a pipeline can run for a merge request, on the merge of its source branch into its target
-- branch. `commit_sha` is then that merge commit, kept under refs/merge-requests/<id>/merge, and `source_sha` the source
-- tip it was made from. A repository can require the latest one to succeed before a merge.
ALTER TABLE pipelines
    ADD COLUMN merge_request_id uuid REFERENCES merge_requests (id) ON DELETE CASCADE,
    ADD COLUMN source_sha text,
    ADD CONSTRAINT pipelines_merge_request_source CHECK ((merge_request_id IS NULL) = (source_sha IS NULL));

CREATE INDEX pipelines_merge_request_idx ON pipelines (merge_request_id, created_at DESC) WHERE merge_request_id IS NOT NULL;

ALTER TABLE repository_settings ADD COLUMN pipeline_must_succeed boolean NOT NULL DEFAULT false;
