# Changelog

Every notable change to FerrisGit. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
versions follow [SemVer](https://semver.org/). Each section is the text of the GitHub release of the same number: see
"Publishing a version" below.

## [Unreleased]

### Added

- A visual pipeline editor: from a repository's pipeline list, or the quick search, "Éditer la pipeline" opens the
  stages as columns and the jobs as cards to drag between them, or the YAML itself, the two views describing the same
  file. Contributors and above are offered it. The server's own parser reads, writes and checks it
  (`POST /api/pipeline-definitions/parse` and `/render`), reports every mistake at once instead of the first, and the
  editor says what a rewrite would lose (comments, keys FerrisGit does not know). Typed YAML is saved as it is, comments
  included.
- Working in the editor is safe to try: every change to the cards can be undone and redone, from the toolbar or with ⌘Z
  and ⇧⌘Z (Ctrl+Z and Ctrl+Y), keystrokes in one field making one step. Leaving the editor, or closing the tab, with
  changes that were not proposed asks first; "Revenir au fichier du dépôt" asks too, and is only offered once something
  changed. A job's ⋮ menu edits, duplicates, moves or deletes it. A card shows the job's main (last) command, and pointing
  at one marks the jobs it waits for and those that wait for it.
- The editor proposes a pipeline made for the repository: it reads the default branch's projects (Rust, Node and its
  frameworks, Go, Python, up to three folders down) and lays out check, test and build jobs for each, with the versions,
  package manager and scripts the project names (`GET /api/repositories/{id}/pipeline-definition/profile`), saying which
  files it read. A Dockerfile or a Helm chart is pointed at with the tile that would ship it. On a pipeline that has
  jobs, "Ajouter un job" offers first the predicted jobs it lacks.
- The cards view is meant to build a whole pipeline without writing YAML: starting templates (Rust, Node or Angular, Go,
  an application published as an image), a catalogue of ready-made job tiles by purpose (compile, test, check the code,
  package, deploy and notify, or an empty job), commands as an editable list, common images and caches to pick, and a
  help bubble on every notion, opened by a click so it also works on a phone.
