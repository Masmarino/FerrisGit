// Shared harness of the flow tests. `spawn_app` starts a server with MFA off for the tests that are not about MFA;
// `spawn_server` enforces MFA with a recording mailer, and `Device` is a real software authenticator that verifies the
// origin and signs like a browser. `http` and `git` hold the bare-REST and git helpers.

#![allow(dead_code, unused_imports)]

pub mod git;
pub mod http;
pub mod mail;

pub use mail::{Mail, RecordingEmail, settled_attempts, wait_for_attempts};

use async_trait::async_trait;
use ferrisgit_api::client_ip::parse_cidr_list;
use ferrisgit_api::{build_router, config::Config, state::AppState};
use ferrisgit_application::mailer::Mailer;
use ferrisgit_application::mfa_crypto::generate_code_at;
use ferrisgit_application::passkey_ceremonies::PasskeyCeremonies;
use ferrisgit_application::use_cases::bootstrap_admin::BootstrapAdminUseCase;
use ferrisgit_application::use_cases::mfa::MfaService;
use ferrisgit_application::use_cases::passkeys::{PasskeyService, build_webauthn};
use ferrisgit_domain::email::EmailPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::NewUser;
use ferrisgit_domain::webauthn::{StoredPasskey, WebauthnCredentialPort};
use ferrisgit_infrastructure::postgres::backup_code_store::PostgresBackupCodeStore;
use ferrisgit_infrastructure::postgres::webauthn_credential_store::PostgresWebauthnCredentialStore;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use url::Url;
use uuid::Uuid;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_rs::prelude::{CreationChallengeResponse, RequestChallengeResponse};

pub const ADMIN_PASSWORD: &str = "adminpassword123";
pub const USER_PASSWORD: &str = "password12345";
pub const WRONG_PASSWORD: &str = "definitely-not-the-password";
/// The browser origin of the test deployment: the test `Config.public_url`. The relying-party id is `localhost`.
pub const ORIGIN: &str = "http://localhost:4200";
/// What the app shell (`index.html`) of a test server contains.
pub const SHELL: &str = "<html>spa</html>";
pub const PASSKEY_METHOD_LABEL: &str = "une clé d'accès (passkey)";

pub struct Device {
    authenticator: WebauthnAuthenticator<SoftPasskey>,
    origin: Url,
}

impl Device {
    pub fn new() -> Self {
        Self::on(ORIGIN)
    }

    pub fn on(origin: &str) -> Self {
        Self {
            authenticator: WebauthnAuthenticator::new(SoftPasskey::new(true)),
            origin: Url::parse(origin).unwrap(),
        }
    }

    pub fn register(&mut self, public_key: &Value) -> Value {
        let challenge: CreationChallengeResponse =
            serde_json::from_value(json!({ "publicKey": public_key }))
                .expect("a creation challenge");
        let credential = self
            .authenticator
            .do_registration(self.origin.clone(), challenge)
            .expect("the soft authenticator registers");
        serde_json::to_value(&credential).unwrap()
    }

    /// `None` when this device holds none of the credentials the challenge allows (a real browser would show an error).
    pub fn try_assert(&mut self, public_key: &Value) -> Option<Value> {
        let challenge: RequestChallengeResponse =
            serde_json::from_value(json!({ "publicKey": public_key }))
                .expect("a request challenge");
        let credential = self
            .authenticator
            .do_authentication(self.origin.clone(), challenge)
            .ok()?;
        Some(serde_json::to_value(&credential).unwrap())
    }

    pub fn assert(&mut self, public_key: &Value) -> Value {
        self.try_assert(public_key)
            .expect("the soft authenticator signs the challenge")
    }
}

/// Makes a device that is not registered for the user sign a challenge.
pub fn challenge_for_credential(public_key: &Value, credential_id: &str) -> Value {
    let mut rewritten = public_key.clone();
    rewritten["allowCredentials"] = json!([{ "type": "public-key", "id": credential_id }]);
    rewritten
}

