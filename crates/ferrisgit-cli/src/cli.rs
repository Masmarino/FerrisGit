use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

const EXIT_CODES: &str = "\
Exit codes:
  0  success
  1  anything else: a Git failure, a pipeline that did not succeed, a local file
  2  usage: a bad or missing argument, or a confirmation with no terminal to ask in
  3  authentication: not signed in, or a token that is invalid or expired
  4  not found, which includes what your account has no right to see
  5  refused by the server: scope, validation, conflict, rate limit, merge conditions
  6  server error, or the server could not be reached

Environment:
  FERRISGIT_TOKEN  API token to use instead of the stored one
  FERRISGIT_HOST   instance to use. When set, FERRISGIT_TOKEN only goes to this instance
  NO_COLOR         no colour, even in a terminal";

#[derive(Parser)]
#[command(
    name = "fgit",
    version,
    about = "The command-line client of FerrisGit, for people, scripts and AI agents",
    after_help = EXIT_CODES,
    propagate_version = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args, Clone, Default)]
pub struct Global {
    /// Repository to act on: host/owner/name, or owner/name on the current instance. Default: the current directory's Git remote
    #[arg(long, short = 'R', global = true, value_name = "REPOSITORY")]
    pub repo: Option<String>,
    /// Print the result as JSON on stdout, with the documented field names
    #[arg(long, global = true)]
    pub json: bool,
    /// Filter the JSON result with a jq expression. Implies --json, and strings print bare
    #[arg(long, global = true, value_name = "EXPRESSION")]
    pub jq: Option<String>,
    /// Confirm what can't be undone. Without a terminal, such a command fails unless this is given
    #[arg(long, short = 'y', global = true)]
    pub yes: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Sign in to an instance, shorthand for `auth login`
    #[command(after_help = "Examples:\n  fgit login\n  fgit login --hostname git.example.com")]
    Login(LoginArgs),
    /// Forget an instance, shorthand for `auth logout`
    #[command(after_help = "Examples:\n  fgit logout\n  fgit logout --hostname git.example.com")]
    Logout(LogoutArgs),
    /// Manage sign-ins on this machine
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Work with repositories
    #[command(subcommand)]
    Repo(RepoCommand),
    /// Work with issues
    #[command(subcommand)]
    Issue(IssueCommand),
    /// Work with merge requests
    #[command(subcommand)]
    Mr(MrCommand),
    /// Follow and cancel pipelines
    #[command(subcommand)]
    Pipeline(PipelineCommand),
    /// Work with releases and their assets
    #[command(subcommand)]
    Release(ReleaseCommand),
    /// Manage a repository's CI variables, whose values appear in job logs
    #[command(subcommand)]
    Variable(VariableCommand),
    /// Manage a repository's CI secrets, whose values are masked in job logs
    #[command(subcommand)]
    Secret(VariableCommand),
    /// Make an authenticated request to any route of the API
    #[command(after_help = "\
The path is relative to /api. The body comes from --field and --raw-field, or from --input.
The method defaults to GET, or POST with a body. A JSON answer prints on stdout, and --jq applies to it.

Examples:
  fgit api /auth/me
  fgit api /auth/me --jq .username
  fgit api /repositories/$ID/labels -f name=bug -f color=d73a4a
  fgit api /merge-requests/$ID -X PATCH --input body.json
  fgit api /repositories/$ID/issues/1/assign -F assigneeId=null")]
    Api(ApiArgs),
}

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Sign in, through the browser or with a token from stdin
    #[command(after_help = "\
Without --with-token, shows a code and opens the browser, where you confirm it with your second factor. The token it
gets goes to the system keychain.

Without a terminal, sign-in never waits for anyone unless asked: use --with-token, FERRISGIT_TOKEN, or --web to print
the code on stderr and wait.

Examples:
  fgit auth login
  fgit auth login --hostname git.example.com
  fgit auth login --scopes issues:write,merge-requests:write --expires-in-days 30
  echo \"$TOKEN\" | fgit auth login --hostname git.example.com --with-token")]
    Login(LoginArgs),
    /// Forget an instance's token on this machine
    #[command(after_help = "\
This only forgets the token here: it keeps working until it expires. To end it now, revoke it in Mon compte.