- Deploying from the editor: tiles with a short form for building and publishing a Docker image (against a remote
  daemon, since runners give jobs no Docker of their own), deploying to a virtual machine over SSH (running commands, or
  copying a folder with rsync, with the server's fingerprint checked by default), and deploying to Kubernetes
  (`kubectl apply`, changing the image of a Deployment or restarting it and waiting for it, Helm). The form shows the
  commands as the answers change, checks every answer that ends up in a command, and lists the secrets the job reads,
  saying which the repository has. Two templates chain tests, image and deployment. Tiles that read secrets are greyed
  out when the instance runs jobs with Kubernetes, which does not pass them.
- Variables and secrets in the editor: an "Insert a variable" menu that writes `$NAME` at the cursor, a warning on a job
  variable that looks like a secret with a button that saves it as an encrypted secret of the repository and removes it
  from the file, the secrets the jobs read and the repository lacks, which jobs read each secret, and a notice when the
  instance runs Kubernetes, which does not pass repository secrets to jobs. Creating the secrets is for maintainers.
  The secret form of the repository settings refuses a name an environment variable cannot have.
- "Proposer la modification" saves the file the way any change is made: on a new `pipeline-editor/…` branch, in a commit
  by the author, with a merge request into the default branch, which is never written directly
  (`GET /api/repositories/{id}/pipeline-definition` and `POST …/pipeline-definition/proposal`). The file is read from,
  and written to, the path the repository's settings name. A file that changed on the default branch since the editor
  was opened is refused with `409` instead of overwritten.

## [0.1.5] - 2026-10-07

### Added

- The application can be installed from Safari's "Ajouter au Dock", a phone's home screen or Chrome's "Installer", and
  then opens in its own window. iOS shows the crab and the name FerrisGit instead of the first letter of the page title;
  Android gets an icon it can crop to its own shape. The bars take the graphite of the shell's header in both themes.
- FerrisGit is licensed under the Apache License 2.0 (`LICENSE`).

### Fixed

- The API documentation describes `POST /api/auth/logout-all` and the `createdAt` field of `GET /api/auth/me`.

## [0.1.4] - 2026-10-06

### Added

- A quick search, opened with ⌘K (Ctrl K on Windows and Linux), `/` or the search button in the header. Before anything
  is typed it offers the repositories opened last in this browser, the current repository's pages and, to those who
  can write, its "Nouveau ticket", "Nouvelle demande de fusion" and "Nouvelle page de wiki"; then the menu's pages and
  the global actions ("Nouveau dépôt", "Nouveau groupe", "Déconnexion"). Typing filters them at once, accents ignored,
  then adds the repositories, tickets and merge requests the server finds, and a link to the full results page. The
  administration pages, and the users found, are only offered to administrators.
- The account page says since when the account exists, and ends on a "Sessions" card: "Se déconnecter partout" ends
  every session of the account, this one included (`POST /auth/logout-all`, recorded as a security event); the Git
  access tokens stay.

### Changed

- The "Exécution" settings only show the chosen engine's: the runners' for Docker, the cluster's for Kubernetes.
  "Exécution" and "Sécurité" now keep their changes until "Enregistrer", which sends them in one request and confirms
  with a notification, or says it failed and keeps them; "Annuler les modifications" puts everything back as saved.
- The administration menu is named "Administration" and opens on its "Tableau de bord": Tableau de bord, Utilisateurs,
  Santé, then Réglages.
- The interface takes the Ferris family's design from Gabarit 2.0: the palette, IBM Plex (now served from the package),
  the graphite shell and the sign-in page. The local theme, its map onto Gabarit's tokens and the shell's overrides are
  gone; the shell's breadcrumb, the repository switcher and the commit graph behind the sign-in panel stay. A few
  shades move slightly so every text reaches 7:1 on the page and on a panel: lighter secondary text in the dark theme,
  a slightly darker danger button.
- An administrator invites someone by e-mail address only: the invitee chooses their username, along with their
  password, on the page the invitation mail links to (`/invitation`). Until then the user list shows them by their
  address, and the API sends `username: null`. A registered account's link still goes to `/activate`, which only asks
  for the password. The interface uses Gabarit 2.2.1.

## [0.1.3] - 2026-10-05

### Added

- A "Ressources" documentation page with the memory, size and start-up figures of an empty instance, and how to
  measure them. An instance administrator cannot read other users' private repositories, through the API or through
  Git: a test now guards the rule.

### Changed

- The public website is rebuilt as a reference document: one page per subject (product, CI/CD, installation, security,
  roadmap), formal copy in the five languages, the measured footprint of an instance on the home page and the security
  headers of a real response on the security page. A page whose code cannot be fetched now loads again once instead of
  staying blank.

## [0.1.2] - 2026-10-03

### Added

- Accounts nobody activated are cleaned up, whether they came from an invitation or from a registration. When the link
  of the last mail has expired, a reminder with a fresh link and the deletion date is sent, which makes one a day, and
  the account is deleted 7 days after it was created, freeing its username and address. Activating is the only thing
  that stops it, and a resent invitation does not extend the delay. The server checks every hour; a reminder that
  cannot be sent leaves the stored link untouched and is retried. Without mail configured there are no reminders but
  the deletion still applies, and accounts already waiting when the server starts are covered too: those created more
  than 7 days ago are deleted at the first check.

### Changed

- Free registration now confirms the e-mail address. `POST /api/auth/register` takes a username and an address only,
  creates the account inactive (its password is unusable) and mails a link, valid for 24 hours and single use, to choose
  a password through `POST /api/auth/activate`. It answers `204` with no session, so nobody can register with an address
  they do not read. The sign-up page asks for the two fields and shows "Consultez votre boîte mail".
- Registering again with the name and address of an account nobody has activated sends a new link and invalidates the
  previous one, instead of answering `409`, so that a lost message does not lock the name.
- Registration needs mail to be configured. Without SMTP, `GET /api/auth/config` reports `registrationEnabled: false`
  even when the switch is on, and `POST /api/auth/register` answers `503`. A confirmation message that cannot be sent
  also answers `503`, without the SMTP error.

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

[0.1.5]: https://github.com/Masmarino/FerrisGit/releases/tag/v0.1.5
[0.1.4]: https://github.com/Masmarino/FerrisGit/releases/tag/v0.1.4
[0.1.3]: https://github.com/Masmarino/FerrisGit/releases/tag/v0.1.3
[0.1.2]: https://github.com/Masmarino/FerrisGit/releases/tag/v0.1.2
[0.1.1]: https://github.com/Masmarino/FerrisGit/releases/tag/v0.1.1
[0.1.0]: https://github.com/Masmarino/FerrisGit/releases/tag/v0.1.0
