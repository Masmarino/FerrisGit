#!/usr/bin/env bash
# Runs the backend and the frontend locally, with hot reload. PostgreSQL comes from docker compose (published on 5435)
# and is left running when the script stops: `docker compose stop postgres` stops it.
# The backend is a local `cargo run` on :8080 and the frontend the Angular dev server on :4201, which forwards /api and
# /health to :8080 (frontend/proxy.conf.json). Do not run the full compose stack at the same time: it also uses :8080.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# Compose reads .env itself; source it here too so the local backend uses the same credentials.
if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

export DATABASE_URL="postgres://ferrisgit:${POSTGRES_PASSWORD:?POSTGRES_PASSWORD must be set in .env}@localhost:5435/ferrisgit"
export JWT_SECRET="${JWT_SECRET:?JWT_SECRET must be set in .env}"
export SETTINGS_ENCRYPTION_KEY="${SETTINGS_ENCRYPTION_KEY:?SETTINGS_ENCRYPTION_KEY must be set in .env}"
export PUBLIC_URL="${PUBLIC_URL:-http://localhost:4201}"
export BIND_ADDR="0.0.0.0:8080"
export STORAGE_ROOT="$ROOT_DIR/data"
export FERRISGIT_BOOTSTRAP_ADMIN_USERNAME="${FERRISGIT_BOOTSTRAP_ADMIN_USERNAME:-admin}"
export FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD="${FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD:-admin123}"
export RUST_LOG="${RUST_LOG:-info}"

echo "==> Starting Postgres (docker-compose, port 5435)"
docker compose up -d --wait postgres

echo "==> Running migrations"
sqlx migrate run --source migrations

echo "==> Starting backend (cargo run -p ferrisgit-api, port 8080)"
cargo run -p ferrisgit-api &
BACKEND_PID=$!

CLEANED_UP=0
cleanup() {
  [ "$CLEANED_UP" = 1 ] && return
  CLEANED_UP=1
  echo
  echo "==> Stopping backend"
  kill "$BACKEND_PID" 2>/dev/null || true
  wait "$BACKEND_PID" 2>/dev/null || true
  exit 0
}
trap cleanup EXIT INT TERM

echo "==> Starting frontend (npm start, port 4201, hot reload)"
npm start --prefix frontend -- --port 4201