pub fn credential_id_of(credential: &Value) -> String {
    credential["id"]
        .as_str()
        .expect("a credential id")
        .to_string()
}

/// Wraps the real store and can make its `insert` report a duplicate credential id, its `delete` fail, or its
/// counter write find no row, on demand.
pub struct Sabotage {
    inner: Arc<dyn WebauthnCredentialPort>,
    pub refuse_insert: AtomicBool,
    pub fail_delete: AtomicBool,
    /// The row is gone by the time the counter is written (deleted from another session mid-login).
    pub vanish_on_update: AtomicBool,
}

#[async_trait]
impl WebauthnCredentialPort for Sabotage {
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<StoredPasskey>, DomainError> {
        self.inner.list_for_user(user_id).await
    }
    async fn count_for_user(&self, user_id: Uuid) -> Result<i64, DomainError> {
        self.inner.count_for_user(user_id).await
    }
    async fn insert(&self, passkey: &StoredPasskey) -> Result<bool, DomainError> {
        if self.refuse_insert.load(Ordering::SeqCst) {
            return Ok(false);
        }
        self.inner.insert(passkey).await
    }
    async fn update_after_authentication(
        &self,
        id: Uuid,
        passkey_json: &str,
    ) -> Result<(), DomainError> {
        if self.vanish_on_update.load(Ordering::SeqCst) {
            return Err(DomainError::NotFound("passkey".to_string()));
        }
        self.inner
            .update_after_authentication(id, passkey_json)
            .await
    }
    async fn delete(&self, id: Uuid, user_id: Uuid) -> Result<bool, DomainError> {
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(DomainError::Infrastructure("injected failure".to_string()));
        }
        self.inner.delete(id, user_id).await
    }
    async fn delete_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.inner.delete_all_for_user(user_id).await
    }
    async fn user_ids_with_passkeys(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError> {
        self.inner.user_ids_with_passkeys(user_ids).await
    }
}

#[derive(Clone)]
pub struct Options {
    pub public_url: String,
    pub trusted_proxy_cidrs: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            public_url: ORIGIN.to_string(),
            trusted_proxy_cidrs: String::new(),
        }
    }
}

pub struct Server {
    pub addr: SocketAddr,
    pub state: AppState,
    pub pool: PgPool,
    pub mailer: Arc<RecordingEmail>,
    pub client: reqwest::Client,
}

pub struct Enrolled {
    pub id: String,
    pub username: String,
    pub session: String,
    pub backup_codes: Vec<String>,
    pub secret: Option<String>,
}

pub async fn spawn_server(pool: PgPool) -> Server {
    spawn_with(pool, Options::default(), |_| {}).await
}

pub async fn spawn_sabotaged(pool: PgPool) -> (Server, Arc<Sabotage>) {
    let sabotage = Arc::new(Sabotage {
        inner: Arc::new(PostgresWebauthnCredentialStore::new(pool.clone())),
        refuse_insert: AtomicBool::new(false),
        fail_delete: AtomicBool::new(false),
        vanish_on_update: AtomicBool::new(false),
    });
    let for_state = sabotage.clone();
    let backup_pool = pool.clone();
    let server = spawn_with(pool, Options::default(), move |state| {
        state.mfa = Arc::new(MfaService::new(
            state.totp_credentials.clone(),
            Arc::new(PostgresBackupCodeStore::new(backup_pool)),
            state.users.clone(),
            state.hasher.clone(),
            for_state.clone(),
        ));
        state.passkeys = Arc::new(PasskeyService::new(
            build_webauthn(&state.config.public_url).map(Arc::new),
            for_state,
            Arc::new(PasskeyCeremonies::new()),
            state.users.clone(),
        ));
    })
    .await;
    (server, sabotage)
}

