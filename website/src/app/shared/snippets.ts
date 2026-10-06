// Code shown on the site. Same in every language: commands and YAML aren't translated.

/**
 * From the CI/CD examples of the docs (docs/ci-cd/exemples.md), minus the optional `cache` and `variables` keys:
 * a complete file FerrisGit accepts, short enough to read next to the pipeline it draws.
 */
export const CI_EXAMPLE = `stages: [lint, test]

jobs:
  format:
    stage: lint
    image: rust:1
    script:
      - rustup component add rustfmt
      - cargo fmt --all -- --check

  clippy:
    stage: lint
    image: rust:1
    script:
      - rustup component add clippy
      - cargo clippy --all-targets -- -D warnings

  test:
    stage: test
    image: rust:1
    needs: [format, clippy]
    script:
      - cargo test --all-targets
`

/** The hero's one-liner: the command that starts the stack (docs/administration/installation.md). */
export const HERO_COMMAND = 'docker compose up -d --build'

/**
 * What the hero prints: the command that measures the stack and its output, copied as printed. The figures and the
 * way they were taken are in docs/administration/ressources.md.
 */
export const PROOF_TERMINAL = `$ docker stats --no-stream measure-ferrisgit-server-1 measure-postgres-1
CONTAINER ID   NAME                         CPU %     MEM USAGE / LIMIT     MEM %     NET I/O           BLOCK I/O         PIDS
04049f2efe20   measure-ferrisgit-server-1   0.00%     3.262MiB / 7.748GiB   0.04%     16.7kB / 65.4kB   0B / 0B           13
76c668d0c77c   measure-postgres-1           1.77%     66.21MiB / 7.748GiB   0.83%     66.3kB / 15.1kB   29.2MB / 55.3MB   13`

/** From docs/administration/installation.md. */
export const COMPOSE_COMMANDS = `git clone https://github.com/Masmarino/FerrisGit.git
cd FerrisGit
cp .env.example .env
# set POSTGRES_PASSWORD, JWT_SECRET, SETTINGS_ENCRYPTION_KEY and the admin password in .env
docker compose up -d --build
`

/** From docs/administration/deploiement-kubernetes.md. */
export const HELM_COMMANDS = `git clone https://github.com/Masmarino/FerrisGit.git
cd FerrisGit
helm upgrade --install ferrisgit ./helm/ferrisgit \\
  --namespace ferrisgit --create-namespace \\
  --set image.tag=0.1.4 \\
  --set ingress.host=git.example.com \\
  --set-string ferrisgit.trustedProxyCidrs=10.42.0.0/16
kubectl -n ferrisgit rollout status deployment/ferrisgit
`

/** From the Development section of the README: the backend with cargo, the web application with the Angular CLI. */
export const SOURCE_COMMANDS = `git clone https://github.com/Masmarino/FerrisGit.git
cd FerrisGit
cp .env.example .env          # then set real secrets
./scripts/dev.sh              # PostgreSQL via Compose, cargo run on :8080, ng serve on :4201
`

/**
 * The response of a local instance (version 0.1.2) to the command, copied as printed. The four security headers come from
 * security_headers() in crates/ferrisgit-api/src/lib.rs, which sets them on every response; x-robots-tag comes from the
 * public pages setting.
 */
export const SECURITY_HEADERS = `$ curl -s -D - -o /dev/null http://localhost:8080/health
HTTP/1.1 200 OK
content-type: text/plain; charset=utf-8
content-security-policy: default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'
x-frame-options: DENY
x-content-type-options: nosniff
referrer-policy: no-referrer
x-robots-tag: noindex, nofollow
content-length: 2
date: Sun, 04 Oct 2026 11:41:43 GMT`
