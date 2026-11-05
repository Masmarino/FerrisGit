//! The real `fgit` binary and real `git` against a real server and database. The first two tests are what issue #72 calls
//! done: a person, then a script holding only a token, each opening a merge request from a branch, following its pipeline
//! and merging it from the terminal. The others go through every other command, so that the typed models meet real answers.

#[path = "../../ferrisgit-api/tests/common/mod.rs"]
#[allow(clippy::duplicate_mod)]
mod common;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use common::http::{get, login, post_empty, post_json};
use common::{ADMIN_PASSWORD, next_step_code};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

const ADMIN: &str = "adminpassword123";
const PIPELINE_FILE: &str =
    "stages: [test]\njobs:\n  unit:\n    stage: test\n    image: alpine\n    script: [\"true\"]\n";
const ALL_SCOPES: [&str; 5] = [
    "repositories:write",
    "issues:write",
    "merge-requests:write",
    "pipelines:write",
    "releases:write",
];

/// One machine: its own fgit configuration and Git configuration, and nothing inherited from the one running the tests.
struct Machine {
    home: tempfile::TempDir,
    envs: Vec<(String, String)>,
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn json(&self) -> Value {
        assert_eq!(self.code, 0, "stderr: {}", self.stderr);
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("stdout isn't JSON ({e}): {:?}", self.stdout))
    }

    fn ok(&self) -> &Self {
        assert_eq!(
            self.code, 0,
            "stdout: {} stderr: {}",
            self.stdout, self.stderr
        );
        self
    }
}

