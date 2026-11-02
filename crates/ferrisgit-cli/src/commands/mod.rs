mod api;
mod auth;
mod issue;
mod mr;
mod pipeline;
mod release;
mod repo;
mod variable;

use crate::cli::Command;
use crate::context::Context;
use crate::error::Result;

pub async fn run(context: &mut Context, command: Command) -> Result<()> {
    match command {
        Command::Login(args) => auth::login(context, args).await,
        Command::Logout(args) => auth::logout(context, args).await,
        Command::Auth(command) => auth::run(context, command).await,
        Command::Repo(command) => repo::run(context, command).await,
        Command::Issue(command) => issue::run(context, command).await,
        Command::Mr(command) => mr::run(context, command).await,
        Command::Pipeline(command) => pipeline::run(context, command).await,
        Command::Release(command) => release::run(context, command).await,
        Command::Variable(command) => variable::run(context, command, false).await,
        Command::Secret(command) => variable::run(context, command, true).await,
        Command::Api(args) => api::run(context, args).await,
    }
}
