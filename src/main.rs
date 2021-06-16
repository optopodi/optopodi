use anyhow::Error;
use clap::{AppSettings, Clap};
use fehler::throws;
use serde_derive::Deserialize;
use std::path::Path;
use tokio::sync::mpsc;

mod google_sheets;
mod metrics;
mod token;
mod util;

use metrics::{Consumer, ExportToSheets, ListReposForOrg, Print, Producer};

#[derive(Debug, Deserialize, Default)]
struct Config {
    github: GithubConfig,
}

#[derive(Debug, Deserialize, Default)]
struct GithubConfig {
    org: Option<String>,
}

impl Config {
    #[throws(anyhow::Error)]
    pub fn load(path: &Path) -> Config {
        let config_text = std::fs::read_to_string(path)?;
        Self::parse(&config_text)?
    }

    #[throws(anyhow::Error)]
    pub fn parse(text: &str) -> Config {
        toml::from_str(text)?
    }
}

#[derive(Clap, Debug)]
#[clap(setting = AppSettings::ColoredHelp)]
#[clap(name = "gh-metrics")]
enum Opt {
    /// list all repositories in the given organization and the number of
    /// Pull Requests created in the last 30 days
    List {
        /// name of GitHub organization to analyze
        #[clap(short, long)]
        org: String,

        /// Verbose mode (-v, -vv, -vvv, etc.)
        #[clap(short, long, parse(from_occurrences))]
        verbose: u8,

        #[clap(short, long)]
        google_sheet: Option<String>,
    },
}

#[throws]
#[tokio::main]
async fn main() {
    let config_path = Path::new("metrics.toml");
    let _config = if config_path.exists() {
        Config::load(config_path)?
    } else {
        Config::default()
    };
    let token = token::github_token()?;
    // initialize static octocrab API -- call `octocrab::instance()` anywhere to retrieve instance
    octocrab::initialise(octocrab::Octocrab::builder().personal_token(token))?;

    match Opt::parse() {
        Opt::List {
            org,
            google_sheet: gs,
            verbose: _,
        } => {
            // utilize `tokio::sync::mpsc` to decouple producing and consuming data
            // `tx` can be used for sending entries (producing data)
            // `rx` can be used to DO something with gathered data (consuming data)
            let (tx, mut rx) = mpsc::channel::<Vec<String>>(400);
            let list_repos = ListReposForOrg::new(&org);
            let column_names = list_repos.column_names();

            tokio::spawn(async move {
                list_repos.producer_task(tx).await;
            });

            match gs {
                // if user specified a google sheet ID, they must want to export data to that sheet
                Some(sheet_id) => {
                    ExportToSheets::new(&sheet_id)
                        .consume(&mut rx, column_names)
                        .await;
                }
                // User wants to print to terminal, they did not specify a google sheet ID
                None => {
                    Print::consume(&Print, &mut rx, column_names).await;
                }
            }
        }
    }
}