Examples:
  fgit auth logout
  fgit auth logout --hostname git.example.com")]
    Logout(LogoutArgs),
    /// Show the instances signed in, and whether their tokens still work
    #[command(after_help = "Examples:\n  fgit auth status\n  fgit auth status --json")]
    Status,
    /// Print the token of an instance
    #[command(
        after_help = "Examples:\n  fgit auth token\n  fgit auth token --hostname git.example.com"
    )]
    Token(HostArgs),
    /// Make Git use fgit for the credentials of the instances signed in
    #[command(
        name = "setup-git",
        after_help = "Examples:\n  fgit auth setup-git\n  fgit auth setup-git --hostname git.example.com"
    )]
    SetupGit(HostArgs),
    /// The Git credential helper: Git calls it, people don't
    #[command(
        name = "git-credential",
        hide = true,
        after_help = "Examples:\n  printf 'protocol=https\\nhost=git.example.com\\n' | fgit auth git-credential get"
    )]
    GitCredential {
        /// get, store or erase, as Git says
        action: String,
    },
}

#[derive(Args)]
pub struct HostArgs {
    /// Instance. Default: FERRISGIT_HOST, the current Git remote, then the default instance
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
}

#[derive(Args)]
pub struct LoginArgs {
    /// Instance to sign in to. Default: FERRISGIT_HOST, the current Git remote, then the default instance
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
    /// Read an API token from stdin instead of opening the browser
    #[arg(long)]
    pub with_token: bool,
    /// Sign in through the browser even with no terminal: the code is on stderr, and this waits for someone to confirm it
    #[arg(long, conflicts_with = "with_token")]
    pub web: bool,
    /// Do not open the browser, only show the address
    #[arg(long, conflicts_with = "with_token")]
    pub no_browser: bool,
    /// Scopes to ask for, comma-separated. Default: write on everything but admin
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "SCOPES",
        conflicts_with = "with_token"
    )]
    pub scopes: Vec<String>,
    /// Days the token lives, up to 365
    #[arg(
        long,
        default_value_t = 90,
        value_name = "DAYS",
        conflicts_with = "with_token"
    )]
    pub expires_in_days: u32,
    /// Keep the token in a plain file instead of the system keychain, for machines that have none
    #[arg(long)]
    pub insecure_storage: bool,
}

#[derive(Args)]
pub struct LogoutArgs {
    /// Instance to forget. Default: FERRISGIT_HOST, the current Git remote, then the default instance
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
}

#[derive(Subcommand)]
pub enum RepoCommand {
    /// List the repositories you can see
    #[command(
        after_help = "Examples:\n  fgit repo list\n  fgit repo list --starred\n  fgit repo list --jq '.[].path | join(\"/\")'"
    )]
    List {
        /// Only the ones you starred
        #[arg(long)]
        starred: bool,
        /// Instance. Default: FERRISGIT_HOST, the current Git remote, then the default instance
        #[arg(long, value_name = "HOST")]
        hostname: Option<String>,
    },
    /// Show a repository
    #[command(
        after_help = "Examples:\n  fgit repo view\n  fgit repo view acme/hello\n  fgit repo view git.example.com/acme/team/hello --json"
    )]
    View {
        /// Repository. Default: --repo, then the current Git remote
        repository: Option<String>,
    },
    /// Create a repository
    #[command(
        after_help = "Examples:\n  fgit repo create hello\n  fgit repo create hello --public --description \"First steps\"\n  fgit repo create hello --group acme/team"
    )]
    Create {
        /// Name of the repository
        name: Option<String>,
        /// Description
        #[arg(long, default_value = "")]
        description: String,
        /// Visible to everyone. Default: private
        #[arg(long)]
        public: bool,
        /// Group to create it in, as a path. Default: your personal space
        #[arg(long, value_name = "PATH")]
        group: Option<String>,
        /// Instance. Default: FERRISGIT_HOST, the current Git remote, then the default instance
        #[arg(long, value_name = "HOST")]
        hostname: Option<String>,
    },
    /// Clone a repository, and set it up so that later `git push` commands use fgit's credentials
    #[command(
        after_help = "Examples:\n  fgit repo clone acme/hello\n  fgit repo clone git.example.com/acme/team/hello work/hello"
    )]
    Clone {
        /// Repository
        repository: String,
        /// Directory to clone into
        directory: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StateFilter {
    Open,
    Closed,
    Merged,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum IssueKind {
    Bug,
    Feature,
    Task,
    Epic,
}

impl IssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IssueKind::Bug => "bug",
            IssueKind::Feature => "feature",
            IssueKind::Task => "task",
            IssueKind::Epic => "epic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum IssueStatus {
    Todo,
    InProgress,
    InReview,
    Done,
}

impl IssueStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            IssueStatus::Todo => "todo",
            IssueStatus::InProgress => "in_progress",
            IssueStatus::InReview => "in_review",
            IssueStatus::Done => "done",
        }
    }
}

