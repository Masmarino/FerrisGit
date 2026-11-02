# fgit

The command-line client of [FerrisGit](https://github.com/Masmarino/FerrisGit), for people in a terminal, for scripts and
CI jobs, and for AI agents working in a repository.

```bash
cargo install ferrisgit-cli
fgit auth login --hostname git.example.com
fgit mr create --title "Fix the login" --push
fgit pipeline view --mr "$ID" --watch
fgit mr merge "$ID" --yes
```

The crate is called `ferrisgit-cli` because `fgit` is taken on crates.io. The command is `fgit`. Ready-made binaries are
attached to each [GitHub release](https://github.com/Masmarino/FerrisGit/releases).

For a program, a token in `FERRISGIT_TOKEN` is all it takes: there is no prompt without a terminal, results are on stdout
and messages on stderr, `--json` and `--jq` work on every command, and exit codes are documented in `fgit --help`.

The documentation, in French, is in the "Ligne de commande" section of the
[FerrisGit documentation](https://github.com/Masmarino/FerrisGit/tree/main/docs/ligne-de-commande).

Licensed under Apache-2.0.
