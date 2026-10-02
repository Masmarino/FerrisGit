use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// A bare user row, no password worth guessing.
pub(crate) async fn seed_user(pool: &PgPool, username: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)",
        id,
        username,
        format!("{username}@example.com"),
        "not-a-real-hash"
    )
    .execute(pool)
    .await
    .unwrap();
    id
}

pub(crate) async fn seed_repository(pool: &PgPool, owner_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO repositories (id, owner_id, name, disk_path) VALUES ($1, $2, $3, $4)",
        id,
        owner_id,
        name,
        format!("{name}.git")
    )
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A user `username` owning a private repository named "hello"; returns `(owner_id, repository_id)`.
pub(crate) async fn seed_owned_repository(pool: &PgPool, username: &str) -> (Uuid, Uuid) {
    let owner_id = seed_user(pool, username).await;
    let repository_id = seed_repository(pool, owner_id, "hello").await;
    (owner_id, repository_id)
}

/// Postgres keeps microseconds: truncate so a round-trip compares equal.
pub(crate) fn micros(at: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp_micros(at.timestamp_micros()).unwrap()
}

pub(crate) fn in_hours(hours: i64) -> DateTime<Utc> {
    micros(Utc::now() + Duration::hours(hours))
}
