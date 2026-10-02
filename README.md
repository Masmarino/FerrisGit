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
- [Website](#website)
- [Development](#development)
- [Repository layout](#repository-layout)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)

## Features

**Hosting and collaboration**

- Git over HTTP (smart protocol): clone, fetch and push, authenticated with your username and a personal access token
  (public repositories can be cloned anonymously while the public pages are on).
- Repositories owned by a user or nested inside hierarchical **groups**, public or private (the description and the
  visibility can be changed afterwards; any signed-in user can create a top-level group).
- Per-repository and per-group roles: **Reader**, **Contributor** and **Maintainer**.
- **Merge requests** with threaded inline comments, code suggestions that can be applied from the interface,
  approvals and change requests, conflict detection, and a timeline of everything that happened.
- **Issues** with labels, milestones, assignees and a kanban board.
- **Wikis** stored as a Git repository of their own, **releases** with attached assets, and repository stars.
- **Webhooks** for merge request, collaborator, pipeline and issue events, with secrets encrypted at rest.
- In-app **notifications**, **search**, a personal dashboard, and repository language statistics.
- **Public pages**: visitors without an account can browse public repositories (catalogue, README, files, commits and
  releases) and download release assets, read-only.
- **Documentation** at `/docs`, open to everyone, signed in or not, and independent of the public pages switch: a user
  guide, the CI/CD reference, administration and self-hosting, and the REST API reference, in French, with search.

**CI/CD**

- Pipelines described in a `.ferrisgit-ci.yml` file at the root of the repository.
- Two execution engines: **Docker runners** (a small `ferrisgit-runner` binary polling the server) or
  **Kubernetes**, where each job runs as a Pod.
- Per-repository CI variables, encrypted at rest (Docker runners only), and per-job caches (Kubernetes only).

**Accounts and administration**

- Mandatory multi-factor authentication for every account: TOTP authenticator apps, **passkeys** (WebAuthn) and
  single-use backup codes.
- Free registration behind an administrator switch, or accounts created by e-mail invitation.
- Administrator tooling: user list and detail pages, invitations, password resets sent by e-mail, MFA resets,
  promotion and demotion of administrators, and deletion of a user with anonymisation of their contributions.
- Instance settings edited in the interface (SMTP, registration, execution engine, runner registration token,
  concurrent job ceiling, job log retention), plus a dashboard with usage metrics and a health page.
- Personal access tokens for Git over HTTPS.

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

- **E-mail (SMTP)**: server, credentials and sender. Needed for invitations, password resets and security alerts
  (activity notifications stay in the application). When a message cannot be sent, the administrator is given the link
  to pass on manually.
- **Registration**: free sign-up is off by default; administrators can always invite users.
- **Execution engine**: Docker runners or Kubernetes (see [CI/CD](#cicd)).
- **Docker runners**: the instance registration token that lets runners register themselves (stored hashed, can be
  generated in the browser and is shown once), and a ceiling on the number of jobs running at the same time (Docker
  runners only, empty for no limit).
- **Job log retention**: a number of days after which the log text of finished jobs is deleted (empty to keep logs
  forever). See [Log retention](#log-retention).

## Using FerrisGit

### Cloning and pushing

Repositories are served under the path of their owner or group:

```bash
git clone https://ferrisgit.example.com/<owner-or-group-path>/<repository>.git
```

Git authenticates with HTTP Basic credentials: your **username** and a **personal access token** (`fg_…`), created
under *Mon compte → Jetons Git*. A token is for Git over HTTPS only (clone, fetch, push): it does not authenticate
against the REST API, which takes the session obtained at sign-in. Wikis are separate repositories reachable at
`<repository>.wiki.git`. A public repository, and its wiki, can be cloned without credentials while *Pages publiques*
is on; see [Public pages](#public-pages).

### Public pages

Public repositories can be browsed without an account. Visitors who are not signed in get a read-only view at the
same URLs the signed-in interface uses, so a link to `/repositories/<owner>/<repository>` works for everyone:

- `/` and `/explore` list the public repositories (search, and sort by popularity, name or date);
- `/repositories/<path>` shows the overview with its README, the file tree and file contents, the commits (`/-/commits`)
  and the releases (`/-/releases`), whose assets can be downloaded;
- issues, merge requests, pipelines, wikis and settings are never exposed; signed-in users keep the usual interface.

A private repository, an unknown one and a repository hidden because the pages are switched off all answer with the
same "not found or private" page (a 401 for Git), so nothing reveals which private repositories exist. Draft releases are hidden.
Visitors are rate limited per client IP.

Two switches under *Admin → Réglages → Sécurité → Pages publiques* control the feature:

| Setting | Default | Effect |
|---|---|---|
| Pages publiques | on | Turns the anonymous pages, the read-only `/api/public/*` API and the anonymous Git clone of public repositories (wikis included) on or off. When off, `/` redirects to the sign-in page and an anonymous clone gets the same 401 as a private repository; signed-in users (username and token) still read public repositories. |
| Référencement par les moteurs de recherche | off | Lets search engines index the public pages. Off, every public response carries `X-Robots-Tag: noindex, nofollow` and `/robots.txt` disallows everything; on, only `/api/`, `/account` and `/admin/` are disallowed. |

What a public repository shows is what an anonymous `git clone` of it already reveals: commit author names and
e-mail addresses are part of the commit data returned by the public API, and the names of tags (including those behind
an unpublished draft release) are listed. Make a repository private if that is a concern.

### Roles

| Role | Can |
|---|---|
| Reader | Browse and clone; read issues, merge requests and wikis. |
| Contributor | Reader rights plus contributing: open issues and merge requests, comment on both, review, apply suggestions, set labels, be assigned an issue. |
| Maintainer | Contributor rights plus merging, repository settings, CI variables, releases and tags, and managing collaborators. |

Roles can be granted on a repository or on a group; a group role applies to everything below it in the hierarchy,
and the effective role is the highest of them. On a public repository every signed-in user is a Reader. The interface
only shows the buttons the server accepts (for instance *Fusionner* is for Maintainers and owners only). Every group
always keeps at least one Maintainer.

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
| `stages` | Ordered list of stages. A stage is a barrier: its jobs start once every job of every earlier stage succeeded; jobs of a stage with no dependency between them run in parallel. |
| `jobs.<name>.stage` | The stage the job belongs to. |
| `jobs.<name>.image` | Container image the job runs in. |
| `jobs.<name>.script` | Commands, chained with `&&`: the first failing command fails the job. |
| `jobs.<name>.variables` | Environment variables for this job. With Docker runners, repository CI variables are added on top. |
| `jobs.<name>.needs` | Jobs that must succeed first. Replaces the stage barrier for this job. Cycles are rejected. |
| `jobs.<name>.tags` | Tags a runner must have to pick the job up (Docker runners only). |
| `jobs.<name>.cache` | Cache keys persisted between runs of the same repository (Kubernetes only). |

The repository's own [`.ferrisgit-ci.yml`](.ferrisgit-ci.yml) is a small smoke-test pipeline.

A pipeline is created after every successful push, from the file at the root of the default branch. It is `pending`
until a first job starts, `running` while its jobs run, then `success`, `failed` or `canceled`. A job whose
`needs` or earlier stages did not succeed never starts: it ends `skipped`, so a failure ends the pipeline instead of
leaving it waiting, and the pipeline is `failed`. Both execution engines apply the same rule. A pipeline file that is
present but invalid (bad YAML, unknown stage, bad `needs`, a `needs` cycle, a bad cache key) still produces a `failed`
pipeline without any job, carrying the parser's message, which the web interface and the API (`error`) show to
whoever pushed. A missing file creates no pipeline.

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
  authorization decision per request and returns the same 404 for private, unknown and switched-off cases. The
  anonymous Git clone of a public repository follows the same switch. Search engine indexing is off unless an
  administrator enables it.
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
change since is a further numbered file (`0002_public_pages.sql` is the first, `0004_job_log_retention.sql` adds the
log retention bookkeeping). Do not edit an already-applied migration: databases that ran it will
refuse the changed checksum.

### Log retention

When *Durée de conservation des journaux* is set under *Admin → Réglages → Exécution*, a background task of the server
empties the logs of finished jobs that ended more than that many days ago. It runs once at startup, then every 24 hours,
one sweep at a time, in batches of 500 jobs. Pipelines, jobs, statuses and dates are kept, and the interface shows
*Journal supprimé le …* instead of the log. Deleted logs are not recoverable (restore a backup for that). The server
stops the task cleanly on `SIGTERM` or Ctrl-C, and gives open connections 10 seconds to finish before exiting.

### Health

Three unauthenticated routes, outside `/api` and outside the rate limits:

| Route | Answers |
|---|---|
| `GET /healthz` (and `GET /health`, the same) | `200 ok` as long as the process runs. It never touches the database, so a database outage does not get the pod restarted. The Helm chart uses it for the startup and liveness probes. |
| `GET /readyz` | `200 ok` when the database answers a trivial query within 2 seconds, otherwise `503` with no detail. The chart uses it for the readiness probe: a pod whose database is down leaves the Service without being restarted. |

The Admin health page reports on the state of the components the server depends on, such as the database and storage.

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

The Helm chart [`helm/ferrisgit`](helm/ferrisgit) deploys FerrisGit with its own PostgreSQL. It replaces the manifests
that used to live in `k8s/`. Its templates:

| Template | Installs |
|---|---|
| `ferrisgit.yaml` | The server: volume claim, `Deployment` (a single replica, `Recreate` strategy) and `Service`. |
| `postgres.yaml` | PostgreSQL: volume claim, `Deployment` and `Service`. |
| `secret.yaml` | The `ferrisgit-secrets` Secret. |
| `ingress.yaml` | The Traefik `Ingress`. |
| `networkpolicy.yaml` | Only the FerrisGit pod reaches PostgreSQL, and the FerrisGit pod only accepts port 8080 (`networkPolicy.enabled`). |
| `rbac.yaml` | The `ferrisgit-ci` `ServiceAccount`, `Role` and `RoleBinding` of [Kubernetes execution engine](#kubernetes-execution-engine) (`kubernetesExecutor.enabled`). |

```bash
helm upgrade --install ferrisgit ./helm/ferrisgit \
  --namespace ferrisgit --create-namespace \
  --set image.tag=<release> \
  --set ingress.host=git.example.com \
  --set-string ferrisgit.trustedProxyCidrs=10.42.0.0/16
kubectl -n ferrisgit rollout status deployment/ferrisgit
```

The release must be called `ferrisgit` for the resource names to be the ones the documentation uses (`ferrisgit`,
`ferrisgit-postgres`, `ferrisgit-secrets`, `ferrisgit-storage`, `ferrisgit-postgres-data`, `ferrisgit-ci`).

- **What the cluster needs.** Traefik as the ingress controller, with the middlewares `traefik-https` and
  `traefik-headers` and the TLSOption `traefik-mintls13` (TLS 1.3 minimum) in the `traefik` namespace; a CNI that enforces
  `NetworkPolicy`; a default `StorageClass` (or `storage.storageClassName` and `postgres.storage.storageClassName`). Set
  `ingress.middlewares` and `ingress.tlsOptions` to `""` to use neither. There is deliberately no rate-limit middleware:
  a clone or a push is a burst of requests that one would reject.
- **Certificate.** By default Traefik's own `letsencrypt-http` resolver (`ingress.certResolver`), which is what the old
  manifests used: it needs a DNS A record from the host to the Traefik load balancer. To use cert-manager instead, set
  `--set ingress.certResolver= --set ingress.clusterIssuer=<issuer>`; setting both is an error, and setting neither
  requests no certificate (the Secret `ingress.tlsSecretName` must then exist).
- **Image.** `image.tag` has no default; `image.digest` pins the image (the CI does). Add `imagePullSecrets` for a private
  registry.
- **Secrets.** Left empty, the chart generates the database password, `JWT_SECRET`, `SETTINGS_ENCRYPTION_KEY` and the
  bootstrap administrator's password in `ferrisgit-secrets` and finds them again with `lookup` on every upgrade. Under
  `helm template`, `--dry-run`, Argo CD or Flux `lookup` returns nothing, so pass every `secrets.*` value yourself
  (`helm/ferrisgit/values-secrets.example.yaml`). Never delete the Secret while the database volume exists, and never
  change `SETTINGS_ENCRYPTION_KEY`: there is no rotation, stored CI variables would become unreadable.
- **Proxy network.** With the ingress on, set `ferrisgit.trustedProxyCidrs` to the ingress controller's pod network: the
  chart refuses to render otherwise, since every visitor would share one sign-in rate-limit budget
  (`ferrisgit.allowSharedRateLimit=true` accepts that).
- **Hardening.** Pods run as uid 100 / 70, without privilege escalation, with a read-only root filesystem (`/data` and
  `/tmp` are writable), seccomp `RuntimeDefault` and no capability. Probes: `/healthz` (startup, up to 5 minutes for the
  migrations, then liveness) and `/readyz` (readiness, see [Health](#health)).
- **Uninstall.** The Secret and both volumes survive `helm uninstall` (`persistence.keepOnUninstall`); delete them by hand
  to wipe the data.

The CI deploys a version tag to the cluster: the `Deploy to Kubernetes` job runs `helm upgrade --install --atomic` with the digest of
the image it just pushed (see [Continuous integration and delivery](#continuous-integration-and-delivery)).

**Rolling back.** `helm rollback`, `--atomic` or an older image tag do not undo migrations. The server applies the ones it
ships at every start (`sqlx::migrate!`) and refuses to start on a database that records a migration it does not know: the
older release stops with `failed to run migrations: VersionMissing(<n>)` and the pod restarts in a loop. Back up before an
upgrade; to go back, restore the database backup (and the repositories volume it matches).

**Coming from the old manifests.** The Secret and the two volume claims keep their names, so they can be adopted without
losing data; the `Deployment`s and `Service`s must be recreated (their selectors changed). The procedure, checked against
the names the chart renders, is in the documentation page *Déploiement sur Kubernetes*
(`docs/administration/deploiement-kubernetes.md`).

## Website

The public site, <https://www.ferrisgit.pro>, lives in [`website/`](website). It is an Angular project of its own (it shares
nothing with `frontend/`), prerendered to static files and served by nginx. The application is at
<https://app.ferrisgit.pro>; `ferrisgit.pro` redirects to `www`.

- **Pages and languages.** Home, Features and Roadmap, in English (the default), French, Italian, Spanish and German:
  fifteen prerendered pages under `/<lang>/`, `/<lang>/features/` and `/<lang>/roadmap/`. The root `/` is not a page: nginx
  redirects it to the language of the visitor's `Accept-Language` header, or to `/en/`.
- **Local development.** Node 26:

  ```bash
  cd website
  npm ci
  npm start        # development server on http://localhost:4200
  npm run lint
  npm test
  npm run build    # prerendered site in dist/ferrisgit-website/browser, then scripts/check-dist.mjs checks it
  ```

  `npm start` and `npm run build` regenerate `public/sitemap.xml` from `src/app/seo/site.json` first (it is not committed).

- **Generated assets.** These are committed, so a normal build does not regenerate them (commands run from `website/`):
  - `npm run images`: the logos, favicon, Apple touch icon and the social previews, one per language;
  - `npm run plan`: the template of the architecture drawing, `src/app/shared/architecture-plan/architecture-plan.html`;
  - `python3 scripts/subset-fonts.py`: the subset Instrument Sans and IBM Plex Mono in `public/fonts/`, from the
    `@fontsource` packages (needs `fonttools` and `brotli`);
  - `node scripts/capture-screens.mjs`: the product screenshots in `public/images/screens/`, from the application's
    Storybook; the instructions are at the top of that script.
- **Image.** `website/Dockerfile` builds the site in a Node stage and serves it with nginx as an unprivileged user on port 8080
  (read-only root filesystem, `/tmp` only). `website/docker/nginx.conf` holds the redirects, the cache rules and the security
  headers. The Content-Security-Policy is generated from the built pages at image build time: the pages carry no inline script
  (`npm run build` fails if one appears), so it is `script-src 'self'`, and an inline event handler fails the image build.
  The image is `masmarino/ferrisgit-website`:

  ```bash
  docker build -t ferrisgit-website website/
  docker run --rm --read-only --tmpfs /tmp -p 8080:8080 ferrisgit-website   # http://localhost:8080
  ```

- **Chart.** [`website/helm/ferrisgit-website`](website/helm/ferrisgit-website) deploys one stateless pod, a `Service`, an
  `Ingress` for `www.ferrisgit.pro` and `ferrisgit.pro` and a `NetworkPolicy`. It follows the conventions of the application
  chart: Traefik's `letsencrypt-http` resolver by default (or a cert-manager `ingress.clusterIssuer`, never both), the
  `traefik-https` and `traefik-headers` middlewares and the `traefik-mintls13` TLSOption (`""` for none), `image.tag`
  without a default and an optional `image.digest`.

  ```bash
  helm upgrade --install ferrisgit-website ./website/helm/ferrisgit-website \
    --namespace ferrisgit-website --create-namespace --set image.tag=<release>
  ```

- **Deployment.** Only with version tags, together with the application: see
  [Continuous integration and delivery](#continuous-integration-and-delivery). The release is `ferrisgit-website`, in the
  namespace `ferrisgit-website`.
- **DNS.** Before the first deploy, add an A record for `www.ferrisgit.pro` and one for `ferrisgit.pro` pointing at the
  Traefik load balancer; Traefik's resolver needs both to issue the certificate. The cluster needs the same Traefik
  middlewares and TLSOption as the application.
- **No tracking.** The site sets no cookie and loads nothing from another origin: no analytics, fonts or scripts from a CDN.

## Development

### Prerequisites

- [`rustup`](https://rustup.rs): the Rust toolchain is pinned in `rust-toolchain.toml` and installed automatically,
  with Clippy and rustfmt
- [`sqlx-cli`](https://crates.io/crates/sqlx-cli), for `sqlx migrate run` and `cargo sqlx prepare`
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
It runs on pushes to `main`, `develop`, `feature/**`, `fix/**` and `release/**`, on version tags (`v*.*.*`), on every
pull request and on demand:

| Job | When | What |
|---|---|---|
| Backend format & lint | always | `cargo fmt --check` and `cargo clippy -D warnings`. |
| Backend build & test | always | Build and tests against a PostgreSQL service container. |
| Frontend build & test | always | Production build and unit tests on Node 26. |
| Security scan (source & dependencies) | pull requests, `main`, `develop`, `release/**`, tags | `cargo audit` (RustSec), plus Trivy and Grype on the source tree. A HIGH or CRITICAL finding fails the job; results also go to the repository's **Security** tab as SARIF. |
| Require the tagged commit to be on main | every push | A `v*.*.*` tag is refused unless its commit is on `main`. |
| Build, scan & push Docker image | `main`, `release/**`, tags | Builds the image, scans it with Trivy and Grype, and only then pushes it. |
| GitHub release | tags | Creates the GitHub release of the tag once the image is pushed, with the matching [`CHANGELOG.md`](CHANGELOG.md) section as its notes. |
| Helm chart lint | always | `helm lint --strict` and `helm template` of `helm/ferrisgit` with the default values, with cert-manager instead of the Traefik resolver, and with several switches off; checks that setting `ingress.certResolver` and `ingress.clusterIssuer` together is refused. |
| Detect website changes | always | Compares the run with its base (the previous commit of a push, the target branch of a pull request) and says whether `website/` or the workflow changed. A version tag always counts as a change. |
| Website lint, build & chart | when the site changed | `npm ci`, `npm run lint`, `npm test` and `npm run build` in `website/`, then `helm lint --strict` and `helm template` of `website/helm/ferrisgit-website` with the default values, with cert-manager instead of the Traefik resolver, with the middlewares off, without the ingress and with a digest; checks that both certificate sources together, and a missing `image.tag`, are refused. |
| Deploy to Kubernetes | tags | Upgrades the production release with `helm upgrade --install --atomic`, pinned to the digest of the image just pushed. Needs the `production` environment, the secret `KUBE_CONFIG` (base64 kubeconfig) and the variable `TRUSTED_PROXY_CIDRS`; `FERRISGIT_INGRESS_HOST`, `FERRISGIT_CERT_RESOLVER` and `FERRISGIT_CLUSTER_ISSUER` are optional. Two deploys never run at once, and the commit must be on `main`. |
| Build, scan & push website image | tags | After the site checks and the check that the tagged commit is on `main`: builds `website/Dockerfile`, scans the image with Trivy and Grype, and only then pushes `masmarino/ferrisgit-website:<version>`. |
| Deploy website to Kubernetes | tags | Upgrades the `ferrisgit-website` release in the `ferrisgit-website` namespace (created if missing) with `helm upgrade --install --atomic`, pinned to the digest of the website image. Same `production` environment and `KUBE_CONFIG` as the application; two website deploys never run at once. |

The image is pushed to Docker Hub as `masmarino/ferrisgit` (`latest` and `sha-*` from `main`, the version from a tag)
only when the repository secrets `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` are set; without them the image is still
built and scanned. A version tag is then deployed to Kubernetes by the Deploy job, see
[Deploying to Kubernetes](#deploying-to-kubernetes).

The [website](#website) is built and shipped by its own jobs, which never hold back the application's: the image
`masmarino/ferrisgit-website` is built only from a version tag (same secrets and same scans), and the same tag deploys it.
The checks of the site run when `website/` or the workflow changed, so a push that leaves the site alone does not rebuild it.
The kubeconfig behind `KUBE_CONFIG` must be allowed to create the `ferrisgit-website` namespace.

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

### Documentation

The documentation site is a set of Markdown files in [`docs/`](docs), served by the application at `/docs`. The files
are copied into the frontend build as static assets (`frontend/docs` is a symlink to `docs/`, and the `Dockerfile` copies
the directory next to `frontend/`); nothing is stored in the database.

- `docs/index.json` lists the sections and pages, in display order. A page is `docs/<section>/<page>.md` and is served
  at `/docs/<section>/<page>`. The first line of a page is its `# Title`, the same as in the index.
- Links between pages are absolute application paths, such as `[reference](/docs/ci-cd/reference-yaml)`. Callouts are
  blockquotes that start with `**Note**` or `**Attention**`.
- A complete pipeline example is written in a fenced block tagged `yaml ferrisgit-ci`.
- In the REST reference (`docs/api/`), each route has a heading of the form ``### `GET /api/repositories/{id}` ``.

Three checks keep the pages honest, and run with the usual tests:

| Check | Where | What it does |
|---|---|---|
| Index and links | `frontend/src/app/docs/docs-content.spec.ts` | Every listed page exists and every page is listed, titles match, and each internal link and anchor resolves. |
| Pipeline examples | `crates/ferrisgit-domain/tests/docs_pipeline_examples.rs` | Every `yaml ferrisgit-ci` block is accepted by the real pipeline parser. |
| API routes | `crates/ferrisgit-api/tests/docs_api_routes.rs` | The paths in the router and in `docs/api/` are the same: a new route without documentation fails. |

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
docs/                        documentation pages (Markdown) and their index
.sqlx/                       SQLx offline query metadata
helm/ferrisgit/              Helm chart (Kubernetes deployment)
website/                     public site (Angular, prerendered), its Dockerfile, nginx configuration and Helm chart
.github/                     GitHub Actions workflow (ci-cd.yml) and Dependabot configuration
.cargo/audit.toml            accepted RustSec advisories, with their justification
rust-toolchain.toml          pinned Rust toolchain
scripts/dev.sh               local development launcher
Dockerfile                   multi-stage build: frontend, backend, runtime image
docker-compose.yml           application and PostgreSQL for local use
docker-compose.override.yml  publishes PostgreSQL on port 5435 (used by scripts/dev.sh)
.env.example                 template of the variables Compose and scripts/dev.sh read
CHANGELOG.md                 release notes, one section per version
.ferrisgit-ci.yml            smoke-test pipeline of FerrisGit's own CI engine
```

## Roadmap

None of the items below exists yet: they are the direction the project is heading in, version by version, and the
details will change as they are designed. Items are only ticked once they ship.

### 0.2: a CI foundation for checks, five languages and a theme setting

Code scanning needs the platform to understand *checks*: results that belong to a commit, show up on a merge request,
and can stop a merge. This version builds that, translates the interface and lets people choose its theme.

- [ ] Protected branches: no direct push, merge only through a merge request, required approvals and required checks.
- [ ] Pipelines on merge requests, with their status shown on the merge request and required before a merge.
- [ ] Predefined CI variables (commit, branch, merge request, pipeline) and `rules` to decide when a job runs.
- [ ] Pipeline artifacts and a report upload API, which is what the scans of the next versions use.
- [ ] Re-running a pipeline from the interface, and scheduled pipelines.
- [ ] Personal access tokens usable on the REST API, with scopes, and an OpenAPI description generated from the code.
- [ ] A first link with ArtiFerris: publish an artifact built by a pipeline to an ArtiFerris instance.
- [ ] Internationalisation with [Transloco](https://github.com/jsverse/transloco): the interface in English, French,
  Italian, Spanish and German. The language follows the browser on a first visit, can be changed on the fly and is saved
  on the account; the e-mails FerrisGit sends use the recipient's language, and the public pages follow the reader's
  `Accept-Language`. The documentation is translated afterwards.
- [ ] Dark mode as a setting: choose system, light or dark, saved on the account and applied without a flash. The interface
  follows the system preference today and has no switch. The logo of the signed-in shell follows the theme too.

### 0.3: the analysis engine, secrets and dependencies

FerrisGit gets its own analysis engine instead of wrapping third-party tools: a `ferrisgit-scan` crate, shipped as a
binary a pipeline job runs, so the analysis scales with the runners and the server stays light. Its analysers are
plugins behind one interface that produce *findings* in a model close to SARIF, so reports from other tools can still be
imported next to the native ones.

- [ ] The engine and the findings store: a scan per commit, findings identified by a stable fingerprint, and a baseline
  per branch to tell the *new* findings of a merge request from the old ones.
- [ ] Native secret detection, in the diff of a merge request and in the whole history.
- [ ] Native dependency audit: lockfiles (`Cargo.lock`, `package-lock.json`, and others later) matched against public
  advisory databases such as OSV.
- [ ] Findings shown inline on the merge request diff and summarised in its timeline.
- [ ] Import of SARIF reports from other tools.

### 0.4: quality gates and the dashboard

- [ ] Quality gates, configurable per repository: for example no new blocker finding and a minimum coverage on new
  code. A gate is a required check, so it can block a merge.
- [ ] A repository dashboard: open findings, their age, trend per branch, technical debt and hotspots.
- [ ] Triage: mark a finding as a false positive or as won't fix, assign it, suppress it in the code, and keep the history.
- [ ] A `.ferrisgit/scan.yml` file to choose analysers, rule sets and thresholds, and to exclude paths.

### 0.5: code quality

The full repository is analysed on every push to the default branch and on every merge request, and the gates only
look at what a merge request introduces (the "clean as you code" approach), so an existing project stays adoptable.

- [ ] Parsing through tree-sitter, so the same engine reads several languages. Rust and TypeScript first.
- [ ] Metrics: size, cyclomatic and cognitive complexity, duplication.
- [ ] Code smells and bug patterns, as declarative rules and as native rules.
- [ ] Test coverage tracking (LCOV and Cobertura reports), with the coverage of what a merge request changes.

### 0.6: security analysis and ArtiFerris

- [ ] Static application security testing: pattern rules first, then data flow within a function.
- [ ] Container image and infrastructure-as-code scanning.
- [ ] A security summary on every merge request and a security dashboard for the repository, with severities and the
  option to block merging above a chosen severity.
- [ ] Trace every artifact published to ArtiFerris back to the commit, merge request and pipeline that produced it, and
  show it on the FerrisGit release and pipeline pages.
- [ ] Bring the artifact scan results of ArtiFerris back into the merge request checks.
- [ ] Explore shared identity, so one account and one MFA enrolment work on both platforms.

### The platform, in no fixed order

- [ ] Git over SSH and deploy keys.
- [ ] Forks and merge requests between repositories; renaming and transferring a repository with redirects.
- [ ] Squash and rebase merges, reopening a merge request, draft merge requests and `CODEOWNERS`.
- [ ] Mentions (`@name`), issue references (`#12`) and closing an issue from a commit or a merge request.
- [ ] Full-text search in the code, and import from GitHub and GitLab.
- [ ] Self-service password reset, e-mail notifications with per-user settings, and single sign-on (OIDC, LDAP).
- [ ] An audit log page in the administration, Prometheus metrics, and backup and restore tooling.
- [ ] Webhooks with retries, a delivery log and more events (push, release).

### Under consideration, not scheduled

Ideas with no version yet; what they contain is still to be defined.

- [ ] AI agents on a repository. Candidates: a review assistant on merge requests, summaries of merge requests and
  commits, an explanation of why a pipeline failed, issue triage and labelling. Whatever ships has to fit a self-hosted
  platform: a model endpoint the administrator chooses (a local one included), opt-in per repository, scoped
  credentials, and every action recorded.
- [ ] A commit graph in the repository view: branches, merges and tags drawn together, next to the file tree and the
  commit list that already exist.
- [ ] Profile photos, stored in S3-compatible object storage. The initials stay as the fallback, and the same storage
  could later hold release files and pipeline artifacts.

### Connecting with ArtiFerris

[ArtiFerris](https://github.com/Masmarino/ArtiFerris) already stores npm packages and Docker/OCI images and scans them
for vulnerabilities. FerrisGit and ArtiFerris are complementary halves of the same delivery chain. The link grows in
three steps: publishing from a pipeline (0.2), tracing an artifact back to its commit (0.6), and bringing the scan
results back into the merge request checks (0.6). Shared identity stays an open question.

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
