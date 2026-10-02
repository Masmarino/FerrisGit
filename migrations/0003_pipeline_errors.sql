-- A pipeline file that cannot be parsed still produces a (failed, job-less) pipeline carrying the parser's message, so
-- whoever pushed can read it in the interface. And a job skipped because something it depends on did not succeed.
ALTER TABLE pipelines ADD COLUMN error text;

ALTER TABLE jobs DROP CONSTRAINT jobs_status_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_status_check
    CHECK (status = ANY (ARRAY['pending'::text, 'running'::text, 'success'::text, 'failed'::text, 'canceled'::text, 'skipped'::text]));