/// `--body` or `--body-file`, which reads stdin with `-`.
#[derive(Args, Clone)]
pub struct BodyArgs {
    /// Text
    #[arg(long, short = 'b', conflicts_with = "body_file")]
    pub body: Option<String>,
    /// Read the text from a file, or from stdin with -
    #[arg(long, short = 'F', value_name = "FILE")]
    pub body_file: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum IssueCommand {
    /// List a repository's issues
    #[command(after_help = "\
\"open\" is every status but done, \"closed\" is done.

Examples:
  fgit issue list
  fgit issue list --state all --label bug
  fgit issue list --assignee alice --json
  fgit issue list --jq '.[] | select(.kind == \"bug\") | .number'")]
    List {
        /// Which ones
        #[arg(long, value_enum, default_value_t = StateFilter::Open)]
        state: StateFilter,
        /// Only with this label. Can be repeated
        #[arg(long, short = 'l')]
        label: Vec<String>,
        /// Only assigned to this username
        #[arg(long, short = 'a')]
        assignee: Option<String>,
    },
    /// Show an issue
    #[command(
        after_help = "Examples:\n  fgit issue view 12\n  fgit issue view 12 --comments\n  fgit issue view 12 --jq .status"
    )]
    View {
        /// Issue number
        number: i64,
        /// Show the comments too
        #[arg(long, short = 'c')]
        comments: bool,
    },
    /// Open an issue
    #[command(
        after_help = "Examples:\n  fgit issue create --title \"Fix the login\" --kind bug\n  fgit issue create -t \"Plan\" --body-file plan.md\n  echo \"Details\" | fgit issue create -t \"Plan\" -F -"
    )]
    Create {
        /// Title
        #[arg(long, short = 't')]
        title: Option<String>,
        #[command(flatten)]
        body: BodyArgs,
        /// Kind
        #[arg(long, short = 'k', value_enum, default_value_t = IssueKind::Task)]
        kind: IssueKind,
    },
    /// Change an issue's title, text, kind or status
    #[command(
        after_help = "Examples:\n  fgit issue edit 12 --title \"A better title\"\n  fgit issue edit 12 --kind bug --status in_progress\n  fgit issue edit 12 --body-file notes.md"
    )]
    Edit {
        /// Issue number
        number: i64,
        /// New title
        #[arg(long, short = 't')]
        title: Option<String>,
        #[command(flatten)]
        body: BodyArgs,
        /// New kind
        #[arg(long, short = 'k', value_enum)]
        kind: Option<IssueKind>,
        /// New status
        #[arg(long, short = 's', value_enum)]
        status: Option<IssueStatus>,
    },
    /// Comment on an issue
    #[command(
        after_help = "Examples:\n  fgit issue comment 12 --body \"Fixed in main\"\n  fgit issue comment 12 --body-file notes.md"
    )]
    Comment {
        /// Issue number
        number: i64,
        #[command(flatten)]
        body: BodyArgs,
    },
    /// Close an issue
    #[command(after_help = "Examples:\n  fgit issue close 12")]
    Close {
        /// Issue number
        number: i64,
    },
    /// Reopen an issue
    #[command(after_help = "Examples:\n  fgit issue reopen 12")]
    Reopen {
        /// Issue number
        number: i64,
    },
    /// Assign an issue, or take its assignee away
    #[command(
        after_help = "Examples:\n  fgit issue assign 12 alice\n  fgit issue assign 12 --unassign"
    )]
    Assign {
        /// Issue number
        number: i64,
        /// Username
        #[arg(required_unless_present = "unassign", conflicts_with = "unassign")]
        username: Option<String>,
        /// Leave it unassigned
        #[arg(long)]
        unassign: bool,
    },
    /// Add or remove an issue's labels
    #[command(
        after_help = "Examples:\n  fgit issue labels 12 --add bug --add urgent\n  fgit issue labels 12 --remove urgent\n  fgit issue labels 12 --jq '.[].name'"
    )]
    Labels {
        /// Issue number
        number: i64,
        /// Label to add. Can be repeated
        #[arg(long)]
        add: Vec<String>,
        /// Label to remove. Can be repeated
        #[arg(long)]
        remove: Vec<String>,
    },
    /// Set or clear an issue's milestone
    #[command(
        after_help = "Examples:\n  fgit issue milestone 12 \"Version 1.0\"\n  fgit issue milestone 12 --clear"
    )]
    Milestone {
        /// Issue number
        number: i64,
        /// Milestone title
        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        title: Option<String>,
        /// Leave it without a milestone
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            MergeMethod::Merge => "merge",
            MergeMethod::Squash => "squash",
            MergeMethod::Rebase => "rebase",
        }
    }
}

