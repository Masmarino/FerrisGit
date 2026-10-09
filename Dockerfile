FROM --platform=$BUILDPLATFORM node:26-alpine3.24 AS frontend-build

WORKDIR /app/frontend

COPY frontend/package*.json ./

RUN npm ci

COPY frontend/ ./
# frontend/docs is a symlink to docs/, so the documentation pages end up in the app as static assets.
COPY docs/ ../docs/

RUN npm run build -- --configuration production

FROM rust:1.99.0-alpine3.24 AS backend-build

WORKDIR /app

RUN apk add --no-cache pkgconfig build-base git openssl-dev openssl-libs-static

# OpenSSL is linked statically (WebAuthn needs it) so the musl binary runs on plain alpine.
ENV OPENSSL_STATIC=1

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .sqlx ./.sqlx
COPY crates ./crates
COPY migrations ./migrations

ENV SQLX_OFFLINE=true

RUN cargo build --release -p ferrisgit-api --locked

FROM alpine:3.24 AS runtime

WORKDIR /app

# git http-backend comes from alpine's git-daemon package.
RUN apk upgrade --no-cache && apk add --no-cache git git-daemon ca-certificates

COPY --from=backend-build /app/target/release/ferrisgit-api ./ferrisgit-api
COPY --from=frontend-build /app/frontend/dist/ferrisgit-web/browser ./static

ENV STATIC_DIR=/app/static

EXPOSE 8080

RUN addgroup -S ferrisgit && adduser -S -G ferrisgit -h /app -H ferrisgit \
    && mkdir -p /data \
    && chown -R ferrisgit:ferrisgit /app /data

USER ferrisgit

CMD ["./ferrisgit-api"]
