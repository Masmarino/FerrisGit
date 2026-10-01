use sqlx::PgPool;
use uuid::Uuid;

/// Inserts a bare user row (no password worth guessing) and returns its id.
pub(crate) async fn seed_user(pool: &PgPool, username: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(username)
        .bind(format!("{username}@example.com"))
        .bind("not-a-real-hash")
        .execute(pool)
        .await
        .unwrap();
    id
}