/// A merge request's id, whole or as a unique start. Default: the open one of the current branch.
#[derive(Args, Clone)]
pub struct MrRef {
    /// Id, or a unique start of it. Default: the open merge request of the current branch
    pub id: Option<String>,
}

#[derive(Subcommand)]
pub enum MrCommand {
    /// List a repository's merge requests
    #[command(
        after_help = "Examples:\n  fgit mr list\n  fgit mr list --state merged\n  fgit mr list --jq '.[] | select(.sourceBranch == \"fix\") | .id'"
    )]
    List {
        /// Which ones
        #[arg(long, value_enum, default_value_t = StateFilter::Open)]
        state: StateFilter,
    },
    /// Show a merge request
    #[command(
        after_help = "Examples:\n  fgit mr view\n  fgit mr view 3f2a91c0 --comments\n  fgit mr view --jq .pipeline.status"
    )]
    View {
        #[command(flatten)]
        mr: MrRef,
        /// Show the comments too
        #[arg(long, short = 'c')]
        comments: bool,
    },
    /// Open a merge request from the current branch
    #[command(after_help = "\
The branch has to be on the server: --push sends it there first.

Examples:
  fgit mr create --title \"Fix the login\" --push
  fgit mr create -t \"Fix\" --target develop --body-file notes.md
  fgit mr create -t \"Fix\" --source fix/login --jq .id")]
    Create {
        /// Title
        #[arg(long, short = 't')]
        title: Option<String>,
        #[command(flatten)]
        body: BodyArgs,
        /// Branch to merge. Default: the current branch
        #[arg(long, short = 's')]
        source: Option<String>,
        /// Branch to merge into. Default: the repository's default branch
        #[arg(long)]
        target: Option<String>,
        /// Push the branch first
        #[arg(long)]
        push: bool,
    },
    /// Check out a merge request's branch locally
    #[command(after_help = "Examples:\n  fgit mr checkout 3f2a91c0")]
    Checkout {
        /// Id, or a unique start of it
        id: String,
    },
    /// Show a merge request's changes
    #[command(
        after_help = "Examples:\n  fgit mr diff\n  fgit mr diff 3f2a91c0 --json\n  fgit mr diff --jq '.[].path'"
    )]
    Diff {
        #[command(flatten)]
        mr: MrRef,
    },
    /// Comment on a merge request
    #[command(
        after_help = "Examples:\n  fgit mr comment --body \"Looks good\"\n  fgit mr comment 3f2a91c0 --body-file notes.md"
    )]
    Comment {
        #[command(flatten)]
        mr: MrRef,
        #[command(flatten)]
        body: BodyArgs,
    },
    /// Approve a merge request or ask for changes
    #[command(after_help = "\
A review holds no text, so --body goes in as a comment first.

Examples:
  fgit mr review --approve
  fgit mr review 3f2a91c0 --request-changes --body \"The test is missing\"")]
    Review {
        #[command(flatten)]
        mr: MrRef,
        /// Approve it
        #[arg(
            long,
            conflicts_with = "request_changes",
            required_unless_present = "request_changes"
        )]
        approve: bool,
        /// Ask for changes
        #[arg(long)]
        request_changes: bool,
        #[command(flatten)]
        body: BodyArgs,
    },
    /// Merge a merge request
    #[command(after_help = "\
Exits with 5 when a merge condition isn't met or the branches conflict.

Examples:
  fgit mr merge
  fgit mr merge 3f2a91c0 --method squash --yes
  fgit mr merge --yes --jq .mergeCommitSha")]
    Merge {
        #[command(flatten)]
        mr: MrRef,
        /// Merge method. Default: the repository's
        #[arg(long, short = 'm', value_enum)]
        method: Option<MergeMethod>,
    },
    /// Close a merge request without merging it
    #[command(after_help = "Examples:\n  fgit mr close\n  fgit mr close 3f2a91c0 --yes")]
    Close {
        #[command(flatten)]
        mr: MrRef,
    },
}