impl Machine {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("gitconfig"),
            "[user]\n\temail = t@t.com\n\tname = t\n[init]\n\tdefaultBranch = main\n",
        )
        .unwrap();
        Self { home, envs: vec![] }
    }

    fn with_env(mut self, key: &str, value: &str) -> Self {
        self.envs.push((key.to_string(), value.to_string()));
        self
    }

    fn command(&self, program: &str, args: &[&str], cwd: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .env_remove("FERRISGIT_TOKEN")
            .env_remove("FERRISGIT_HOST")
            .env("FERRISGIT_CONFIG_DIR", self.home.path().join("fgit"))
            .env("HOME", self.home.path())
            .env("GIT_CONFIG_GLOBAL", self.home.path().join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .envs(self.envs.iter().map(|(k, v)| (k, v)))
            .kill_on_drop(true);
        command
    }

    fn fgit_command(&self, args: &[&str], cwd: &Path) -> Command {
        self.command(env!("CARGO_BIN_EXE_fgit"), args, cwd)
    }

    /// Never a terminal, and an empty stdin: a prompt would fail, not hang.
    async fn fgit(&self, cwd: &Path, args: &[&str]) -> Run {
        let output = self
            .fgit_command(args, cwd)
            .stdin(Stdio::null())
            .output()
            .await
            .unwrap();
        Run {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }
    }

    async fn fgit_with_stdin(&self, cwd: &Path, args: &[&str], stdin: &str) -> Run {
        let mut child = self
            .fgit_command(args, cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(stdin.as_bytes()).await.unwrap();
        drop(pipe);
        let output = child.wait_with_output().await.unwrap();
        Run {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }
    }

    /// With this machine's Git configuration, so that the helper `fgit repo clone` set up is the only way in.
    async fn git(&self, cwd: &Path, args: &[&str]) -> String {
        let output = self
            .command("git", args, cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    async fn commit(&self, work: &Path, file: &str, content: &str) {
        std::fs::write(work.join(file), content).unwrap();
        self.git(work, &["add", "."]).await;
        self.git(work, &["commit", "-q", "-m", file]).await;
    }
}

/// A plain runner, over the same HTTP as the real one: claims whatever is pending until nothing is, and logs a line per job.
async fn run_pending_jobs(addr: SocketAddr, runner_token: &str, status: &str) {
    let client = reqwest::Client::new();
    for _ in 0..50 {
        let claimed = post_empty(&client, addr, runner_token, "/runner/jobs/claim").await;
        if claimed.status() == 204 {
            return;
        }
        assert_eq!(claimed.status(), 200);
        let job: Value = claimed.json().await.unwrap();
        let id = job["id"].as_str().unwrap();
        let logged = common::http::post(
            &client,
            addr,
            runner_token,
            &format!("/runner/jobs/{id}/logs"),
            &json!({ "chunk": format!("hello from {}\n", job["name"].as_str().unwrap()) }),
        )
        .await;
        assert!(logged.status().is_success());
        let reported = common::http::post(
            &client,
            addr,
            runner_token,
            &format!("/runner/jobs/{id}/result"),
            &json!({ "status": status }),
        )
        .await;
        assert!(reported.status().is_success());
    }
    panic!("the jobs never ran out");
}

async fn new_runner(addr: SocketAddr, session: &str) -> String {
    let client = reqwest::Client::new();
    let runner: Value = post_json(
        &client,
        addr,
        session,
        "/admin/runners",
        &json!({ "name": "test", "tags": [] }),
    )
    .await;
    runner["token"].as_str().unwrap().to_string()
}

async fn new_token(addr: SocketAddr, session: &str, scopes: &[&str]) -> String {
    let client = reqwest::Client::new();
    let created: Value = post_json(
        &client,
        addr,
        session,
        "/access-tokens",
        &json!({ "name": "script", "scopes": scopes, "expiresInDays": 1 }),
    )
    .await;
    created["token"].as_str().unwrap().to_string()
}

/// The scenario of the issue, whoever runs it. `machine` is already authenticated, one way or another.
async fn create_a_merge_request_follow_its_pipeline_and_merge_it(
    machine: &Machine,
    addr: SocketAddr,
    runner_token: &str,
) {
    let parent = tempfile::tempdir().unwrap();
    let repo_arg = format!("{addr}/admin/hello");
    let hostname = format!("http://{addr}");

    let created = machine
        .fgit(
            parent.path(),
            &["repo", "create", "hello", "--hostname", &hostname, "--json"],
        )
        .await
        .json();
    assert_eq!(created["path"], json!(["admin", "hello"]));
    assert_eq!(created["visibility"], "private");

    machine
        .fgit(parent.path(), &["repo", "clone", &repo_arg, "work"])
        .await
        .ok();
    let work: PathBuf = parent.path().join("work");

    // Plain git from here: the clone's configuration has to be what lets it through.
    machine
        .commit(&work, ".ferrisgit-ci.yml", PIPELINE_FILE)
        .await;
    machine
        .git(&work, &["push", "-q", "origin", "HEAD:main"])
        .await;
    machine
        .git(&work, &["checkout", "-q", "-b", "feature"])
        .await;
    machine.commit(&work, "feature.txt", "one\n").await;

    // No --repo from here on: the repository and the instance come from the remote.
    let not_pushed = machine
        .fgit(&work, &["mr", "create", "--title", "Feature", "--json"])
        .await;
    assert_eq!(not_pushed.code, 5, "stderr: {}", not_pushed.stderr);
    assert!(
        not_pushed.stderr.contains("--push"),
        "{}",
        not_pushed.stderr
    );

    let opened = machine
        .fgit(
            &work,
            &["mr", "create", "--title", "Feature", "--push", "--json"],
        )
        .await
        .json();
    assert_eq!(opened["sourceBranch"], "feature");
    assert_eq!(opened["targetBranch"], "main");
    assert_eq!(opened["status"], "open");
    let mr_id = opened["id"].as_str().unwrap().to_string();

    // Without an id: the open merge request of the current branch.
    let viewed = machine.fgit(&work, &["mr", "view", "--json"]).await.json();
    assert_eq!(viewed["id"], mr_id.as_str());
    let by_start = machine
        .fgit(&work, &["mr", "view", &mr_id[..8], "--jq", ".title"])
        .await;
    assert_eq!(by_start.ok().stdout, "Feature");

    let listed = machine.fgit(&work, &["mr", "list", "--jq", "length"]).await;
    assert_eq!(listed.ok().stdout, "1");

    let diff = machine.fgit(&work, &["mr", "diff"]).await;
    assert!(diff.ok().stdout.contains("+one"), "{}", diff.stdout);
    assert!(diff.stdout.contains("feature.txt"), "{}", diff.stdout);

    // Follows the logs while the job runs, then says whether it can go on.
    let mut follow = machine
        .fgit_command(
            &[
                "pipeline", "logs", "--mr", &mr_id, "--job", "unit", "--follow",
            ],
            &work,
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    run_pending_jobs(addr, runner_token, "success").await;
    let followed = wait_for(&mut follow).await;
    assert_eq!(followed.code, 0, "stderr: {}", followed.stderr);
    assert!(
        followed.stdout.contains("hello from unit"),
        "{}",
        followed.stdout
    );

    let watched = machine
        .fgit(
            &work,
            &["pipeline", "view", "--mr", &mr_id, "--watch", "--json"],
        )
        .await
        .json();
    assert_eq!(watched["status"], "success");
    assert_eq!(watched["jobs"][0]["name"], "unit");

    let refused = machine.fgit(&work, &["mr", "merge"]).await;
    assert_eq!(refused.code, 2, "merging takes --yes without a terminal");
    assert!(refused.stderr.contains("--yes"), "{}", refused.stderr);

    let merged = machine
        .fgit(&work, &["mr", "merge", "--yes", "--json"])
        .await
        .json();
    assert_eq!(merged["status"], "merged");
    assert!(merged["mergeCommitSha"].is_string());

    let gone = machine.fgit(&work, &["mr", "view", "--json"]).await;
    assert_eq!(gone.code, 4, "no open merge request is left on this branch");
}

async fn wait_for(child: &mut Child) -> Run {
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (a, b) = tokio::join!(
        tokio::io::AsyncReadExt::read_to_end(&mut stdout, &mut out),
        tokio::io::AsyncReadExt::read_to_end(&mut stderr, &mut err),
    );
    a.unwrap();
    b.unwrap();
    let status = child.wait().await.unwrap();
    Run {
        code: status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out).trim().to_string(),
        stderr: String::from_utf8_lossy(&err).trim().to_string(),
    }
}

#[sqlx::test]
async fn a_person_signs_in_opens_follows_and_merges_from_the_terminal(pool: PgPool) {
    let server = common::spawn_server(pool).await;
    let addr = server.addr;
    let admin = server.first_setup_with_totp("admin", ADMIN_PASSWORD).await;
    let machine = Machine::new();
    let cwd = tempfile::tempdir().unwrap();
    let hostname = format!("http://{addr}");

    // --web because tests have no terminal. The code shows up on stderr, and someone confirms it in the web interface.
    let mut signing_in = machine
        .fgit_command(
            &[
                "login",
                "--web",
                "--hostname",
                &hostname,
                "--insecure-storage",
                "--json",
            ],
            cwd.path(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(signing_in.stderr.take().unwrap()).lines();
    let user_code = loop {
        let line = lines.next_line().await.unwrap().expect("a code on stderr");
        if let Some(code) = line.strip_prefix("Confirm this code in your browser: ") {
            break code.to_string();
        }
    };
    let approved = common::http::post(
        &server.client,
        addr,
        &admin.session,
        &format!("/auth/device/{user_code}/approve"),
        &json!({ "code": next_step_code(admin.secret.as_ref().unwrap()) }),
    )
    .await;
    assert_eq!(approved.status(), 204);
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    let output = signing_in.wait_with_output().await.unwrap();
    assert!(output.status.success());
    let signed_in: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(signed_in["username"], "admin");
    assert_eq!(signed_in["host"], hostname.as_str());
    assert_eq!(signed_in["source"], "file");
    assert!(
        !signed_in["scopes"]
            .as_array()
            .unwrap()
            .contains(&json!("admin:write")),
        "signing in doesn't ask for admin by default"
    );

    let status = machine
        .fgit(cwd.path(), &["auth", "status", "--json"])
        .await
        .json();
    assert_eq!(status[0]["username"], "admin");
    assert_eq!(status[0]["valid"], true);

    let runner = new_runner(addr, &admin.session).await;
    create_a_merge_request_follow_its_pipeline_and_merge_it(&machine, addr, &runner).await;

    machine
        .fgit(cwd.path(), &["logout", "--hostname", &hostname])
        .await
        .ok();
    assert_eq!(
        machine
            .fgit(cwd.path(), &["repo", "list", "--hostname", &hostname])
            .await
            .code,
        3
    );
}

#[sqlx::test]
async fn a_script_with_only_a_token_does_the_same_with_no_prompt(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    let runner = new_runner(addr, &session).await;
    let token = new_token(addr, &session, &ALL_SCOPES).await;

    let machine = Machine::new().with_env("FERRISGIT_TOKEN", &token);

    create_a_merge_request_follow_its_pipeline_and_merge_it(&machine, addr, &runner).await;

    assert!(
        !machine.home.path().join("fgit").join("hosts.json").exists(),
        "a token from the environment stores nothing"
    );
}

#[sqlx::test]
async fn results_are_on_stdout_and_messages_on_stderr(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    let token = new_token(addr, &session, &ALL_SCOPES).await;
    let machine = Machine::new().with_env("FERRISGIT_TOKEN", &token);
    let cwd = tempfile::tempdir().unwrap();

    let created = machine
        .fgit(
            cwd.path(),
            &[
                "repo",
                "create",
                "hello",
                "--hostname",
                &format!("http://{addr}"),
                "--jq",
                ".name",
            ],
        )
        .await;

    assert_eq!(created.ok().stdout, "hello");
    assert!(created.stderr.contains("Created"), "{}", created.stderr);
    assert!(
        !created.stdout.contains('\x1b'),
        "no colour outside a terminal"
    );
    assert!(
        !created.stderr.contains('\x1b'),
        "no colour outside a terminal"
    );
}

#[sqlx::test]
async fn each_kind_of_failure_has_its_exit_code(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    let hostname = format!("http://{addr}");
    let cwd = tempfile::tempdir().unwrap();
    let repo = format!("{addr}/admin/hello");
    let list_mrs = ["mr", "list", "--repo", repo.as_str()];

    // Usage.
    let nobody = Machine::new();
    assert_eq!(nobody.fgit(cwd.path(), &["mr", "nonsense"]).await.code, 2);
    assert_eq!(
        nobody
            .fgit(
                cwd.path(),
                &["repo", "list", "--jq", ".[", "--hostname", &hostname]
            )
            .await
            .code,
        2,
        "a bad filter is caught before anything runs"
    );
    assert_eq!(
        nobody
            .fgit(cwd.path(), &["login", "--hostname", &hostname])
            .await
            .code,
        2,
        "signing in doesn't wait for anyone without a terminal"
    );

    // Authentication.
    assert_eq!(nobody.fgit(cwd.path(), &list_mrs).await.code, 3);
    let forged = Machine::new().with_env("FERRISGIT_TOKEN", "fga_forged");
    assert_eq!(forged.fgit(cwd.path(), &list_mrs).await.code, 3);

    // Not found.
    let token = new_token(addr, &session, &ALL_SCOPES).await;
    let machine = Machine::new().with_env("FERRISGIT_TOKEN", &token);
    assert_eq!(machine.fgit(cwd.path(), &list_mrs).await.code, 4);

    // Usage again: a required value with nobody to ask for it, on a repository that exists.
    machine
        .fgit(
            cwd.path(),
            &["repo", "create", "hello", "--hostname", &hostname],
        )
        .await
        .ok();
    let missing = machine
        .fgit(cwd.path(), &["issue", "create", "--repo", &repo])
        .await;
    assert_eq!(missing.code, 2);
    assert!(missing.stderr.contains("--title"), "{}", missing.stderr);

    // Refused: the token's scope doesn't cover it.
    let narrow = new_token(addr, &session, &["repositories:read"]).await;
    let refused = Machine::new()
        .with_env("FERRISGIT_TOKEN", &narrow)
        .fgit(cwd.path(), &list_mrs)
        .await;
    assert_eq!(refused.code, 5);
    assert!(
        refused.stderr.contains("merge-requests:read"),
        "{}",
        refused.stderr
    );

    // Server: nobody answers there.
    let unreachable = machine
        .fgit(
            cwd.path(),
            &["repo", "list", "--hostname", "http://127.0.0.1:9"],
        )
        .await;
    assert_eq!(unreachable.code, 6, "stderr: {}", unreachable.stderr);
}

#[sqlx::test]
async fn a_token_from_the_environment_only_goes_to_the_instance_it_is_for(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let cwd = tempfile::tempdir().unwrap();
    let machine = Machine::new()
        .with_env("FERRISGIT_TOKEN", "fga_secret")
        .with_env("FERRISGIT_HOST", "https://git.example.com");

    let elsewhere = machine
        .fgit(
            cwd.path(),
            &["auth", "token", "--hostname", &format!("http://{addr}")],
        )
        .await;
    assert_eq!(elsewhere.code, 3);
    assert!(!elsewhere.stdout.contains("fga_secret"));

    let credential = |protocol: &str, host: &str| format!("protocol={protocol}\nhost={host}\n\n");
    let to_git = machine
        .fgit_with_stdin(
            cwd.path(),
            &["auth", "git-credential", "get"],
            &credential("https", "git.example.com"),
        )
        .await;
    assert!(
        to_git.ok().stdout.contains("password=fga_secret"),
        "{}",
        to_git.stdout
    );

    let wrong_host = machine
        .fgit_with_stdin(
            cwd.path(),
            &["auth", "git-credential", "get"],
            &credential("https", "evil.example.com"),
        )
        .await;
    assert_eq!(
        wrong_host.ok().stdout,
        "",
        "Git gets nothing for another host"
    );

    let downgraded = machine
        .fgit_with_stdin(
            cwd.path(),
            &["auth", "git-credential", "get"],
            &credential("http", "git.example.com"),
        )
        .await;
    assert_eq!(
        downgraded.ok().stdout,
        "",
        "nor for the same host over plain HTTP"
    );
}

#[sqlx::test]
async fn a_remote_over_plain_http_elsewhere_does_not_get_a_token(pool: PgPool) {
    let _server = common::spawn_app(pool).await;
    let work = tempfile::tempdir().unwrap();
    let machine = Machine::new().with_env("FERRISGIT_TOKEN", "fga_secret");
    machine.git(work.path(), &["init", "-q"]).await;
    machine
        .git(
            work.path(),
            &[
                "remote",
                "add",
                "origin",
                "http://git.example.com/acme/hello.git",
            ],
        )
        .await;

    let refused = machine.fgit(work.path(), &["mr", "list"]).await;

    assert_eq!(refused.code, 5, "stderr: {}", refused.stderr);
    assert!(refused.stderr.contains("plain HTTP"), "{}", refused.stderr);
}

/// A repository with a commit on main, through the CLI, and a machine signed in by token.
async fn repository_with_a_commit(
    addr: SocketAddr,
) -> (Machine, tempfile::TempDir, PathBuf, String) {
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    let token = new_token(addr, &session, &ALL_SCOPES).await;
    let machine = Machine::new().with_env("FERRISGIT_TOKEN", &token);
    let parent = tempfile::tempdir().unwrap();
    machine
        .fgit(
            parent.path(),
            &[
                "repo",
                "create",
                "hello",
                "--hostname",
                &format!("http://{addr}"),
            ],
        )
        .await
        .ok();
    let repo_arg = format!("{addr}/admin/hello");
    machine
        .fgit(parent.path(), &["repo", "clone", &repo_arg, "work"])
        .await
        .ok();
    let work = parent.path().join("work");
    machine.commit(&work, "README.md", "# hello\n").await;
    machine
        .git(&work, &["push", "-q", "origin", "HEAD:main"])
        .await;
    (machine, parent, work, session)
}

#[sqlx::test]
async fn the_repository_commands(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let (machine, parent, work, _) = repository_with_a_commit(addr).await;
    let hostname = format!("http://{addr}");

    let listed = machine
        .fgit(
            parent.path(),
            &["repo", "list", "--hostname", &hostname, "--jq", ".[0].name"],
        )
        .await;
    assert_eq!(listed.ok().stdout, "hello");

    let viewed = machine
        .fgit(&work, &["repo", "view", "--json"])
        .await
        .json();
    assert_eq!(viewed["name"], "hello");
    assert_eq!(viewed["role"], "owner");
    assert!(viewed["sizeBytes"].is_number(), "{viewed}");

    let named = machine
        .fgit(
            parent.path(),
            &[
                "repo",
                "view",
                &format!("{addr}/admin/hello"),
                "--jq",
                ".visibility",
            ],
        )
        .await;
    assert_eq!(named.ok().stdout, "private");

    assert_eq!(
        machine
            .fgit(
                parent.path(),
                &["repo", "view", &format!("{addr}/admin/none")]
            )
            .await
            .code,
        4
    );
}

#[sqlx::test]
async fn the_issue_commands(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let (machine, _parent, work, session) = repository_with_a_commit(addr).await;
    let run = |args: Vec<&'static str>| {
        let (machine, work) = (&machine, &work);
        async move { machine.fgit(work, &args).await }
    };

    let created = run(vec![
        "issue", "create", "-t", "Broken", "-b", "Details", "-k", "bug", "--json",
    ])
    .await
    .json();
    assert_eq!(created["number"], 1);
    assert_eq!(created["kind"], "bug");

    let from_stdin = machine
        .fgit_with_stdin(
            &work,
            &["issue", "create", "-t", "Plan", "-F", "-", "--json"],
            "From stdin",
        )
        .await
        .json();
    assert_eq!(from_stdin["description"], "From stdin");
    assert_eq!(from_stdin["kind"], "task");

    assert_eq!(
        run(vec!["issue", "list", "--jq", "length"])
            .await
            .ok()
            .stdout,
        "2"
    );

    // A partial edit keeps what it isn't changing.
    let edited = run(vec![
        "issue",
        "edit",
        "1",
        "--title",
        "Still broken",
        "--status",
        "in_progress",
        "--json",
    ])
    .await
    .json();
    assert_eq!(edited["title"], "Still broken");
    assert_eq!(edited["description"], "Details");
    assert_eq!(edited["kind"], "bug");
    assert_eq!(edited["status"], "in_progress");

    assert_eq!(
        run(vec!["issue", "edit", "1"]).await.code,
        2,
        "nothing to change"
    );

    let commented = run(vec!["issue", "comment", "1", "--body", "On it", "--json"])
        .await
        .json();
    assert_eq!(commented["body"], "On it");
    assert_eq!(commented["author"]["username"], "admin");
    let with_comments = run(vec!["issue", "view", "1", "--comments", "--json"])
        .await
        .json();
    assert_eq!(with_comments["comments"][0]["body"], "On it");
    assert_eq!(with_comments["commentCount"], 1);

    let assigned = run(vec!["issue", "assign", "1", "admin", "--json"])
        .await
        .json();
    assert_eq!(assigned["assignee"]["username"], "admin");
    assert_eq!(
        run(vec![
            "issue",
            "list",
            "--assignee",
            "admin",
            "--jq",
            "length"
        ])
        .await
        .ok()
        .stdout,
        "1"
    );
    let unassigned = run(vec!["issue", "assign", "1", "--unassign", "--json"])
        .await
        .json();
    assert!(unassigned["assignee"].is_null());
    assert_eq!(run(vec!["issue", "assign", "1", "nobody"]).await.code, 4);

    let client = reqwest::Client::new();
    let repo_id = run(vec!["repo", "view", "--jq", ".id"])
        .await
        .ok()
        .stdout
        .clone();
    let label: reqwest::Response = common::http::post(
        &client,
        addr,
        &session,
        &format!("/repositories/{repo_id}/labels"),
        &json!({ "name": "urgent", "color": "#ff0000" }),
    )
    .await;
    assert!(label.status().is_success());
    let milestone = common::http::post(
        &client,
        addr,
        &session,
        &format!("/repositories/{repo_id}/milestones"),
        &json!({ "title": "v1", "description": "" }),
    )
    .await;
    assert!(milestone.status().is_success());

    let labels = run(vec![
        "issue", "labels", "1", "--add", "urgent", "--jq", ".[].name",
    ])
    .await;
    assert_eq!(labels.ok().stdout, "urgent");
    assert_eq!(
        run(vec!["issue", "list", "--label", "urgent", "--jq", "length"])
            .await
            .ok()
            .stdout,
        "1"
    );
    let removed = run(vec!["issue", "labels", "1", "--remove", "urgent", "--json"])
        .await
        .json();
    assert_eq!(removed, json!([]));
    assert_eq!(
        run(vec!["issue", "labels", "1", "--add", "nope"])
            .await
            .code,
        4
    );

    let set = run(vec!["issue", "milestone", "1", "v1", "--json"])
        .await
        .json();
    assert!(set["milestoneId"].is_string());
    assert_eq!(set["title"], "Still broken", "the rest of the issue stays");
    let cleared = run(vec!["issue", "milestone", "1", "--clear", "--json"])
        .await
        .json();
    assert!(cleared["milestoneId"].is_null());

    let closed = run(vec!["issue", "close", "1", "--json"]).await.json();
    assert_eq!(closed["status"], "done");
    assert_eq!(
        run(vec!["issue", "list", "--jq", "length"])
            .await
            .ok()
            .stdout,
        "1",
        "open by default"
    );
    assert_eq!(
        run(vec![
            "issue",
            "list",
            "--state",
            "closed",
            "--jq",
            ".[0].number"
        ])
        .await
        .ok()
        .stdout,
        "1"
    );
    let reopened = run(vec!["issue", "reopen", "1", "--json"]).await.json();
    assert_eq!(reopened["status"], "todo");

    let human = run(vec!["issue", "view", "1"]).await;
    assert!(
        human.ok().stdout.contains("Still broken"),
        "{}",
        human.stdout
    );
    assert_eq!(run(vec!["issue", "view", "99"]).await.code, 4);
}

#[sqlx::test]
async fn the_rest_of_the_merge_request_commands(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let (machine, _parent, work, _) = repository_with_a_commit(addr).await;

    machine
        .git(&work, &["checkout", "-q", "-b", "feature"])
        .await;
    machine.commit(&work, "a.txt", "a\n").await;
    let opened = machine
        .fgit(
            &work,
            &["mr", "create", "-t", "A", "-b", "Why", "--push", "--json"],
        )
        .await
        .json();
    let id = opened["id"].as_str().unwrap().to_string();
    assert_eq!(opened["description"], "Why");

    let comment = machine
        .fgit(&work, &["mr", "comment", "--body", "Hm", "--json"])
        .await
        .json();
    assert_eq!(comment["body"], "Hm");
    let with_comments = machine
        .fgit(&work, &["mr", "view", "--comments", "--json"])
        .await
        .json();
    assert_eq!(with_comments["comments"][0]["body"], "Hm");

    let review = machine
        .fgit(
            &work,
            &[
                "mr",
                "review",
                "--request-changes",
                "--body",
                "Not yet",
                "--json",
            ],
        )
        .await
        .json();
    assert_eq!(review["decision"], "changes_requested");
    let comments = machine
        .fgit(&work, &["mr", "view", "--jq", ".commentCount"])
        .await;
    assert_eq!(
        comments.ok().stdout,
        "2",
        "the review's text went in as a comment"
    );
    assert_eq!(
        machine.fgit(&work, &["mr", "review"]).await.code,
        2,
        "a review needs a decision"
    );

    // Checks out a branch the local clone hasn't got, from elsewhere in the repository.
    machine.git(&work, &["checkout", "-q", "main"]).await;
    machine.git(&work, &["branch", "-q", "-D", "feature"]).await;
    machine
        .fgit(&work, &["mr", "checkout", &id[..8]])
        .await
        .ok();
    assert_eq!(
        machine
            .git(&work, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await,
        "feature"
    );

    assert_eq!(
        machine.fgit(&work, &["mr", "close"]).await.code,
        2,
        "closing takes --yes"
    );
    let closed = machine
        .fgit(&work, &["mr", "close", "--yes", "--json"])
        .await
        .json();
    assert_eq!(closed["status"], "closed");
    assert_eq!(
        machine
            .fgit(
                &work,
                &["mr", "list", "--state", "closed", "--jq", ".[0].id"]
            )
            .await
            .ok()
            .stdout,
        id
    );
    assert_eq!(machine.fgit(&work, &["mr", "view", "0000"]).await.code, 4);
}

#[sqlx::test]
async fn following_a_failed_pipeline_says_so_and_a_pipeline_can_be_cancelled(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let (machine, _parent, work, session) = repository_with_a_commit(addr).await;
    let runner = new_runner(addr, &session).await;

    machine
        .commit(&work, ".ferrisgit-ci.yml", PIPELINE_FILE)
        .await;
    machine
        .git(&work, &["push", "-q", "origin", "HEAD:main"])
        .await;
    let newest = machine
        .fgit(&work, &["pipeline", "list", "--json"])
        .await
        .json();
    let first = newest[0]["id"].as_str().unwrap().to_string();
    assert_eq!(newest[0]["status"], "pending");

    let viewed = machine
        .fgit(&work, &["pipeline", "view", &first[..8], "--json"])
        .await
        .json();
    assert_eq!(viewed["jobs"][0]["name"], "unit");

    run_pending_jobs(addr, &runner, "failed").await;

    let followed = machine.fgit(&work, &["pipeline", "logs", "--follow"]).await;
    assert_eq!(followed.code, 1, "a failed pipeline isn't a success");
    assert!(
        followed.stdout.contains("hello from unit"),
        "{}",
        followed.stdout
    );
    let watched = machine
        .fgit(&work, &["pipeline", "view", "--watch", "--json"])
        .await;
    assert_eq!(watched.code, 1);
    let json: Value = serde_json::from_str(&watched.stdout).unwrap();
    assert_eq!(json["status"], "failed", "the result still prints");

    let only_job = machine
        .fgit(&work, &["pipeline", "logs", "--job", "unit"])
        .await;
    assert!(only_job.ok().stdout.contains("hello from unit"));
    assert_eq!(
        machine
            .fgit(&work, &["pipeline", "logs", "--job", "nope"])
            .await
            .code,
        4
    );

    machine.commit(&work, "next.txt", "next\n").await;
    machine
        .git(&work, &["push", "-q", "origin", "HEAD:main"])
        .await;
    let second = machine
        .fgit(
            &work,
            &["pipeline", "list", "--limit", "1", "--jq", ".[0].id"],
        )
        .await
        .ok()
        .stdout
        .clone();
    assert_ne!(second, first);
    assert_eq!(
        machine
            .fgit(&work, &["pipeline", "cancel", &second])
            .await
            .code,
        2,
        "cancelling takes --yes"
    );
    let cancelled = machine
        .fgit(&work, &["pipeline", "cancel", &second, "--yes", "--json"])
        .await
        .json();
    assert_eq!(cancelled["status"], "canceled");
}

#[sqlx::test]
async fn the_release_commands(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let (machine, parent, work, _) = repository_with_a_commit(addr).await;
    let asset = parent.path().join("app.tar.gz");
    std::fs::write(&asset, b"binary \x00 content").unwrap();
    let asset_text = asset.to_str().unwrap();

    let refused = machine
        .fgit(
            &work,
            &[
                "release",
                "create",
                "v0",
                "-t",
                "Zero",
                "-a",
                "/no/such/file",
            ],
        )
        .await;
    assert_eq!(refused.code, 2);
    assert_eq!(
        machine
            .fgit(&work, &["release", "list", "--jq", "length"])
            .await
            .ok()
            .stdout,
        "0",
        "a missing file leaves nothing behind"
    );

    let created = machine
        .fgit(
            &work,
            &[
                "release", "create", "v1.0.0", "-t", "One", "-n", "Notes", "-a", asset_text,
                "--json",
            ],
        )
        .await
        .json();
    assert_eq!(created["tagName"], "v1.0.0");
    assert_eq!(created["notes"], "Notes");
    assert_eq!(created["assets"][0]["filename"], "app.tar.gz");
    assert_eq!(created["draft"], false);

    let listed = machine
        .fgit(&work, &["release", "list", "--json"])
        .await
        .json();
    assert_eq!(listed[0]["assetCount"], 1);
    assert_eq!(listed[0]["author"]["username"], "admin");

    let second = parent.path().join("notes.txt");
    std::fs::write(&second, "more").unwrap();
    let uploaded = machine
        .fgit(
            &work,
            &[
                "release",
                "upload",
                "v1.0.0",
                second.to_str().unwrap(),
                "--json",
            ],
        )
        .await
        .json();
    assert_eq!(uploaded[0]["filename"], "notes.txt");

    let out = parent.path().join("out");
    let out_text = out.to_str().unwrap();
    machine
        .fgit(
            &work,
            &[
                "release",
                "download",
                "v1.0.0",
                "--asset",
                "app.tar.gz",
                "--dir",
                out_text,
            ],
        )
        .await
        .ok();
    assert_eq!(
        std::fs::read(out.join("app.tar.gz")).unwrap(),
        b"binary \x00 content"
    );
    assert!(!out.join("notes.txt").exists(), "only what was asked for");

    let again = machine
        .fgit(
            &work,
            &[
                "release",
                "download",
                "v1.0.0",
                "--asset",
                "app.tar.gz",
                "--dir",
                out_text,
            ],
        )
        .await;
    assert_eq!(again.code, 2, "it doesn't replace a file unless told to");
    machine
        .fgit(
            &work,
            &[
                "release",
                "download",
                "v1.0.0",
                "--dir",
                out_text,
                "--clobber",
            ],
        )
        .await
        .ok();
    assert_eq!(std::fs::read(out.join("notes.txt")).unwrap(), b"more");

    let viewed = machine.fgit(&work, &["release", "view", "v1.0.0"]).await;
    assert!(
        viewed.ok().stdout.contains("app.tar.gz"),
        "{}",
        viewed.stdout
    );
    assert_eq!(
        machine.fgit(&work, &["release", "view", "v9"]).await.code,
        4
    );
    assert_eq!(
        machine
            .fgit(
                &work,
                &[
                    "release", "download", "v1.0.0", "--asset", "none", "--dir", out_text
                ]
            )
            .await
            .code,
        4
    );
}

#[sqlx::test]
async fn variables_and_secrets_are_kept_apart(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let (machine, _parent, work, _) = repository_with_a_commit(addr).await;

    let plain = machine
        .fgit(
            &work,
            &[
                "variable",
                "set",
                "DEPLOY_ENV",
                "--value",
                "staging",
                "--json",
            ],
        )
        .await
        .json();
    assert_eq!(plain["masked"], false);

    let secret = machine
        .fgit_with_stdin(
            &work,
            &["secret", "set", "DEPLOY_TOKEN", "--json"],
            "s3cret-value\n",
        )
        .await
        .json();
    assert_eq!(secret["key"], "DEPLOY_TOKEN");
    assert!(!secret.to_string().contains("s3cret-value"));

    // Neither command takes over the other's name, and nothing changes when it's refused.
    let unmask = machine
        .fgit(
            &work,
            &["variable", "set", "DEPLOY_TOKEN", "--value", "open"],
        )
        .await;
    assert_eq!(unmask.code, 5, "stderr: {}", unmask.stderr);
    assert!(unmask.stderr.contains("secret"), "{}", unmask.stderr);
    let takeover = machine
        .fgit(&work, &["secret", "set", "DEPLOY_ENV", "--value", "hidden"])
        .await;
    assert_eq!(takeover.code, 5, "stderr: {}", takeover.stderr);

    let empty = machine
        .fgit_with_stdin(&work, &["secret", "set", "EMPTY"], "")
        .await;
    assert_eq!(empty.code, 5, "stderr: {}", empty.stderr);
    assert!(empty.stderr.contains("empty"), "{}", empty.stderr);

    assert_eq!(
        machine
            .fgit(&work, &["variable", "list", "--jq", ".[].key"])
            .await
            .ok()
            .stdout,
        "DEPLOY_ENV"
    );
    assert_eq!(
        machine
            .fgit(&work, &["secret", "list", "--jq", ".[].key"])
            .await
            .ok()
            .stdout,
        "DEPLOY_TOKEN"
    );

    assert_eq!(
        machine
            .fgit(&work, &["secret", "delete", "DEPLOY_ENV", "--yes"])
            .await
            .code,
        4,
        "a plain variable isn't a secret to delete"
    );
    assert_eq!(
        machine
            .fgit(&work, &["secret", "delete", "DEPLOY_TOKEN"])
            .await
            .code,
        2,
        "deleting takes --yes"
    );
    machine
        .fgit(&work, &["secret", "delete", "DEPLOY_TOKEN", "--yes"])
        .await
        .ok();
    assert_eq!(
        machine
            .fgit(&work, &["secret", "list", "--jq", "length"])
            .await
            .ok()
            .stdout,
        "0"
    );
}

#[sqlx::test]
async fn the_raw_api_command(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let (machine, parent, work, _) = repository_with_a_commit(addr).await;

    assert_eq!(
        machine
            .fgit(&work, &["api", "/auth/me", "--jq", ".username"])
            .await
            .ok()
            .stdout,
        "admin"
    );

    let repo_id = machine
        .fgit(&work, &["repo", "view", "--jq", ".id"])
        .await
        .ok()
        .stdout
        .clone();
    let labels = format!("/repositories/{repo_id}/labels");
    let created = machine
        .fgit(
            &work,
            &["api", &labels, "-f", "name=bug", "-f", "color=#d73a4a"],
        )
        .await
        .json();
    assert_eq!(created["name"], "bug");

    let input = parent.path().join("label.json");
    std::fs::write(&input, r##"{"name":"ui","color":"#00ff00"}"##).unwrap();
    let from_file = machine
        .fgit(&work, &["api", &labels, "--input", input.to_str().unwrap()])
        .await
        .json();
    assert_eq!(from_file["name"], "ui");

    let label_id = created["id"].as_str().unwrap();
    machine
        .fgit(
            &work,
            &["api", &format!("/labels/{label_id}"), "-X", "DELETE"],
        )
        .await
        .ok();
    assert_eq!(
        machine
            .fgit(&work, &["api", &labels, "--jq", "length"])
            .await
            .ok()
            .stdout,
        "1"
    );

    assert_eq!(
        machine.fgit(&work, &["api", "/no/such/route"]).await.code,
        4
    );
    assert_eq!(
        machine
            .fgit(&work, &["api", "https://evil.example.com/x"])
            .await
            .code,
        2
    );
    let session_only = machine.fgit(&work, &["api", "/access-tokens"]).await;
    assert_eq!(session_only.code, 5, "stderr: {}", session_only.stderr);

    let client = reqwest::Client::new();
    assert_eq!(
        get(&client, addr, "not-a-token", "/auth/me").await.status(),
        401
    );
}

#[sqlx::test]
async fn a_token_given_on_stdin_is_checked_before_it_is_stored(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let session = login(&client, addr, "admin", ADMIN).await;
    let token = new_token(addr, &session, &["repositories:read"]).await;
    let machine = Machine::new();
    let cwd = tempfile::tempdir().unwrap();
    let hostname = format!("http://{addr}");
    let args = [
        "auth",
        "login",
        "--hostname",
        hostname.as_str(),
        "--with-token",
        "--insecure-storage",
    ];

    let bad = machine
        .fgit_with_stdin(cwd.path(), &args, "fga_forged\n")
        .await;
    assert_eq!(bad.code, 3);
    assert!(!machine.home.path().join("fgit").join("hosts.json").exists());

    let empty = machine.fgit_with_stdin(cwd.path(), &args, "").await;
    assert_eq!(empty.code, 2);

    machine
        .fgit_with_stdin(cwd.path(), &args, &format!("{token}\n"))
        .await
        .ok();
    assert_eq!(
        machine
            .fgit(cwd.path(), &["auth", "token"])
            .await
            .ok()
            .stdout,
        token,
        "the instance signed in to is now the default"
    );
    assert_eq!(
        machine
            .fgit(cwd.path(), &["repo", "list", "--jq", "length"])
            .await
            .ok()
            .stdout,
        "0"
    );
}
