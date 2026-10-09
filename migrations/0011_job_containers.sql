-- Docker in jobs: a Docker daemon (Docker-in-Docker, privileged) beside every job, with either engine. Off by default:
-- a privileged container can reach the machine or the node it runs on.
ALTER TABLE system_settings ADD COLUMN docker_in_jobs boolean NOT NULL DEFAULT false;

-- What a Kubernetes job clones its repository with: read access to that repository only, and only while the job has not
-- finished. Only the SHA-256 of the token is kept.
CREATE TABLE job_clone_tokens (
    job_id uuid PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    token_hash text NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now()
);
