# FerrisGit

[![CI](https://github.com/Masmarino/FerrisGit/actions/workflows/ci-cd.yml/badge.svg)](https://github.com/Masmarino/FerrisGit/actions/workflows/ci-cd.yml)

FerrisGit is a self-hosted Git platform written in Rust. It hosts repositories over HTTP and adds the
collaboration tooling around them: merge requests with inline review, issues, wikis, releases, webhooks, and a
built-in CI/CD engine. The backend is a single Rust binary that also serves the Angular web application.

FerrisGit is developed alongside [ArtiFerris](https://github.com/Masmarino/ArtiFerris), a self-hosted artifact
registry (npm and Docker/OCI). Connecting the two is one of the [long-term goals](#roadmap).

> **Status.** FerrisGit is under active development. The [Roadmap](#roadmap) lists what is planned but does not exist
> yet, in particular code-quality scanning and security audits.

- [Features](#features)
- [Architecture](#architecture)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [Using FerrisGit](#using-ferrisgit)
- [CI/CD](#cicd)
- [Security](#security)
- [Operations](#operations)
- [Deploying to Kubernetes](#deploying-to-kubernetes)
- [Development](#development)
- [Repository layout](#repository-layout)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)

## Features

**Hosting and collaboration**

- Git over HTTP (smart protocol): clone, fetch and push, authenticated with your username and an API token.
- Repositories owned by a user or nested inside hierarchical **groups**, public or private.
- Per-repository and per-group roles: **Reader**, **Contributor** and **Maintainer**.
- **Merge requests** with threaded inline comments, code suggestions that can be applied from the interface,
  approvals and change requests, conflict detection, and a timeline of everything that happened.
- **Issues** with labels, milestones, assignees and a kanban board.
- **Wikis** stored as a Git repository of their own, **releases** with attached assets, and repository stars.
- **Webhooks** for merge request, collaborator, pipeline and issue events, with secrets encrypted at rest.
- In-app **notifications**, **search**, a personal dashboard, and repository language statistics.
- **Public pages**: visitors without an account can browse public repositories (catalogue, README, files, commits and
  releases) and download release assets, read-only.

**CI/CD**

- Pipelines described in a `.ferrisgit-ci.yml` file at the root of the repository.
- Two execution engines: **Docker runners** (a small `ferrisgit-runner` binary polling the server) or
  **Kubernetes**, where each job runs as a Pod.
- Per-repository CI variables, encrypted at rest, and per-job caches.

**Accounts and administration**

- Mandatory multi-factor authentication for every account: TOTP authenticator apps, **passkeys** (WebAuthn) and
  single-use backup codes.
- Free registration behind an administrator switch, or accounts created by e-mail invitation.
- Administrator tooling: user list and detail pages, invitations, password resets sent by e-mail, MFA resets,
  promotion and demotion of administrators, and deletion of a user with anonymisation of their contributions.
- Instance settings edited in the interface (SMTP, registration, execution engine, retention), plus a dashboard
  with usage metrics and a health page.
- Personal API tokens.

The web interface is in French.

## Architecture

FerrisGit follows a hexagonal (ports and adapters) layout, split across five Cargo crates:

| Crate | Role |
|---|---|
| `ferrisgit-domain` | Entities, value objects and the *ports* (traits) the rest of the system depends on. No I/O. |
| `ferrisgit-application` | Use cases. Depends on the domain only and is tested against in-memory fakes. |
| `ferrisgit-infrastructure` | Adapters: PostgreSQL stores (`sqlx`), Git access (`gix` and the `git` binary), SMTP (`lettre`), Kubernetes (`kube`), password hashing (Argon2), encryption (AES-GCM), JWT. |
| `ferrisgit-api` | The HTTP server (`axum`): REST API under `/api`, the Git smart-HTTP endpoints, and the static Angular bundle. Produces the `ferrisgit-api` binary. |
| `ferrisgit-runner` | The stand-alone Docker job runner. |

| Layer | Technology |
|---|---|
| Backend | Rust (edition 2024), Axum, SQLx, gitoxide |
| Database | PostgreSQL 18 |
| Frontend | Angular 22 (standalone components and signals), the [Gabarit](https://www.npmjs.com/package/@masmarino/gabarit) component library |
| Tests | `cargo test` with per-test throwaway databases; Vitest and Storybook for the frontend |

Repository contents live on disk under `STORAGE_ROOT`; everything else lives in PostgreSQL. Database migrations are
embedded in the binary and applied automatically at startup.

## Quick start

The fastest way to try FerrisGit is the Docker Compose stack, which builds the application image and starts
PostgreSQL next to it. You need Docker with Compose.

```bash
git clone https://github.com/Masmarino/FerrisGit.git
cd FerrisGit
cp .env.example .env
```

Edit `.env` and set real values for at least `POSTGRES_PASSWORD`, `JWT_SECRET` (32 characters or more),
`SETTINGS_ENCRYPTION_KEY` (**exactly** 32 characters) and the bootstrap administrator password. See
[Configuration](#configuration) for what each variable does, then start the stack:

```bash
docker compose up --build
```

FerrisGit is now served on <http://localhost:8080>. Sign in as the bootstrap administrator
(`FERRISGIT_BOOTSTRAP_ADMIN_USERNAME` / `FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD`). Multi-factor authentication is
mandatory, so the first sign-in walks you through enrolling an authenticator app or a passkey and shows your backup
codes.

The bootstrap account is only created when the `users` table is empty, so the variables can safely stay set across
restarts.

## Configuration

The server is configured through environment variables. Missing required variables, or values that are invalid, stop
the server at startup with an explicit message.

| Variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | yes | | PostgreSQL connection string. |
| `JWT_SECRET` | yes | | Signing key for session tokens. At least 32 characters. |
| `SETTINGS_ENCRYPTION_KEY` | yes | | Exactly 32 characters. Encrypts CI variables, webhook secrets, the SMTP password and TOTP secrets at rest. See the warning below. |
| `PUBLIC_URL` | yes | | The address users reach the application at: scheme and host (and port), no path. Activation and password-reset links and WebAuthn origins are built from it. `http://` is only accepted for `localhost`. |
| `TRUSTED_PROXY_CIDRS` | no | empty | Comma-separated CIDRs of your reverse proxy or ingress. Rate limits are per client IP; behind a proxy, list it here so `X-Forwarded-For` is honoured. Leave empty when the server is reached directly. List the proxy only, never a network untrusted clients can connect from. |
| `STORAGE_ROOT` | no | `./data` | Directory holding the Git repositories, wikis and release assets. |
| `BIND_ADDR` | no | `0.0.0.0:8080` | Listen address. |
| `STATIC_DIR` | no | `./static` | Directory of the built Angular application (set to `/app/static` in the image). |
| `FERRISGIT_BOOTSTRAP_ADMIN_USERNAME` | no | | Together with the password below, creates the first administrator when the user table is empty. Without them there is no way to create the first account. |
| `FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD` | no | | At least 8 characters, otherwise the bootstrap is skipped (and logged). |
| `RUST_LOG` | no | | Log filter, for example `info` or `ferrisgit_api=debug`. |

The Compose file additionally reads `POSTGRES_PASSWORD`, which is shared by the database and the connection string.

> **`SETTINGS_ENCRYPTION_KEY` cannot be rotated in place.** Every stored secret is encrypted with it and there is no
> re-encryption path. Changing it makes existing CI variables, webhook secrets, the SMTP password and TOTP secrets
> undecryptable: they have to be re-entered, and users whose TOTP secret can no longer be read have to have their MFA
> reset. Restoring a database dump onto an instance with a different key has the same effect.

Other settings are managed at runtime by an administrator, under **Réglages admin**:

- **E-mail (SMTP)**: server, credentials and sender. Needed for invitations, password resets and notifications.
  When a message cannot be sent, the administrator is given the link to pass on manually.
- **Registration**: free sign-up is off by default; administrators can always invite users.
- **Execution engine**: Docker runners or Kubernetes (see [CI/CD](#cicd)), and job retention.

## Using FerrisGit

### Cloning and pushing

Repositories are served under the path of their owner or group:

```bash
git clone https://ferrisgit.example.com/<owner-or-group-path>/<repository>.git
```

Git authenticates with HTTP Basic credentials: your **username** and a **personal API token**, created under
*Mon compte → Jetons d'accès*. Wikis are separate repositories reachable at `<repository>.wiki.git`.

### Public pages

Public repositories can be browsed without an account. Visitors who are not signed in get a read-only view at the
same URLs the signed-in interface uses, so a link to `/repositories/<owner>/<repository>` works for everyone:

- `/` and `/explore` list the public repositories (search, and sort by popularity, name or date);
- `/repositories/<path>` shows the overview with its README, the file tree and file contents, the commits (`/-/commits`)
  and the releases (`/-/releases`), whose assets can be downloaded;
- issues, merge requests, pipelines, wikis and settings are never exposed; signed-in users keep the usual interface.

A private repository, an unknown one and a repository hidden because the pages are switched off all answer with the
same "not found or private" page, so nothing reveals which private repositories exist. Draft releases are hidden.
Visitors are rate limited per client IP.

Two switches under *Admin → Réglages → Sécurité → Pages publiques* control the feature:

| Setting | Default | Effect |
|---|---|---|
| Pages publiques | on | Turns the anonymous pages and the read-only `/api/public/*` API on or off. When off, `/` redirects to the sign-in page. |
| Référencement par les moteurs de recherche | off | Lets search engines index the public pages. Off, every public response carries `X-Robots-Tag: noindex, nofollow` and `/robots.txt` disallows everything; on, only `/api/`, `/account` and `/admin/` are disallowed. |

What a public repository shows is what an anonymous `git clone` of it already reveals: commit author names and
e-mail addresses are part of the commit data returned by the public API, and the names of tags (including those behind
an unpublished draft release) are listed. Make a repository private if that is a concern.

### Roles

| Role | Can |
|---|---|
| Reader | Browse and clone. |
| Contributor | Reader rights plus contributing: open merge requests and issues, comment, review, apply suggestions, set labels. |
| Maintainer | Contributor rights plus merging, repository settings, CI variables, releases and tags, and managing collaborators. |

Roles can be granted on a repository or on a group; a group role applies to everything below it in the hierarchy.
Every group always keeps at least one Maintainer.

### Administration

Administrators (the *Super-administrateur* role) manage the instance from the Admin section: users (list, detail
page, invitations, password and MFA resets, admin promotion, deletion), settings, runners, the metrics dashboard
and the health page. Administrators see repository *metrics* (size, creation date) on a user's page, never the
source code.

Deleting a user removes their personal repositories. Merge requests, comments and releases they authored are kept and
shown as written by *Utilisateur supprimé*; issues, issue comments, reviews and pipelines they triggered are deleted.
Repositories owned by a group survive and are reassigned to the administrator performing the deletion. A user who is
the last Maintainer of a group, or the last active administrator, cannot be deleted.

## CI/CD

A pipeline is defined in `.ferrisgit-ci.yml` at the root of the repository:

```yaml
stages: [build, test]

jobs:
  compile:
    stage: build
    image: rust:1
    script:
      - cargo build --release

  unit-tests:
    stage: test
    image: rust:1
    needs: [compile]
    variables:
      RUST_LOG: debug
    cache: [target]
    script:
      - cargo test
```

| Key | Meaning |
|---|---|
| `stages` | Ordered list of stages. Jobs of a stage run in parallel. |
| `jobs.<name>.stage` | The stage the job belongs to. |
| `jobs.<name>.image` | Container image the job runs in. |
| `jobs.<name>.script` | Commands, chained with `&&`: the first failing command fails the job. |
| `jobs.<name>.variables` | Environment variables for this job. Repository CI variables are added on top. |
| `jobs.<name>.needs` | Jobs that must succeed first. |
| `jobs.<name>.tags` | Runner tags a runner must have to pick the job up. |
| `jobs.<name>.cache` | Cache keys persisted between runs of the same repository. |

The repository's own [`.ferrisgit-ci.yml`](.ferrisgit-ci.yml) is a small smoke-test pipeline.

### Docker runners

Run one or more `ferrisgit-runner` processes on hosts with Docker. The runner claims jobs from the server, clones the
repository, runs each job in a container and streams the logs back.

```bash
cargo build --release -p ferrisgit-runner

FERRISGIT_SERVER_URL=https://ferrisgit.example.com \
FERRISGIT_RUNNER_TOKEN=<runner token> \
FERRISGIT_RUNNER_TAGS=linux,docker \
./target/release/ferrisgit-runner
```

| Variable | Default | Description |
|---|---|---|
| `FERRISGIT_SERVER_URL` | required | Base URL of the FerrisGit server. |
| `FERRISGIT_RUNNER_TOKEN` | required | The runner's token. |
| `FERRISGIT_RUNNER_TAGS` | empty | Comma-separated tags this runner accepts. |
| `FERRISGIT_POLL_INTERVAL_SECS` | `5` | Seconds between polls when idle. |
| `FERRISGIT_WORKDIR_ROOT` | `/tmp/ferrisgit-runner` | Where jobs are checked out. |

A runner token is obtained either by an administrator creating the runner under *Runners* (the token is shown once),
or by the runner self-registering at `POST /api/runner/register` with the instance registration token configured in
the admin settings.

### Kubernetes execution engine

FerrisGit itself always runs as a container. With the Kubernetes engine selected in the admin settings, each CI job
instead runs as a Pod in your cluster. FerrisGit does not deploy anything to your cluster on its own behalf.

**How it finds the cluster.** The client is configured the standard way:

- Running inside the target cluster: mount a `ServiceAccount` token into the Pod (the Kubernetes default) with the
  permissions below. Nothing else is needed.
- Running elsewhere (for example Docker Compose): mount a kubeconfig file and point `KUBECONFIG` at it.

If neither is available, the server still starts; selecting the Kubernetes engine then fails explicitly and the Docker
runner mode is unaffected. At startup FerrisGit also detects the pod's namespace (or the kubeconfig context's) and,
when permitted, the cluster's default `StorageClass`, to pre-fill the settings.

**Settings**

| Setting | Notes |
|---|---|
| Execution engine | Can be switched back at any time; running pipelines keep the engine they started with. |
| `k8s_namespace` | Namespace for job Pods; defaults to the detected one. **Changing it requires a server restart**: the Pod watcher reads it once at startup while new Pods use the live value, so after a change without a restart every new job stays `Running` with no logs. |
| `k8s_cache_storage_class` | Required by any job that declares `cache:`. Must be a `StorageClass` supporting `ReadWriteMany`. A pre-filled default is never applied until an administrator saves it. |

**Permissions.** A namespaced `Role` is enough; FerrisGit never touches any other namespace:

```yaml
apiVersion: v1
kind: ServiceAccount
metadata: { name: ferrisgit-ci, namespace: ferrisgit-jobs }
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata: { name: ferrisgit-ci, namespace: ferrisgit-jobs }
rules:
  - apiGroups: [""]
    resources: ["pods", "pods/log"]
    verbs: ["get", "list", "watch", "create", "delete"]
  - apiGroups: [""]
    resources: ["persistentvolumeclaims"]
    verbs: ["get", "list", "create"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata: { name: ferrisgit-ci, namespace: ferrisgit-jobs }
subjects:
  - { kind: ServiceAccount, name: ferrisgit-ci, namespace: ferrisgit-jobs }
roleRef: { kind: Role, name: ferrisgit-ci, apiGroup: rbac.authorization.k8s.io }
```

`StorageClass` is cluster-scoped, so the optional default-class detection needs an extra `ClusterRole` granting `get`
and `list` on `storage.k8s.io/storageclasses`, bound to the same `ServiceAccount`. Without it the field is simply not
pre-filled.

**What FerrisGit does with them**

- Creates one Pod per job, `ferrisgit-job-<job-id>`, labelled `ferrisgit.io/job-id`, with `restartPolicy: Never`.
- Watches those Pods; on `Succeeded` or `Failed` it collects the logs and deletes the Pod. Cancelling a job or a
  pipeline deletes its Pod immediately.
- Creates one `PersistentVolumeClaim` per repository and cache key on first use and reuses it afterwards. Cache
  claims are never deleted automatically: delete one yourself to reset that cache.

**Known limitation.** There is no timeout for a Pod that never reaches a terminal phase (unschedulable, stuck in
`ImagePullBackOff`, waiting on a volume that cannot bind). Its job stays `Running`. If a pipeline never completes,
look for such a Pod with `kubectl get pods -n <namespace>`.

## Security

- **Multi-factor authentication is mandatory.** Every sign-in goes through the same flow; an account with no factor
  enrolled is forced through setup before it can do anything else.
- **Passwords** are hashed with Argon2. **Secrets at rest** (CI variables, webhook secrets, the SMTP password, TOTP
  secrets) are encrypted with AES-GCM under `SETTINGS_ENCRYPTION_KEY`.
- **Sessions** are JWTs stored in the browser's `localStorage`, not in an `HttpOnly` cookie. To limit the impact of a
  cross-site scripting bug, every response carries a Content-Security-Policy (`default-src 'self'`, `object-src
  'none'`, `frame-ancestors 'none'`) and `X-Frame-Options: DENY`. Changing or resetting a password, and resetting MFA,
  end the user's existing sessions.
- **Public pages** expose only repositories marked public, through a dedicated read-only API that makes a single
  authorization decision per request and returns the same 404 for private, unknown and switched-off cases. Search
  engine indexing is off unless an administrator enables it.
- **Rate limiting** applies to sign-in, registration, activation, password reset, MFA and the public endpoints, per client IP
  (IPv6 clients are counted per /64). Configure `TRUSTED_PROXY_CIDRS` correctly behind a proxy, otherwise every client
  shares one budget.
- **Passkeys** are bound to the exact origin in `PUBLIC_URL`. Changing the public host invalidates registered passkeys.
- Administrator actions such as password resets, promotions, MFA resets and user deletion are recorded as audit events.
- The repository itself is scanned for vulnerable dependencies on every pull request and on `main` (see
  [Continuous integration and delivery](#continuous-integration-and-delivery)), and the release image is scanned before
  it is pushed.
- Serve FerrisGit over HTTPS in production.

To report a vulnerability, use GitHub's private vulnerability reporting (the **Security** tab of the repository, then
*Report a vulnerability*) rather than opening a public issue.

## Operations

### Backups

State lives in two places, and both must be backed up together:

- the PostgreSQL database, with `pg_dump`;
- the `STORAGE_ROOT` volume (Git repositories, wikis, release assets).

Also keep `SETTINGS_ENCRYPTION_KEY` and `JWT_SECRET` somewhere safe: without the encryption key a restored database
cannot decrypt its secrets.

### Upgrades

Migrations run automatically at startup. The baseline is `migrations/0001_init.sql`; every schema
change since is a further numbered file (`0002_public_pages.sql` is the first). Do not edit an already-applied migration: databases that ran it will
refuse the changed checksum.

### Health

`GET /health` answers `200` when the server is up and is what the Kubernetes liveness and readiness probes call. The
Admin health page reports on the state of the components the server depends on, such as the database and storage.

### Recovering an account that lost its second factor

Every account must use a second factor. If a user loses **every** factor **and** their backup codes, an administrator
resets their MFA.

1. **From the interface (preferred).** Admin, then Utilisateurs, then the user's row, then *Réinitialiser la double
   authentification* (`DELETE /api/admin/users/{id}/mfa`). This removes the TOTP credential, the backup codes and the
   passkeys, signs the user out everywhere and e-mails them. At their next sign-in they are forced through MFA setup
   again.
2. **If no administrator can sign in** (the only administrator lost every factor), reset directly in the database.
   Replace `<USER>` with the username:

   ```sql
   BEGIN;
   -- 1. end every session and pending MFA token of that user
   UPDATE users SET token_epoch = token_epoch + 1 WHERE username = '<USER>';
   -- 2. remove the factors
   DELETE FROM mfa_backup_codes     WHERE user_id = (SELECT id FROM users WHERE username = '<USER>');
   DELETE FROM totp_credentials     WHERE user_id = (SELECT id FROM users WHERE username = '<USER>');
   DELETE FROM webauthn_credentials WHERE user_id = (SELECT id FROM users WHERE username = '<USER>');
   COMMIT;
   ```

   The next sign-in with the password then requires MFA setup. On Kubernetes, open a shell on the database Pod with
   `kubectl -n ferrisgit exec -it deploy/ferrisgit-postgres -- psql -U <db-user> <db-name>` and paste the block.

If a stored TOTP secret can no longer be decrypted (wrong `SETTINGS_ENCRYPTION_KEY`, for example a dump restored on
another instance), sign-ins for that user answer HTTP 500 by design, never a bypass. Use the same reset.

## Deploying to Kubernetes

The [`k8s/`](k8s) directory holds the manifests used for the author's own deployment:

| File | Content |
|---|---|
| `namespace.yaml` | The `ferrisgit` namespace. |
| `postgres.yaml` | PostgreSQL deployment and its volume claim. |
| `rbac.yaml` | The `ferrisgit-ci` `ServiceAccount`, `Role` and `RoleBinding` for the Kubernetes execution engine. |
| `ferrisgit.yaml` | The application: volume claim, `Deployment` (single replica, `Recreate` strategy), `Service`. |
| `ingress.yaml` | A Traefik ingress with a Let's Encrypt certificate. |

They are a starting point, not a turnkey chart, and contain values specific to that environment: the hostname
`app.ferrisgit.pro`, the private registry `alume.artiferris.pro/alume-docker/ferrisgit` with its `alume-registry` pull
secret, and Traefik annotations. Adapt them before use.

The manifests expect a Secret named `ferrisgit-secrets` with the keys `POSTGRES_PASSWORD`, `JWT_SECRET`,
`SETTINGS_ENCRYPTION_KEY` and `BOOTSTRAP_ADMIN_PASSWORD`:

```bash
kubectl create namespace ferrisgit
kubectl -n ferrisgit create secret generic ferrisgit-secrets \
  --from-literal=POSTGRES_PASSWORD=... \
  --from-literal=JWT_SECRET=... \
  --from-literal=SETTINGS_ENCRYPTION_KEY=... \
  --from-literal=BOOTSTRAP_ADMIN_PASSWORD=...
```

Use the image published by CI (`masmarino/ferrisgit`, once the Docker Hub credentials are configured) or build and
publish your own for the architecture of your nodes, then apply the manifests:

```bash
docker buildx build --platform linux/amd64 -t <registry>/ferrisgit:latest --push .
kubectl diff  -f k8s/
kubectl apply -f k8s/
kubectl -n ferrisgit rollout status deployment/ferrisgit-server
```

Apply the manifests whenever they change: restarting the Deployment alone only re-applies the spec already stored in the
cluster, so new environment variables would not reach the Pod. The server refuses to start without `PUBLIC_URL`. The
`Recreate` strategy stops the old Pod before starting the new one, so a rollout that fails leaves the service down
until it is fixed.

## Development

### Prerequisites

- [`rustup`](https://rustup.rs): the Rust toolchain is pinned in `rust-toolchain.toml` and installed automatically,
  with Clippy and rustfmt. Also [`sqlx-cli`](https://crates.io/crates/sqlx-cli)
- A C toolchain (`aws-lc-rs`, which provides TLS, and OpenSSL, which WebAuthn needs, both compile C code)
- Node.js 26 (`.nvmrc`, `engines` in `frontend/package.json`, and the version CI and the Docker image use), with npm
- Docker with Compose
- `git` on the `PATH` (the server shells out to `git http-backend`)

### Running locally with hot reload

```bash
cp .env.example .env          # then set real secrets
./scripts/dev.sh
```

The script starts PostgreSQL through Compose (published on port 5435), applies the migrations, runs the backend on
port 8080 with `cargo run` and the Angular dev server on <http://localhost:4201>, which proxies `/api` and `/health`
to the backend. Do not run the full Compose stack at the same time: only the database is needed.

If `.env` does not set them, the bootstrap administrator defaults to `admin` / `admin123`. Never use these outside a
local machine.

### Tests and checks

Backend tests connect to a real PostgreSQL and create one throwaway database per test, so `DATABASE_URL` must point at
a role that can create databases:

```bash
docker run --rm -d --name ferrisgit-test-db -p 5432:5432 \
  -e POSTGRES_USER=ferrisgit -e POSTGRES_PASSWORD=ferrisgit -e POSTGRES_DB=ferrisgit postgres:18-alpine

export DATABASE_URL=postgres://ferrisgit:ferrisgit@localhost:5432/ferrisgit
export SQLX_OFFLINE=true
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

`--test-threads=1` is needed because the configuration tests modify process environment variables. A few tests of
the Kubernetes adapter create a local [`kind`](https://kind.sigs.k8s.io) cluster and need `kind` and Docker; the
integration suite in `crates/ferrisgit-infrastructure/tests/` is additionally gated behind the `kind-tests` feature.

Frontend:

```bash
cd frontend
npm ci
npx ng test --watch=false     # unit tests (Vitest)
npx ng build                  # production build
npm run storybook             # component stories
```

### Continuous integration and delivery

The pipeline in `.github/workflows/ci-cd.yml` is shared in shape with [ArtiFerris](https://github.com/Masmarino/ArtiFerris).
It runs on pushes to `main`, `develop`, `feature/**`, `fix/**` and `release/**`, on version tags (`v*.*.*`) and on every
pull request:

| Job | When | What |
|---|---|---|
| Backend format & lint | always | `cargo fmt --check` and `cargo clippy -D warnings`. |
| Backend build & test | always | Build and tests against a PostgreSQL service container. |
| Frontend build & test | always | Production build and unit tests on Node 26. |
| Security scan | pull requests, `main`, `develop`, `release/**`, tags | `cargo audit` (RustSec), plus Trivy and Grype on the source tree. A HIGH or CRITICAL finding fails the job; results also go to the repository's **Security** tab as SARIF. |
| Tag guard | every push | A `v*.*.*` tag is refused unless its commit is on `main`. |
| Docker image | `main`, `release/**`, tags | Builds the image, scans it with Trivy and Grype, and only then pushes it. |
| GitHub release | tags | Creates the GitHub release of the tag once the image is pushed, with the matching [`CHANGELOG.md`](CHANGELOG.md) section as its notes. |

The image is pushed to Docker Hub as `masmarino/ferrisgit` (`latest` and `sha-*` from `main`, the version from a tag)
only when the repository secrets `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` are set; without them the image is still
built and scanned. There is no deploy job: FerrisGit is deployed by hand, see
[Deploying to Kubernetes](#deploying-to-kubernetes).

To publish a version, bump `version` in `[workspace.package]` of `Cargo.toml`, add a `## [X.Y.Z] - YYYY-MM-DD` section
at the top of [`CHANGELOG.md`](CHANGELOG.md), merge to `main` and push the tag `vX.Y.Z` on that commit. A missing section
fails only the release job; add it and rerun the job.

Dependencies are kept current by Dependabot (`.github/dependabot.yml`): Cargo, npm, Docker and GitHub Actions, weekly.
The GitHub Actions and base images are pinned by digest. The one accepted RustSec advisory and the reasoning behind it
are in `.cargo/audit.toml`.

### Database migrations and the SQLx query cache

Queries written with the `sqlx::query!` family are checked at compile time. To build without a database
(`SQLX_OFFLINE=true`, which the Docker build and CI use), their metadata is committed in `.sqlx/`. After changing a
query or a migration, regenerate it against a database that has every migration applied:

```bash
sqlx migrate run --source migrations          # DATABASE_URL pointing at a scratch database
cargo sqlx prepare --workspace -- --all-targets
```

Commit the resulting `.sqlx/` changes. The workspace-root `.sqlx/` is the only cache; do not add one inside a crate,
since a crate-level directory takes precedence and can silently go stale.

### Conventions

- Code, identifiers, comments and commit messages are in English; the user interface is in French.
- The domain and application crates never depend on infrastructure: new behaviour goes through a port.
- Keep `cargo clippy -- -D warnings` and `cargo fmt --check` clean.

## Repository layout

```
crates/
  ferrisgit-domain/          entities and ports
  ferrisgit-application/     use cases
  ferrisgit-infrastructure/  PostgreSQL, Git, SMTP, Kubernetes, crypto adapters
  ferrisgit-api/             HTTP server and API (binary: ferrisgit-api)
  ferrisgit-runner/          Docker job runner (binary: ferrisgit-runner)
frontend/                    Angular application
migrations/                  SQL schema (applied at startup)
.sqlx/                       SQLx offline query metadata
k8s/                         Kubernetes manifests
.github/                     GitHub Actions workflow (ci-cd.yml) and Dependabot configuration
.cargo/audit.toml            accepted RustSec advisories, with their justification
rust-toolchain.toml          pinned Rust toolchain
scripts/dev.sh               local development launcher
Dockerfile                   multi-stage build: frontend, backend, runtime image
docker-compose.yml           application and PostgreSQL for local use
```

## Roadmap

None of the items below exists yet: they are the direction the project is heading in, in rough priority order, and
the details will change as they are designed. Items are only ticked once they ship.

### Automated checks on every merge request and on `main`

The CI engine already runs arbitrary jobs. The goal is to make *checks* first-class: a set of analyses that run
automatically on every merge request and on every push to `main`, report their result on the merge request itself,
and can be made required before a merge.

**Code quality scanning, in the spirit of SonarQube**

- [ ] Static analysis of the changed code: bugs, code smells, duplication and complexity.
- [ ] Test coverage tracking, with the trend per branch and the coverage of what a merge request changes.
- [ ] Findings shown inline on the merge request diff and summarised in its timeline, with the history for `main`.
- [ ] Quality gates: configurable thresholds (for example no new blocker issue, minimum coverage on new code) that
  can block a merge.
- [ ] Support for the languages the platform's own users write, starting with Rust and TypeScript, through pluggable
  analysers rather than a single built-in engine.

**Security audits**

- [ ] Dependency audits against public advisory databases (lockfiles such as `Cargo.lock` and `package-lock.json`).
- [ ] Secret detection: credentials and keys committed in a diff or already present in the history.
- [ ] Static application security testing (SAST) of the changed code.
- [ ] Container image and infrastructure-as-code scanning.
- [ ] A security summary on every merge request and for the current state of `main`, with severity levels, and the
  option to block merging above a chosen severity.
- [ ] A repository-level security dashboard: open findings, their age, and how they evolve.

FerrisGit's own repository already runs comparable scans through GitHub Actions, which is the reference for what the
platform should offer natively.

Both families are meant to share the same building blocks: a pipeline stage or job that produces a machine-readable
report, a place to store and diff reports over time, and a check status that the merge request understands. The exact
report format (for example SARIF) and which analysers to integrate are open questions.

### Connecting with ArtiFerris

[ArtiFerris](https://github.com/Masmarino/ArtiFerris) already stores npm packages and Docker/OCI images and scans them
for vulnerabilities. FerrisGit and ArtiFerris are complementary halves of the same delivery chain, and the aim is to
connect them:

- [ ] Publish the artifacts a pipeline builds (npm packages, container images) to an ArtiFerris instance, using
  credentials held as CI variables or a dedicated integration.
- [ ] Trace every published artifact back to the commit, merge request and pipeline that produced it, and show it on
  the FerrisGit release and pipeline pages.
- [ ] Bring the artifact scan results from ArtiFerris back into the merge request checks described above.
- [ ] Explore shared identity, so one account and one MFA enrolment work on both platforms.

## Contributing

The project is developed on GitHub at <https://github.com/Masmarino/FerrisGit>.

- **Bugs and ideas:** open an [issue](https://github.com/Masmarino/FerrisGit/issues). For a bug, include what you did,
  what you expected, what happened, and the version or commit you ran.
- **Vulnerabilities:** do not open an issue; see [Security](#security).
- **Code:** open a pull request against `main`. For anything beyond a small fix, start with an issue so the approach
  can be agreed first. Before pushing, run the checks listed under [Tests and checks](#tests-and-checks) so that the
  GitHub Actions run is green; keep commits focused, with a message that explains why the change is made.
- **Conventions:** see [Conventions](#conventions).

## License

No license has been chosen yet, so all rights are reserved by default. Add a `LICENSE` file before distributing or
accepting outside contributions.
