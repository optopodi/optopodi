use clap::{AppSettings, Clap};
use fehler::throws;
use stable_eyre::eyre::{Error, WrapErr};
use std::path::PathBuf;

mod metrics;
mod report;
mod token;
mod util;

use crate::report::Report;

#[derive(Clap, Debug, PartialEq)]
#[clap(setting = AppSettings::ColoredHelp)]
#[clap(name = "optopodi")]
struct OctoCli {
    /// Load the saved results of graphql queries from disk (if they are present).
    #[clap(long)]
    replay_graphql: bool,

    /// the sub-command to run
    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(Clap, Debug, PartialEq)]
enum Cmd {
    Report { directory: String },
}

#[throws]
#[tokio::main]
async fn main() {
    stable_eyre::install().wrap_err("Failed to install `stable_eyre`")?;
    env_logger::init();

    let token = token::github_token().wrap_err("Failed to initialize GitHub Token")?;

    // initialize static octocrab API -- call `octocrab::instance()` anywhere to retrieve instance
    octocrab::initialise(octocrab::Octocrab::builder().personal_token(token))
        .wrap_err("Failed to initialize static instance of Octocrab")?;

    let cli = OctoCli::parse();

    match cli.cmd {
        Cmd::Report { directory } => {
            Report::new(PathBuf::from(&directory), cli.replay_graphql)
                .run()
                .await
                .wrap_err_with(|| {
                    format!(
                        "Failed to generate new report from directory {}",
                        &directory
                    )
                })?;
        }
    }
}