#[derive(Subcommand)]
pub enum PipelineCommand {
    /// List a repository's pipelines, newest first
    #[command(
        after_help = "Examples:\n  fgit pipeline list\n  fgit pipeline list --limit 5 --jq '.[].status'"
    )]
    List {
        /// How many
        #[arg(long, short = 'L', default_value_t = 20)]
        limit: usize,
    },
    /// Show a pipeline and its jobs
    #[command(after_help = "\
Log text is in the JSON, one field per job, and `pipeline logs` prints it.
With --watch, waits for the end and exits with 1 unless the pipeline succeeded.

Examples:
  fgit pipeline view
  fgit pipeline view 8c1d07aa --json
  fgit pipeline view --mr 3f2a91c0 --watch")]
    View {
        /// Id, or a unique start of it. Default: the newest pipeline of the repository
        id: Option<String>,
        /// The pipeline of this merge request instead
        #[arg(long, value_name = "ID", conflicts_with = "id")]
        mr: Option<String>,
        /// Wait until it finishes
        #[arg(long, short = 'w')]
        watch: bool,
    },
    /// Print a pipeline's job logs, or follow them as they are written
    #[command(after_help = "\
With --follow, keeps printing until the pipeline ends, and exits with 1 unless it succeeded.

Examples:
  fgit pipeline logs
  fgit pipeline logs 8c1d07aa --job test
  fgit pipeline logs --mr 3f2a91c0 --job build --follow")]
    Logs {
        /// Id, or a unique start of it. Default: the newest pipeline of the repository
        id: Option<String>,
        /// The pipeline of this merge request instead
        #[arg(long, value_name = "ID", conflicts_with = "id")]
        mr: Option<String>,
        /// Only this job
        #[arg(long, short = 'j')]
        job: Option<String>,
        /// Keep printing until it ends
        #[arg(long, short = 'f')]
        follow: bool,
    },
    /// Cancel a pipeline
    #[command(
        after_help = "Examples:\n  fgit pipeline cancel 8c1d07aa\n  fgit pipeline cancel 8c1d07aa --yes"
    )]
    Cancel {
        /// Id, or a unique start of it
        id: String,
    },
}

#[derive(Subcommand)]
pub enum ReleaseCommand {
    /// List a repository's releases
    #[command(
        after_help = "Examples:\n  fgit release list\n  fgit release list --jq '.[0].tagName'"
    )]
    List,
    /// Show a release and its assets
    #[command(
        after_help = "Examples:\n  fgit release view v1.0.0\n  fgit release view v1.0.0 --jq '.assets[].filename'"
    )]
    View {
        /// Tag
        tag: String,
    },
    /// Create a release
    #[command(
        after_help = "Examples:\n  fgit release create v1.0.0 --title \"Version 1.0\"\n  fgit release create v1.0.0 -t \"1.0\" --target develop --notes-file CHANGES.md\n  fgit release create v1.0.0 -t \"1.0\" --draft --asset dist/app.tar.gz"
    )]
    Create {
        /// Tag. Created if it doesn't exist
        tag: String,
        /// Title
        #[arg(long, short = 't')]
        title: Option<String>,
        /// Release notes
        #[arg(long, short = 'n', conflicts_with = "notes_file")]
        notes: Option<String>,
        /// Read the notes from a file, or from stdin with -
        #[arg(long, value_name = "FILE")]
        notes_file: Option<PathBuf>,
        /// Branch or full commit id to tag. Default: the repository's default branch
        #[arg(long)]
        target: Option<String>,
        /// Leave it unpublished
        #[arg(long)]
        draft: bool,
        /// Mark it as a pre-release
        #[arg(long)]
        prerelease: bool,
        /// File to attach. Can be repeated
        #[arg(long, short = 'a', value_name = "FILE")]
        asset: Vec<PathBuf>,
    },
    /// Attach files to a release
    #[command(after_help = "Examples:\n  fgit release upload v1.0.0 dist/app.tar.gz dist/app.zip")]
    Upload {
        /// Tag
        tag: String,
        /// Files to attach
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Download a release's assets
    #[command(
        after_help = "Examples:\n  fgit release download v1.0.0\n  fgit release download v1.0.0 --asset app.tar.gz --dir dist --clobber"
    )]
    Download {
        /// Tag
        tag: String,
        /// Only this file. Can be repeated
        #[arg(long, short = 'a', value_name = "NAME")]
        asset: Vec<String>,
        /// Directory to download into
        #[arg(long, short = 'd', default_value = ".")]
        dir: PathBuf,
        /// Replace files already there
        #[arg(long)]
        clobber: bool,
    },
}