/// A server with no per-user MFA budget (a window of zero renews it on every call), for the tests that need many
/// attempts of one user. The per-IP throttles stay as they are.
pub async fn spawn_unthrottled(pool: PgPool) -> Server {
    spawn_with(pool, Options::default(), |state| {
        state.mfa_limiter = Arc::new(
            ferrisgit_api::mfa_rate_limiter::MfaRateLimiter::with_window(std::time::Duration::ZERO),
        );
    })
    .await
}

pub async fn spawn_with_instantly_expiring_ceremonies(pool: PgPool) -> Server {
    spawn_with(pool, Options::default(), |state| {
        state.passkeys = Arc::new(PasskeyService::new(
            build_webauthn(&state.config.public_url).map(Arc::new),
            state.passkey_credentials.clone(),
            Arc::new(PasskeyCeremonies::with_limits(
                chrono::Duration::zero(),
                10_000,
                5,
            )),
            state.users.clone(),
        ));
    })
    .await
}

pub async fn spawn_with(
    pool: PgPool,
    options: Options,
    customise: impl FnOnce(&mut AppState),
) -> Server {
    let mailer = RecordingEmail::new();
    let recording = mailer.clone();
    let started = start(pool.clone(), options, move |state| {
        assert!(
            state.mfa_enforced,
            "AppState::new must enforce MFA: production has no way to turn it off"
        );
        state.mailer = Arc::new(Mailer::new(recording));
        customise(state);
    })
    .await;
    Server {
        addr: started.addr,
        state: started.state,
        pool,
        mailer,
        client: reqwest::Client::new(),
    }
}

/// A server for the tests that are not about MFA: login hands out a plain session, and the mailer is the real one.
pub struct App {
    pub addr: SocketAddr,
    pub state: AppState,
    pub pool: PgPool,
    pub storage_root: PathBuf,
    pub static_dir: PathBuf,
}

pub async fn spawn_app(pool: PgPool) -> App {
    spawn_app_with(pool, Options::default(), |_| {}).await
}

pub async fn spawn_app_with(
    pool: PgPool,
    options: Options,
    customise: impl FnOnce(&mut AppState),
) -> App {
    let started = start(pool.clone(), options, |state| {
        state.mfa_enforced = false;
        customise(state);
    })
    .await;
    App {
        addr: started.addr,
        state: started.state,
        pool,
        storage_root: started.storage_root,
        static_dir: started.static_dir,
    }
}

struct Started {
    addr: SocketAddr,
    state: AppState,
    storage_root: PathBuf,
    static_dir: PathBuf,
}

