# Changelog

Every notable change to FerrisGit. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
versions follow [SemVer](https://semver.org/). Each section is the text of the GitHub release of the same number: see
"Publishing a version" below.

## [0.1.1] - 2026-10-02

### Added

- Documentation at `/docs`, open to everyone and independent of the public pages switch: a user guide, the CI/CD
  reference, administration and self-hosting, and the REST API reference, in French, with search.
- A Helm chart in `helm/ferrisgit` (server, PostgreSQL, execution engine `Role`, `NetworkPolicy`, Traefik `Ingress` with
  a `letsencrypt-http` certificate by default, or cert-manager), which replaces the former `k8s/` manifests. The CI
  lints it on every push and deploys each version tag to Kubernetes with `helm upgrade --install --atomic`, pinned to the
  digest of the image it just pushed.
- `GET /healthz` (the process runs) and `GET /readyz` (the database answers within 2 seconds, `503` otherwise) next to
  `GET /health`, for the chart's startup, liveness and readiness probes.
- A repository's description and visibility can be edited (`PATCH /api/repositories/by-id/{id}`), and a root group can
  be created from the interface.
- The runners registration token, the concurrent jobs limit and the log retention have fields in the admin settings.
  The retention is now applied: a daily sweep clears the logs of finished jobs older than the limit.
- A pushed pipeline file that is present but invalid creates a failed pipeline carrying the parser's message, shown in
  the interface, instead of a line in the server log. A `needs` cycle is refused by the parser.
- The server shuts down gracefully on `SIGTERM`.

### Changed

- Stages are barriers. A job without `needs` waits for every job of the earlier stages, and the jobs that can no longer
  start after a failure are `skipped`, so a pipeline ends instead of waiting forever. A pipeline is `running` as soon as
  a job has started.
- Anonymous Git reads of a public repository, and of its wiki, follow the "Pages publiques" switch: switched off, they
  get the same answer as a private repository.
- Creating and commenting on an issue needs the Contributor role. Assignees and collaborator management take the role
  inherited from a group into account.
- The Merge button only shows for the people the server lets merge, and the token screens say a token is for Git over
  HTTPS, not for the REST API.

### Migrations

Two migrations apply at startup, with no manual step: `0003_pipeline_errors` (the error of an invalid pipeline file,
and the `skipped` job status) and `0004_job_log_retention` (which job logs the retention sweep has cleared).

### Upgrading from 0.1.0

- A pipeline whose stages relied on running in parallel now runs stage after stage: add `needs` between jobs, or put
  the jobs in the same stage, if that is not what you want.
- To deploy with the Helm chart instead of the `k8s/` manifests, follow "Déploiement sur Kubernetes" in the
  documentation: the chart needs an image that serves `/healthz` and `/readyz`, so `0.1.1` or later.

## [0.1.0] - 2026-10-01

First release.

### Added

**Hosting and collaboration**

- Git over HTTP (smart protocol): clone, fetch and push, authenticated with your username and a personal API token.
- Repositories owned by a user or nested inside hierarchical groups, public or private, with per-repository and
  per-group roles (Reader, Contributor, Maintainer).
- Merge requests with threaded inline comments, code suggestions that can be applied from the interface, approvals and
  change requests, conflict detection and a timeline.
- Issues with labels, milestones, assignees and a kanban board.
- Wikis stored as a Git repository of their own, releases with attached assets, and repository stars.
- Webhooks for merge request, collaborator, pipeline and issue events, with secrets encrypted at rest.
- In-app notifications, search, a personal dashboard and repository language statistics.
- Public pages: visitors without an account can browse public repositories read-only (catalogue, README, files,
  commits, releases and release assets). An administrator switches them off, and search engine indexing is a separate
  switch that is off by default (`X-Robots-Tag` and `/robots.txt`).

**CI/CD**

- Pipelines described in a `.ferrisgit-ci.yml` file at the root of the repository.
- Two execution engines: Docker runners (the `ferrisgit-runner` binary polling the server) or Kubernetes, where each
  job runs as a Pod.
- Per-repository CI variables, encrypted at rest, and per-job caches.

**Accounts and administration**

- Mandatory multi-factor authentication for every account: TOTP, passkeys (WebAuthn) and single-use backup codes.
- Free registration behind an administrator switch, or accounts created by e-mail invitation.
- Administrator tooling: user list and detail pages, invitations, password resets sent by e-mail, MFA resets,
  promotion and demotion of administrators, and deletion of a user with anonymisation of their contributions.
- Instance settings edited in the interface (SMTP, registration, public pages, execution engine, retention), a metrics
  dashboard and a health page.
- Personal API tokens.

**Security and operations**

- Passwords hashed with Argon2; secrets at rest (CI variables, webhook secrets, the SMTP password, TOTP secrets)
  encrypted with AES-GCM.
- A Content-Security-Policy on every response, per-IP rate limiting of the sign-in, registration, activation,
  password-reset, MFA and public endpoints, and audit events for administrator actions.
- A Docker image (`masmarino/ferrisgit`), Kubernetes manifests in `k8s/`, and a CI/CD pipeline that scans the sources
  and the image before publishing.

### Migrations

Two migrations apply at startup, with no manual step: `0001_init` (the schema) and `0002_public_pages` (the two public
pages switches in `system_settings`). See "Upgrades" in the README.

## Publishing a version

1. Set `version` in `[workspace.package]` of `Cargo.toml` to the new number and refresh `Cargo.lock`.
2. Add a `## [X.Y.Z] - YYYY-MM-DD` section at the top of this file, in the same format as above.
3. Merge to `main`, then push the tag `vX.Y.Z` on that commit.

The CI/CD workflow builds and publishes the image, then creates the GitHub release `vX.Y.Z` with the text of the matching
section. If the section is missing, only the release job fails (the image is not affected): add it, then rerun the job.

[0.1.1]: https://github.com/Masmarino/FerrisGit/releases/tag/v0.1.1
[0.1.0]: https://github.com/Masmarino/FerrisGit/releases/tag/v0.1.0
