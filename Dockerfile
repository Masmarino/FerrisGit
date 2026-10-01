FROM --platform=$BUILDPLATFORM node:26-alpine3.24 AS frontend-build

WORKDIR /app/frontend

COPY frontend/package*.json ./

RUN npm ci

COPY frontend/ ./

RUN npm run build -- --configuration production

FROM rust:1.98.1-alpine3.24 AS backend-build

WORKDIR /app

RUN apk add --no-cache pkgconfig build-base git openssl-dev openssl-libs-static

ENV OPENSSL_STATIC=1

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .sqlx ./.sqlx
COPY crates ./crates
COPY migrations ./migrations

ENV SQLX_OFFLINE=true

RUN cargo build --release -p ferrisgit-api --locked

FROM alpine:3.24 AS runtime

WORKDIR /app

RUN apk add --no-cache git git-daemon ca-certificates

COPY --from=backend-build /app/target/release/ferrisgit-api ./ferrisgit-api
COPY --from=frontend-build /app/frontend/dist/ferrisgit-web/browser ./static

ENV STATIC_DIR=/app/static

EXPOSE 8080

RUN addgroup -S ferrisgit && adduser -S -G ferrisgit -h /app -H ferrisgit \
    && mkdir -p /data \
    && chown -R ferrisgit:ferrisgit /app /data

USER ferrisgit

CMD ["./ferrisgit-api"]