#[derive(Subcommand)]
pub enum VariableCommand {
    /// List the names. Values never come back from the server
    #[command(after_help = "Examples:\n  fgit variable list\n  fgit secret list --jq '.[].key'")]
    List,
    /// Create or replace one
    #[command(after_help = "\
Without --value, reads the value from stdin. Prefer that for a secret: a command line stays in your shell's history.

Examples:
  fgit variable set DEPLOY_ENV --value staging
  printf %s \"$TOKEN\" | fgit secret set DEPLOY_TOKEN
  fgit secret set DEPLOY_KEY --value-file key.pem")]
    Set {
        /// Name: letters, digits and _, not starting with a digit
        key: String,
        /// Value
        #[arg(long, conflicts_with = "value_file")]
        value: Option<String>,
        /// Read the value from a file
        #[arg(long, value_name = "FILE")]
        value_file: Option<PathBuf>,
    },
    /// Delete one
    #[command(
        after_help = "Examples:\n  fgit variable delete DEPLOY_ENV\n  fgit secret delete DEPLOY_TOKEN --yes"
    )]
    Delete {
        /// Name
        key: String,
    },
}

#[derive(Args)]
pub struct ApiArgs {
    /// Path below /api
    pub path: String,
    /// HTTP method. Default: GET, or POST with a body
    #[arg(long, short = 'X', value_name = "METHOD")]
    pub method: Option<String>,
    /// A body field whose value is a string: key=value. Can be repeated
    #[arg(long = "raw-field", short = 'f', value_name = "KEY=VALUE", action = ArgAction::Append)]
    pub raw_fields: Vec<String>,
    /// A body field whose value is JSON (true, 12, null, ["a"]), or else a string: key=value. Can be repeated
    #[arg(long = "field", short = 'F', value_name = "KEY=VALUE", action = ArgAction::Append)]
    pub fields: Vec<String>,
    /// Send this file as the body, or stdin with -
    #[arg(long, value_name = "FILE", conflicts_with_all = ["raw_fields", "fields"])]
    pub input: Option<PathBuf>,
    /// Instance. Default: FERRISGIT_HOST, the current Git remote, then the default instance
    #[arg(long, value_name = "HOST")]
    pub hostname: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn leaves(command: &clap::Command, path: String, found: &mut Vec<(String, clap::Command)>) {
        let mut subcommands = command.get_subcommands().peekable();
        if subcommands.peek().is_none() {
            found.push((path, command.clone()));
            return;
        }
        for sub in subcommands {
            leaves(sub, format!("{path} {}", sub.get_name()), found);
        }
    }

    fn all_leaves() -> Vec<(String, clap::Command)> {
        let mut found = Vec::new();
        leaves(&Cli::command(), "fgit".to_string(), &mut found);
        found
    }

    #[test]
    fn the_command_tree_is_valid() {
        Cli::command().debug_assert();
    }

    /// An agent finds its way through `--help`, so this is part of the interface.
    #[test]
    fn every_command_has_a_description_and_examples() {
        let leaves = all_leaves();
        assert!(leaves.len() > 40, "only {} commands", leaves.len());
        for (path, command) in leaves {
            assert!(command.get_about().is_some(), "{path} has no description");
            let after = command.get_after_help().map(ToString::to_string);
            let after = after.unwrap_or_default();
            assert!(after.contains("Examples:"), "{path} has no examples");
            assert!(after.contains("fgit "), "{path}: examples must show fgit");
        }
    }

    #[test]
    fn every_argument_is_explained() {
        for (path, command) in all_leaves() {
            for arg in command.get_arguments() {
                if matches!(arg.get_id().as_str(), "help" | "version") {
                    continue;
                }
                assert!(arg.get_help().is_some(), "{path} --{}", arg.get_id());
            }
        }
    }

    #[test]
    fn the_exit_codes_are_in_the_top_level_help() {
        let help = Cli::command().render_long_help().to_string();
        for code in [
            "  2  usage",
            "  3  authentication",
            "  4  not found",
            "  5  refused",
            "  6  server",
        ] {
            assert!(help.contains(code), "{code}");
        }
    }
}
