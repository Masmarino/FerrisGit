use std::io::{IsTerminal, Read, Write};

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
use jaq_json::{Val, read};
use serde::Serialize;

use crate::error::{CliError, Result};

/// Results go to stdout, everything else to stderr. Colour only in a terminal, and never with `NO_COLOR`.
pub struct Output {
    json: bool,
    jq: Option<String>,
    stdout_is_terminal: bool,
    color: bool,
}

impl Output {
    pub fn new(json: bool, jq: Option<String>) -> Result<Self> {
        if let Some(filter) = &jq {
            validate_filter(filter)?;
        }
        let stdout_is_terminal = std::io::stdout().is_terminal();
        Ok(Self {
            json,
            jq,
            stdout_is_terminal,
            color: stdout_is_terminal && std::env::var_os("NO_COLOR").is_none(),
        })
    }

    pub fn wants_json(&self) -> bool {
        self.json || self.jq.is_some()
    }

    /// A status for the person watching. Never part of the result.
    pub fn message(&self, text: &str) {
        eprintln!("{text}");
    }

    /// `human` only runs without `--json`, so it can print freely.
    pub fn result<T: Serialize>(&self, value: &T, human: impl FnOnce()) -> Result<()> {
        if !self.wants_json() {
            human();
            return Ok(());
        }
        self.print_json(&serde_json::to_value(value).map_err(unexpected)?)
    }

    pub fn print_json(&self, value: &serde_json::Value) -> Result<()> {
        if let Some(filter) = &self.jq {
            for line in run_filter(filter, value)? {
                self.line(&line);
            }
            return Ok(());
        }
        let text = if self.stdout_is_terminal {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        };
        self.line(&text.map_err(unexpected)?);
        Ok(())
    }

    /// A closed pipe (`fgit … | head`) isn't an error worth a message.
    pub fn line(&self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{text}");
    }

    pub fn raw(&self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(text.as_bytes());
        let _ = out.flush();
    }

    pub fn table(&self, rows: &[Vec<String>]) {
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        let widths: Vec<usize> = (0..columns)
            .map(|c| {
                rows.iter()
                    .filter_map(|row| row.get(c))
                    .map(|cell| clean(cell).chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for row in rows {
            let cells: Vec<String> = row
                .iter()
                .enumerate()
                .map(|(c, cell)| {
                    let cell = clean(cell);
                    if c + 1 == row.len() {
                        cell
                    } else {
                        format!("{cell:<width$}", width = widths[c])
                    }
                })
                .collect();
            self.line(&cells.join("  "));
        }
    }

    pub fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    /// Server text heading to a terminal: logs can hold anything a job printed.
    pub fn terminal_text(&self, text: &str) -> String {
        if self.stdout_is_terminal {
            clean_keeping_lines(text)
        } else {
            text.to_string()
        }
    }
}

/// What other people wrote (a title, a branch, a client name) mustn't be able to drive the terminal it is shown in.
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub fn clean_keeping_lines(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

fn unexpected(e: impl std::fmt::Display) -> CliError {
    CliError::Other(format!("could not format the result: {e}"))
}

enum FilterError {
    /// The expression itself is wrong: a usage error, reported before anything runs.
    Invalid(String),
    /// It doesn't apply to this input.
    Runtime(String),
}

fn evaluate(
    filter: &str,
    input: &serde_json::Value,
) -> std::result::Result<Vec<String>, FilterError> {
    let program = File {
        code: filter,
        path: (),
    };
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let loader = Loader::new(defs);
    let arena = Arena::default();
    let invalid = || FilterError::Invalid(format!("invalid --jq filter: {filter}"));
    let modules = loader.load(&arena, program).map_err(|_| invalid())?;
    let compiled = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|_| invalid())?;

    let text = input.to_string();
    let parsed =
        read::parse_single(text.as_bytes()).map_err(|e| FilterError::Runtime(e.to_string()))?;
    let ctx = Ctx::<data::JustLut<Val>>::new(&compiled.lut, Vars::new([]));
    let mut lines = Vec::new();
    for item in compiled.id.run((ctx, parsed)).map(unwrap_valr) {
        let value = item.map_err(|e| FilterError::Runtime(e.to_string()))?;
        lines.push(render_filtered(&value.to_string()));
    }
    Ok(lines)
}

/// Like `jq -r` and `gh --jq`: a string prints bare, so it can go straight into a shell variable.
fn render_filtered(json: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::String(text)) => text,
        _ => json.to_string(),
    }
}

fn validate_filter(filter: &str) -> Result<()> {
    match evaluate(filter, &serde_json::Value::Null) {
        Err(FilterError::Invalid(message)) => Err(CliError::Usage(message)),
        _ => Ok(()),
    }
}

fn run_filter(filter: &str, input: &serde_json::Value) -> Result<Vec<String>> {
    evaluate(filter, input).map_err(|e| match e {
        FilterError::Invalid(message) => CliError::Usage(message),
        FilterError::Runtime(message) => CliError::Other(format!("--jq failed: {message}")),
    })
}

pub fn stdin_is_terminal() -> bool {
    std::io::stdin().is_terminal()
}

pub fn read_stdin() -> Result<String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| CliError::Other(format!("could not read stdin: {e}")))?;
    Ok(text)
}

