mod cli;
mod client;
mod commands;
mod config;
mod context;
mod error;
mod git;
mod model;
mod output;
mod repo_ref;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;
use crate::context::Context;
use crate::error::Result;
use crate::output::Output;

async fn run(cli: Cli) -> Result<()> {
    let out = Output::new(cli.global.json, cli.global.jq.clone())?;
    let store = config::Store::load()?;
    let mut context = Context {
        global: cli.global,
        out,
        store,
    };
    commands::run(&mut context, cli.command).await
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}
