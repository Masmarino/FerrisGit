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
  --set image.tag=0.1.2 \\
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