/// The storage and static directories are leaked on purpose: they must outlive the spawned `axum::serve` task.
async fn start(pool: PgPool, options: Options, customise: impl FnOnce(&mut AppState)) -> Started {
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let storage_dir = tempfile::tempdir().unwrap().keep();
    let static_dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(static_dir.join("index.html"), SHELL).unwrap();

    let config = Config {
        database_url: String::new(),
        jwt_secret: "test-secret-that-is-at-least-32-characters-long".to_string(),
        storage_root: storage_dir.to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: static_dir.to_string_lossy().to_string(),
        bootstrap_admin_username: Some("admin".to_string()),
        bootstrap_admin_password: Some(ADMIN_PASSWORD.to_string()),
        settings_encryption_key: [b'k'; 32],
        public_url: options.public_url,
        trusted_proxy_cidrs: parse_cidr_list(&options.trusted_proxy_cidrs).unwrap(),
    };

    let mut state = AppState::new(pool, config.clone()).await;
    customise(&mut state);
    BootstrapAdminUseCase::new(state.users.clone(), state.hasher.clone())
        .execute(
            config.bootstrap_admin_username.clone(),
            config.bootstrap_admin_password.clone(),
        )
        .await
        .unwrap();

    let app = build_router(state.clone(), &static_dir);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    Started {
        addr,
        state,
        storage_root: storage_dir,
        static_dir,
    }
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn totp_code(secret: &str) -> String {
    generate_code_at(secret, now_unix())
}

/// The code of the following 30 s step: accepted with the server's skew, and newer than the one an enrolment spent.
pub fn next_step_code(secret: &str) -> String {
    generate_code_at(secret, now_unix() + 30)
}

/// A six-digit code that no step the server accepts would produce.
pub fn wrong_code(secret: &str) -> String {
    let valid: Vec<String> = (-3i64..=3)
        .map(|step| generate_code_at(secret, (now_unix() as i64 + step * 30) as u64))
        .collect();
    (0..1_000_000)
        .map(|n| format!("{n:06}"))
        .find(|candidate| !valid.contains(candidate))
        .unwrap()
}

pub fn codes_of(body: &Value) -> Vec<String> {
    body["backupCodes"]
        .as_array()
        .expect("backupCodes")
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect()
}

pub fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

pub fn key_set(names: &[&str]) -> Vec<String> {
    let mut sorted: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    sorted.sort();
    sorted
}

impl Server {
    pub fn url(&self, path: &str) -> String {
        format!("http://{}/api{path}", self.addr)
    }

    pub async fn post(&self, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    pub async fn post_as(&self, session: &str, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .bearer_auth(session)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_as(&self, session: &str, path: &str) -> reqwest::Response {
        self.client
            .get(self.url(path))
            .bearer_auth(session)
            .send()
            .await
            .unwrap()
    }

    pub async fn post_raw(&self, path: &str, body: Vec<u8>) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn create_user(&self, username: &str, email: &str) -> String {
        let password_hash = self.state.hasher.hash(USER_PASSWORD).unwrap();
        let user = self
            .state
            .users
            .create(NewUser {
                username: username.to_string(),
                email: email.to_string(),
                password_hash,
                is_admin: false,
            })
            .await
            .unwrap();
        user.id.to_string()
    }

    /// Renews the budget without depending on wall-clock time.
    pub fn reset_budget(&self, user_id: &str) {
        self.state.mfa_limiter.reset(user_id.parse().unwrap());
    }

    pub async fn user_id(&self, username: &str) -> String {
        self.state
            .users
            .find_by_username(username)
            .await
            .unwrap()
            .unwrap()
            .id
            .to_string()
    }

    pub async fn new_user(&self, username: &str) -> String {
        self.create_user(username, &format!("{username}@example.com"))
            .await
    }

    pub async fn login(&self, username: &str, password: &str) -> Value {
        let res = self
            .post(
                "/auth/login",
                json!({ "username": username, "password": password }),
            )
            .await;
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(
            body["token"].is_null(),
            "a local account never gets a session from the password step: {body}"
        );
        body
    }

    pub async fn mfa_token(&self, username: &str, password: &str) -> String {
        self.login(username, password).await["mfaToken"]
            .as_str()
            .expect("an mfaToken")
            .to_string()
    }

    /// Minted directly, so the per-IP login throttle stays out of the way.
    pub async fn pending_token(&self, user_id: &str) -> String {
        let id: Uuid = user_id.parse().unwrap();
        let epoch = self.state.users.get_token_epoch(id).await.unwrap();
        self.state.mfa_pending.issue(id, epoch).unwrap()
    }

    pub async fn setup_start(&self, mfa_token: &str) -> reqwest::Response {
        self.post(
            "/auth/mfa/setup/passkey/start",
            json!({ "mfaToken": mfa_token }),
        )
        .await
    }

    pub async fn setup_finish(
        &self,
        mfa_token: &str,
        challenge_id: &str,
        credential: &Value,
        name: &str,
    ) -> reqwest::Response {
        self.post("/auth/mfa/setup/passkey/finish", json!({ "mfaToken": mfa_token, "challengeId": challenge_id, "credential": credential, "name": name })).await
    }

    pub async fn passkey_start(&self, mfa_token: &str) -> reqwest::Response {
        self.post("/auth/mfa/passkey/start", json!({ "mfaToken": mfa_token }))
            .await
    }

    pub async fn passkey_finish(
        &self,
        mfa_token: &str,
        challenge_id: &str,
        credential: &Value,
    ) -> reqwest::Response {
        self.post(
            "/auth/mfa/passkey/finish",
            json!({ "mfaToken": mfa_token, "challengeId": challenge_id, "credential": credential }),
        )
        .await
    }

    pub async fn start_login_challenge(&self, mfa_token: &str) -> (String, Value) {
        let res = self.passkey_start(mfa_token).await;
        assert_eq!(res.status(), 200, "the login challenge must start");
        let body: Value = res.json().await.unwrap();
        (
            body["challengeId"].as_str().unwrap().to_string(),
            body["publicKey"].clone(),
        )
    }

    pub async fn start_setup_challenge(&self, mfa_token: &str) -> (String, Value) {
        let res = self.setup_start(mfa_token).await;
        assert_eq!(res.status(), 200, "the setup challenge must start");
        let body: Value = res.json().await.unwrap();
        (
            body["challengeId"].as_str().unwrap().to_string(),
            body["publicKey"].clone(),
        )
    }

    pub async fn first_setup_with_passkey(
        &self,
        device: &mut Device,
        username: &str,
        password: &str,
        name: &str,
    ) -> Enrolled {
        let body = self.login(username, password).await;
        assert_eq!(body["mfaSetupRequired"], json!(true));
        let mfa_token = body["mfaToken"].as_str().unwrap().to_string();
        let (challenge_id, public_key) = self.start_setup_challenge(&mfa_token).await;
        let credential = device.register(&public_key);
        let res = self
            .setup_finish(&mfa_token, &challenge_id, &credential, name)
            .await;
        assert_eq!(res.status(), 200, "the passkey setup must complete");
        let body: Value = res.json().await.unwrap();
        Enrolled {
            id: self.user_id(username).await,
            username: username.to_string(),
            session: body["token"].as_str().unwrap().to_string(),
            backup_codes: codes_of(&body),
            secret: None,
        }
    }

    pub async fn first_setup_with_totp(&self, username: &str, password: &str) -> Enrolled {
        let mfa_token = self.mfa_token(username, password).await;
        let enrol: Value = self
            .post(
                "/auth/mfa/setup/totp/enroll",
                json!({ "mfaToken": mfa_token }),
            )
            .await
            .json()
            .await
            .unwrap();
        let secret = enrol["secret"].as_str().unwrap().to_string();
        let res = self
            .post(
                "/auth/mfa/setup/totp/confirm",
                json!({ "mfaToken": mfa_token, "code": totp_code(&secret) }),
            )
            .await;
        assert_eq!(res.status(), 200, "the TOTP setup must complete");
        let body: Value = res.json().await.unwrap();
        Enrolled {
            id: self.user_id(username).await,
            username: username.to_string(),
            session: body["token"].as_str().unwrap().to_string(),
            backup_codes: codes_of(&body),
            secret: Some(secret),
        }
    }

    pub async fn login_with_passkey(
        &self,
        device: &mut Device,
        username: &str,
        password: &str,
    ) -> String {
        let mfa_token = self.mfa_token(username, password).await;
        let (challenge_id, public_key) = self.start_login_challenge(&mfa_token).await;
        let credential = device.assert(&public_key);
        let res = self
            .passkey_finish(&mfa_token, &challenge_id, &credential)
            .await;
        assert_eq!(res.status(), 200, "the passkey login must complete");
        res.json::<Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    }

    pub async fn me(&self, bearer: &str) -> reqwest::Response {
        self.get_as(bearer, "/auth/me").await
    }

    pub async fn status(&self, session: &str) -> Value {
        let res = self.get_as(session, "/me/mfa").await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    pub async fn register_start(&self, session: &str, password: &str) -> reqwest::Response {
        self.post_as(
            session,
            "/me/mfa/passkeys/register/start",
            json!({ "currentPassword": password }),
        )
        .await
    }

    pub async fn register_finish(
        &self,
        session: &str,
        challenge_id: &str,
        credential: &Value,
        name: &str,
    ) -> reqwest::Response {
        self.post_as(
            session,
            "/me/mfa/passkeys/register/finish",
            json!({ "challengeId": challenge_id, "credential": credential, "name": name }),
        )
        .await
    }

    pub async fn add_passkey(
        &self,
        session: &str,
        device: &mut Device,
        password: &str,
        name: &str,
    ) -> (String, Value) {
        let start = self.register_start(session, password).await;
        assert_eq!(start.status(), 200, "the registration must start");
        let start: Value = start.json().await.unwrap();
        let credential = device.register(&start["publicKey"]);
        let res = self
            .register_finish(
                session,
                start["challengeId"].as_str().unwrap(),
                &credential,
                name,
            )
            .await;
        assert_eq!(res.status(), 201, "the registration must finish");
        (
            res.json::<Value>().await.unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string(),
            credential,
        )
    }

    pub async fn delete_passkey(
        &self,
        session: &str,
        passkey_id: &str,
        password: &str,
    ) -> reqwest::Response {
        self.post_as(
            session,
            &format!("/me/mfa/passkeys/{passkey_id}/delete"),
            json!({ "currentPassword": password }),
        )
        .await
    }

    pub async fn disable_totp(&self, session: &str, password: &str) -> reqwest::Response {
        self.post_as(
            session,
            "/me/mfa/totp/disable",
            json!({ "currentPassword": password }),
        )
        .await
    }

    pub async fn admin_reset(&self, session: &str, user_id: &str) -> reqwest::Response {
        self.client
            .delete(self.url(&format!("/admin/users/{user_id}/mfa")))
            .bearer_auth(session)
            .send()
            .await
            .unwrap()
    }

    pub async fn admin_users(&self, session: &str) -> Value {
        let res = self.get_as(session, "/admin/users").await;
        assert_eq!(res.status(), 200);
        res.json().await.unwrap()
    }

    pub async fn passkey_rows(&self, user_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM webauthn_credentials WHERE user_id = $1::uuid",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    pub async fn passkey_state(&self, user_id: &str) -> Vec<(String, Option<String>)> {
        sqlx::query_as::<_, (String, Option<String>)>("SELECT passkey::text, last_used_at::text FROM webauthn_credentials WHERE user_id = $1::uuid ORDER BY created_at, id")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }

    pub async fn backup_code_rows(&self, user_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mfa_backup_codes WHERE user_id = $1::uuid",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    pub async fn backup_code_hashes(&self, user_id: &str) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT code_hash FROM mfa_backup_codes WHERE user_id = $1::uuid ORDER BY code_hash",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    pub async fn totp_rows(&self, user_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM totp_credentials WHERE user_id = $1::uuid",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    pub async fn security_events_of(&self, user_id: &str) -> Vec<String> {
        sqlx::query_scalar::<_, String>("SELECT event_type FROM domain_events WHERE aggregate_type = 'Security' AND aggregate_id = $1 ORDER BY version")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }

    pub async fn stored_credential_ids(&self, user_id: &str) -> Vec<String> {
        use base64::Engine;
        let rows = sqlx::query_scalar::<_, Vec<u8>>("SELECT credential_id FROM webauthn_credentials WHERE user_id = $1::uuid ORDER BY created_at, id").bind(user_id).fetch_all(&self.pool).await.unwrap();
        rows.iter()
            .map(|raw| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw))
            .collect()
    }
}

pub async fn assert_error(res: reqwest::Response, status: u16, message: &str) {
    assert_eq!(res.status().as_u16(), status);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body, json!({ "error": message }));
}
