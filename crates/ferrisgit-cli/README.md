# fgit

The command-line client of [FerrisGit](https://github.com/Masmarino/FerrisGit), for people in a terminal, for scripts and
CI jobs, and for AI agents working in a repository.

```bash
brew install masmarino/tap/fgit
fgit auth login --hostname git.example.com
fgit mr create --title "Fix the login" --push
fgit pipeline view --mr "$ID" --watch
fgit mr merge "$ID" --yes
```

Without Homebrew, on Linux or macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/Masmarino/FerrisGit/main/scripts/install-fgit.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/Masmarino/FerrisGit/main/scripts/install-fgit.ps1 | iex
```

With a Rust toolchain, `cargo install ferrisgit-cli`: the crate has that name because `fgit` is taken on crates.io, and the
command is `fgit`. The binaries are also attached to each [GitHub release](https://github.com/Masmarino/FerrisGit/releases).

For a program, a token in `FERRISGIT_TOKEN` is all it takes: there is no prompt without a terminal, results are on stdout
and messages on stderr, `--json` and `--jq` work on every command, and exit codes are documented in `fgit --help`.

The documentation, in French, is in the "Ligne de commande" section of the
[FerrisGit documentation](https://github.com/Masmarino/FerrisGit/tree/main/docs/ligne-de-commande).

Licensed under Apache-2.0.