/// `value`, or a question in a terminal. Elsewhere nobody can answer, so it is a usage error naming the flag.
pub fn required(value: Option<String>, flag: &str, question: &str) -> Result<String> {
    if let Some(value) = value {
        return Ok(value);
    }
    if !stdin_is_terminal() {
        return Err(CliError::Usage(format!("{flag} is required")));
    }
    eprint!("{question}: ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliError::Other(format!("could not read the answer: {e}")))?;
    let line = line.trim().to_string();
    if line.is_empty() {
        return Err(CliError::Usage(format!("{flag} is required")));
    }
    Ok(line)
}

/// For what can't be undone. `--yes` skips the question, and without a terminal it is the only way through.
pub fn confirm(yes: bool, question: &str) -> Result<()> {
    if yes {
        return Ok(());
    }
    if !stdin_is_terminal() {
        return Err(CliError::Usage(format!(
            "{question}: pass --yes to confirm, there is no terminal to ask in"
        )));
    }
    eprint!("{question} [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliError::Other(format!("could not read the answer: {e}")))?;
    if matches!(line.trim().to_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        Err(CliError::Other("cancelled".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn filtered(filter: &str, input: serde_json::Value) -> Vec<String> {
        run_filter(filter, &input).unwrap()
    }

    #[test]
    fn a_filter_selects_and_a_string_prints_bare() {
        assert_eq!(filtered(".name", json!({ "name": "hello" })), vec!["hello"]);
        assert_eq!(filtered(".n", json!({ "n": 3 })), vec!["3"]);
    }

    #[test]
    fn a_filter_can_give_several_results_and_use_the_standard_library() {
        let input = json!([{ "n": 1, "s": "open" }, { "n": 2, "s": "merged" }]);
        assert_eq!(filtered(".[].n", input.clone()), vec!["1", "2"]);
        assert_eq!(
            filtered("map(select(.s == \"open\")) | length", input),
            vec!["1"]
        );
    }

    #[test]
    fn a_filter_that_does_not_parse_is_a_usage_error() {
        assert!(matches!(validate_filter(".["), Err(CliError::Usage(_))));
        assert!(matches!(
            validate_filter("no_such_function(1)"),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn a_filter_that_only_fails_on_null_still_validates() {
        assert!(validate_filter(".[0].name | ascii_downcase").is_ok());
    }

    #[test]
    fn what_others_wrote_can_not_drive_the_terminal() {
        assert_eq!(clean("a\x1b[2Jb\nc"), "a [2Jb c");
        assert_eq!(clean_keeping_lines("a\x1b[2Jb\n\tc\r"), "a[2Jb\n\tc");
    }
}
