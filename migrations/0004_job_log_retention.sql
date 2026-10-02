-- Log retention: the sweep empties the logs of old finished jobs. This remembers which ones, so the interface can say
-- "log deleted" instead of showing a log that was simply empty. A side table keeps `jobs` untouched.
CREATE TABLE job_log_purges (
    job_id uuid PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    purged_at timestamptz NOT NULL DEFAULT now()
);

-- What the sweep scans: finished jobs that still hold some log text. A purged job leaves the index.
CREATE INDEX jobs_logs_to_purge_idx ON jobs (finished_at) WHERE logs <> '';
